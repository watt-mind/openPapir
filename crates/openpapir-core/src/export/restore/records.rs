//! The records an export holds, and what the archive already holds of them.
//!
//! Every record the manifest lists is read from the export and parsed as a
//! record of its own kind before anything is written. Nothing is repaired,
//! guessed at, or partly accepted: a document that cannot be read as the
//! record it claims to be is `record.malformed`, exactly as it would be
//! inside an archive, and one the manifest names and the export does not hold
//! is `export.record_missing`.
//!
//! A record is restored under its original identifier. That is what makes a
//! restored case the case that was exported rather than a copy of it, and it
//! is why an identifier already taken in the target archive by a different
//! record is `export.record_conflict` rather than a silently renamed record.
//! A byte-identical record already there is not a conflict: the export
//! describes what the archive already holds, so restoring it again is the
//! same archive, which is what makes a second import of one export do
//! nothing.

use std::path::Path;

use crate::archive::import::{EVENT_KIND, ImportEvent};
use crate::error::{Details, Diagnostic, codes};
use crate::export::destination::RECORDS_DIR;
use crate::export::restore::manifest::Manifest;
use crate::export::restore::{SOURCE_SCOPE, Unreadable, linked, read_document};
use crate::records::association::Association;
use crate::records::case::Case;
use crate::records::document::{self, Record};
use crate::records::receipt::Receipt;
use crate::records::submission::Submission;

/// The record kinds an export holds, in the fixed order of the kinds.
pub const KINDS: [&str; 5] = [
    Case::KIND,
    Submission::KIND,
    Receipt::KIND,
    Association::KIND,
    EVENT_KIND,
];

/// The kind of the one record every export holds exactly one of.
pub const CASE_KIND: &str = Case::KIND;

/// The archive directory one record kind lives in, when this build knows it.
///
/// The kind is matched against openPapir's own fixed names, so a kind the
/// manifest invented never reaches a path.
#[must_use]
pub fn directory_of(kind: &str) -> Option<&'static str> {
    match kind {
        Case::KIND => Some(Case::DIRECTORY),
        Submission::KIND => Some(Submission::DIRECTORY),
        Receipt::KIND => Some(Receipt::DIRECTORY),
        Association::KIND => Some(Association::DIRECTORY),
        EVENT_KIND => Some(ImportEvent::DIRECTORY),
        _ => None,
    }
}

/// One record read out of an export, parsed as the kind it claims to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// The case itself.
    Case(Case),
    /// One submission recorded against the case.
    Submission(Submission),
    /// One receipt an association ties to one of those submissions.
    Receipt(Receipt),
    /// One association the user recorded.
    Association(Association),
    /// One import event that introduced a referenced artefact.
    ImportEvent(ImportEvent),
}

impl Held {
    /// The record's own kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Case(_) => Case::KIND,
            Self::Submission(_) => Submission::KIND,
            Self::Receipt(_) => Receipt::KIND,
            Self::Association(_) => Association::KIND,
            Self::ImportEvent(_) => EVENT_KIND,
        }
    }

    /// The archive directory this record is written into.
    #[must_use]
    pub const fn directory(&self) -> &'static str {
        match self {
            Self::Case(_) => Case::DIRECTORY,
            Self::Submission(_) => Submission::DIRECTORY,
            Self::Receipt(_) => Receipt::DIRECTORY,
            Self::Association(_) => Association::DIRECTORY,
            Self::ImportEvent(_) => ImportEvent::DIRECTORY,
        }
    }

    /// The record's own identifier, which is also its file name.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Case(record) => record.id(),
            Self::Submission(record) => record.id(),
            Self::Receipt(record) => record.id(),
            Self::Association(record) => record.id(),
            Self::ImportEvent(record) => record.id(),
        }
    }

    /// The document this record is written as, with sorted keys.
    ///
    /// It is rendered from the parsed record rather than copied from the
    /// export, so the archive only ever holds a document this build wrote
    /// itself. For a record an export of this schema version wrote, the two
    /// are the same bytes.
    ///
    /// # Errors
    ///
    /// Returns `input.cap.record_size` or `internal.unexpected`.
    pub fn document(&self) -> Result<String, Diagnostic> {
        match self {
            Self::Case(record) => document::document(record),
            Self::Submission(record) => document::document(record),
            Self::Receipt(record) => document::document(record),
            Self::Association(record) => document::document(record),
            Self::ImportEvent(record) => document::document(record),
        }
    }

    /// Whether the archive already holds exactly this record.
    ///
    /// # Errors
    ///
    /// Returns `export.record_conflict` when the identifier is taken by a
    /// different record, and `record.malformed` when what is there cannot be
    /// read as a record of this kind at all.
    fn present(&self, root: &Path) -> Result<bool, Diagnostic> {
        match self {
            Self::Case(record) => present(root, record),
            Self::Submission(record) => present(root, record),
            Self::Receipt(record) => present(root, record),
            Self::Association(record) => present(root, record),
            Self::ImportEvent(record) => present(root, record),
        }
    }
}

