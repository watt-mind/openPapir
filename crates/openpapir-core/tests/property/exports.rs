//! Invariant 3: an export answers with a manifest or a documented refusal.
//!
//! This module holds the export writer to its half of that contract. `case
//! import` reads a manifest back (`docs/architecture.md`), and the reader is
//! covered by its own tests; what is checked here is the writer, whose
//! contract has two halves. When an export succeeds, the manifest is one
//! LF-terminated JSON object with sorted keys that names every record and
//! object the export wrote and nothing else. When it does not, the
//! refusal is one of the documented codes, `export.*` whenever the destination
//! is what stood in the way, it carries no value the caller supplied, and the
//! destination is left as the export found it.

use std::fs;
use std::path::{Path, PathBuf};

use proptest::prelude::*;

use openpapir_core::error::codes;
use openpapir_core::records::{case, submission};

use super::support;

/// The codes an export may answer when the destination is what stood in the
/// way. `export.destination_conflict` is the one this suite pins by name; the
/// others cover a destination that is not a usable directory at all.
const DESTINATION_CODES: [&str; 4] = [
    codes::EXPORT_DESTINATION_CONFLICT,
    codes::USAGE_ARGUMENTS,
    codes::PATH_SYMLINK,
    codes::WRITE_INTERRUPTED,
];

/// What the destination path already holds when the export runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Occupant {
    /// Nothing: the export creates the directory itself.
    Absent,
    /// An empty directory, which the export is allowed to fill.
    EmptyDirectory,
    /// A directory holding something, which is a conflict.
    NonEmptyDirectory,
    /// A regular file, which is not a usable destination.
    File,
}

fn occupant() -> impl Strategy<Value = Occupant> {
    prop_oneof![
        Just(Occupant::Absent),
        Just(Occupant::EmptyDirectory),
        Just(Occupant::NonEmptyDirectory),
        Just(Occupant::File),
    ]
}

/// Put the generated occupant at `destination`.
fn place(occupant: Occupant, destination: &PathBuf) {
    match occupant {
        Occupant::Absent => {}
        Occupant::EmptyDirectory => {
            fs::create_dir_all(destination).expect("an empty destination is created");
        }
        Occupant::NonEmptyDirectory => {
            fs::create_dir_all(destination.join("held")).expect("an occupied destination");
        }
        Occupant::File => {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).expect("the destination's parent");
            }
            fs::write(destination, b"held").expect("a file destination");
        }
    }
}

/// Assert the manifest contract of a successful export.
fn manifest_is_well_formed(destination: &Path, case_id: &str) -> Result<(), TestCaseError> {
    let rendered = fs::read_to_string(destination.join("manifest.json"))
        .expect("a successful export writes a manifest");
    prop_assert!(
        rendered.ends_with('\n') && rendered.matches('\n').count() == 1,
        "a manifest is one LF-terminated line"
    );
    let value: serde_json::Value =
        serde_json::from_str(&rendered).expect("a manifest is one JSON object");
    let object = value.as_object().expect("a manifest is an object");
    let keys: Vec<&String> = object.keys().collect();
    prop_assert!(
        keys.windows(2).all(|pair| pair[0] <= pair[1]),
        "a manifest sorts its keys"
    );
    prop_assert_eq!(object["schema_version"].as_u64(), Some(1));
    prop_assert_eq!(object["archive_schema_version"].as_u64(), Some(1));
    prop_assert_eq!(object["case_id"].as_str(), Some(case_id));

    let records = object["records"].as_array().expect("records is a list");
    for entry in records {
        let kind = entry["kind"]
            .as_str()
            .expect("a record entry names its kind");
        let id = entry["id"].as_str().expect("a record entry names its id");
        prop_assert!(
            destination
                .join("records")
                .join(kind)
                .join(format!("{id}.json"))
                .is_file(),
            "every record the manifest lists was written"
        );
    }
    prop_assert!(
        object["objects"]
            .as_array()
            .is_some_and(|objects| objects.is_empty()),
        "an export of a case with no artefact lists no object"
    );
    Ok(())
}

proptest! {
    #![proptest_config(support::config(64))]

    /// An export either writes a well-formed manifest or refuses with a
    /// documented code, and a refusal leaves the destination as it found it.
    #[test]
    fn an_export_writes_a_manifest_or_refuses(
        occupant in occupant(),
        name in prop_oneof![
            Just("out".to_owned()),
            Just("nested/out".to_owned()),
            Just(".hidden".to_owned()),
        ],
        wrong_case in proptest::option::of(support::hostile_name()),
    ) {
        let root = tempfile::tempdir().expect("a temporary archive root");
        openpapir_core::archive::init(root.path()).expect("an archive is created");
        let created = case::create(root.path(), "Title", None)
            .expect("a case is created")
            .data
            .case;
        submission::add(root.path(), &created.id, "Description", None, &[])
            .expect("a submission is added");

        let outside = tempfile::tempdir().expect("a directory outside the archive");
        let destination = outside.path().join(&name);
        place(occupant, &destination);
        let before = support::listing(outside.path());

        let case_id = wrong_case.clone().unwrap_or_else(|| created.id.clone());
        match openpapir_core::export::export_case(root.path(), &case_id, &destination) {
            Ok(exported) => {
                prop_assert_eq!(wrong_case, None, "only the stored case exports");
                prop_assert_eq!(&exported.data.case_id, &created.id);
                manifest_is_well_formed(&destination, &created.id)?;
            }
            Err(failure) => {
                let refusal = failure.error;
                support::details_are_permitted(&refusal)?;
                support::exit_code_is_bucketed(&refusal)?;
                if wrong_case.is_none() {
                    prop_assert!(
                        DESTINATION_CODES.contains(&refusal.code),
                        "a destination refusal used an undocumented code"
                    );
                    if occupant == Occupant::NonEmptyDirectory {
                        prop_assert_eq!(
                            refusal.code,
                            codes::EXPORT_DESTINATION_CONFLICT,
                            "an occupied destination is an export conflict"
                        );
                    }
                } else {
                    prop_assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
                }
                prop_assert_eq!(
                    support::listing(outside.path()),
                    before,
                    "a refused export leaves the destination as it found it"
                );
            }
        }
    }
}
