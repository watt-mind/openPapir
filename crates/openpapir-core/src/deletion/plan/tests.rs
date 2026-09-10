//! The scan pass's own tests, one directory down so that the pass and
//! the evidence for it each stay well inside the repository's
//! file-length check.

use super::*;
use crate::records::association::{Candidate, Evidence};
use crate::records::submission::ArtefactRef;

const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
const OTHER: &str = "b113fe1606d66a616548df8650a82a022c814484bee3c60af536fd168e742725";

fn id(seed: u8) -> String {
    format!("{seed:02x}").repeat(16)
}

fn association(seed: u8, receipt: u8, submissions: &[u8]) -> Association {
    Association {
        archive_schema_version: 1,
        candidates: submissions
            .iter()
            .map(|submission| Candidate {
                confidence: "weak".to_owned(),
                evidence: vec![Evidence {
                    kind: "user_assertion".to_owned(),
                    source: "user".to_owned(),
                    statement: "The user stated a link.".to_owned(),
                }],
                submission_id: id(*submission),
            })
            .collect(),
        created_at: "2026-01-17T12:00:00Z".to_owned(),
        created_by: "user".to_owned(),
        id: id(seed),
        outcome: "candidate".to_owned(),
        receipt_id: id(receipt),
        record_kind: "association".to_owned(),
        statement: None,
        submission_id: None,
        supersedes: None,
    }
}

fn receipt(seed: u8, digest: &str) -> Receipt {
    Receipt {
        archive_schema_version: 1,
        artefact_digest: digest.to_owned(),
        created_at: "2026-01-16T11:00:00Z".to_owned(),
        id: id(seed),
        import_event_id: id(0xee),
        label: None,
        record_kind: "receipt".to_owned(),
    }
}

fn submission(seed: u8, case: u8, digests: &[&str]) -> Submission {
    Submission {
        archive_schema_version: 1,
        artefacts: digests
            .iter()
            .map(|digest| ArtefactRef {
                digest: (*digest).to_owned(),
                role: None,
            })
            .collect(),
        case_id: id(case),
        created_at: "2026-01-15T10:00:00Z".to_owned(),
        description: "The user states they sent this.".to_owned(),
        id: id(seed),
        record_kind: "submission".to_owned(),
        stated_date: None,
    }
}

/// The set of digests one plan would unlink, for a scan of these records.
fn objects(submissions: &[Submission], receipts: &[Receipt], purge: bool) -> Plan {
    let going = BTreeSet::from([submissions[0].id.as_str()]);
    let going_receipts = BTreeSet::new();
    let mut plan = Plan::default();
    plan_objects(
        &mut plan,
        submissions,
        receipts,
        &going,
        &going_receipts,
        purge,
    );
    plan
}

/// A reference that is not a digest at all, on a record that survives,
/// says the record points at something this archive cannot resolve. The
/// deletion cannot tell which object it meant, so no object may go.
#[test]
fn a_malformed_reference_on_a_surviving_record_retains_every_object() {
    let qualified = format!("sha256:{DIGEST}");
    let other = format!("sha256:{OTHER}");
    let submissions = vec![
        submission(1, 9, &[&qualified, &other]),
        submission(2, 8, &["not-a-digest"]),
    ];
    let plan = objects(&submissions, &[], true);
    assert_eq!(plan.malformed_references, 1);
    assert!(
        plan.objects.is_empty(),
        "a purge never proceeds past a reference it cannot resolve"
    );
    assert_eq!(plan.referenced_elsewhere, 2, "every candidate is retained");
    assert_eq!(plan.purge_not_requested, 0);
    assert_eq!(
        plan.malformed_withheld, 2,
        "both are held back by the reference alone"
    );
}

/// The same reference on a record that is going says nothing: the record
/// and its claim both leave, so nothing is retained for it.
#[test]
fn a_malformed_reference_on_a_departing_record_retains_nothing() {
    let qualified = format!("sha256:{DIGEST}");
    let submissions = vec![
        submission(1, 9, &[&qualified, "not-a-digest"]),
        submission(2, 8, &[]),
    ];
    let plan = objects(&submissions, &[], true);
    assert_eq!(plan.malformed_references, 0);
    assert_eq!(plan.objects, vec![DIGEST.to_owned()]);
    assert_eq!(plan.referenced_elsewhere, 0);
}

