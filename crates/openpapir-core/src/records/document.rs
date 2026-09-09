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

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::archive::{limits, write};
use crate::error::{Details, Diagnostic, Warning, codes};

/// The prefix the atomic write procedure gives a staging file.
///
/// A staging file is openPapir's own transient artefact rather than a record,
/// so a reader passes over one instead of reporting a malformed record. It
/// can never become a record, because a record file is named by an identifier
/// and a staging name is not one. It is not silently dropped either: a reader
/// counts it, so that a leftover from an interrupted write is visible
/// (`docs/archive-layout.md`). Removing one is a write, which only the holder
/// of the writer lock may do, so no reader ever deletes one.
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
/// Returns `record.not_found` when no such document exists,
/// `record.malformed` when the document exists but is not a regular file or
/// is not a valid record of this kind, and `input.cap.record_size` when the
/// stored document is larger than the record cap.
///
/// A stored document is untrusted input like any other. It is opened without
/// following a symbolic link, and its kind and its length are both taken from
/// the opened handle rather than from a separate look at the path, so the file
/// that is checked is the file that is read and no local writer can swap one
/// for the other in between. A path that is not a regular file is refused, and
/// one over the record cap is refused before a byte is read into memory.
pub fn read_record<R: Record>(
    root: &Path,
    id: &str,
    reference_kind: &'static str,
) -> Result<R, Diagnostic> {
    if !is_identifier(id) {
        return Err(not_found(R::KIND, reference_kind));
    }
    let path = root.join(R::DIRECTORY).join(format!("{id}.json"));
    match read_document(&path) {
        Ok(text) => parse::<R>(&text, id).ok_or_else(|| malformed(R::KIND, 1)),
        Err(Unreadable::Missing) => Err(not_found(R::KIND, reference_kind)),
        Err(Unreadable::Malformed) => Err(malformed(R::KIND, 1)),
        Err(Unreadable::TooLarge(refusal)) => Err(refusal),
    }
}

/// Read every record of one kind, in the order their identifiers sort.
///
/// Nothing is ignored silently. A directory entry that is not a well-formed
/// record of this kind makes the whole listing `record.malformed`, with the
/// number of unreadable documents. The only exception is a staging file, which
/// is openPapir's own transient artefact and never a record: it is counted
/// rather than reported, and [`visit_records_checked`] returns the count.
///
/// An entry that is not a regular file, and one larger than the record cap,
/// counts as unreadable and is refused on the opened no-follow handle before a
/// byte is read: the link is never followed and the bytes are never allocated.
/// Both therefore report `record.malformed`, the condition of the directory
/// being read, rather than a cap refusal about an input the caller did not
/// supply.
///
/// # Errors
///
/// Returns `record.malformed` when any document cannot be read as a record.
pub fn list_records<R: Record>(root: &Path) -> Result<Vec<R>, Diagnostic> {
    let mut records = Vec::new();
    let unreadable = visit_records::<R, _>(root, |record| records.push(record));
    if unreadable > 0 {
        return Err(malformed(R::KIND, unreadable));
    }
    records.sort_by(|left, right| left.id().cmp(right.id()));
    Ok(records)
}

/// What reading one record directory yielded.
///
/// The three answers are kept apart on purpose. `unreadable` counts documents
/// that were found and could not be read as a record of this kind, which is
/// `record.malformed`. `staging` counts openPapir's own leftover staging
/// files, which are not records and are not damage. `unchecked` says the
/// directory itself could not be listed, which asserts nothing about any
/// record: the reader looked and was refused, rather than looking and finding
/// nothing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Visited {
    /// How many documents could not be read as a record of this kind.
    pub unreadable: u64,
    /// How many leftover staging files the directory holds. A staging file is
    /// openPapir's own transient artefact from an interrupted write, so it is
    /// neither a record nor a malformed one. It is counted and left exactly
    /// where it is: removing it is a write, and a reader holds no lock.
    pub staging: u64,
    /// Whether the directory could not be listed at all. A directory that is
    /// not there reads as empty and leaves this `false`; any other failure to
    /// list it sets it, because the records it may hold were not read.
    pub unchecked: bool,
}

