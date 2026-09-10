//! Exporting a whole archive, in the shape `case export` writes.
//!
//! A whole-archive export is the same plain copy outward as a case export,
//! taken over everything the archive holds rather than over one case: every
//! object in the store, every record of every kind, one manifest, and a copy
//! of the archive marker, so that the schema version the export was taken
//! under travels with it (`docs/archive-layout.md`).
//!
//! The objects come from the store itself rather than from what the records
//! reference. That is the difference that makes the export a whole archive: an
//! object no record names is still the user's own bytes, and a copy that left
//! it behind would be a smaller archive rather than the same one. The two
//! sets are still compared, so a record naming an object the store does not
//! hold is `record.not_found`, exactly as it is for one case.
//!
//! Nothing here interprets a record, and nothing is converted, re-encoded,
//! normalised, compressed, or encrypted. The archive is opened read-only and
//! is never modified. [`restore::import_archive`] holds the other direction.
//!
//! [`restore::import_archive`]: crate::export::restore::import_archive

use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::path::Path;

use serde::Serialize;

use crate::archive::{Archive, MARKER_FILE, limits, objects, paths};
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::export::destination::{self, Destination};
use crate::export::{KindCount, collect, copy, destination as dest, manifest};
use crate::records::is_digest;

/// The number of hexadecimal characters in one fan-out directory's name.
const FAN_OUT: usize = 2;

/// What one whole-archive export reports.
///
/// The destination is deliberately not serialised, exactly as a case export's
/// is not: it is a user-supplied path outside the archive, which the privacy
/// rule keeps out of `data`, out of `message`, and out of `details`
/// (`docs/error-contract.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveExported {
    /// How many bytes were copied into the destination.
    pub bytes_copied: u64,
    /// How many case records the export holds.
    pub case_count: u64,
    /// How many objects were copied.
    pub object_count: u64,
    /// How many records were written.
    pub record_count: u64,
    /// One entry per record kind, ordered by kind, including the empty ones.
    pub records: Vec<KindCount>,
    /// The destination exactly as the user supplied it, never serialised.
    #[serde(skip)]
    pub destination: String,
}

