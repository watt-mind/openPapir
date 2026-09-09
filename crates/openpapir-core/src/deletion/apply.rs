//! The apply pass: unlink what the plan named, records first, objects second.
//!
//! Every removal is the unlink of one file openPapir created, at a path it
//! derived from the archive root, its own fixed directory names, and an
//! identifier or digest it minted and validated. No directory is ever removed,
//! nothing is removed recursively, and nothing outside `records/` and
//! `objects/sha256/` is touched (`docs/archive-layout.md`).
//!
//! Records go first, so that an object is only ever unlinked once nothing in
//! the archive names it. The record pass is all or nothing: every directory
//! it would remove an entry from, and every document it would unlink, is
//! probed before the first unlink, and a probe that says one of them will not
//! go refuses the whole deletion while the archive is still exactly as it
//! was. An object that cannot be unlinked leaves the store exactly as it was
//! and is counted; the deletion completes what it safely can and reports what
//! it did not remove.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::archive::import::ImportEvent;
use crate::archive::{objects, paths};
use crate::deletion::plan::Plan;
use crate::error::{Details, Diagnostic, Warning, codes};
use crate::records::association::Association;
use crate::records::case::Case;
use crate::records::document::Record;
use crate::records::receipt::Receipt;
use crate::records::submission::Submission;

/// What the apply pass removed, and what it could not.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Removed {
    /// How many association records were unlinked.
    pub associations: u64,
    /// How many case records were unlinked, which is one or none.
    pub cases: u64,
    /// How many import-event records were unlinked.
    pub import_events: u64,
    /// How many receipt records were unlinked.
    pub receipts: u64,
    /// How many submission records were unlinked.
    pub submissions: u64,
    /// How many objects were unlinked.
    pub objects: u64,
    /// How many record documents the filesystem refused to unlink. Any at all
    /// stops the object pass before it begins.
    pub records_retained: u64,
    /// How many objects the purge planned to unlink and could not.
    pub unremovable: u64,
}

impl Removed {
    /// How many record documents were unlinked in total.
    #[must_use]
    pub const fn records(&self) -> u64 {
        self.associations + self.cases + self.import_events + self.receipts + self.submissions
    }
}

/// Carry out one plan under the writer lock the caller already holds.
///
/// The order is fixed: the case's own records first, associations, receipts,
/// submissions, and the case itself, and only then the objects nothing names
/// any more. An import event is not one of the case's records: it is the
/// history of an object, so it goes with the object it names, once that
/// object has actually gone. An object that could not be unlinked therefore
/// keeps its import event and stays a referenced object rather than becoming
/// an orphan.
///
/// The record pass runs only once the probe below has said the whole of it
/// can run, and it still stops at the first refusal and takes the object pass
/// with it, because the probe cannot promise what the filesystem will do a
/// moment later. The order it runs in is the order of the references between
/// the kinds: an association names a receipt and a submission, a submission
/// names the case, so each kind goes only once everything that could name it
/// has gone. Carrying on past a refusal would remove a record something still
/// there names, and purging afterwards would remove bytes a surviving record
/// still names. Both are the dangling references the integrity check reports,
/// so neither is attempted: the deletion keeps what it had already removed,
/// touches no object at all, and reports how much it did not remove.
pub fn run(root: &Path, plan: &Plan, warnings: &mut Vec<Warning>) -> Removed {
    let mut removed = Removed::default();
    let planned = planned_documents(plan);

    if !removable(root, plan) {
        // Nothing has been unlinked and nothing will be, so every planned
        // document is retained and no directory needs flushing.
        removed.records_retained = planned;
        return removed;
    }

    let associations = unlink_records::<Association>(root, &plan.associations);
    removed.associations = associations.removed;
    let mut refused = associations.retained > 0;
    if !refused {
        let receipts = unlink_records::<Receipt>(root, &plan.receipts);
        removed.receipts = receipts.removed;
        refused = receipts.retained > 0;
    }
    if !refused {
        let submissions = unlink_records::<Submission>(root, &plan.submissions);
        removed.submissions = submissions.removed;
        refused = submissions.retained > 0;
    }
    if !refused {
        let cases = unlink_records::<Case>(root, std::slice::from_ref(&plan.case));
        removed.cases = cases.removed;
        refused = cases.retained > 0;
    }

    if refused {
        removed.records_retained = planned - removed.records();
    } else {
        for digest in &plan.objects {
            if unlink_object(root, digest, warnings) {
                removed.objects += 1;
                if let Some(events) = plan.import_events.get(digest) {
                    removed.import_events += unlink_records::<ImportEvent>(root, events).removed;
                }
            } else {
                removed.unremovable += 1;
            }
        }
    }
    flush_record_directories(root, plan, removed.import_events > 0, warnings);
    removed
}

