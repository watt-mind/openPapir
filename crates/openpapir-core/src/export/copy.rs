//! Copying stored objects out of the archive, byte for byte.
//!
//! Each object is opened read-only with the platform's no-follow flag and
//! streamed in bounded chunks into a file created at its final path in the
//! destination, digesting the bytes on the way out. Nothing is memory-mapped,
//! nothing is held whole, and no hard link is made, so the copy is a plain
//! copy of the original bytes and works on any filesystem
//! (`docs/archive-layout.md`).
//!
//! The copy is compared with the digest its source path names once it is
//! written. A copy that differs is `export.copy_mismatch` and the partial
//! file is removed at once, so the destination never keeps a file whose name
//! does not describe its content. The comparison is a storage-layer identity
//! check and never a cryptographic verification.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::Path;

use sha2::{Digest as _, Sha256};

use serde::Serialize;

use crate::archive::{limits, objects, paths};
use crate::error::{Details, Diagnostic, codes};
use crate::export::destination::{self, Destination};
use crate::ident;
use crate::records::document;

/// How many bytes are read from an object at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// The stage name an interrupted object copy reports.
///
/// Every failure on the way out of the archive and into the destination is
/// one of these: reading the stored object, streaming it, flushing it, and
/// making the finished copy read-only are all part of copying that object
/// (`docs/error-contract.md`).
const STAGE: &str = destination::OBJECT_WRITE;

/// One copied object, as the manifest lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectEntry {
    /// The digest algorithm, which names the store's first level.
    pub algorithm: String,
    /// The number of bytes copied.
    pub byte_length: u64,
    /// The lowercase hexadecimal digest, which is also the copy's file name.
    pub digest: String,
}

/// Copy every referenced object into the destination, in digest order.
///
/// # Errors
///
/// Returns `record.not_found` when the archive holds no such object,
/// `path.symlink` for a link in the store or the destination,
/// `export.destination_conflict` for a target path already there,
/// `export.copy_mismatch` when a copy re-digests to something else,
/// `input.cap.file_size` when a stored object exceeds the single-file cap,
/// and `write.interrupted` when a copy cannot be completed.
pub fn copy_objects(
    root: &Path,
    destination: &Destination,
    digests: &BTreeSet<String>,
) -> Result<Vec<ObjectEntry>, Diagnostic> {
    if digests.is_empty() {
        return Ok(Vec::new());
    }
    let directory = destination.objects_directory()?;
    let mut copied = Vec::with_capacity(digests.len());
    for (index, digest) in digests.iter().enumerate() {
        copied.push(copy_one(
            root,
            destination,
            &directory,
            digest,
            index as u64,
        )?);
    }
    Ok(copied)
}

/// Copy one object and re-digest it before the copy is kept.
fn copy_one(
    root: &Path,
    destination: &Destination,
    directory: &Path,
    digest: &str,
    index: u64,
) -> Result<ObjectEntry, Diagnostic> {
    let relative = format!("{}/{digest}", destination::OBJECTS_DIR);
    let mut source = open_object(root, digest)?;
    let target = directory.join(digest);
    let mut copy = destination.create(directory, digest, &relative, STAGE)?;
    let streamed =
        stream(&mut source, &mut copy, index, &relative).and_then(|(copied, byte_length)| {
            if copied == digest {
                Ok(byte_length)
            } else {
                Err(mismatch(digest))
            }
        });
    let byte_length = match streamed {
        Ok(byte_length) => byte_length,
        Err(error) => {
            // The partial copy goes at once rather than waiting for the
            // export to unwind, so nothing in the destination ever holds
            // bytes its name does not describe.
            drop(copy);
            destination.remove_created(&target);
            return Err(error);
        }
    };
    copy.sync_all()
        .map_err(|error| destination::refusal(&error, &relative, STAGE))?;
    drop(copy);
    // The copy becomes read-only only once it is complete, so that a failure
    // can still remove it on a platform that honours a read-only attribute.
    paths::set_object_read_only(&target)
        .map_err(|error| destination::refusal(&error, &relative, STAGE))?;
    Ok(ObjectEntry {
        algorithm: objects::ALGORITHM.to_owned(),
        byte_length,
        digest: digest.to_owned(),
    })
}

