//! The synthetic archive the scan benchmark measures against.
//!
//! Nothing here reads a fixture from disk and nothing here is derived from
//! real correspondence: every byte the archive holds comes from the counter
//! and the mixing function below, so the same `cases` argument builds the
//! same archive on every machine.
//!
//! The archive is built through the library API rather than the binary. A
//! record written through `openpapir_core` costs one archive open, one lock,
//! and one atomic write; the same record written through the binary costs a
//! process start as well, and at ten thousand cases that is four ten
//! thousand process starts of setup before the first measurement runs.
//!
//! Every step stays inside the input caps of `docs/architecture.md`: the
//! objects are imported in batches of the per-import file cap, and each case
//! carries two tags of a few bytes each.

use std::fs;
use std::path::{Path, PathBuf};

use openpapir_core::records;
use openpapir_core::records::case::Status;
use tempfile::TempDir;

/// The number of cases the benchmark builds unless it is told another.
pub const DEFAULT_CASES: usize = 10_000;

/// The environment variable that names another number of cases.
pub const CASES_VARIABLE: &str = "OPENPAPIR_BENCH_CASES";

/// The word one case in [`QUERY_ONE_IN`] carries in its notes.
///
/// `case list --query` is timed against it, so the query matches a small
/// share of the archive and the scan still has to read every record to know
/// that. The share is fixed, so the benchmark can check that the scan it
/// timed matched what it was built to match.
pub const QUERY_WORD: &str = "escalated";

/// How many cases carry [`QUERY_WORD`]: one in this many, starting at the
/// first, so an archive of `cases` holds `cases.div_ceil(QUERY_ONE_IN)`.
pub const QUERY_ONE_IN: usize = 100;

/// Files per import, which is the per-import file cap itself.
const IMPORT_BATCH: usize = 1_000;

/// The bytes in one synthetic object.
const OBJECT_BYTES: usize = 256;

/// One built archive and the identifiers a measurement needs to name.
pub struct Synthetic {
    /// Kept so that the temporary tree outlives the measurements.
    directory: TempDir,
    root: PathBuf,
    /// Every case identifier, in the order the cases were created.
    pub case_ids: Vec<String>,
}

