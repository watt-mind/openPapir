//! The scan pass: what a deletion would remove, decided before anything goes.
//!
//! The pass reads every record of every kind through the bounded, no-follow
//! reader the record commands already use. A document that cannot be read as
//! a record of its kind makes the whole scan `record.malformed`, so a
//! deletion aborts before it has unlinked anything rather than part way
//! through (`docs/archive-layout.md`).
//!
//! Nothing here touches the filesystem beyond reading. The plan it returns is
//! a list of identifiers openPapir minted itself and a list of validated
//! digests, and the apply pass joins nothing else into a path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::archive::import::ImportEvent;
use crate::error::{Details, Diagnostic, Warning, codes};
use crate::records::association::Association;
use crate::records::case::Case;
use crate::records::document;
use crate::records::receipt::Receipt;
use crate::records::submission::Submission;
use crate::records::{DIGEST_PREFIX, is_digest};

/// The record kinds a deletion reports, ordered by name.
pub const KINDS: [&str; 5] = [
    "association",
    "case",
    "import_event",
    "receipt",
    "submission",
];

/// The reason an object the purge planned to unlink could not be unlinked.
pub const UNREMOVABLE: &str = "unremovable";

/// Why an object the purge considered is still in the store, ordered by name.
pub const REASONS: [&str; 4] = [
    "purge_not_requested",
    "records_retained",
    "referenced_elsewhere",
    UNREMOVABLE,
];

/// What a deletion will remove, and what it will leave behind.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The case's own identifier.
    pub case: String,
    /// The identifiers of the submissions belonging to the case.
    pub submissions: Vec<String>,
    /// The identifiers of the receipts that go with them.
    pub receipts: Vec<String>,
    /// The identifiers of the associations that go with them.
    pub associations: Vec<String>,
    /// The import events naming each object being purged, by that object's
    /// digest. An event goes with the object it names, so it goes only when
    /// that object actually went.
    pub import_events: BTreeMap<String, Vec<String>>,
    /// The bare, validated digests of the objects to unlink.
    pub objects: Vec<String>,
    /// Objects a remaining submission or receipt still references.
    pub referenced_elsewhere: u64,
    /// Objects that would become unreferenced, left because no purge was asked
    /// for.
    pub purge_not_requested: u64,
    /// How many references on records this deletion keeps are not digests at
    /// all. Any at all retains every candidate object a purge would otherwise
    /// have unlinked, because such a reference names something unknown and
    /// the unknown may be any of them.
    pub malformed_references: u64,
    /// How many candidate objects those references alone held back. It is
    /// above zero only when `--purge` was given and this case had a candidate
    /// the purge would otherwise have unlinked, which is exactly when a
    /// malformed reference changed what this deletion did.
    pub malformed_withheld: u64,
}

/// The bare hexadecimal digest of a reference, when it is one at all.
///
/// A record field is free text until it is checked. Only a value of the
/// documented form reaches the plan, so nothing else is ever joined into an
/// object path. `None` says only that the value is not a path this archive
/// can resolve; on a record the deletion keeps, it never says the record
/// references nothing, and [`plan_objects`] counts it instead.
fn hex(value: &str) -> Option<&str> {
    value
        .strip_prefix(DIGEST_PREFIX)
        .filter(|hex| is_digest(hex))
}

/// Read every record and decide what this deletion removes.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no case, and
/// `record.malformed` when any stored document cannot be read as a record of
/// its kind. Both are returned before anything has been removed.
pub fn build(root: &Path, case_id: &str, purge: bool) -> Result<Plan, Diagnostic> {
    let case = document::read_record::<Case>(root, case_id, "case_id")?;
    let submissions = document::list_records::<Submission>(root)?;
    let receipts = document::list_records::<Receipt>(root)?;
    let associations = document::list_records::<Association>(root)?;
    let events = document::list_records::<ImportEvent>(root)?;

    let going: BTreeSet<&str> = submissions
        .iter()
        .filter(|submission| submission.case_id == case.id)
        .map(|submission| submission.id.as_str())
        .collect();
    let going_associations = doomed_associations(&associations, &going);
    refuse_entangled(&associations, &going, &going_associations)?;
    let going_receipts = doomed_receipts(&receipts, &associations, &going_associations);

    let mut plan = Plan {
        case: case.id.clone(),
        submissions: going.iter().map(|id| (*id).to_owned()).collect(),
        receipts: going_receipts.iter().map(|id| (*id).to_owned()).collect(),
        associations: going_associations
            .iter()
            .map(|id| (*id).to_owned())
            .collect(),
        ..Plan::default()
    };
    plan_objects(
        &mut plan,
        &submissions,
        &receipts,
        &going,
        &going_receipts,
        purge,
    );
    let purged: BTreeSet<&str> = plan.objects.iter().map(String::as_str).collect();
    for event in &events {
        if let Some(hex) = hex(&event.digest).filter(|hex| purged.contains(hex)) {
            plan.import_events
                .entry(hex.to_owned())
                .or_default()
                .push(event.id.clone());
        }
    }
    Ok(plan)
}

