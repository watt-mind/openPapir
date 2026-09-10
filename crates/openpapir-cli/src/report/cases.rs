//! The lines the `case` commands print, deletion excepted.

use openpapir_core::{Case, CaseCreated, CaseList, CaseUpdated, CaseView};

use super::{RECORD_DISCLAIMER, derived_lines, submission_lines};

/// The lines describing one case, without its submissions.
///
/// `updated_at` is printed only once there is one: a case nobody has changed
/// has never been updated.
fn case_lines(case: &Case) -> Vec<String> {
    let mut lines = vec![
        format!("Case {}, recorded {}.", case.id, case.created_at),
        format!("Title: {}", case.title),
    ];
    if let Some(notes) = &case.notes {
        lines.push(format!("Notes: {notes}"));
    }
    lines.push(format!("Status: {}", case.status.as_str()));
    let tags = case.tags.join(", ");
    lines.push(format!(
        "Tags: {}",
        if tags.is_empty() { "none" } else { &tags }
    ));
    if let Some(updated_at) = &case.updated_at {
        lines.push(format!("Updated: {updated_at}"));
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
///
/// The count is what the listing holds, so under a filter what matched. The
/// filter is not echoed back: the query is the user's own text.
#[must_use]
pub fn case_list(list: &CaseList) -> Vec<String> {
    let mut lines = vec![format!("{} case(s) listed.", list.count)];
    for case in &list.cases {
        lines.push(format!(
            "{} {} {} {}",
            case.id,
            case.created_at,
            case.status.as_str(),
            case.title
        ));
    }
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

/// The lines `case update` prints when it succeeds.
///
/// The changed fields are named and no former value is printed: the record
/// above already carries what each value is now.
#[must_use]
pub fn case_updated(updated: &CaseUpdated) -> Vec<String> {
    let mut lines = case_lines(&updated.case);
    lines.push(format!("Changed: {}.", updated.changed.join(", ")));
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

/// The lines `case show` prints when it succeeds.
///
/// The receipts are the ones a live association ties to a submission of the
/// case. Each is the user's own assertion, so the line names the outcome the
/// user recorded and claims nothing beyond it.
#[must_use]
pub fn case_shown(view: &CaseView) -> Vec<String> {
    let mut lines = case_lines(&view.case);
    lines.push(format!("Submissions recorded: {}.", view.submission_count));
    for submission in &view.submissions {
        lines.extend(submission_lines(submission));
    }
    lines.push(format!(
        "Receipts a live association names: {}.",
        view.receipts.len()
    ));
    for entry in &view.receipts {
        lines.push(format!(
            "Receipt {} outcome {} by association {}",
            entry.receipt.id, entry.outcome, entry.association_id
        ));
        for submission_id in &entry.submission_ids {
            lines.push(format!("names submission {submission_id}"));
        }
    }
    lines.extend(derived_lines(&view.derived));
    lines.push(RECORD_DISCLAIMER.to_owned());
    lines
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{CASE_ID, SUBMISSION_ID, submission};
    use super::*;

    fn case() -> Case {
        Case {
            archive_schema_version: 1,
            created_at: "2026-01-14T09:12:33Z".to_owned(),
            id: CASE_ID.to_owned(),
            notes: Some("A note the user wrote.".to_owned()),
            record_kind: "case".to_owned(),
            status: openpapir_core::CaseStatus::Open,
            tags: vec!["appeal".to_owned(), "tax".to_owned()],
            title: "Tax matter".to_owned(),
            updated_at: None,
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
        assert!(listed.contains("1 case(s) listed."));
        assert!(listed.contains("Tax matter"));
        assert!(!listed.contains('/'));

        let empty = case_list(&CaseList {
            cases: Vec::new(),
            count: 0,
        });
        assert_eq!(empty.len(), 2, "an empty listing still says so");
        assert!(empty[0].starts_with("0 case(s)"));
    }

    #[test]
    fn human_case_output_carries_the_status_the_tags_and_any_update_time() {
        let text = case_created(&CaseCreated { case: case() }).join("\n");
        assert!(text.contains("Status: open"));
        assert!(text.contains("Tags: appeal, tax"));
        assert!(
            !text.contains("Updated:"),
            "a case nobody changed has no update time"
        );

        let mut changed = case();
        changed.tags.clear();
        changed.status = openpapir_core::CaseStatus::Closed;
        changed.updated_at = Some("2026-02-01T08:00:00Z".to_owned());
        let text = case_updated(&CaseUpdated {
            case: changed,
            changed: vec!["status".to_owned(), "tags".to_owned()],
        })
        .join("\n");
        assert!(text.contains("Status: closed"));
        assert!(text.contains("Tags: none"));
        assert!(text.contains("Updated: 2026-02-01T08:00:00Z"));
        assert!(text.contains("Changed: status, tags."));
        assert!(!text.contains('/'), "no path ever reaches human output");
    }

    #[test]
    fn a_shown_case_lists_its_submissions_without_a_date_when_none_was_given() {
        let mut without_date = submission();
        without_date.stated_date = None;
        without_date.artefacts.clear();
        let text = case_shown(&CaseView {
            case: case(),
            derived: Vec::new(),
            receipts: Vec::new(),
            submissions: vec![without_date],
            submission_count: 1,
        })
        .join("\n");
        assert!(text.contains("Submissions recorded: 1."));
        assert!(text.contains("Artefacts referenced: 0."));
        assert!(!text.contains("Date stated by the user"));
        assert!(text.contains("Receipts a live association names: 0."));
    }

    /// A receipt reaches the case only through a live association, so the
    /// line names that association and the outcome the user recorded, and
    /// claims nothing else about the file.
    #[test]
    fn a_shown_case_names_the_receipts_its_live_associations_name() {
        const RECEIPT_ID: &str = "aaaabbbbccccddddeeeeffff00001111";
        const ASSOCIATION_ID: &str = "1111000fffeeeeddddccccbbbbaaaa00";
        let text = case_shown(&CaseView {
            case: case(),
            derived: Vec::new(),
            receipts: vec![openpapir_core::CaseReceipt {
                association_id: ASSOCIATION_ID.to_owned(),
                outcome: "candidate".to_owned(),
                receipt: openpapir_core::Receipt {
                    archive_schema_version: 1,
                    artefact_digest: "sha256:bb".to_owned(),
                    created_at: "2026-01-16T11:00:00Z".to_owned(),
                    id: RECEIPT_ID.to_owned(),
                    import_event_id: "22223333444455556666777788889999".to_owned(),
                    label: None,
                    record_kind: "receipt".to_owned(),
                },
                submission_ids: vec![SUBMISSION_ID.to_owned()],
            }],
            submissions: vec![submission()],
            submission_count: 1,
        })
        .join("\n");
        assert!(text.contains("Receipts a live association names: 1."));
        assert!(text.contains(&format!(
            "Receipt {RECEIPT_ID} outcome candidate by association {ASSOCIATION_ID}"
        )));
        assert!(text.contains(&format!("names submission {SUBMISSION_ID}")));
        assert!(!text.contains('/'), "no path ever reaches human output");
    }
}
