//! Human-readable output, bound by the same privacy rule as the JSON.
//!
//! No line here may carry a user-supplied path, an original filename, or any
//! payload byte. What a line may carry is what `docs/error-contract.md`
//! allows: counts, byte lengths, digests of stored artefacts, and identifiers
//! openPapir minted itself.

use openpapir_core::Report;
use openpapir_core::archive::Created;
use openpapir_core::archive::import::Imported;
use openpapir_core::{
    Association, AssociationCreated, AssociationHistory, Case, CaseCreated, CaseList, CaseView,
    Deleted, Exported, Receipt, ReceiptAdded, ReceiptList, Repaired, Submission,
    SubmissionAdded,
};

/// The lines `archive init` prints when it succeeds.
#[must_use]
pub fn created(created: &Created) -> Vec<String> {
    vec![
        "Archive created at the supplied root.".to_owned(),
        format!(
            "Archive identifier: {}. Schema version: {}.",
            created.archive_id, created.archive_schema_version
        ),
    ]
}

/// The lines `import` prints when it succeeds.
#[must_use]
pub fn imported(imported: &Imported) -> Vec<String> {
    let mut lines = vec![format!(
        "Stored {} artefact(s); {} already present.",
        imported.imported, imported.duplicates
    )];
    for artefact in &imported.artefacts {
        let state = if artefact.created_object {
            "stored".to_owned()
        } else {
            format!(
                "already present since {} after {} earlier import(s)",
                artefact
                    .first_imported_at
                    .as_deref()
                    .unwrap_or("an unrecorded time"),
                artefact.previous_import_count.unwrap_or(0)
            )
        };
        lines.push(format!(
            "{} ({} bytes), import event {}, {state}.",
            artefact.digest, artefact.byte_length, artefact.import_event
        ));
    }
    lines.push(
        "A digest identifies bytes only. Nothing here is verified, matched, or delivered."
            .to_owned(),
    );
    lines
}

/// The lines `archive check` prints, in either outcome.
///
/// Counts only. The path, the name, and the digest of a damaged object never
/// reach this output, exactly as they never reach the JSON report: the count
/// answers the only question the privacy rule allows an answer to.
#[must_use]
pub fn integrity(report: &Report) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Checked {} object(s) and {} record(s); {} byte(s) digested.",
            report.objects_checked, report.records_checked, report.bytes_digested
        ),
        if report.is_clean() {
            "No problem found.".to_owned()
        } else {
            "Problems found:".to_owned()
        },
    ];
    for problem in &report.problems {
        if problem.count > 0 {
            lines.push(format!("{} {}", problem.code, problem.count));
        }
    }
    lines.push(format!(
        "Orphan object(s): {}. Object(s) not digested: {}. Record directory(ies) not read: {}. Leftover staging file(s): {}.",
        report.orphan_objects,
        report.objects_unchecked,
        report.records_unchecked,
        report.staging_files
    ));
    lines.push(
        "The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}

/// The lines `case export` prints when it succeeds.
///
/// The destination is the one path any output here may carry: the user
/// supplied it in this invocation, and the line repeats their own argument
/// back to them. It never reaches the JSON envelope, where the privacy rule
/// admits no user-supplied path at all.
#[must_use]
pub fn exported(exported: &Exported) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Exported case {} to {}.",
            exported.case_id, exported.destination
        ),
        format!(
            "Copied {} object(s), {} byte(s), and wrote {} record(s).",
            exported.object_count, exported.bytes_copied, exported.record_count
        ),
    ];
    for kind in &exported.records {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(
        "The archive was not changed. Every copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}

/// The lines `archive repair-permissions` prints when it succeeds.
#[must_use]
pub fn repaired(repaired: &Repaired) -> Vec<String> {
    let mut lines = vec![format!(
        "Narrowed {} of {} archive path(s) to owner-only.",
        repaired.paths_changed, repaired.paths_checked
    )];
    for kind in &repaired.changed {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(
        "Permissions are only ever narrowed here; nothing was widened and no content was read or changed."
            .to_owned(),
    );
    lines
}

/// The closing line every record command prints.
///
/// A case and a submission are the user's own local organisation. openPapir
/// sends nothing, so a submission is the user's own statement about what they
/// sent, and no line above it may be read otherwise.
const RECORD_DISCLAIMER: &str = "Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.";

/// The lines describing one case, without its submissions.
fn case_lines(case: &Case) -> Vec<String> {
    let mut lines = vec![
        format!("Case {}, recorded {}.", case.id, case.created_at),
        format!("Title: {}", case.title),
    ];
    if let Some(notes) = &case.notes {
        lines.push(format!("Notes: {notes}"));
    }
    lines
}

/// The lines describing one submission and its artefact references.
fn submission_lines(submission: &Submission) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Submission {}, recorded {}.",
            submission.id, submission.created_at
        ),
        format!("Description: {}", submission.description),
    ];
    if let Some(stated_date) = &submission.stated_date {
        lines.push(format!(
            "Date stated by the user: {stated_date}. openPapir does not interpret it."
        ));
    }
    lines.push(format!(
        "Artefacts referenced: {}.",
        submission.artefacts.len()
    ));
    for artefact in &submission.artefacts {
        lines.push(match &artefact.role {
            Some(role) => format!("{} as {role}", artefact.digest),
            None => artefact.digest.clone(),
        });
    }
    lines
}

