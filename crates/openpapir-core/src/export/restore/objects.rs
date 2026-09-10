//! The objects an export holds: checked first, stored second.
//!
//! Every object the manifest lists is re-digested from the export's own bytes
//! before the archive is written to at all, so an export whose copies do not
//! hold what their names say is refused while the archive is still exactly as
//! it was. Re-digesting is a storage-layer identity check and never a
//! cryptographic verification: it says the bytes are the bytes the digest
//! names, and nothing about authenticity, origin, delivery, or legal effect.
//!
//! Storing goes through the archive's own content-addressed store, so an
//! object the archive already holds is left untouched and is not an error,
//! exactly as a duplicate import is not (`docs/archive-layout.md`). The store
//! re-digests the bytes a second time as it writes them, so a copy that
//! changed between the check and the store is refused rather than kept.

use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use crate::archive::{limits, objects, paths};
use crate::error::{Details, Diagnostic, codes};
use crate::export::destination::OBJECTS_DIR;
use crate::export::restore::manifest::Manifest;
use crate::export::restore::{SOURCE_SCOPE, linked};
use crate::ident;

/// How many bytes are read from an exported copy at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// Why an exported object is not the object the manifest describes.
///
/// The values are a closed set and are stable, so a caller may branch on one.
mod reasons {
    /// The export does not hold the copy at all.
    pub const ABSENT: &str = "absent";
    /// The copy re-digests to something other than its own name.
    pub const DIGEST: &str = "digest";
    /// The copy holds a different number of bytes than the manifest says.
    pub const LENGTH: &str = "length";
    /// The path is there and is not a regular file, so it holds no copy.
    pub const UNUSABLE: &str = "unusable";
}

/// One object this import put into the store, or found already there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The number of bytes the object holds.
    pub byte_length: u64,
    /// Whether this import created the object or found it already present.
    pub created: bool,
    /// The lowercase hexadecimal digest naming the object.
    pub digest: String,
}

/// Every object one import placed, in the order the manifest lists them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Placements(Vec<Placed>);

impl Placements {
    /// The objects this import created, oldest first.
    pub fn created_objects(&self) -> impl Iterator<Item = &Placed> {
        self.0.iter().filter(|placed| placed.created)
    }

    /// How many bytes of newly stored objects this import read.
    #[must_use]
    pub fn bytes_stored(&self) -> u64 {
        self.created_objects()
            .map(|placed| placed.byte_length)
            .sum()
    }

    /// How many objects this import stored.
    #[must_use]
    pub fn created(&self) -> u64 {
        self.created_objects().count() as u64
    }

    /// How many objects the archive already held.
    #[must_use]
    pub fn present(&self) -> u64 {
        self.0.len() as u64 - self.created()
    }

    /// Record one placement, for a test that needs a set without a store.
    #[cfg(test)]
    pub fn push_for_test(&mut self, digest: &str, byte_length: u64, created: bool) {
        self.0.push(Placed {
            byte_length,
            created,
            digest: digest.to_owned(),
        });
    }
}

/// Re-digest every object the manifest lists, changing nothing anywhere.
///
/// # Errors
///
/// Returns `export.object_mismatch` for a copy the export does not hold or
/// whose bytes disagree with the manifest, `path.symlink` for a link in the
/// export, `input.cap.file_size` for a copy over the single-file cap, and
/// `write.interrupted` when a copy cannot be read.
pub fn verify(source: &Path, manifest: &Manifest) -> Result<(), Diagnostic> {
    for (index, object) in manifest.objects.iter().enumerate() {
        let mut copy = open(source, &object.digest)?;
        let (digest, byte_length) = stream(&mut copy, index as u64)?;
        if byte_length != object.byte_length {
            return Err(mismatch(&object.digest, reasons::LENGTH));
        }
        if digest != object.digest {
            return Err(mismatch(&object.digest, reasons::DIGEST));
        }
    }
    Ok(())
}

/// Store every object the manifest lists, under the writer lock.
///
/// `placed` collects what was actually put in the store, and it collects it
/// whether the pass finished or not: an object placed before a refusal is
/// still in the archive, and the caller has to be able to remove it. Handing
/// the set back only on success would leave those objects behind as orphans
/// that `archive check` then reports.
///
/// # Errors
///
/// Returns the refusals of [`verify`], plus the store's own
/// `integrity.length_mismatch`, `path.overwrite`, `archive.permissions_wide`,
/// `input.cap.import_bytes`, and `write.interrupted`.
pub fn store_all(
    root: &Path,
    source: &Path,
    manifest: &Manifest,
    placed: &mut Placements,
) -> Result<(), Diagnostic> {
    let mut read_total = 0_u64;
    for (index, object) in manifest.objects.iter().enumerate() {
        let mut copy = open(source, &object.digest)?;
        let stored = objects::store(root, &mut copy, index as u64, &mut read_total)?;
        if stored.digest != object.digest {
            // The copy changed between the check and the store, which is the
            // one thing the pre-flight check cannot promise. The object is
            // recorded before it is refused, so it is removed with the rest.
            placed.0.push(Placed {
                byte_length: stored.byte_length,
                created: stored.created_object,
                digest: stored.digest,
            });
            return Err(mismatch(&object.digest, reasons::DIGEST));
        }
        placed.0.push(Placed {
            byte_length: stored.byte_length,
            created: stored.created_object,
            digest: stored.digest,
        });
    }
    Ok(())
}

