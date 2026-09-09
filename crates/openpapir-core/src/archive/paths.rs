//! Path safety, owner-only permissions, and durability primitives.
//!
//! Every rule here comes from `docs/archive-layout.md`. No path inside the
//! archive root may be a symbolic link, and the rule is enforced with the
//! platform's no-follow flag when the path is opened rather than by a stat
//! call beforehand, because a pre-check is a time-of-check-to-time-of-use bug
//! rather than a defence. Directories are created owner-only, files are
//! created owner-read-write, and a stored object becomes owner-read-only.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use crate::error::{Details, Diagnostic, Warning, codes};

/// Open a file for reading without following a symbolic link.
///
/// # Errors
///
/// Returns the underlying I/O error. On Unix a symbolic link fails at the
/// system call itself; on Windows, which has no portable no-follow flag, the
/// link state is checked first and the weaker guarantee is documented.
pub fn open_no_follow(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        if is_symlink(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is a symbolic link",
            ));
        }
        OpenOptions::new().read(true).open(path)
    }
}

/// Whether the path itself is a symbolic link, without following it.
#[must_use]
pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Create a directory owner-only, ignoring the case where it already exists.
///
/// # Errors
///
/// Returns the underlying I/O error.
pub fn create_dir_owner_only(path: &Path) -> io::Result<()> {
    if path.is_dir() && !is_symlink(path) {
        return narrow_to_owner_only(path);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        fs::DirBuilder::new().mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    {
        fs::DirBuilder::new().create(path)?;
    }
    Ok(())
}

/// Create a new file owner-read-write, refusing to replace an existing one.
///
/// # Errors
///
/// Returns the underlying I/O error, including `AlreadyExists`.
pub fn create_file_owner_only(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

/// Narrow a path's permissions to owner-only. It never widens them.
///
/// # Errors
///
/// Returns the underlying I/O error.
pub fn narrow_to_owner_only(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let metadata = fs::symlink_metadata(path)?;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            let owner_only = if metadata.is_dir() { 0o700 } else { 0o600 };
            fs::set_permissions(path, fs::Permissions::from_mode(owner_only))?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Make a stored object owner-read-only, so that it is never rewritten.
///
/// # Errors
///
/// Returns the underlying I/O error.
pub fn set_object_read_only(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o400))
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions)
    }
}

/// Whether a path's permissions are wider than owner-only.
#[must_use]
pub fn is_wider_than_owner_only(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::symlink_metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o077 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// The device a path lives on, where the platform reports one.
#[must_use]
pub fn device_of(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        fs::symlink_metadata(path)
            .ok()
            .map(|metadata| metadata.dev())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// Flush a directory entry, reporting the platforms that cannot.
///
/// On Unix the directory is opened and synchronised. On Windows there is no
/// portable equivalent, so the named degradation is returned as a warning
/// rather than silently accepted (`docs/error-contract.md`).
#[must_use]
pub fn sync_directory(path: &Path, stage: &'static str) -> Option<Warning> {
    #[cfg(unix)]
    {
        let _ = File::open(path).map(|directory| directory.sync_all());
        let _ = stage;
        None
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Some(no_directory_fsync_warning(stage))
    }
}

/// The warning reported where a directory entry cannot be flushed.
#[must_use]
pub fn no_directory_fsync_warning(stage: &'static str) -> Warning {
    Diagnostic::new(
        codes::PLATFORM_NO_DIRECTORY_FSYNC,
        "Directory durability is weaker on this platform.",
        Details::new().text("stage", stage),
    )
}

/// The warning reported where owner-only access is an access-control list.
#[must_use]
pub fn owner_only_via_acl_warning() -> Warning {
    Diagnostic::new(
        codes::PLATFORM_OWNER_ONLY_VIA_ACL,
        "Owner-only access is expressed as an access-control list on this platform.",
        Details::new(),
    )
}

/// The refusal reported for a symbolic link that must not be one.
#[must_use]
pub fn symlink_refusal(details: Details) -> Diagnostic {
    Diagnostic::new(
        codes::PATH_SYMLINK,
        "A path that must not be a symbolic link is one.",
        details,
    )
}

/// Map an I/O error raised while publishing a file into the contract.
///
/// `AlreadyExists` is a refusal to replace a file openPapir did not create; a
/// cross-device link or rename means the archive is misconfigured; anything
/// else interrupted the write before its publish step, so the archive holds
/// the complete artefact or nothing.
#[must_use]
pub fn publish_refusal(error: &io::Error, archive_path: &str, stage: &'static str) -> Diagnostic {
    if error.kind() == io::ErrorKind::AlreadyExists {
        return Diagnostic::new(
            codes::PATH_OVERWRITE,
            "A write would replace a file openPapir did not create.",
            Details::new().text("archive_path", archive_path),
        );
    }
    if is_cross_device(error) {
        return Diagnostic::new(
            codes::PATH_CROSS_DEVICE,
            "A write would cross a device boundary, which the archive forbids.",
            Details::new().text("archive_path", archive_path),
        );
    }
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "A write was interrupted before it could be published.",
        Details::new().text("stage", stage),
    )
    .retryable()
}

/// Whether an I/O error reports a cross-device operation.
#[must_use]
pub fn is_cross_device(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::EXDEV)
    }
    #[cfg(not(unix))]
    {
        // Windows reports a cross-volume link as ERROR_NOT_SAME_DEVICE (17).
        error.raw_os_error() == Some(17)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cross_device_error_is_refused_as_a_path_condition() {
        #[cfg(unix)]
        let error = io::Error::from_raw_os_error(libc::EXDEV);
        #[cfg(not(unix))]
        let error = io::Error::from_raw_os_error(17);
        let refusal = publish_refusal(&error, "objects/sha256/ab/cd/digest", "object");
        assert_eq!(refusal.code, codes::PATH_CROSS_DEVICE);
        assert_eq!(refusal.exit_code(), 3);
        assert!(is_cross_device(&error));
    }

    #[test]
    fn an_existing_destination_is_refused_as_an_overwrite() {
        let error = io::Error::new(io::ErrorKind::AlreadyExists, "exists");
        let refusal = publish_refusal(&error, "records/imports/id.json", "record");
        assert_eq!(refusal.code, codes::PATH_OVERWRITE);
        assert_eq!(refusal.exit_code(), 3);
    }

    #[test]
    fn any_other_publish_failure_is_an_interrupted_write() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let refusal = publish_refusal(&error, "objects/sha256/ab/cd/digest", "object");
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert!(refusal.is_retryable());
        assert_eq!(refusal.exit_code(), 4);
        assert!(!is_cross_device(&error));
    }

    #[test]
    fn the_named_platform_degradations_are_warnings_with_their_stage() {
        let warning = no_directory_fsync_warning("record_write");
        assert_eq!(warning.code, codes::PLATFORM_NO_DIRECTORY_FSYNC);
        assert_eq!(
            serde_json::to_value(&warning).unwrap()["details"]["stage"],
            "record_write"
        );
        assert_eq!(
            owner_only_via_acl_warning().code,
            codes::PLATFORM_OWNER_ONLY_VIA_ACL
        );
        assert_eq!(
            symlink_refusal(Details::new().text("scope", "archive")).code,
            codes::PATH_SYMLINK
        );
    }
}