/// Whether one record of a known kind is already stored, byte for byte.
fn present<R: Record>(root: &Path, record: &R) -> Result<bool, Diagnostic> {
    match document::read_record::<R>(root, record.id(), "record_id") {
        Ok(stored) => {
            if document::document(&stored)? == document::document(record)? {
                Ok(true)
            } else {
                Err(conflict(R::KIND))
            }
        }
        Err(refusal) if refusal.code == codes::RECORD_NOT_FOUND => Ok(false),
        Err(refusal) => Err(refusal),
    }
}

/// Read every record the manifest lists, in the order it lists them.
///
/// # Errors
///
/// Returns `export.record_missing` when the export does not hold a record the
/// manifest names, `path.symlink` when a record's path is a symbolic link,
/// and `record.malformed` when a document it holds cannot be read as a record
/// of its kind.
pub fn read_all(source: &Path, manifest: &Manifest) -> Result<Vec<Held>, Diagnostic> {
    let mut held = Vec::with_capacity(manifest.records.len());
    for row in &manifest.records {
        let kind = kind_of(&row.kind).ok_or_else(missing_kind)?;
        let path = source
            .join(RECORDS_DIR)
            .join(kind)
            .join(format!("{}.json", row.id));
        let text = read_document(&path).map_err(|why| match why {
            Unreadable::Absent => missing(kind),
            Unreadable::Link => linked(),
            Unreadable::Malformed => document::malformed(kind, 1),
        })?;
        held.push(parse(kind, &text, &row.id).ok_or_else(|| document::malformed(kind, 1))?);
    }
    Ok(held)
}

/// Parse one document as the record kind the manifest names.
///
/// The record must claim its own kind and name the file it lives in, exactly
/// as a stored record must inside an archive.
fn parse(kind: &'static str, text: &str, id: &str) -> Option<Held> {
    fn read<R: Record>(text: &str, id: &str) -> Option<R> {
        let record: R = serde_json::from_str(text).ok()?;
        (record.record_kind() == R::KIND && record.id() == id).then_some(record)
    }
    match kind {
        Case::KIND => read::<Case>(text, id).map(Held::Case),
        Submission::KIND => read::<Submission>(text, id).map(Held::Submission),
        Receipt::KIND => read::<Receipt>(text, id).map(Held::Receipt),
        Association::KIND => read::<Association>(text, id).map(Held::Association),
        EVENT_KIND => read::<ImportEvent>(text, id).map(Held::ImportEvent),
        _ => None,
    }
}

/// The static form of a kind name this build knows.
fn kind_of(kind: &str) -> Option<&'static str> {
    KINDS.into_iter().find(|known| *known == kind)
}

/// What the record pass will write, and what it found already there.
#[derive(Debug, Default)]
pub struct Plan {
    /// Every record the archive does not hold yet, in manifest order.
    pub absent: Vec<Held>,
    /// How many records the archive already held, byte for byte.
    pub present: u64,
}

/// Sort every read record into what will be written and what is already
/// there, refusing the whole import on the first collision.
///
/// The probe runs under the writer lock and before a single record is
/// written, so a conflict leaves the archive exactly as it was.
///
/// # Errors
///
/// Returns `export.record_conflict` and `record.malformed`.
pub fn probe(root: &Path, held: Vec<Held>) -> Result<Plan, Diagnostic> {
    let mut plan = Plan::default();
    for record in held {
        if record.present(root)? {
            plan.present += 1;
        } else {
            plan.absent.push(record);
        }
    }
    Ok(plan)
}

/// The refusal for a record the manifest names and the export does not hold.
///
/// The record's identifier is not reported: the kind and the count answer
/// what the user can act on, and the identifier would name a record of an
/// export the refusal is about rather than of the archive.
fn missing(record_kind: &'static str) -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_RECORD_MISSING,
        "The export manifest names a record document the export does not hold.",
        Details::new()
            .text("scope", SOURCE_SCOPE)
            .text("record_kind", record_kind)
            .int("path_count", 1),
    )
}

/// The refusal for a manifest row whose kind reached the record pass.
///
/// The manifest check refuses an unknown kind long before this, so reaching
/// it is a violated invariant rather than a condition of the export.
fn missing_kind() -> Diagnostic {
    Diagnostic::new(
        codes::INTERNAL_UNEXPECTED,
        "A record kind reached the record pass without being checked.",
        Details::new(),
    )
}

