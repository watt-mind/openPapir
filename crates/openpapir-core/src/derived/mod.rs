//! Derived metadata on explicit request: `archive derive`.
//!
//! The operation walks the artefact store, computes one derived-metadata
//! record per stored object, and writes it under the writer lock
//! (`docs/archive-layout.md`). It is the only thing that writes, replaces, or
//! removes a derived record: nothing recomputes on its own, no other command
//! needs one, and an archive that has never been derived is a complete
//! archive.
//!
//! Each object is opened once, without following a link. Its length comes
//! from that handle, and at most [`media::SNIFF_BYTES`] are read from it to
//! decide the media type, so the cost of the operation grows with the number
//! of objects and never with their size. An object the single-file cap
//! forbids reading is counted and left without a record, exactly as the
//! integrity check leaves it undigested.
//!
//! A media type is a statement about leading bytes. It is not a verification,
//! not evidence, and never a reason to treat an artefact as a receipt: no
//! caller may infer anything about a receipt, a submission, or an association
//! from one.

pub mod media;

use std::fs::{self, DirEntry};
use std::io::{ErrorKind, Read as _};
use std::path::Path;

use serde::Serialize;

use crate::archive::lock::WriterLock;
use crate::archive::objects::{ALGORITHM, OBJECTS_DIR};
use crate::archive::{Archive, DERIVED_DIR, SUPPORTED_SCHEMA_VERSION, limits, paths};
use crate::clock;
use crate::error::{Diagnostic, Failure, Outcome, Result, Warning};
use crate::records::DIGEST_PREFIX;
use crate::records::derived::{
    DerivedMetadata, EXTRACTOR_NAME, EXTRACTOR_VERSION, KIND, filed, remove_derived, write_derived,
};
use crate::records::is_digest;

/// How many objects of one media type the store holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MediaCount {
    /// How many derived records name this media type.
    pub count: u64,
    /// One value of the closed table, including the ones that were not seen.
    pub media_type: &'static str,
}

/// What one `archive derive` did, in counts only.
///
/// The struct is `#[non_exhaustive]`: the operation gains figures as it
/// learns to look at more of an archive, so a caller outside this crate
/// matches on the fields it knows and never builds one by literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Derived {
    /// How many stored objects were examined.
    pub objects_checked: u64,
    /// How many objects were not read, because one exceeds the per-file cap,
    /// is not a regular file, or could not be opened. None of them is
    /// asserted to be damaged, and none has a record written for it.
    pub objects_unchecked: u64,
    /// One entry per value of the closed media-type table, ordered by value,
    /// including the values this archive holds none of.
    pub media_types: Vec<MediaCount>,
    /// How many derived records were written, which is one per object read
    /// whether or not a record for it was already there.
    pub records_written: u64,
    /// How many derived records were removed because the archive no longer
    /// holds the object they describe. A derived record is disposable, so a
    /// stale one is discarded rather than reported.
    pub records_removed: u64,
    /// How many bytes were read to decide the media types, which is at most
    /// [`media::SNIFF_BYTES`] per object read.
    pub bytes_sniffed: u64,
}