/// How many record documents this plan would remove in all.
///
/// The case, the associations, the receipts, and the submissions, plus every
/// import event that would go with a purged object: an event is planned for
/// removal even though it goes only in the object pass, so a deletion that
/// never reaches that pass did not remove it and has to say so. Leaving them
/// out under-reported `retained_count` by exactly the events in
/// `records/imports` whenever that was the directory the probe refused.
fn planned_documents(plan: &Plan) -> u64 {
    let events: usize = plan.import_events.values().map(Vec::len).sum();
    (plan.associations.len() + plan.receipts.len() + plan.submissions.len() + events) as u64 + 1
}

/// Whether every record document this plan names can be unlinked.
///
/// A deletion that unlinks part of its record pass and then stops leaves the
/// archive in a state the user cannot get out of by retrying: the records it
/// did remove are gone, so the next run plans a smaller deletion, and an
/// object whose last referencing record went in the interrupted pass is no
/// longer a purge candidate at all. It stays in the store with the import
/// event that describes it, which is a state `archive check` calls clean,
/// and nothing tells the user. The record pass is therefore all or nothing
/// per case: this probe runs first, and a `false` here refuses the deletion
/// with `delete.records_retained` before a single document is unlinked.
///
/// The probe attempts nothing destructive. It opens each record directory
/// the plan would remove an entry from, without following a link, and reads
/// the mode of the opened handle; then it looks at each planned document,
/// which must be either absent or a regular file. Nothing is created, moved,
/// or removed.
///
/// # What it cannot promise
///
/// The probe is a check that precedes the use, so the window between them is
/// a time-of-check-to-time-of-use gap: a mode changed, a filesystem remounted
/// read only, or a quota reached after the probe and before the unlink still
/// refuses, and the record pass still stops at that first refusal. The
/// writer lock is held across both, so no other openPapir writer can move in
/// the meantime, and nothing outside openPapir is under its control. The
/// probe reads permission bits rather than asking the kernel whether this
/// process may write, so a refusal expressed some other way — an
/// access-control list, an immutable flag, a mandatory lock — is not seen
/// here either. It narrows the window that stranded a purge candidate; it
/// does not close it, and the stop-at-first-refusal rule is what still holds
/// when it is wrong.
fn removable(root: &Path, plan: &Plan) -> bool {
    let events: Vec<String> = plan.import_events.values().flatten().cloned().collect();
    kind_removable::<Association>(root, &plan.associations)
        && kind_removable::<Receipt>(root, &plan.receipts)
        && kind_removable::<Submission>(root, &plan.submissions)
        && kind_removable::<Case>(root, std::slice::from_ref(&plan.case))
        && kind_removable::<ImportEvent>(root, &events)
}

/// Whether one kind's directory and every document the plan names in it can
/// be unlinked. A kind the plan removes nothing from is not probed, because
/// its directory is not touched.
fn kind_removable<R: Record>(root: &Path, ids: &[String]) -> bool {
    if ids.is_empty() {
        return true;
    }
    let directory = root.join(R::DIRECTORY);
    directory_writable(&directory)
        && ids
            .iter()
            .all(|id| unlinkable(&document_path(&directory, id)))
}

