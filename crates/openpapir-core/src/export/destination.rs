//! The export destination: where openPapir may write, and how.
//!
//! The destination is the one path in this crate that lies outside the
//! archive root, so every rule the archive applies inward is applied outward
//! here. A symbolic link is refused rather than followed, a file that is
//! already there is refused rather than replaced, and a destination inside the
//! archive root is refused before anything is copied, because an export that
//! wrote into the archive would no longer be a copy of it.
//!
//! The destination path itself never reaches a diagnostic. What a refusal
//! names is the path relative to the destination, which is built from
//! openPapir's own fixed directory names plus a digest or an identifier
//! (`docs/error-contract.md`).

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::archive::paths;
use crate::archive::write::Staging;
use crate::error::{Details, Diagnostic, Warning, codes};

/// The directory holding copied objects, relative to the destination.
pub const OBJECTS_DIR: &str = "objects";
/// The directory holding exported records, relative to the destination.
pub const RECORDS_DIR: &str = "records";
/// The manifest's file name, relative to the destination.
pub const MANIFEST_FILE: &str = "manifest.json";

/// A prepared destination directory, owned by this export.
#[derive(Debug)]
pub struct Destination {
    path: PathBuf,
}

impl Destination {
    /// The destination's own path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create the directory copied objects go into, once.
    ///
    /// # Errors
    ///
    /// Returns `path.symlink`, `export.destination_conflict`, or
    /// `write.interrupted`.
    pub fn objects_directory(&self) -> Result<PathBuf, Diagnostic> {
        self.directory(OBJECTS_DIR)
    }

    /// Create the directory one record kind goes into, once.
    ///
    /// The kind is one of openPapir's own record-kind names, so no
    /// user-supplied text is ever joined into a path.
    ///
    /// # Errors
    ///
    /// Returns `path.symlink`, `export.destination_conflict`, or
    /// `write.interrupted`.
    pub fn records_directory(&self, kind: &str) -> Result<PathBuf, Diagnostic> {
        self.directory(RECORDS_DIR)?;
        self.directory(&format!("{RECORDS_DIR}/{kind}"))
    }

    /// Create one directory inside the destination, owner-only.
    fn directory(&self, relative: &str) -> Result<PathBuf, Diagnostic> {
        let path = self.path.join(relative);
        if paths::is_symlink(&path) {
            return Err(symlink_refusal(relative));
        }
        paths::create_dir_owner_only(&path).map_err(|error| refusal(&error, relative))?;
        Ok(path)
    }
}

/// Refuse the destination before anything is copied, then make it ready.
///
/// The destination is an existing empty directory or one this call creates,
/// owner-only. It is never inside the archive root, and it is never a
/// symbolic link.
///
/// # Errors
///
/// Returns `usage.arguments` when the destination is unusable or lies inside
/// the archive root, `path.symlink` when it is a symbolic link,
/// `export.destination_conflict` when it already holds anything, and
/// `write.interrupted` when it cannot be created.
pub fn prepare(archive_root: &Path, destination: &Path) -> Result<Destination, Diagnostic> {
    if paths::is_symlink(destination) {
        return Err(symlink_refusal("."));
    }
    refuse_inside_archive(archive_root, destination)?;
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_dir() => refuse_non_empty(destination)?,
        Ok(_) => return Err(unusable()),
        Err(_) => {
            paths::create_dir_owner_only(destination).map_err(|error| refusal(&error, "."))?
        }
    }
    Ok(Destination {
        path: destination.to_path_buf(),
    })
}

/// Refuse a destination inside the archive root.
///
/// The comparison is made on resolved paths, so a relative path, a `..`
/// segment, or a link on the way to the destination cannot hide the fact that
/// the export would write into the archive it is copying.
fn refuse_inside_archive(archive_root: &Path, destination: &Path) -> Result<(), Diagnostic> {
    let Ok(archive) = archive_root.canonicalize() else {
        return Ok(());
    };
    let resolved = match destination.canonicalize() {
        Ok(resolved) => resolved,
        Err(_) => {
            let parent = destination
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty());
            let name = destination.file_name().ok_or_else(unusable)?;
            let Ok(parent) = parent.unwrap_or_else(|| Path::new(".")).canonicalize() else {
                return Err(unusable());
            };
            parent.join(name)
        }
    };
    if resolved.starts_with(&archive) {
        return Err(Diagnostic::new(
            codes::USAGE_ARGUMENTS,
            "The export destination is inside the archive root.",
            Details::new()
                .text("argument", "destination")
                .text("scope", "export_destination"),
        ));
    }
    Ok(())
}