/// Compute the derived-metadata record of every stored object.
///
/// The writer lock is held for the whole walk, because the operation writes.
/// Every object present gets a record whether or not it had one, and a record
/// naming an object the archive no longer holds is removed: a derived record
/// describes the store as it is now, and it is never evidence of what the
/// store held before.
///
/// # Errors
///
/// Returns the refusals of opening an archive for writing,
/// `lock.held` when another writer holds the lock, and any refusal of the
/// atomic write procedure: `path.symlink`, `path.cross_device`,
/// `write.interrupted`, or `input.cap.record_size`.
pub fn derive(root: &Path) -> Result<Derived> {
    let mut warnings = Vec::new();
    match run(root, &mut warnings) {
        Ok(derived) => Ok(Outcome {
            data: derived,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn run(root: &Path, warnings: &mut Vec<Warning>) -> std::result::Result<Derived, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let root = archive.root();
    let mut counts = Counts::default();
    let mut existing = filed(root);
    let computed_at = clock::now_rfc3339();
    let mut buffer = vec![0_u8; media::SNIFF_BYTES];
    for digest in present_objects(root, &mut counts) {
        let Some(record) = describe(root, &digest, &computed_at, &mut buffer, &mut counts) else {
            existing.remove(&digest);
            continue;
        };
        warnings.extend(write_derived(root, &record)?);
        counts.records_written += 1;
        counts.seen(record.media_type.as_str());
        existing.remove(&digest);
    }
    for stale in &existing {
        if remove_derived(root, stale) {
            counts.records_removed += 1;
        }
    }
    if let Some(warning) = paths::sync_directory(&root.join(DERIVED_DIR), "record_write") {
        warnings.push(warning);
    }
    Ok(counts.finish())
}

/// The counters the walk fills, before they become a report.
#[derive(Debug, Default)]
struct Counts {
    objects_checked: u64,
    objects_unchecked: u64,
    records_written: u64,
    records_removed: u64,
    bytes_sniffed: u64,
    by_media_type: Vec<(&'static str, u64)>,
}

impl Counts {
    /// Count one object of this media type.
    fn seen(&mut self, media_type: &str) {
        let named = media::MEDIA_TYPES
            .iter()
            .find(|known| **known == media_type)
            .copied()
            .unwrap_or(media::UNKNOWN);
        match self.by_media_type.iter_mut().find(|(key, _)| *key == named) {
            Some((_, count)) => *count += 1,
            None => self.by_media_type.push((named, 1)),
        }
    }

    /// The report, with one entry per value of the closed table.
    fn finish(self) -> Derived {
        let media_types = media::MEDIA_TYPES
            .iter()
            .map(|media_type| MediaCount {
                count: self
                    .by_media_type
                    .iter()
                    .find(|(key, _)| key == media_type)
                    .map_or(0, |(_, count)| *count),
                media_type,
            })
            .collect();
        Derived {
            objects_checked: self.objects_checked,
            objects_unchecked: self.objects_unchecked,
            media_types,
            records_written: self.records_written,
            records_removed: self.records_removed,
            bytes_sniffed: self.bytes_sniffed,
        }
    }
}

/// Every digest the store holds, as `sha256:<hex>`, in fan-out order.
///
/// An entry whose name is not a digest filed under its own fan-out
/// directories is passed over rather than judged: saying what is wrong with
/// the store is `archive check`'s work, and this operation only describes the
/// objects it can name.
fn present_objects(root: &Path, counts: &mut Counts) -> Vec<String> {
    let mut digests = Vec::new();
    let base = root.join(OBJECTS_DIR).join(ALGORITHM);
    let Ok(first_level) = fs::read_dir(&base) else {
        return digests;
    };
    for high in first_level.flatten().filter(is_fan_out) {
        let Ok(second_level) = fs::read_dir(high.path()) else {
            counts.objects_unchecked += 1;
            continue;
        };
        for low in second_level.flatten().filter(is_fan_out) {
            let Ok(objects) = fs::read_dir(low.path()) else {
                counts.objects_unchecked += 1;
                continue;
            };
            let (high, low) = (name_of(&high), name_of(&low));
            for object in objects.flatten() {
                let name = name_of(&object);
                if is_digest(&name) && name.starts_with(&high) && name[2..].starts_with(&low) {
                    digests.push(format!("{DIGEST_PREFIX}{name}"));
                }
            }
        }
    }
    digests.sort();
    digests
}

/// One directory entry's name.
fn name_of(entry: &DirEntry) -> String {
    entry.file_name().to_string_lossy().into_owned()
}

/// Whether an entry is a fan-out directory: two lowercase hexadecimal
/// characters, and a directory rather than a link.
fn is_fan_out(entry: &DirEntry) -> bool {
    let name = name_of(entry);
    let shaped = name.len() == 2
        && name
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character));
    shaped
        && fs::symlink_metadata(entry.path())
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

/// The record one stored object gets, or nothing when it was not read.
///
/// The object is opened once, with the platform's no-follow flag. Its length
/// is taken from that handle rather than from a second look at the path, so
/// the file that is measured is the file that is sniffed, and at most
/// [`media::SNIFF_BYTES`] are read from it.
fn describe(
    root: &Path,
    digest: &str,
    computed_at: &str,
    buffer: &mut [u8],
    counts: &mut Counts,
) -> Option<DerivedMetadata> {
    let hex = digest.strip_prefix(DIGEST_PREFIX).unwrap_or(digest);
    let path = crate::archive::objects::absolute_path(root, hex);
    counts.objects_checked += 1;
    let Ok(mut file) = paths::open_no_follow_nonblocking(&path) else {
        counts.objects_unchecked += 1;
        return None;
    };
    let Ok(metadata) = file.metadata() else {
        counts.objects_unchecked += 1;
        return None;
    };
    if !metadata.is_file() || limits::check_file_size(metadata.len(), 0).is_err() {
        counts.objects_unchecked += 1;
        return None;
    }
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            // A read a signal interrupted read nothing and says nothing
            // about the object, so it is retried rather than counted: an
            // object left unchecked here would silently lose its record.
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => {
                counts.objects_unchecked += 1;
                return None;
            }
        }
    }
    counts.bytes_sniffed += filled as u64;
    Some(DerivedMetadata {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        artefact_digest: digest.to_owned(),
        byte_length: metadata.len(),
        computed_at: computed_at.to_owned(),
        extractor_name: EXTRACTOR_NAME.to_owned(),
        extractor_version: EXTRACTOR_VERSION.to_owned(),
        media_type: media::sniff(&buffer[..filled]).to_owned(),
        record_kind: KIND.to_owned(),
    })
}

#[cfg(test)]
mod tests;
