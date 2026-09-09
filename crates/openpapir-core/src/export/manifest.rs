//! The export manifest: what the destination holds, and nothing else.
//!
//! The manifest is one JSON document with sorted keys and a final newline,
//! the same shape every record in the archive has, so that a diff of two
//! exports is stable. It lists every copied object with its digest and byte
//! length and every written record with its kind and identifier, and it is
//! authoritative for what the export contains.
//!
//! The manifest is the destination's own top-level document, so a write of it
//! that is interrupted reports the stage `marker_write` rather than
//! `record_write`: it describes the export rather than belonging to any one
//! record (`docs/error-contract.md`).
//!
//! It carries no original filename. An exported object is named by its digest
//! alone, and the filename the user's own import recorded stays where it has
//! always been: an attribute inside the exported import-event record
//! (`docs/archive-layout.md`).

use serde::Serialize;

use crate::archive::SUPPORTED_SCHEMA_VERSION;
use crate::clock;
use crate::error::{Details, Diagnostic, codes};
use crate::export::collect::RecordEntry;
use crate::export::copy::ObjectEntry;
use crate::export::destination::{self, Destination};

/// The manifest format's own version, independent of the archive's.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The manifest document, rendered with sorted keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Manifest<'a> {
    /// The schema version of the archive the export was taken from.
    archive_schema_version: u32,
    /// The case the export holds.
    case_id: &'a str,
    /// When openPapir wrote the export.
    exported_at: String,
    /// Every copied object, ordered by digest.
    objects: &'a [ObjectEntry],
    /// Every written record, ordered by kind and then identifier.
    records: &'a [RecordEntry],
    /// The manifest format's own version.
    schema_version: u32,
}

/// Write the manifest into the destination.
///
/// # Errors
///
/// Returns `internal.unexpected` when the manifest cannot be serialised,
/// `path.symlink`, `export.destination_conflict`, or `write.interrupted`.
pub fn write(
    destination: &Destination,
    case_id: &str,
    objects: &[ObjectEntry],
    records: &[RecordEntry],
) -> Result<(), Diagnostic> {
    let document = render(case_id, objects, records)?;
    destination.write_new(
        destination.path(),
        destination::MANIFEST_FILE,
        destination::MANIFEST_FILE,
        destination::MARKER_WRITE,
        document.as_bytes(),
    )
}

/// Render the manifest as one document with sorted keys and a final newline.
///
/// The keys are sorted by construction: the manifest is rendered through a
/// JSON value whose object is an ordered map, so the document does not depend
/// on the order the fields were declared in.
fn render(
    case_id: &str,
    objects: &[ObjectEntry],
    records: &[RecordEntry],
) -> Result<String, Diagnostic> {
    let manifest = Manifest {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        case_id,
        exported_at: clock::now_rfc3339(),
        objects,
        records,
        schema_version: MANIFEST_SCHEMA_VERSION,
    };
    let value = serde_json::to_value(&manifest).map_err(|_| {
        Diagnostic::new(
            codes::INTERNAL_UNEXPECTED,
            "An export manifest could not be serialised.",
            Details::new(),
        )
    })?;
    Ok(format!("{value}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    fn objects() -> Vec<ObjectEntry> {
        vec![ObjectEntry {
            algorithm: "sha256".to_owned(),
            byte_length: 16,
            digest: DIGEST.to_owned(),
        }]
    }

    fn records() -> Vec<RecordEntry> {
        vec![RecordEntry {
            id: "0123456789abcdef0123456789abcdef".to_owned(),
            kind: "case".to_owned(),
        }]
    }

    #[test]
    fn the_manifest_has_sorted_keys_and_lists_what_was_written() {
        let document = render("0123456789abcdef0123456789abcdef", &objects(), &records()).unwrap();
        assert!(document.ends_with('\n'));
        assert!(
            document.find("\"archive_schema_version\"") < document.find("\"case_id\""),
            "keys are sorted"
        );
        assert!(document.find("\"case_id\"") < document.find("\"exported_at\""));
        assert!(document.find("\"exported_at\"") < document.find("\"objects\""));
        assert!(document.find("\"objects\"") < document.find("\"records\""));
        assert!(document.find("\"records\"") < document.find("\"schema_version\""));

        let parsed: serde_json::Value = serde_json::from_str(&document).unwrap();
        assert_eq!(parsed["archive_schema_version"], 1);
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["objects"][0]["digest"], DIGEST);
        assert_eq!(parsed["objects"][0]["byte_length"], 16);
        assert_eq!(parsed["objects"][0]["algorithm"], "sha256");
        assert_eq!(parsed["records"][0]["kind"], "case");
        assert!(parsed["exported_at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(parsed.as_object().unwrap().len(), 6);
    }

    /// The manifest is the destination's own top-level document, so an
    /// interrupted write of it is a marker write. Unix-only, because
    /// withholding write access from the destination is a mode change.
    #[cfg(unix)]
    #[test]
    fn an_interrupted_manifest_write_reports_the_marker_write_stage() {
        use crate::error::codes;
        use std::os::unix::fs::PermissionsExt as _;

        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        std::fs::create_dir(&root).unwrap();
        let destination = home.path().join("export");
        let prepared = destination::prepare(&root, &destination).unwrap();
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o500)).unwrap();
        let refused = write(&prepared, "0123456789abcdef0123456789abcdef", &[], &[]);
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o700)).unwrap();
        let Err(refusal) = refused else {
            // The process can write anyway, which happens when the tests run
            // with privileges that ignore the permission bits.
            return;
        };
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["stage"], "marker_write");
        assert_eq!(json["details"]["scope"], "export_destination");
    }

    #[test]
    fn an_export_of_nothing_still_lists_its_empty_arrays() {
        let document = render("0123456789abcdef0123456789abcdef", &[], &[]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&document).unwrap();
        assert!(parsed["objects"].as_array().unwrap().is_empty());
        assert!(parsed["records"].as_array().unwrap().is_empty());
    }
}
