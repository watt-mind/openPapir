//! Unit tests for the archive summary and its reminders.
//!
//! The reminder arithmetic is exercised directly on built records, so a case
//! that needs a particular date does not need an archive on disk. The
//! archive-level behaviour, the read-only guarantee included, is exercised
//! against a real temporary archive.

use super::*;
use crate::archive;
use crate::archive::SUPPORTED_SCHEMA_VERSION;
use crate::error::codes;
use crate::records::association::{Candidate, Evidence};
use crate::records::case;
use crate::records::submission::ArtefactRef;

/// One submission record, built rather than written.
fn submission(id: &str, stated_date: Option<&str>) -> Submission {
    Submission {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        artefacts: Vec::<ArtefactRef>::new(),
        case_id: "0".repeat(32),
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        description: "Posted the completed form.".to_owned(),
        id: id.to_owned(),
        record_kind: crate::records::submission::KIND.to_owned(),
        stated_date: stated_date.map(str::to_owned),
    }
}

/// One association naming `submission_id` with the supplied outcome.
fn association(outcome: &str, submission_id: &str) -> Association {
    Association {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        candidates: vec![Candidate {
            confidence: "moderate".to_owned(),
            evidence: vec![Evidence {
                kind: crate::records::association::EVIDENCE_KIND.to_owned(),
                source: crate::records::association::EVIDENCE_SOURCE.to_owned(),
                statement: "The reference matches.".to_owned(),
            }],
            submission_id: submission_id.to_owned(),
        }],
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        created_by: crate::records::association::CREATED_BY.to_owned(),
        id: "a".repeat(32),
        outcome: outcome.to_owned(),
        receipt_id: "b".repeat(32),
        record_kind: crate::records::association::KIND.to_owned(),
        statement: None,
        submission_id: (outcome == "associated").then(|| submission_id.to_owned()),
        supersedes: None,
    }
}

#[test]
fn a_submission_inside_the_window_is_listed_with_its_computed_dates() {
    let submissions = [submission(&"1".repeat(32), Some("2026-01-20"))];
    let (reminders, undated) = reminders(&submissions, &BTreeSet::new(), days_of("2026-02-10"));
    assert_eq!(undated, 0);
    assert_eq!(reminders.len(), 1);
    assert_eq!(reminders[0].submission_date, "2026-01-20");
    assert_eq!(reminders[0].retrieve_by, "2026-02-19");
    assert_eq!(reminders[0].days_left, 9);
    assert_eq!(reminders[0].submission_id, "1".repeat(32));
}

#[test]
fn the_last_day_of_the_window_is_still_open_and_the_next_one_is_not() {
    let submissions = [submission(&"1".repeat(32), Some("2026-01-13"))];
    let on_the_day = reminders(&submissions, &BTreeSet::new(), days_of("2026-02-12")).0;
    assert_eq!(on_the_day.len(), 1, "the last day still reminds");
    assert_eq!(on_the_day[0].days_left, 0);
    let after = reminders(&submissions, &BTreeSet::new(), days_of("2026-02-13")).0;
    assert!(
        after.is_empty(),
        "the day after the window reminds of nothing"
    );
}

#[test]
fn a_future_as_of_is_a_what_if_rather_than_a_refusal() {
    let submissions = [submission(&"1".repeat(32), Some("2026-01-13"))];
    let earlier = reminders(&submissions, &BTreeSet::new(), days_of("2020-01-01")).0;
    assert_eq!(earlier[0].days_left, 2_234);
    assert_eq!(earlier[0].retrieve_by, "2026-02-12");
    assert!(
        reminders(&submissions, &BTreeSet::new(), days_of("2099-01-01"))
            .0
            .is_empty()
    );
}

