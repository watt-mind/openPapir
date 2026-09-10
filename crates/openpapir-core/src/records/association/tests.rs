//! The association record's own tests, one directory down so that the record
//! and the evidence for it each stay well inside the repository's
//! file-length check.

use super::*;
use crate::archive;
use crate::records::{case, receipt, submission};
use std::fs;
use std::path::PathBuf;

const PAYLOAD: &[u8] = b"synthetic bytes\n";
const PAYLOAD_DIGEST: &str =
    "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
const ABSENT_ID: &str = "0123456789abcdef0123456789abcdef";

/// An archive holding one receipt and two submissions.
struct Fixture {
    root: tempfile::TempDir,
    receipt_id: String,
    first: String,
    second: String,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let file = inputs.path().join("note.txt");
    fs::write(&file, PAYLOAD).unwrap();
    archive::import::import(root.path(), &[PathBuf::from(&file)]).unwrap();
    let case_id = case::create(root.path(), "Case", None)
        .unwrap()
        .data
        .case
        .id;
    let first = submission::add(root.path(), &case_id, "First", None, &[])
        .unwrap()
        .data
        .submission
        .id;
    let second = submission::add(root.path(), &case_id, "Second", None, &[])
        .unwrap()
        .data
        .submission
        .id;
    let receipt_id = receipt::add(root.path(), PAYLOAD_DIGEST, None, None)
        .unwrap()
        .data
        .receipt
        .id;
    Fixture {
        root,
        receipt_id,
        first,
        second,
    }
}

fn candidate(submission_id: &str, confidence: &str) -> String {
    format!("{submission_id}:{confidence}:The user stated a link.")
}

#[test]
fn every_outcome_is_recorded_and_none_is_an_error() {
    let f = fixture();
    let unassociated = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap()
        .data
        .association;
    assert_eq!(unassociated.outcome, "unassociated");
    assert_eq!(unassociated.submission_id, None);
    assert!(unassociated.candidates.is_empty());
    assert_eq!(unassociated.created_by, "user");
    assert_eq!(unassociated.supersedes, None);
    assert_eq!(unassociated.record_kind, KIND);
    assert_eq!(unassociated.archive_schema_version, 1);

    let candidates = [candidate(&f.first, "weak")];
    let one = create(f.root.path(), &f.receipt_id, "candidate", &candidates, None)
        .unwrap()
        .data
        .association;
    assert_eq!(one.candidates.len(), 1);
    assert_eq!(one.submission_id, None, "only associated names one");
    assert_eq!(one.candidates[0].confidence, "weak");
    assert_eq!(one.candidates[0].evidence[0].kind, "user_assertion");
    assert_eq!(one.candidates[0].evidence[0].source, "user");
    assert_eq!(
        one.candidates[0].evidence[0].statement,
        "The user stated a link."
    );

    let associated = create(
        f.root.path(),
        &f.receipt_id,
        "associated",
        &candidates,
        None,
    )
    .unwrap()
    .data
    .association;
    assert_eq!(associated.submission_id.as_deref(), Some(f.first.as_str()));

    let both = [
        candidate(&f.first, "moderate"),
        candidate(&f.second, "strong"),
    ];
    let contradictory = create(f.root.path(), &f.receipt_id, "contradictory", &both, None)
        .unwrap()
        .data
        .association;
    assert_eq!(contradictory.candidates.len(), 2);
    assert_eq!(contradictory.submission_id, None);

    let history = list(f.root.path(), &f.receipt_id).unwrap().data;
    assert_eq!(history.count, 4, "every record is retained");
    assert_eq!(history.receipt_id, f.receipt_id);
}

#[test]
fn a_stored_association_keeps_the_null_fields_the_contract_names() {
    let f = fixture();
    let association = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap()
        .data
        .association;
    let stored = fs::read_to_string(
        f.root
            .path()
            .join(ASSOCIATIONS_DIR)
            .join(format!("{}.json", association.id)),
    )
    .unwrap();
    assert!(stored.ends_with("}\n"), "one LF-terminated document");
    let value: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert!(value["submission_id"].is_null());
    assert!(value["supersedes"].is_null());
    assert_eq!(value["created_by"], "user");
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "keys are stored sorted");
}

