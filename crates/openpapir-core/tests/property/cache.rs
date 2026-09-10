//! Invariant: the cached answer is the scanned answer.
//!
//! `docs/archive-layout.md` decides that any index is a rebuildable cache
//! which is never authoritative, so `cache/import-events-by-digest.json` may
//! be deleted, damaged, or left behind by an older state of the archive
//! without changing a single answer. The property here states exactly that:
//! for a generated archive, the two readers that consult the index give the
//! same answer with a current cache, with no cache at all, with a cache full
//! of bytes that are not a document, and with a genuinely stale cache the
//! archive has since moved past.
//!
//! The file is named here rather than imported, because the index is internal
//! to the crate and the name is a decision of the layout document. A test
//! that had to be told the name by the code it tests could not notice the
//! name changing.

use std::fs;
use std::path::{Path, PathBuf};

use proptest::prelude::*;

use openpapir_core::deletion::plan;
use openpapir_core::records;

use super::support;

/// The index file, as `docs/archive-layout.md` names it.
const INDEX_FILE: &str = "cache/import-events-by-digest.json";

/// Bytes that are not a document of any kind.
const NOT_A_DOCUMENT: &[u8] = b"{\"cache_kind\": \xff\xfe not json at all";

/// What one generated archive holds, in the order it was built.
struct Built {
    /// Kept so that the archive outlives the property.
    directory: tempfile::TempDir,
    root: PathBuf,
    /// Every distinct digest the imports stored, in digest order.
    digests: Vec<String>,
    /// The case the submission belongs to.
    case_id: String,
    /// A cache document the archive has since moved past.
    stale: Vec<u8>,
}

proptest! {
    #![proptest_config(support::config(24))]

    /// The index answers what a scan of the import events answers.
    ///
    /// The four states are the four the module documents: current, absent,
    /// unparseable, and stale. The deletion plan and the import event
    /// `receipt add` resolves are read in each of them and must not differ.
    #[test]
    fn the_cached_answer_is_the_scanned_answer(
        payloads in prop::collection::vec(0_u8..4, 1..8),
    ) {
        let built = build(&payloads);
        let root = built.root.as_path();

        let mut plans = Vec::new();
        for state in states() {
            state(&built);
            plans.push(plan::build(root, &built.case_id, true).expect("a plan is built"));
        }
        for (index, planned) in plans.iter().enumerate().skip(1) {
            prop_assert_eq!(
                planned,
                &plans[0],
                "the deletion plan differed in cache state {}",
                index
            );
        }

        for digest in &built.digests {
            let mut resolved = Vec::new();
            for state in states() {
                state(&built);
                resolved.push(
                    records::receipt::add(root, digest, None, None)
                        .expect("a receipt is recorded")
                        .data
                        .receipt
                        .import_event_id,
                );
            }
            for (index, event) in resolved.iter().enumerate().skip(1) {
                prop_assert_eq!(
                    event,
                    &resolved[0],
                    "the resolved import event differed in cache state {}",
                    index
                );
            }
        }

        // A read that found no cache leaves one behind, so that the next one
        // is answered from it. It is owner-only, exactly as every other file
        // openPapir writes into the archive.
        let index = root.join(INDEX_FILE);
        prop_assert!(index.is_file(), "a read rebuilt the index it did not find");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&index).expect("the index is there").permissions().mode();
            prop_assert_eq!(mode & 0o777, 0o600, "the index is owner-only");
        }
        drop(built.directory);
    }
}

/// The four cache states a reader has to give the same answer in.
fn states() -> [fn(&Built); 4] {
    [current, absent, unparseable, stale]
}

/// Leave whatever the last read wrote, which is a current cache.
fn current(_built: &Built) {}

/// Remove the whole cache directory, not only the file inside it.
fn absent(built: &Built) {
    let _ = fs::remove_dir_all(built.root.join("cache"));
}

/// Put bytes that are not a document where the index belongs.
fn unparseable(built: &Built) {
    write_index(&built.root, NOT_A_DOCUMENT);
}

/// Put back an index the archive has since moved past.
fn stale(built: &Built) {
    write_index(&built.root, &built.stale);
}

/// Replace the index file, whatever is there now.
///
/// The directory and the file are narrowed to owner-only, because openPapir
/// refuses to open an archive whose permissions are wider and the property is
/// about the content of the index rather than about its mode.
fn write_index(root: &Path, content: &[u8]) {
    let path = root.join(INDEX_FILE);
    let directory = path.parent().expect("the index has a directory");
    fs::create_dir_all(directory).expect("the cache directory is created");
    let _ = fs::remove_file(&path);
    fs::write(&path, content).expect("the index is written");
    owner_only(directory, 0o700);
    owner_only(&path, 0o600);
}

/// Narrow one path to owner-only, where the platform has a mode at all.
#[allow(unused_variables)]
fn owner_only(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .expect("the path is narrowed to owner-only");
    }
}

/// Build one archive from the generated payloads.
///
/// Each payload is imported on its own, so a payload that repeats records a
/// second import event against the same object and the history the readers
/// resolve is more than one event long. One case and one submission naming
/// every digest give the deletion plan something to plan.
fn build(payloads: &[u8]) -> Built {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("archive");
    fs::create_dir(&root).expect("the archive directory");
    openpapir_core::init(&root).expect("the archive is initialised");
    let inputs = directory.path().join("inputs");
    fs::create_dir(&inputs).expect("the input directory");

    let mut digests = Vec::new();
    for (index, payload) in payloads.iter().enumerate() {
        let path = inputs.join(format!("input-{index}.bin"));
        fs::write(&path, [*payload; 8]).expect("an input is written");
        let imported = openpapir_core::import(&root, &[path]).expect("an input is imported");
        digests.push(imported.data.artefacts[0].digest.clone());
    }
    digests.sort();
    digests.dedup();

    let case = records::case::create(&root, "Generated case", None)
        .expect("a case is recorded")
        .data
        .case;
    records::submission::add(&root, &case.id, "A generated submission.", None, &digests)
        .expect("a submission is recorded");

    // Warm the index, keep a copy of it, and then move the archive past that
    // copy: the copy is a genuinely stale document rather than one this test
    // edited into looking stale. The later import repeats the first input, so
    // the event it records is one the copy does not know and one the deletion
    // plan has to list, which is what makes a stale index a wrong answer.
    plan::build(&root, &case.id, true).expect("a plan is built");
    let stale = fs::read(root.join(INDEX_FILE)).expect("the warmed index is there");
    openpapir_core::import(&root, &[inputs.join("input-0.bin")])
        .expect("the first input is imported again");

    Built {
        directory,
        root,
        digests,
        case_id: case.id,
        stale,
    }
}