/// The lines `case create` prints when it succeeds.
#[must_use]
pub fn case_created(created: &CaseCreated) -> Vec<String> {
    let mut lines = case_lines(&created.case);
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

/// The lines `case list` prints when it succeeds.
#[must_use]
pub fn case_list(list: &CaseList) -> Vec<String> {
    let mut lines = vec![format!("{} case(s) in this archive.", list.count)];
    for case in &list.cases {
        lines.push(format!("{} {} {}", case.id, case.created_at, case.title));
    }
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

/// The lines `case show` prints when it succeeds.
#[must_use]
pub fn case_shown(view: &CaseView) -> Vec<String> {
    let mut lines = case_lines(&view.case);
    lines.push(format!("Submissions recorded: {}.", view.submission_count));
    for submission in &view.submissions {
        lines.extend(submission_lines(submission));
    }
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

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
        if deleted.purge {
            "A purge was requested: an object is unlinked only when no remaining import event, receipt, or submission references it.".to_owned()
        } else {
            "No purge was requested, so no object was removed.".to_owned()
        },
        DELETION_DISCLAIMER.to_owned(),
    ]
}

/// The closing line every receipt and association command prints.
///
/// A receipt is an artefact the user believes to be a receipt, and an
/// association is the user's own assertion about it. openPapir reads no
/// artefact bytes and matches nothing on its own.
const ASSERTION_DISCLAIMER: &str = "These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.";

/// The lines describing one receipt.
fn receipt_lines(receipt: &Receipt) -> Vec<String> {
    let mut lines = vec![
        format!("Receipt {}, recorded {}.", receipt.id, receipt.created_at),
        format!("Artefact: {}", receipt.artefact_digest),
        format!("Import event: {}", receipt.import_event_id),
    ];
    if let Some(label) = &receipt.label {
        lines.push(format!("Label: {label}"));
    }
    lines
}

/// The lines `receipt add` prints when it succeeds.
#[must_use]
pub fn receipt_added(added: &ReceiptAdded) -> Vec<String> {
    let mut lines = receipt_lines(&added.receipt);
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

/// The lines `receipt list` prints when it succeeds.
#[must_use]
pub fn receipt_list(list: &ReceiptList) -> Vec<String> {
    let mut lines = vec![format!("{} receipt(s) in this archive.", list.count)];
    for receipt in &list.receipts {
        lines.push(format!(
            "{} {} {}",
            receipt.id, receipt.created_at, receipt.artefact_digest
        ));
    }
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

/// The lines describing one association and its candidates.
fn association_lines(association: &Association) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Association {}, recorded {} by {}.",
            association.id, association.created_at, association.created_by
        ),
        format!("Receipt: {}", association.receipt_id),
        format!("Outcome: {}", association.outcome),
        format!(
            "Confirmed submission: {}",
            association.submission_id.as_deref().unwrap_or("none")
        ),
        format!(
            "Supersedes: {}",
            association.supersedes.as_deref().unwrap_or("nothing")
        ),
        format!("Candidates: {}.", association.candidates.len()),
    ];
    for candidate in &association.candidates {
        lines.push(format!(
            "{} confidence {}",
            candidate.submission_id, candidate.confidence
        ));
        for evidence in &candidate.evidence {
            lines.push(format!(
                "{} from {}: {}",
                evidence.kind, evidence.source, evidence.statement
            ));
        }
    }
    lines
}

