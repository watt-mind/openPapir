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
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::archive::paths;
use crate::clock;
use crate::error::{Details, Diagnostic, codes};

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
    /// when it is a symbolic link, and `write.interrupted` when it cannot be
    /// created for any other reason.
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
            Err(error) => Err(paths::publish_refusal(&error, LOCK_FILE, "lock")),
        }
    }
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
}
