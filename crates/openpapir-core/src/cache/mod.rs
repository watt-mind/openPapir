//! The disposable index under `cache/`, which is never authoritative.
//!
//! `docs/archive-layout.md` decides that plain files are the system of record
//! and that any index is a rebuildable cache which is safe to delete at any
//! time. This module holds the one index that exists:
//! `cache/import-events-by-digest.json`, which maps an artefact digest to the
//! import events recorded against it. Losing it, truncating it, or filling it
//! with nonsense changes no answer: every reader that cannot prove the file
//! current reads the records instead.
//!
//! # What it is for
//!
//! Records are named by a minted identifier, so every question of the form
//! "which import events name this artefact" is a scan of the import events.
//! Three callers ask it: `receipt add` without `--import-event`, the
//! duplicate history of an import, and the deletion planner. The index
//! answers all three from one file.
//!
//! # Freshness, and why a reader is safe without the lock
//!
//! Writing the index is a write, so only the holder of the writer lock ever
//! writes one. Reading it is not, and a reader holds no lock, so a reader
//! cannot assume the import events stay still while it looks at them. The
//! rule that makes that safe is that the cache is trusted only when it can be
//! **proved** current, and that anything else is treated as an absent cache
//! and answered by a scan:
//!
//! - A stamp of the import-event directory is taken before the cache file is
//!   opened and again after it has been parsed. The stamp is a digest of
//!   every entry name the directory holds, sorted, together with the number
//!   of entries and the newest modification time among them, all read from
//!   the same directory listing.
//! - The cache is used only when the two stamps are equal to each other and
//!   to the stamp the document itself records.
//! - An absent file, one that is not a regular file, one over the file cap,
//!   one that does not parse, and one of another kind or version are all the
//!   same answer: no cache.
//!
//! The digest of the entry names is what makes the stamp a change detector
//! rather than a heuristic. An import event is written once under an
//! identifier openPapir minted and is never rewritten, so the set of names
//! **is** the set of events: adding one, removing one, or exchanging one for
//! another changes the digest, whatever the count and the timestamps do. The
//! count and the newest modification time are kept beside it because they
//! come free from the same listing, but neither is relied on to notice a
//! change on its own. Nothing here would be safe if it rested on timestamps
//! alone: two writes inside one filesystem's timestamp granularity, a restore
//! that preserved modification times, and a clock moved backwards all leave a
//! count and a newest time unchanged, and all change the names.
//!
//! A concurrent writer changes the directory, which changes the stamp, which
//! makes the reader scan. The check is therefore wrong in one direction only:
//! it can refuse a cache that was in fact current, which costs a scan, and it
//! cannot accept one that is not. A writer holding the lock runs the same
//! check, which is exact for it because nothing else may write.
//!
//! What a cache proved current says is then used verbatim, with one further
//! read: the callers that name a record from it confirm that record before
//! they write its identifier into a new one. The boundary that makes that
//! enough is the one `records/` already rests on, the writer lock and
//! owner-only permissions on every file. A local process that can rewrite the
//! index can rewrite a record just as easily, and openPapir does not defend
//! the archive against an attacker who is already its owner.
//!
//! Nothing here is reported. The index holds digests and identifiers, which
//! the privacy rule of `docs/error-contract.md` keeps out of every message,
//! and no function in this module returns anything that reaches output.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::archive::import::{EVENT_KIND, ImportEvent};
use crate::archive::{CACHE_DIR, IMPORTS_DIR, limits, paths, write};
use crate::error::{Diagnostic, stages};
use crate::records::document;

/// The index file, as `docs/archive-layout.md` names it.
const IMPORT_EVENTS_FILE: &str = "import-events-by-digest.json";

/// The `cache_kind` the index document carries.
const IMPORT_EVENTS_KIND: &str = "import_events_by_digest";

/// The shape version of the index document itself.
///
/// It is the cache's own version and has nothing to do with the archive
/// schema version: a build that does not recognise it rebuilds the file
/// rather than refusing anything. It went to `2` when the stamp gained the
/// digest of the entry names, so that an index an earlier build wrote, whose
/// stamp was the count and the newest modification time alone, is rebuilt
/// rather than trusted.
const CACHE_SCHEMA_VERSION: u64 = 2;

