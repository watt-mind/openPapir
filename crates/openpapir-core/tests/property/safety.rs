//! Invariant 5: a hostile name never becomes a path.
//!
//! Every path openPapir uses is the archive root plus its own fixed directory
//! names plus an identifier it minted (`docs/archive-layout.md`). The
//! invariant that keeps that true is that a value carrying a `..`, a
//! separator, a NUL, or a component longer than the filename cap is refused
//! before it is joined into anything: `is_identifier` and `is_digest` reject
//! it, the reference checks answer `record.not_found` or `usage.arguments`,
//! the archive on disk is untouched, and the refusal never echoes the value.
//!
//! The `path.*` codes are the other half of the same rule: they are what the
//! archive answers when a path openPapir did build turns out to be a link or
//! to be occupied, which is the only way a write can leave the layout.

use std::fs;

use proptest::prelude::*;

use openpapir_core::error::codes;
use openpapir_core::records::case::{Case, KIND, Status};
use openpapir_core::records::document::{self, Record};
use openpapir_core::records::{self, is_digest};

use super::support;

/// A stored case document, used where a valid record has to exist.
fn case(id: &str) -> Case {
    Case {
        archive_schema_version: 1,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        id: id.to_owned(),
        notes: None,
        record_kind: KIND.to_owned(),
        status: Status::Open,
        tags: Vec::new(),
        title: "Title".to_owned(),
        updated_at: None,
    }
}

proptest! {
    #![proptest_config(support::config(256))]

    /// A hostile name is neither an identifier nor a digest, so it never
    /// reaches a path join in the first place.
    #[test]
    fn a_hostile_name_is_never_an_identifier_or_a_digest(name in support::hostile_name()) {
        prop_assert!(!document::is_identifier(&name));
        prop_assert!(!is_digest(&name));
        prop_assert!(!is_digest(name.trim_start_matches("sha256:")));
    }

    /// A reference carrying a hostile name is refused, the refusal names only
    /// the shape of the failure, and the archive is left exactly as it was.
    #[test]
    fn a_hostile_reference_is_refused_without_touching_the_archive(
        name in support::hostile_name(),
    ) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        support::record_directories(root.path());
        let stored = case("0123456789abcdef0123456789abcdef");
        document::write_record(root.path(), &stored).expect("a record is written");
        // A document a traversal could reach, to make an escape observable.
        fs::write(root.path().join("escape.json"), "{}").expect("a decoy is stored");
        let before = support::listing(root.path());

        let refusal = document::read_record::<Case>(root.path(), &name, "case_id")
            .expect_err("a hostile reference names no record");
        prop_assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        prop_assert_eq!(refusal.exit_code(), 4);
        support::details_are_permitted(&refusal)?;
        prop_assert!(
            !serde_json::to_string(&refusal)
                .expect("a diagnostic serialises")
                .contains(name.trim()),
            "a refusal never echoes the value that caused it"
        );

        let refusal = records::checked_digest("artefact", &name)
            .expect_err("a hostile reference is not a digest");
        prop_assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        support::details_are_permitted(&refusal)?;

        if name.contains('\u{0}') {
            let refusal = records::checked_title(&name)
                .expect_err("a control character is never a record field");
            prop_assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        }
        prop_assert_eq!(support::listing(root.path()), before, "the archive is untouched");
    }
}

proptest! {
    #![proptest_config(support::config(128))]

    /// A record path that is already occupied is refused with `path.overwrite`
    /// rather than replacing what is there, whatever the record is called.
    #[test]
    fn an_occupied_record_path_is_refused_rather_than_replaced(id in support::identifier()) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        support::record_directories(root.path());
        let occupied = root
            .path()
            .join(Case::DIRECTORY)
            .join(format!("{id}.json"));
        fs::write(&occupied, b"someone else's bytes").expect("the path is occupied");

        let refusal = document::write_record(root.path(), &case(&id))
            .expect_err("an occupied record path is never overwritten");
        prop_assert_eq!(refusal.code, codes::PATH_OVERWRITE);
        prop_assert_eq!(refusal.exit_code(), 3);
        support::details_are_permitted(&refusal)?;
        prop_assert_eq!(
            fs::read(&occupied).expect("the occupant is read back"),
            b"someone else's bytes".to_vec(),
            "the occupant is left exactly as it was"
        );
    }

    /// A record path that is a symbolic link is refused with `path.symlink`,
    /// and the link's target is neither written through nor read through.
    #[cfg(unix)]
    #[test]
    fn a_linked_record_path_is_refused_and_never_followed(id in support::identifier()) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        support::record_directories(root.path());
        let outside = tempfile::tempdir().expect("a directory outside the archive");
        let target = outside.path().join("target.json");
        fs::write(&target, b"outside bytes").expect("a target is stored");
        let linked = root
            .path()
            .join(Case::DIRECTORY)
            .join(format!("{id}.json"));
        std::os::unix::fs::symlink(&target, &linked).expect("a link is planted");

        let refusal = openpapir_core::archive::write::write_document(
            &root.path().join(Case::DIRECTORY),
            &format!("{id}.json"),
            "records/cases/record.json",
            b"{}\n",
            "record_write",
        )
        .expect_err("a linked record path is never written through");
        prop_assert_eq!(refusal.code, codes::PATH_SYMLINK);
        prop_assert_eq!(refusal.exit_code(), 3);
        support::details_are_permitted(&refusal)?;

        let refusal = document::read_record::<Case>(root.path(), &id, "case_id")
            .expect_err("a linked record path is never read through");
        prop_assert_eq!(refusal.code, codes::RECORD_MALFORMED);
        prop_assert_eq!(
            fs::read(&target).expect("the target is read back"),
            b"outside bytes".to_vec(),
            "the link's target is untouched"
        );
    }
}
