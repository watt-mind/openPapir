//! Which records belong to a case, and how they are written out.
//!
//! A case owns its submissions directly. A receipt belongs to the case only
//! through an association the user recorded, because openPapir matches
//! nothing on its own: an association is the user's own statement
//! (`docs/archive-layout.md`). An import event belongs to the case when it
//! introduced one of the artefacts those records reference.
//!
//! Nothing here interprets a record. The export copies what the archive
//! already holds, so a document is written out exactly as it was stored, and
//! an original filename stays what it always was: an attribute inside an
//! import-event record, never a file name and never part of a path.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::archive::import::ImportEvent;
use crate::error::Diagnostic;
use crate::export::KindCount;
use crate::export::destination::{self, Destination};
use crate::records::association::Association;
use crate::records::case::Case;
use crate::records::document::{self, Record};
use crate::records::receipt::Receipt;
use crate::records::submission::Submission;
use crate::records::{DIGEST_PREFIX, is_digest};

/// One exported record, as the manifest lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordEntry {
    /// The record's own identifier, which is also the file's name.
    pub id: String,
    /// The record kind, which is also the directory the file lives in.
    pub kind: String,
}

/// The records of one case, and the objects they reference.
#[derive(Debug)]
pub struct Collected {
    /// The case itself.
    pub case: Case,
    /// Every submission recorded against the case.
    pub submissions: Vec<Submission>,
    /// Every receipt an association ties to one of those submissions.
    pub receipts: Vec<Receipt>,
    /// Every association naming one of those submissions.
    pub associations: Vec<Association>,
    /// Every import event that introduced one of the referenced artefacts.
    pub import_events: Vec<ImportEvent>,
    /// The bare hexadecimal digests of every referenced object.
    pub digests: BTreeSet<String>,
}

/// What writing the records produced.
#[derive(Debug)]
pub struct Written {
    /// One entry per written record, ordered by kind and then identifier.
    pub entries: Vec<RecordEntry>,
    /// One count per record kind, in the fixed order of the kinds.
    pub counts: Vec<KindCount>,
}

/// Read every record belonging to the case, without taking the writer lock.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no case, and
/// `record.malformed` when a stored document cannot be read as a record of
/// its kind.
pub fn gather(root: &Path, case_id: &str) -> Result<Collected, Diagnostic> {
    let case = document::read_record::<Case>(root, case_id, "case_id")?;
    let submissions: Vec<Submission> = document::list_records::<Submission>(root)?
        .into_iter()
        .filter(|submission| submission.case_id == case.id)
        .collect();
    let submission_ids: BTreeSet<&str> = submissions
        .iter()
        .map(|submission| submission.id.as_str())
        .collect();

    let associations: Vec<Association> = document::list_records::<Association>(root)?
        .into_iter()
        .filter(|association| names_submission(association, &submission_ids))
        .collect();
    let receipt_ids: BTreeSet<&str> = associations
        .iter()
        .map(|association| association.receipt_id.as_str())
        .collect();
    let receipts: Vec<Receipt> = document::list_records::<Receipt>(root)?
        .into_iter()
        .filter(|receipt| receipt_ids.contains(receipt.id.as_str()))
        .collect();

    let mut digests = BTreeSet::new();
    for submission in &submissions {
        for artefact in &submission.artefacts {
            insert_digest(&mut digests, &artefact.digest);
        }
    }
    for receipt in &receipts {
        insert_digest(&mut digests, &receipt.artefact_digest);
    }
    let import_events: Vec<ImportEvent> = document::list_records::<ImportEvent>(root)?
        .into_iter()
        .filter(|event| bare_digest(&event.digest).is_some_and(|hex| digests.contains(&hex)))
        .collect();

    Ok(Collected {
        case,
        submissions,
        receipts,
        associations,
        import_events,
        digests,
    })
}

/// Whether an association names one of the case's submissions, as the
/// confirmed one or as a candidate. History is never filtered: a superseded
/// association is exported with the rest.
fn names_submission(association: &Association, submissions: &BTreeSet<&str>) -> bool {
    association
        .submission_id
        .as_deref()
        .is_some_and(|id| submissions.contains(id))
        || association
            .candidates
            .iter()
            .any(|candidate| submissions.contains(candidate.submission_id.as_str()))
}

/// The bare hexadecimal form of an algorithm-qualified digest.
fn bare_digest(value: &str) -> Option<String> {
    value
        .strip_prefix(DIGEST_PREFIX)
        .filter(|hex| is_digest(hex))
        .map(str::to_owned)
}

