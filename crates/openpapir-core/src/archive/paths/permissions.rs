//! Owner-only permissions, the stored object's read-only attribute, and the
//! durability primitive a directory entry needs.
//!
//! Every rule here comes from `docs/archive-layout.md`. Directories are
//! created owner-only, files are created owner-read-write, and a stored
//! object becomes owner-read-only. Nothing here ever widens access, and a
//! directory that is already there is never narrowed on the way past: that is
//! an explicit repair action, not a side effect.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use crate::error::{Details, Diagnostic, Warning, codes};

use super::open::is_symlink;
#[cfg(not(unix))]
use super::open::open_no_follow;

/// Create a directory owner-only, ignoring the case where it already exists.
///
/// An existing directory is left exactly as it is. Narrowing a directory that
/// is already there would be an implicit permission repair, and
/// `docs/archive-layout.md` allows narrowing only through an explicit repair
/// action, which is not implemented. A directory that is already wider than
/// owner-only is refused by the permission check instead.
///
/// # Errors
///
/// Returns the underlying I/O error.
pub fn create_dir_owner_only(path: &Path) -> io::Result<()> {
    if path.is_dir() && !is_symlink(path) {
        return Ok(());
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

/// Clear a path's read-only attribute, reporting the permissions it had.
///
/// The counterpart of [`set_object_read_only`], compiled only for platforms
/// whose read-only attribute refuses an unlink outright. `set_readonly(false)`
/// clears it for everyone rather than for the owner alone, which is what the
/// lint names; it widens nothing, because owner-only access there is the
/// access-control list and not this bit (`docs/archive-layout.md`), and every
/// caller removes or replaces the file a moment later. The permissions are
/// read through [`open_no_follow`], so a planted link is refused here and not
/// in each caller; the clearing itself is still by path, for want of a handle
/// form of it, so nothing is locked across the two calls. The permissions are
/// returned so a caller restoring the attribute reads them once, not twice.
///
/// # Errors
///
/// Returns the underlying I/O error, a planted link's refusal included.
#[cfg(not(unix))]
pub(crate) fn clear_read_only(path: &Path) -> io::Result<fs::Permissions> {
    let original = open_no_follow(path)?.metadata()?.permissions();
    let mut cleared = original.clone();
    #[allow(clippy::permissions_set_readonly_false)]
    cleared.set_readonly(false);
    fs::set_permissions(path, cleared).map(|()| original)
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

/// Flush a directory entry, reporting the platforms that cannot.
///
/// On Unix the directory is opened and synchronised. On Windows there is no
/// portable equivalent, so the named degradation is returned as a warning
/// rather than silently accepted (`docs/error-contract.md`).
#[must_use]
pub fn sync_directory(path: &Path, stage: &'static str) -> Option<Warning> {
    #[cfg(unix)]
    {
        match File::open(path).and_then(|directory| directory.sync_all()) {
            Ok(()) => None,
            // The flush the design requires did not happen, so the weakening
            // is reported rather than swallowed.
            Err(_) => Some(no_directory_fsync_warning(stage)),
        }
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

/// Refuse a path inside the archive whose permissions are wider than
/// owner-only. There is no override flag; the only remedy the design allows
/// is an explicit repair action, which only narrows and is not implemented.
///
/// # Errors
///
/// Returns `archive.permissions_wide`, naming the archive-relative path.
pub fn refuse_if_wide(path: &Path, archive_path: &str) -> Result<(), Diagnostic> {
    if is_wider_than_owner_only(path) {
        return Err(wide_permissions_refusal(archive_path, &[]));
    }
    Ok(())
}

/// The refusal reported for archive permissions wider than owner-only.
///
/// The refused set is non-empty by the signature: `first` is the path the
/// refusal names and `others` are the further paths it only counts. A caller
/// that found nothing wide has nothing to refuse and never calls this, so no
/// refusal can name `.` with a count of zero.
#[must_use]
pub fn wide_permissions_refusal(first: &str, others: &[String]) -> Diagnostic {
    Diagnostic::new(
        codes::ARCHIVE_PERMISSIONS_WIDE,
        "The archive's permissions are wider than owner-only.",
        Details::new()
            .text("archive_path", first)
            .int("path_count", others.len() as u64 + 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_path_is_refused_and_names_itself_relative_to_the_archive() {
        let refusal =
            wide_permissions_refusal("objects/sha256/ab/cd", &["records/imports".to_owned()]);
        assert_eq!(refusal.code, codes::ARCHIVE_PERMISSIONS_WIDE);
        assert_eq!(refusal.exit_code(), 4);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["archive_path"], "objects/sha256/ab/cd");
        assert_eq!(json["details"]["path_count"], 2);
        // The set is non-empty by the signature, so the smallest refusal names
        // its one path and counts one, never `.` with a count of zero.
        let single = serde_json::to_value(wide_permissions_refusal("records", &[])).unwrap();
        assert_eq!(single["details"]["archive_path"], "records");
        assert_eq!(single["details"]["path_count"], 1);
    }

    #[test]
    fn an_existing_directory_is_never_narrowed_on_the_way_past() {
        let directory = tempfile::tempdir().unwrap();
        let existing = directory.path().join("records");
        fs::create_dir(&existing).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();
            create_dir_owner_only(&existing).unwrap();
            assert_eq!(
                fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
                0o755,
                "openPapir never repairs permissions implicitly"
            );
            assert!(refuse_if_wide(&existing, "records").is_err());
        }
        let fresh = directory.path().join("objects");
        create_dir_owner_only(&fresh).unwrap();
        assert!(refuse_if_wide(&fresh, "objects").is_ok());
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
    }
}
