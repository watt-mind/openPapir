//! Importing a `case export` directory back into an archive.
//!
//! An export is a plain copy outward, so an import is a plain copy back. The
//! manifest is authoritative for what the export contains
//! (`docs/archive-layout.md`): every object it lists is re-digested from the
//! export's own bytes and every record it lists is read and parsed before the
//! archive is written to at all. An export the manifest does not describe is
//! refused rather than half read, and nothing outside the manifest is copied.
//!
//! The source directory is user-supplied and lies outside the archive, so the
//! rules the archive applies inward are applied outward here as well: a
//! symbolic link is refused rather than followed, a file that is not a
//! regular file is refused rather than read, and the path itself never
//! reaches the envelope, a message, or a detail (`docs/error-contract.md`).
//!
//! Publication is all or nothing at the record level. The whole record set is
//! probed under the writer lock, staged, and only then published, and a
//! publication that cannot be completed removes exactly what this import
//! wrote: the records it had already published and the objects it had
//! created. An interrupted import therefore leaves the whole case or nothing
//! of it, and `archive check` is clean either way.
//!
//! Nothing here verifies anything. Re-digesting an exported object is a
//! storage-layer identity check: it says the bytes are the bytes the digest
//! names, and nothing about authenticity, origin, delivery, or legal effect.

pub mod manifest;
pub mod objects;
pub mod publish;
pub mod records;

use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::archive::lock::WriterLock;
use crate::archive::{Archive, limits, paths};
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::export::KindCount;

/// The `scope` every refusal raised inside an export source carries.
///
/// It is the counterpart of the export's own `export_destination`: the path a
/// refusal would otherwise name belongs to a directory outside the archive,
/// so the refusal names the scope and the relative path instead
/// (`docs/error-contract.md`).
pub const SOURCE_SCOPE: &str = "export_source";

/// What one import of an export reports.
///
/// The source directory is deliberately not serialised, exactly as an
/// export's destination is not. It is a user-supplied path outside the
/// archive, which the privacy rule keeps out of `data`, out of `message`, and
/// out of `details`; the human form echoes the argument the user just typed,
/// and nothing else ever repeats it (`docs/error-contract.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Restored {
    /// How many bytes of newly stored objects this import read.
    pub bytes_stored: u64,
    /// The case the export holds, as the manifest names it.
    pub case_id: String,
    /// How many import events this import recorded, one per stored object.
    pub events_recorded: u64,
    /// How many objects the manifest lists.
    pub object_count: u64,
    /// How many of those the archive already held.
    pub objects_present: u64,
    /// How many of those this import stored.
    pub objects_stored: u64,
    /// How many of the export's own records this import wrote. The import
    /// events it recorded of its own are counted in `events_recorded`.
    pub record_count: u64,
    /// One entry per record kind the manifest lists, including the empty
    /// ones, in the fixed order of the kinds.
    pub records: Vec<KindCount>,
    /// How many records the archive already held, byte for byte.
    pub records_present: u64,
    /// The source exactly as the user supplied it, never serialised.
    #[serde(skip)]
    pub source: String,
}

/// Import a `case export` directory into the archive at `root`.
///
/// # Errors
///
/// Returns any archive refusal of [`Archive::open`], `usage.arguments` when
/// the source is not a usable directory or lies inside the archive root,
/// `path.symlink` for a link in the source, `export.manifest_missing`,
/// `export.manifest_malformed`, `export.object_mismatch`,
/// `export.record_missing`, `export.record_conflict`, `archive.schema_newer`
/// and `archive.schema_older` for an export of another schema version, the
/// input caps, `lock.held`, and the write refusals of the atomic write
/// procedure.
pub fn import_case(root: &Path, source: &Path) -> Result<Restored> {
    let mut warnings = Vec::new();
    match run(root, source, &mut warnings) {
        Ok(data) => Ok(Outcome { data, warnings }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn run(
    root: &Path,
    source: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Restored, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    // The resolved path is what is read; the argument the user typed is what
    // the human form echoes back, unchanged and unresolved.
    let supplied = source.to_string_lossy().into_owned();
    let source = prepare(archive.root(), source)?;
    // Everything the export claims is read and checked here, before the lock
    // is taken and before a single byte is written into the archive.
    let manifest = manifest::read(&source)?;
    let held = records::read_all(&source, &manifest)?;
    objects::verify(&source, &manifest)?;

    let _lock = WriterLock::acquire(archive.root())?;
    let plan = records::probe(archive.root(), held)?;
    let stored = objects::store_all(archive.root(), &source, &manifest)?;
    let published = match publish::run(archive.root(), &plan, &stored, warnings) {
        Ok(published) => published,
        Err(error) => {
            // The records this import had already published and the objects
            // it had just created are removed, so the archive holds the whole
            // case or nothing of it.
            publish::discard(archive.root(), &stored);
            return Err(error);
        }
    };
    Ok(Restored {
        bytes_stored: stored.bytes_stored(),
        case_id: manifest.case_id,
        events_recorded: published.events,
        object_count: manifest.objects.len() as u64,
        objects_present: stored.present(),
        objects_stored: stored.created(),
        record_count: published.records,
        records: manifest.counts,
        records_present: plan.present,
        source: supplied,
    })
}

/// Check the source directory itself, before anything in it is opened.
///
/// The rules are the export's, in the other direction. The source must be an
/// existing directory that is not a symbolic link, and it may not lie inside
/// the archive root: an import that read its own archive would be describing
/// the archive to itself rather than restoring a copy of it. Both paths are
/// resolved before they are compared, and a path that cannot be resolved at
/// all is refused rather than let through, because the question the check
/// answers is whether the source is inside the archive.
fn prepare(root: &Path, source: &Path) -> std::result::Result<PathBuf, Diagnostic> {
    if paths::is_symlink(source) {
        return Err(paths::symlink_refusal(
            Details::new().text("scope", SOURCE_SCOPE),
        ));
    }
    if !source.is_dir() {
        return Err(unusable_source());
    }
    let resolved = source.canonicalize().map_err(|_| unusable_source())?;
    let archive = root.canonicalize().map_err(|_| unusable_source())?;
    if resolved.starts_with(&archive) {
        return Err(unusable_source());
    }
    Ok(resolved)
}

/// The refusal for a source path openPapir cannot read an export from.
///
/// The path is user-supplied and outside the archive, so the argument name is
/// the whole answer and the value is never echoed.
fn unusable_source() -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "The export source is not a readable directory outside the archive.",
        Details::new().text("argument", "from"),
    )
}

/// Why one file inside the export could not be read as a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// Nothing is there at all.
    Absent,
    /// Something is there that is not a readable, bounded regular file.
    Malformed,
}

