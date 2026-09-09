//! The object pass of the integrity check: what the store actually holds.
//!
//! Every object is opened read-only with the platform's no-follow flag and
//! streamed through SHA-256 in bounded chunks, so the check never allocates a
//! whole object and never follows a link out of the archive. Nothing is
//! written, renamed, or repaired: the pass only reads and counts.
//!
//! A re-computed digest is a storage-layer identity. A matching one says the
//! stored bytes are the bytes the path names, and nothing about authenticity,
//! origin, delivery, or legal effect.

use std::collections::BTreeSet;
use std::fs::{self, DirEntry};
use std::io::{self, Read as _};
use std::path::Path;

use sha2::{Digest as _, Sha256};

use crate::archive::objects::{ALGORITHM, INCOMING_DIR, OBJECTS_DIR};
use crate::archive::{limits, paths};
use crate::ident;
use crate::integrity::Counts;
use crate::integrity::references::{DigestKey, References, object_key};

/// How many bytes are read from an object at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// What the object pass observed, in counters only.
#[derive(Debug, Default)]
pub struct Store {
    /// How many object entries were examined.
    pub objects_checked: u64,
    /// How many bytes were streamed through the digest.
    pub bytes_digested: u64,
    /// How many entries the check could not read: an object over the
    /// per-file cap, one whose metadata or bytes could not be read, and each
    /// fan-out directory that could not be listed. None of them is asserted
    /// to be damaged, because the check did not read them to say so.
    pub objects_unchecked: u64,
    /// How many leftover staging files sit in the incoming directory.
    pub staging_files: u64,
    /// The digests the store actually holds, as fixed-size keys.
    pub present: BTreeSet<DigestKey>,
    /// Whether the store itself could not be listed at all.
    unlistable: bool,
    /// The first fan-out byte of each first-level directory that could not be
    /// listed, so that a digest under one is never judged absent.
    unlistable_high: BTreeSet<u8>,
    /// The two fan-out bytes of each second-level directory that could not be
    /// listed, for the same reason.
    unlistable_pair: BTreeSet<[u8; 2]>,
}

impl Store {
    /// Whether the store holds a digest: `Some(true)` when the object is
    /// there, `Some(false)` when the directory it would live in was read and
    /// does not hold it, and `None` when that directory could not be listed.
    ///
    /// The third answer matters: an object the check could not look for is
    /// not an object the archive does not have, so a reference to it is left
    /// uncounted rather than reported as dangling.
    #[must_use]
    pub fn holds(&self, key: &DigestKey) -> Option<bool> {
        if self.unlistable
            || self.unlistable_high.contains(&key[0])
            || self.unlistable_pair.contains(&[key[0], key[1]])
        {
            return None;
        }
        Some(self.present.contains(key))
    }

    /// Record that a directory naming these fan-out bytes could not be read.
    ///
    /// An empty prefix means the check could not tell which digests the
    /// unread directory covers, so none of them may be judged absent.
    fn unlistable(&mut self, prefix: &[u8]) {
        self.objects_unchecked += 1;
        match prefix {
            [high] => {
                self.unlistable_high.insert(*high);
            }
            [high, low] => {
                self.unlistable_pair.insert([*high, *low]);
            }
            _ => self.unlistable = true,
        }
    }
}

/// The byte a two-character fan-out directory name stands for.
fn fan_out_byte(name: &str) -> Option<u8> {
    u8::from_str_radix(name, 16)
        .ok()
        .filter(|_| name.len() == 2)
}

