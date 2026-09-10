//! Invariant 3: an export answers with a manifest or a documented refusal.
//!
//! The manifest is a boundary in both directions. The writer's contract has
//! two halves: when an export succeeds, the manifest is one LF-terminated
//! JSON object with sorted keys that names every record and object the export
//! wrote and nothing else; when it does not, the refusal is one of the
//! documented codes, `export.*` whenever the destination is what stood in the
//! way, it carries no value the caller supplied, and the destination is left
//! as the export found it.
//!
//! The reader's contract is the record reader's, one level up. An export
//! directory is user-supplied, so the manifest in it is untrusted input: any
//! bytes at `manifest.json` either read back as a manifest or are refused
//! with a documented code and never panic, and the refusal names the shape of
//! the failure rather than the document. A manifest an export wrote reads
//! back with exactly the rows it lists, and one carrying any single
//! difference the reader checks for is refused with the code that check
//! answers with: `export.manifest_malformed` for a document this build cannot
//! read as a manifest, and the archive's own schema codes for one that names
//! a schema version this build does not support, which is a document it has
//! no business reading rather than a broken one.

use std::fs;
use std::path::{Path, PathBuf};

use proptest::prelude::*;

use openpapir_core::archive::SUPPORTED_SCHEMA_VERSION as SCHEMA_VERSION;
use openpapir_core::error::{Diagnostic, codes};
use openpapir_core::export::destination::MANIFEST_FILE;
use openpapir_core::export::restore::{SOURCE_SCOPE, manifest};
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

/// The codes the manifest reader is allowed to answer a bad document with.
///
/// Every one of them is reached by a generated case rather than merely
/// admitted. The three path codes come from the placement the
/// first property generates, and the two schema codes from the schema damages
/// the third one does, which is why each damage carries the code it is
/// documented to produce instead of the property accepting any member of this
/// set.
const MANIFEST_READER_CODES: [&str; 5] = [
    codes::EXPORT_MANIFEST_MISSING,
    codes::EXPORT_MANIFEST_MALFORMED,
    codes::PATH_SYMLINK,
    codes::ARCHIVE_SCHEMA_NEWER,
    codes::ARCHIVE_SCHEMA_OLDER,
];

/// Assert the whole refusal contract of the manifest reader.
///
/// Which details a refusal carries depends on which check raised it, so the
/// assertion goes code by code rather than applying one blanket rule. The two
/// manifest codes name the source scope and the manifest's own path relative
/// to the source. `path.symlink` names the scope and no path at all: one
/// refusal serves a link anywhere in the export, the manifest, a record and an
/// object alike (`docs/architecture.md`), so it says which export it was
/// reading and never which file. The schema codes name neither, because the
/// rule that raises them is the archive's own, reached with the version an
/// archive marker holds as well as with the one a manifest declares; they
/// name the two versions instead.
fn manifest_refusal_is_documented(refusal: &Diagnostic) -> Result<(), TestCaseError> {
    prop_assert!(
        MANIFEST_READER_CODES.contains(&refusal.code),
        "the manifest reader answered with an undocumented code"
    );
    support::details_are_permitted(refusal)?;
    support::exit_code_is_bucketed(refusal)?;
    let rendered = serde_json::to_value(refusal).expect("a diagnostic serialises");
    let details = &rendered["details"];
    match refusal.code {
        codes::EXPORT_MANIFEST_MISSING | codes::EXPORT_MANIFEST_MALFORMED => {
            prop_assert_eq!(
                details["scope"].as_str(),
                Some(SOURCE_SCOPE),
                "a refusal inside an export source names the scope rather than the path"
            );
            prop_assert_eq!(
                details["export_path"].as_str(),
                Some(MANIFEST_FILE),
                "it names the manifest by its path relative to the source"
            );
        }
        codes::PATH_SYMLINK => {
            prop_assert_eq!(
                details["scope"].as_str(),
                Some(SOURCE_SCOPE),
                "a linked path in an export source names the scope"
            );
            prop_assert!(
                details.get("export_path").is_none(),
                "and never which path was linked"
            );
        }
        _ => {
            prop_assert_ne!(
                details["archive_schema_version"].as_u64(),
                Some(u64::from(SCHEMA_VERSION)),
                "a schema refusal names the version the export declared, not this build's"
            );
            prop_assert_eq!(
                details["supported_schema_version"].as_u64(),
                Some(u64::from(SCHEMA_VERSION)),
                "and the one this build supports"
            );
        }
    }
    Ok(())
}

/// Where the manifest is, and what it is, when the reader looks for it.
///
/// The three placements are what makes the reader's three path codes
/// reachable: nothing there is `export.manifest_missing`, a regular file is
/// read and then judged on its content, and a symbolic link is refused
/// without being followed however valid its target may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Nothing is at the manifest path.
    Absent,
    /// A regular file holding the generated bytes.
    File,
    /// A symbolic link to a regular file holding them.
    Link,
}

fn placement() -> impl Strategy<Value = Placement> {
    prop_oneof![
        Just(Placement::Absent),
        Just(Placement::File),
        Just(Placement::Link),
    ]
}