#[test]
fn only_an_associated_or_candidate_outcome_takes_a_submission_off_the_list() {
    let id = "1".repeat(32);
    let submissions = [submission(&id, Some("2026-01-20"))];
    for (outcome, listed) in [
        ("associated", false),
        ("candidate", false),
        ("unassociated", true),
        ("contradictory", true),
    ] {
        let associations = [association(outcome, &id)];
        let named = named_submissions(&associations);
        let found = reminders(&submissions, &named, days_of("2026-02-10")).0;
        assert_eq!(
            found.len(),
            usize::from(listed),
            "outcome {outcome} decides the reminder"
        );
    }
}

/// Retracting an assertion brings the reminder back. Only the live head of a
/// supersession chain says what the user asserts today; the superseded record
/// stays readable and is simply no longer read here.
#[test]
fn a_superseded_association_no_longer_takes_a_submission_off_the_list() {
    let id = "1".repeat(32);
    let submissions = [submission(&id, Some("2026-01-20"))];
    let mut asserted = association("associated", &id);
    asserted.id = "c".repeat(32);
    let mut retraction = association("unassociated", &id);
    retraction.candidates.clear();
    retraction.submission_id = None;
    retraction.id = "d".repeat(32);
    retraction.supersedes = Some(asserted.id.clone());

    let live_only = [asserted.clone()];
    assert!(
        reminders(
            &submissions,
            &named_submissions(&live_only),
            days_of("2026-02-10")
        )
        .0
        .is_empty(),
        "a live assertion still ends the reminder"
    );

    let retracted = [asserted, retraction];
    let named = named_submissions(&retracted);
    assert!(named.is_empty(), "the superseded record is not read");
    assert_eq!(
        reminders(&submissions, &named, days_of("2026-02-10"))
            .0
            .len(),
        1,
        "retracting the assertion brings the reminder back"
    );
}

/// A chain of three: only the last record is read, whatever the two it
/// supersedes said.
#[test]
fn only_the_live_head_of_a_supersession_chain_is_read() {
    let id = "1".repeat(32);
    let submissions = [submission(&id, Some("2026-01-20"))];
    let mut first = association("unassociated", &id);
    first.candidates.clear();
    first.submission_id = None;
    first.id = "c".repeat(32);
    let mut second = association("associated", &id);
    second.id = "d".repeat(32);
    second.supersedes = Some(first.id.clone());
    let mut third = association("candidate", &id);
    third.id = "e".repeat(32);
    third.supersedes = Some(second.id.clone());

    let chain = [first, second, third];
    let named = named_submissions(&chain);
    assert_eq!(named.len(), 1, "the live candidate names the submission");
    assert!(
        reminders(&submissions, &named, days_of("2026-02-10"))
            .0
            .is_empty()
    );
}

#[test]
fn an_association_naming_another_submission_leaves_this_one_listed() {
    let id = "1".repeat(32);
    let submissions = [submission(&id, Some("2026-01-20"))];
    let associations = [association("associated", &"2".repeat(32))];
    let named = named_submissions(&associations);
    assert_eq!(
        reminders(&submissions, &named, days_of("2026-02-10"))
            .0
            .len(),
        1
    );
}

#[test]
fn a_submission_without_a_usable_date_is_counted_and_never_listed() {
    let submissions = [
        submission(&"1".repeat(32), None),
        submission(&"2".repeat(32), Some("")),
        submission(&"3".repeat(32), Some("2026-02-30")),
        submission(&"4".repeat(32), Some("not a date")),
    ];
    let (found, undated) = reminders(&submissions, &BTreeSet::new(), days_of("2026-02-10"));
    assert!(found.is_empty());
    assert_eq!(undated, 4, "a date openPapir cannot read is not a window");
}

#[test]
fn the_list_is_ordered_by_the_date_the_window_closes() {
    let submissions = [
        submission(&"3".repeat(32), Some("2026-02-01")),
        submission(&"1".repeat(32), Some("2026-01-20")),
        submission(&"2".repeat(32), Some("2026-01-20")),
    ];
    let found = reminders(&submissions, &BTreeSet::new(), days_of("2026-02-10")).0;
    let order: Vec<&str> = found
        .iter()
        .map(|reminder| reminder.retrieve_by.as_str())
        .collect();
    assert_eq!(order, ["2026-02-19", "2026-02-19", "2026-03-03"]);
    assert!(
        found[0].submission_id < found[1].submission_id,
        "one closing date is broken by identifier, so the order is fixed"
    );
}

