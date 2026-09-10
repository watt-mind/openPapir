//! Importing local files into the artefact store, with byte preservation.
//!
//! Import stores the bytes exactly as they were read and records one JSON
//! document per import event. Re-importing bytes already present is not an
//! error: the object is left untouched and a second import event is recorded
//! against it, so an import-event count is history, not an anomaly
//! (`docs/archive-layout.md`).
//!
//! The original filename is recorded as an attribute of the import event and
//! is never joined into a path and never reported, so a traversal segment in
//! an imported name cannot escape the root and cannot reach the output.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive::lock::WriterLock;
use crate::archive::{Archive, IMPORTS_DIR, SUPPORTED_SCHEMA_VERSION, limits, objects, paths};
use crate::clock;
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::ident;
use crate::records::document::{self, Record};
use crate::records::submission::{self, FileRef, Submission};

/// One stored artefact, as reported to the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Artefact {
    /// The algorithm-qualified digest of the stored bytes.
    pub digest: String,
    /// The number of bytes stored.
    pub byte_length: u64,
    /// The identifier of the import event this import recorded.
    pub import_event: String,
    /// Whether this import created the object or found it already present.
    pub created_object: bool,
    /// How many import events already referenced the object, for a duplicate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_import_count: Option<u64>,
    /// When the object was first imported, for a duplicate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_imported_at: Option<String>,
}

/// What one import operation reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Imported {
    /// How many inputs created an object.
    pub imported: u64,
    /// How many inputs were already present.
    pub duplicates: u64,
    /// One entry per input, in the order the inputs were given.
    pub artefacts: Vec<Artefact>,
}

/// What one import that also records a submission reports.
///
/// The import's own fields stay where a caller of plain `import` already
/// finds them, and the submission the same invocation recorded is the one
/// field added to them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportedIntoCase {
    /// Everything plain `import` reports, at the same place in the object.
    #[serde(flatten)]
    pub imported: Imported,
    /// The submission recorded in the same invocation, naming every import.
    pub submission: Submission,
}

/// The value an import-event record carries in `record_kind`.
pub const EVENT_KIND: &str = "import_event";

/// The `source` an import event carries when a restored export introduced
/// the object rather than a file the user named on the command line.
///
/// The field is absent on every event `import` writes, which is what the
/// user's own import has always looked like, so an archive written by an
/// earlier build reads unchanged and an event without the field means the
/// user imported a local file (`docs/archive-layout.md`).
pub const SOURCE_EXPORT: &str = "export";

/// One import event record, stored as `records/imports/<id>.json`.
///
/// The record is public so that another record kind can name the import event
/// that introduced an artefact. The original filename stays an attribute: it
/// is never joined into a path and never reported (`docs/archive-layout.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportEvent {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u64,
    /// The number of bytes the import stored or found already present.
    pub byte_length: u64,
    /// Whether this import created the object or found it already present.
    pub created_object: bool,
    /// The algorithm-qualified digest of the artefact.
    pub digest: String,
    /// The import event's own identifier, minted by openPapir.
    pub id: String,
    /// When openPapir recorded the import.
    pub imported_at: String,
    /// The original filename, an attribute only, never joined into a path.
    pub original_filename: String,
    /// The record kind, always `import_event`.
    pub record_kind: String,
    /// Where the bytes came from, present only when they came from an
    /// export. An absent field is the user's own import of a local file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Record for ImportEvent {
    const KIND: &'static str = EVENT_KIND;
    const DIRECTORY: &'static str = IMPORTS_DIR;

    fn id(&self) -> &str {
        &self.id
    }

    fn record_kind(&self) -> &str {
        &self.record_kind
    }
}

/// An input accepted by the pre-allocation checks, ready to be streamed.
#[derive(Debug)]
struct Planned {
    path: PathBuf,
    original_filename: String,
}

