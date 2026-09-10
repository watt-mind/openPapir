//! Association records: the user's own statement about a receipt.
//!
//! An association is a separate record with its own evidence, never a foreign
//! key implying certainty (`docs/archive-layout.md`). It records that the
//! user asserted something about whether a receipt relates to a submission,
//! and it implies no delivery, no receipt by an authority, no authenticity,
//! and no legal effect.
//!
//! openPapir asserts nothing of its own here. Every evidence entry in this
//! build carries `kind` `user_assertion` and `source` `user`, and every
//! record carries `created_by` `user`: no automatic matching, derived
//! metadata, or receipt parsing exists, so nothing else could be recorded.
//!
//! Records are append-only. A change writes a new record superseding the
//! previous one, so history is inspectable: nothing is edited or deleted, a
//! superseded record is never modified, and a listing never collapses or
//! filters what it holds.
//!
//! Retiring an association is that same supersession, used to withdraw an
//! assertion: it writes a record with outcome `unassociated` and no
//! candidate, so the history reads as what the user asserted and then that
//! they withdrew it. Nothing is edited and nothing is removed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::lock::WriterLock;
use crate::archive::{Archive, SUPPORTED_SCHEMA_VERSION};
use crate::clock;
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::ident;
use crate::records::document::{self, Record};
use crate::records::receipt::Receipt;
use crate::records::submission::Submission;
use crate::records::{ASSOCIATIONS_DIR, checked_statement, inconsistent};

/// The value an association record carries in `record_kind`.
pub const KIND: &str = "association";

/// The four recorded outcomes, in the order the design lists them.
pub const OUTCOMES: [&str; 4] = ["unassociated", "candidate", "associated", "contradictory"];
/// The closed ordinal confidence set. It is never a number, because no
/// calibration data exists and a number would imply one.
pub const CONFIDENCES: [&str; 3] = ["weak", "moderate", "strong"];

/// The value every evidence entry carries in `kind` in this build.
pub const EVIDENCE_KIND: &str = "user_assertion";
/// The value every evidence entry carries in `source` in this build.
pub const EVIDENCE_SOURCE: &str = "user";
/// The value every association carries in `created_by` in this build.
pub const CREATED_BY: &str = "user";

/// The consistency rules an association can break, as stable `rule` values.
pub mod rules {
    /// `unassociated` was given a candidate.
    pub const UNASSOCIATED_HAS_CANDIDATES: &str = "unassociated_has_candidates";
    /// `candidate` was given no candidate.
    pub const CANDIDATE_REQUIRES_CANDIDATES: &str = "candidate_requires_candidates";
    /// `associated` was not given exactly one candidate.
    pub const ASSOCIATED_REQUIRES_ONE_CANDIDATE: &str = "associated_requires_one_candidate";
    /// `contradictory` was given fewer than two candidates.
    pub const CONTRADICTORY_REQUIRES_TWO_CANDIDATES: &str = "contradictory_requires_two_candidates";
    /// One submission was named as a candidate more than once.
    pub const DUPLICATE_CANDIDATE_SUBMISSION: &str = "duplicate_candidate_submission";
    /// The superseded record belongs to another receipt.
    pub const SUPERSEDES_OTHER_RECEIPT: &str = "supersedes_other_receipt";
    /// The record a retirement names is superseded already.
    pub const ALREADY_SUPERSEDED: &str = "already_superseded";
}

/// One evidence entry: what was observed, and who observed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// The kind of evidence, always `user_assertion` in this build.
    pub kind: String,
    /// Where the evidence came from, always `user` in this build.
    pub source: String,
    /// The user's own readable statement of what they observed.
    pub statement: String,
}

/// One candidate submission and the evidence the user gave for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// An ordinal label from the closed set, never a probability.
    pub confidence: String,
    /// The evidence entries for this candidate, at least one.
    pub evidence: Vec<Evidence>,
    /// The submission this candidate names.
    pub submission_id: String,
}