/// The write stage the atomic write procedure reports this file under.
///
/// A cached file is a record write: the stage names the kind of path being
/// written and the contract enumerates the kinds, so the index does not get a
/// stage of its own ([`crate::error::stages`]).
const STAGE: &str = stages::RECORD_WRITE;

/// One import event, reduced to what the three callers ask of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Event {
    /// The import event's own identifier.
    pub(crate) id: String,
    /// When openPapir recorded the import.
    pub(crate) imported_at: String,
}

/// The import events of an archive, keyed by artefact digest.
///
/// The events of one digest are ordered by identifier, which is the order
/// `records::document::list_records` returns them in, so a caller reading the
/// index sees them in the order it saw them from a scan.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ImportEvents {
    /// Every digest the archive holds an import event for.
    by_digest: BTreeMap<String, Vec<Event>>,
    /// How many documents in the import-event directory could not be read as
    /// an import event. A scan reports that as `record.malformed`, so the
    /// index carries the count and reports the same refusal.
    unreadable: u64,
}

impl ImportEvents {
    /// Refuse exactly as a scan of the same directory would.
    ///
    /// A caller that would have used `list_records` calls this first, so that
    /// an unreadable import-event document is still `record.malformed` with
    /// the same count rather than a record the index silently passed over.
    ///
    /// # Errors
    ///
    /// Returns `record.malformed` when any document could not be read.
    pub(crate) fn refuse_unreadable(&self) -> Result<(), Diagnostic> {
        if self.unreadable > 0 {
            return Err(document::malformed(EVENT_KIND, self.unreadable));
        }
        Ok(())
    }

    /// The events recorded against one digest, ordered by identifier.
    pub(crate) fn of(&self, digest: &str) -> &[Event] {
        self.by_digest.get(digest).map_or(&[], Vec::as_slice)
    }

    /// The earliest event of one digest, by `imported_at` and then by
    /// identifier, so that the choice is the same on every run.
    pub(crate) fn earliest(&self, digest: &str) -> Option<&Event> {
        self.of(digest).iter().min_by(|left, right| {
            (&left.imported_at, &left.id).cmp(&(&right.imported_at, &right.id))
        })
    }

    /// Every digest the index knows an event for, in digest order.
    pub(crate) fn digests(&self) -> impl Iterator<Item = (&String, &[Event])> {
        self.by_digest
            .iter()
            .map(|(digest, events)| (digest, events.as_slice()))
    }

    /// Whether the record the index names for one event is really there.
    ///
    /// The index is openPapir's own accelerator and is not a record, so
    /// nothing it says may be written into a record without the record it
    /// names being read first. The read is the one a named `--import-event`
    /// already goes through, so confirming costs exactly what naming an
    /// event costs: one record read.
    ///
    /// A record that is not there, that cannot be read, or that records
    /// another digest all answer `false`, and every caller answers a `false`
    /// by reading the records themselves rather than by refusing: the index
    /// may be wrong, and the records are what is true.
    pub(crate) fn confirms(root: &Path, event: &Event, digest: &str) -> bool {
        document::read_record::<ImportEvent>(root, &event.id, "import_event_id")
            .is_ok_and(|record| record.digest == digest)
    }

    /// Fold one event the same operation has just written into the index.
    ///
    /// The event keeps the index in identifier order, so an index the caller
    /// then writes is the index a scan would have built.
    pub(crate) fn record(&mut self, event: &ImportEvent) {
        let events = self.by_digest.entry(event.digest.clone()).or_default();
        let folded = Event {
            id: event.id.clone(),
            imported_at: event.imported_at.clone(),
        };
        match events.binary_search_by(|held| held.id.cmp(&folded.id)) {
            Ok(_) => {}
            Err(position) => events.insert(position, folded),
        }
    }
}

/// The index of an archive whose writer lock the caller holds.
///
/// The cache is used when it can be proved current and is rebuilt from the
/// records when it cannot. A rebuild writes the file back, which is why this
/// is for a caller under the lock; a failure to write it is not reported,
/// because a cache that could not be written is a cache that is absent and
/// the answer is the same either way.
pub(crate) fn import_events(root: &Path) -> ImportEvents {
    if let Some(index) = read_import_events(root) {
        return index;
    }
    let before = stamp(root);
    let index = scanned_import_events(root);
    if before.is_some() && before == stamp(root) {
        write_import_events(root, &index);
    }
    index
}

