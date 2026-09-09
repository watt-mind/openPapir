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

/// Open the reparse point itself instead of whatever it points at.
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
/// Permit a directory handle, so that a junction is opened and then refused
/// rather than failing as an unreadable path.
#[cfg(windows)]
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// The attribute every reparse point carries, an NTFS junction included.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
/// The attribute a directory carries.
#[cfg(windows)]
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;

/// Open a file for reading without following a symbolic link.
///
/// On Unix the open carries `O_NOFOLLOW`, so a symbolic link fails at the
/// system call itself. On Windows the open carries
/// `FILE_FLAG_OPEN_REPARSE_POINT`, so the reparse point itself is opened and
/// never its target; the handle is then refused when it names a reparse
/// point, which covers an NTFS junction as well as a symbolic link. Neither
/// platform stats the path first, because a pre-check is a
/// time-of-check-to-time-of-use bug rather than a defence.
///
/// # Errors
///
/// Returns the underlying I/O error. A refusal of the path because it is a
/// link answers [`is_no_follow_refusal`].
pub fn open_no_follow(path: &Path) -> io::Result<File> {
    open_no_follow_inner(path, false)
}

/// Open a file for reading without following a link and without waiting.
///
/// It is [`open_no_follow`] with the platform's non-blocking flag added, and
/// every other rule of that function holds unchanged. The flag is what keeps
/// the open itself bounded where a directory holds untrusted entries: a named
/// pipe with no writer would otherwise hold the open call open forever. It
/// changes nothing for a regular file, which is the only thing a caller here
/// goes on to read. Only Unix has such a flag; every other platform opens
/// exactly as [`open_no_follow`] does, because there is nothing to add.
///
/// # Errors
///
/// Returns the underlying I/O error, exactly as [`open_no_follow`] does.
pub fn open_no_follow_nonblocking(path: &Path) -> io::Result<File> {
    open_no_follow_inner(path, true)
}

/// The one no-follow open both variants share, so that the rule has one
/// implementation per platform rather than one per caller.
fn open_no_follow_inner(path: &Path, nonblocking: bool) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let flags = if nonblocking {
            libc::O_NOFOLLOW | libc::O_NONBLOCK
        } else {
            libc::O_NOFOLLOW
        };
        OpenOptions::new().read(true).custom_flags(flags).open(path)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
        // There is no non-blocking open to add here, so the open is the
        // no-follow one unchanged.
        let _ = nonblocking;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        let attributes = file.metadata()?.file_attributes();
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is a reparse point",
            ));
        }
        if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            // The backup flag above is what let the directory open at all; it
            // is there for the junction case, and a directory is still not a
            // file this archive reads.
            return Err(io::Error::from(io::ErrorKind::IsADirectory));
        }
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = nonblocking;
        if is_symlink(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is a symbolic link",
            ));
        }
        OpenOptions::new().read(true).open(path)
    }
}

/// Whether an open refused the path because it is a link rather than for any
/// other reason, so that the caller reports `path.symlink` without a second
/// look at the path.
#[must_use]
pub fn is_no_follow_refusal(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ELOOP)
    }
    #[cfg(not(unix))]
    {
        error.kind() == io::ErrorKind::InvalidInput
    }
}

/// Whether the path itself is a link, without following it.
///
/// On Windows every reparse point counts, so an NTFS junction is a link here
/// even though the standard library's own symbolic-link test does not report
/// one. A reparse point of any other tag is refused too: the archive holds no
/// reparse point of its own, so refusing one it did not create is correct.
#[must_use]
pub fn is_symlink(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        fs::symlink_metadata(path).is_ok_and(|metadata| {
            metadata.file_type().is_symlink()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        })
    }
    #[cfg(not(windows))]
    {
        fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    }
}

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