/// The path of one record document inside its own kind's directory.
///
/// The probe and the unlink pass must look at exactly the same file, or the
/// all-or-nothing guarantee is a promise about one path and a removal of
/// another. They therefore build it here and nowhere else. The identifier is
/// one openPapir minted and the reader validated, so nothing user-supplied is
/// joined into the path.
fn document_path(directory: &Path, id: &str) -> PathBuf {
    directory.join(format!("{id}.json"))
}

/// Whether a record directory can have an entry removed from it.
///
/// The handle is opened without following a link, so the directory probed is
/// the one the unlink will reach rather than whatever a link points at, and
/// the mode is read from that handle rather than from the path a second time.
/// Owner write and search are the whole test: the archive is owner-only by
/// design, and a process that could not read the directory would have failed
/// in the scan pass long before this.
#[cfg(unix)]
fn directory_writable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    paths::open_no_follow(path).is_ok_and(|handle| {
        handle.metadata().is_ok_and(|metadata| {
            metadata.is_dir() && metadata.permissions().mode() & 0o300 == 0o300
        })
    })
}

/// Whether a record directory can have an entry removed from it.
///
/// Here the permission is an access-control list rather than a mode, and
/// openPapir does not read one, so the probe confirms only that the path is
/// a directory openPapir created rather than a reparse point. A refusal the
/// list expresses is still caught by the stop-at-first-refusal rule.
#[cfg(not(unix))]
fn directory_writable(path: &Path) -> bool {
    !paths::is_symlink(path) && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// Whether one planned document is in a state the unlink can remove.
///
/// A document that is already absent answers `true`: there is nothing left to
/// remove, which the unlink pass does not count as a refusal either. Anything
/// at the path that is not a regular file is not a record openPapir wrote and
/// is never unlinked, so it refuses the deletion here rather than part way
/// through it.
fn unlinkable(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Err(error) => error.kind() == io::ErrorKind::NotFound,
        Ok(metadata) => metadata.is_file() && !read_only(&metadata),
    }
}

/// Whether a file's own attribute stops it from being unlinked.
///
/// On Unix it never does: the unlink needs the permission of the directory
/// holding the entry, and the file's own mode has no say in it.
#[cfg(unix)]
const fn read_only(_metadata: &fs::Metadata) -> bool {
    false
}

/// Whether a file's own attribute stops it from being unlinked.
///
/// Where a read-only file cannot be unlinked at all, the attribute is part of
/// the answer. openPapir never marks a record document read-only, so one that
/// is marked was marked from outside.
#[cfg(not(unix))]
fn read_only(metadata: &fs::Metadata) -> bool {
    metadata.permissions().readonly()
}

/// How one kind's unlink pass went: what went, and what would not go.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Unlinked {
    /// How many documents this pass unlinked.
    removed: u64,
    /// How many the filesystem refused to unlink.
    retained: u64,
}

/// Unlink one kind's documents, counting what went and what would not.
///
/// The file name is an identifier openPapir minted and the reader validated,
/// so nothing user-supplied is joined into the path. A document that is
/// already absent is neither removed nor retained: there is nothing left to
/// remove, so it is not a refusal. Every other failure is a refusal and is
/// counted, because a record that is still there still names its artefacts.
fn unlink_records<R: Record>(root: &Path, ids: &[String]) -> Unlinked {
    let directory = root.join(R::DIRECTORY);
    let mut unlinked = Unlinked::default();
    for id in ids {
        match fs::remove_file(document_path(&directory, id)) {
            Ok(()) => unlinked.removed += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => unlinked.retained += 1,
        }
    }
    unlinked
}

/// The flag saying whether a cleared read-only attribute was put back.
///
/// It answers a question that exists only once the attribute has actually
/// been cleared, so it is emitted on that path alone. A warning without it
/// says the attribute was never cleared and nothing was widened, which is a
/// different statement from `true`, and reporting `true` there would claim a
/// restore that never happened.
pub const READ_ONLY_RESTORED: &str = "read_only_restored";