/// Refuse a destination directory that already holds anything.
fn refuse_non_empty(destination: &Path) -> Result<(), Diagnostic> {
    let entries = fs::read_dir(destination).map_err(|_| unusable())?;
    let count = entries.count() as u64;
    if count > 0 {
        return Err(conflict(".", count));
    }
    Ok(())
}

/// Write one file into the destination, replacing nothing.
///
/// The content is staged inside the destination itself and linked into place,
/// which is the archive's own publish step: it is atomic, stays on one
/// filesystem, and fails rather than replacing a file openPapir did not
/// create.
///
/// # Errors
///
/// Returns `path.symlink`, `export.destination_conflict`, or
/// `write.interrupted`.
pub fn write_new(
    directory: &Path,
    file_name: &str,
    relative: &str,
    content: &[u8],
    stage: &'static str,
) -> Result<Vec<Warning>, Diagnostic> {
    let target = directory.join(file_name);
    refuse_existing(&target, relative)?;
    let mut staging = Staging::create(directory, stage)?;
    staging
        .file()
        .write_all(content)
        .map_err(|error| refusal(&error, relative))?;
    publish(staging, &target, relative, stage)
}

/// Put a staged file at its destination path, mapping the refusals outward.
///
/// # Errors
///
/// Returns `path.symlink`, `export.destination_conflict`, or
/// `write.interrupted`.
pub fn publish(
    staging: Staging,
    target: &Path,
    relative: &str,
    stage: &'static str,
) -> Result<Vec<Warning>, Diagnostic> {
    staging.publish(target, relative, stage).map_err(|error| {
        if error.code == codes::PATH_OVERWRITE {
            conflict(relative, 1)
        } else {
            error
        }
    })
}

/// Refuse a target path that already exists, whatever it is.
///
/// # Errors
///
/// Returns `path.symlink` for a link and `export.destination_conflict` for
/// anything else already there.
pub fn refuse_existing(target: &Path, relative: &str) -> Result<(), Diagnostic> {
    match fs::symlink_metadata(target) {
        Err(_) => Ok(()),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(symlink_refusal(relative)),
        Ok(_) => Err(conflict(relative, 1)),
    }
}

/// The refusal for a destination path that must not be a symbolic link.
#[must_use]
pub fn symlink_refusal(relative: &str) -> Diagnostic {
    paths::symlink_refusal(
        Details::new()
            .text("scope", "export_destination")
            .text("export_path", relative.to_owned()),
    )
}

/// The refusal for a destination that already holds something.
#[must_use]
pub fn conflict(relative: &str, count: u64) -> Diagnostic {
    Diagnostic::new(
        codes::EXPORT_DESTINATION_CONFLICT,
        "The export destination already holds a path the export would have to replace.",
        Details::new()
            .text("scope", "export_destination")
            .text("export_path", relative.to_owned())
            .int("conflict_count", count),
    )
}

/// The refusal for a destination openPapir cannot use as a directory.
fn unusable() -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "The export destination is not a usable directory.",
        Details::new()
            .text("argument", "destination")
            .text("scope", "export_destination"),
    )
}