/// The warning reported where the no-follow rule is enforced after the open.
///
/// It is the degradation left after the platform's no-follow flag is used:
/// the reparse point itself is opened and the handle is then refused, rather
/// than the open failing, and the reparse tag is not distinguished, so a
/// junction is reported as `path.symlink` like a symbolic link.
#[must_use]
pub fn no_follow_after_open_warning() -> Warning {
    Diagnostic::new(
        codes::PLATFORM_NO_FOLLOW_AFTER_OPEN,
        "A no-follow open refuses the link it opened rather than failing at the system call.",
        Details::new(),
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

/// Map an I/O error raised by the hard-link publish step into the contract.
///
/// A filesystem that cannot create a hard link at all, FAT32 and exFAT among
/// them, cannot host an archive whose publish step is a hard link. That is
/// reported as `platform.filesystem_unsupported`, because retrying never
/// succeeds and `write.interrupted` invites the caller to retry. Every other
/// failure of the same call keeps the mapping of [`publish_refusal`].
#[must_use]
pub fn link_refusal(error: &io::Error, archive_path: &str, stage: &'static str) -> Diagnostic {
    if is_link_unsupported(error) {
        return Diagnostic::new(
            codes::PLATFORM_FILESYSTEM_UNSUPPORTED,
            "The filesystem cannot create the hard link the archive's write procedure needs.",
            Details::new()
                .text("capability", "hard_link")
                .text("stage", stage),
        );
    }
    publish_refusal(error, archive_path, stage)
}

/// Whether an I/O error says the filesystem has no hard links.
///
/// The mapped codes are: any error the standard library classifies as
/// `Unsupported`; on Unix `EPERM`, which `link(2)` documents as "the
/// filesystem containing oldpath and newpath does not support the creation of
/// hard links", and `EOPNOTSUPP`; on Windows `ERROR_INVALID_FUNCTION` (1) and
/// `ERROR_NOT_SUPPORTED` (50), which a FAT32 or exFAT volume reports for
/// `CreateHardLinkW`. `EPERM` also covers a hardened kernel refusing a link to
/// a file the caller does not own, which cannot arise here: the source is the
/// staging file openPapir just created and owns.
#[must_use]
pub fn is_link_unsupported(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(error.raw_os_error(), Some(libc::EPERM | libc::EOPNOTSUPP))
    }
    #[cfg(not(unix))]
    {
        matches!(error.raw_os_error(), Some(1 | 50))
    }
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
    fn a_filesystem_without_hard_links_is_a_platform_refusal_not_a_retry() {
        #[cfg(unix)]
        let unsupported = io::Error::from_raw_os_error(libc::EPERM);
        #[cfg(not(unix))]
        let unsupported = io::Error::from_raw_os_error(50);
        assert!(is_link_unsupported(&unsupported));
        assert!(is_link_unsupported(&io::Error::from(
            io::ErrorKind::Unsupported
        )));
        let refusal = link_refusal(&unsupported, "objects/sha256/ab/cd/digest", "object");
        assert_eq!(refusal.code, codes::PLATFORM_FILESYSTEM_UNSUPPORTED);
        assert_eq!(refusal.exit_code(), 5);
        assert!(
            !refusal.is_retryable(),
            "a filesystem without hard links never succeeds on a retry"
        );
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["capability"], "hard_link");
        assert_eq!(json["details"]["stage"], "object");
        assert_eq!(json["details"]["bucket"], "platform");

        // Every other failure of the same call keeps its own mapping.
        let existing = io::Error::new(io::ErrorKind::AlreadyExists, "exists");
        assert!(!is_link_unsupported(&existing));
        assert_eq!(
            link_refusal(&existing, "records/imports/id.json", "record").code,
            codes::PATH_OVERWRITE
        );
        let denied = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(
            link_refusal(&denied, "records/imports/id.json", "record").code,
            codes::WRITE_INTERRUPTED
        );
    }

    #[test]
    fn a_link_refused_by_the_open_is_told_apart_from_any_other_failure() {
        #[cfg(unix)]
        let link = io::Error::from_raw_os_error(libc::ELOOP);
        #[cfg(not(unix))]
        let link = io::Error::new(io::ErrorKind::InvalidInput, "path is a reparse point");
        assert!(is_no_follow_refusal(&link));
        assert!(!is_no_follow_refusal(&io::Error::from(
            io::ErrorKind::NotFound
        )));
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("note.txt");
        fs::write(&file, b"synthetic").unwrap();
        assert!(open_no_follow(&file).is_ok(), "a regular file opens");
        let absent = open_no_follow(&directory.path().join("absent.txt")).unwrap_err();
        assert!(!is_no_follow_refusal(&absent));
        #[cfg(unix)]
        {
            let linked = directory.path().join("linked.txt");
            std::os::unix::fs::symlink(&file, &linked).unwrap();
            let refused = open_no_follow(&linked).unwrap_err();
            assert!(is_no_follow_refusal(&refused), "the open refused the link");
            assert!(is_symlink(&linked));
            assert!(!is_symlink(&file));
        }
    }

    #[test]
    fn the_non_blocking_variant_is_the_same_open_with_the_same_no_follow_rule() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("note.txt");
        fs::write(&file, b"synthetic").unwrap();
        assert!(
            open_no_follow_nonblocking(&file).is_ok(),
            "a regular file opens exactly as it does through the blocking open"
        );
        let absent = open_no_follow_nonblocking(&directory.path().join("absent.txt")).unwrap_err();
        assert_eq!(absent.kind(), io::ErrorKind::NotFound);
        assert!(!is_no_follow_refusal(&absent));
        #[cfg(unix)]
        {
            let linked = directory.path().join("linked.txt");
            std::os::unix::fs::symlink(&file, &linked).unwrap();
            let refused = open_no_follow_nonblocking(&linked).unwrap_err();
            assert!(
                is_no_follow_refusal(&refused),
                "the link is refused by the open itself, non-blocking or not"
            );

            // The flag the variant adds is the one that keeps the open
            // bounded: a pipe with no writer would hold a blocking open open
            // forever, so only the non-blocking one is asked to open it.
            let pipe = directory.path().join("pipe");
            let made = std::process::Command::new("mkfifo")
                .arg(&pipe)
                .status()
                .is_ok_and(|status| status.success());
            if made {
                let opened = open_no_follow_nonblocking(&pipe).expect("the open returns at once");
                assert!(
                    !opened.metadata().unwrap().is_file(),
                    "the handle is refused on its kind rather than read"
                );
            }
        }
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
        let residual = no_follow_after_open_warning();
        assert_eq!(residual.code, codes::PLATFORM_NO_FOLLOW_AFTER_OPEN);
        assert_eq!(residual.bucket(), crate::error::Bucket::Platform);
        assert_eq!(
            symlink_refusal(Details::new().text("scope", "archive")).code,
            codes::PATH_SYMLINK
        );
    }
}