/// Read every record of one kind, one at a time, and count the unreadable.
///
/// The reader is the bounded, no-follow one [`list_records`] uses, and it
/// holds one document at a time rather than the whole directory, so a caller
/// that only needs a fixed-size key from each record never accumulates the
/// documents themselves. The return value is the number of documents that
/// could not be read as a record of this kind; a staging file is openPapir's
/// own transient artefact, so it is counted separately by
/// [`visit_records_checked`] rather than reported as a malformed record.
///
/// A directory that could not be listed reads as empty here, which is what a
/// listing wants: a record command asks for the records that are there. A
/// caller that must tell an absent directory from an unreadable one uses
/// [`visit_records_checked`] instead.
pub fn visit_records<R: Record, F: FnMut(R)>(root: &Path, visit: F) -> u64 {
    visit_records_checked::<R, F>(root, visit).unreadable
}

/// Read every record of one kind, telling an absent directory from one that
/// could not be listed.
///
/// The distinction is taken from the failure itself rather than from a second
/// look at the path, because a directory the process cannot search reports as
/// missing when it is asked whether it exists. Only `NotFound` means the
/// archive holds no records of this kind; every other failure means the
/// records were not read, and nothing may be concluded from their absence.
///
/// A leftover staging file is counted in [`Visited::staging`] and left where
/// it is, so an interrupted write is visible rather than silently passed over.
pub fn visit_records_checked<R: Record, F: FnMut(R)>(root: &Path, mut visit: F) -> Visited {
    let directory = root.join(R::DIRECTORY);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            return Visited {
                unchecked: error.kind() != io::ErrorKind::NotFound,
                ..Visited::default()
            };
        }
    };
    let mut visited = Visited::default();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(STAGING_PREFIX) {
            visited.staging += 1;
            continue;
        }
        let Some(id) = name.strip_suffix(".json").filter(|id| is_identifier(id)) else {
            visited.unreadable += 1;
            continue;
        };
        match read_document(&entry.path())
            .ok()
            .and_then(|text| parse::<R>(&text, id))
        {
            Some(record) => visit(record),
            None => visited.unreadable += 1,
        }
    }
    visited
}

/// Why a stored document could not be read as one.
enum Unreadable {
    /// Nothing is stored at that path.
    Missing,
    /// Something is stored there that is not a readable record document: a
    /// link, a directory, a device, or a file the process cannot read.
    Malformed,
    /// The document is larger than the record cap, which is the cap refusal.
    TooLarge(Diagnostic),
}

/// Read one stored document through a single opened handle.
///
/// The handle is opened without following a symbolic link, and both the file
/// kind and the length are taken from that handle, so the file that passes
/// the checks is the file whose bytes are read. Checking the path first and
/// opening it afterwards would leave a window in which a local writer could
/// put a link or a device in its place. The read is capped as well as
/// checked, so a file that grows between the two still reads no more than the
/// record cap allows.
fn read_document(path: &Path) -> Result<String, Unreadable> {
    let file = open_document(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => Unreadable::Missing,
        _ => Unreadable::Malformed,
    })?;
    let metadata = file.metadata().map_err(|_| Unreadable::Malformed)?;
    if !metadata.is_file() {
        return Err(Unreadable::Malformed);
    }
    if let Err(refusal) = limits::check_record_size(metadata.len()) {
        return Err(Unreadable::TooLarge(refusal));
    }
    let mut text = String::new();
    file.take(limits::MAX_RECORD_BYTES)
        .read_to_string(&mut text)
        .map_err(|_| Unreadable::Malformed)?;
    Ok(text)
}