/// Walk `objects/` and count what disagrees with the records.
///
/// The digest of an object is derived from its own path, so a name that is
/// not a digest, or one filed under the wrong fan-out directories, is a
/// malformed object entry and counts as `integrity.digest_mismatch`. A
/// symbolic link or any other non-regular file inside `objects/` is
/// `path.symlink` and is never opened.
pub fn walk(root: &Path, references: &References, counts: &mut Counts) -> Store {
    let mut store = Store {
        staging_files: count_staging(&root.join(INCOMING_DIR)),
        ..Store::default()
    };
    let base = root.join(OBJECTS_DIR).join(ALGORITHM);
    let first_level = match fs::read_dir(&base) {
        Ok(entries) => entries,
        // Only a store that is not there holds nothing. Any other failure
        // means the store was not read, so no digest under it may be judged
        // absent. The distinction is taken from the error itself rather than
        // from a second look at the path, because a directory the process
        // cannot search reports both as missing.
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                store.unlistable(&[]);
            }
            return store;
        }
    };
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    for entry in first_level.flatten() {
        let Some(high) = fan_out(&entry, counts, &mut store) else {
            continue;
        };
        let Some(high_byte) = fan_out_byte(&high) else {
            store.unlistable(&[]);
            continue;
        };
        let Ok(second_level) = fs::read_dir(entry.path()) else {
            store.unlistable(&[high_byte]);
            continue;
        };
        for entry in second_level.flatten() {
            let Some(low) = fan_out(&entry, counts, &mut store) else {
                continue;
            };
            let Some(low_byte) = fan_out_byte(&low) else {
                store.unlistable(&[]);
                continue;
            };
            let Ok(objects) = fs::read_dir(entry.path()) else {
                store.unlistable(&[high_byte, low_byte]);
                continue;
            };
            for object in objects.flatten() {
                check_object(
                    &object,
                    &high,
                    &low,
                    references,
                    counts,
                    &mut store,
                    &mut buffer,
                );
            }
        }
    }
    store
}

/// How many entries the staging directory still holds.
///
/// A leftover staging file is openPapir's own transient artefact, never a
/// record and never an object. It is counted and left exactly where it is:
/// the check deletes nothing.
fn count_staging(directory: &Path) -> u64 {
    fs::read_dir(directory).map_or(0, |entries| entries.flatten().count() as u64)
}

/// The name of a fan-out directory, or a count against the entry that is not
/// one: a link is `path.symlink`, anything else a malformed object entry.
fn fan_out(entry: &DirEntry, counts: &mut Counts, store: &mut Store) -> Option<String> {
    let name = entry.file_name().to_string_lossy().into_owned();
    let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
        // The entry could not be read, so nothing may be asserted about it.
        store.objects_unchecked += 1;
        return None;
    };
    if metadata.file_type().is_symlink() {
        counts.symlink += 1;
        return None;
    }
    let shaped = name.len() == 2
        && name
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character));
    if !metadata.is_dir() || !shaped {
        counts.digest_mismatch += 1;
        return None;
    }
    Some(name)
}

/// Check one stored object against its own path and against the records.
fn check_object(
    entry: &DirEntry,
    high: &str,
    low: &str,
    references: &References,
    counts: &mut Counts,
    store: &mut Store,
    buffer: &mut [u8],
) {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        // The entry could not be read, so nothing may be asserted about it.
        store.objects_unchecked += 1;
        return;
    };
    if !metadata.is_file() {
        // A link or any other non-regular file inside the store is refused
        // rather than followed, exactly as every other reader refuses one.
        counts.symlink += 1;
        return;
    }
    let Some(key) =
        object_key(&name).filter(|_| name.starts_with(high) && name[2..].starts_with(low))
    else {
        counts.digest_mismatch += 1;
        return;
    };
    store.objects_checked += 1;
    store.present.insert(key);
    if !references.referenced.contains(&key) {
        counts.orphan += 1;
    }
    let Some((digest, byte_length)) = digest_of(&path, metadata.len(), buffer, store) else {
        store.objects_unchecked += 1;
        return;
    };
    if digest != name {
        counts.digest_mismatch += 1;
    }
    if references
        .lengths
        .get(&key)
        .is_some_and(|lengths| !lengths.contains(&byte_length))
    {
        counts.length_mismatch += 1;
    }
}

