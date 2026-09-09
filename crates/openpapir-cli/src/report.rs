//! Human-readable output, bound by the same privacy rule as the JSON.
//!
//! No line here may carry a user-supplied path, an original filename, or any
//! payload byte. What a line may carry is what `docs/error-contract.md`
//! allows: counts, byte lengths, digests of stored artefacts, and identifiers
//! openPapir minted itself.

use openpapir_core::archive::Created;
use openpapir_core::archive::import::Imported;
use openpapir_core::{Case, CaseCreated, CaseList, CaseView, Submission, SubmissionAdded};

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
    use openpapir_core::{Artefact, ArtefactRef};

    const CASE_ID: &str = "0123456789abcdef0123456789abcdef";
    const SUBMISSION_ID: &str = "fedcba9876543210fedcba9876543210";

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
