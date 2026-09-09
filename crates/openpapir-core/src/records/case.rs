//! Case records: a user-created folder of related correspondence.
//!
//! A case is purely local. It corresponds to nothing any government service
//! issues, and holding correspondence in one asserts nothing about delivery,
//! receipt by an authority, authenticity, or legal effect.
//!
//! Creating a case takes the archive's single-writer lock and runs the
//! archive's permission checks first. Listing and showing a case take no lock,
//! because no record file is ever modified in place.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::lock::WriterLock;
use crate::archive::{Archive, SUPPORTED_SCHEMA_VERSION};
use crate::clock;
use crate::error::{Diagnostic, Failure, Outcome, Result, Warning};
use crate::ident;
use crate::records::document::{self, Record};
use crate::records::submission::Submission;
use crate::records::{CASES_DIR, checked_notes, checked_title};

/// The value a case record carries in `record_kind`.
pub const KIND: &str = "case";

/// One case record, stored as `records/cases/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u32,
    /// When openPapir recorded the case.
    pub created_at: String,
    /// The case's own identifier, minted by openPapir.
    pub id: String,
    /// The user's own notes, absent when none were supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// The record kind, always `case`.
    pub record_kind: String,
    /// The user's own title for the case.
    pub title: String,
}

impl Record for Case {
    const KIND: &'static str = KIND;
    const DIRECTORY: &'static str = CASES_DIR;

    fn id(&self) -> &str {
        &self.id
    }

    fn record_kind(&self) -> &str {
        &self.record_kind
    }
}

/// What creating a case reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseCreated {
    /// The case as it was stored.
    pub case: Case,
}

/// What listing cases reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseList {
    /// Every case in the archive, ordered by identifier.
    pub cases: Vec<Case>,
    /// How many cases the archive holds.
    pub count: u64,
}

/// What showing one case reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseView {
    /// The case itself.
    pub case: Case,
    /// The submissions recorded against it, ordered by identifier.
    pub submissions: Vec<Submission>,
    /// How many submissions the case holds.
    pub submission_count: u64,
}