/// The index of an archive, without ever writing one.
///
/// A caller that may not hold the writer lock uses this: it takes the cache
/// when the cache can be proved current and reads the records when it cannot,
/// and it writes nothing either way. Writing an index is a write, and a
/// reader holds no lock.
pub(crate) fn cached_import_events(root: &Path) -> ImportEvents {
    read_import_events(root).unwrap_or_else(|| scanned_import_events(root))
}

/// The index of an archive, read only, without a rebuild.
///
/// `None` says the cache could not be proved current, which is the same
/// answer as no cache at all. Nothing is written and no scan is run, so this
/// is what a caller uses when it may not need the index at all.
pub(crate) fn read_import_events(root: &Path) -> Option<ImportEvents> {
    let before = stamp(root)?;
    let document = read_document(&index_path(root))?;
    let document: Document = serde_json::from_str(&document).ok()?;
    if document.cache_kind != IMPORT_EVENTS_KIND
        || document.cache_schema_version != CACHE_SCHEMA_VERSION
        || document.source != before
        || stamp(root)? != before
    {
        return None;
    }
    Some(ImportEvents {
        by_digest: document.digests,
        unreadable: document.unreadable_records,
    })
}

/// Write the index of an archive whose writer lock the caller holds.
///
/// The stamp is taken here rather than by the caller, so the document
/// describes the directory as it is at the moment the file is published. Any
/// failure is dropped: the cache is disposable, and an operation that stored
/// what the user asked for does not fail because an accelerator could not be
/// saved.
pub(crate) fn write_import_events(root: &Path, index: &ImportEvents) {
    let Some(source) = stamp(root) else {
        return;
    };
    let document = Document {
        cache_kind: IMPORT_EVENTS_KIND.to_owned(),
        cache_schema_version: CACHE_SCHEMA_VERSION,
        digests: index.by_digest.clone(),
        source,
        unreadable_records: index.unreadable,
    };
    let Ok(value) = serde_json::to_value(&document) else {
        return;
    };
    let content = format!("{value}\n");
    if content.len() as u64 > limits::MAX_FILE_BYTES {
        return;
    }
    let directory = root.join(CACHE_DIR);
    let _ = write::replace_document(
        &directory,
        IMPORT_EVENTS_FILE,
        &format!("{CACHE_DIR}/{IMPORT_EVENTS_FILE}"),
        content.as_bytes(),
        STAGE,
    );
}

/// How many files the cache directory holds.
///
/// `archive check` reports the figure so that a cache is visible rather than
/// invisible, and never as a problem: every file here is openPapir's own
/// rebuildable computation, so a missing one, a stale one, and a damaged one
/// are all nothing at all.
pub(crate) fn file_count(root: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(root.join(CACHE_DIR)) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .count() as u64
}

/// The index document as it is stored, with keys sorted by construction.
#[derive(Debug, Serialize, Deserialize)]
struct Document {
    /// Which cache this is, so a file of another kind is never adopted.
    cache_kind: String,
    /// The shape version of this document.
    cache_schema_version: u64,
    /// The events of each digest, ordered by identifier.
    digests: BTreeMap<String, Vec<Event>>,
    /// The stamp of the import-event directory the document describes.
    source: Stamp,
    /// How many documents could not be read as an import event.
    unreadable_records: u64,
}

/// What the import-event directory looked like when the index was built.
///
/// Every figure is compared for equality and never ordered, so nothing here
/// depends on a clock being monotonic, on two filesystems agreeing, or on a
/// timestamp being comparable across a backup and a restore. A stamp that
/// differs in any figure means the index is not the directory's.
///
/// `entry_digest` is what makes the stamp complete. An import event is
/// written once under a minted identifier and never rewritten, so the sorted
/// entry names determine the set of events exactly; the other two figures
/// come free from the same listing and are kept for what they say, not
/// because anything rests on them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    /// A digest of every entry name the directory holds, sorted, each
    /// followed by a byte that cannot appear in a name, so that no two
    /// different listings render as the same input.
    entry_digest: String,
    /// How many entries the directory holds, of any name.
    entry_count: u64,
    /// The newest modification time among them, in nanoseconds since the
    /// epoch. Absent when the directory holds no entry at all.
    newest_modified_nanos: Option<u64>,
}

