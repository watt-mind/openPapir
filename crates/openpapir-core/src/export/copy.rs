//! Copying stored objects out of the archive, byte for byte.
//!
//! Each object is opened read-only with the platform's no-follow flag and
//! streamed in bounded chunks into a staging file inside the destination,
//! digesting the bytes on the way out. Nothing is memory-mapped, nothing is
//! held whole, no hard link is made, and the copy is therefore valid across
//! filesystems: it is a plain copy of the original bytes
//! (`docs/archive-layout.md`).
//!
//! The copy is compared with the digest its source path names before it is
//! published. A copy that differs is `export.copy_mismatch` and its partial
//! file is removed, so the destination never holds a file whose name does not
//! describe its content. The comparison is a storage-layer identity check and
//! never a cryptographic verification.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::Path;

use sha2::{Digest as _, Sha256};

use serde::Serialize;

use crate::archive::write::Staging;
use crate::archive::{limits, objects, paths};
use crate::error::{Details, Diagnostic, Warning, codes};
use crate::export::destination::{self, Destination};
use crate::ident;
use crate::records::document;

/// How many bytes are read from an object at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// The stage name every object copy reports a degradation under.
const STAGE: &str = "object_write";

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
    warnings: &mut Vec<Warning>,
) -> Result<Vec<ObjectEntry>, Diagnostic> {
    if digests.is_empty() {
        return Ok(Vec::new());
    }
    let directory = destination.objects_directory()?;
    let mut copied = Vec::with_capacity(digests.len());
    for (index, digest) in digests.iter().enumerate() {
        copied.push(copy_one(root, &directory, digest, index as u64, warnings)?);
    }
    Ok(copied)
}

/// Copy one object and re-digest it before it is published.
fn copy_one(
    root: &Path,
    directory: &Path,
    digest: &str,
    index: u64,
    warnings: &mut Vec<Warning>,
) -> Result<ObjectEntry, Diagnostic> {
    let relative = format!("{}/{digest}", destination::OBJECTS_DIR);
    destination::refuse_existing(&directory.join(digest), &relative)?;
    let mut source = open_object(root, digest)?;
    let mut staging = Staging::create(directory, STAGE)?;
    let (copied, byte_length) = stream(&mut source, &mut staging, index)?;
    if copied != digest {
        return Err(mismatch(digest));
    }
    staging.finish(STAGE)?;
    let target = directory.join(digest);
    warnings.extend(destination::publish(staging, &target, &relative, STAGE)?);
    // The copy becomes read-only only once it is in place. Narrowing the
    // staging file first would leave it behind on a platform that refuses to
    // remove a read-only file, and an export must hold nothing its manifest
    // does not list.
    paths::set_object_read_only(&target)
        .map_err(|error| destination::refusal(&error, &relative))?;
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

/// Stream the source into the staging file, digesting and counting as it goes.
///
/// The single-file cap is enforced while streaming, exactly as import
/// enforces it, so a stored object that has grown past the cap stops the
/// export rather than being copied unbounded.
fn stream(
    source: &mut File,
    staging: &mut Staging,
    index: u64,
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
        staging
            .file()
            .write_all(&buffer[..read])
            .map_err(|error| destination::refusal(&error, ""))?;
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
        let mut warnings = Vec::new();
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let copied = copy_objects(&root, &prepared, &digests, &mut warnings).unwrap();
        assert_eq!(copied.len(), 1);
        assert_eq!(copied[0].byte_length, PAYLOAD.len() as u64);
        assert_eq!(copied[0].algorithm, "sha256");
        let copy = prepared.path().join("objects").join(DIGEST);
        assert_eq!(fs::read(&copy).unwrap(), PAYLOAD);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.bucket() == crate::error::Bucket::Platform)
        );
    }

    #[test]
    fn nothing_is_created_for_a_case_that_references_no_object() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        let prepared = destination(home.path(), &root);
        let mut warnings = Vec::new();
        assert!(
            copy_objects(&root, &prepared, &BTreeSet::new(), &mut warnings)
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
        let mut warnings = Vec::new();
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let refusal = copy_objects(&root, &prepared, &digests, &mut warnings).unwrap_err();
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
        let mut warnings = Vec::new();
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let refusal = copy_objects(&root, &prepared, &digests, &mut warnings).unwrap_err();
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
        let mut warnings = Vec::new();
        let digests = BTreeSet::from([DIGEST.to_owned()]);
        let refusal = copy_objects(&root, &prepared, &digests, &mut warnings).unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
    }
}
