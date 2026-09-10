//! Reading the export manifest, which is authoritative for the export.
//!
//! The manifest says which objects and which records the export holds
//! (`docs/archive-layout.md`). An import reads nothing the manifest does not
//! list, so a file dropped into the directory afterwards is neither copied
//! nor reported, and a manifest that lists something the directory does not
//! hold is a refusal rather than a smaller import.
//!
//! Everything the document claims is checked before it is believed: the
//! manifest format's own version, the archive schema version the export was
//! taken from, the shape of every digest and identifier, and the record kinds
//! this build knows. A manifest that fails any of those is
//! `export.manifest_malformed`, and the archive is untouched.

use std::path::Path;

use serde::Deserialize;

use crate::archive;
use crate::error::{Details, Diagnostic, codes};
use crate::export::KindCount;
use crate::export::destination::MANIFEST_FILE;
use crate::export::manifest::MANIFEST_SCHEMA_VERSION;
use crate::export::restore::{SOURCE_SCOPE, Unreadable, read_document, records};
use crate::records::{document, is_digest};

/// One object the manifest lists.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ObjectRow {
    /// The digest algorithm, which is always the store's own.
    pub algorithm: String,
    /// The number of bytes the exported copy holds.
    pub byte_length: u64,
    /// The lowercase hexadecimal digest, which is also the copy's file name.
    pub digest: String,
}

/// One record the manifest lists.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RecordRow {
    /// The record's own identifier, which is also the file's name.
    pub id: String,
    /// The record kind, which is also the directory the file lives in.
    pub kind: String,
}

/// The manifest document as it is stored.
#[derive(Debug, Deserialize)]
struct Document {
    /// The schema version of the archive the export was taken from.
    archive_schema_version: u32,
    /// The case the export holds.
    case_id: String,
    /// Every copied object.
    objects: Vec<ObjectRow>,
    /// Every written record.
    records: Vec<RecordRow>,
    /// The manifest format's own version.
    schema_version: u32,
}

/// One export's manifest, checked and ready to be acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The case the export holds.
    pub case_id: String,
    /// Every object the export holds, in the order the manifest lists them.
    pub objects: Vec<ObjectRow>,
    /// Every record the export holds, in the order the manifest lists them.
    pub records: Vec<RecordRow>,
    /// One count per record kind, in the fixed order of the kinds.
    pub counts: Vec<KindCount>,
}

/// Read and check the manifest at the root of the export.
///
/// # Errors
///
/// Returns `export.manifest_missing` when the export holds no manifest,
/// `export.manifest_malformed` when it holds one this build cannot read as a
/// manifest, and `archive.schema_newer` or `archive.schema_older` when the
/// export was taken from an archive of another schema version.
pub fn read(source: &Path) -> Result<Manifest, Diagnostic> {
    let text = read_document(&source.join(MANIFEST_FILE)).map_err(|why| match why {
        Unreadable::Absent => missing(),
        Unreadable::Malformed => malformed(),
    })?;
    let document: Document = serde_json::from_str(&text).map_err(|_| malformed())?;
    check(&document)?;
    Ok(Manifest {
        case_id: document.case_id,
        counts: counts(&document.records),
        objects: document.objects,
        records: document.records,
    })
}

/// Check everything the manifest claims about itself.
///
/// The schema versions come first, because a manifest from a build this one
/// does not support is refused by the schema rules rather than reported as
/// malformed: it is a document this build has no business reading, not a
/// broken one.
fn check(document: &Document) -> Result<(), Diagnostic> {
    if document.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(malformed());
    }
    archive::check_schema_version(document.archive_schema_version)?;
    if !document::is_identifier(&document.case_id) {
        return Err(malformed());
    }
    check_objects(&document.objects)?;
    check_records(&document.case_id, &document.records)
}

/// Check every object row: its algorithm, its digest, and its uniqueness.
fn check_objects(objects: &[ObjectRow]) -> Result<(), Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    for object in objects {
        if object.algorithm != crate::archive::objects::ALGORITHM
            || !is_digest(&object.digest)
            || !seen.insert(object.digest.as_str())
        {
            return Err(malformed());
        }
    }
    Ok(())
}

/// Check every record row, and that the manifest names its own case once.
///
/// A manifest that lists no case record, or more than one, describes no case
/// this import could restore, so it is refused here rather than part way
/// through the record pass.
fn check_records(case_id: &str, rows: &[RecordRow]) -> Result<(), Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    let mut cases = 0_u64;
    for row in rows {
        if !document::is_identifier(&row.id)
            || records::directory_of(&row.kind).is_none()
            || !seen.insert((row.kind.as_str(), row.id.as_str()))
        {
            return Err(malformed());
        }
        if row.kind == records::CASE_KIND {
            cases += 1;
            if row.id != case_id {
                return Err(malformed());
            }
        }
    }
    if cases != 1 {
        return Err(malformed());
    }
    Ok(())
}

/// One count per record kind, in the fixed order of the kinds.
fn counts(rows: &[RecordRow]) -> Vec<KindCount> {
    records::KINDS
        .iter()
        .map(|kind| KindCount {
            count: rows.iter().filter(|row| row.kind == *kind).count() as u64,
            kind,
        })
        .collect()
}

