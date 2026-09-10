//! Exporting one case, and the permission repair a restore needs.
//!
//! An export is a plain copy outward. Every object a case references is
//! copied byte for byte to `<destination>/objects/<digest>`, every record
//! that belongs to the case is written as JSON under
//! `<destination>/records/<kind>/<id>.json`, and `manifest.json` lists what
//! was written. Nothing is converted, re-encoded, normalised, compressed, or
//! encrypted, and the result is readable without openPapir: the objects are
//! the original bytes and the records are readable JSON
//! (`docs/archive-layout.md`).
//!
//! The archive is opened read-only and is never modified: no lock is taken,
//! no layout directory is created, and nothing is written inside the root.
//! The path rules apply outward as well. A symbolic link in the destination
//! is refused rather than followed, an existing file there is refused rather
//! than replaced, and a destination inside the archive root is refused
//! outright.
//!
//! Every copy is re-digested while it is written and compared with the digest
//! its source path names. A digest is a storage-layer identity only: a
//! matching copy holds the same bytes, and that says nothing about
//! authenticity, origin, delivery, or legal effect.
//!
//! [`restore`] holds the other direction: reading an export directory back
//! into an archive. It is the same plain copy inward, checked against the
//! manifest before anything is written, and it is the only thing in this
//! module that writes inside the archive root.
//!
//! [`repair`] holds the permission repair, because restoring an export or a
//! backup with ordinary copy tooling is what widens an archive's permissions
//! in the first place.

pub mod collect;
pub mod copy;
pub mod destination;
pub mod manifest;
pub mod repair;
pub mod restore;
pub mod whole;

use std::path::Path;

use serde::Serialize;

use crate::archive::Archive;
use crate::error::{Diagnostic, Failure, Outcome, Result, Warning};

/// One kind and a count of it, used for records written and paths narrowed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KindCount {
    /// How many of this kind the operation reports.
    pub count: u64,
    /// The kind's stable name.
    pub kind: &'static str,
}

/// What one export reports.
///
/// The destination is deliberately not serialised. It is a user-supplied path
/// outside the archive, which the privacy rule keeps out of `data`, out of
/// `message`, and out of `details`; the human form echoes the argument the
/// user just typed, and nothing else ever repeats it
/// (`docs/error-contract.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Exported {
    /// How many bytes were copied into the destination.
    pub bytes_copied: u64,
    /// The case that was exported.
    pub case_id: String,
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

/// Export one case to a destination directory outside the archive.
///
/// # Errors
///
/// Returns any archive refusal of [`Archive::open_read_only`],
/// `record.not_found` when the case identifier names no case or a record
/// references an object this archive does not hold, `record.malformed` for an
/// unreadable record document, `usage.arguments` when the destination is
/// unusable or lies inside the archive root, `path.symlink` for a link in the
/// destination, `export.destination_conflict` when the destination is not
/// empty or already holds a target path, `export.copy_mismatch` when a copy
/// re-digests to something else, and `write.interrupted` when a copy cannot
/// be completed.
pub fn export_case(root: &Path, case_id: &str, destination: &Path) -> Result<Exported> {
    let mut warnings = Vec::new();
    match run(root, case_id, destination, &mut warnings) {
        Ok(data) => Ok(Outcome { data, warnings }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

#[allow(clippy::type_complexity)]
fn write_export(
    prepared: &destination::Destination,
    root: &Path,
    collected: &collect::Collected,
) -> std::result::Result<(Vec<copy::ObjectEntry>, collect::Written), Diagnostic> {
    let objects = copy::copy_objects(root, prepared, &collected.digests)?;
    let records = collect::write_records(prepared, collected)?;
    manifest::write(
        prepared,
        manifest::CASE_SCOPE,
        Some(&collected.case.id),
        &objects,
        &records.entries,
    )?;
    Ok((objects, records))
}

fn run(
    root: &Path,
    case_id: &str,
    destination: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Exported, Diagnostic> {
    let mut archive = Archive::open_read_only(root)?;
    warnings.extend(archive.take_warnings());
    let collected = collect::gather(archive.root(), case_id)?;
    let prepared = destination::prepare(archive.root(), destination)?;
    // Everything past this point may have put something in the destination,
    // so a refusal removes exactly what this export created before it is
    // reported. A retry then meets the destination it met the first time.
    let written = write_export(&prepared, archive.root(), &collected);
    let (objects, records) = match written {
        Ok(written) => written,
        Err(error) => {
            prepared.discard();
            return Err(error);
        }
    };
    Ok(Exported {
        bytes_copied: objects.iter().map(|object| object.byte_length).sum(),
        case_id: collected.case.id,
        object_count: objects.len() as u64,
        record_count: records.entries.len() as u64,
        records: records.counts,
        destination: destination.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::codes;

    #[test]
    fn an_export_reports_counts_and_never_serialises_its_destination() {
        let exported = Exported {
            bytes_copied: 16,
            case_id: "0123456789abcdef0123456789abcdef".to_owned(),
            object_count: 1,
            record_count: 2,
            records: vec![KindCount {
                count: 1,
                kind: "case",
            }],
            destination: "/home/someone/export".to_owned(),
        };
        let json = serde_json::to_string(&exported).unwrap();
        assert!(json.contains("\"object_count\":1"));
        assert!(
            !json.contains("/home/someone/export"),
            "a user-supplied path never reaches the envelope"
        );
        assert!(!json.contains("destination"));
    }

    #[test]
    fn exporting_from_a_directory_that_is_no_archive_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let failure = export_case(
            home.path(),
            "0123456789abcdef0123456789abcdef",
            &home.path().join("out"),
        )
        .unwrap_err();
        assert_eq!(failure.error.code, codes::ARCHIVE_MARKER_MISSING);
        assert!(
            !home.path().join("out").exists(),
            "nothing is created before the archive is opened"
        );
    }
}