/// Put the generated bytes at the manifest path in the generated form.
///
/// A platform without symbolic links falls back to a regular file, so the
/// property still runs there and simply reaches one placement fewer.
fn place_manifest(source: &Path, placement: Placement, body: &[u8]) -> Placement {
    let manifest = source.join(MANIFEST_FILE);
    match placement {
        Placement::Absent => Placement::Absent,
        Placement::File => {
            fs::write(&manifest, body).expect("a manifest is stored");
            Placement::File
        }
        Placement::Link => {
            let target = source.join("elsewhere");
            fs::write(&target, body).expect("a link target is stored");
            if symlink(&target, &manifest) {
                Placement::Link
            } else {
                fs::write(&manifest, body).expect("a manifest is stored");
                Placement::File
            }
        }
    }
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

/// Write one manifest document into a fresh export directory and read it.
fn read_manifest(body: &[u8]) -> (tempfile::TempDir, Result<manifest::Manifest, Diagnostic>) {
    let source = tempfile::tempdir().expect("a temporary export source");
    fs::write(source.path().join(MANIFEST_FILE), body).expect("a manifest is stored");
    let read = manifest::read(source.path(), manifest::Scope::Case);
    (source, read)
}

proptest! {
    #![proptest_config(support::config(128))]

    /// Any byte sequence at `manifest.json` reads back as a manifest or is
    /// refused with a documented code, whether it is there as a file, as a
    /// symbolic link, or not at all.
    ///
    /// Random bytes reach only the refusing arm, which is the point of this
    /// property: it is the tolerance half of the contract. The two properties
    /// below reach the reading arm by construction. What the placement adds is
    /// the other two path codes, so each of the three is pinned to the
    /// placement that must produce it rather than accepted as one of a set.
    #[test]
    fn any_bytes_at_a_manifest_path_read_or_are_refused(
        body in proptest::collection::vec(any::<u8>(), 0..512),
        placement in placement(),
    ) {
        let source = tempfile::tempdir().expect("a temporary export source");
        let placed = place_manifest(source.path(), placement, &body);
        match manifest::read(source.path(), manifest::Scope::Case) {
            Ok(_) => prop_assert_eq!(
                placed,
                Placement::File,
                "only a regular file at the manifest path can read back"
            ),
            Err(refusal) => {
                manifest_refusal_is_documented(&refusal)?;
                match placed {
                    Placement::Absent => prop_assert_eq!(
                        refusal.code,
                        codes::EXPORT_MANIFEST_MISSING,
                        "an export holding no manifest describes no case"
                    ),
                    Placement::Link => prop_assert_eq!(
                        refusal.code,
                        codes::PATH_SYMLINK,
                        "a linked manifest is refused rather than followed"
                    ),
                    Placement::File => prop_assert_eq!(
                        refusal.code,
                        codes::EXPORT_MANIFEST_MALFORMED,
                        "a file this build cannot read as a manifest is malformed"
                    ),
                }
            }
        }
    }

    /// A manifest an export wrote reads back with exactly the rows it lists,
    /// and a difference the reader is documented to ignore leaves that true.
    #[test]
    fn a_manifest_this_build_wrote_reads_back(
        plan in support::manifest_plan(),
        damage in support::tolerated_manifest_damage(),
        key in proptest::sample::select(support::MANIFEST_REQUIRED_KEYS.to_vec()),
        row in 0_usize..8,
    ) {
        let body = support::damaged_manifest(&plan, damage, key, row);
        let (_source, read) = read_manifest(body.as_bytes());
        prop_assert!(damage.tolerated(), "this arm generates only tolerated damage");
        let read = read.expect("a manifest this build wrote reads back");

        prop_assert_eq!(read.case_id.as_deref(), Some(plan.case_id.as_str()));
        prop_assert_eq!(read.objects.len(), plan.objects.len());
        for (listed, (digest, byte_length)) in read.objects.iter().zip(&plan.objects) {
            prop_assert_eq!(&listed.digest, digest, "an object row keeps its digest");
            prop_assert_eq!(listed.byte_length, *byte_length);
            prop_assert_eq!(listed.algorithm.as_str(), "sha256");
        }
        prop_assert_eq!(read.records.len(), plan.rows.len());
        for (listed, (kind, id)) in read.records.iter().zip(&plan.rows) {
            prop_assert_eq!(&listed.kind, kind, "a record row keeps its kind");
            prop_assert_eq!(&listed.id, id, "a record row keeps its identifier");
        }
        let counted: u64 = read.counts.iter().map(|kind| kind.count).sum();
        prop_assert_eq!(
            counted,
            plan.rows.len() as u64,
            "the counts account for every row and for nothing else"
        );
        prop_assert_eq!(
            read.counts.iter().find(|kind| kind.kind == "case").map(|kind| kind.count),
            Some(1),
            "an export holds exactly one case"
        );
    }

    /// A manifest carrying one difference the reader checks for is refused
    /// with the code that check answers with, and nothing about the document
    /// reaches the refusal.
    ///
    /// Every difference but the two schema versions is
    /// `export.manifest_malformed`. A manifest declaring a schema version
    /// this build does not support is refused by the schema rules instead,
    /// because it is a document this build has no business reading rather
    /// than a broken one.
    #[test]
    fn a_damaged_manifest_is_refused_with_the_documented_code(
        plan in support::manifest_plan(),
        damage in support::breaking_manifest_damage(),
        key in proptest::sample::select(support::MANIFEST_REQUIRED_KEYS.to_vec()),
        row in 0_usize..8,
    ) {
        let body = support::damaged_manifest(&plan, damage, key, row);
        let (_source, read) = read_manifest(body.as_bytes());
        prop_assert!(!damage.tolerated(), "this arm generates only breaking damage");
        let refusal = read.expect_err("a damaged manifest is never read as a manifest");

        prop_assert_eq!(
            Some(refusal.code),
            damage.refusal_code(),
            "a damage is refused with the code its own check answers with"
        );
        manifest_refusal_is_documented(&refusal)?;
        let rendered = serde_json::to_string(&refusal).expect("a diagnostic serialises");
        prop_assert!(
            !rendered.contains(&plan.case_id)
                && !rendered.contains(support::PATH_SHAPED_ID)
                && !rendered.contains("papir_not_a_kind"),
            "a manifest refusal never echoes the document it refused"
        );
    }
}
