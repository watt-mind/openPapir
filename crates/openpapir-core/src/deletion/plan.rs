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
use crate::error::Diagnostic;
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
pub const REASONS: [&str; 3] = ["purge_not_requested", "referenced_elsewhere", UNREMOVABLE];

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
}

/// The bare hexadecimal digest of a reference, when it is one at all.
///
/// A record field is free text until it is checked. Only a value of the
/// documented form reaches the plan, so nothing else is ever joined into an
/// object path.
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
fn plan_objects(
    plan: &mut Plan,
    submissions: &[Submission],
    receipts: &[Receipt],
    going: &BTreeSet<&str>,
    going_receipts: &BTreeSet<&str>,
    purge: bool,
) {
    let mut candidates: BTreeSet<&str> = BTreeSet::new();
    let mut remaining: BTreeSet<&str> = BTreeSet::new();
    for submission in submissions {
        let target = if going.contains(submission.id.as_str()) {
            &mut candidates
        } else {
            &mut remaining
        };
        for artefact in &submission.artefacts {
            if let Some(hex) = hex(&artefact.digest) {
                target.insert(hex);
            }
        }
    }
    for receipt in receipts {
        let target = if going_receipts.contains(receipt.id.as_str()) {
            &mut candidates
        } else {
            &mut remaining
        };
        if let Some(hex) = hex(&receipt.artefact_digest) {
            target.insert(hex);
        }
    }
    for digest in candidates {
        if remaining.contains(digest) {
            plan.referenced_elsewhere += 1;
        } else if purge {
            plan.objects.push(digest.to_owned());
        } else {
            plan.purge_not_requested += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::association::{Candidate, Evidence};

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

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

    #[test]
    fn only_a_reference_of_the_documented_form_reaches_a_path() {
        assert_eq!(hex(&format!("sha256:{DIGEST}")), Some(DIGEST));
        assert_eq!(hex(DIGEST), None, "the prefix is required");
        assert_eq!(hex("sha256:../../etc/passwd"), None);
        assert_eq!(hex("sha256:"), None);
        assert_eq!(KINDS.len(), 5);
        assert_eq!(REASONS.len(), 3);
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
