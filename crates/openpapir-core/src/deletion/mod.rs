//! Deleting a case: its records, and its objects only on an explicit purge.
//!
//! Deleting a case deletes the case record and every submission recorded
//! against it. A receipt and an association are records of their own, so each
//! goes only when it references no submission and no case this deletion
//! leaves behind. Objects are not touched at all without `--purge`; with it,
//! every object no remaining import event, receipt, or submission references
//! is unlinked, and the import-event records naming a purged object go with
//! it, because an event describing content that is gone is history of nothing
//! (`docs/archive-layout.md`).
//!
//! # Plan, then apply
//!
//! The whole archive is read first, under the writer lock, and the removal
//! set is decided before a single file is unlinked. A record document that
//! cannot be read as a record of its kind therefore aborts the deletion with
//! `record.malformed` while the archive is still exactly as it was.
//!
//! # What is removed, and what is never removed
//!
//! Every removal is the unlink of one file openPapir created: a record
//! document under `records/`, or an object at its own fan-out path under
//! `objects/sha256/`. No directory is removed, nothing is removed
//! recursively, and nothing outside those two trees is touched.
//!
//! # What is reported
//!
//! Counts, record kinds, and the reason an object stayed. Never a digest,
//! never a path, never a filename, and nothing is persisted: a deletion
//! summary that recorded the fingerprint of purged content would defeat the
//! purge, so there is no deletion record and no audit log
//! (`docs/error-contract.md`). Deletion does not erase data from the storage
//! medium, and no output here says that it does.

pub mod apply;
pub mod plan;

use std::path::Path;

use serde::Serialize;

use crate::archive::Archive;
use crate::archive::lock::WriterLock;
use crate::error::{Diagnostic, Failure, Outcome, Result, Warning};

/// How many records of one kind a deletion removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RemovedRecords {
    /// How many documents of that kind were unlinked.
    pub count: u64,
    /// The record kind, as `docs/architecture.md` names it.
    pub kind: &'static str,
}

/// How many objects stayed in the store, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RetainedObjects {
    /// How many objects the reason applies to.
    pub count: u64,
    /// The reason, one of the fixed set in [`plan::REASONS`].
    pub reason: &'static str,
}

/// What one deletion did: counts, kinds, and reasons, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Deleted {
    /// How many objects were unlinked, which is none without a purge.
    pub objects_removed: u64,
    /// One entry per reason an object stayed, ordered by reason.
    pub objects_retained: Vec<RetainedObjects>,
    /// How many objects stayed in the store for any reason.
    pub objects_retained_total: u64,
    /// Whether the caller asked for a purge.
    pub purge: bool,
    /// One entry per record kind, ordered by kind, including the kinds that
    /// lost nothing, so a caller reads a count rather than testing for a key.
    pub records_removed: Vec<RemovedRecords>,
    /// How many record documents were unlinked in total.
    pub records_removed_total: u64,
    /// How many record documents the filesystem refused to unlink. Any at all
    /// means no object was touched, whatever `purge` says.
    pub records_retained: u64,
}

impl Deleted {
    /// The refusal a deletion that could not remove everything reports.
    ///
    /// The deletion did the rest of its stated work, so its counts stay in
    /// `data` and the error names, by count alone, what is still there. A
    /// record that would not go comes first: it is the condition that stopped
    /// the purge from running at all, and it is the reason any object is
    /// still in the store, so reporting the objects instead would name the
    /// symptom rather than the cause.
    #[must_use]
    pub fn problem(&self) -> Option<Diagnostic> {
        if self.records_retained > 0 {
            return Some(apply::records_retained(self.records_retained));
        }
        let unremovable = self
            .objects_retained
            .iter()
            .find(|retained| retained.reason == plan::UNREMOVABLE)
            .map_or(0, |retained| retained.count);
        (unremovable > 0).then(|| apply::objects_retained(unremovable))
    }
}