/// The path of the index file inside an archive.
fn index_path(root: &Path) -> PathBuf {
    root.join(CACHE_DIR).join(IMPORT_EVENTS_FILE)
}

/// Stamp the import-event directory, without reading a record.
///
/// Every entry is counted, a leftover staging file included, because the
/// point is to notice any change to the directory at all rather than to
/// classify what is in it. `None` says the directory could not be listed or
/// holds a timestamp this build cannot compare, and a caller that cannot
/// stamp the directory treats the cache as absent.
fn stamp(root: &Path) -> Option<Stamp> {
    let entries = fs::read_dir(root.join(IMPORTS_DIR)).ok()?;
    let mut names = Vec::new();
    let mut newest_modified_nanos: Option<u64> = None;
    for entry in entries {
        let entry = entry.ok()?;
        names.push(entry.file_name());
        let nanos = modified_nanos(&entry.metadata().ok()?)?;
        newest_modified_nanos = Some(newest_modified_nanos.map_or(nanos, |held| held.max(nanos)));
    }
    names.sort();
    let mut digest = Sha256::new();
    for name in &names {
        digest.update(name.as_encoded_bytes());
        // A separator no filename can hold, so that two different listings
        // cannot render as one input by running their names together.
        digest.update([0]);
    }
    Some(Stamp {
        entry_digest: format!("{:x}", digest.finalize()),
        entry_count: names.len() as u64,
        newest_modified_nanos,
    })
}

/// One entry's modification time, in nanoseconds since the epoch.
///
/// A time the platform does not report, one before the epoch, and one beyond
/// what fits are all `None`, which makes the whole directory unstampable and
/// the cache unusable. That is the conservative answer: a stamp that could
/// not be taken is never a stamp that matched.
fn modified_nanos(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| u64::try_from(since.as_nanos()).ok())
}