/// Report a degradation once, keeping the worst outcome any path saw.
///
/// A warning here names a stage rather than a path, so a second copy of the
/// same code would tell the caller nothing and would grow the array with the
/// size of the archive. Two warnings of one code are not always equal,
/// though: one may say a guarantee was restored and the next that it was not.
/// Collapsing them by code alone would let the order of the objects decide
/// what the caller is told, and would hide the object whose read-only
/// attribute is still cleared. The kept warning is therefore replaced by an
/// incoming one that reports the weaker outcome, so the array carries the
/// worst thing that happened rather than the first.
fn note(warnings: &mut Vec<Warning>, warning: Warning) {
    let Some(seen) = warnings.iter_mut().find(|seen| seen.code == warning.code) else {
        warnings.push(warning);
        return;
    };
    if fidelity(&warning) > fidelity(seen) {
        *seen = warning;
    }
}

/// How much one copy of a warning is worth keeping, highest wins.
///
/// An object left writable is the worst thing that can have happened and
/// outranks everything, whichever object saw it first. A warning carrying no
/// flag at all never cleared the attribute, so it never displaces one that
/// did; but it also answers less, so a later copy that did clear the
/// attribute and put it back replaces it rather than being dropped for
/// arriving second.
fn fidelity(warning: &Warning) -> u8 {
    match warning.details.flag_value(READ_ONLY_RESTORED) {
        Some(false) => 2,
        Some(true) => 1,
        None => 0,
    }
}

/// Flush every record directory this deletion removed an entry from.
fn flush_record_directories(root: &Path, plan: &Plan, events: bool, warnings: &mut Vec<Warning>) {
    let touched = [
        (!plan.associations.is_empty(), Association::DIRECTORY),
        (!plan.receipts.is_empty(), Receipt::DIRECTORY),
        (!plan.submissions.is_empty(), Submission::DIRECTORY),
        (true, Case::DIRECTORY),
        (events, ImportEvent::DIRECTORY),
    ];
    for directory in touched
        .into_iter()
        .filter_map(|(touched, directory)| touched.then_some(directory))
    {
        if let Some(warning) = paths::sync_directory(&root.join(directory), "delete") {
            note(warnings, warning);
        }
    }
}

/// Unlink one object at its own fan-out path, reporting whether it went.
///
/// The digest was validated before it reached the plan, so the path is the
/// store's own. A path that holds anything other than a regular file
/// openPapir created is left alone and counted as retained, because unlinking
/// it would be a removal this design never promises.
///
/// The object's own mode is not changed on Unix: unlinking needs the
/// permission of the directory holding it, which is owner-only and enough.
/// Where the platform makes a read-only file undeletable instead, the
/// attribute is cleared for the unlink and put back if the unlink still
/// fails, so an object that survives survives read-only as it was.
fn unlink_object(root: &Path, digest: &str, warnings: &mut Vec<Warning>) -> bool {
    let path = objects::absolute_path(root, digest);
    if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
        return false;
    }
    match fs::remove_file(&path) {
        Ok(()) => {
            if let Some(parent) = path.parent()
                && let Some(warning) = paths::sync_directory(parent, "purge")
            {
                note(warnings, warning);
            }
            true
        }
        Err(_) => retry_unlink(&path, warnings),
    }
}

/// The one retry the platform can need, and the degradation it reports.
#[cfg(unix)]
fn retry_unlink(_path: &Path, _warnings: &mut Vec<Warning>) -> bool {
    false
}

/// Clear the read-only attribute and unlink once more.
///
/// On a platform where a read-only file cannot be unlinked at all, clearing
/// the attribute is a precondition of the removal rather than a widening of
/// access: the file is gone a moment later. When the retried unlink still
/// fails, the attribute is put back, so an object that survives is left
/// exactly as read-only as it was and nothing is widened. A file another
/// process still holds open cannot go now, and the deferred removal is
/// reported as the named degradation rather than counted as a removal.
///
/// The warning says whether the attribute was restored only where it was
/// cleared in the first place. The helper reports the permissions it found,
/// so the restore uses the ones already in hand rather than reading the
/// metadata a second time; where it failed, the file is exactly as it was,
/// there is nothing to put back, and the flag is left out rather than
/// reported as `true`: a restore that never had to happen is not a restore
/// that succeeded.
#[cfg(not(unix))]
fn retry_unlink(path: &Path, warnings: &mut Vec<Warning>) -> bool {
    let Ok(original) = paths::clear_read_only(path) else {
        note(warnings, replace_while_open(None));
        return false;
    };
    if fs::remove_file(path).is_ok() {
        return true;
    }
    let restored = fs::set_permissions(path, original).is_ok();
    note(warnings, replace_while_open(Some(restored)));
    false
}