/// The lines `association create` prints when it succeeds.
#[must_use]
pub fn association_created(created: &AssociationCreated) -> Vec<String> {
    let mut lines = association_lines(&created.association);
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

/// The lines `association list` prints when it succeeds.
///
/// The whole history is printed, newest first, superseded records included.
#[must_use]
pub fn association_history(history: &AssociationHistory) -> Vec<String> {
    let mut lines = vec![format!(
        "{} association(s) for receipt {}, newest first.",
        history.count, history.receipt_id
    )];
    for association in &history.associations {
        lines.extend(association_lines(association));
    }
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

/// The lines `submission add` prints when it succeeds.
#[must_use]
pub fn submission_added(added: &SubmissionAdded) -> Vec<String> {
    let mut lines = vec![format!("Case {}.", added.submission.case_id)];
    lines.extend(submission_lines(&added.submission));
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpapir_core::{
        Artefact, ArtefactRef, Candidate, Evidence, RemovedRecords, RetainedObjects,
    };

    const CASE_ID: &str = "0123456789abcdef0123456789abcdef";
    const SUBMISSION_ID: &str = "fedcba9876543210fedcba9876543210";
    const RECEIPT_ID: &str = "aaaabbbbccccddddeeeeffff00001111";
    const ASSOCIATION_ID: &str = "1111000fffeeeeddddccccbbbbaaaa00";

    /// Words no receipt or association output may use of its own record.
    const FORBIDDEN: [&str; 6] = [
        "delivered",
        "accepted",
        "official",
        "legally effective",
        "authentic",
        "verified",
    ];

    fn receipt() -> Receipt {
        Receipt {
            archive_schema_version: 1,
            artefact_digest: "sha256:aa".to_owned(),
            created_at: "2026-01-16T11:00:00Z".to_owned(),
            id: RECEIPT_ID.to_owned(),
            import_event_id: "22223333444455556666777788889999".to_owned(),
            label: Some("Envelope from the post".to_owned()),
            record_kind: "receipt".to_owned(),
        }
    }

    fn association(outcome: &str, submission_id: Option<&str>) -> Association {
        Association {
            archive_schema_version: 1,
            candidates: vec![Candidate {
                confidence: "moderate".to_owned(),
                evidence: vec![Evidence {
                    kind: "user_assertion".to_owned(),
                    source: "user".to_owned(),
                    statement: "The user stated a link.".to_owned(),
                }],
                submission_id: SUBMISSION_ID.to_owned(),
            }],
            created_at: "2026-01-17T12:00:00Z".to_owned(),
            created_by: "user".to_owned(),
            id: ASSOCIATION_ID.to_owned(),
            outcome: outcome.to_owned(),
            receipt_id: RECEIPT_ID.to_owned(),
            record_kind: "association".to_owned(),
            submission_id: submission_id.map(str::to_owned),
            supersedes: None,
        }
    }

    /// Assert that no line claims delivery, authenticity, or legal effect.
    fn assert_no_claim(text: &str) {
        let lower = text.to_lowercase();
        for word in FORBIDDEN {
            let claimed = lower
                .match_indices(word)
                .any(|(at, _)| !lower[..at].contains("reports no"));
            assert!(!claimed, "output claims something it may not claim");
        }
    }

    #[test]
    fn human_receipt_output_carries_the_record_and_no_path() {
        let text = receipt_added(&ReceiptAdded { receipt: receipt() }).join("\n");
        assert!(text.contains(RECEIPT_ID));
        assert!(text.contains("Artefact: sha256:aa"));
        assert!(text.contains("Import event: 2222"));
        assert!(text.contains("Label: Envelope from the post"));
        assert!(!text.contains('/'), "no path ever reaches human output");
        assert_no_claim(&text);

        let mut plain = receipt();
        plain.label = None;
        let listed = receipt_list(&ReceiptList {
            count: 1,
            receipts: vec![plain],
        })
        .join("\n");
        assert!(listed.contains("1 receipt(s) in this archive."));
        assert!(!listed.contains("Label"));
        assert_no_claim(&listed);

        let empty = receipt_list(&ReceiptList {
            count: 0,
            receipts: Vec::new(),
        });
        assert!(empty[0].starts_with("0 receipt(s)"));
    }

    #[test]
    fn human_association_output_names_its_outcome_and_claims_nothing() {
        let text = association_created(&AssociationCreated {
            association: association("associated", Some(SUBMISSION_ID)),
        })
        .join("\n");
        assert!(text.contains("Outcome: associated"));
        assert!(text.contains(&format!("Confirmed submission: {SUBMISSION_ID}")));
        assert!(text.contains("Supersedes: nothing"));
        assert!(text.contains("Candidates: 1."));
        assert!(text.contains("user_assertion from user: The user stated a link."));
        assert!(!text.contains('/'), "no path ever reaches human output");
        assert_no_claim(&text);

        for outcome in ["unassociated", "candidate", "contradictory"] {
            let text = association_created(&AssociationCreated {
                association: association(outcome, None),
            })
            .join("\n");
            assert!(text.contains("Confirmed submission: none"));
            assert_no_claim(&text);
        }
    }

    #[test]
    fn a_listed_history_is_newest_first_and_shows_what_each_supersedes() {
        let mut newer = association("candidate", None);
        newer.id = "99998888777766665555444433332222".to_owned();
        newer.supersedes = Some(ASSOCIATION_ID.to_owned());
        let text = association_history(&AssociationHistory {
            associations: vec![newer, association("unassociated", None)],
            count: 2,
            receipt_id: RECEIPT_ID.to_owned(),
        })
        .join("\n");
        assert!(text.contains(&format!(
            "2 association(s) for receipt {RECEIPT_ID}, newest first."
        )));
        assert!(text.contains(&format!("Supersedes: {ASSOCIATION_ID}")));
        assert!(text.contains("Supersedes: nothing"), "history is complete");
        assert_no_claim(&text);
    }

    fn case() -> Case {
        Case {
            archive_schema_version: 1,
            created_at: "2026-01-14T09:12:33Z".to_owned(),
            id: CASE_ID.to_owned(),
            notes: Some("A note the user wrote.".to_owned()),
            record_kind: "case".to_owned(),
            title: "Tax matter".to_owned(),
        }
    }

    fn submission() -> Submission {
        Submission {
            archive_schema_version: 1,
            artefacts: vec![
                ArtefactRef {
                    digest: "sha256:aa".to_owned(),
                    role: Some("cover letter".to_owned()),
                },
                ArtefactRef {
                    digest: "sha256:bb".to_owned(),
                    role: None,
                },
            ],
            case_id: CASE_ID.to_owned(),
            created_at: "2026-01-15T10:00:00Z".to_owned(),
            description: "Posted the completed form.".to_owned(),
            id: SUBMISSION_ID.to_owned(),
            record_kind: "submission".to_owned(),
            stated_date: Some("2026-01-13".to_owned()),
        }
    }

    #[test]
    fn human_case_output_carries_the_users_own_fields_and_no_path() {
        let text = case_created(&CaseCreated { case: case() }).join("\n");
        assert!(text.contains(CASE_ID));
        assert!(text.contains("Title: Tax matter"));
        assert!(text.contains("Notes: A note the user wrote."));
        assert!(text.contains("Nothing here is verified, matched, or delivered."));
        assert!(!text.contains('/'), "no path ever reaches human output");

        let listed = case_list(&CaseList {
            cases: vec![case()],
            count: 1,
        })
        .join("\n");
        assert!(listed.contains("1 case(s) in this archive."));
        assert!(listed.contains("Tax matter"));
        assert!(!listed.contains('/'));

        let empty = case_list(&CaseList {
            cases: Vec::new(),
            count: 0,
        });
        assert_eq!(empty.len(), 2, "an empty archive still says so");
        assert!(empty[0].starts_with("0 case(s)"));
    }

    #[test]
    fn human_submission_output_never_calls_a_stated_date_a_delivery() {
        let text = submission_added(&SubmissionAdded {
            submission: submission(),
        })
        .join("\n");
        assert!(text.contains(SUBMISSION_ID));
        assert!(text.contains("Description: Posted the completed form."));
        assert!(text.contains("Date stated by the user: 2026-01-13."));
        assert!(text.contains("sha256:aa as cover letter"));
        assert!(text.contains("sha256:bb"));
        assert!(text.contains("Artefacts referenced: 2."));
        for forbidden in ["deliver", "receiv", "verified successfully", "legal"] {
            assert!(
                !text.to_lowercase().contains(forbidden)
                    || text.contains("Nothing here is verified, matched, or delivered."),
                "no wording implies delivery or legal effect"
            );
        }
    }

    #[test]
    fn a_shown_case_lists_its_submissions_without_a_date_when_none_was_given() {
        let mut without_date = submission();
        without_date.stated_date = None;
        without_date.artefacts.clear();
        let text = case_shown(&CaseView {
            case: case(),
            submissions: vec![without_date],
            submission_count: 1,
        })
        .join("\n");
        assert!(text.contains("Submissions recorded: 1."));
        assert!(text.contains("Artefacts referenced: 0."));
        assert!(!text.contains("Date stated by the user"));
    }

    #[test]
    fn human_import_output_names_no_file_and_promises_nothing() {
        let lines = imported(&Imported {
            imported: 1,
            duplicates: 1,
            artefacts: vec![
                Artefact {
                    digest: "sha256:aa".to_owned(),
                    byte_length: 4,
                    import_event: "0123".to_owned(),
                    created_object: true,
                    previous_import_count: None,
                    first_imported_at: None,
                },
                Artefact {
                    digest: "sha256:bb".to_owned(),
                    byte_length: 4,
                    import_event: "4567".to_owned(),
                    created_object: false,
                    previous_import_count: Some(2),
                    first_imported_at: Some("2026-01-14T09:12:33Z".to_owned()),
                },
            ],
        });
        let text = lines.join("\n");
        assert!(text.contains("Stored 1 artefact(s); 1 already present."));
        assert!(text.contains("already present since 2026-01-14T09:12:33Z after 2"));
        assert!(text.contains("Nothing here is verified"));
        assert!(!text.contains('/'), "no path ever reaches human output");
    }

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
            ..deleted
        })
        .join("\n");
        assert!(without_purge.contains("No purge was requested, so no object was removed."));
        assert_no_claim(&without_purge);
    }

    #[test]
    fn human_creation_output_reports_the_identifier_not_the_root() {
        let lines = created(&Created {
            archive_id: "0123456789abcdef0123456789abcdef".to_owned(),
            archive_schema_version: 1,
        });
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("0123456789abcdef0123456789abcdef"));
        assert!(!lines.join("\n").contains('/'));
    }
}