/// One association record, stored as `records/associations/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Association {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u32,
    /// The candidate submissions, none for `unassociated`.
    pub candidates: Vec<Candidate>,
    /// When openPapir recorded the association.
    pub created_at: String,
    /// Who created the record, always `user` in this build.
    pub created_by: String,
    /// The association's own identifier, minted by openPapir.
    pub id: String,
    /// One of the four recorded outcomes.
    pub outcome: String,
    /// The receipt this association is about.
    pub receipt_id: String,
    /// The record kind, always `association`.
    pub record_kind: String,
    /// The user's own statement of why the record was written, carried by a
    /// retirement and absent from every other record. It is the user's own
    /// text, so it is stored and never repeated in a message or a count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
    /// The confirmed submission, `null` unless the outcome is `associated`.
    pub submission_id: Option<String>,
    /// The association this record supersedes, `null` when it supersedes none.
    pub supersedes: Option<String>,
}

impl Record for Association {
    const KIND: &'static str = KIND;
    const DIRECTORY: &'static str = ASSOCIATIONS_DIR;

    fn id(&self) -> &str {
        &self.id
    }

    fn record_kind(&self) -> &str {
        &self.record_kind
    }
}

/// What creating an association reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssociationCreated {
    /// The association as it was stored.
    pub association: Association,
}

/// What listing one receipt's associations reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssociationHistory {
    /// Every association for the receipt, newest first, including superseded
    /// records. History is never collapsed or filtered.
    pub associations: Vec<Association>,
    /// How many associations the receipt holds.
    pub count: u64,
    /// The receipt the history belongs to.
    pub receipt_id: String,
}

/// What showing one association reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssociationView {
    /// The association as it is stored.
    pub association: Association,
    /// The supersession chain the association belongs to: the association
    /// itself, every record it supersedes, and every record that supersedes
    /// it, newest first as `association list` orders a history. Nothing is
    /// collapsed or filtered, so a chain reads as everything the user has
    /// asserted about this receipt along this line of records.
    pub chain: Vec<Association>,
    /// How many records the chain holds, the association included.
    pub chain_length: u64,
    /// Whether the association is the live head of its chain, meaning no
    /// stored record supersedes it. A superseded record is history: it says
    /// what the user asserted then, not what they assert today.
    pub live: bool,
}