/// Read one bounded document out of the export, without following a link.
///
/// The file kind and the length are both taken from the opened handle rather
/// than from a separate look at the path, so the file that passes the checks
/// is the file whose bytes are read, and the open is the non-blocking
/// no-follow one, because a named pipe in a user-supplied directory would
/// otherwise hold the open call open forever. Every document an export holds
/// is a record document, so the record cap bounds the read.
///
/// # Errors
///
/// Returns [`Unreadable::Absent`] when nothing is there and
/// [`Unreadable::Malformed`] for anything that is not a bounded regular file.
pub fn read_document(path: &Path) -> std::result::Result<String, Unreadable> {
    let file = paths::open_no_follow_nonblocking(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => Unreadable::Absent,
        _ => Unreadable::Malformed,
    })?;
    let metadata = file.metadata().map_err(|_| Unreadable::Malformed)?;
    if !metadata.is_file() || metadata.len() > limits::MAX_RECORD_BYTES {
        return Err(Unreadable::Malformed);
    }
    let mut text = String::new();
    file.take(limits::MAX_RECORD_BYTES)
        .read_to_string(&mut text)
        .map_err(|_| Unreadable::Malformed)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;

    #[test]
    fn a_restored_case_reports_counts_and_never_serialises_its_source() {
        let restored = Restored {
            bytes_stored: 16,
            case_id: "0123456789abcdef0123456789abcdef".to_owned(),
            events_recorded: 1,
            object_count: 1,
            objects_present: 0,
            objects_stored: 1,
            record_count: 2,
            records: vec![KindCount {
                count: 1,
                kind: "case",
            }],
            records_present: 0,
            source: "/home/someone/export".to_owned(),
        };
        let json = serde_json::to_string(&restored).unwrap();
        assert!(json.contains("\"object_count\":1"));
        assert!(
            !json.contains("/home/someone/export"),
            "a user-supplied path never reaches the envelope"
        );
        assert!(!json.contains("source"));
    }

    #[test]
    fn importing_into_a_directory_that_is_no_archive_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let source = home.path().join("export");
        std::fs::create_dir(&source).unwrap();
        let failure = import_case(home.path(), &source).unwrap_err();
        assert_eq!(failure.error.code, codes::ARCHIVE_MARKER_MISSING);
    }

    #[test]
    fn a_source_that_is_not_a_usable_directory_is_a_usage_refusal() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        std::fs::create_dir(&root).unwrap();
        archive::init(&root).unwrap();
        for source in [home.path().join("absent"), home.path().join("file")] {
            let failure = import_case(&root, &source).unwrap_err();
            assert_eq!(failure.error.code, codes::USAGE_ARGUMENTS);
            let json = serde_json::to_value(&failure.error).unwrap();
            assert_eq!(json["details"]["argument"], "from");
            assert!(
                !serde_json::to_string(&failure.error)
                    .unwrap()
                    .contains("absent")
            );
        }
    }

    #[test]
    fn a_source_inside_the_archive_is_refused_before_anything_is_read() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        std::fs::create_dir(&root).unwrap();
        archive::init(&root).unwrap();
        let inside = root.join("cache");
        assert_eq!(
            import_case(&root, &inside).unwrap_err().error.code,
            codes::USAGE_ARGUMENTS
        );
    }

    #[test]
    fn a_document_that_is_not_a_bounded_regular_file_is_not_read() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("manifest.json");
        assert_eq!(read_document(&path), Err(Unreadable::Absent));
        std::fs::create_dir(&path).unwrap();
        assert_eq!(read_document(&path), Err(Unreadable::Malformed));
        let file = home.path().join("record.json");
        std::fs::write(&file, b"{}\n").unwrap();
        assert_eq!(read_document(&file).unwrap(), "{}\n");
    }
}