/// Delete one case, and purge the objects nothing else references when asked.
///
/// The archive's permission checks run first and the writer lock is held for
/// the whole operation, the scan included, so no other writer can change what
/// the plan was computed from.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no case,
/// `record.malformed` when a stored document cannot be read as a record of
/// its kind, `lock.held` when another writer holds the lock, and any archive
/// or path refusal of `docs/error-contract.md`. Each is returned before
/// anything has been removed.
pub fn delete(root: &Path, case_id: &str, purge: bool) -> Result<Deleted> {
    let mut warnings = Vec::new();
    match run(root, case_id, purge, &mut warnings) {
        Ok(data) => Ok(Outcome { data, warnings }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn run(
    root: &Path,
    case_id: &str,
    purge: bool,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Deleted, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let plan = plan::build(archive.root(), case_id, purge)?;
    let removed = apply::run(archive.root(), &plan, warnings);
    Ok(report(&plan, &removed, purge))
}

/// Turn a plan and what it removed into the report the caller receives.
fn report(plan: &plan::Plan, removed: &apply::Removed, purge: bool) -> Deleted {
    // A record that would not go stops the object pass before it begins, so
    // every object the purge had planned is still in the store for that one
    // reason, and none of them was even attempted.
    let stopped = removed.records_retained > 0;
    let retained = [
        plan.purge_not_requested,
        if stopped {
            plan.objects.len() as u64
        } else {
            0
        },
        plan.referenced_elsewhere,
        removed.unremovable,
    ];
    let counts = [
        removed.associations,
        removed.cases,
        removed.import_events,
        removed.receipts,
        removed.submissions,
    ];
    Deleted {
        objects_removed: removed.objects,
        objects_retained: plan::REASONS
            .into_iter()
            .zip(retained)
            .map(|(reason, count)| RetainedObjects { count, reason })
            .collect(),
        objects_retained_total: retained.iter().sum(),
        purge,
        records_removed: plan::KINDS
            .into_iter()
            .zip(counts)
            .map(|(kind, count)| RemovedRecords { count, kind })
            .collect(),
        records_removed_total: removed.records(),
        records_retained: removed.records_retained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::codes;
    use crate::records::case;

    /// An archive with one case, ready to be deleted.
    fn archive() -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        crate::init(root.path()).unwrap();
        let created = case::create(root.path(), "A local matter", None).unwrap();
        (root, created.data.case.id)
    }

    #[test]
    fn a_deletion_reports_every_kind_and_every_reason_with_a_count() {
        let (root, case_id) = archive();
        let outcome = delete(root.path(), &case_id, false).unwrap();
        let data = outcome.data;
        assert_eq!(data.records_removed_total, 1);
        assert_eq!(data.records_removed.len(), plan::KINDS.len());
        assert_eq!(data.objects_retained.len(), plan::REASONS.len());
        assert_eq!(data.objects_removed, 0);
        assert_eq!(data.objects_retained_total, 0);
        assert!(!data.purge);
        assert_eq!(data.problem(), None);
        assert_eq!(data.records_retained, 0);
        let kinds: Vec<&str> = data.records_removed.iter().map(|kind| kind.kind).collect();
        assert_eq!(kinds, plan::KINDS, "every kind is listed, in order");
        let removed: Vec<u64> = data.records_removed.iter().map(|kind| kind.count).collect();
        assert_eq!(removed, vec![0, 1, 0, 0, 0]);
        assert!(
            case::show(root.path(), &case_id).is_err(),
            "the case is gone"
        );
    }

    #[test]
    fn an_unknown_case_is_refused_and_nothing_is_removed() {
        let (root, case_id) = archive();
        let failure = delete(root.path(), "00000000000000000000000000000000", true).unwrap_err();
        assert_eq!(failure.error.code, codes::RECORD_NOT_FOUND);
        assert_eq!(failure.error.exit_code(), 4);
        assert!(case::show(root.path(), &case_id).is_ok(), "the case stays");
        let refusal = delete(root.path(), "../../etc/passwd", false).unwrap_err();
        assert_eq!(refusal.error.code, codes::RECORD_NOT_FOUND);
        assert!(
            !serde_json::to_string(&refusal.error)
                .unwrap()
                .contains("passwd"),
            "no reference the user supplied is echoed"
        );
    }

    #[test]
    fn the_writer_lock_is_held_for_the_whole_operation() {
        let (root, case_id) = archive();
        let held = WriterLock::acquire(root.path()).unwrap();
        let failure = delete(root.path(), &case_id, true).unwrap_err();
        assert_eq!(failure.error.code, codes::LOCK_HELD);
        assert!(failure.error.is_retryable());
        assert!(case::show(root.path(), &case_id).is_ok(), "nothing went");
        drop(held);
        assert!(delete(root.path(), &case_id, true).is_ok());
    }

    #[test]
    fn a_report_never_carries_a_digest_a_path_or_a_filename() {
        let (root, case_id) = archive();
        let data = delete(root.path(), &case_id, true).unwrap().data;
        let json = serde_json::to_string(&data).unwrap();
        assert!(!json.contains("sha256"), "no digest reaches the report");
        assert!(!json.contains('/'), "no path reaches the report");
        assert!(json.contains("\"purge\":true"));
        assert!(
            !json.contains("unremovable\",\"count\":1"),
            "an ordinary purge retains nothing"
        );
    }
}
