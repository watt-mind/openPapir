//! Invariant 4: a valid record round-trips byte for byte.
//!
//! A record document is the archive's durable form, so the contract is
//! stronger than "parses back". For every generated case, submission, receipt
//! and association record, the document is one LF-terminated line with sorted
//! keys, parsing it back yields the same record, and rendering that record
//! again yields the same bytes. That is what makes a diff, a backup and an
//! export stable (`docs/archive-layout.md`).

use std::fs;

use proptest::prelude::*;
use serde::de::DeserializeOwned;

use openpapir_core::archive::import::{ImportEvent, SOURCE_EXPORT};
use openpapir_core::records::case;
use openpapir_core::records::document::{self, Record};

use super::support;

/// Render, parse, re-render, and store one record, asserting the whole
/// round-trip contract for it.
fn round_trips<R>(record: &R) -> Result<(), TestCaseError>
where
    R: Record + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let rendered = document::document(record).expect("a capped record renders");
    prop_assert!(
        rendered.ends_with('\n') && rendered.matches('\n').count() == 1,
        "a record document is one LF-terminated line"
    );
    prop_assert!(
        keys_are_sorted(&rendered),
        "a record document sorts its keys"
    );

    let parsed: R = serde_json::from_str(&rendered).expect("a rendered record parses back");
    prop_assert_eq!(&parsed, record, "a record survives its own document");
    let again = document::document(&parsed).expect("a parsed record renders");
    prop_assert_eq!(&again, &rendered, "the round-trip is byte-identical");

    let root = tempfile::tempdir().expect("a temporary archive root");
    fs::create_dir_all(root.path().join(R::DIRECTORY)).expect("a record directory is created");
    document::write_record(root.path(), record).expect("a record is written");
    let stored = fs::read_to_string(
        root.path()
            .join(R::DIRECTORY)
            .join(format!("{}.json", record.id())),
    )
    .expect("the stored document is read back");
    prop_assert_eq!(&stored, &rendered, "what is stored is what was rendered");
    let read: R = document::read_record(root.path(), record.id(), "record_id")
        .expect("a written record reads back");
    prop_assert_eq!(&read, record);
    Ok(())
}

/// Whether the top-level keys of a JSON object appear in sorted order.
fn keys_are_sorted(rendered: &str) -> bool {
    let value: serde_json::Value = match serde_json::from_str(rendered) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    let keys: Vec<&String> = object.keys().collect();
    keys.windows(2).all(|pair| pair[0] <= pair[1])
}

proptest! {
    #![proptest_config(support::config(256))]

    /// A case carries an optional notes field and an optional `updated_at`,
    /// so the round-trip has to survive keys that are written only when they
    /// are there, and a status and a tag list that are written always and
    /// default when a record an earlier build wrote does not carry them. The
    /// generated title, status and tags are put through the write path itself,
    /// so the round-trip covers a case the archive would really store rather
    /// than one only this test would accept.
    #[test]
    fn a_case_record_round_trips(record in support::case_record()) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        openpapir_core::archive::init(root.path()).expect("an archive is created");
        let written = case::create_with(
            root.path(),
            &record.title,
            record.notes.as_deref(),
            record.status,
            &record.tags,
        )
        .expect("the write path accepts a generated case")
        .data
        .case;
        prop_assert_eq!(&written.title, &record.title);
        prop_assert_eq!(&written.notes, &record.notes);
        prop_assert_eq!(written.status, record.status);
        prop_assert_eq!(
            &written.tags,
            &record.tags,
            "the write path stores the generated tags sorted and deduplicated"
        );
        prop_assert_eq!(written.updated_at, None, "a new case has not been updated");

        round_trips(&record)?;
    }

    /// A submission carries a list of artefact references, each with an optional role, so the round-trip has to survive a nested optional inside a list.
    #[test]
    fn a_submission_record_round_trips(record in support::submission_record()) {
        round_trips(&record)?;
    }

    /// A receipt names an artefact and the import event it came from, and carries an optional label.
    #[test]
    fn a_receipt_record_round_trips(record in support::receipt_record()) {
        round_trips(&record)?;
    }

    /// An import event carries `source` only when the bytes it records came
    /// from an export, and the field is written with `skip_serializing_if`,
    /// so an absent source is an absent key rather than a null. Both values
    /// are built here rather than left to the generator, so both arms of the
    /// distinction are reached by construction: the document without the key
    /// and the document with it each round-trip to their own bytes, and the
    /// one without it parses back to an absent source rather than to a
    /// default the reader filled in.
    #[test]
    fn an_import_event_record_round_trips(record in support::import_event_record()) {
        let absent = ImportEvent { source: None, ..record.clone() };
        let present = ImportEvent { source: Some(SOURCE_EXPORT.to_owned()), ..record };

        let without = document::document(&absent).expect("a capped record renders");
        let with = document::document(&present).expect("a capped record renders");
        prop_assert!(
            !without.contains("\"source\""),
            "an absent source is written as no key at all"
        );
        prop_assert!(
            with.contains(&format!("\"source\":\"{SOURCE_EXPORT}\"")),
            "a present source is written as its own key"
        );
        prop_assert_ne!(
            &without,
            &with,
            "the two events render as two different documents"
        );

        round_trips(&absent)?;
        round_trips(&present)?;
    }

    /// An association carries nested candidates and evidence, two optional
    /// identifiers that are written as null rather than omitted, and the
    /// retirement statement, which is omitted entirely when it is absent. The
    /// generated statement is asserted against the write path's own check, so
    /// the round-trip covers text the archive would really store rather than
    /// text only this test would accept.
    #[test]
    fn an_association_record_round_trips(record in support::association_record()) {
        if let Some(statement) = record.statement.as_deref() {
            prop_assert!(
                openpapir_core::records::checked_statement(statement).is_ok(),
                "a generated statement is one the write path would accept"
            );
        }
        round_trips(&record)?;
    }
}