/// Record what the user asserts about a receipt.
///
/// Each entry of `candidates` is `<submission-id>:<confidence>:<statement>`,
/// split on the first two colons only, so a statement may contain a colon.
/// Every consistency rule is enforced before anything is written.
///
/// # Errors
///
/// Returns `usage.arguments` for an unknown outcome or confidence and a
/// malformed candidate, `input.cap.field_length` for an oversized statement,
/// `record.not_found` for a receipt, submission, or superseded association
/// that does not exist, `record.inconsistent` for a rule violation,
/// `record.malformed` for an unreadable document, and any archive, lock,
/// path, or write refusal of `docs/error-contract.md`.
pub fn create(
    root: &Path,
    receipt_id: &str,
    outcome: &str,
    candidates: &[String],
    supersedes: Option<&str>,
) -> Result<AssociationCreated> {
    let mut warnings = Vec::new();
    match create_record(
        root,
        receipt_id,
        outcome,
        candidates,
        supersedes,
        &mut warnings,
    ) {
        Ok(association) => Ok(Outcome {
            data: AssociationCreated { association },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn create_record(
    root: &Path,
    receipt_id: &str,
    outcome: &str,
    candidates: &[String],
    supersedes: Option<&str>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Association, Diagnostic> {
    let outcome = checked_outcome(outcome)?;
    let candidates = parse_candidates(candidates)?;
    check_shape(&outcome, &candidates)?;

    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let receipt = document::read_record::<Receipt>(archive.root(), receipt_id, "receipt_id")?;
    for candidate in &candidates {
        document::read_record::<Submission>(
            archive.root(),
            &candidate.submission_id,
            "submission_id",
        )?;
    }
    let supersedes = checked_supersedes(archive.root(), &receipt.id, supersedes)?;

    let association = Association {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        created_at: clock::now_rfc3339(),
        created_by: CREATED_BY.to_owned(),
        id: ident::new_id()?,
        receipt_id: receipt.id,
        record_kind: KIND.to_owned(),
        statement: None,
        submission_id: (outcome == "associated").then(|| candidates[0].submission_id.clone()),
        supersedes,
        candidates,
        outcome,
    };
    warnings.extend(document::write_record(archive.root(), &association)?);
    Ok(association)
}

/// Read one of the four recorded outcomes.
fn checked_outcome(value: &str) -> std::result::Result<String, Diagnostic> {
    if OUTCOMES.contains(&value) {
        return Ok(value.to_owned());
    }
    Err(unusable("outcome"))
}

/// Read `<submission-id>:<confidence>:<statement>` for each candidate.
///
/// The value is split on the first two colons only, so a statement may carry
/// one. Neither the identifier nor the statement is echoed by a refusal.
fn parse_candidates(entries: &[String]) -> std::result::Result<Vec<Candidate>, Diagnostic> {
    let mut candidates = Vec::with_capacity(entries.len());
    for entry in entries {
        let (submission_id, rest) = entry.split_once(':').ok_or_else(|| unusable("candidate"))?;
        let (confidence, statement) = rest.split_once(':').ok_or_else(|| unusable("candidate"))?;
        if !document::is_identifier(submission_id) || !CONFIDENCES.contains(&confidence) {
            return Err(unusable("candidate"));
        }
        candidates.push(Candidate {
            confidence: confidence.to_owned(),
            evidence: vec![Evidence {
                kind: EVIDENCE_KIND.to_owned(),
                source: EVIDENCE_SOURCE.to_owned(),
                statement: checked_statement(statement)?,
            }],
            submission_id: submission_id.to_owned(),
        });
    }
    Ok(candidates)
}

/// Enforce every rule that needs no archive, before the archive is opened.
fn check_shape(outcome: &str, candidates: &[Candidate]) -> std::result::Result<(), Diagnostic> {
    let rule = match (outcome, candidates.len()) {
        ("unassociated", 0) | ("candidate" | "associated", 1) => None,
        ("unassociated", _) => Some(rules::UNASSOCIATED_HAS_CANDIDATES),
        ("candidate", 0) => Some(rules::CANDIDATE_REQUIRES_CANDIDATES),
        ("associated", _) => Some(rules::ASSOCIATED_REQUIRES_ONE_CANDIDATE),
        ("contradictory", count) if count < 2 => Some(rules::CONTRADICTORY_REQUIRES_TWO_CANDIDATES),
        _ => None,
    };
    if let Some(rule) = rule {
        return Err(inconsistent(KIND, rule));
    }
    let mut seen: Vec<&str> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if seen.contains(&candidate.submission_id.as_str()) {
            return Err(inconsistent(KIND, rules::DUPLICATE_CANDIDATE_SUBMISSION));
        }
        seen.push(&candidate.submission_id);
    }
    Ok(())
}

/// Resolve the superseded record, which must belong to the same receipt.
///
/// The superseded record itself is never modified: supersession is recorded
/// by the new record alone, so history stays inspectable. Only an absent
/// value means no supersession: a supplied empty one names no association.
fn checked_supersedes(
    root: &Path,
    receipt_id: &str,
    supersedes: Option<&str>,
) -> std::result::Result<Option<String>, Diagnostic> {
    let Some(supersedes) = supersedes else {
        return Ok(None);
    };
    let superseded = document::read_record::<Association>(root, supersedes, "association_id")?;
    if superseded.receipt_id != receipt_id {
        return Err(inconsistent(KIND, rules::SUPERSEDES_OTHER_RECEIPT));
    }
    Ok(Some(superseded.id))
}

/// Retire one association by superseding it with a record that claims nothing.
///
/// Nothing is edited and nothing is removed. The retirement is a new record
/// for the same receipt, with outcome `unassociated`, no candidate, and
/// `supersedes` naming the record the user retired, so the history reads as
/// what was asserted and then that the user withdrew it. A retired record
/// stays exactly as it was written.
///
/// `reason` is the user's own statement of why they withdrew the assertion.
/// It is stored on the new record as its `statement` and is never repeated in
/// a message, a warning, or a count.
///
/// A record another association already supersedes is refused: the newer
/// record is what a further statement would have to supersede, and writing a
/// second record over the same one would leave two records claiming to
/// replace it.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no association,
/// `record.inconsistent` with rule `already_superseded` when the named record
/// is superseded already, `input.cap.field_length` for an oversized reason,
/// `record.malformed` for an unreadable document, and any archive, lock,
/// path, or write refusal of `docs/error-contract.md`.
pub fn retire(
    root: &Path,
    association_id: &str,
    reason: Option<&str>,
) -> Result<AssociationCreated> {
    let mut warnings = Vec::new();
    match retire_record(root, association_id, reason, &mut warnings) {
        Ok(association) => Ok(Outcome {
            data: AssociationCreated { association },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn retire_record(
    root: &Path,
    association_id: &str,
    reason: Option<&str>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Association, Diagnostic> {
    let statement = reason.map(checked_statement).transpose()?;
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let retired =
        document::read_record::<Association>(archive.root(), association_id, "association_id")?;
    let stored = document::list_records::<Association>(archive.root())?;
    if stored
        .iter()
        .any(|association| association.supersedes.as_deref() == Some(retired.id.as_str()))
    {
        return Err(inconsistent(KIND, rules::ALREADY_SUPERSEDED));
    }

    let association = Association {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        candidates: Vec::new(),
        created_at: clock::now_rfc3339(),
        created_by: CREATED_BY.to_owned(),
        id: ident::new_id()?,
        outcome: "unassociated".to_owned(),
        receipt_id: retired.receipt_id.clone(),
        record_kind: KIND.to_owned(),
        statement,
        submission_id: None,
        supersedes: Some(retired.id),
    };
    warnings.extend(document::write_record(archive.root(), &association)?);
    Ok(association)
}

/// How many records each association supersedes, following the chain.
///
/// The walk is bounded by the number of records, so a hand-edited archive
/// holding a cycle yields a finite depth instead of looping.
fn supersession_depths(associations: &[Association]) -> BTreeMap<String, usize> {
    let by_id: BTreeMap<&str, &Association> = associations
        .iter()
        .map(|association| (association.id.as_str(), association))
        .collect();
    let mut depths = BTreeMap::new();
    for association in associations {
        let mut depth = 0;
        let mut current = association;
        while let Some(previous) = current
            .supersedes
            .as_deref()
            .and_then(|id| by_id.get(id))
            .filter(|_| depth < associations.len())
        {
            depth += 1;
            current = previous;
        }
        depths.insert(association.id.clone(), depth);
    }
    depths
}

/// Where each `supersedes` cycle the given edges form closes on itself.
///
/// `edges` names, for each record, the record it supersedes. An association
/// supersedes at most one other record, so the supersession graph has
/// out-degree one and every walk along it either ends at a record that
/// supersedes nothing, leaves the graph at a reference nothing stored
/// answers, or closes on itself. A closed walk is a cycle: every record in it
/// is superseded by another, so the history it forms has no live record and
/// nothing says what the user asserts today. No openPapir command writes one,
/// because `association retire` refuses a record another one supersedes
/// already and `association create` refuses a `--supersedes` outside the
/// receipt, so a cycle reaches an archive only by hand.
///
/// One key is returned per cycle, the record the walk came back to, so a
/// caller may count the cycles or name whatever it holds each record under. A
/// cycle several records lead into is still one cycle, and two separate
/// cycles are two.
///
/// Each record is walked at most twice, once on the walk that reaches it and
/// once as a start that stops immediately, so a hand-edited archive holding a
/// cycle settles instead of looping.
///
/// The keys are whatever identifies a record to the caller, so this holds no
/// stored value of its own.
#[must_use]
pub(crate) fn supersession_cycles<K: Copy + Ord>(edges: &BTreeMap<K, K>) -> Vec<K> {
    let mut settled: BTreeSet<K> = BTreeSet::new();
    let mut cycles = Vec::new();
    for start in edges.keys() {
        if settled.contains(start) {
            continue;
        }
        let mut walked: BTreeSet<K> = BTreeSet::new();
        let mut here = *start;
        while !settled.contains(&here) {
            if !walked.insert(here) {
                cycles.push(here);
                break;
            }
            match edges.get(&here) {
                Some(previous) => here = *previous,
                None => break,
            }
        }
        settled.extend(walked);
    }
    cycles
}

/// The refusal for a value openPapir cannot read, naming the argument only.
fn unusable(argument: &'static str) -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "An argument is not one of the documented values for it.",
        Details::new().text("argument", argument),
    )
}

/// List one receipt's associations, newest first, without taking the lock.
///
/// The listing holds the whole history, superseded records included, and each
/// record carries the identifier it supersedes. Nothing is collapsed or
/// filtered.
///
/// Newest first means by `created_at`, then by how many records a record
/// supersedes, then by identifier, all descending. The middle key matters
/// because openPapir records whole seconds: two records written in the same
/// second would otherwise order arbitrarily, and a record that supersedes
/// another is by construction the later of the two.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no receipt,
/// `record.malformed` for an unreadable document, and any archive refusal.
pub fn list(root: &Path, receipt_id: &str) -> Result<AssociationHistory> {
    let mut warnings = Vec::new();
    match list_history(root, receipt_id, &mut warnings) {
        Ok(history) => Ok(Outcome {
            data: history,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn list_history(
    root: &Path,
    receipt_id: &str,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<AssociationHistory, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let receipt = document::read_record::<Receipt>(archive.root(), receipt_id, "receipt_id")?;
    let associations = history_of(archive.root(), &receipt.id)?;
    Ok(AssociationHistory {
        count: associations.len() as u64,
        associations,
        receipt_id: receipt.id,
    })
}

/// Every stored association for one receipt, newest first.
///
/// The receipt identifier is the one a record already carries, so nothing is
/// resolved here: the caller has read the receipt, or the association whose
/// receipt this is, and this is the listing that belongs to it.
pub(crate) fn history_of(
    root: &Path,
    receipt_id: &str,
) -> std::result::Result<Vec<Association>, Diagnostic> {
    let mut associations: Vec<Association> = document::list_records::<Association>(root)?
        .into_iter()
        .filter(|association| association.receipt_id == receipt_id)
        .collect();
    sort_newest_first(&mut associations);
    Ok(associations)
}

/// Order associations newest first, exactly as `association list` reports one
/// receipt's history.
///
/// Newest first means by `created_at`, then by how many records a record
/// supersedes, then by identifier, all descending. The middle key matters
/// because openPapir records whole seconds: two records written in the same
/// second would otherwise order arbitrarily, and a record that supersedes
/// another is by construction the later of the two.
pub(crate) fn sort_newest_first(associations: &mut [Association]) {
    let depths = supersession_depths(associations);
    associations.sort_by(|left, right| {
        (&right.created_at, depths[&right.id], &right.id).cmp(&(
            &left.created_at,
            depths[&left.id],
            &left.id,
        ))
    });
}

/// Whether no record in `associations` supersedes the one with this identifier.
///
/// A live head is what the user asserts today. Everything behind it stays
/// exactly as it was written and is still listed, which is what makes a
/// withdrawal inspectable rather than a deletion.
pub(crate) fn is_live(associations: &[Association], id: &str) -> bool {
    !associations
        .iter()
        .any(|association| association.supersedes.as_deref() == Some(id))
}

/// Whether one association names this submission, as a candidate or as the
/// confirmed submission of an `associated` outcome.
pub(crate) fn names_submission(association: &Association, submission_id: &str) -> bool {
    association.submission_id.as_deref() == Some(submission_id)
        || association
            .candidates
            .iter()
            .any(|candidate| candidate.submission_id == submission_id)
}

/// Show one association with the supersession chain it belongs to.
///
/// The chain is everything reachable from the association along `supersedes`,
/// in both directions: what it supersedes, transitively, and what supersedes
/// it. The walk keeps a visited set, so a hand-edited archive holding a cycle
/// yields a finite chain instead of looping. Reading takes no writer lock,
/// because every record file is written whole.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no association,
/// `record.malformed` for an unreadable document, and any archive refusal.
pub fn show(root: &Path, association_id: &str) -> Result<AssociationView> {
    let mut warnings = Vec::new();
    match show_record(root, association_id, &mut warnings) {
        Ok(view) => Ok(Outcome {
            data: view,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn show_record(
    root: &Path,
    association_id: &str,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<AssociationView, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let association =
        document::read_record::<Association>(archive.root(), association_id, "association_id")?;
    let history = history_of(archive.root(), &association.receipt_id)?;
    let chain = chain_of(&history, &association.id);
    Ok(AssociationView {
        chain_length: chain.len() as u64,
        live: is_live(&history, &association.id),
        association,
        chain,
    })
}

/// The records connected to `id` through `supersedes`, in `history`'s order.
///
/// A record may in principle be superseded by more than one other, so the
/// walk follows every edge rather than a single line, and the visited set
/// bounds it by the number of records the receipt holds.
fn chain_of(history: &[Association], id: &str) -> Vec<Association> {
    let mut reached: BTreeSet<&str> = BTreeSet::new();
    let mut pending = vec![id];
    while let Some(current) = pending.pop() {
        if !reached.insert(current) {
            continue;
        }
        for association in history {
            if association.id == current {
                pending.extend(association.supersedes.as_deref());
            }
            if association.supersedes.as_deref() == Some(current) {
                pending.push(&association.id);
            }
        }
    }
    history
        .iter()
        .filter(|association| reached.contains(association.id.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