impl Synthetic {
    /// The archive root as the command line spells it.
    #[must_use]
    pub fn archive_string(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// A destination under the temporary tree, which `case export` creates.
    #[must_use]
    pub fn destination(&self, name: &str) -> String {
        self.directory
            .path()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }
}

/// The smallest archive the measurements have something to measure on.
///
/// The benchmark exports one case and deletes another, so there have to be
/// two of them.
pub const MINIMUM_CASES: usize = 2;

/// The number of cases to build: the environment's, or the default.
///
/// A setting that is absent or is not a number at all gives the documented
/// default. A number below the minimum is refused rather than rounded up: it
/// was asked for deliberately, and a benchmark that quietly measured a size
/// nobody chose would report a ceiling for a shape that was never run.
///
/// # Panics
///
/// Panics when the variable names a number below [`MINIMUM_CASES`], naming
/// the variable and the value.
#[must_use]
pub fn requested_cases() -> usize {
    let Some(cases) = std::env::var(CASES_VARIABLE)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
    else {
        return DEFAULT_CASES;
    };
    assert!(
        cases >= MINIMUM_CASES,
        "{CASES_VARIABLE} is {cases}, and the benchmark needs at least \
         {MINIMUM_CASES}: it exports one case and deletes another"
    );
    cases
}

/// Build an archive of `cases` cases, submissions, receipts, and
/// associations over `2 * cases` distinct objects.
///
/// # Panics
///
/// Panics when any setup step is refused, because a benchmark that measured a
/// half-built archive would report a number for a shape that does not exist.
#[must_use]
pub fn build(cases: usize) -> Synthetic {
    let directory = tempfile::tempdir().expect("create the temporary root");
    let root = directory.path().join("archive");
    fs::create_dir(&root).expect("create the archive directory");
    openpapir_core::init(&root).expect("initialise the synthetic archive");

    let digests = import_objects(&root, directory.path(), 2 * cases);
    let mut case_ids = Vec::with_capacity(cases);
    for index in 0..cases {
        case_ids.push(record_one(
            &root,
            index,
            &digests[index],
            &digests[cases + index],
        ));
        progress("cases recorded", index + 1, cases);
    }
    Synthetic {
        directory,
        root,
        case_ids,
    }
}

/// One stored object, as the records that name it need it.
struct Stored {
    digest: String,
    import_event: String,
}

/// Write and import `count` distinct objects, in per-import-cap batches.
///
/// Each batch's input files are removed once they are stored, so the
/// temporary tree never holds two copies of the whole set at once.
fn import_objects(root: &Path, temporary: &Path, count: usize) -> Vec<Stored> {
    let inputs = temporary.join("inputs");
    let mut digests = Vec::with_capacity(count);
    for start in (0..count).step_by(IMPORT_BATCH) {
        let end = (start + IMPORT_BATCH).min(count);
        fs::create_dir(&inputs).expect("create the input directory");
        let batch: Vec<PathBuf> = (start..end)
            .map(|index| {
                let path = inputs.join(format!("object-{index:07}.bin"));
                fs::write(&path, payload(index)).expect("write a synthetic object");
                path
            })
            .collect();
        let imported = openpapir_core::import(root, &batch).expect("import a batch of objects");
        assert_eq!(
            imported.data.duplicates, 0,
            "every synthetic object is distinct"
        );
        digests.extend(imported.data.artefacts.iter().map(|artefact| Stored {
            digest: artefact.digest.clone(),
            import_event: artefact.import_event.clone(),
        }));
        fs::remove_dir_all(&inputs).expect("remove the imported inputs");
        progress("objects imported", digests.len(), count);
    }
    assert_eq!(digests.len(), count, "one digest per synthetic object");
    digests
}

/// Say how far the setup has come, at each tenth of the way.
///
/// Building the archive takes minutes and the measurements take under a
/// second, so a run that printed nothing until the end would look stopped.
/// Only counts are printed.
fn progress(what: &str, done: usize, total: usize) {
    let step = (total / 10).max(1);
    if done.is_multiple_of(step) || done == total {
        println!("setup: {what} {done}/{total}");
    }
}

/// Record one case, its submission, its receipt, and its association.
///
/// The receipt names its own import event. Resolving an artefact to its
/// earliest import event instead reads every import-event record in the
/// archive, so leaving it out would make the setup quadratic in `cases` and
/// measure the generator rather than the scans.
fn record_one(root: &Path, index: usize, submitted: &Stored, received: &Stored) -> String {
    let title = format!("Synthetic case {index:07}");
    let notes = notes(index);
    let tags = vec!["synthetic".to_owned(), format!("batch-{}", index % 8)];
    let status = if index.is_multiple_of(10) {
        Status::Closed
    } else {
        Status::Open
    };
    let case = records::case::create_with(root, &title, Some(&notes), status, &tags)
        .expect("record a synthetic case")
        .data
        .case;
    let submission = records::submission::add(
        root,
        &case.id,
        "Synthetic submission of one stored object.",
        Some("2026-01-13"),
        std::slice::from_ref(&submitted.digest),
    )
    .expect("record a synthetic submission")
    .data
    .submission;
    let receipt = records::receipt::add(
        root,
        &received.digest,
        Some(&received.import_event),
        Some("Synthetic receipt"),
    )
    .expect("record a synthetic receipt")
    .data
    .receipt;
    let candidate = format!(
        "{}:moderate:The synthetic reference matches.",
        submission.id
    );
    records::association::create(root, &receipt.id, "candidate", &[candidate], None)
        .expect("record a synthetic association");
    case.id
}

/// The notes of the case at `index`, one in [`QUERY_ONE_IN`] queryable.
fn notes(index: usize) -> String {
    if index.is_multiple_of(QUERY_ONE_IN) {
        format!("Synthetic notes for case {index:07}, {QUERY_WORD}.")
    } else {
        format!("Synthetic notes for case {index:07}.")
    }
}

/// The bytes of the object at `index`.
///
/// The mixing function is a bijection over its input, so two indices never
/// produce the same first eight bytes and the content-addressed store keeps
/// one object per index rather than folding duplicates together.
fn payload(index: usize) -> Vec<u8> {
    const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut state = index as u64;
    let mut bytes = Vec::with_capacity(OBJECT_BYTES);
    while bytes.len() < OBJECT_BYTES {
        state = state.wrapping_add(GOLDEN);
        let mut mixed = state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^= mixed >> 31;
        bytes.extend_from_slice(&mixed.to_le_bytes());
    }
    bytes
}
