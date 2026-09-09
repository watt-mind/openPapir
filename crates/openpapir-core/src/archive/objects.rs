//! The content-addressed, write-once artefact store.
//!
//! An object's path is derived from its SHA-256 digest as
//! `objects/sha256/<first two>/<next two>/<full digest>`, which bounds
//! directory fan-out and leaves room for a second algorithm later. Objects are
//! write-once: once an object exists it is never modified, truncated, or
//! replaced, and its permissions become owner-read-only.
//!
//! The digest is a storage-layer identity only. It says two files have the
//! same bytes. It says nothing about authenticity, origin, or whether the file
//! is a receipt at all, and no output may present it as verification.

use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::archive::write::Staging;
use crate::archive::{limits, paths};
use crate::error::{Details, Diagnostic, Warning, codes};
use crate::ident;

/// The directory holding stored objects, relative to the archive root.
pub const OBJECTS_DIR: &str = "objects";
/// The staging directory for object writes, relative to the archive root.
pub const INCOMING_DIR: &str = "objects/incoming";
/// The digest algorithm that names the store's first level.
pub const ALGORITHM: &str = "sha256";

/// How many bytes are read from an input at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// What storing one input's bytes produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The lowercase hexadecimal SHA-256 digest of the bytes.
    pub digest: String,
    /// The number of bytes read, enforced against the caps while streaming.
    pub byte_length: u64,
    /// Whether this call created the object or found it already present.
    pub created_object: bool,
    /// Degradations observed while storing.
    pub warnings: Vec<Warning>,
}

/// The object's path relative to the archive root.
#[must_use]
pub fn archive_path(digest: &str) -> String {
    format!(
        "{OBJECTS_DIR}/{ALGORITHM}/{}/{}/{digest}",
        &digest[0..2],
        &digest[2..4]
    )
}

/// The object's absolute path inside `root`.
#[must_use]
pub fn absolute_path(root: &Path, digest: &str) -> PathBuf {
    root.join(OBJECTS_DIR)
        .join(ALGORITHM)
        .join(&digest[0..2])
        .join(&digest[2..4])
        .join(digest)
}

/// Stream one input into the store, enforcing the caps while reading.
///
/// `read_total` carries the import's running byte count, so the per-operation
/// cap is enforced again while streaming and not only from the sizes the
/// filesystem reported.
///
/// # Errors
///
/// Returns a cap refusal, a path-safety refusal, `integrity.length_mismatch`,
/// or `write.interrupted`.
pub fn store(
    root: &Path,
    source: &mut File,
    input_index: u64,
    read_total: &mut u64,
) -> Result<Stored, Diagnostic> {
    let mut staging = Staging::create(&root.join(INCOMING_DIR), "object")?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    let mut byte_length = 0_u64;
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| paths::publish_refusal(&error, "", "object"))?;
        if read == 0 {
            break;
        }
        byte_length += read as u64;
        *read_total += read as u64;
        limits::check_file_size(byte_length, input_index)?;
        limits::check_import_bytes(*read_total, input_index)?;
        hasher.update(&buffer[..read]);
        staging
            .file()
            .write_all(&buffer[..read])
            .map_err(|error| paths::publish_refusal(&error, "", "object"))?;
    }
    staging.finish("object")?;
    let digest = ident::hex(&hasher.finalize());
    place(root, staging, &digest, byte_length)
}

/// Put a staged object at its digest-derived path, or find it already there.
fn place(
    root: &Path,
    staging: Staging,
    digest: &str,
    byte_length: u64,
) -> Result<Stored, Diagnostic> {
    let destination = absolute_path(root, digest);
    let relative = archive_path(digest);
    if paths::is_symlink(&destination) {
        return Err(paths::symlink_refusal(
            Details::new()
                .text("scope", "archive")
                .text("archive_path", relative),
        ));
    }
    check_object_permissions(root, digest)?;
    if let Ok(metadata) = fs::symlink_metadata(&destination) {
        // Shape before permissions: a directory at an object's path is not an
        // object whose permissions could be wide, it is something openPapir
        // did not create, so it reports `path.overwrite` rather than
        // `archive.permissions_wide`.
        if !metadata.is_file() {
            return Err(Diagnostic::new(
                codes::PATH_OVERWRITE,
                "A stored object's path holds something openPapir did not create.",
                Details::new().text("archive_path", relative),
            ));
        }
        paths::refuse_if_wide(&destination, &relative)?;
        if metadata.len() != byte_length {
            return Err(Diagnostic::new(
                codes::INTEGRITY_LENGTH_MISMATCH,
                "A stored object's length differs from the incoming length.",
                Details::new()
                    .text("archive_path", relative)
                    .int("expected_bytes", byte_length)
                    .int("observed_bytes", metadata.len()),
            ));
        }
        // The staging file is discarded by its own drop; the object is left
        // untouched, and the caller records a second import event against it.
        return Ok(Stored {
            digest: digest.to_owned(),
            byte_length,
            created_object: false,
            warnings: Vec::new(),
        });
    }
    let mut warnings = create_object_directories(root, digest, &relative)?;
    paths::set_object_read_only(staging.path())
        .map_err(|error| paths::publish_refusal(&error, &relative, "object"))?;
    warnings.extend(staging.publish(&destination, &relative, "object")?);
    Ok(Stored {
        digest: digest.to_owned(),
        byte_length,
        created_object: true,
        warnings,
    })
}

/// Refuse a fan-out directory of this digest that is wider than owner-only.
///
/// The check runs before anything is published, so a wide directory stops the
/// operation rather than receiving an object. A record that references an
/// object runs the same check before it is written.
///
/// # Errors
///
/// Returns `archive.permissions_wide`, naming the archive-relative path.
pub fn check_object_permissions(root: &Path, digest: &str) -> Result<(), Diagnostic> {
    let mut relative = format!("{OBJECTS_DIR}/{ALGORITHM}");
    let mut path = root.join(OBJECTS_DIR).join(ALGORITHM);
    for segment in [&digest[0..2], &digest[2..4]] {
        relative = format!("{relative}/{segment}");
        path = path.join(segment);
        if path.exists() {
            paths::refuse_if_wide(&path, &relative)?;
        }
    }
    Ok(())
}

/// Create the two fan-out directories for a digest, owner-only.
fn create_object_directories(
    root: &Path,
    digest: &str,
    relative: &str,
) -> Result<Vec<Warning>, Diagnostic> {
    let mut warnings = Vec::new();
    let first = root.join(OBJECTS_DIR).join(ALGORITHM).join(&digest[0..2]);
    let second = first.join(&digest[2..4]);
    for directory in [&first, &second] {
        if paths::is_symlink(directory) {
            return Err(paths::symlink_refusal(
                Details::new()
                    .text("scope", "archive")
                    .text("archive_path", relative),
            ));
        }
        paths::create_dir_owner_only(directory)
            .map_err(|error| paths::publish_refusal(&error, relative, "object"))?;
        if let Some(warning) =
            paths::sync_directory(directory.parent().unwrap_or(root), "object_write")
        {
            warnings.push(warning);
        }
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn the_object_path_fans_out_on_the_digest() {
        assert_eq!(
            archive_path(EMPTY_DIGEST),
            format!("objects/sha256/e3/b0/{EMPTY_DIGEST}")
        );
        let root = Path::new("/archive");
        assert_eq!(
            absolute_path(root, EMPTY_DIGEST),
            root.join("objects/sha256/e3/b0").join(EMPTY_DIGEST)
        );
    }
}