/// The degradation reported where an unlink is deferred by the platform.
///
/// `restored` is `None` on every path that never cleared the read-only
/// attribute, and the flag is then absent from the details rather than
/// asserting a restore that was never needed. The rule is
/// platform-independent, so the builder is compiled for the tests everywhere
/// as well as for the platforms whose unlink can reach it.
#[cfg(any(not(unix), test))]
fn replace_while_open(restored: Option<bool>) -> Warning {
    let details = Details::new().text("stage", "purge");
    let details = match restored {
        Some(restored) => details.flag(READ_ONLY_RESTORED, restored),
        None => details,
    };
    Diagnostic::new(
        codes::PLATFORM_REPLACE_WHILE_OPEN,
        "A file could not be removed now because another process holds it open.",
        details,
    )
    .retryable()
}

/// The refusal reported when a record document could not be unlinked.
///
/// It is a refusal rather than a degradation because it changes what the
/// deletion did: a record that is still there still names its artefacts, so
/// no object was touched at all and the case is only partly gone. The count
/// is every document the deletion planned to remove and did not, including
/// the ones it never reached after it stopped; the record's kind, identifier,
/// and path are omitted.
#[must_use]
pub fn records_retained(retained_count: u64) -> Diagnostic {
    Diagnostic::new(
        codes::DELETE_RECORDS_RETAINED,
        "A record document could not be removed, so no object was purged.",
        Details::new().int("retained_count", retained_count),
    )
}