/// The refusal for an export directory that holds no manifest.
fn missing() -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_MANIFEST_MISSING,
        "The export directory holds no manifest, so it describes no case.",
        Details::new()
            .text("scope", SOURCE_SCOPE)
            .text("export_path", MANIFEST_FILE),
    )
}

/// The refusal for a manifest this build cannot read as a manifest.
///
/// The document's own content is never echoed: it is a file the user pointed
/// openPapir at, so the path relative to the source is the whole answer.
fn malformed() -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_MANIFEST_MALFORMED,
        "The export manifest cannot be read as a manifest of this format.",
        Details::new()
            .text("scope", SOURCE_SCOPE)
            .text("export_path", MANIFEST_FILE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const CASE: &str = "0123456789abcdef0123456789abcdef";
    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    fn write(source: &Path, body: &str) {
        fs::write(source.join(MANIFEST_FILE), body).unwrap();
    }

    fn valid() -> String {
        format!(
            "{{\"archive_schema_version\":1,\"case_id\":\"{CASE}\",\
             \"exported_at\":\"2026-01-17T12:00:00Z\",\
             \"objects\":[{{\"algorithm\":\"sha256\",\"byte_length\":16,\
             \"digest\":\"{DIGEST}\"}}],\
             \"records\":[{{\"id\":\"{CASE}\",\"kind\":\"case\"}}],\
             \"schema_version\":1}}\n"
        )
    }

    #[test]
    fn a_manifest_export_wrote_is_read_back_with_its_counts() {
        let source = tempfile::tempdir().unwrap();
        write(source.path(), &valid());
        let manifest = read(source.path()).unwrap();
        assert_eq!(manifest.case_id, CASE);
        assert_eq!(manifest.objects[0].byte_length, 16);
        assert_eq!(manifest.records[0].kind, "case");
        assert_eq!(manifest.counts.len(), records::KINDS.len());
        assert_eq!(manifest.counts[0].count, 1);
        assert!(manifest.counts[1..].iter().all(|kind| kind.count == 0));
    }

    #[test]
    fn an_export_without_a_manifest_is_refused_as_missing() {
        let source = tempfile::tempdir().unwrap();
        let refusal = read(source.path()).unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_MANIFEST_MISSING);
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["scope"], "export_source");
        assert_eq!(json["details"]["export_path"], "manifest.json");
    }

    #[test]
    fn every_claim_the_manifest_makes_about_itself_is_checked() {
        let source = tempfile::tempdir().unwrap();
        let bad = [
            "{ not json".to_owned(),
            valid().replace("\"schema_version\":1", "\"schema_version\":2"),
            valid().replace(CASE, "not-an-identifier"),
            valid().replace("sha256", "sha512"),
            valid().replace(DIGEST, "abc"),
            valid().replace("\"kind\":\"case\"", "\"kind\":\"invented\""),
            valid().replace("\"records\":[", "\"records\":[],\"unused\":["),
        ];
        for body in bad {
            write(source.path(), &body);
            assert_eq!(
                read(source.path()).unwrap_err().code,
                codes::EXPORT_MANIFEST_MALFORMED,
                "refused: {body}"
            );
        }
    }

    #[test]
    fn an_export_of_another_schema_version_is_refused_by_the_schema_rules() {
        let source = tempfile::tempdir().unwrap();
        write(
            source.path(),
            &valid().replace(
                "\"archive_schema_version\":1",
                "\"archive_schema_version\":2",
            ),
        );
        assert_eq!(
            read(source.path()).unwrap_err().code,
            codes::ARCHIVE_SCHEMA_NEWER
        );
        write(
            source.path(),
            &valid().replace(
                "\"archive_schema_version\":1",
                "\"archive_schema_version\":0",
            ),
        );
        assert_eq!(
            read(source.path()).unwrap_err().code,
            codes::ARCHIVE_SCHEMA_OLDER
        );
    }

    #[test]
    fn a_repeated_object_or_record_is_refused_rather_than_imported_twice() {
        let source = tempfile::tempdir().unwrap();
        let twice = valid().replace(
            &format!("\"digest\":\"{DIGEST}\"}}]"),
            &format!(
                "\"digest\":\"{DIGEST}\"}},{{\"algorithm\":\"sha256\",\
                 \"byte_length\":16,\"digest\":\"{DIGEST}\"}}]"
            ),
        );
        write(source.path(), &twice);
        assert_eq!(
            read(source.path()).unwrap_err().code,
            codes::EXPORT_MANIFEST_MALFORMED
        );
        let two_cases = valid().replace(
            &format!("{{\"id\":\"{CASE}\",\"kind\":\"case\"}}]"),
            &format!(
                "{{\"id\":\"{CASE}\",\"kind\":\"case\"}},\
                 {{\"id\":\"ffffffffffffffffffffffffffffffff\",\"kind\":\"case\"}}]"
            ),
        );
        write(source.path(), &two_cases);
        assert_eq!(
            read(source.path()).unwrap_err().code,
            codes::EXPORT_MANIFEST_MALFORMED
        );
    }
}