/// A surviving receipt reaches the same rule as a surviving submission.
#[test]
fn a_surviving_receipt_reaches_the_same_rule_as_a_surviving_submission() {
    let qualified = format!("sha256:{DIGEST}");
    let submissions = vec![submission(1, 9, &[&qualified]), submission(2, 8, &[])];
    let receipts = vec![receipt(0x30, "sha256:not a digest")];
    let plan = objects(&submissions, &receipts, true);
    assert_eq!(plan.malformed_references, 1);
    assert!(plan.objects.is_empty());
    assert_eq!(plan.referenced_elsewhere, 1);
    assert_eq!(plan.purge_not_requested, 0);
    assert_eq!(plan.malformed_withheld, 1);
}

/// Without a purge no object was going to be unlinked, so the reference
/// the archive cannot resolve decided nothing. The candidate keeps
/// `purge_not_requested`, the reason that actually held it, and nothing
/// is withheld for the warning to report.
#[test]
fn without_a_purge_a_malformed_reference_leaves_the_reason_unchanged() {
    let qualified = format!("sha256:{DIGEST}");
    let submissions = vec![submission(1, 9, &[&qualified]), submission(2, 8, &[])];
    let receipts = vec![receipt(0x30, "sha256:not a digest")];
    let plan = objects(&submissions, &receipts, false);
    assert_eq!(plan.malformed_references, 1);
    assert!(plan.objects.is_empty(), "no purge unlinks nothing");
    assert_eq!(plan.referenced_elsewhere, 0);
    assert_eq!(plan.purge_not_requested, 1);
    assert_eq!(plan.malformed_withheld, 0, "nothing was held back");
    let clean = objects(&submissions, &[], false);
    assert_eq!(
        clean.purge_not_requested, plan.purge_not_requested,
        "the same archive without the malformed reference reads the same"
    );
}

/// A candidate a surviving record names outright is `referenced_elsewhere`
/// with or without a purge: that record, not the absent purge, is why the
/// object stays.
#[test]
fn a_candidate_a_surviving_record_names_stays_referenced_elsewhere() {
    let qualified = format!("sha256:{DIGEST}");
    let submissions = vec![
        submission(1, 9, &[&qualified]),
        submission(2, 8, &[&qualified]),
    ];
    for purge in [true, false] {
        let plan = objects(&submissions, &[], purge);
        assert_eq!(plan.malformed_references, 0);
        assert!(plan.objects.is_empty());
        assert_eq!(plan.referenced_elsewhere, 1);
        assert_eq!(plan.purge_not_requested, 0);
        assert_eq!(plan.malformed_withheld, 0);
    }
}

/// A malformed reference with no candidate to hold back changed nothing,
/// so there is nothing for the warning to report.
#[test]
fn a_malformed_reference_with_no_candidate_withholds_nothing() {
    let submissions = vec![submission(1, 9, &[]), submission(2, 8, &["not-a-digest"])];
    let plan = objects(&submissions, &[], true);
    assert_eq!(plan.malformed_references, 1);
    assert_eq!(plan.malformed_withheld, 0);
    assert_eq!(plan.referenced_elsewhere, 0);
    assert_eq!(plan.purge_not_requested, 0);
}

/// The two counts answer different questions, so each is reported under
/// its own name and neither is derived from the other.
#[test]
fn a_malformed_reference_is_reported_as_two_counts_and_nothing_else() {
    let warning = malformed_references(2, 1);
    assert_eq!(warning.code, codes::RECORD_MALFORMED);
    assert!(!warning.is_retryable());
    let json = serde_json::to_value(&warning).unwrap();
    assert_eq!(
        json["details"]["malformed_count"], 2,
        "the references the whole scan read"
    );
    assert_eq!(
        json["details"]["withheld_count"], 1,
        "the candidate objects of this case they held back"
    );
    assert_eq!(json["details"]["stage"], "delete");
    assert_eq!(json["details"]["bucket"], "record");
    assert_eq!(
        json["details"].as_object().unwrap().len(),
        4,
        "the two counts, the stage, and the bucket, and never the value itself"
    );
}

#[test]
fn only_a_reference_of_the_documented_form_reaches_a_path() {
    assert_eq!(hex(&format!("sha256:{DIGEST}")), Some(DIGEST));
    assert_eq!(hex(DIGEST), None, "the prefix is required");
    assert_eq!(hex("sha256:../../etc/passwd"), None);
    assert_eq!(hex("sha256:"), None);
    assert_eq!(KINDS.len(), 5);
    assert_eq!(REASONS.len(), 4);
}