/// Map an I/O failure in the destination into the contract.
///
/// An existing path is a conflict rather than an overwrite, because the
/// destination is outside the archive; everything else interrupted the write,
/// which leaves the complete file or nothing.
#[must_use]
pub fn refusal(error: &io::Error, relative: &str) -> Diagnostic {
    if error.kind() == io::ErrorKind::AlreadyExists {
        return conflict(relative, 1);
    }
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "A write into the export destination was interrupted before it could be published.",
        Details::new().text("stage", "record_write"),
    )
    .retryable()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_and_home() -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("archive");
        fs::create_dir(&root).unwrap();
        (home, root)
    }

    #[test]
    fn a_destination_inside_the_archive_is_refused_before_anything_is_written() {
        let (home, root) = archive_and_home();
        for inside in [root.join("export"), root.clone(), root.join("objects/out")] {
            let refusal = prepare(&root, &inside).unwrap_err();
            assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
            assert_eq!(refusal.exit_code(), 2);
        }
        let outside = home.path().join("export");
        assert!(prepare(&root, &outside).is_ok());
        assert!(outside.is_dir(), "the destination is created");
        let _ = home;
    }

    #[test]
    fn a_destination_that_holds_anything_is_a_conflict() {
        let (home, root) = archive_and_home();
        let destination = home.path().join("export");
        fs::create_dir(&destination).unwrap();
        assert!(
            prepare(&root, &destination).is_ok(),
            "an existing empty directory is accepted"
        );
        fs::write(destination.join("stray"), b"x").unwrap();
        let refusal = prepare(&root, &destination).unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_DESTINATION_CONFLICT);
        assert_eq!(refusal.exit_code(), 4);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["conflict_count"], 1);
        assert_eq!(json["details"]["scope"], "export_destination");
        let rendered = serde_json::to_string(&refusal).unwrap();
        assert!(
            !rendered.contains(&destination.to_string_lossy().into_owned()),
            "the destination path is never echoed"
        );
    }

    #[test]
    fn a_destination_that_is_not_a_directory_is_a_usage_refusal() {
        let (home, root) = archive_and_home();
        let file = home.path().join("plain");
        fs::write(&file, b"x").unwrap();
        assert_eq!(
            prepare(&root, &file).unwrap_err().code,
            codes::USAGE_ARGUMENTS
        );
        let missing_parent = home.path().join("absent").join("export");
        assert_eq!(
            prepare(&root, &missing_parent).unwrap_err().code,
            codes::USAGE_ARGUMENTS
        );
    }

    #[test]
    fn an_existing_target_file_is_refused_rather_than_replaced() {
        let (home, root) = archive_and_home();
        let destination = home.path().join("export");
        let prepared = prepare(&root, &destination).unwrap();
        let objects = prepared.objects_directory().unwrap();
        write_new(&objects, "name", "objects/name", b"first\n", "object_write").unwrap();
        assert_eq!(fs::read(objects.join("name")).unwrap(), b"first\n");
        let refusal = write_new(
            &objects,
            "name",
            "objects/name",
            b"second\n",
            "object_write",
        )
        .unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_DESTINATION_CONFLICT);
        assert_eq!(
            fs::read(objects.join("name")).unwrap(),
            b"first\n",
            "the existing file is left exactly as it was"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_in_the_destination_is_never_followed() {
        let (home, root) = archive_and_home();
        let elsewhere = home.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let linked = home.path().join("linked");
        std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();
        let refusal = prepare(&root, &linked).unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        assert_eq!(refusal.exit_code(), 3);

        let destination = home.path().join("export");
        let prepared = prepare(&root, &destination).unwrap();
        let objects = prepared.objects_directory().unwrap();
        let target = objects.join("name");
        std::os::unix::fs::symlink(home.path().join("outside"), &target).unwrap();
        assert_eq!(
            write_new(&objects, "name", "objects/name", b"x", "object_write")
                .unwrap_err()
                .code,
            codes::PATH_SYMLINK
        );
        assert!(
            !home.path().join("outside").exists(),
            "the link's target is never created"
        );
    }

    #[test]
    fn an_interrupted_write_is_reported_as_such() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let interrupted = refusal(&error, "objects/name");
        assert_eq!(interrupted.code, codes::WRITE_INTERRUPTED);
        assert!(interrupted.is_retryable());
        assert_eq!(
            refusal(&io::Error::new(io::ErrorKind::AlreadyExists, "exists"), ".").code,
            codes::EXPORT_DESTINATION_CONFLICT
        );
    }
}
