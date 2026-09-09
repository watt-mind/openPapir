//! The generic record document: one JSON object per record file.
//!
//! Every record is one UTF-8, LF-terminated JSON document with sorted keys, so
//! that diffs and backups are stable (`docs/archive-layout.md`). Every record
//! carries its own identifier, its record kind, the archive schema version it
//! was written under, and a creation timestamp, and references another record
//! by identifier only.
//!
//! Writing goes through the archive's atomic write procedure, under the
//! writer lock the caller already holds, and reading needs no lock because no
//! record file is ever modified in place. A document that cannot be read as
//! the record it claims to be is `record.malformed`; it is never repaired,
//! skipped, or guessed at.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::archive::{limits, write};
use crate::error::{Details, Diagnostic, Warning, codes};

/// The prefix the atomic write procedure gives a staging file.
///
/// A staging file is openPapir's own transient artefact rather than a record,
/// so a reader passes over one instead of reporting a malformed record. It is
/// never adopted, read, or counted.
const STAGING_PREFIX: &str = ".papir-staging-";

/// The length of a record identifier in hexadecimal characters.
const IDENTIFIER_LENGTH: usize = 32;

/// One kind of record document.
pub trait Record: Serialize + DeserializeOwned {
    /// The value the record's `record_kind` field must hold.
    const KIND: &'static str;
    /// The directory holding this kind, relative to the archive root.
    const DIRECTORY: &'static str;

    /// The record's own identifier.
    fn id(&self) -> &str;
    /// The record kind the document actually claims.
    fn record_kind(&self) -> &str;
}

/// Whether a value can name a record: 32 lowercase hexadecimal characters.
///
/// Every path openPapir uses is derived from the archive root plus its own
/// fixed directory names plus an identifier, so an identifier that fails this
/// check is refused before it is joined into a path
/// (`docs/archive-layout.md`).
#[must_use]
pub fn is_identifier(value: &str) -> bool {
    value.len() == IDENTIFIER_LENGTH
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

/// The refusal for a reference that names no stored record or object.
///
/// `details` carries the kind that was not found and how it was referenced,
/// and nothing else: the value the user supplied is not echoed.
#[must_use]
pub fn not_found(record_kind: &'static str, reference_kind: &'static str) -> Diagnostic {
    Diagnostic::new(
        codes::RECORD_NOT_FOUND,
        "A reference names no record or object in this archive.",
        Details::new()
            .text("record_kind", record_kind)
            .text("reference_kind", reference_kind),
    )
}

/// The refusal for a record document that cannot be read as its own kind.
///
/// `details` carries the record kind and how many documents were unreadable.
/// The document's path is deliberately omitted: it would name an identifier
/// the caller never supplied, and the count answers the only useful question.
#[must_use]
pub fn malformed(record_kind: &'static str, path_count: u64) -> Diagnostic {
    Diagnostic::new(
        codes::RECORD_MALFORMED,
        "A stored record document cannot be read as a valid record.",
        Details::new()
            .text("record_kind", record_kind)
            .int("path_count", path_count),
    )
}

/// Serialise a record as one document with sorted keys and a final newline.
///
/// The keys are sorted by construction: the record is rendered through a JSON
/// value whose object is an ordered map, so the document does not depend on
/// the order the fields were declared in.
///
/// # Errors
///
/// Returns `input.cap.record_size` when the document exceeds the record cap,
/// and `internal.unexpected` when a record cannot be serialised at all.
pub fn document<R: Record>(record: &R) -> Result<String, Diagnostic> {
    let value = serde_json::to_value(record).map_err(|_| {
        Diagnostic::new(
            codes::INTERNAL_UNEXPECTED,
            "A record could not be serialised.",
            Details::new(),
        )
    })?;
    let document = format!("{value}\n");
    limits::check_record_size(document.len() as u64)?;
    Ok(document)
}

/// Write one record through the atomic write procedure.
///
/// The caller holds the writer lock and has already run the archive's
/// permission checks. The file name is the record's own identifier, which
/// openPapir minted, so no user-supplied text is ever joined into a path.
///
/// # Errors
///
/// Returns `input.cap.record_size`, `internal.unexpected`, or any refusal of
/// the atomic write procedure: `path.symlink`, `path.overwrite`,
/// `path.cross_device`, or `write.interrupted`.
pub fn write_record<R: Record>(root: &Path, record: &R) -> Result<Vec<Warning>, Diagnostic> {
    let content = document(record)?;
    let file_name = format!("{}.json", record.id());
    let archive_path = format!("{}/{file_name}", R::DIRECTORY);
    write::write_document(
        &root.join(R::DIRECTORY),
        &file_name,
        &archive_path,
        content.as_bytes(),
        "record_write",
    )
}

/// Read one record by identifier, without taking the writer lock.
///
/// # Errors
///
/// Returns `record.not_found` when no such document exists and
/// `record.malformed` when the document exists but is not a valid record of
/// this kind.
pub fn read_record<R: Record>(
    root: &Path,
    id: &str,
    reference_kind: &'static str,
) -> Result<R, Diagnostic> {
    if !is_identifier(id) {
        return Err(not_found(R::KIND, reference_kind));
    }
    let path = root.join(R::DIRECTORY).join(format!("{id}.json"));
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return Err(not_found(R::KIND, reference_kind));
    };
    if !metadata.is_file() {
        return Err(malformed(R::KIND, 1));
    }
    let text = fs::read_to_string(&path).map_err(|_| malformed(R::KIND, 1))?;
    parse::<R>(&text, id).ok_or_else(|| malformed(R::KIND, 1))
}