#[test]
fn every_consistency_rule_names_itself_and_writes_nothing() {
    let f = fixture();
    let one = [candidate(&f.first, "weak")];
    let two = [candidate(&f.first, "weak"), candidate(&f.second, "weak")];
    let duplicate = [candidate(&f.first, "weak"), candidate(&f.first, "strong")];
    for (outcome, candidates, rule) in [
        ("unassociated", &one[..], rules::UNASSOCIATED_HAS_CANDIDATES),
        ("candidate", &[][..], rules::CANDIDATE_REQUIRES_CANDIDATES),
        (
            "associated",
            &[][..],
            rules::ASSOCIATED_REQUIRES_ONE_CANDIDATE,
        ),
        (
            "associated",
            &two[..],
            rules::ASSOCIATED_REQUIRES_ONE_CANDIDATE,
        ),
        (
            "contradictory",
            &one[..],
            rules::CONTRADICTORY_REQUIRES_TWO_CANDIDATES,
        ),
        (
            "contradictory",
            &[][..],
            rules::CONTRADICTORY_REQUIRES_TWO_CANDIDATES,
        ),
        (
            "contradictory",
            &duplicate[..],
            rules::DUPLICATE_CANDIDATE_SUBMISSION,
        ),
    ] {
        let refusal = create(f.root.path(), &f.receipt_id, outcome, candidates, None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_INCONSISTENT, "{rule}");
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "association");
        assert_eq!(json["details"]["rule"], rule);
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
    }
    assert_eq!(
        fs::read_dir(f.root.path().join(ASSOCIATIONS_DIR))
            .unwrap()
            .count(),
        0,
        "a rule violation writes nothing"
    );
}

#[test]
fn a_supersession_chain_lists_newest_first_and_changes_nothing() {
    let f = fixture();
    let first = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap()
        .data
        .association;
    let path = f
        .root
        .path()
        .join(ASSOCIATIONS_DIR)
        .join(format!("{}.json", first.id));
    let before = fs::read_to_string(&path).unwrap();
    let second = create(
        f.root.path(),
        &f.receipt_id,
        "candidate",
        &[candidate(&f.first, "moderate")],
        Some(&first.id),
    )
    .unwrap()
    .data
    .association;
    assert_eq!(second.supersedes.as_deref(), Some(first.id.as_str()));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        before,
        "a superseded record is never modified"
    );

    let history = list(f.root.path(), &f.receipt_id).unwrap().data;
    assert_eq!(history.count, 2);
    assert_eq!(history.associations[0].id, second.id, "newest first");
    assert_eq!(history.associations[1].id, first.id);
    assert_eq!(history.associations[1].supersedes, None);
}

#[test]
fn supersedes_may_not_reach_another_receipts_history() {
    let f = fixture();
    let other = receipt::add(f.root.path(), PAYLOAD_DIGEST, None, Some("Second"))
        .unwrap()
        .data
        .receipt
        .id;
    let elsewhere = create(f.root.path(), &other, "unassociated", &[], None)
        .unwrap()
        .data
        .association;
    let refusal = create(
        f.root.path(),
        &f.receipt_id,
        "unassociated",
        &[],
        Some(&elsewhere.id),
    )
    .unwrap_err()
    .error;
    assert_eq!(refusal.code, codes::RECORD_INCONSISTENT);
    assert_eq!(
        serde_json::to_value(&refusal).unwrap()["details"]["rule"],
        "supersedes_other_receipt"
    );
    assert_eq!(
        list(f.root.path(), &f.receipt_id).unwrap().data.count,
        0,
        "the refusal wrote nothing"
    );
}

