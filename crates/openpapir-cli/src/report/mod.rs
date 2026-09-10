//! Human-readable output, bound by the same privacy rule as the JSON.
//! `case import`'s renderer obeys it from `restore`.
//!
//! No line here may carry a user-supplied path, an original filename, or any
//! payload byte. What a line may carry is what `docs/error-contract.md`
//! allows: counts, byte lengths, digests of stored artefacts, and identifiers
//! openPapir minted itself.
//!
//! One module per command family renders that family's lines. This file holds
//! what more than one of them needs and re-exports every renderer, so a
//! caller still names `report::case_created` and nothing else moved.

mod archive;
mod cases;
mod deletion;
mod records;
mod status;
mod transfer;

pub use archive::created;
pub use cases::{case_created, case_list, case_shown, case_updated};
pub use deletion::case_deleted;
pub use records::{
    association_created, association_history, association_retired, receipt_added, receipt_list,
    submission_added,
};
pub use status::integrity;
pub use transfer::{exported, imported, repaired};

use openpapir_core::Submission;

/// The closing line every record command prints.
///
/// A case and a submission are the user's own local organisation. openPapir
/// sends nothing, so a submission is the user's own statement about what they
/// sent, and no line above it may be read otherwise.
const RECORD_DISCLAIMER: &str = "Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.";

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

/// What the family modules' own tests build their records from.
#[cfg(test)]
mod fixtures {
    use openpapir_core::{ArtefactRef, Submission};

    pub const CASE_ID: &str = "0123456789abcdef0123456789abcdef";
    pub const SUBMISSION_ID: &str = "fedcba9876543210fedcba9876543210";

    /// Words no receipt or association output may use of its own record.
    const FORBIDDEN: [&str; 6] = [
        "delivered",
        "accepted",
        "official",
        "legally effective",
        "authentic",
        "verified",
    ];

    /// Assert that no line claims delivery, authenticity, or legal effect.
    pub fn assert_no_claim(text: &str) {
        let lower = text.to_lowercase();
        for word in FORBIDDEN {
            let claimed = lower
                .match_indices(word)
                .any(|(at, _)| !lower[..at].contains("reports no"));
            assert!(!claimed, "output claims something it may not claim");
        }
    }

    /// One submission of two artefacts, one of them without a role.
    pub fn submission() -> Submission {
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
}