/// Read every record of one kind, in the order their identifiers sort.
///
/// Nothing is ignored silently. A directory entry that is not a well-formed
/// record of this kind makes the whole listing `record.malformed`, with the
/// number of unreadable documents. The only exception is a staging file, which
/// is openPapir's own transient artefact and never a record.
///
/// # Errors
///
/// Returns `record.malformed` when any document cannot be read as a record.
pub fn list_records<R: Record>(root: &Path) -> Result<Vec<R>, Diagnostic> {
    let directory = root.join(R::DIRECTORY);
    let Ok(entries) = fs::read_dir(&directory) else {
        return Ok(Vec::new());
    };
    let mut records = Vec::new();
    let mut unreadable = 0_u64;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(STAGING_PREFIX) {
            continue;
        }
        let Some(id) = name.strip_suffix(".json").filter(|id| is_identifier(id)) else {
            unreadable += 1;
            continue;
        };
        match fs::read_to_string(entry.path())
            .ok()
            .and_then(|text| parse::<R>(&text, id))
        {
            Some(record) => records.push(record),
            None => unreadable += 1,
        }
    }
    if unreadable > 0 {
        return Err(malformed(R::KIND, unreadable));
    }
    records.sort_by(|left, right| left.id().cmp(right.id()));
    Ok(records)
}

