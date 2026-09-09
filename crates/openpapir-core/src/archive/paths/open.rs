//! Opening a path without following a link, and the refusals a link raises.
//!
//! No path inside the archive root may be a symbolic link, and the rule is
//! enforced with the platform's no-follow flag when the path is opened rather
//! than by a stat call beforehand, because a pre-check is a
//! time-of-check-to-time-of-use bug rather than a defence
//! (`docs/archive-layout.md`).

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

/// The error a Windows no-follow open raises for the link it opened.
///
/// It is a type rather than a message, so [`is_no_follow_refusal`] recognises
/// exactly the refusal this module raised and never another caller's
/// `InvalidInput`.
#[cfg(windows)]
#[derive(Debug)]
struct ReparsePointRefusal;

#[cfg(windows)]
impl std::fmt::Display for ReparsePointRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("path is a reparse point")
    }
}

#[cfg(windows)]
impl std::error::Error for ReparsePointRefusal {}

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
///
/// The handle it returns still carries the non-blocking flag, which the open
/// does not clear, so a read from it can return `WouldBlock` where the path
/// was not a regular file after all. The caller's obligation is therefore to
/// check the handle's kind and read only a regular file, which is what every
/// caller in this crate does: it is the same check that refuses a named pipe
/// or a device planted in a record directory.
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
                ReparsePointRefusal,
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
}

/// Whether an open refused the path because it is a link rather than for any
/// other reason, so that the caller reports `path.symlink` without a second
/// look at the path.
///
/// Linux and macOS fail an `O_NOFOLLOW` open of a symbolic link with `ELOOP`.
/// FreeBSD and DragonFly report the same refusal as `EMLINK`, which is
/// accepted there as well; neither is a target this project builds for today.
/// On Windows the open itself succeeds and the handle is refused, so the
/// refusal is the sentinel this module raised and never another caller's
/// `InvalidInput`.
#[must_use]
pub fn is_no_follow_refusal(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        let reported = error.raw_os_error();
        reported == Some(libc::ELOOP)
            || (cfg!(any(target_os = "freebsd", target_os = "dragonfly"))
                && reported == Some(libc::EMLINK))
    }
    #[cfg(not(unix))]
    {
        error
            .get_ref()
            .is_some_and(|inner| inner.is::<ReparsePointRefusal>())
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

/// The refusal reported for a symbolic link that must not be one.
#[must_use]
pub fn symlink_refusal(details: Details) -> Diagnostic {
    Diagnostic::new(
        codes::PATH_SYMLINK,
        "A path that must not be a symbolic link is one.",
        details,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_refused_by_the_open_is_told_apart_from_any_other_failure() {
        #[cfg(unix)]
        let link = io::Error::from_raw_os_error(libc::ELOOP);
        #[cfg(not(unix))]
        let link = io::Error::new(io::ErrorKind::InvalidInput, ReparsePointRefusal);
        assert!(is_no_follow_refusal(&link));
        assert!(!is_no_follow_refusal(&io::Error::from(
            io::ErrorKind::NotFound
        )));
        // Only this module's own refusal counts, never another caller's
        // error of the same kind.
        assert!(!is_no_follow_refusal(&io::Error::new(
            io::ErrorKind::InvalidInput,
            "some other caller's malformed input"
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
            } else {
                // Saying so keeps a missing `mkfifo` from reading as a pass.
                eprintln!("skipped the named-pipe case: this system has no usable mkfifo command");
            }
        }
    }

    /// A Windows no-follow open refuses the reparse point it opened, so the
    /// junction the standard library does not call a symbolic link is a link
    /// here and is refused exactly as one.
    #[test]
    #[cfg(windows)]
    fn a_windows_reparse_point_is_a_link_the_open_refuses() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("note.txt");
        fs::write(&file, b"synthetic").unwrap();
        let linked = directory.path().join("linked.txt");
        if std::os::windows::fs::symlink_file(&file, &linked).is_err() {
            // Creating a symbolic link on Windows needs a privilege the test
            // environment does not always grant. Saying so keeps a missing
            // privilege from reading as a pass.
            eprintln!(
                "skipped the reparse-point case: this process may not create a Windows symbolic link"
            );
            return;
        }
        assert!(is_symlink(&linked), "the planted link is one");
        assert!(!is_symlink(&file), "its target is not");
        let refused = open_no_follow(&linked).unwrap_err();
        assert!(
            is_no_follow_refusal(&refused),
            "the handle is refused because it names a reparse point"
        );
    }

    #[test]
    fn the_link_degradation_and_the_link_refusal_name_their_own_codes() {
        let residual = no_follow_after_open_warning();
        assert_eq!(residual.code, codes::PLATFORM_NO_FOLLOW_AFTER_OPEN);
        assert_eq!(residual.bucket(), crate::error::Bucket::Platform);
        assert_eq!(
            symlink_refusal(Details::new().text("scope", "archive")).code,
            codes::PATH_SYMLINK
        );
    }
}
