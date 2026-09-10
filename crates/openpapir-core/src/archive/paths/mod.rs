//! Path safety, owner-only permissions, and durability primitives.
//!
//! Every rule here comes from `docs/archive-layout.md`. No path inside the
//! archive root may be a symbolic link, and the rule is enforced with the
//! platform's no-follow flag when the path is opened rather than by a stat
//! call beforehand, because a pre-check is a time-of-check-to-time-of-use bug
//! rather than a defence. Directories are created owner-only, files are
//! created owner-read-write, and a stored object becomes owner-read-only.
//!
//! The rules live in two private submodules, `open` for the no-follow open
//! and the link refusals, and `permissions` for the owner-only modes, the
//! read-only attribute, and the directory flush. Both are re-exported here,
//! so every caller keeps naming one flat `paths::name`. What stays in this
//! file is the mapping from an I/O error raised by a write's publish step to
//! the contract's own codes, which belongs to neither half.

mod open;
mod permissions;

pub use open::{
    is_no_follow_refusal, is_symlink, no_follow_after_open_warning, open_no_follow,
    open_no_follow_nonblocking, symlink_refusal,
};
#[cfg(not(unix))]
pub(crate) use permissions::clear_read_only;
pub use permissions::{
    create_dir_owner_only, create_file_owner_only, is_wider_than_owner_only, narrow_to_owner_only,
    no_directory_fsync_warning, owner_only_via_acl_warning, refuse_if_wide, set_object_read_only,
    sync_directory, wide_permissions_refusal,
};

use std::io;
use std::path::Path;

use crate::error::{Details, Diagnostic, codes, stages};

/// Every no-follow rule below is a Unix or a Windows one. A target with
/// neither has no way to open a path without following a link, and this
/// archive has no weaker mode to fall back to, so it refuses to build rather
/// than build something that only looks safe.
#[cfg(not(any(unix, windows)))]
compile_error!(
    "openPapir supports Unix and Windows targets only: no other target has a no-follow open."
);