#[test]
fn an_association_stays_when_it_names_a_submission_that_remains() {
    let going = BTreeSet::from([id(1)]);
    let going: BTreeSet<&str> = going.iter().map(String::as_str).collect();
    let associations = vec![
        association(0x10, 0x20, &[1]),
        association(0x11, 0x21, &[1, 2]),
        association(0x12, 0x22, &[]),
    ];
    let doomed = doomed_associations(&associations, &going);
    let doomed: Vec<String> = doomed.iter().map(|id| (*id).to_owned()).collect();
    assert_eq!(doomed, vec![id(0x10)]);
}

#[test]
fn a_superseded_association_is_kept_when_the_newer_one_stays() {
    let going = BTreeSet::from([id(1)]);
    let going: BTreeSet<&str> = going.iter().map(String::as_str).collect();
    let mut newer = association(0x11, 0x20, &[1, 2]);
    newer.supersedes = Some(id(0x10));
    let associations = vec![association(0x10, 0x20, &[1]), newer];
    assert!(
        doomed_associations(&associations, &going).is_empty(),
        "removing it would leave the newer record naming nothing"
    );
}

#[test]
fn a_receipt_goes_only_when_every_association_naming_it_goes() {
    let associations = vec![association(0x10, 0x20, &[1]), association(0x11, 0x20, &[2])];
    let receipts = vec![
        receipt(0x20, &format!("sha256:{DIGEST}")),
        receipt(0x21, ""),
    ];
    let doomed = BTreeSet::from([associations[0].id.as_str()]);
    assert!(
        doomed_receipts(&receipts, &associations, &doomed).is_empty(),
        "one remaining association keeps the receipt"
    );
    let both = BTreeSet::from([associations[0].id.as_str(), associations[1].id.as_str()]);
    assert_eq!(
        doomed_receipts(&receipts, &associations, &both),
        BTreeSet::from([receipts[0].id.as_str()]),
        "a receipt no association names is not tied to this case"
    );
}

/// A retirement withdraws the assertion, so the chain behind it is history of
/// the case it was about and goes with it, whichever case the withdrawn
/// record named.
#[test]
fn a_retired_chain_goes_with_the_case_its_history_named() {
    let going = BTreeSet::from([id(1)]);
    let going: BTreeSet<&str> = going.iter().map(String::as_str).collect();
    let mut retirement = association(0x12, 0x20, &[]);
    retirement.supersedes = Some(id(0x11));
    let associations = vec![association(0x11, 0x20, &[1, 2]), retirement];
    let doomed = doomed_associations(&associations, &going);
    assert_eq!(
        doomed,
        BTreeSet::from([associations[0].id.as_str(), associations[1].id.as_str()]),
        "the whole chain goes, so neither record is left naming the other"
    );
    assert!(
        refuse_entangled(&associations, &going).is_ok(),
        "a withdrawn assertion is no longer in the way"
    );
}

/// The refusal counts the live records, which are the ones a retirement can
/// name, rather than the history behind them.
#[test]
fn a_live_association_spanning_two_cases_is_refused_by_a_count_of_live_records() {
    let going = BTreeSet::from([id(1)]);
    let going: BTreeSet<&str> = going.iter().map(String::as_str).collect();
    let mut newer = association(0x11, 0x20, &[1, 2]);
    newer.supersedes = Some(id(0x10));
    let associations = vec![association(0x10, 0x20, &[1]), newer];
    let refusal = refuse_entangled(&associations, &going).unwrap_err();
    assert_eq!(refusal.code, codes::DELETE_RECORD_ENTANGLED);
    assert_eq!(refusal.exit_code(), 4);
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["record_kind"], "association");
    assert_eq!(
        json["details"]["retained_count"], 1,
        "one live record, not the history behind it"
    );
    assert!(
        !serde_json::to_string(&refusal).unwrap().contains(&id(0x11)),
        "no identifier is echoed"
    );
}

/// A chain none of whose records names a departing submission is no business
/// of this deletion, and a chain that supersedes itself still settles.
#[test]
fn a_chain_this_deletion_does_not_touch_stays_and_a_cycle_settles() {
    let going = BTreeSet::from([id(1)]);
    let going: BTreeSet<&str> = going.iter().map(String::as_str).collect();
    let mut first = association(0x10, 0x20, &[2]);
    let mut second = association(0x11, 0x20, &[]);
    first.supersedes = Some(id(0x11));
    second.supersedes = Some(id(0x10));
    let associations = vec![first, second];
    assert!(doomed_associations(&associations, &going).is_empty());
    assert!(refuse_entangled(&associations, &going).is_ok());
}