#[test]
fn a_missing_receipt_submission_or_superseded_record_is_refused() {
    let f = fixture();
    for (receipt_id, candidates, supersedes, kind, reference) in [
        (ABSENT_ID, &[][..], None, "receipt", "receipt_id"),
        (
            f.receipt_id.as_str(),
            &[candidate(ABSENT_ID, "weak")][..],
            None,
            "submission",
            "submission_id",
        ),
        (
            f.receipt_id.as_str(),
            &[][..],
            Some(ABSENT_ID),
            "association",
            "association_id",
        ),
        (
            f.receipt_id.as_str(),
            &[][..],
            Some(""),
            "association",
            "association_id",
        ),
    ] {
        let outcome = if candidates.is_empty() {
            "unassociated"
        } else {
            "candidate"
        };
        let refusal = create(f.root.path(), receipt_id, outcome, candidates, supersedes)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], kind);
        assert_eq!(json["details"]["reference_kind"], reference);
    }
    let refusal = list(f.root.path(), ABSENT_ID).unwrap_err().error;
    assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
}

#[test]
fn an_unusable_outcome_confidence_or_candidate_is_a_usage_refusal() {
    let f = fixture();
    let refusal = create(f.root.path(), &f.receipt_id, "delivered", &[], None)
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
    assert_eq!(
        serde_json::to_value(&refusal).unwrap()["details"]["argument"],
        "outcome"
    );

    for entry in [
        format!("{}:certain:Statement", f.first),
        format!("{}:weak", f.first),
        f.first.clone(),
        "../../etc/passwd:weak:Statement".to_owned(),
        format!("{}:0.9:Statement", f.first),
    ] {
        let refusal = create(f.root.path(), &f.receipt_id, "candidate", &[entry], None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        assert_eq!(refusal.exit_code(), 2);
        let rendered = serde_json::to_string(&refusal).unwrap();
        assert!(!rendered.contains("passwd"), "no value is echoed");
    }
}

#[test]
fn a_statement_keeps_its_colons_and_its_cap() {
    let f = fixture();
    let association = create(
        f.root.path(),
        &f.receipt_id,
        "candidate",
        &[format!("{}:strong:Reference: 12:34 on the page.", f.first)],
        None,
    )
    .unwrap()
    .data
    .association;
    assert_eq!(
        association.candidates[0].evidence[0].statement,
        "Reference: 12:34 on the page."
    );

    for statement in ["s".repeat(513), String::new(), "one\ntwo".to_owned()] {
        let refusal = create(
            f.root.path(),
            &f.receipt_id,
            "candidate",
            &[format!("{}:weak:{statement}", f.first)],
            None,
        )
        .unwrap_err()
        .error;
        assert!(
            refusal.code == codes::INPUT_CAP_FIELD_LENGTH || refusal.code == codes::USAGE_ARGUMENTS
        );
        let json = serde_json::to_value(&refusal).unwrap();
        let named = json["details"]["field"]
            .as_str()
            .or(json["details"]["argument"].as_str());
        assert_eq!(named, Some("statement"));
    }
}

#[test]
fn creating_needs_the_writer_lock_and_listing_needs_none() {
    let f = fixture();
    let _held = WriterLock::acquire(f.root.path()).unwrap();
    let refusal = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::LOCK_HELD);
    assert!(refusal.is_retryable());
    assert_eq!(list(f.root.path(), &f.receipt_id).unwrap().data.count, 0);
}

#[test]
fn a_malformed_association_document_is_reported_rather_than_skipped() {
    let f = fixture();
    fs::write(
        f.root
            .path()
            .join(ASSOCIATIONS_DIR)
            .join("ffffffffffffffffffffffffffffffff.json"),
        b"{ not a record",
    )
    .unwrap();
    let refusal = list(f.root.path(), &f.receipt_id).unwrap_err().error;
    assert_eq!(refusal.code, codes::RECORD_MALFORMED);
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["record_kind"], "association");
    assert_eq!(json["details"]["path_count"], 1);
}