/// Read the index file through one opened handle, bounded and no-follow.
///
/// The handle is the archive's own no-follow open, and the kind and the
/// length are taken from it rather than from a second look at the path, so
/// the file that passes the checks is the file whose bytes are read. Every
/// failure is `None`: the cache is disposable and an unreadable one is an
/// absent one.
fn read_document(path: &Path) -> Option<String> {
    let file = paths::open_no_follow_nonblocking(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limits::MAX_FILE_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(limits::MAX_FILE_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Build the index by reading every import-event record, consulting no cache.
///
/// The reader is the one `list_records` uses, so the index holds exactly the
/// records a scan would have accepted and counts exactly the documents a scan
/// would have reported as malformed. This is the answer every caller falls
/// back to, and the answer every caller must agree with.
pub(crate) fn scanned_import_events(root: &Path) -> ImportEvents {
    let mut index = ImportEvents::default();
    let unreadable = document::visit_records::<ImportEvent, _>(root, |event| {
        index.record(&event);
    });
    index.unreadable = unreadable;
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;
    use crate::error::codes;

    /// An archive holding one import event per payload, in payload order.
    fn archive_with(payloads: &[&[u8]]) -> (tempfile::TempDir, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        let inputs = tempfile::tempdir().unwrap();
        archive::init(root.path()).unwrap();
        for (index, payload) in payloads.iter().enumerate() {
            let path = inputs.path().join(format!("input-{index}.bin"));
            fs::write(&path, payload).unwrap();
            crate::import(root.path(), &[path]).unwrap();
        }
        (root, inputs)
    }

    /// The digest of the only object in an archive of one payload.
    fn only_digest(index: &ImportEvents) -> String {
        let mut digests = index.digests();
        let digest = digests.next().expect("one digest").0.clone();
        assert!(digests.next().is_none(), "one digest only");
        digest
    }

    #[test]
    fn a_read_that_finds_no_index_builds_one_and_writes_it_owner_only() {
        let (root, _inputs) = archive_with(&[b"one", b"two"]);
        assert!(!index_path(root.path()).exists(), "init writes no index");
        let built = import_events(root.path());
        assert_eq!(built.digests().count(), 2);
        assert!(index_path(root.path()).is_file(), "the read wrote one");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(index_path(root.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the index is owner-only");
        }
        assert_eq!(read_import_events(root.path()), Some(built));
    }

    #[test]
    fn an_index_the_archive_has_moved_past_is_not_used() {
        let (root, inputs) = archive_with(&[b"one"]);
        let warmed = import_events(root.path());
        let stale = fs::read(index_path(root.path())).unwrap();
        let later = inputs.path().join("later.bin");
        fs::write(&later, b"later").unwrap();
        crate::import(root.path(), &[later]).unwrap();

        fs::remove_file(index_path(root.path())).unwrap();
        fs::write(index_path(root.path()), &stale).unwrap();
        paths::narrow_to_owner_only(&index_path(root.path())).unwrap();
        assert_eq!(read_import_events(root.path()), None, "the stamp differs");
        let rebuilt = import_events(root.path());
        assert_eq!(rebuilt.digests().count(), 2, "the rebuild read the records");
        assert_eq!(warmed.digests().count(), 1, "the stale one knew of one");
    }

    #[test]
    fn an_index_that_is_not_a_document_is_an_absent_one() {
        let (root, _inputs) = archive_with(&[b"one"]);
        let built = import_events(root.path());
        fs::remove_file(index_path(root.path())).unwrap();
        fs::write(index_path(root.path()), b"not a document at all").unwrap();
        paths::narrow_to_owner_only(&index_path(root.path())).unwrap();
        assert_eq!(read_import_events(root.path()), None);
        assert_eq!(import_events(root.path()), built, "a rebuild answers again");
    }

    #[test]
    fn an_index_of_another_kind_or_version_is_never_adopted() {
        let (root, _inputs) = archive_with(&[b"one"]);
        import_events(root.path());
        let text = fs::read_to_string(index_path(root.path())).unwrap();
        for changed in [
            text.replace(IMPORT_EVENTS_KIND, "some_other_cache"),
            text.replace("\"cache_schema_version\":2", "\"cache_schema_version\":1"),
        ] {
            fs::remove_file(index_path(root.path())).unwrap();
            fs::write(index_path(root.path()), &changed).unwrap();
            paths::narrow_to_owner_only(&index_path(root.path())).unwrap();
            assert_eq!(read_import_events(root.path()), None);
        }
    }

    #[test]
    fn an_unreadable_import_event_is_refused_exactly_as_a_scan_refuses_it() {
        let (root, _inputs) = archive_with(&[b"one"]);
        let index = import_events(root.path());
        index.refuse_unreadable().unwrap();
        let digest = only_digest(&index);
        assert_eq!(index.of(&digest).len(), 1);
        assert_eq!(index.earliest(&digest), index.of(&digest).first());

        fs::write(
            root.path()
                .join(IMPORTS_DIR)
                .join("0123456789abcdef0123456789abcdef.json"),
            b"not a record",
        )
        .unwrap();
        let refusal = import_events(root.path()).refuse_unreadable().unwrap_err();
        assert_eq!(refusal.code, codes::RECORD_MALFORMED);
    }

    #[test]
    fn the_files_of_the_cache_directory_are_counted_and_never_read() {
        let (root, _inputs) = archive_with(&[b"one"]);
        assert_eq!(file_count(root.path()), 0, "init writes no cache file");
        import_events(root.path());
        assert_eq!(file_count(root.path()), 1);
        fs::remove_dir_all(root.path().join(CACHE_DIR)).unwrap();
        assert_eq!(file_count(root.path()), 0, "an absent cache counts none");
    }

    #[test]
    fn an_event_folded_in_is_the_event_a_scan_would_have_read() {
        let (root, inputs) = archive_with(&[b"one"]);
        let mut folded = import_events(root.path());
        let later = inputs.path().join("again.bin");
        fs::write(&later, b"one").unwrap();
        crate::import(root.path(), &[later]).unwrap();
        for event in document::list_records::<ImportEvent>(root.path()).unwrap() {
            folded.record(&event);
        }
        assert_eq!(folded, scanned_import_events(root.path()));
    }
}