/// Create a case in the archive at `root`.
///
/// The field caps are checked before the archive is opened, so an oversized
/// field is refused before anything is written.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments` for a field that
/// breaks its cap or shape, and any archive, lock, path, or write refusal of
/// `docs/error-contract.md`.
pub fn create(root: &Path, title: &str, notes: Option<&str>) -> Result<CaseCreated> {
    let mut warnings = Vec::new();
    match create_record(root, title, notes, &mut warnings) {
        Ok(case) => Ok(Outcome {
            data: CaseCreated { case },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn create_record(
    root: &Path,
    title: &str,
    notes: Option<&str>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Case, Diagnostic> {
    let title = checked_title(title)?;
    let notes = checked_notes(notes)?;
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let case = Case {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        created_at: clock::now_rfc3339(),
        id: ident::new_id()?,
        notes,
        record_kind: KIND.to_owned(),
        title,
    };
    warnings.extend(document::write_record(archive.root(), &case)?);
    Ok(case)
}

/// List every case in the archive at `root`, without taking the lock.
///
/// # Errors
///
/// Returns any archive refusal, or `record.malformed` when a stored document
/// cannot be read as a case.
pub fn list(root: &Path) -> Result<CaseList> {
    let mut warnings = Vec::new();
    match list_records(root, &mut warnings) {
        Ok(cases) => Ok(Outcome {
            data: CaseList {
                count: cases.len() as u64,
                cases,
            },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn list_records(
    root: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Vec<Case>, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    document::list_records::<Case>(archive.root())
}

/// Show one case and the submissions recorded against it.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no case,
/// `record.malformed` when a stored document cannot be read, and any archive
/// refusal.
pub fn show(root: &Path, case_id: &str) -> Result<CaseView> {
    let mut warnings = Vec::new();
    match show_record(root, case_id, &mut warnings) {
        Ok(view) => Ok(Outcome {
            data: view,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn show_record(
    root: &Path,
    case_id: &str,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<CaseView, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let case = document::read_record::<Case>(archive.root(), case_id, "case_id")?;
    let submissions: Vec<Submission> = document::list_records::<Submission>(archive.root())?
        .into_iter()
        .filter(|submission| submission.case_id == case.id)
        .collect();
    Ok(CaseView {
        case,
        submission_count: submissions.len() as u64,
        submissions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;
    use crate::error::codes;
    use crate::records::submission;
    use std::fs;

    fn archive_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        archive::init(root.path()).unwrap();
        root
    }

    #[test]
    fn a_created_case_is_listed_and_shown_again() {
        let root = archive_root();
        let created = create(root.path(), "Tax matter", Some("First contact.")).unwrap();
        let case = created.data.case;
        assert_eq!(case.title, "Tax matter");
        assert_eq!(case.notes.as_deref(), Some("First contact."));
        assert_eq!(case.record_kind, KIND);
        assert_eq!(case.archive_schema_version, 1);
        assert_eq!(case.id.len(), 32);
        assert!(case.created_at.ends_with('Z'));

        let listed = list(root.path()).unwrap().data;
        assert_eq!(listed.count, 1);
        assert_eq!(listed.cases[0], case);

        let shown = show(root.path(), &case.id).unwrap().data;
        assert_eq!(shown.case, case);
        assert_eq!(shown.submission_count, 0);
        assert!(shown.submissions.is_empty());
    }

    #[test]
    fn a_case_without_notes_omits_the_field_entirely() {
        let root = archive_root();
        let case = create(root.path(), "Plain title", None).unwrap().data.case;
        assert_eq!(case.notes, None);
        let stored = fs::read_to_string(
            root.path()
                .join(CASES_DIR)
                .join(format!("{}.json", case.id)),
        )
        .unwrap();
        assert!(!stored.contains("notes"));
        assert!(stored.ends_with("}\n"), "one LF-terminated document");
        let keys: Vec<String> = serde_json::from_str::<serde_json::Value>(&stored)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "keys are stored sorted");
    }

    #[test]
    fn an_empty_archive_lists_no_case() {
        let root = archive_root();
        let listed = list(root.path()).unwrap().data;
        assert_eq!(listed.count, 0);
        assert!(listed.cases.is_empty());
    }

    #[test]
    fn an_unknown_case_is_refused_without_echoing_the_reference() {
        let root = archive_root();
        let refusal = show(root.path(), "0123456789abcdef0123456789abcdef")
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        assert_eq!(refusal.exit_code(), 4);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "case");
        assert_eq!(json["details"]["reference_kind"], "case_id");
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
    }

    #[test]
    fn a_field_that_breaks_its_cap_is_refused_before_any_write() {
        let root = archive_root();
        let refusal = create(root.path(), &"t".repeat(201), None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        let refusal = create(root.path(), "Title", Some(&"n".repeat(4097)))
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        assert_eq!(
            fs::read_dir(root.path().join(CASES_DIR)).unwrap().count(),
            0,
            "no record survives a refused field"
        );
    }

    #[test]
    fn creating_a_case_needs_the_writer_lock() {
        let root = archive_root();
        let _held = WriterLock::acquire(root.path()).unwrap();
        let refusal = create(root.path(), "Held", None).unwrap_err().error;
        assert_eq!(refusal.code, codes::LOCK_HELD);
        assert!(refusal.is_retryable());
        assert!(list(root.path()).is_ok(), "listing needs no lock at all");
    }

    #[test]
    fn a_malformed_case_document_is_reported_rather_than_skipped() {
        let root = archive_root();
        let case = create(root.path(), "Readable", None).unwrap().data.case;
        fs::write(
            root.path()
                .join(CASES_DIR)
                .join("ffffffffffffffffffffffffffffffff.json"),
            b"{ not a record",
        )
        .unwrap();
        let refusal = list(root.path()).unwrap_err().error;
        assert_eq!(refusal.code, codes::RECORD_MALFORMED);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["path_count"],
            1
        );
        assert_eq!(
            show(root.path(), &case.id).unwrap().data.case.title,
            "Readable",
            "one unreadable document does not stop reading another by name"
        );
    }

    #[test]
    fn a_case_shows_its_own_submissions_and_no_others() {
        let root = archive_root();
        let first = create(root.path(), "First", None).unwrap().data.case;
        let second = create(root.path(), "Second", None).unwrap().data.case;
        submission::add(root.path(), &first.id, "One", None, &[]).unwrap();
        submission::add(root.path(), &second.id, "Two", None, &[]).unwrap();
        let shown = show(root.path(), &first.id).unwrap().data;
        assert_eq!(shown.submission_count, 1);
        assert_eq!(shown.submissions[0].description, "One");
        assert_eq!(shown.submissions[0].case_id, first.id);
    }
}