#[test]
fn a_retirement_supersedes_the_record_and_changes_nothing_it_names() {
    let f = fixture();
    let asserted = create(
        f.root.path(),
        &f.receipt_id,
        "candidate",
        &[candidate(&f.first, "strong")],
        None,
    )
    .unwrap()
    .data
    .association;
    let path = f
        .root
        .path()
        .join(ASSOCIATIONS_DIR)
        .join(format!("{}.json", asserted.id));
    let before = fs::read_to_string(&path).unwrap();

    let retired = retire(
        f.root.path(),
        &asserted.id,
        Some("The user withdrew the statement."),
    )
    .unwrap()
    .data
    .association;
    assert_eq!(retired.outcome, "unassociated");
    assert!(retired.candidates.is_empty(), "a retirement claims nothing");
    assert_eq!(retired.submission_id, None);
    assert_eq!(retired.supersedes.as_deref(), Some(asserted.id.as_str()));
    assert_eq!(retired.receipt_id, asserted.receipt_id);
    assert_eq!(retired.created_by, "user");
    assert_eq!(
        retired.statement.as_deref(),
        Some("The user withdrew the statement."),
        "the reason is stored as the user's own statement"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        before,
        "the retired record is never modified"
    );

    let history = list(f.root.path(), &f.receipt_id).unwrap().data;
    assert_eq!(history.count, 2, "both records are retained");
    assert_eq!(history.associations[0].id, retired.id, "newest first");
}

#[test]
fn a_retirement_without_a_reason_stores_no_statement() {
    let f = fixture();
    let asserted = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap()
        .data
        .association;
    let retired = retire(f.root.path(), &asserted.id, None)
        .unwrap()
        .data
        .association;
    assert_eq!(retired.statement, None);
    let stored = fs::read_to_string(
        f.root
            .path()
            .join(ASSOCIATIONS_DIR)
            .join(format!("{}.json", retired.id)),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert!(
        value.get("statement").is_none(),
        "an absent optional field is not written at all"
    );
    assert!(value["submission_id"].is_null());
}

#[test]
fn a_record_that_is_superseded_already_may_not_be_retired_twice() {
    let f = fixture();
    let asserted = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap()
        .data
        .association;
    retire(f.root.path(), &asserted.id, None).unwrap();
    let refusal = retire(f.root.path(), &asserted.id, None).unwrap_err().error;
    assert_eq!(refusal.code, codes::RECORD_INCONSISTENT);
    assert_eq!(refusal.exit_code(), 4);
    assert!(!refusal.is_retryable());
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["record_kind"], "association");
    assert_eq!(json["details"]["rule"], "already_superseded");
    assert_eq!(
        list(f.root.path(), &f.receipt_id).unwrap().data.count,
        2,
        "the refusal wrote nothing"
    );
}

#[test]
fn retiring_needs_a_stored_record_a_usable_reason_and_the_writer_lock() {
    let f = fixture();
    let asserted = create(f.root.path(), &f.receipt_id, "unassociated", &[], None)
        .unwrap()
        .data
        .association;

    let absent = retire(f.root.path(), ABSENT_ID, None).unwrap_err().error;
    assert_eq!(absent.code, codes::RECORD_NOT_FOUND);
    let json = serde_json::to_value(&absent).unwrap();
    assert_eq!(json["details"]["record_kind"], "association");
    assert_eq!(json["details"]["reference_kind"], "association_id");

    let long = "s".repeat(513);
    let oversized = retire(f.root.path(), &asserted.id, Some(&long))
        .unwrap_err()
        .error;
    assert_eq!(oversized.code, codes::INPUT_CAP_FIELD_LENGTH);
    let json = serde_json::to_value(&oversized).unwrap();
    assert_eq!(json["details"]["field"], "statement");
    let rendered = serde_json::to_string(&oversized).unwrap();
    assert!(!rendered.contains("ssss"), "no reason is echoed");

    let _held = WriterLock::acquire(f.root.path()).unwrap();
    let refusal = retire(f.root.path(), &asserted.id, None).unwrap_err().error;
    assert_eq!(refusal.code, codes::LOCK_HELD);
    assert!(refusal.is_retryable());
    assert_eq!(
        list(f.root.path(), &f.receipt_id).unwrap().data.count,
        1,
        "no refusal wrote a record"
    );
}