#[test]
fn an_as_of_that_is_not_a_calendar_date_is_a_usage_refusal() {
    for value in ["2026-13-01", "2026-02-30", "20260210", "today", "2026-2-1"] {
        let refusal = checked_as_of(Some(value)).unwrap_err();
        assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        assert_eq!(refusal.exit_code(), 2);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["argument"], "as_of");
        assert!(
            !json.to_string().contains(value),
            "a refused value is never echoed"
        );
    }
    assert_eq!(checked_as_of(Some("2026-02-10")).unwrap(), "2026-02-10");
}

#[test]
fn an_absent_as_of_is_the_clock_s_own_date() {
    let today = checked_as_of(None).unwrap();
    assert_eq!(today, clock::today());
    assert_eq!(checked_as_of(Some("")).unwrap(), today);
}

/// The same retraction, through the real records rather than built ones.
#[test]
fn a_retracted_assertion_brings_the_reminder_back_in_a_real_archive() {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let input = inputs.path().join("alpha.txt");
    std::fs::write(&input, b"synthetic alpha\n").unwrap();
    let imported = crate::archive::import::import(root.path(), &[input])
        .unwrap()
        .data;
    let digest = imported.artefacts[0].digest.clone();
    let case = case::create(root.path(), "Tax matter", None)
        .unwrap()
        .data
        .case;
    let recorded = crate::records::submission::add(
        root.path(),
        &case.id,
        "Posted the completed form.",
        Some("2026-01-20"),
        std::slice::from_ref(&digest),
    )
    .unwrap()
    .data
    .submission;
    let receipt = crate::records::receipt::add(root.path(), &digest, None, None)
        .unwrap()
        .data
        .receipt;

    let asserted = crate::records::association::create(
        root.path(),
        &receipt.id,
        "associated",
        &[format!(
            "{}:strong:The case number is the same.",
            recorded.id
        )],
        None,
    )
    .unwrap()
    .data
    .association;
    assert!(
        status(root.path(), Some("2026-02-10"))
            .unwrap()
            .data
            .receipts_to_retrieve
            .is_empty(),
        "a live assertion ends the reminder"
    );

    crate::records::association::create(
        root.path(),
        &receipt.id,
        "unassociated",
        &[],
        Some(&asserted.id),
    )
    .unwrap();
    let summary = status(root.path(), Some("2026-02-10")).unwrap().data;
    assert_eq!(summary.associations, 2, "history keeps both records");
    assert_eq!(
        summary.receipts_to_retrieve.len(),
        1,
        "the retraction brings the reminder back"
    );
    assert_eq!(summary.receipts_to_retrieve[0].submission_id, recorded.id);
}

#[test]
fn an_empty_archive_is_summarised_with_zeroes_and_no_reminder() {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let summary = status(root.path(), Some("2026-02-10")).unwrap().data;
    assert_eq!(summary.as_of, "2026-02-10");
    assert_eq!(summary.cases, 0);
    assert_eq!(summary.submissions, 0);
    assert_eq!(summary.receipts, 0);
    assert_eq!(summary.associations, 0);
    assert_eq!(summary.stored_objects, 0);
    assert_eq!(summary.undated_submissions, 0);
    assert_eq!(summary.retention_window_days, 30);
    assert!(summary.receipts_to_retrieve.is_empty());
    let statuses: Vec<(&str, u64)> = summary
        .cases_by_status
        .iter()
        .map(|entry| (entry.status, entry.count))
        .collect();
    assert_eq!(
        statuses,
        [("open", 0), ("closed", 0)],
        "every status is reported, including one no case holds"
    );
}