/// Stream one object through SHA-256 in bounded chunks, honouring the cap.
///
/// The object is opened read-only and never modified. An object larger than
/// the per-file cap is not read at all, and one the cap catches mid-stream is
/// abandoned, because the check may not allocate more than an import may.
fn digest_of(
    path: &Path,
    reported_bytes: u64,
    buffer: &mut [u8],
    store: &mut Store,
) -> Option<(String, u64)> {
    if limits::check_file_size(reported_bytes, 0).is_err() {
        return None;
    }
    let mut file = paths::open_no_follow(path).ok()?;
    let mut hasher = Sha256::new();
    let mut byte_length = 0_u64;
    loop {
        let read = file.read(buffer).ok()?;
        if read == 0 {
            break;
        }
        byte_length += read as u64;
        if limits::check_file_size(byte_length, 0).is_err() {
            return None;
        }
        store.bytes_digested += read as u64;
        hasher.update(&buffer[..read]);
    }
    Some((ident::hex(&hasher.finalize()), byte_length))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    /// A store holding one entry at `objects/sha256/<high>/<low>/<name>`.
    fn store_with(high: &str, low: &str, name: &str, bytes: &[u8]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("objects/sha256").join(high).join(low);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(name), bytes).unwrap();
        root
    }

    #[test]
    fn an_entry_that_is_not_a_digest_in_its_own_fan_out_is_malformed() {
        for (high, low, name) in [
            ("a0", "02", "notes.txt"),
            ("a0", "03", DIGEST),
            ("zz", "02", DIGEST),
        ] {
            let root = store_with(high, low, name, b"synthetic bytes\n");
            let mut counts = Counts::default();
            let store = walk(root.path(), &References::default(), &mut counts);
            assert_eq!(counts.digest_mismatch, 1, "{high}/{low}/{name}");
            assert_eq!(store.objects_checked, 0);
            assert_eq!(counts.symlink, 0);
        }
    }

    #[test]
    fn an_object_the_cap_forbids_reading_is_counted_rather_than_digested() {
        let root = store_with("a0", "02", DIGEST, b"");
        let path = root.path().join("objects/sha256/a0/02").join(DIGEST);
        fs::File::create(&path)
            .unwrap()
            .set_len(limits::MAX_FILE_BYTES + 1)
            .unwrap();
        let mut counts = Counts::default();
        let store = walk(root.path(), &References::default(), &mut counts);
        assert_eq!(store.objects_checked, 1);
        assert_eq!(store.objects_unchecked, 1, "the cap bounds the check too");
        assert_eq!(store.bytes_digested, 0);
        assert_eq!(counts.digest_mismatch, 0, "an unread object is not damaged");
        assert_eq!(counts.orphan, 1, "no record references it");
    }

    #[test]
    fn a_leftover_staging_file_is_counted_and_left_alone() {
        let root = store_with("a0", "02", DIGEST, b"synthetic bytes\n");
        let incoming = root.path().join(INCOMING_DIR);
        fs::create_dir_all(&incoming).unwrap();
        let staging = incoming.join(".papir-staging-abc");
        fs::write(&staging, b"partial").unwrap();
        let mut counts = Counts::default();
        let store = walk(root.path(), &References::default(), &mut counts);
        assert_eq!(store.staging_files, 1);
        assert!(staging.exists(), "the check deletes nothing");
        assert_eq!(store.objects_checked, 1);
        assert_eq!(store.bytes_digested, 16);
        assert_eq!(counts.digest_mismatch, 0, "the stored bytes match the path");
    }

    #[test]
    fn an_archive_with_no_object_directory_reports_nothing() {
        let root = tempfile::tempdir().unwrap();
        let mut counts = Counts::default();
        let store = walk(root.path(), &References::default(), &mut counts);
        assert_eq!(store.objects_checked, 0);
        assert_eq!(store.staging_files, 0);
        assert_eq!(store.bytes_digested, 0);
        assert!(store.present.is_empty());
    }
}