/// Open a stored document without following a link and without waiting.
///
/// The no-follow rule is the archive's (`docs/archive-layout.md`). The
/// non-blocking flag is what keeps the open itself bounded: a named pipe with
/// no writer would otherwise hold the open call open forever, and a record
/// directory is untrusted input. It changes nothing for a regular file, which
/// is the only thing that gets past the check on the handle.
fn open_document(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        crate::archive::paths::open_no_follow(path)
    }
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
    fn a_stray_file_is_reported_and_a_staging_file_is_counted() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        let staging = directory.join(format!("{STAGING_PREFIX}abc"));
        fs::write(&staging, b"partial").unwrap();
        assert!(list_records::<Sample>(root.path()).unwrap().is_empty());
        let visited = visit_records_checked::<Sample, _>(root.path(), |_| unreachable!());
        assert_eq!(
            visited,
            Visited {
                staging: 1,
                ..Visited::default()
            },
            "a leftover staging file is counted rather than passed over"
        );
        assert!(
            staging.exists(),
            "a reader holds no lock and deletes nothing"
        );
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
        let visited = visit_records_checked::<Sample, _>(root.path(), |_| unreachable!());
        assert_eq!(visited, Visited::default());
        assert!(
            !visited.unchecked,
            "a directory that is not there genuinely holds no records"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_directory_that_cannot_be_listed_is_unchecked_rather_than_empty() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        write_record(root.path(), &sample(ID)).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read_dir(&directory).is_ok() {
            // The process reads the directory anyway, which happens when the
            // tests run with privileges that ignore the permission bits.
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            return;
        }
        let visited = visit_records_checked::<Sample, _>(root.path(), |_| unreachable!());
        let listed = list_records::<Sample>(root.path());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            visited,
            Visited {
                unchecked: true,
                ..Visited::default()
            }
        );
        assert!(
            listed.unwrap().is_empty(),
            "a listing still reads an unreadable directory as no records"
        );
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

    /// Create a file of `bytes` length without allocating it, to test a cap.
    fn sparse(path: &Path, bytes: u64) {
        let handle = fs::File::create(path).expect("create a sparse document");
        handle.set_len(bytes).expect("set the reported length");
    }

    #[test]
    fn a_stored_document_is_bounded_before_it_is_read() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        sparse(
            &directory.join(format!("{ID}.json")),
            limits::MAX_RECORD_BYTES + 1,
        );
        let refusal = read_record::<Sample>(root.path(), ID, "sample_id")
            .err()
            .unwrap();
        assert_eq!(
            refusal.code,
            codes::INPUT_CAP_RECORD_SIZE,
            "the cap is checked from the reported size, before any allocation"
        );
        let refusal = list_records::<Sample>(root.path()).unwrap_err();
        assert_eq!(
            refusal.code,
            codes::RECORD_MALFORMED,
            "a listing reports the directory's condition"
        );
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["path_count"],
            1
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_record_that_is_not_a_regular_file_is_never_followed() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        // A symbolic link to an endless device would hang a reader that
        // followed it, and one to a host file would read bytes the archive
        // does not own. Neither is opened.
        std::os::unix::fs::symlink("/dev/zero", directory.join(format!("{ID}.json"))).unwrap();
        assert_eq!(
            read_record::<Sample>(root.path(), ID, "sample_id")
                .err()
                .unwrap()
                .code,
            codes::RECORD_MALFORMED
        );
        assert_eq!(
            list_records::<Sample>(root.path()).unwrap_err().code,
            codes::RECORD_MALFORMED
        );

        fs::remove_file(directory.join(format!("{ID}.json"))).unwrap();
        fs::create_dir(directory.join(format!("{ID}.json"))).unwrap();
        assert_eq!(
            list_records::<Sample>(root.path()).unwrap_err().code,
            codes::RECORD_MALFORMED,
            "a directory in a record directory is not a record"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_named_pipe_is_refused_rather_than_waited_on() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Sample::DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{ID}.json"));
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .is_ok_and(|status| status.success());
        if !made {
            // The platform has no mkfifo, so there is nothing to refuse.
            return;
        }
        // A pipe with no writer would hold a blocking open forever. The
        // reader opens it without waiting and then refuses it on its kind.
        assert_eq!(
            read_record::<Sample>(root.path(), ID, "sample_id")
                .err()
                .unwrap()
                .code,
            codes::RECORD_MALFORMED
        );
        assert_eq!(
            list_records::<Sample>(root.path()).unwrap_err().code,
            codes::RECORD_MALFORMED
        );
    }
}