/// The refusal for an identifier a different record already holds.
///
/// The identifier is deliberately omitted. It names a record of the target
/// archive, and the kind plus the count is what the user acts on: the two
/// records disagree, so openPapir edits neither
/// (`docs/error-contract.md`).
fn conflict(record_kind: &'static str) -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_RECORD_CONFLICT,
        "A record identifier in the export is held by a different record in the archive.",
        Details::new()
            .text("record_kind", record_kind)
            .int("conflict_count", 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::restore::manifest::RecordRow;
    use std::fs;

    const CASE: &str = "0123456789abcdef0123456789abcdef";

    fn case(title: &str) -> Case {
        Case {
            archive_schema_version: 1,
            created_at: "2026-01-17T12:00:00Z".to_owned(),
            id: CASE.to_owned(),
            notes: None,
            record_kind: "case".to_owned(),
            status: crate::records::case::Status::Open,
            tags: Vec::new(),
            title: title.to_owned(),
            updated_at: None,
        }
    }

    fn manifest_of(rows: Vec<RecordRow>) -> Manifest {
        Manifest {
            case_id: CASE.to_owned(),
            counts: Vec::new(),
            objects: Vec::new(),
            records: rows,
        }
    }

    fn export_with(source: &Path, body: &str) {
        let directory = source.join(RECORDS_DIR).join("case");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("{CASE}.json")), body).unwrap();
    }

    fn one_case() -> Manifest {
        manifest_of(vec![RecordRow {
            id: CASE.to_owned(),
            kind: "case".to_owned(),
        }])
    }

    #[test]
    fn every_known_kind_has_one_directory_and_nothing_else_does() {
        for kind in KINDS {
            assert!(directory_of(kind).is_some(), "{kind} has a directory");
        }
        assert_eq!(directory_of("verification"), None);
        assert_eq!(directory_of("../../etc"), None);
        assert_eq!(KINDS.len(), 5);
    }

    #[test]
    fn a_record_the_manifest_names_and_the_export_lacks_is_missing() {
        let source = tempfile::tempdir().unwrap();
        let refusal = read_all(source.path(), &one_case()).unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_RECORD_MISSING);
        assert_eq!(refusal.exit_code(), 4);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "case");
        assert_eq!(json["details"]["scope"], "export_source");
        assert_eq!(json["details"]["path_count"], 1);
    }

    #[test]
    fn a_document_that_is_no_record_of_its_kind_is_malformed() {
        let source = tempfile::tempdir().unwrap();
        export_with(source.path(), "{ not json");
        assert_eq!(
            read_all(source.path(), &one_case()).unwrap_err().code,
            codes::RECORD_MALFORMED
        );
        let elsewhere = document::document(&case("Tax matter"))
            .unwrap()
            .replace(CASE, "ffffffffffffffffffffffffffffffff");
        export_with(source.path(), &elsewhere);
        assert_eq!(
            read_all(source.path(), &one_case()).unwrap_err().code,
            codes::RECORD_MALFORMED,
            "a record must name the file it lives in"
        );
    }

    #[test]
    fn a_read_record_keeps_its_kind_its_identifier_and_its_document() {
        let source = tempfile::tempdir().unwrap();
        let document = document::document(&case("Tax matter")).unwrap();
        export_with(source.path(), &document);
        let held = read_all(source.path(), &one_case()).unwrap();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].kind(), "case");
        assert_eq!(held[0].id(), CASE);
        assert_eq!(held[0].directory(), Case::DIRECTORY);
        assert_eq!(held[0].document().unwrap(), document);
    }

    #[test]
    fn an_identical_record_is_present_and_a_different_one_conflicts() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(Case::DIRECTORY)).unwrap();
        let held = vec![Held::Case(case("Tax matter"))];
        let plan = probe(root.path(), held.clone()).unwrap();
        assert_eq!(plan.present, 0);
        assert_eq!(plan.absent.len(), 1);

        document::write_record(root.path(), &case("Tax matter")).unwrap();
        let plan = probe(root.path(), held).unwrap();
        assert_eq!(plan.present, 1);
        assert!(plan.absent.is_empty(), "an identical record is not written");

        let refusal = probe(root.path(), vec![Held::Case(case("Another matter"))]).unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_RECORD_CONFLICT);
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "case");
        assert_eq!(json["details"]["conflict_count"], 1);
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
        assert!(
            !serde_json::to_string(&refusal).unwrap().contains(CASE),
            "no identifier of the archive is echoed"
        );
    }
}
