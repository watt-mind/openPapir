//! Unit tests for the derived-metadata record: how it is written, read, and
//! deliberately not believed.

use super::*;
use crate::archive;
use std::fs;

const DIGEST: &str = "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
const OTHER: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn archive_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    root
}

fn record(digest: &str, media_type: &str, byte_length: u64) -> DerivedMetadata {
    DerivedMetadata {
        archive_schema_version: 1,
        artefact_digest: digest.to_owned(),
        byte_length,
        computed_at: "2026-01-20T09:00:00Z".to_owned(),
        extractor_name: EXTRACTOR_NAME.to_owned(),
        extractor_version: EXTRACTOR_VERSION.to_owned(),
        media_type: media_type.to_owned(),
        record_kind: KIND.to_owned(),
    }
}

fn path_of(root: &Path, digest: &str) -> std::path::PathBuf {
    let hex = digest.strip_prefix(DIGEST_PREFIX).unwrap();
    root.join(DERIVED_DIR).join(format!("{hex}.json"))
}

#[test]
fn a_derived_record_is_filed_under_its_own_digest_with_sorted_keys() {
    let root = archive_root();
    write_derived(root.path(), &record(DIGEST, "text", 27)).unwrap();
    let stored_text = fs::read_to_string(path_of(root.path(), DIGEST)).unwrap();
    assert!(
        stored_text.starts_with("{\"archive_schema_version\""),
        "sorted"
    );
    assert!(stored_text.ends_with("}\n"), "one LF-terminated document");
    assert!(stored_text.contains("\"record_kind\":\"derived_metadata\""));

    let read = read_derived(root.path(), DIGEST).unwrap();
    assert_eq!(read, record(DIGEST, "text", 27));
    assert_eq!(read.extractor_name, "openpapir.magic-bytes");
    assert_eq!(read.extractor_version, "1");
    assert_eq!(count(root.path()), 1);
}

#[test]
fn recomputing_replaces_the_record_rather_than_leaving_a_second() {
    let root = archive_root();
    write_derived(root.path(), &record(DIGEST, "unknown", 27)).unwrap();
    write_derived(root.path(), &record(DIGEST, "text", 27)).unwrap();
    assert_eq!(
        read_derived(root.path(), DIGEST).unwrap().media_type,
        "text"
    );
    assert_eq!(count(root.path()), 1, "one object keeps one record");
    assert_eq!(
        fs::read_dir(root.path().join(DERIVED_DIR)).unwrap().count(),
        1
    );
}

#[test]
fn a_disposable_record_that_cannot_be_read_is_no_record_and_no_damage() {
    let root = archive_root();
    write_derived(root.path(), &record(DIGEST, "text", 27)).unwrap();
    fs::write(path_of(root.path(), DIGEST), b"{ not a record").unwrap();
    assert_eq!(read_derived(root.path(), DIGEST), None);
    assert_eq!(count(root.path()), 0, "unreadable is not counted");
    assert!(facts_for(root.path(), [DIGEST]).is_empty());

    // A document that reads but names another object is not this object's.
    write_derived(root.path(), &record(DIGEST, "text", 27)).unwrap();
    fs::write(
        path_of(root.path(), DIGEST),
        crate::records::document::json_document(&record(OTHER, "text", 27)).unwrap(),
    )
    .unwrap();
    assert_eq!(read_derived(root.path(), DIGEST), None);
}

#[test]
fn a_reference_that_is_not_a_digest_never_becomes_a_path() {
    let root = archive_root();
    for value in ["../../etc/passwd", "sha256:short", "", "sha256:"] {
        assert_eq!(read_derived(root.path(), value), None);
        assert!(!remove_derived(root.path(), value));
    }
    let refusal = write_derived(root.path(), &record("sha256:short", "text", 1)).unwrap_err();
    assert_eq!(refusal.code, crate::error::codes::RECORD_INCONSISTENT);
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["rule"], "artefact_digest_shape");
    assert_eq!(json["details"]["record_kind"], "derived_metadata");
}

#[test]
fn the_facts_are_one_entry_per_named_artefact_that_has_a_record() {
    let root = archive_root();
    write_derived(root.path(), &record(DIGEST, "pdf", 1024)).unwrap();
    let facts = facts_for(root.path(), [DIGEST, DIGEST, OTHER]);
    assert_eq!(facts.len(), 1, "one entry however often it is named");
    assert_eq!(facts[0].artefact_digest, DIGEST);
    assert_eq!(facts[0].media_type, "pdf");
    assert_eq!(facts[0].byte_length, 1024);
    let rendered = serde_json::to_value(&facts[0]).unwrap();
    assert_eq!(rendered.as_object().unwrap().len(), 3, "facts only");
    assert!(facts_for(root.path(), [OTHER]).is_empty());
    assert!(facts_for(root.path(), []).is_empty());
}

#[test]
fn a_removed_record_leaves_nothing_and_removing_twice_is_no_error() {
    let root = archive_root();
    write_derived(root.path(), &record(DIGEST, "text", 27)).unwrap();
    assert!(remove_derived(root.path(), DIGEST));
    assert!(!remove_derived(root.path(), DIGEST));
    assert_eq!(count(root.path()), 0);
    assert!(stored(root.path()).is_empty());
}

#[test]
fn a_directory_entry_that_is_not_a_derived_record_is_passed_over() {
    let root = archive_root();
    write_derived(root.path(), &record(DIGEST, "text", 27)).unwrap();
    fs::write(root.path().join(DERIVED_DIR).join("notes.txt"), b"x").unwrap();
    fs::write(
        root.path().join(DERIVED_DIR).join(".papir-staging-abc"),
        b"x",
    )
    .unwrap();
    assert_eq!(count(root.path()), 1);
    assert_eq!(stored(root.path()).len(), 1);
}