/// Import local files into the archive at `root`.
///
/// # Errors
///
/// Returns any refusal of the archive, cap, path-safety, lock, write, or
/// integrity conditions in `docs/error-contract.md`.
pub fn import(root: &Path, inputs: &[PathBuf]) -> Result<Imported> {
    let mut warnings = Vec::new();
    match run(root, inputs, &mut warnings) {
        Ok(data) => Ok(Outcome { data, warnings }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn run(
    root: &Path,
    inputs: &[PathBuf],
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Imported, Diagnostic> {
    let names = checked_names(inputs)?;
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    store_named(archive.root(), inputs, names, warnings)
}

/// Import local files into an archive and record one submission naming them.
///
/// It is `submission add --file` from the other side: the same writer lock,
/// the same import, and the same record. Every imported artefact is
/// referenced with the default role, because this form takes no role of its
/// own.
///
/// # Errors
///
/// Returns any refusal of `import` or of `submission add`.
pub fn import_into_case(
    root: &Path,
    inputs: &[PathBuf],
    case_id: &str,
    description: &str,
    stated_date: Option<&str>,
) -> Result<ImportedIntoCase> {
    let files: Vec<FileRef> = inputs
        .iter()
        .map(|path| FileRef {
            path: path.clone(),
            role: None,
        })
        .collect();
    let outcome = submission::add_with_files(root, case_id, description, stated_date, &[], &files)?;
    Ok(Outcome {
        data: ImportedIntoCase {
            imported: outcome.data.imported.unwrap_or_default(),
            submission: outcome.data.submission,
        },
        warnings: outcome.warnings,
    })
}

/// Check the caps that need only the command line, before an archive opens.
///
/// The result is the original filename of each input, in input order, which
/// is what the plan below needs and the only thing read from the paths here.
pub(crate) fn checked_names(inputs: &[PathBuf]) -> std::result::Result<Vec<String>, Diagnostic> {
    limits::check_file_count(inputs.len() as u64)?;
    argument_names(inputs)
}

/// Store every input into an archive whose writer lock the caller holds.
///
/// `names` is what [`checked_names`] returned for the same inputs, so the
/// caller can refuse an unusable command line before it opens anything.
pub(crate) fn store_named(
    root: &Path,
    inputs: &[PathBuf],
    names: Vec<String>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Imported, Diagnostic> {
    let planned = plan(inputs, names)?;
    let mut read_total = 0_u64;
    let mut histories = Histories::default();
    let mut artefacts = Vec::new();
    for (index, input) in planned.iter().enumerate() {
        artefacts.push(store_one(
            root,
            input,
            index as u64,
            &mut read_total,
            &mut histories,
            warnings,
        )?);
    }
    let imported = artefacts
        .iter()
        .filter(|artefact| artefact.created_object)
        .count() as u64;
    Ok(Imported {
        imported,
        duplicates: artefacts.len() as u64 - imported,
        artefacts,
    })
}

/// Check the caps that need only the command line, before any archive is
/// touched: the number of inputs and the length of each original filename.
fn argument_names(inputs: &[PathBuf]) -> std::result::Result<Vec<String>, Diagnostic> {
    let mut names = Vec::with_capacity(inputs.len());
    for (index, path) in inputs.iter().enumerate() {
        let index = index as u64;
        let original_filename = original_filename(path, index)?;
        limits::check_filename_length(original_filename.len() as u64, index)?;
        names.push(original_filename);
    }
    Ok(names)
}

/// Check every remaining cap before a byte is read or allocated.
fn plan(inputs: &[PathBuf], names: Vec<String>) -> std::result::Result<Vec<Planned>, Diagnostic> {
    let mut planned = Vec::with_capacity(inputs.len());
    let mut declared_total = 0_u64;
    for ((index, path), original_filename) in inputs.iter().enumerate().zip(names) {
        let index = index as u64;
        // An early refusal, so that a link named third does not stop the
        // operation after the first two inputs were already stored. It is not
        // the defence: the no-follow open in `store_one` is, and it refuses
        // the same path again without a stat of its own.
        if paths::is_symlink(path) {
            return Err(input_symlink_refusal());
        }
        let metadata = fs::symlink_metadata(path).map_err(|_| unusable_input(index))?;
        if !metadata.is_file() {
            return Err(unusable_input(index));
        }
        limits::check_file_size(metadata.len(), index)?;
        declared_total += metadata.len();
        limits::check_import_bytes(declared_total, index)?;
        planned.push(Planned {
            path: path.clone(),
            original_filename,
        });
    }
    Ok(planned)
}

/// The input's final path component, kept as an attribute and never joined.
fn original_filename(path: &Path, index: u64) -> std::result::Result<String, Diagnostic> {
    path.file_name()
        .map(OsStr::to_string_lossy)
        .map(|name| name.into_owned())
        .ok_or_else(|| unusable_input(index))
}

/// The refusal for an input that is a symbolic link.
///
/// `scope` is `input`: the path is outside the archive, so no `archive_path`
/// exists for it, and the contract's value set names `input` for exactly this
/// case. Which input it was is deliberately not reported, because the path
/// itself is user-supplied and no key the contract lists for this code
/// carries one.
fn input_symlink_refusal() -> Diagnostic {
    paths::symlink_refusal(Details::new().text("scope", "input"))
}

/// The refusal for an input that is not a readable regular file.
fn unusable_input(index: u64) -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "An input is not a readable regular file.",
        Details::new()
            .text("argument", "file")
            .int("input_index", index),
    )
}

/// Stream one input into the store and record its import event.
///
/// `histories` is the operation's own view of the import events, so that a
/// batch of inputs costs at most one read of them rather than one per input.
fn store_one(
    root: &Path,
    input: &Planned,
    index: u64,
    read_total: &mut u64,
    histories: &mut Histories,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Artefact, Diagnostic> {
    let mut source = paths::open_no_follow(&input.path).map_err(|error| {
        if paths::is_no_follow_refusal(&error) {
            input_symlink_refusal()
        } else {
            unusable_input(index)
        }
    })?;
    let stored = objects::store(root, &mut source, index, read_total)?;
    warnings.extend(stored.warnings);

    let digest = format!("{}:{}", objects::ALGORITHM, stored.digest);
    // Only a duplicate reports a history, and only a duplicate asks for one.
    let history = if stored.created_object {
        History::default()
    } else {
        histories.of(root, &digest)
    };
    let event = ImportEvent {
        archive_schema_version: u64::from(SUPPORTED_SCHEMA_VERSION),
        byte_length: stored.byte_length,
        created_object: stored.created_object,
        digest: digest.clone(),
        id: ident::new_id()?,
        imported_at: clock::now_rfc3339(),
        original_filename: input.original_filename.clone(),
        record_kind: EVENT_KIND.to_owned(),
        source: None,
    };
    warnings.extend(document::write_record(root, &event)?);
    histories.record(&event);
    Ok(Artefact {
        digest,
        byte_length: stored.byte_length,
        import_event: event.id,
        created_object: stored.created_object,
        previous_import_count: (!stored.created_object).then_some(history.count),
        first_imported_at: if stored.created_object {
            None
        } else {
            history.first_imported_at
        },
    })
}

/// How many import events already reference a digest, and the earliest.
#[derive(Debug, Default, Clone)]
struct History {
    count: u64,
    first_imported_at: Option<String>,
}

impl History {
    /// Fold one further event for the same digest into the history.
    fn add(&mut self, imported_at: &str) {
        self.count += 1;
        if self
            .first_imported_at
            .as_ref()
            .is_none_or(|earliest| imported_at < earliest.as_str())
        {
            self.first_imported_at = Some(imported_at.to_owned());
        }
    }
}

/// The import events one operation has to know about, keyed by digest.
///
/// Only a duplicate reports a history, so the records are read at most once
/// for a whole operation and not at all when every input is new: importing a
/// directory costs one pass over the events rather than one per file. Every
/// event the same operation writes is folded in as it is written, so the
/// second duplicate of one digest counts the first exactly as it did when
/// each input re-read the directory.
#[derive(Debug, Default)]
struct Histories {
    by_digest: Option<HashMap<String, History>>,
}

impl Histories {
    /// The history of one digest, reading the stored events on first use.
    fn of(&mut self, root: &Path, digest: &str) -> History {
        self.by_digest
            .get_or_insert_with(|| stored_histories(root))
            .get(digest)
            .cloned()
            .unwrap_or_default()
    }

    /// Fold an event this operation has just written into what was read.
    ///
    /// Nothing is folded in when the events have not been read: the record is
    /// already on disk, so a later read sees it once and only once.
    fn record(&mut self, event: &ImportEvent) {
        if let Some(by_digest) = self.by_digest.as_mut() {
            by_digest
                .entry(event.digest.clone())
                .or_default()
                .add(&event.imported_at);
        }
    }
}

/// Read every stored import-event record into a history per digest.
///
/// A record that cannot be parsed is not counted. Reporting a malformed
/// record is `record.malformed`, which the error contract reserves and this
/// build does not implement.
fn stored_histories(root: &Path) -> HashMap<String, History> {
    let mut histories: HashMap<String, History> = HashMap::new();
    let Ok(entries) = fs::read_dir(root.join(IMPORTS_DIR)) else {
        return histories;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<ImportEvent>(&text) else {
            continue;
        };
        histories
            .entry(event.digest)
            .or_default()
            .add(&event.imported_at);
    }
    histories
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;

    fn archive_with(contents: &[(&str, &[u8])]) -> (tempfile::TempDir, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let inputs = tempfile::tempdir().unwrap();
        archive::init(root.path()).unwrap();
        for (name, bytes) in contents {
            fs::write(inputs.path().join(name), bytes).unwrap();
        }
        (root, inputs)
    }

    #[test]
    fn stored_bytes_are_preserved_and_the_object_is_read_only() {
        let (root, inputs) = archive_with(&[("note.txt", b"synthetic bytes\n")]);
        let outcome = import(root.path(), &[inputs.path().join("note.txt")]).unwrap();
        assert_eq!(outcome.data.imported, 1);
        assert_eq!(outcome.data.duplicates, 0);
        let artefact = &outcome.data.artefacts[0];
        assert!(artefact.created_object);
        assert_eq!(artefact.byte_length, 16);
        let hex = artefact.digest.strip_prefix("sha256:").unwrap();
        let stored = objects::absolute_path(root.path(), hex);
        assert_eq!(fs::read(&stored).unwrap(), b"synthetic bytes\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&stored).unwrap().permissions().mode() & 0o777,
                0o400
            );
        }
    }

    #[test]
    fn a_duplicate_import_records_a_second_event_against_the_object() {
        let (root, inputs) = archive_with(&[("a.txt", b"same"), ("b.txt", b"same")]);
        let first = import(root.path(), &[inputs.path().join("a.txt")]).unwrap();
        let second = import(root.path(), &[inputs.path().join("b.txt")]).unwrap();
        assert_eq!(second.data.imported, 0);
        assert_eq!(second.data.duplicates, 1);
        let artefact = &second.data.artefacts[0];
        assert!(!artefact.created_object);
        assert_eq!(artefact.previous_import_count, Some(1));
        assert_eq!(artefact.digest, first.data.artefacts[0].digest);
        assert!(artefact.first_imported_at.as_ref().unwrap().ends_with('Z'));
        assert!(first.data.artefacts[0].previous_import_count.is_none());
        let events = fs::read_dir(root.path().join(IMPORTS_DIR)).unwrap().count();
        assert_eq!(events, 2, "each import records its own event");
    }

    #[test]
    fn duplicates_inside_one_operation_count_the_events_it_wrote_itself() {
        let (root, inputs) = archive_with(&[
            ("a.txt", b"same"),
            ("b.txt", b"other"),
            ("c.txt", b"same"),
            ("d.txt", b"same"),
        ]);
        let outcome = import(
            root.path(),
            &[
                inputs.path().join("a.txt"),
                inputs.path().join("b.txt"),
                inputs.path().join("c.txt"),
                inputs.path().join("d.txt"),
            ],
        )
        .unwrap();
        assert_eq!(outcome.data.imported, 2);
        assert_eq!(outcome.data.duplicates, 2);
        let counts: Vec<Option<u64>> = outcome
            .data
            .artefacts
            .iter()
            .map(|artefact| artefact.previous_import_count)
            .collect();
        assert_eq!(counts, vec![None, None, Some(1), Some(2)]);
        let first = &outcome.data.artefacts[0];
        for later in &outcome.data.artefacts[2..] {
            assert_eq!(later.digest, first.digest);
            assert!(later.first_imported_at.is_some());
        }
        let events = fs::read_dir(root.path().join(IMPORTS_DIR)).unwrap().count();
        assert_eq!(events, 4, "each input records its own event");
    }

    #[test]
    fn a_history_read_once_reports_what_a_read_per_input_reported() {
        let (root, inputs) = archive_with(&[("a.txt", b"same"), ("b.txt", b"same")]);
        import(root.path(), &[inputs.path().join("a.txt")]).unwrap();
        import(root.path(), &[inputs.path().join("b.txt")]).unwrap();
        let outcome = import(root.path(), &[inputs.path().join("a.txt")]).unwrap();
        let artefact = &outcome.data.artefacts[0];
        let indexed = stored_histories(root.path());
        let history = indexed.get(&artefact.digest).unwrap();
        assert_eq!(artefact.previous_import_count, Some(2));
        assert_eq!(history.count, 3, "the third event is stored as well");
        assert_eq!(artefact.first_imported_at, history.first_imported_at);
    }

    #[test]
    fn the_original_filename_is_an_attribute_and_never_a_path() {
        let (root, inputs) = archive_with(&[("..%2f..%2fescape.txt", b"payload")]);
        let outcome = import(root.path(), &[inputs.path().join("..%2f..%2fescape.txt")]).unwrap();
        let rendered = serde_json::to_string(&outcome.data).unwrap();
        assert!(!rendered.contains("escape"), "no filename reaches output");
        let event_dir = root.path().join(IMPORTS_DIR);
        let entry = fs::read_dir(&event_dir).unwrap().next().unwrap().unwrap();
        let event: ImportEvent =
            serde_json::from_str(&fs::read_to_string(entry.path()).unwrap()).unwrap();
        assert_eq!(event.original_filename, "..%2f..%2fescape.txt");
        assert_eq!(event.record_kind, "import_event");
        assert!(
            entry.file_name().to_string_lossy().ends_with(".json"),
            "records are named by a minted identifier"
        );
    }

    #[test]
    fn a_stored_object_of_a_different_length_is_reported_as_damage() {
        let (root, inputs) = archive_with(&[("a.txt", b"content")]);
        let outcome = import(root.path(), &[inputs.path().join("a.txt")]).unwrap();
        let hex = outcome.data.artefacts[0]
            .digest
            .strip_prefix("sha256:")
            .unwrap()
            .to_owned();
        let stored = objects::absolute_path(root.path(), &hex);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&stored, fs::Permissions::from_mode(0o600)).unwrap();
        }
        #[cfg(not(unix))]
        paths::clear_read_only(&stored).unwrap();
        fs::write(&stored, b"damaged store").unwrap();
        let refusal = import(root.path(), &[inputs.path().join("a.txt")]).unwrap_err();
        assert_eq!(refusal.error.code, codes::INTEGRITY_LENGTH_MISMATCH);
        assert_eq!(fs::read(&stored).unwrap(), b"damaged store");
    }

    #[test]
    fn an_input_that_is_not_a_readable_file_is_a_usage_refusal() {
        let (root, inputs) = archive_with(&[]);
        let refusal = import(root.path(), &[inputs.path().join("absent.txt")]).unwrap_err();
        assert_eq!(refusal.error.code, codes::USAGE_ARGUMENTS);
        assert_eq!(refusal.error.exit_code(), 2);
        let refusal = import(root.path(), &[inputs.path().to_path_buf()]).unwrap_err();
        assert_eq!(refusal.error.code, codes::USAGE_ARGUMENTS);
    }
}
