//! Derived-metadata records: what openPapir computed about stored bytes.
//!
//! One record per stored object, at `records/derived/<digest>.json`, holding
//! the object's byte length, the media type the closed table names for its
//! leading bytes, and the extractor that decided both
//! (`docs/archive-layout.md`). The record is keyed by the artefact digest
//! rather than by a minted identifier, because there is exactly one derived
//! record per object and recomputing it must replace the one that is there
//! rather than leave a second beside it.
//!
//! **A derived record is disposable and never authoritative.** Deleting every
//! one of them and computing them again changes no original and no record the
//! user wrote. Nothing in the archive references one, no command refuses
//! because one is missing, and no conclusion about a receipt, a submission,
//! or an association may be drawn from one. Reading one that cannot be parsed
//! therefore reports no record rather than `record.malformed`: the archive's
//! evidence is untouched by whatever is in that file, and `archive derive`
//! writes it again.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::{limits, paths, write};
use crate::error::{Diagnostic, Warning};
use crate::records::{DERIVED_DIR, DIGEST_PREFIX, is_digest};

/// The value a derived-metadata record carries in `record_kind`.
pub const KIND: &str = "derived_metadata";

/// The extractor that decides a media type from the leading bytes.
pub const EXTRACTOR_NAME: &str = "openpapir.magic-bytes";

/// The extractor's version, which changes when its table or its rules do.
///
/// It is recorded so that a record written by an older extractor can be told
/// from a current one. Nothing recomputes on its own when it changes:
/// recomputation is `archive derive` and nothing else.
pub const EXTRACTOR_VERSION: &str = "1";

/// One derived-metadata record, stored as `records/derived/<digest>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedMetadata {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u32,
    /// The algorithm-qualified digest of the object it describes.
    pub artefact_digest: String,
    /// The object's length in bytes, as the store holds it.
    pub byte_length: u64,
    /// When openPapir computed the record.
    pub computed_at: String,
    /// The extractor that computed it.
    pub extractor_name: String,
    /// The extractor's version.
    pub extractor_version: String,
    /// One value of the closed media-type table.
    pub media_type: String,
    /// The record kind, always `derived_metadata`.
    pub record_kind: String,
}

/// What a derived record says about one artefact a listing already names.
///
/// It is the record without its bookkeeping: the two facts a reader asked
/// for, against the digest they belong to. A listing carries one entry per
/// artefact it names that has a record, and nothing for an artefact that has
/// none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DerivedFacts {
    /// The algorithm-qualified digest of the object described.
    pub artefact_digest: String,
    /// The object's length in bytes.
    pub byte_length: u64,
    /// One value of the closed media-type table.
    pub media_type: String,
}

/// The record's file name, which is the digest and nothing the user typed.
fn file_name(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix(DIGEST_PREFIX).unwrap_or(digest);
    is_digest(hex).then(|| format!("{hex}.json"))
}

/// Write one derived record, replacing the one already stored for its object.
///
/// The caller holds the writer lock. The publish step replaces rather than
/// refuses, because recomputing is the point of the record: a reader sees the
/// whole old document or the whole new one and never a partial file.
///
/// # Errors
///
/// Returns `input.cap.record_size`, `internal.unexpected`, or any refusal of
/// the atomic write procedure: `path.symlink`, `path.cross_device`, or
/// `write.interrupted`.
pub fn write_derived(root: &Path, record: &DerivedMetadata) -> Result<Vec<Warning>, Diagnostic> {
    let content = crate::records::document::json_document(record)?;
    let Some(file_name) = file_name(&record.artefact_digest) else {
        return Err(crate::records::inconsistent(KIND, rules::DIGEST_SHAPE));
    };
    let archive_path = format!("{DERIVED_DIR}/{file_name}");
    write::replace_document(
        &root.join(DERIVED_DIR),
        &file_name,
        &archive_path,
        content.as_bytes(),
        "record_write",
    )
}

/// The consistency rules a derived record can break, as stable `rule` values.
pub mod rules {
    /// The digest the record would be filed under is not a sha256 digest.
    pub const DIGEST_SHAPE: &str = "artefact_digest_shape";
}

/// Read the derived record for one object, or nothing.
///
/// Nothing is the answer for an absent record, an unreadable one, and one
/// that cannot be parsed as this kind: a disposable record that cannot be
/// read holds no facts, and it is not damage to the archive's evidence.
#[must_use]
pub fn read_derived(root: &Path, digest: &str) -> Option<DerivedMetadata> {
    let path = root.join(DERIVED_DIR).join(file_name(digest)?);
    let file = paths::open_no_follow_nonblocking(&path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || limits::check_record_size(metadata.len()).is_err() {
        return None;
    }
    let mut text = String::new();
    file.take(limits::MAX_RECORD_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let record: DerivedMetadata = serde_json::from_str(&text).ok()?;
    let filed_as = file_name(&record.artefact_digest)?;
    (record.record_kind == KIND && Some(filed_as) == file_name(digest)).then_some(record)
}

/// Remove the derived record for one object, if there is one.
///
/// The caller holds the writer lock. A derived record is openPapir's own
/// disposable computation, so unlinking one removes nothing the user wrote
/// and nothing any other record references. Returns whether a file went.
pub fn remove_derived(root: &Path, digest: &str) -> bool {
    file_name(digest)
        .is_some_and(|name| std::fs::remove_file(root.join(DERIVED_DIR).join(name)).is_ok())
}

/// The digests the derived directory holds a record file for.
///
/// The set holds the algorithm-qualified digest each file is filed under, so
/// a caller can tell which stored objects already have one. The pass names
/// and validates: an entry is taken when its name is a digest with the
/// `.json` suffix and the entry is a regular file rather than a link or a
/// directory. Nothing is opened, read, or parsed, so the walk costs one
/// listing of the directory whatever the records inside it hold.
///
/// Not parsing them is the rule rather than an optimisation. A derived record
/// is disposable, so what one contains decides nothing here: every caller of
/// this pass wants to know which objects already have a file, and
/// `archive derive` replaces the file whether it still parses or not.
#[must_use]
pub fn filed(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root.join(DERIVED_DIR)) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(hex) = name.strip_suffix(".json").filter(|hex| is_digest(hex)) else {
            continue;
        };
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            found.insert(format!("{DIGEST_PREFIX}{hex}"));
        }
    }
    found
}

/// How many derived records the archive holds.
///
/// The count is what `archive check` reports, and it is [`filed`]'s answer:
/// how many files the derived directory holds under a digest name. A missing
/// record is nothing at all rather than a problem, because no operation needs
/// one.
#[must_use]
pub fn count(root: &Path) -> u64 {
    filed(root).len() as u64
}

/// The facts every named artefact has a derived record for, by digest.
///
/// The digests are the ones a listing already carries, so nothing new is
/// disclosed. Each is looked up once, whatever number of records name it, and
/// an artefact with no derived record contributes no entry.
#[must_use]
pub fn facts_for<'a, I: IntoIterator<Item = &'a str>>(
    root: &Path,
    digests: I,
) -> Vec<DerivedFacts> {
    let mut wanted: Vec<&str> = digests.into_iter().collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted
        .into_iter()
        .filter_map(|digest| {
            read_derived(root, digest).map(|record| DerivedFacts {
                artefact_digest: record.artefact_digest,
                byte_length: record.byte_length,
                media_type: record.media_type,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
