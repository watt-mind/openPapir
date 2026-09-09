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

use std::collections::BTreeMap;
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
/// by the new record alone, so history stays inspectable.
fn checked_supersedes(
    root: &Path,
    receipt_id: &str,
    supersedes: Option<&str>,
) -> std::result::Result<Option<String>, Diagnostic> {
    let Some(supersedes) = supersedes.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let superseded = document::read_record::<Association>(root, supersedes, "association_id")?;
    if superseded.receipt_id != receipt_id {
        return Err(inconsistent(KIND, rules::SUPERSEDES_OTHER_RECEIPT));
    }
    Ok(Some(superseded.id))
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
    let mut associations: Vec<Association> = document::list_records::<Association>(archive.root())?
        .into_iter()
        .filter(|association| association.receipt_id == receipt.id)
        .collect();
    let depths = supersession_depths(&associations);
    associations.sort_by(|left, right| {
        (&right.created_at, depths[&right.id], &right.id).cmp(&(
            &left.created_at,
            depths[&left.id],
            &left.id,
        ))
    });
    Ok(AssociationHistory {
        count: associations.len() as u64,
        associations,
        receipt_id: receipt.id,
    })
}

#[cfg(test)]
mod tests {
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
                refusal.code == codes::INPUT_CAP_FIELD_LENGTH
                    || refusal.code == codes::USAGE_ARGUMENTS
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
}
