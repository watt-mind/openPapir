//! Invariant 1: a record document reader answers, it never panics.
//!
//! A stored record document is untrusted input. The reader's contract is that
//! any byte sequence at a record path either reads back as a record of that
//! kind or is refused with a `record.*` diagnostic, that the refusal names the
//! shape of the failure and never the bytes, and that a document larger than
//! the record cap is refused on the opened handle before its bytes are
//! allocated.

use std::fs;

use proptest::prelude::*;

use openpapir_core::archive::import::ImportEvent;
use openpapir_core::archive::limits;
use openpapir_core::archive::{Archive, MARKER_FILE};
use openpapir_core::error::codes;
use openpapir_core::records::association::Association;
use openpapir_core::records::case::Case;
use openpapir_core::records::document::{self, Record};
use openpapir_core::records::receipt::Receipt;
use openpapir_core::records::submission::Submission;

use super::support;

/// The identifier every generated document is stored under.
const ID: &str = "0123456789abcdef0123456789abcdef";

/// The codes a record reader is allowed to answer a bad document with.
const READER_CODES: [&str; 3] = [
    codes::RECORD_MALFORMED,
    codes::RECORD_NOT_FOUND,
    codes::INPUT_CAP_RECORD_SIZE,
];

/// Read one kind back and assert the reader's whole contract for it.
fn reads_or_refuses<R: Record>(root: &std::path::Path) -> Result<(), TestCaseError> {
    let path = root.join(R::DIRECTORY).join(format!("{ID}.json"));
    match document::read_record::<R>(root, ID, "record_id") {
        Ok(record) => {
            prop_assert_eq!(
                record.id(),
                ID,
                "a record that reads back names its own file"
            );
            prop_assert_eq!(record.record_kind(), R::KIND);
        }
        Err(refusal) => {
            prop_assert!(
                READER_CODES.contains(&refusal.code),
                "a record reader answered with an undocumented code"
            );
            support::details_are_permitted(&refusal)?;
            support::exit_code_is_bucketed(&refusal)?;
        }
    }
    match document::list_records::<R>(root) {
        Ok(records) => prop_assert!(records.len() <= 1),
        Err(refusal) => prop_assert_eq!(refusal.code, codes::RECORD_MALFORMED),
    }
    prop_assert!(path.exists(), "a reader never removes what it read");
    Ok(())
}

proptest! {
    #![proptest_config(support::config(256))]

    /// Any byte sequence stored at a record path reads back as that record or
    /// is refused with a documented `record.*` code, for every record kind.
    #[test]
    fn any_bytes_at_a_record_path_read_or_are_refused(
        bytes in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        support::record_directories(root.path());
        for directory in [
            Case::DIRECTORY,
            Submission::DIRECTORY,
            Receipt::DIRECTORY,
            Association::DIRECTORY,
            ImportEvent::DIRECTORY,
        ] {
            fs::write(root.path().join(directory).join(format!("{ID}.json")), &bytes)
                .expect("a generated document is stored");
        }
        reads_or_refuses::<Case>(root.path())?;
        reads_or_refuses::<Submission>(root.path())?;
        reads_or_refuses::<Receipt>(root.path())?;
        reads_or_refuses::<Association>(root.path())?;
        reads_or_refuses::<ImportEvent>(root.path())?;
    }

    /// Any byte sequence in the archive marker opens the archive or is refused
    /// with an `archive.*` diagnostic. The marker is the first untrusted
    /// document every operation reads.
    #[test]
    fn any_bytes_in_the_marker_open_the_archive_or_are_refused(
        bytes in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        openpapir_core::archive::init(root.path()).expect("an archive is created");
        fs::write(root.path().join(MARKER_FILE), &bytes).expect("a marker is stored");

        if let Err(refusal) = Archive::open_read_only(root.path()) {
            prop_assert!(
                refusal.code.starts_with("archive."),
                "a marker that cannot be read is an archive refusal"
            );
            support::details_are_permitted(&refusal)?;
            support::exit_code_is_bucketed(&refusal)?;
        }
    }
}

proptest! {
    #![proptest_config(support::config(4))]

    /// A document over the record cap is refused, and one under it is read as
    /// far as its bytes allow: the reader never allocates past the cap.
    #[test]
    fn a_document_over_the_record_cap_is_refused_without_being_read(
        excess in 1_u64..4096,
    ) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        support::record_directories(root.path());
        let path = root.path().join(Case::DIRECTORY).join(format!("{ID}.json"));
        let size = limits::MAX_RECORD_BYTES + excess;
        fs::write(&path, vec![b'{'; usize::try_from(size).expect("the cap fits in memory")])
            .expect("an oversized document is stored");

        let refusal = document::read_record::<Case>(root.path(), ID, "case_id")
            .expect_err("an oversized document is never read as a record");
        prop_assert_eq!(refusal.code, codes::INPUT_CAP_RECORD_SIZE);
        prop_assert_eq!(refusal.exit_code(), 3);
        support::details_are_permitted(&refusal)?;

        let refusal = document::list_records::<Case>(root.path())
            .expect_err("a listing reports the directory's condition");
        prop_assert_eq!(refusal.code, codes::RECORD_MALFORMED);
    }
}
