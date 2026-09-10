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
    refuse_entangled(&associations, &going)?;
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
/// An association may name submissions in more than one case. While the user
/// still asserts it, one of those submissions going and another remaining
/// leaves the record naming a submission the archive no longer holds: exactly
/// the dangling reference the integrity check reports.
///
/// openPapir cannot edit a stored record, so it cannot drop the departing
/// candidate and keep the rest. Removing the assertion instead would delete
/// the user's own statement about a case they did not ask to delete. The
/// deletion is therefore refused, before anything is touched, and the user is
/// told how many records stand in the way, of what kind, and what to do about
/// it: `association retire` withdraws the assertion, and the deletion then
/// takes the withdrawn history with the case. Nothing is lost, and the
/// refusal is the only one of the three outcomes that can be undone.
///
/// The count is of live records, the ones a retirement can name, so it counts
/// what the user has to act on rather than the history behind it.
///
/// # Errors
///
/// Returns `delete.record_entangled`, carrying the kind and the count and
/// never an identifier.
fn refuse_entangled(
    associations: &[Association],
    going: &BTreeSet<&str>,
) -> Result<(), Diagnostic> {
    let chains = Chains::of(associations);
    let states = chains.states(associations, going);
    let entangled = associations
        .iter()
        .filter(|association| chains.live.contains(association.id.as_str()))
        .filter(|association| states[&chains.chain[association.id.as_str()]].entangled())
        .count() as u64;
    if entangled == 0 {
        return Ok(());
    }
    Err(Diagnostic::new(
        codes::DELETE_RECORD_ENTANGLED,
        "A record this deletion must keep names a submission it would remove. Retire it first.",
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

/// What one supersession chain means to this deletion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct State {
    /// Whether any record in the chain names a departing submission.
    departing: bool,
    /// Whether the chain's live record still names a submission that remains.
    /// A superseded record naming one does not count: the user withdrew it.
    asserted: bool,
}

impl State {
    /// Whether the whole chain goes with the case.
    const fn doomed(self) -> bool {
        self.departing && !self.asserted
    }

    /// Whether the chain is the entanglement the deletion refuses.
    const fn entangled(self) -> bool {
        self.departing && self.asserted
    }
}

/// The supersession chains an archive's association records form.
///
/// Two records lie in one chain when either supersedes the other, followed
/// until it settles, so a chain holds a record, everything it supersedes, and
/// everything that supersedes it. The record no other record supersedes is
/// the chain's live one: it is what the user asserts today, and everything
/// behind it is history the archive keeps.
struct Chains<'a> {
    /// The chain each association lies in, named by the lowest position of
    /// any of its records.
    chain: BTreeMap<&'a str, usize>,
    /// The associations no other association supersedes.
    live: BTreeSet<&'a str>,
}

impl<'a> Chains<'a> {
    /// Read the chains out of the records, without following a reference that
    /// names no stored record.
    ///
    /// The merge repeats until a pass changes nothing, and every pass joins at
    /// least two chains, so a hand-edited archive holding a cycle settles
    /// instead of looping.
    fn of(associations: &'a [Association]) -> Self {
        let mut chain: BTreeMap<&str, usize> = associations
            .iter()
            .enumerate()
            .map(|(position, association)| (association.id.as_str(), position))
            .collect();
        let superseded: BTreeSet<&str> = associations
            .iter()
            .filter_map(|association| association.supersedes.as_deref())
            .collect();
        let live = associations
            .iter()
            .map(|association| association.id.as_str())
            .filter(|id| !superseded.contains(id))
            .collect();
        loop {
            let mut joined = false;
            for association in associations {
                let Some(previous) = association.supersedes.as_deref() else {
                    continue;
                };
                let (Some(&here), Some(&there)) =
                    (chain.get(association.id.as_str()), chain.get(previous))
                else {
                    continue;
                };
                if here == there {
                    continue;
                }
                let (kept, replaced) = (here.min(there), here.max(there));
                for position in chain.values_mut() {
                    if *position == replaced {
                        *position = kept;
                    }
                }
                joined = true;
            }
            if !joined {
                return Self { chain, live };
            }
        }
    }

    /// What each chain means to a deletion that removes `going`.
    fn states(
        &self,
        associations: &'a [Association],
        going: &BTreeSet<&str>,
    ) -> BTreeMap<usize, State> {
        let mut states: BTreeMap<usize, State> = BTreeMap::new();
        for association in associations {
            let id = association.id.as_str();
            let state = states.entry(self.chain[id]).or_default();
            for submission in named(association) {
                if going.contains(submission) {
                    state.departing = true;
                } else if self.live.contains(id) {
                    state.asserted = true;
                }
            }
        }
        states
    }
}

/// The associations that reference no submission this deletion leaves behind.
///
/// The unit is the supersession chain, not the single record, because a
/// record that supersedes another cannot go without it: removing the older
/// record alone would leave the newer one naming a record the archive no
/// longer holds, which the integrity check reports as a dangling reference.
///
/// A chain goes when one of its records names a departing submission and its
/// live record names none that remains. That is the ordinary case, where
/// every candidate the chain ever named belongs to the case being deleted,
/// and it is also what a retirement produces: the live record of a retired
/// chain asserts nothing at all, so the history behind it goes with the case
/// it was about. A chain whose live record still names a submission that
/// remains is refused instead, and a chain naming no departing submission is
/// no business of this deletion and stays.
fn doomed_associations<'a>(
    associations: &'a [Association],
    going: &BTreeSet<&str>,
) -> BTreeSet<&'a str> {
    let chains = Chains::of(associations);
    let states = chains.states(associations, going);
    associations
        .iter()
        .map(|association| association.id.as_str())
        .filter(|id| states[&chains.chain[id]].doomed())
        .collect()
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
/// record it planned to remove, and only the purge is held back. The counts
/// are the whole of it, exactly as every other deletion report: naming the
/// record would say which surviving record points at content the user asked
/// to purge, and naming the value would echo a stored field
/// (`docs/error-contract.md`).
///
/// The scan reads the whole archive, so such a reference may sit on a record
/// that has nothing to do with this case. It is reported only when it held an
/// object of this deletion back, and its text says so, because a warning on
/// every later deletion would describe the archive rather than the command
/// the user ran. Finding one wherever it sits is `archive check`'s work.
///
/// Both facts are therefore reported, because they answer different
/// questions and either may be the larger. `malformed_count` is the
/// archive-wide number of such references the scan read, and
/// `withheld_count` is how many candidate objects of this deletion they held
/// back, which is what the message's `for this case` describes.
#[must_use]
pub fn malformed_references(malformed_count: u64, withheld_count: u64) -> Warning {
    Diagnostic::new(
        codes::RECORD_MALFORMED,
        "A record this deletion keeps references an artefact by something that is not a digest, so no object was purged for this case.",
        Details::new()
            .text("stage", "delete")
            .int("malformed_count", malformed_count)
            .int("withheld_count", withheld_count),
    )
}

#[cfg(test)]
mod tests;