/// Keep a well-formed digest, and pass over one that is not.
///
/// A reference that is not a digest at all names no object, so there is
/// nothing to copy. The whole-archive integrity check is what reports such a
/// record; an export neither repairs nor judges what it copies.
fn insert_digest(digests: &mut BTreeSet<String>, value: &str) {
    if let Some(hex) = bare_digest(value) {
        digests.insert(hex);
    }
}

/// Write every collected record into the destination as JSON.
///
/// # Errors
///
/// Returns `input.cap.record_size` for a document over the record cap,
/// `internal.unexpected` when a record cannot be serialised, `path.symlink`,
/// `export.destination_conflict`, or `write.interrupted`.
pub fn write_records(
    destination: &Destination,
    collected: &Collected,
) -> Result<Written, Diagnostic> {
    let mut entries = Vec::new();
    let counts = vec![
        write_kind(
            destination,
            std::slice::from_ref(&collected.case),
            &mut entries,
        )?,
        write_kind(destination, &collected.submissions, &mut entries)?,
        write_kind(destination, &collected.receipts, &mut entries)?,
        write_kind(destination, &collected.associations, &mut entries)?,
        write_kind(destination, &collected.import_events, &mut entries)?,
    ];
    Ok(Written { entries, counts })
}

/// Write every record of one kind into its own directory.
fn write_kind<R: Record>(
    destination: &Destination,
    records: &[R],
    entries: &mut Vec<RecordEntry>,
) -> Result<KindCount, Diagnostic> {
    if records.is_empty() {
        return Ok(KindCount {
            count: 0,
            kind: R::KIND,
        });
    }
    let directory = destination.records_directory(R::KIND)?;
    for record in records {
        let file_name = format!("{}.json", record.id());
        let relative = format!("{}/{}/{file_name}", destination::RECORDS_DIR, R::KIND);
        let document = document::document(record)?;
        destination.write_new(
            &directory,
            &file_name,
            &relative,
            destination::RECORD_WRITE,
            document.as_bytes(),
        )?;
        entries.push(RecordEntry {
            id: record.id().to_owned(),
            kind: R::KIND.to_owned(),
        });
    }
    Ok(KindCount {
        count: records.len() as u64,
        kind: R::KIND,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::codes;
    use crate::records::association::Candidate;

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    fn association(submission_id: Option<&str>, candidate: Option<&str>) -> Association {
        Association {
            archive_schema_version: 1,
            candidates: candidate
                .map(|id| {
                    vec![Candidate {
                        confidence: "weak".to_owned(),
                        evidence: Vec::new(),
                        submission_id: id.to_owned(),
                    }]
                })
                .unwrap_or_default(),
            created_at: "2026-01-17T12:00:00Z".to_owned(),
            created_by: "user".to_owned(),
            id: "1111000fffeeeeddddccccbbbbaaaa00".to_owned(),
            outcome: "candidate".to_owned(),
            receipt_id: "aaaabbbbccccddddeeeeffff00001111".to_owned(),
            record_kind: "association".to_owned(),
            submission_id: submission_id.map(str::to_owned),
            supersedes: None,
        }
    }

    #[test]
    fn an_association_belongs_to_the_case_through_either_reference() {
        let mine = BTreeSet::from(["fedcba9876543210fedcba9876543210"]);
        assert!(names_submission(
            &association(Some("fedcba9876543210fedcba9876543210"), None),
            &mine
        ));
        assert!(names_submission(
            &association(None, Some("fedcba9876543210fedcba9876543210")),
            &mine
        ));
        assert!(!names_submission(&association(None, None), &mine));
        assert!(!names_submission(
            &association(Some("00000000000000000000000000000000"), None),
            &mine
        ));
    }

    #[test]
    fn only_a_well_formed_digest_names_an_object_to_copy() {
        let mut digests = BTreeSet::new();
        insert_digest(&mut digests, &format!("sha256:{DIGEST}"));
        insert_digest(&mut digests, DIGEST);
        insert_digest(&mut digests, "sha256:not-a-digest");
        assert_eq!(digests, BTreeSet::from([DIGEST.to_owned()]));
        assert_eq!(
            bare_digest(&format!("sha256:{DIGEST}")).as_deref(),
            Some(DIGEST)
        );
        assert_eq!(bare_digest("sha512:x"), None);
    }

    #[test]
    fn a_case_the_archive_does_not_hold_is_not_found() {
        let root = tempfile::tempdir().unwrap();
        let refusal = gather(root.path(), "0123456789abcdef0123456789abcdef").unwrap_err();
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        assert_eq!(
            gather(root.path(), "not-an-identifier").unwrap_err().code,
            codes::RECORD_NOT_FOUND,
            "an unusable identifier is never joined into a path"
        );
    }
}