/// Open one exported copy read-only, refusing everything that is not one.
///
/// The source is a directory the user named, so it is untrusted input like a
/// record directory is. A symbolic link is refused rather than followed, and
/// the open is the non-blocking no-follow one, because a named pipe planted
/// at an object's path would otherwise hold a blocking open open forever and
/// hang the import. The file kind is then taken from the opened handle rather
/// than from a second look at the path, so the file that passes the check is
/// the file whose bytes are read, and anything that is not a regular file is
/// refused as holding no copy at all.
fn open(source: &Path, digest: &str) -> Result<File, Diagnostic> {
    let path = source.join(OBJECTS_DIR).join(digest);
    if paths::is_symlink(&path) {
        return Err(linked());
    }
    let file = paths::open_no_follow_nonblocking(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            mismatch(digest, reasons::ABSENT)
        } else {
            read_refusal()
        }
    })?;
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(mismatch(digest, reasons::UNUSABLE));
    }
    Ok(file)
}

/// Read one copy in bounded chunks, digesting and counting as it goes.
///
/// The single-file cap is enforced while reading, exactly as import enforces
/// it, so a copy that is larger than the archive accepts stops the import
/// rather than being read unbounded.
fn stream(copy: &mut File, index: u64) -> Result<(String, u64), Diagnostic> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    let mut byte_length = 0_u64;
    loop {
        let read = copy.read(&mut buffer).map_err(|_| read_refusal())?;
        if read == 0 {
            break;
        }
        byte_length += read as u64;
        limits::check_file_size(byte_length, index)?;
        hasher.update(&buffer[..read]);
    }
    Ok((ident::hex(&hasher.finalize()), byte_length))
}

/// The refusal for an exported copy whose bytes could not be read.
fn read_refusal() -> Diagnostic {
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "An exported copy could not be read for the import.",
        Details::new()
            .text("scope", SOURCE_SCOPE)
            .text("stage", "object_write"),
    )
    .retryable()
}

/// The refusal for a copy that is not the object the manifest describes.
///
/// The digest is the manifest's own name for the object, which is a digest of
/// a stored artefact in a context the design already exposes, so it is
/// reported; nothing about the bytes actually found is
/// (`docs/error-contract.md`).
fn mismatch(digest: &str, reason: &'static str) -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_OBJECT_MISMATCH,
        "An exported object is not the object the manifest describes.",
        Details::new()
            .text("scope", SOURCE_SCOPE)
            .text("digest", digest.to_owned())
            .text("reason", reason)
            .int("conflict_count", 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::restore::manifest::ObjectRow;
    use std::fs;

    const PAYLOAD: &[u8] = b"synthetic bytes\n";
    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    fn manifest(byte_length: u64) -> Manifest {
        Manifest {
            case_count: 1,
            case_id: Some("0123456789abcdef0123456789abcdef".to_owned()),
            counts: Vec::new(),
            objects: vec![ObjectRow {
                algorithm: "sha256".to_owned(),
                byte_length,
                digest: DIGEST.to_owned(),
            }],
            records: Vec::new(),
        }
    }

    fn export_with(source: &Path, bytes: &[u8]) {
        let directory = source.join(OBJECTS_DIR);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(DIGEST), bytes).unwrap();
    }

    #[test]
    fn an_export_whose_copies_hold_their_own_bytes_verifies() {
        let source = tempfile::tempdir().unwrap();
        export_with(source.path(), PAYLOAD);
        verify(source.path(), &manifest(16)).unwrap();
    }

    #[test]
    fn a_copy_the_export_lacks_or_changed_is_refused_with_its_reason() {
        let source = tempfile::tempdir().unwrap();
        for (bytes, length, reason) in [
            (None, 16, "absent"),
            (Some(&b"other bytes 12345"[..]), 16, "length"),
            (Some(&b"other bytes 1234"[..]), 16, "digest"),
        ] {
            match bytes {
                Some(bytes) => export_with(source.path(), bytes),
                None => {
                    let _ = fs::remove_file(source.path().join(OBJECTS_DIR).join(DIGEST));
                }
            }
            let refusal = verify(source.path(), &manifest(length)).unwrap_err();
            assert_eq!(refusal.code, codes::EXPORT_OBJECT_MISMATCH);
            assert_eq!(refusal.exit_code(), 4);
            assert!(!refusal.is_retryable());
            let json = serde_json::to_value(&refusal).unwrap();
            assert_eq!(json["details"]["reason"], reason);
            assert_eq!(json["details"]["digest"], DIGEST);
            assert_eq!(json["details"]["scope"], "export_source");
            assert_eq!(json["details"]["conflict_count"], 1);
        }
    }

    #[test]
    fn storing_reports_what_was_created_and_what_was_already_there() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        crate::archive::init(&root).unwrap();
        let source = home.path().join("export");
        fs::create_dir(&source).unwrap();
        export_with(&source, PAYLOAD);

        let mut placed = Placements::default();
        store_all(&root, &source, &manifest(16), &mut placed).unwrap();
        assert_eq!(placed.created(), 1);
        assert_eq!(placed.present(), 0);
        assert_eq!(placed.bytes_stored(), 16);
        assert_eq!(
            fs::read(objects::absolute_path(&root, DIGEST)).unwrap(),
            PAYLOAD
        );

        let mut again = Placements::default();
        store_all(&root, &source, &manifest(16), &mut again).unwrap();
        assert_eq!(again.created(), 0);
        assert_eq!(again.present(), 1);
        assert_eq!(again.bytes_stored(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_copy_is_refused_rather_than_followed() {
        let home = tempfile::tempdir().unwrap();
        let source = home.path().join("export");
        fs::create_dir_all(source.join(OBJECTS_DIR)).unwrap();
        let elsewhere = home.path().join("elsewhere");
        fs::write(&elsewhere, PAYLOAD).unwrap();
        std::os::unix::fs::symlink(&elsewhere, source.join(OBJECTS_DIR).join(DIGEST)).unwrap();
        let refusal = verify(&source, &manifest(16)).unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["scope"],
            "export_source"
        );
    }
}
