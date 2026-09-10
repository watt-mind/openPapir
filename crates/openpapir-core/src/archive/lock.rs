//! The single-writer advisory lock.
//!
//! One `lock` file in the archive root admits one writer at a time. A second
//! writer refuses rather than waiting indefinitely (`docs/archive-layout.md`).
//! The file records the holder's process identifier, host, and start time, and
//! none of that is ever echoed: a hostname is environment data the privacy
//! rule forbids in output.
//!
//! Taking over a lock whose holder is provably gone is deliberately not
//! implemented. A stale lock is never broken silently and never on a timeout,
//! so this module reports `lock.held` and stops; `lock.stale` stays reserved.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::archive::paths;
use crate::clock;
use crate::error::{Details, Diagnostic, codes, stages};

/// The lock file's name inside the archive root.
pub const LOCK_FILE: &str = "lock";

/// A held writer lock, released when it is dropped.
#[derive(Debug)]
pub struct WriterLock {
    path: PathBuf,
}

impl WriterLock {
    /// Take the writer lock, refusing when another writer holds it.
    ///
    /// # Errors
    ///
    /// Returns `lock.held` when the lock file already exists, `path.symlink`
    /// when it is a symbolic link, `archive.not_writable` when the archive
    /// root refuses to be written to at all, and `write.interrupted` when it
    /// cannot be created for any other reason.
    pub fn acquire(root: &Path) -> Result<Self, Diagnostic> {
        let path = root.join(LOCK_FILE);
        if paths::is_symlink(&path) {
            return Err(paths::symlink_refusal(
                Details::new()
                    .text("scope", "archive")
                    .text("archive_path", LOCK_FILE),
            ));
        }
        match paths::create_file_owner_only(&path) {
            Ok(mut file) => {
                let _ = file.write_all(holder_document().as_bytes());
                let _ = file.sync_all();
                Ok(Self { path })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(Diagnostic::new(
                    codes::LOCK_HELD,
                    "Another writer holds the archive's single-writer lock.",
                    Details::new(),
                )
                .retryable())
            }
            Err(error) if is_not_writable(&error) => Err(not_writable_refusal()),
            Err(error) => Err(paths::publish_refusal(
                &error,
                LOCK_FILE,
                stages::RECORD_WRITE,
            )),
        }
    }
}

/// Whether creating the lock file failed because the directory holding it
/// refuses to be written to.
///
/// The lock file is created directly in the archive root, so a denial here is
/// a denial of the root itself: the operating system refused the creation
/// rather than interrupting it, and no retry of the same command can succeed.
/// Both kinds are mapped, because a read-only mount and a directory whose
/// permissions withhold write access are the same condition for a caller.
fn is_not_writable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
    )
}

/// The refusal reported where the archive root cannot be written to.
///
/// It is deliberately not retryable and deliberately not `write.interrupted`:
/// nothing was interrupted, nothing was left behind, and the archive is
/// exactly as it was. The message names the directory that has to change and
/// says what the repair cannot do, because `archive repair-permissions` takes
/// the same lock and fails the same way.
fn not_writable_refusal() -> Diagnostic {
    Diagnostic::new(
        codes::ARCHIVE_NOT_WRITABLE,
        "The archive root directory is not writable, so openPapir cannot take \
         the single-writer lock; make the archive root itself writable, \
         because archive repair-permissions cannot help when the root is \
         read-only.",
        Details::new()
            .text("scope", "archive")
            .text("archive_path", "."),
    )
}

impl Drop for WriterLock {
    /// Release the lock on normal exit. A leftover lock is the documented
    /// deferred case and is never taken over automatically.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// The lock file's content: process identifier, host, and start time.
fn holder_document() -> String {
    format!(
        "{{\"host\":{},\"pid\":{},\"started_at\":{}}}\n",
        json_string(&host_name()),
        std::process::id(),
        json_string(&clock::now_rfc3339())
    )
}

/// Escape a string for the small JSON document written above.
fn json_string(value: &str) -> String {
    let escaped: String = value
        .chars()
        .filter(|character| !character.is_control())
        .flat_map(|character| match character {
            '"' | '\\' => vec!['\\', character],
            other => vec![other],
        })
        .collect();
    format!("\"{escaped}\"")
}

/// A best-effort host name, recorded in the lock file and never reported.
fn host_name() -> String {
    #[cfg(unix)]
    let from_file = fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| fs::read_to_string("/etc/hostname"))
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    #[cfg(not(unix))]
    let from_file: Option<String> = None;

    from_file
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_writer_refuses_and_the_first_release_frees_the_lock() {
        let root = tempfile::tempdir().unwrap();
        let held = WriterLock::acquire(root.path()).unwrap();
        let refusal = WriterLock::acquire(root.path()).unwrap_err();
        assert_eq!(refusal.code, codes::LOCK_HELD);
        assert_eq!(refusal.exit_code(), 4);
        assert!(refusal.is_retryable());
        assert!(
            serde_json::to_value(&refusal).unwrap()["details"]
                .as_object()
                .unwrap()
                .len()
                == 1,
            "the holder's identity is never echoed"
        );
        drop(held);
        assert!(!root.path().join(LOCK_FILE).exists());
        let _second = WriterLock::acquire(root.path()).unwrap();
    }

    #[test]
    fn the_lock_file_records_its_holder_without_reporting_it() {
        let root = tempfile::tempdir().unwrap();
        let _held = WriterLock::acquire(root.path()).unwrap();
        let document: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.path().join(LOCK_FILE)).unwrap())
                .unwrap();
        assert_eq!(document["pid"], std::process::id());
        assert!(document["host"].is_string());
        assert!(document["started_at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    /// A root that refuses the lock file names the cause and does not invite
    /// a retry that cannot succeed. Unix-only, because a permission bit is
    /// the only portable way to withhold write access from a directory here;
    /// on Windows the read-only attribute of a directory does not stop a file
    /// from being created in it.
    #[test]
    #[cfg(unix)]
    fn a_root_that_cannot_be_written_to_names_the_cause_rather_than_a_retry() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let refused = WriterLock::acquire(root.path());
        let restored = fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700));
        let Err(refusal) = refused else {
            // The process writes anyway, which happens when the tests run
            // with privileges that ignore the permission bits.
            restored.unwrap();
            return;
        };
        restored.unwrap();
        assert_eq!(refusal.code, codes::ARCHIVE_NOT_WRITABLE);
        assert_eq!(refusal.exit_code(), 4);
        assert!(
            !refusal.is_retryable(),
            "no retry succeeds until the directory changes"
        );
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["bucket"], "archive");
        assert_eq!(json["details"]["archive_path"], ".");
        assert_eq!(json["details"]["scope"], "archive");
        assert!(
            json["details"].get("stage").is_none(),
            "nothing was being written, so no stage names one"
        );
        assert!(!root.path().join(LOCK_FILE).exists());
    }

    /// Every other failure of the same creation is still an interrupted
    /// write, and it names a stage the contract enumerates.
    #[test]
    fn a_failure_that_is_not_a_refusal_to_write_keeps_the_interrupted_write() {
        assert!(is_not_writable(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        assert!(is_not_writable(&io::Error::from(
            io::ErrorKind::ReadOnlyFilesystem
        )));
        assert!(!is_not_writable(&io::Error::from(io::ErrorKind::NotFound)));
        let refusal = paths::publish_refusal(
            &io::Error::from(io::ErrorKind::NotFound),
            LOCK_FILE,
            stages::RECORD_WRITE,
        );
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["stage"],
            "record_write"
        );
    }
}
