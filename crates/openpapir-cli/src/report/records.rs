//! The lines the record commands print: a submission the user states they
//! sent, a receipt they believe they received, and what they assert about
//! whether the two relate.

use openpapir_core::{
    Association, AssociationCreated, AssociationHistory, AssociationView, Receipt, ReceiptAdded,
    ReceiptList, ReceiptView, SubmissionAdded, SubmissionView,
};

use super::{RECORD_DISCLAIMER, submission_lines};

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

/// The lines `association retire` prints when it succeeds.
///
/// A retirement is a record like any other, so it is printed like any other,
/// with the record it supersedes named. The user's own reason is stored on
/// the record and is not printed back at them.
#[must_use]
pub fn association_retired(retired: &AssociationCreated) -> Vec<String> {
    let mut lines = vec![
        "The assertion is withdrawn. Both records stay, and nothing was edited or removed."
            .to_owned(),
    ];
    lines.extend(association_lines(&retired.association));
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

/// The lines `submission show` prints when it succeeds.
///
/// The associations naming the submission follow it, the live heads first and
/// the superseded records after them, each one printed whole.
#[must_use]
pub fn submission_shown(view: &SubmissionView) -> Vec<String> {
    let mut lines = vec![format!("Case {}.", view.submission.case_id)];
    lines.extend(submission_lines(&view.submission));
    lines.push(format!(
        "Associations naming this submission: {}, live first.",
        view.association_count
    ));
    for association in &view.associations {
        lines.extend(association_lines(association));
    }
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

/// The lines `receipt show` prints when it succeeds.
///
/// The whole association history follows the receipt, newest first and
/// superseded records included, exactly as `association list` prints it.
#[must_use]
pub fn receipt_shown(view: &ReceiptView) -> Vec<String> {
    let mut lines = receipt_lines(&view.receipt);
    lines.push(format!(
        "Associations about this receipt: {}, newest first.",
        view.association_count
    ));
    for association in &view.associations {
        lines.extend(association_lines(association));
    }
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

/// The lines `association show` prints when it succeeds.
///
/// The shown record is named first, with whether it is the live head of its
/// chain, and then the chain follows, newest first and superseded records
/// included. The chain always holds the shown record itself, so printing its
/// block before the chain as well would print the same record twice; it is
/// marked where it stands in the chain instead. `data` is unchanged: the JSON
/// form still carries `association` beside `chain`.
#[must_use]
pub fn association_shown(view: &AssociationView) -> Vec<String> {
    let mut lines = vec![
        format!("Association {}, with its chain.", view.association.id),
        format!(
            "Live head of its chain: {}.",
            if view.live { "yes" } else { "no" }
        ),
        format!(
            "Records in the chain: {}, newest first. The record shown is marked.",
            view.chain_length
        ),
    ];
    for association in &view.chain {
        if association.id == view.association.id {
            lines.push("The record shown:".to_owned());
        }
        lines.extend(association_lines(association));
    }
    lines.push(ASSERTION_DISCLAIMER.to_owned());
    lines
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{SUBMISSION_ID, assert_no_claim, submission};
    use super::*;
    use openpapir_core::{Candidate, Evidence};

    const RECEIPT_ID: &str = "aaaabbbbccccddddeeeeffff00001111";
    const ASSOCIATION_ID: &str = "1111000fffeeeeddddccccbbbbaaaa00";

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
            statement: None,
            submission_id: submission_id.map(str::to_owned),
            supersedes: None,
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

    #[test]
    fn a_shown_association_is_printed_once_and_marked_in_its_chain() {
        let mut newer = association("candidate", None);
        newer.id = "99998888777766665555444433332222".to_owned();
        newer.supersedes = Some(ASSOCIATION_ID.to_owned());
        let shown = association("unassociated", None);
        let lines = association_shown(&AssociationView {
            association: shown.clone(),
            chain: vec![newer, shown],
            chain_length: 2,
            live: false,
        });
        let text = lines.join("\n");
        assert!(text.contains(&format!("Association {ASSOCIATION_ID}, with its chain.")));
        assert!(text.contains("Live head of its chain: no."));
        assert!(text.contains("The record shown:"));
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with(&format!("Association {ASSOCIATION_ID}, recorded")))
                .count(),
            1,
            "the shown record's own block is printed once"
        );
        assert_no_claim(&text);
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
}