/// The device a path lives on, where the platform reports one.
#[must_use]
pub fn device_of(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::symlink_metadata(path)
            .ok()
            .map(|metadata| metadata.dev())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// Map an I/O error raised while publishing a file into the contract.
///
/// `AlreadyExists` is a refusal to replace a file openPapir did not create; a
/// cross-device link or rename means the archive is misconfigured; anything
/// else interrupted the write before its publish step, so the archive holds
/// the complete artefact or nothing.
///
/// # Panics
///
/// In a debug build, when `stage` is not one of the stages the contract
/// enumerates ([`stages::ALL`]). A stage outside that set is a defect in the
/// caller rather than a condition of the archive, so it is caught where every
/// test runs rather than published to a caller that cannot branch on it.
#[must_use]
pub fn publish_refusal(error: &io::Error, archive_path: &str, stage: &'static str) -> Diagnostic {
    debug_assert!(
        stages::is_write(stage),
        "a write stage the error contract does not enumerate"
    );
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
/// A filesystem that says it cannot create a hard link at all cannot host an
/// archive whose publish step is a hard link. That is reported as
/// `platform.filesystem_unsupported`, because retrying never succeeds and
/// `write.interrupted` invites the caller to retry.
///
/// A link the system merely refused is reported as `write.interrupted`
/// instead, naming the condition it observed rather than a cause it cannot
/// prove; see [`is_link_refused`]. Every other failure of the same call keeps
/// the mapping of [`publish_refusal`].
///
/// # Panics
///
/// In a debug build, when `stage` is not one the contract enumerates; see
/// [`publish_refusal`].
#[must_use]
pub fn link_refusal(error: &io::Error, archive_path: &str, stage: &'static str) -> Diagnostic {
    debug_assert!(
        stages::is_write(stage),
        "a write stage the error contract does not enumerate"
    );
    if is_link_unsupported(error) {
        return Diagnostic::new(
            codes::PLATFORM_FILESYSTEM_UNSUPPORTED,
            "The filesystem cannot create the hard link the archive's write procedure needs.",
            Details::new()
                .text("capability", "hard_link")
                .text("stage", stage),
        );
    }
    if is_link_refused(error) {
        return Diagnostic::new(
            codes::WRITE_INTERRUPTED,
            "The filesystem refused to create the hard link the archive's write procedure needs.",
            Details::new()
                .text("capability", "hard_link")
                .text("condition", "link_refused")
                .text("stage", stage),
        )
        .retryable();
    }
    publish_refusal(error, archive_path, stage)
}

/// Whether an I/O error says the filesystem has no hard links.
///
/// The mapped codes are: any error the standard library classifies as
/// `Unsupported`; on Unix `EOPNOTSUPP`; on Windows `ERROR_INVALID_FUNCTION`
/// (1) and `ERROR_NOT_SUPPORTED` (50), which a FAT32 or exFAT volume reports
/// for `CreateHardLinkW`. Each of them states that the operation is not
/// supported, so no retry can succeed.
///
/// Unix `EPERM` is deliberately not one of them: see [`is_link_refused`].
#[must_use]
pub fn is_link_unsupported(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(error.raw_os_error(), Some(libc::EOPNOTSUPP))
    }
    #[cfg(not(unix))]
    {
        matches!(error.raw_os_error(), Some(1 | 50))
    }
}

/// Whether an I/O error is a refusal of the link whose cause is ambiguous.
///
/// `link(2)` returns `EPERM` for a filesystem that does not support hard
/// links, which a FAT32 or exFAT volume mounted on Linux does, and equally
/// for a source or destination carrying the immutable or append-only
/// attribute, and for a kernel hardened against linking a file the caller
/// does not own. Telling them apart needs a `statfs` or an attribute `ioctl`,
/// neither of which is reachable without `unsafe`, which this workspace
/// forbids. openPapir therefore reports what it observed, that the filesystem
/// refused the link, and names the condition in `details.condition` so the
/// operator knows to check the filesystem type and the file attributes rather
/// than reading a cause openPapir cannot prove.
#[must_use]
pub fn is_link_refused(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = error;
        false
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
        let refusal = publish_refusal(&error, "objects/sha256/ab/cd/digest", stages::OBJECT_WRITE);
        assert_eq!(refusal.code, codes::PATH_CROSS_DEVICE);
        assert_eq!(refusal.exit_code(), 3);
        assert!(is_cross_device(&error));
    }

    #[test]
    fn an_existing_destination_is_refused_as_an_overwrite() {
        let error = io::Error::new(io::ErrorKind::AlreadyExists, "exists");
        let refusal = publish_refusal(&error, "records/imports/id.json", stages::RECORD_WRITE);
        assert_eq!(refusal.code, codes::PATH_OVERWRITE);
        assert_eq!(refusal.exit_code(), 3);
    }

    #[test]
    fn any_other_publish_failure_is_an_interrupted_write() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let refusal = publish_refusal(&error, "objects/sha256/ab/cd/digest", stages::OBJECT_WRITE);
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert!(refusal.is_retryable());
        assert_eq!(refusal.exit_code(), 4);
        assert!(!is_cross_device(&error));
    }

    #[test]
    fn a_filesystem_without_hard_links_is_a_platform_refusal_not_a_retry() {
        #[cfg(unix)]
        let unsupported = io::Error::from_raw_os_error(libc::EOPNOTSUPP);
        #[cfg(not(unix))]
        let unsupported = io::Error::from_raw_os_error(50);
        assert!(is_link_unsupported(&unsupported));
        assert!(is_link_unsupported(&io::Error::from(
            io::ErrorKind::Unsupported
        )));
        let refusal = link_refusal(
            &unsupported,
            "objects/sha256/ab/cd/digest",
            stages::OBJECT_WRITE,
        );
        assert_eq!(refusal.code, codes::PLATFORM_FILESYSTEM_UNSUPPORTED);
        assert_eq!(refusal.exit_code(), 5);
        assert!(
            !refusal.is_retryable(),
            "a filesystem without hard links never succeeds on a retry"
        );
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["capability"], "hard_link");
        assert_eq!(json["details"]["stage"], "object_write");
        assert_eq!(json["details"]["bucket"], "platform");

        // Every other failure of the same call keeps its own mapping.
        let existing = io::Error::new(io::ErrorKind::AlreadyExists, "exists");
        assert!(!is_link_unsupported(&existing));
        assert_eq!(
            link_refusal(&existing, "records/imports/id.json", stages::RECORD_WRITE).code,
            codes::PATH_OVERWRITE
        );
        let denied = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(
            link_refusal(&denied, "records/imports/id.json", stages::RECORD_WRITE).code,
            codes::WRITE_INTERRUPTED
        );
    }

    /// `EPERM` is the ambiguous one: a filesystem without hard links reports
    /// it, and so does an immutable or append-only file. The refusal states
    /// the condition it observed rather than claiming the filesystem is
    /// unsupported.
    #[test]
    #[cfg(unix)]
    fn a_link_the_system_refused_states_the_condition_rather_than_a_cause() {
        let refused = io::Error::from_raw_os_error(libc::EPERM);
        assert!(
            !is_link_unsupported(&refused),
            "EPERM does not prove the filesystem has no hard links"
        );
        assert!(is_link_refused(&refused));
        let refusal = link_refusal(
            &refused,
            "objects/sha256/ab/cd/digest",
            stages::OBJECT_WRITE,
        );
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert_eq!(refusal.exit_code(), 4);
        assert!(
            refusal.is_retryable(),
            "the code carries its own retry hint"
        );
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["capability"], "hard_link");
        assert_eq!(json["details"]["condition"], "link_refused");
        assert_eq!(json["details"]["stage"], "object_write");
        assert_eq!(json["details"]["bucket"], "write");

        // Nothing else reaches the ambiguous branch, so an unsupported
        // filesystem and an overwrite keep the codes they had.
        for other in [
            io::Error::from_raw_os_error(libc::EOPNOTSUPP),
            io::Error::new(io::ErrorKind::AlreadyExists, "exists"),
            io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        ] {
            assert!(!is_link_refused(&other));
        }
    }
}