/// The breakdown counts every status and sums to the total, and a closed case
/// is still reminded of: closing a case is the user's own filing and openPapir
/// never decides that it means no receipt is wanted.
#[test]
fn the_status_breakdown_counts_every_case_and_changes_no_reminder() {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let open = case::create(root.path(), "Tax matter", None)
        .unwrap()
        .data
        .case;
    let closed = case::create(root.path(), "Parking notice", None)
        .unwrap()
        .data
        .case;
    crate::records::submission::add(
        root.path(),
        &closed.id,
        "Posted the completed form.",
        Some("2026-01-20"),
        &[],
    )
    .unwrap();
    case::update(
        root.path(),
        &closed.id,
        &case::Change {
            status: Some(case::Status::Closed),
            ..case::Change::default()
        },
    )
    .unwrap();

    let summary = status(root.path(), Some("2026-02-10")).unwrap().data;
    assert_eq!(summary.cases, 2);
    let statuses: Vec<(&str, u64)> = summary
        .cases_by_status
        .iter()
        .map(|entry| (entry.status, entry.count))
        .collect();
    assert_eq!(statuses, [("open", 1), ("closed", 1)]);
    assert_eq!(
        statuses.iter().map(|(_, count)| count).sum::<u64>(),
        summary.cases,
        "the breakdown sums to the total"
    );
    assert_eq!(
        summary.receipts_to_retrieve.len(),
        1,
        "a closed case's submission is still reminded of"
    );
    assert_eq!(summary.receipts_to_retrieve[0].case_id, closed.id);
    let _ = open;
}

#[test]
fn the_summary_counts_what_the_archive_holds_and_carries_no_user_text() {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let case = case::create(root.path(), "Tax matter", Some("Private notes."))
        .unwrap()
        .data
        .case;
    crate::records::submission::add(
        root.path(),
        &case.id,
        "Posted the completed form.",
        Some("2026-01-20"),
        &[],
    )
    .unwrap();
    crate::records::submission::add(root.path(), &case.id, "Sent the evidence.", None, &[])
        .unwrap();

    let summary = status(root.path(), Some("2026-02-10")).unwrap().data;
    assert_eq!(summary.cases, 1);
    assert_eq!(summary.submissions, 2);
    assert_eq!(summary.undated_submissions, 1);
    assert_eq!(summary.receipts_to_retrieve.len(), 1);
    assert_eq!(summary.receipts_to_retrieve[0].case_id, case.id);

    let json = serde_json::to_string(&summary).unwrap();
    for text in ["Tax matter", "Private notes.", "Posted the completed form."] {
        assert!(!json.contains(text), "no user text reaches the summary");
    }
}

#[test]
fn the_summary_counts_the_objects_the_store_holds() {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let first = inputs.path().join("alpha.txt");
    let second = inputs.path().join("beta.txt");
    std::fs::write(&first, b"synthetic alpha\n").unwrap();
    std::fs::write(&second, b"synthetic beta\n").unwrap();
    crate::archive::import::import(root.path(), &[first, second]).unwrap();
    assert_eq!(status(root.path(), None).unwrap().data.stored_objects, 2);
}

#[test]
fn a_held_writer_lock_never_stops_the_summary() {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let lock = crate::archive::lock::WriterLock::acquire(root.path()).unwrap();
    let summary = status(root.path(), Some("2026-02-10"));
    assert!(summary.is_ok(), "the summary reads and never waits");
    drop(lock);
}

#[test]
fn an_archive_that_cannot_be_opened_is_a_refusal() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        status(root.path(), Some("2026-02-10"))
            .unwrap_err()
            .error
            .code,
        codes::ARCHIVE_MARKER_MISSING
    );
}

#[test]
fn the_as_of_is_checked_before_the_archive_is_opened() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        status(root.path(), Some("not a date"))
            .unwrap_err()
            .error
            .code,
        codes::USAGE_ARGUMENTS,
        "an unusable argument is refused before anything is read"
    );
}