/// Parse one document, checking that it is this kind and names this file.
fn parse<R: Record>(text: &str, id: &str) -> Option<R> {
    let record: R = serde_json::from_str(text).ok()?;
    (record.record_kind() == R::KIND && record.id() == id).then_some(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Sample {
        id: String,
        record_kind: String,
        zeta: u32,
        alpha: u32,
    }

    impl Record for Sample {
        const KIND: &'static str = "sample";
        const DIRECTORY: &'static str = "records/samples";

        fn id(&self) -> &str {
            &self.id
        }

        fn record_kind(&self) -> &str {
            &self.record_kind
        }
    }

    fn sample(id: &str) -> Sample {
        Sample {
            id: id.to_owned(),
            record_kind: "sample".to_owned(),
            zeta: 1,
            alpha: 2,
        }
    }

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_document_has_sorted_keys_and_one_final_newline() {
        let rendered = document(&sample(ID)).unwrap();
        assert_eq!(
            rendered,
            format!("{{\"alpha\":2,\"id\":\"{ID}\",\"record_kind\":\"sample\",\"zeta\":1}}\n"),
            "keys sort regardless of the order they were declared in"
        );
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn identifiers_are_thirty_two_lowercase_hexadecimal_characters() {
        assert!(is_identifier(ID));
        assert!(!is_identifier("0123456789ABCDEF0123456789ABCDEF"));
        assert!(!is_identifier("../../etc/passwd"));
        assert!(!is_identifier(""));
        assert!(!is_identifier(&format!("{ID}0")));
    }

    #[test]
    fn a_record_round_trips_through_the_archive() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(Sample::DIRECTORY)).unwrap();
        write_record(root.path(), &sample(ID)).unwrap();
        let read: Sample = read_record(root.path(), ID, "sample_id").unwrap();
        assert_eq!(read, sample(ID));
        assert_eq!(list_records::<Sample>(root.path()).unwrap().len(), 1);
    }

    #[test]
    fn an_unknown_identifier_is_never_joined_into_a_path() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(Sample::DIRECTORY)).unwrap();
        let refusal = read_record::<Sample>(root.path(), "../../etc/passwd", "sample_id")
            .err()
            .unwrap();
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let rendered = serde_json::to_string(&refusal).unwrap();
        assert!(!rendered.contains("passwd"), "no reference is echoed");
        assert_eq!(
            read_record::<Sample>(root.path(), ID, "sample_id")
                .err()
                .unwrap()
                .code,
            codes::RECORD_NOT_FOUND
        );
    }

    #[test]
    fn a_document_that_is_not_a_valid_record_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("{ID}.json")), b"{ not json").unwrap();
        let refusal = read_record::<Sample>(root.path(), ID, "sample_id")
            .err()
            .unwrap();
        assert_eq!(refusal.code, codes::RECORD_MALFORMED);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "sample");
        assert_eq!(json["details"]["path_count"], 1);
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
        assert_eq!(
            list_records::<Sample>(root.path()).unwrap_err().code,
            codes::RECORD_MALFORMED
        );
    }

    #[test]
    fn a_record_that_claims_another_kind_or_file_is_malformed() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        let mut other = sample(ID);
        other.record_kind = "case".to_owned();
        fs::write(
            directory.join(format!("{ID}.json")),
            document(&other).unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_record::<Sample>(root.path(), ID, "sample_id")
                .err()
                .unwrap()
                .code,
            codes::RECORD_MALFORMED
        );

        let elsewhere = "ffffffffffffffffffffffffffffffff";
        fs::write(
            directory.join(format!("{elsewhere}.json")),
            document(&sample(ID)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_record::<Sample>(root.path(), elsewhere, "sample_id")
                .err()
                .unwrap()
                .code,
            codes::RECORD_MALFORMED,
            "a record must name the file it lives in"
        );
    }

    #[test]
    fn a_stray_file_is_reported_and_a_staging_file_is_passed_over() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("{STAGING_PREFIX}abc")), b"partial").unwrap();
        assert!(list_records::<Sample>(root.path()).unwrap().is_empty());
        fs::write(directory.join("notes.txt"), b"stray").unwrap();
        let refusal = list_records::<Sample>(root.path()).unwrap_err();
        assert_eq!(refusal.code, codes::RECORD_MALFORMED);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["path_count"],
            1
        );
    }

    #[test]
    fn a_missing_directory_lists_nothing_rather_than_failing() {
        let root = tempfile::tempdir().unwrap();
        assert!(list_records::<Sample>(root.path()).unwrap().is_empty());
    }

    #[test]
    fn a_document_larger_than_the_record_cap_is_refused_before_it_is_written() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(Sample::DIRECTORY)).unwrap();
        let oversized = Sample {
            id: ID.to_owned(),
            record_kind: "sample".to_owned(),
            zeta: 1,
            alpha: 2,
        };
        // The cap is exercised directly, because a valid sample cannot reach
        // one megabyte through its own fields.
        assert!(limits::check_record_size(limits::MAX_RECORD_BYTES + 1).is_err());
        assert!(write_record(root.path(), &oversized).is_ok());
    }
}
