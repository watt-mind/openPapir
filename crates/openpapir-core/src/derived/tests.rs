//! Unit tests for `archive derive`: what it computes, what it leaves alone,
//! and what it refuses.

use super::*;
use crate::archive;
use crate::archive::lock::WriterLock;
use crate::error::codes;
use crate::records::derived::read_derived;
use std::fs;
use std::path::PathBuf;

/// An archive holding the named payloads, imported in order.
fn archive_with(payloads: &[&[u8]]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = payloads
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let path = inputs.path().join(format!("input-{index}.bin"));
            fs::write(&path, payload).unwrap();
            path
        })
        .collect();
    if !files.is_empty() {
        archive::import::import(root.path(), &files).unwrap();
    }
    root
}

/// The count the report gives one media type.
fn count_of(report: &Derived, media_type: &str) -> u64 {
    report
        .media_types
        .iter()
        .find(|entry| entry.media_type == media_type)
        .map_or(u64::MAX, |entry| entry.count)
}

#[test]
fn every_stored_object_gets_a_record_naming_its_type_and_its_length() {
    let root = archive_with(&[
        b"synthetic text\n",
        b"%PDF-1.7\ntrailer\n",
        b"PK\x03\x04zip",
    ]);
    let report = derive(root.path()).unwrap().data;
    assert_eq!(report.objects_checked, 3);
    assert_eq!(report.objects_unchecked, 0);
    assert_eq!(report.records_written, 3);
    assert_eq!(report.records_removed, 0);
    assert_eq!(count_of(&report, "text"), 1);
    assert_eq!(count_of(&report, "pdf"), 1);
    assert_eq!(count_of(&report, "zip"), 1);
    assert_eq!(count_of(&report, "unknown"), 0);
    assert_eq!(
        report.media_types.len(),
        media::MEDIA_TYPES.len(),
        "every value of the closed table is reported, seen or not"
    );
    assert_eq!(report.bytes_sniffed, 15 + 17 + 7);

    let total: u64 = report.media_types.iter().map(|entry| entry.count).sum();
    assert_eq!(total, report.records_written);
    assert_eq!(crate::records::derived::count(root.path()), 3);
}

#[test]
fn deriving_twice_recomputes_in_place_and_adds_no_second_record() {
    let root = archive_with(&[b"synthetic text\n"]);
    let first = derive(root.path()).unwrap().data;
    let second = derive(root.path()).unwrap().data;
    assert_eq!(first.records_written, 1);
    assert_eq!(second.records_written, 1);
    assert_eq!(second.records_removed, 0);
    assert_eq!(crate::records::derived::count(root.path()), 1);
}

#[test]
fn a_record_naming_an_object_the_archive_no_longer_holds_is_discarded() {
    let root = archive_with(&[b"synthetic text\n"]);
    derive(root.path()).unwrap();
    let stale = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    crate::records::derived::write_derived(
        root.path(),
        &crate::records::derived::DerivedMetadata {
            archive_schema_version: 1,
            artefact_digest: stale.to_owned(),
            byte_length: 1,
            computed_at: "2026-01-20T09:00:00Z".to_owned(),
            extractor_name: crate::records::derived::EXTRACTOR_NAME.to_owned(),
            extractor_version: crate::records::derived::EXTRACTOR_VERSION.to_owned(),
            media_type: "text".to_owned(),
            record_kind: crate::records::derived::KIND.to_owned(),
        },
    )
    .unwrap();
    assert_eq!(crate::records::derived::count(root.path()), 2);

    let report = derive(root.path()).unwrap().data;
    assert_eq!(report.records_removed, 1);
    assert_eq!(report.records_written, 1);
    assert_eq!(read_derived(root.path(), stale), None);
    assert_eq!(crate::records::derived::count(root.path()), 1);
}

/// The one stored object of an archive that holds exactly one.
fn only_digest(root: &Path) -> String {
    crate::records::derived::filed(root)
        .into_iter()
        .next()
        .expect("the archive holds one derived record")
}

#[cfg(unix)]
#[test]
fn an_object_over_the_single_file_cap_is_counted_and_left_without_a_record() {
    use std::os::unix::fs::PermissionsExt;

    let root = archive_with(&[b"synthetic text\n"]);
    derive(root.path()).unwrap();
    let digest = only_digest(root.path());
    let hex = digest.strip_prefix(crate::records::DIGEST_PREFIX).unwrap();
    let path = crate::archive::objects::absolute_path(root.path(), hex);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(crate::archive::limits::MAX_FILE_BYTES + 1)
        .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

    let report = derive(root.path()).unwrap().data;
    assert_eq!(report.objects_checked, 1);
    assert_eq!(report.objects_unchecked, 1);
    assert_eq!(report.records_written, 0, "an unread object gets none");
    assert_eq!(report.records_removed, 0, "and keeps the record it had");
    assert!(
        read_derived(root.path(), &digest).is_some(),
        "the record it already had is not discarded by a cap"
    );
}

#[test]
fn an_empty_object_names_no_type_at_all() {
    let root = archive_with(&[b""]);
    let report = derive(root.path()).unwrap().data;
    assert_eq!(report.objects_checked, 1);
    assert_eq!(count_of(&report, "unknown"), 1);
    assert_eq!(report.bytes_sniffed, 0);
}

#[test]
fn an_archive_with_no_object_derives_nothing_and_is_not_an_error() {
    let root = archive_with(&[]);
    let report = derive(root.path()).unwrap().data;
    assert_eq!(report.objects_checked, 0);
    assert_eq!(report.records_written, 0);
    assert!(report.media_types.iter().all(|entry| entry.count == 0));
}

#[test]
fn deriving_needs_the_writer_lock_and_an_archive() {
    let root = archive_with(&[b"synthetic text\n"]);
    let held = WriterLock::acquire(root.path()).unwrap();
    let refusal = derive(root.path()).unwrap_err().error;
    assert_eq!(refusal.code, codes::LOCK_HELD);
    assert!(refusal.is_retryable());
    drop(held);

    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        derive(empty.path()).unwrap_err().error.code,
        codes::ARCHIVE_MARKER_MISSING
    );
}

#[test]
fn the_report_carries_counts_only_and_never_a_digest_or_a_path() {
    let root = archive_with(&[b"synthetic text\n"]);
    let report = derive(root.path()).unwrap().data;
    let rendered = serde_json::to_string(&report).unwrap();
    assert!(!rendered.contains("sha256"), "no digest reaches the report");
    assert!(!rendered.contains('/'), "no path reaches the report");
    assert!(!rendered.contains("input-0"), "no filename either");
}

#[test]
fn a_sniff_reads_no_more_than_the_bounded_window() {
    let mut payload = b"%PDF-1.7\n".to_vec();
    payload.resize(media::SNIFF_BYTES * 3, b'a');
    let root = archive_with(&[&payload]);
    let report = derive(root.path()).unwrap().data;
    assert_eq!(report.bytes_sniffed, media::SNIFF_BYTES as u64);
    assert_eq!(count_of(&report, "pdf"), 1);
    let record = read_derived(root.path(), &only_digest(root.path())).unwrap();
    assert_eq!(
        record.byte_length,
        payload.len() as u64,
        "the length is the object's own, not the window's"
    );
}
