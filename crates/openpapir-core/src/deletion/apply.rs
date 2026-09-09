//! The apply pass: unlink what the plan named, records first, objects second.
//!
//! Every removal is the unlink of one file openPapir created, at a path it
//! derived from the archive root, its own fixed directory names, and an
//! identifier or digest it minted and validated. No directory is ever removed,
//! nothing is removed recursively, and nothing outside `records/` and
//! `objects/sha256/` is touched (`docs/archive-layout.md`).
//!
//! Records go first, so that an object is only ever unlinked once nothing in
//! the archive names it. The record pass stops at the first unlink the
//! filesystem refuses and takes the object pass with it, because a record
//! that is still there still names its artefacts. An object that cannot be
//! unlinked leaves the store exactly as it was and is counted; the deletion
//! completes what it safely can and reports what it did not remove.

use std::fs;
use std::io;
use std::path::Path;

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
/// The record pass stops at the first refusal and takes the object pass with
/// it. The order it runs in is the order of the references between the kinds:
/// an association names a receipt and a submission, a submission names the
/// case, so each kind goes only once everything that could name it has gone.
/// Carrying on past a refusal would remove a record something still there
/// names, and purging afterwards would remove bytes a surviving record still
/// names. Both are the dangling references the integrity check reports, so
/// neither is attempted: the deletion keeps what it had already removed,
/// touches no object at all, and reports how much it did not remove.
pub fn run(root: &Path, plan: &Plan, warnings: &mut Vec<Warning>) -> Removed {
    let mut removed = Removed::default();
    let planned =
        (plan.associations.len() + plan.receipts.len() + plan.submissions.len()) as u64 + 1;

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
        match fs::remove_file(directory.join(format!("{id}.json"))) {
            Ok(()) => unlinked.removed += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => unlinked.retained += 1,
        }
    }
    unlinked
}

/// Report a degradation once, however many paths saw it.
///
/// A warning here names a stage rather than a path, so a second copy of the
/// same code would tell the caller nothing and would grow the array with the
/// size of the archive.
fn note(warnings: &mut Vec<Warning>, warning: Warning) {
    if !warnings.iter().any(|seen| seen.code == warning.code) {
        warnings.push(warning);
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
/// reported as the named degradation rather than counted as a removal; the
/// warning says whether the attribute was restored, because a failure to
/// restore it is a weakening the caller must be told about.
#[cfg(not(unix))]
fn retry_unlink(path: &Path, warnings: &mut Vec<Warning>) -> bool {
    let Some(original) = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions())
    else {
        note(warnings, replace_while_open(true));
        return false;
    };
    let mut writable = original.clone();
    writable.set_readonly(false);
    if fs::set_permissions(path, writable).is_ok() {
        if fs::remove_file(path).is_ok() {
            return true;
        }
        let restored = fs::set_permissions(path, original).is_ok();
        note(warnings, replace_while_open(restored));
        return false;
    }
    note(warnings, replace_while_open(true));
    false
}

/// The degradation reported where an unlink is deferred by the platform.
#[cfg(not(unix))]
fn replace_while_open(restored: bool) -> Warning {
    Diagnostic::new(
        codes::PLATFORM_REPLACE_WHILE_OPEN,
        "A file could not be removed now because another process holds it open.",
        Details::new()
            .text("stage", "purge")
            .flag("read_only_restored", restored),
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
        fs::write(directory.join(format!("{present}.json")), "{}\n").unwrap();
        fs::write(directory.join("keep.json"), "{}\n").unwrap();
        assert_eq!(
            unlink_records::<Case>(root.path(), &[present.clone(), absent]),
            Unlinked {
                removed: 1,
                retained: 0
            },
            "a document that was not there is neither removed nor retained"
        );
        assert!(!directory.join(format!("{present}.json")).exists());
        assert!(directory.join("keep.json").exists(), "nothing else goes");
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