/// Open a stored object read-only, refusing a link and a missing object.
fn open_object(root: &Path, digest: &str) -> Result<File, Diagnostic> {
    let path = objects::absolute_path(root, digest);
    if paths::is_symlink(&path) {
        return Err(paths::symlink_refusal(
            Details::new()
                .text("scope", "archive")
                .text("archive_path", objects::archive_path(digest)),
        ));
    }
    paths::open_no_follow(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            document::not_found("artefact", "artefact_digest")
        } else {
            read_refusal()
        }
    })
}

/// Stream the source into the copy, digesting and counting as it goes.
///
/// The single-file cap is enforced while streaming, exactly as import
/// enforces it, so a stored object that has grown past the cap stops the
/// export rather than being copied unbounded.
fn stream(
    source: &mut File,
    copy: &mut File,
    index: u64,
    relative: &str,
) -> Result<(String, u64), Diagnostic> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    let mut byte_length = 0_u64;
    loop {
        let read = source.read(&mut buffer).map_err(|_| read_refusal())?;
        if read == 0 {
            break;
        }
        byte_length += read as u64;
        limits::check_file_size(byte_length, index)?;
        hasher.update(&buffer[..read]);
        copy.write_all(&buffer[..read])
            .map_err(|error| destination::refusal(&error, relative, STAGE))?;
    }
    Ok((ident::hex(&hasher.finalize()), byte_length))
}

/// The refusal for a stored object whose bytes could not be read.
fn read_refusal() -> Diagnostic {
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "A stored object could not be read for the export.",
        Details::new().text("stage", STAGE),
    )
    .retryable()
}

/// The refusal for a copy that does not hold the bytes its name describes.
fn mismatch(digest: &str) -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_COPY_MISMATCH,
        "An exported copy re-digested to something other than the stored object.",
        Details::new()
            .text("digest", digest.to_owned())
            .int("conflict_count", 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PAYLOAD: &[u8] = b"synthetic bytes\n";
    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    /// An archive root holding one object, without going through import.
    fn store(root: &Path, digest: &str, content: &[u8]) {
        let path = objects::absolute_path(root, digest);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }

    fn destination(home: &Path, root: &Path) -> Destination {
        destination::prepare(root, &home.join("export")).unwrap()
    }

    #[test]
    fn a_copy_is_byte_identical_and_named_by_its_digest() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        store(&root, DIGEST, PAYLOAD);
        let prepared = destination(home.path(), &root);
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let copied = copy_objects(&root, &prepared, &digests).unwrap();
        assert_eq!(copied.len(), 1);
        assert_eq!(copied[0].byte_length, PAYLOAD.len() as u64);
        assert_eq!(copied[0].algorithm, "sha256");
        let copy = prepared.path().join("objects").join(DIGEST);
        assert_eq!(fs::read(&copy).unwrap(), PAYLOAD);
    }

    #[test]
    fn nothing_is_created_for_a_case_that_references_no_object() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        let prepared = destination(home.path(), &root);
        assert!(
            copy_objects(&root, &prepared, &BTreeSet::new())
                .unwrap()
                .is_empty()
        );
        assert!(!prepared.path().join("objects").exists());
    }

    #[test]
    fn a_source_whose_bytes_do_not_match_its_path_is_a_copy_mismatch() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        store(&root, DIGEST, b"other bytes\n");
        let prepared = destination(home.path(), &root);
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let refusal = copy_objects(&root, &prepared, &digests).unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_COPY_MISMATCH);
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["digest"], DIGEST);
        assert_eq!(json["details"]["conflict_count"], 1);
        assert!(
            !prepared.path().join("objects").join(DIGEST).exists(),
            "the partial copy is removed"
        );
        let remaining: Vec<_> = fs::read_dir(prepared.path().join("objects"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(remaining.is_empty(), "no staging file is left behind");
    }

    #[test]
    fn an_object_the_archive_does_not_hold_is_not_found() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        let prepared = destination(home.path(), &root);
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let refusal = copy_objects(&root, &prepared, &digests).unwrap_err();
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_object_is_refused_rather_than_followed() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        let elsewhere = home.path().join("elsewhere");
        fs::write(&elsewhere, PAYLOAD).unwrap();
        let path = objects::absolute_path(&root, DIGEST);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        let prepared = destination(home.path(), &root);
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let refusal = copy_objects(&root, &prepared, &digests).unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
    }
}
