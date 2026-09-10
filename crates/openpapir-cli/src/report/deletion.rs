//! The lines `case delete` prints.

use openpapir_core::Deleted;

/// The closing line `case delete` prints, whatever it removed.
///
/// Deletion unlinks files. It cannot reach a copy the user already took, and
/// it says nothing about what the storage medium still holds.
const DELETION_DISCLAIMER: &str = "Deletion unlinked files in this archive. It does not erase data from the storage medium, and any backup already taken is outside openPapir's reach.";

/// The lines `case delete` prints, in either outcome.
///
/// Counts, record kinds, and the reason an object stayed, in the same order
/// as the JSON. No digest, no path, and no filename reaches this output: one
/// would be a fingerprint of content the user asked to delete.
#[must_use]
pub fn case_deleted(deleted: &Deleted) -> Vec<String> {
    let kinds: Vec<String> = deleted
        .records_removed
        .iter()
        .map(|kind| format!("{} {}", kind.kind, kind.count))
        .collect();
    let reasons: Vec<String> = deleted
        .objects_retained
        .iter()
        .map(|retained| format!("{} {}", retained.reason, retained.count))
        .collect();
    vec![
        format!(
            "Removed {} record(s): {}.",
            deleted.records_removed_total,
            kinds.join(", ")
        ),
        format!(
            "Removed {} object(s); {} retained: {}.",
            deleted.objects_removed,
            deleted.objects_retained_total,
            reasons.join(", ")
        ),
        if deleted.records_retained > 0 {
            "A record could not be removed, so no object was touched: a record that is still here still references its artefacts.".to_owned()
        } else if deleted.purge {
            "A purge was requested: an object is unlinked only when no remaining import event, receipt, or submission references it.".to_owned()
        } else {
            "No purge was requested, so no object was removed.".to_owned()
        },
        DELETION_DISCLAIMER.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::assert_no_claim;
    use super::*;
    use openpapir_core::{RemovedRecords, RetainedObjects};

    #[test]
    fn human_deletion_output_is_counts_and_reasons_and_nothing_else() {
        let deleted = Deleted {
            objects_removed: 2,
            objects_retained: vec![
                RetainedObjects {
                    count: 0,
                    reason: "purge_not_requested",
                },
                RetainedObjects {
                    count: 0,
                    reason: "records_retained",
                },
                RetainedObjects {
                    count: 1,
                    reason: "referenced_elsewhere",
                },
                RetainedObjects {
                    count: 0,
                    reason: "unremovable",
                },
            ],
            objects_retained_total: 1,
            purge: true,
            records_removed: vec![
                RemovedRecords {
                    count: 1,
                    kind: "case",
                },
                RemovedRecords {
                    count: 3,
                    kind: "submission",
                },
            ],
            records_removed_total: 4,
            records_retained: 0,
        };
        let text = case_deleted(&deleted).join("\n");
        assert!(text.contains("Removed 4 record(s): case 1, submission 3."));
        assert!(text.contains("Removed 2 object(s); 1 retained: purge_not_requested 0"));
        assert!(text.contains("referenced_elsewhere 1"));
        assert!(text.contains("A purge was requested"));
        assert!(text.contains("does not erase data from the storage medium"));
        assert!(!text.contains('/'), "no path ever reaches human output");
        assert!(!text.contains("sha256"), "no digest reaches human output");
        assert_no_claim(&text);

        let without_purge = case_deleted(&Deleted {
            purge: false,
            ..deleted.clone()
        })
        .join("\n");
        assert!(without_purge.contains("No purge was requested, so no object was removed."));
        assert_no_claim(&without_purge);

        let stopped = case_deleted(&Deleted {
            records_retained: 1,
            ..deleted
        })
        .join("\n");
        assert!(
            stopped.contains("A record could not be removed, so no object was touched"),
            "a refused record unlink is stated before anything about a purge"
        );
        assert!(!stopped.contains("A purge was requested"));
        assert_no_claim(&stopped);
    }
}