/// Export a whole archive to a destination directory outside the archive.
///
/// # Errors
///
/// Returns any archive refusal of [`Archive::open_read_only`],
/// `record.malformed` for an unreadable record document, `record.not_found`
/// when a record references an object this archive does not hold,
/// `integrity.digest_mismatch` for an entry under `objects/` that is not an
/// object filed under its own name, `usage.arguments` when the destination is
/// unusable or lies inside the archive root, `path.symlink` for a link in the
/// store or the destination, `export.destination_conflict` when the
/// destination is not empty or already holds a target path,
/// `export.copy_mismatch` when a copy re-digests to something else,
/// `input.cap.file_size` for a stored object over the single-file cap, and
/// `write.interrupted` when a copy cannot be completed.
pub fn export_archive(root: &Path, destination: &Path) -> Result<ArchiveExported> {
    let mut warnings = Vec::new();
    match run(root, destination, &mut warnings) {
        Ok(data) => Ok(Outcome { data, warnings }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn run(
    root: &Path,
    destination: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<ArchiveExported, Diagnostic> {
    let mut archive = Archive::open_read_only(root)?;
    warnings.extend(archive.take_warnings());
    let all = collect::gather_all(archive.root())?;
    let digests = list_digests(archive.root())?;
    refuse_dangling(&all, &digests)?;
    let marker = read_marker(archive.root())?;
    let prepared = destination::prepare(archive.root(), destination)?;
    // Everything past this point may have put something in the destination,
    // so a refusal removes exactly what this export created before it is
    // reported. A retry then meets the destination it met the first time.
    let written = write_export(&prepared, archive.root(), &all, &digests, &marker);
    let (objects, records) = match written {
        Ok(written) => written,
        Err(error) => {
            prepared.discard();
            return Err(error);
        }
    };
    Ok(ArchiveExported {
        bytes_copied: objects.iter().map(|object| object.byte_length).sum(),
        case_count: all.cases.len() as u64,
        object_count: objects.len() as u64,
        record_count: records.entries.len() as u64,
        records: records.counts,
        destination: destination.to_string_lossy().into_owned(),
    })
}

#[allow(clippy::type_complexity)]
fn write_export(
    prepared: &Destination,
    root: &Path,
    all: &collect::All,
    digests: &BTreeSet<String>,
    marker: &[u8],
) -> std::result::Result<(Vec<copy::ObjectEntry>, collect::Written), Diagnostic> {
    let objects = copy::copy_objects(root, prepared, digests)?;
    let records = collect::write_everything(prepared, all)?;
    prepared.write_new(
        prepared.path(),
        MARKER_FILE,
        MARKER_FILE,
        dest::MARKER_WRITE,
        marker,
    )?;
    manifest::write(
        prepared,
        manifest::ARCHIVE_SCOPE,
        None,
        &objects,
        &records.entries,
    )?;
    Ok((objects, records))
}

/// Read the archive marker, so that the schema version travels with the copy.
///
/// The marker is copied byte for byte rather than rendered again, so what the
/// export holds is what the archive holds. It has already been parsed by
/// [`Archive::open_read_only`], so this reads a document the archive itself
/// accepted.
fn read_marker(root: &Path) -> std::result::Result<Vec<u8>, Diagnostic> {
    let path = root.join(MARKER_FILE);
    if paths::is_symlink(&path) {
        return Err(paths::symlink_refusal(
            Details::new()
                .text("scope", "archive")
                .text("archive_path", MARKER_FILE),
        ));
    }
    let file = paths::open_no_follow(&path).map_err(|_| unreadable_marker())?;
    // A marker over the record cap is refused rather than copied short: a
    // truncated copy would be a marker the export invented, and the export
    // would claim a schema version no document in the destination holds. The
    // cap is checked from the opened handle and again from the bytes read, so
    // a marker that grew between the two is refused rather than cut, which is
    // why one byte more than the cap is read.
    limits::check_record_size(file.metadata().map_err(|_| unreadable_marker())?.len())?;
    let mut bytes = Vec::new();
    file.take(limits::MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable_marker())?;
    limits::check_record_size(bytes.len() as u64)?;
    Ok(bytes)
}

/// Refuse a record that names an object the store does not hold.
///
/// It is the whole-archive form of the case export's own refusal: an export
/// that copied a record naming bytes nobody has would describe an archive
/// this one is not. The digest is never echoed, exactly as it is not when the
/// user supplied one.
fn refuse_dangling(
    all: &collect::All,
    digests: &BTreeSet<String>,
) -> std::result::Result<(), Diagnostic> {
    if all.digests().is_subset(digests) {
        return Ok(());
    }
    Err(crate::records::document::not_found(
        "artefact",
        "artefact_digest",
    ))
}

/// Every object the store holds, by its bare hexadecimal digest.
///
/// The store is walked rather than derived from the records, because a whole
/// archive is what it holds. An entry that is not an object filed under its
/// own name is refused rather than skipped: an export that passed over it
/// would quietly write a smaller archive than the one it was pointed at.
///
/// # Errors
///
/// Returns `path.symlink` for a link inside the store,
/// `integrity.digest_mismatch` for an entry that is not an object filed under
/// its own name, and `write.interrupted` when a directory cannot be listed.
fn list_digests(root: &Path) -> std::result::Result<BTreeSet<String>, Diagnostic> {
    let base = root.join(objects::OBJECTS_DIR).join(objects::ALGORITHM);
    let mut digests = BTreeSet::new();
    for high in entries(&base)? {
        let high_name = fan_out_name(&high)?;
        for low in entries(&high)? {
            let low_name = fan_out_name(&low)?;
            for object in entries(&low)? {
                digests.insert(object_digest(&object, &high_name, &low_name)?);
            }
        }
    }
    Ok(digests)
}

/// The paths one directory in the store holds, in a fixed order.
///
/// A path that is not there at all is the store an archive with no objects
/// has, so it reads as empty. A path that is there and is not a directory is
/// a malformed entry, and every other failure to list one means the objects
/// under it were not read, which no export may pass over.
fn entries(path: &Path) -> std::result::Result<Vec<std::path::PathBuf>, Diagnostic> {
    refuse_link(path)?;
    let listing = match fs::read_dir(path) {
        Ok(listing) => listing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => {
            return Err(malformed_entry());
        }
        Err(_) => return Err(unreadable_store()),
    };
    let mut paths = Vec::new();
    for entry in listing {
        paths.push(entry.map_err(|_| unreadable_store())?.path());
    }
    paths.sort();
    Ok(paths)
}

/// The name of one fan-out directory, which is two lowercase hexadecimal
/// characters and nothing else.
fn fan_out_name(path: &Path) -> std::result::Result<String, Diagnostic> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(malformed_entry)?;
    if name.len() != FAN_OUT || !name.chars().all(hexadecimal) {
        return Err(malformed_entry());
    }
    Ok(name.to_owned())
}

/// The digest one stored object's path names, checked against that path.
///
/// An object's expected digest is its own path, so a name that is not a
/// digest, or one filed under fan-out directories that do not match it, is a
/// malformed object entry (`docs/error-contract.md`).
fn object_digest(path: &Path, high: &str, low: &str) -> std::result::Result<String, Diagnostic> {
    refuse_link(path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(malformed_entry)?;
    if !is_digest(name) || &name[0..FAN_OUT] != high || &name[FAN_OUT..FAN_OUT * 2] != low {
        return Err(malformed_entry());
    }
    if !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(malformed_entry());
    }
    Ok(name.to_owned())
}

/// Refuse a symbolic link anywhere inside the store.
fn refuse_link(path: &Path) -> std::result::Result<(), Diagnostic> {
    if paths::is_symlink(path) {
        return Err(paths::symlink_refusal(
            Details::new()
                .text("scope", "archive")
                .text("archive_path", store_path()),
        ));
    }
    Ok(())
}

/// The store's own path relative to the archive root.
///
/// It is the whole answer a refusal about the store gives. The entry's own
/// name is never echoed: a malformed entry's name is not a digest openPapir
/// minted, so it is not openPapir's to publish.
fn store_path() -> String {
    format!("{}/{}", objects::OBJECTS_DIR, objects::ALGORITHM)
}

/// The refusal for an entry under `objects/` that is not an object.
fn malformed_entry() -> Diagnostic {
    Diagnostic::new(
        codes::INTEGRITY_DIGEST_MISMATCH,
        "An entry in the artefact store is not an object filed under its own digest.",
        Details::new().int("count", 1),
    )
}

/// The refusal for an archive marker that could not be read.
///
/// The marker is the archive's own top-level document, so an interrupted
/// read of it reports the stage the write-stage table gives the marker rather
/// than the one it gives an object (`docs/error-contract.md`).
fn unreadable_marker() -> Diagnostic {
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "The archive marker could not be read for the export.",
        Details::new()
            .text("stage", dest::MARKER_WRITE)
            .text("archive_path", MARKER_FILE),
    )
    .retryable()
}

/// The refusal for a part of the store that could not be read.
fn unreadable_store() -> Diagnostic {
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "The artefact store could not be read for the export.",
        Details::new().text("stage", dest::OBJECT_WRITE),
    )
    .retryable()
}

/// Whether one character is a lowercase hexadecimal digit.
fn hexadecimal(character: char) -> bool {
    character.is_ascii_hexdigit() && !character.is_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    /// A store with one entry at the path the arguments spell.
    fn store_with(high: &str, low: &str, name: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let directory = root
            .path()
            .join(objects::OBJECTS_DIR)
            .join(objects::ALGORITHM)
            .join(high)
            .join(low);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(name), b"synthetic\n").unwrap();
        root
    }

    #[test]
    fn an_export_reports_counts_and_never_serialises_its_destination() {
        let exported = ArchiveExported {
            bytes_copied: 16,
            case_count: 2,
            object_count: 1,
            record_count: 3,
            records: vec![KindCount {
                count: 2,
                kind: "case",
            }],
            destination: "/home/someone/export".to_owned(),
        };
        let json = serde_json::to_string(&exported).unwrap();
        assert!(json.contains("\"case_count\":2"));
        assert!(
            !json.contains("/home/someone/export"),
            "a user-supplied path never reaches the envelope"
        );
        assert!(!json.contains("destination"));
    }

    #[test]
    fn exporting_from_a_directory_that_is_no_archive_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let failure = export_archive(home.path(), &home.path().join("out")).unwrap_err();
        assert_eq!(failure.error.code, codes::ARCHIVE_MARKER_MISSING);
        assert!(
            !home.path().join("out").exists(),
            "nothing is created before the archive is opened"
        );
    }

    #[test]
    fn a_store_that_is_not_there_holds_no_objects() {
        let root = tempfile::tempdir().unwrap();
        assert!(list_digests(root.path()).unwrap().is_empty());
    }

    #[test]
    fn every_object_the_store_holds_is_listed_by_its_own_digest() {
        let root = store_with(&DIGEST[0..2], &DIGEST[2..4], DIGEST);
        assert_eq!(
            list_digests(root.path()).unwrap(),
            BTreeSet::from([DIGEST.to_owned()])
        );
    }

    #[test]
    fn an_entry_that_is_not_an_object_filed_under_its_own_name_is_refused() {
        for (high, low, name) in [
            (&DIGEST[0..2], &DIGEST[2..4], "not-a-digest"),
            ("zz", &DIGEST[2..4], DIGEST),
            (&DIGEST[0..2], "ff", DIGEST),
            (&DIGEST[0..2], &DIGEST[2..4], &DIGEST.to_uppercase()[..]),
        ] {
            let root = store_with(high, low, name);
            let refusal = list_digests(root.path()).unwrap_err();
            assert_eq!(refusal.code, codes::INTEGRITY_DIGEST_MISMATCH);
            let json = serde_json::to_value(&refusal).unwrap();
            assert_eq!(json["details"]["count"], 1);
            assert!(
                !serde_json::to_string(&refusal).unwrap().contains(name),
                "the name of an entry openPapir did not mint is never echoed"
            );
        }
    }

    #[test]
    fn a_record_naming_an_object_the_store_does_not_hold_is_not_found() {
        let all = collect::All {
            receipts: vec![crate::records::receipt::Receipt {
                archive_schema_version: 1,
                artefact_digest: format!("sha256:{DIGEST}"),
                created_at: "2026-01-17T12:00:00Z".to_owned(),
                id: "0123456789abcdef0123456789abcdef".to_owned(),
                import_event_id: "fedcba9876543210fedcba9876543210".to_owned(),
                label: None,
                record_kind: "receipt".to_owned(),
            }],
            ..collect::All::default()
        };
        let refusal = refuse_dangling(&all, &BTreeSet::new()).unwrap_err();
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        assert!(!serde_json::to_string(&refusal).unwrap().contains(DIGEST));
        refuse_dangling(&all, &BTreeSet::from([DIGEST.to_owned()])).unwrap();
    }

    /// A file where a fan-out directory should be is not a store to walk.
    #[test]
    fn a_file_in_place_of_a_fan_out_directory_is_a_malformed_entry() {
        let root = tempfile::tempdir().unwrap();
        let base = root
            .path()
            .join(objects::OBJECTS_DIR)
            .join(objects::ALGORITHM);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join(&DIGEST[0..2]), b"not a directory\n").unwrap();
        assert_eq!(
            list_digests(root.path()).unwrap_err().code,
            codes::INTEGRITY_DIGEST_MISMATCH
        );
    }

    /// A link in the store is refused rather than followed, and the refusal
    /// names the store rather than the entry.
    #[cfg(unix)]
    #[test]
    fn a_link_in_the_store_is_refused_wherever_it_is() {
        let root = tempfile::tempdir().unwrap();
        let base = root
            .path()
            .join(objects::OBJECTS_DIR)
            .join(objects::ALGORITHM);
        fs::create_dir_all(&base).unwrap();
        std::os::unix::fs::symlink(root.path(), base.join(&DIGEST[0..2])).unwrap();
        let refusal = list_digests(root.path()).unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["archive_path"], "objects/sha256");
    }

    /// A marker that is a link is refused rather than followed, and one that
    /// cannot be opened at all is a retryable read refusal.
    #[cfg(unix)]
    #[test]
    fn a_marker_that_cannot_be_copied_is_refused_rather_than_invented() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        let elsewhere = home.path().join("elsewhere");
        fs::write(&elsewhere, b"{}\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join(MARKER_FILE)).unwrap();
        assert_eq!(read_marker(&root).unwrap_err().code, codes::PATH_SYMLINK);

        fs::remove_file(root.join(MARKER_FILE)).unwrap();
        let refusal = read_marker(&root).unwrap_err();
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert!(refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["stage"], "marker_write");
        assert_eq!(json["details"]["archive_path"], MARKER_FILE);
    }

    /// A marker over the record cap is refused rather than copied short.
    #[test]
    fn a_marker_over_the_record_cap_is_refused_rather_than_truncated() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        let oversized = vec![b' '; limits::MAX_RECORD_BYTES as usize + 1];
        fs::write(root.join(MARKER_FILE), &oversized).unwrap();
        assert_eq!(
            read_marker(&root).unwrap_err().code,
            codes::INPUT_CAP_RECORD_SIZE
        );
    }

    /// A store directory the process cannot list stops the export rather than
    /// reading as empty: the objects under it were not read.
    #[cfg(unix)]
    #[test]
    fn a_store_directory_that_cannot_be_listed_stops_the_export() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = store_with(&DIGEST[0..2], &DIGEST[2..4], DIGEST);
        let closed = root
            .path()
            .join(objects::OBJECTS_DIR)
            .join(objects::ALGORITHM)
            .join(&DIGEST[0..2]);
        fs::set_permissions(&closed, fs::Permissions::from_mode(0o000)).unwrap();
        let listed = list_digests(root.path());
        fs::set_permissions(&closed, fs::Permissions::from_mode(0o700)).unwrap();
        let Err(refusal) = listed else {
            // The process can list it anyway, which happens when the tests run
            // with privileges that ignore the permission bits.
            return;
        };
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
    }

    /// A destination this export could not finish writing is emptied again,
    /// so a retry meets the destination the first attempt met.
    #[cfg(unix)]
    #[test]
    fn a_refused_export_removes_exactly_what_it_created() {
        use std::os::unix::fs::PermissionsExt as _;

        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        archive::init(&root).unwrap();
        let destination = home.path().join("export");
        fs::create_dir(&destination).unwrap();
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o500)).unwrap();
        let refused = export_archive(&root, &destination);
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
        let Err(failure) = refused else {
            return;
        };
        assert_eq!(failure.error.code, codes::WRITE_INTERRUPTED);
        assert_eq!(
            fs::read_dir(&destination).unwrap().count(),
            0,
            "the destination the user made is left empty"
        );
    }

    #[test]
    fn an_empty_archive_exports_its_marker_and_an_empty_manifest() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        archive::init(&root).unwrap();
        let destination = home.path().join("export");
        let exported = export_archive(&root, &destination).unwrap();
        assert_eq!(exported.data.case_count, 0);
        assert_eq!(exported.data.object_count, 0);
        assert_eq!(exported.data.record_count, 0);
        let marker = fs::read(destination.join(MARKER_FILE)).unwrap();
        assert_eq!(marker, fs::read(root.join(MARKER_FILE)).unwrap());
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["export_scope"], "archive");
        assert!(manifest.as_object().unwrap().get("case_id").is_none());
    }
}