/// Refuse a deletion that would leave a surviving record naming a removed one.
///
/// An association may name submissions in more than one case. If one of them
/// is going and another remains, the association itself has to stay, because
/// it still references a submission this deletion leaves behind, and it would
/// then name a submission the archive no longer holds: exactly the dangling
/// reference the integrity check reports.
///
/// openPapir cannot edit a stored record, so it cannot drop the departing
/// candidate and keep the rest. Removing the association instead would delete
/// the user's own assertion about a case they did not ask to delete. The
/// deletion is therefore refused, before anything is touched, and the user is
/// told how many records stand in the way and of what kind. Nothing is lost,
/// and the refusal is the only one of the three outcomes that can be undone.
///
/// # Errors
///
/// Returns `delete.record_entangled`, carrying the kind and the count and
/// never an identifier.
fn refuse_entangled(
    associations: &[Association],
    going: &BTreeSet<&str>,
    going_associations: &BTreeSet<&str>,
) -> Result<(), Diagnostic> {
    let entangled = associations
        .iter()
        .filter(|association| !going_associations.contains(association.id.as_str()))
        .filter(|association| named(association).any(|id| going.contains(id)))
        .count() as u64;
    if entangled == 0 {
        return Ok(());
    }
    Err(Diagnostic::new(
        codes::DELETE_RECORD_ENTANGLED,
        "A record this deletion must keep names a submission it would remove.",
        Details::new()
            .text("record_kind", "association")
            .int("retained_count", entangled),
    ))
}

/// Every submission an association names, confirmed or as a candidate.
fn named(association: &Association) -> impl Iterator<Item = &str> {
    association.submission_id.as_deref().into_iter().chain(
        association
            .candidates
            .iter()
            .map(|candidate| candidate.submission_id.as_str()),
    )
}

/// The associations that reference no submission this deletion leaves behind.
///
/// An association goes when every submission it names is going and it names
/// at least one. One that names a submission belonging to another case
/// references something that remains, so it stays, and so does one that names
/// no submission at all.
///
/// A retained association may supersede one that would otherwise go. Removing
/// the older record would leave the newer one naming a record the archive no
/// longer holds, which the integrity check reports as a dangling reference,
/// so the older record is kept as well. The rule is applied until it settles,
/// because keeping one record can require keeping the record it supersedes.
fn doomed_associations<'a>(
    associations: &'a [Association],
    going: &BTreeSet<&str>,
) -> BTreeSet<&'a str> {
    let mut doomed: BTreeSet<&str> = associations
        .iter()
        .filter(|association| {
            let mut named_any = false;
            let all_going = named(association).all(|id| {
                named_any = true;
                going.contains(id)
            });
            named_any && all_going
        })
        .map(|association| association.id.as_str())
        .collect();
    loop {
        let rescued: Vec<&str> = associations
            .iter()
            .filter(|association| !doomed.contains(association.id.as_str()))
            .filter_map(|association| association.supersedes.as_deref())
            .filter(|superseded| doomed.contains(superseded))
            .collect();
        if rescued.is_empty() {
            return doomed;
        }
        for id in rescued {
            doomed.remove(id);
        }
    }
}

/// The receipts that reference no submission or case this deletion leaves.
///
/// A receipt is its own record, so it goes only when an association tied it to
/// a submission that is going and no remaining association still names it. A
/// receipt no association names is not tied to this case and stays.
fn doomed_receipts<'a>(
    receipts: &'a [Receipt],
    associations: &[Association],
    doomed_associations: &BTreeSet<&str>,
) -> BTreeSet<&'a str> {
    receipts
        .iter()
        .filter(|receipt| {
            let mut tied = false;
            for association in associations
                .iter()
                .filter(|association| association.receipt_id == receipt.id)
            {
                if doomed_associations.contains(association.id.as_str()) {
                    tied = true;
                } else {
                    return false;
                }
            }
            tied
        })
        .map(|receipt| receipt.id.as_str())
        .collect()
}