/// The refusal reported when a purge left objects in the store.
///
/// The count is the whole answer. The digest, the path, and the name of a
/// retained object are all omitted: reporting the fingerprint of content the
/// user asked to purge would defeat the purge
/// (`docs/error-contract.md`).
#[must_use]
pub fn objects_retained(retained_count: u64) -> Diagnostic {
    Diagnostic::new(
        codes::DELETE_OBJECTS_RETAINED,
        "A purge could not remove every object it planned to remove.",
        Details::new()
            .text("reason", "unremovable")
            .int("retained_count", retained_count),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    #[test]
    fn a_retained_object_is_reported_as_a_count_and_nothing_else() {
        let refusal = objects_retained(2);
        assert_eq!(refusal.code, codes::DELETE_OBJECTS_RETAINED);
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["retained_count"], 2);
        assert_eq!(json["details"]["bucket"], "delete");
        assert_eq!(json["details"]["reason"], "unremovable");
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
    }

    #[test]
    fn an_object_that_is_not_a_regular_file_is_left_exactly_as_it_is() {
        let root = tempfile::tempdir().unwrap();
        let path = objects::absolute_path(root.path(), DIGEST);
        let mut warnings = Vec::new();
        assert!(
            !unlink_object(root.path(), DIGEST, &mut warnings),
            "an absent object is never counted as removed"
        );
        fs::create_dir_all(&path).unwrap();
        assert!(!unlink_object(root.path(), DIGEST, &mut warnings));
        assert!(path.is_dir(), "a directory is never removed");
    }

    #[test]
    fn only_the_documents_the_plan_named_are_unlinked() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Case::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        let present = "0123456789abcdef0123456789abcdef".to_owned();
        let absent = "fedcba9876543210fedcba9876543210".to_owned();
        let path = document_path(&directory, &present);
        fs::write(&path, "{}\n").unwrap();
        fs::write(directory.join("keep.json"), "{}\n").unwrap();
        assert!(
            unlinkable(&path),
            "the probe and the pass look at the one path this helper builds"
        );
        assert_eq!(
            unlink_records::<Case>(root.path(), &[present.clone(), absent]),
            Unlinked {
                removed: 1,
                retained: 0
            },
            "a document that was not there is neither removed nor retained"
        );
        assert!(!path.exists());
        assert!(directory.join("keep.json").exists(), "nothing else goes");
    }

    /// The merge rule is platform-independent, so the warnings are built
    /// here rather than produced by an unlink: only Windows defers one, and
    /// the rule that decides which of two survives has to hold everywhere.
    #[test]
    fn a_repeated_warning_keeps_the_worst_outcome_whatever_the_order() {
        let deferred = |restored: bool| replace_while_open(Some(restored));

        for order in [[true, false], [false, true]] {
            let mut warnings = Vec::new();
            for restored in order {
                note(&mut warnings, deferred(restored));
            }
            assert_eq!(warnings.len(), 1, "one code is reported once");
            assert_eq!(
                warnings[0].details.flag_value(READ_ONLY_RESTORED),
                Some(false),
                "an object left writable is reported whichever object saw it first"
            );
        }

        let mut warnings = Vec::new();
        note(&mut warnings, deferred(true));
        note(&mut warnings, deferred(true));
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].details.flag_value(READ_ONLY_RESTORED),
            Some(true),
            "nothing is worsened by repetition alone"
        );

        let mut warnings = Vec::new();
        note(&mut warnings, deferred(false));
        note(&mut warnings, paths::no_directory_fsync_warning("purge"));
        assert_eq!(warnings.len(), 2, "a different code is its own entry");
        assert_eq!(
            warnings[0].details.flag_value(READ_ONLY_RESTORED),
            Some(false),
            "a flagless warning of another code never displaces one"
        );
        assert_eq!(
            warnings[1].details.flag_value(READ_ONLY_RESTORED),
            None,
            "a warning with no such flag reads as none"
        );
    }

    /// The flag answers a question that only exists once the attribute has
    /// been cleared, so the path that never cleared it must not answer it.
    #[test]
    fn the_restored_flag_is_absent_where_the_attribute_was_never_cleared() {
        let never = replace_while_open(None);
        assert_eq!(never.code, codes::PLATFORM_REPLACE_WHILE_OPEN);
        assert!(never.is_retryable());
        let json = serde_json::to_value(&never).unwrap();
        assert_eq!(json["details"]["stage"], "purge");
        assert_eq!(
            json["details"].get(READ_ONLY_RESTORED),
            None,
            "no restore is claimed where nothing was cleared"
        );
        assert_eq!(json["details"].as_object().unwrap().len(), 2);
        assert_eq!(never.details.flag_value(READ_ONLY_RESTORED), None);

        let cleared = replace_while_open(Some(true));
        assert_eq!(
            serde_json::to_value(&cleared).unwrap()["details"][READ_ONLY_RESTORED],
            true,
            "the clearing path still answers it"
        );

        // A warning that never cleared the attribute widened nothing, so it
        // is not the weaker outcome and never displaces one that did.
        let mut warnings = Vec::new();
        note(&mut warnings, replace_while_open(Some(false)));
        note(&mut warnings, replace_while_open(None));
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].details.flag_value(READ_ONLY_RESTORED),
            Some(false),
            "the object left writable is still the one reported"
        );

        // A flagless copy answers less than one that cleared the attribute
        // and put it back, so a later copy that did replaces it. It is still
        // never allowed to displace the object that was left writable.
        for later in [Some(true), Some(false)] {
            let mut warnings = Vec::new();
            note(&mut warnings, replace_while_open(None));
            note(&mut warnings, replace_while_open(later));
            assert_eq!(warnings.len(), 1);
            assert_eq!(
                warnings[0].details.flag_value(READ_ONLY_RESTORED),
                later,
                "the copy that answers the question is the one kept"
            );
        }
        assert_eq!(fidelity(&paths::no_directory_fsync_warning("purge")), 0);
    }

    /// The probe is the whole of the all-or-nothing rule, so each of its
    /// answers is asserted directly: a directory that cannot have an entry
    /// removed from it, a path holding something openPapir did not write,
    /// and a document that is simply not there any more.
    #[test]
    fn a_record_that_cannot_be_unlinked_refuses_the_pass_before_it_starts() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Case::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        let case = "0123456789abcdef0123456789abcdef".to_owned();
        fs::write(directory.join(format!("{case}.json")), "{}\n").unwrap();
        let plan = Plan {
            case: case.clone(),
            ..Plan::default()
        };
        assert!(removable(root.path(), &plan), "an ordinary case may go");

        assert!(
            unlinkable(&directory.join("absent.json")),
            "a document that is already gone is not a refusal"
        );
        assert!(
            !unlinkable(&directory),
            "a directory at a record's path is never unlinked"
        );
        assert!(
            !directory_writable(&root.path().join("records/nowhere")),
            "a directory that is not there cannot have an entry removed"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            let mut warnings = Vec::new();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
            assert!(
                !directory_writable(&directory),
                "search alone is not enough"
            );
            assert!(!removable(root.path(), &plan));
            let removed = run(root.path(), &plan, &mut warnings);
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(removed.records_retained, 1);
            assert_eq!(removed.records(), 0);
            assert!(
                warnings.is_empty(),
                "nothing was touched, so nothing synced"
            );
            assert!(
                directory.join(format!("{case}.json")).is_file(),
                "the document the deletion could not finish is still there"
            );
        }
    }

    /// An import event is a document the plan would remove, so a deletion
    /// that never reached the object pass has to count it as retained.
    #[test]
    fn a_planned_import_event_counts_as_a_document_the_deletion_planned() {
        let mut plan = Plan {
            case: "0123456789abcdef0123456789abcdef".to_owned(),
            submissions: vec!["a".to_owned(), "b".to_owned()],
            ..Plan::default()
        };
        assert_eq!(planned_documents(&plan), 3, "the case and its submissions");
        plan.import_events
            .insert(DIGEST.to_owned(), vec!["c".to_owned(), "d".to_owned()]);
        assert_eq!(planned_documents(&plan), 5, "and the events going with it");
    }

    /// The count `delete.records_retained` carries is every document the
    /// deletion planned to remove and did not, so a refusal that names the
    /// import directory still reports the events it never reached.
    #[cfg(unix)]
    #[test]
    fn a_refused_import_directory_is_reported_in_the_retained_count() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().unwrap();
        let cases = root.path().join(Case::DIRECTORY);
        let imports = root.path().join(ImportEvent::DIRECTORY);
        fs::create_dir_all(&cases).unwrap();
        fs::create_dir_all(&imports).unwrap();
        let case = "0123456789abcdef0123456789abcdef".to_owned();
        let event = "fedcba9876543210fedcba9876543210".to_owned();
        fs::write(document_path(&cases, &case), "{}\n").unwrap();
        fs::write(document_path(&imports, &event), "{}\n").unwrap();
        let mut plan = Plan {
            case,
            objects: vec![DIGEST.to_owned()],
            ..Plan::default()
        };
        plan.import_events
            .insert(DIGEST.to_owned(), vec![event.clone()]);

        let mut warnings = Vec::new();
        fs::set_permissions(&imports, fs::Permissions::from_mode(0o500)).unwrap();
        let removed = run(root.path(), &plan, &mut warnings);
        fs::set_permissions(&imports, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(removed.records(), 0, "the probe refused before the pass");
        assert_eq!(
            removed.records_retained, 2,
            "the case and the import event it never reached"
        );
        assert_eq!(removed.objects, 0, "and no object was touched");
        assert!(document_path(&imports, &event).is_file());
    }

    #[test]
    fn a_removed_count_totals_every_record_kind() {
        let removed = Removed {
            associations: 1,
            cases: 1,
            import_events: 2,
            receipts: 1,
            submissions: 3,
            objects: 4,
            records_retained: 0,
            unremovable: 0,
        };
        assert_eq!(removed.records(), 8);
        assert_eq!(Removed::default().records(), 0);
    }
}