/// Split the objects the departing records name into purged and retained.
///
/// An object is a candidate only because a record that is going named it.
/// It survives when a remaining submission or receipt still names it, and it
/// survives without a purge whatever else is true, because objects go only by
/// an explicit purge (`docs/archive-layout.md`).
///
/// The reasons are tested in that order, so each candidate is reported under
/// the reason that actually decided it. A reference no remaining record
/// resolves and no purge asked about is `purge_not_requested`, whatever else
/// the archive holds; `referenced_elsewhere` is reserved for a candidate a
/// purge would otherwise have removed, either because a surviving record
/// names it or because a surviving record names something the archive cannot
/// identify and that unknown may be this one.
fn plan_objects<'a>(
    plan: &mut Plan,
    submissions: &'a [Submission],
    receipts: &'a [Receipt],
    going: &BTreeSet<&str>,
    going_receipts: &BTreeSet<&str>,
    purge: bool,
) {
    let mut references = References::default();
    for submission in submissions {
        let surviving = !going.contains(submission.id.as_str());
        for artefact in &submission.artefacts {
            references.record(plan, surviving, &artefact.digest);
        }
    }
    for receipt in receipts {
        let surviving = !going_receipts.contains(receipt.id.as_str());
        references.record(plan, surviving, &receipt.artefact_digest);
    }
    // A reference that is not a digest names something this archive cannot
    // resolve, and the deletion cannot tell which object it meant. Every
    // candidate may be the one, so a purge unlinks none of them.
    let unknown = plan.malformed_references > 0;
    for digest in references.candidates {
        if references.remaining.contains(digest) {
            plan.referenced_elsewhere += 1;
        } else if !purge {
            plan.purge_not_requested += 1;
        } else if unknown {
            plan.referenced_elsewhere += 1;
            plan.malformed_withheld += 1;
        } else {
            plan.objects.push(digest.to_owned());
        }
    }
}

/// The digests the scan saw, split by whether the record naming one is going.
#[derive(Debug, Default)]
struct References<'a> {
    /// Digests only records this deletion removes reference.
    candidates: BTreeSet<&'a str>,
    /// Digests a record this deletion keeps still references.
    remaining: BTreeSet<&'a str>,
}

impl<'a> References<'a> {
    /// File one stored reference, or count one that is not a digest at all.
    ///
    /// A malformed reference on a record that is going says nothing: the
    /// record and its claim both leave, and no object was ever a candidate
    /// because of it. On a record that survives it is the opposite, and the
    /// asymmetry is the whole point: the record still asserts that it
    /// references some artefact, so it is counted as a reference to an
    /// unknown object rather than silently dropped as a reference to none.
    fn record(&mut self, plan: &mut Plan, surviving: bool, value: &'a str) {
        match (hex(value), surviving) {
            (Some(hex), true) => {
                self.remaining.insert(hex);
            }
            (Some(hex), false) => {
                self.candidates.insert(hex);
            }
            (None, true) => plan.malformed_references += 1,
            (None, false) => {}
        }
    }
}

/// The degradation reported when a surviving record's reference is not a
/// digest.
///
/// It is a warning rather than a refusal: the deletion still removes every
/// record it planned to remove, and only the purge is held back. The count is
/// the whole of it, exactly as every other deletion report: naming the record
/// would say which surviving record points at content the user asked to
/// purge, and naming the value would echo a stored field
/// (`docs/error-contract.md`).
///
/// The scan reads the whole archive, so such a reference may sit on a record
/// that has nothing to do with this case. It is reported only when it held an
/// object of this deletion back, and its text says so, because a warning on
/// every later deletion would describe the archive rather than the command
/// the user ran. Finding one wherever it sits is `archive check`'s work.
#[must_use]
pub fn malformed_references(malformed_count: u64) -> Warning {
    Diagnostic::new(
        codes::RECORD_MALFORMED,
        "A record this deletion keeps references an artefact by something that is not a digest, so no object was purged for this case.",
        Details::new()
            .text("stage", "delete")
            .int("malformed_count", malformed_count),
    )
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_malformed_reference_is_reported_as_a_count_and_nothing_else() {
        let warning = malformed_references(2);
        assert_eq!(warning.code, codes::RECORD_MALFORMED);
        assert!(!warning.is_retryable());
        let json = serde_json::to_value(&warning).unwrap();
        assert_eq!(json["details"]["malformed_count"], 2);
        assert_eq!(json["details"]["stage"], "delete");
        assert_eq!(json["details"]["bucket"], "record");
        assert_eq!(
            json["details"].as_object().unwrap().len(),
            3,
            "the count, the stage, and the bucket, and never the value itself"
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
}
