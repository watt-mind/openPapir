//! The error, warning, and exit-code contract of `docs/error-contract.md`.
//!
//! A [`Diagnostic`] carries a stable `code`, the bucket its prefix names, one
//! human sentence, and bounded `details`. The same shape serves as an error
//! and as a warning, exactly as the contract specifies. Nothing here ever
//! holds a user-supplied path, an original filename, or payload bytes: the
//! privacy rule binds `message` and `details` as strictly as it binds `data`.

use std::collections::BTreeMap;

use serde::Serialize;

/// The largest number of keys a `details` object may carry.
pub const MAX_DETAIL_KEYS: usize = 16;
/// The largest number of scalars an array inside `details` may carry.
pub const MAX_DETAIL_LIST: usize = 16;

/// Stable error-code constants, one per implemented condition.
pub mod codes {
    /// The command line is malformed or a value is unusable.
    pub const USAGE_ARGUMENTS: &str = "usage.arguments";
    /// No archive root was supplied, or the supplied path does not exist.
    pub const USAGE_ARCHIVE_ROOT_MISSING: &str = "usage.archive_root_missing";

    /// The root exists but holds no archive marker.
    pub const ARCHIVE_MARKER_MISSING: &str = "archive.marker_missing";
    /// The marker exists but cannot be read as a valid marker.
    pub const ARCHIVE_MARKER_MALFORMED: &str = "archive.marker_malformed";
    /// Initialisation was asked for on a non-empty directory with no marker.
    pub const ARCHIVE_ADOPT_REFUSED: &str = "archive.adopt_refused";
    /// The marker's schema version is newer than this build supports.
    pub const ARCHIVE_SCHEMA_NEWER: &str = "archive.schema_newer";
    /// The archive predates this build and needs an explicit migration.
    pub const ARCHIVE_SCHEMA_OLDER: &str = "archive.schema_older";
    /// Permissions on the archive are wider than owner-only.
    pub const ARCHIVE_PERMISSIONS_WIDE: &str = "archive.permissions_wide";
    /// The archive root spans more than one filesystem.
    pub const ARCHIVE_MULTIPLE_FILESYSTEMS: &str = "archive.multiple_filesystems";

    /// One file exceeds the single-file size cap.
    pub const INPUT_CAP_FILE_SIZE: &str = "input.cap.file_size";
    /// The import's total bytes exceed the per-operation cap.
    pub const INPUT_CAP_IMPORT_BYTES: &str = "input.cap.import_bytes";
    /// The import names more files than the per-operation cap.
    pub const INPUT_CAP_IMPORT_FILES: &str = "input.cap.import_files";
    /// A record document would exceed the record cap.
    pub const INPUT_CAP_RECORD_SIZE: &str = "input.cap.record_size";
    /// A supplied original filename exceeds the attribute cap.
    pub const INPUT_CAP_FILENAME_LENGTH: &str = "input.cap.filename_length";
    /// A user-supplied record field exceeds its own length cap.
    pub const INPUT_CAP_FIELD_LENGTH: &str = "input.cap.field_length";

    /// A path that must not be a symbolic link is one.
    pub const PATH_SYMLINK: &str = "path.symlink";
    /// A write would replace a file openPapir did not create.
    pub const PATH_OVERWRITE: &str = "path.overwrite";
    /// A rename would cross a device boundary.
    pub const PATH_CROSS_DEVICE: &str = "path.cross_device";

    /// Another writer holds the advisory lock.
    pub const LOCK_HELD: &str = "lock.held";

    /// A write was interrupted before its publish step.
    pub const WRITE_INTERRUPTED: &str = "write.interrupted";

    /// A reference names no record or object in this archive.
    pub const RECORD_NOT_FOUND: &str = "record.not_found";
    /// A stored record document cannot be read as a valid record.
    pub const RECORD_MALFORMED: &str = "record.malformed";
    /// A record's fields break a consistency rule of the archive design.
    pub const RECORD_INCONSISTENT: &str = "record.inconsistent";

    /// A stored object's bytes no longer digest to its own path.
    pub const INTEGRITY_DIGEST_MISMATCH: &str = "integrity.digest_mismatch";
    /// A stored object's length differs from the length recorded for it.
    pub const INTEGRITY_LENGTH_MISMATCH: &str = "integrity.length_mismatch";
    /// A record names a record or object this archive does not hold.
    pub const INTEGRITY_DANGLING_REFERENCE: &str = "integrity.dangling_reference";
    /// A stored object is referenced by no record in this archive.
    pub const INTEGRITY_ORPHAN_OBJECT: &str = "integrity.orphan_object";

    /// The filesystem cannot express owner-only access.
    pub const PLATFORM_FILESYSTEM_UNSUPPORTED: &str = "platform.filesystem_unsupported";
    /// The directory entry a rename created may not be durable.
    pub const PLATFORM_NO_DIRECTORY_FSYNC: &str = "platform.no_directory_fsync";
    /// Owner-only access is expressed as an access-control list.
    pub const PLATFORM_OWNER_ONLY_VIA_ACL: &str = "platform.owner_only_via_acl";

    /// An invariant of the design was violated.
    pub const INTERNAL_UNEXPECTED: &str = "internal.unexpected";
}

/// The bucket a code belongs to; it decides the process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    /// The invocation itself.
    Usage,
    /// A bound refused before allocation.
    Input,
    /// A path-safety refusal.
    Path,
    /// The state of the archive itself.
    Archive,
    /// The single-writer lock.
    Lock,
    /// An interrupted or incomplete write.
    Write,
    /// A record document.
    Record,
    /// Stored bytes disagree with what is recorded.
    Integrity,
    /// Writing outside the archive.
    Export,
    /// Deletion and purge.
    Delete,
    /// The environment cannot provide a guarantee.
    Platform,
    /// A bug in openPapir.
    Internal,
}

impl Bucket {
    /// The bucket's wire name, as it appears in `details.bucket`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Input => "input",
            Self::Path => "path",
            Self::Archive => "archive",
            Self::Lock => "lock",
            Self::Write => "write",
            Self::Record => "record",
            Self::Integrity => "integrity",
            Self::Export => "export",
            Self::Delete => "delete",
            Self::Platform => "platform",
            Self::Internal => "internal",
        }
    }

    /// The process exit code for this bucket. `1` is never emitted.
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Usage => 2,
            Self::Input | Self::Path => 3,
            Self::Archive
            | Self::Lock
            | Self::Write
            | Self::Record
            | Self::Integrity
            | Self::Export
            | Self::Delete => 4,
            Self::Platform => 5,
            Self::Internal => 6,
        }
    }

    /// The bucket a code's prefix names. An unknown prefix is a bug, so it
    /// reports as [`Bucket::Internal`] rather than inventing a bucket.
    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code.split('.').next().unwrap_or_default() {
            "usage" => Self::Usage,
            "input" => Self::Input,
            "path" => Self::Path,
            "archive" => Self::Archive,
            "lock" => Self::Lock,
            "write" => Self::Write,
            "record" => Self::Record,
            "integrity" => Self::Integrity,
            "export" => Self::Export,
            "delete" => Self::Delete,
            "platform" => Self::Platform,
            _ => Self::Internal,
        }
    }
}

/// A scalar or a bounded array of scalars; `details` nests no further.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum DetailValue {
    /// A count, a byte length, a cap, or an index.
    Int(u64),
    /// A flag.
    Bool(bool),
    /// A bucket name, a stage name, or an archive-relative path.
    Text(String),
    /// A bounded array of the strings above.
    List(Vec<String>),
}

/// A bounded, privacy-constrained `details` object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Details(BTreeMap<&'static str, DetailValue>);

impl Details {
    /// An empty `details` object.
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Insert a value, silently ignoring anything past [`MAX_DETAIL_KEYS`].
    ///
    /// One slot is reserved for `bucket`, which every diagnostic carries, so
    /// the finished object never exceeds the contract's bound.
    fn insert(mut self, key: &'static str, value: DetailValue) -> Self {
        if self.0.len() < MAX_DETAIL_KEYS - 1 || self.0.contains_key(key) {
            self.0.insert(key, value);
        }
        self
    }

    /// Add a string value.
    #[must_use]
    pub fn text(self, key: &'static str, value: impl Into<String>) -> Self {
        self.insert(key, DetailValue::Text(value.into()))
    }

    /// Add an integer value.
    #[must_use]
    pub fn int(self, key: &'static str, value: u64) -> Self {
        self.insert(key, DetailValue::Int(value))
    }

    /// Add a boolean value.
    #[must_use]
    pub fn flag(self, key: &'static str, value: bool) -> Self {
        self.insert(key, DetailValue::Bool(value))
    }

    /// Add an array, truncated to [`MAX_DETAIL_LIST`] entries.
    #[must_use]
    pub fn list(self, key: &'static str, values: Vec<String>) -> Self {
        let mut values = values;
        values.truncate(MAX_DETAIL_LIST);
        self.insert(key, DetailValue::List(values))
    }

    /// The number of keys currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no key is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn with_bucket(mut self, bucket: Bucket) -> Self {
        self.0
            .insert("bucket", DetailValue::Text(bucket.as_str().to_owned()));
        self
    }
}

/// One error or one warning: a stable code, a sentence, and bounded details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// The stable code a caller may branch on.
    pub code: &'static str,
    /// One human sentence, never parsed.
    pub message: String,
    /// Bounded, privacy-constrained details, always carrying `bucket`.
    pub details: Details,
    #[serde(skip)]
    bucket: Bucket,
    #[serde(skip)]
    retryable: bool,
}

impl Diagnostic {
    /// Build a diagnostic, deriving the bucket from the code's prefix.
    #[must_use]
    pub fn new(code: &'static str, message: &str, details: Details) -> Self {
        let bucket = Bucket::from_code(code);
        Self {
            code,
            message: message.to_owned(),
            details: details.with_bucket(bucket),
            bucket,
            retryable: false,
        }
    }

    /// Mark the condition as one the same invocation may retry later.
    #[must_use]
    pub const fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    /// The bucket the code belongs to.
    #[must_use]
    pub const fn bucket(&self) -> Bucket {
        self.bucket
    }

    /// Whether the contract calls this condition retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.retryable
    }

    /// The process exit code this diagnostic maps to as an error.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.bucket.exit_code()
    }
}

/// A warning has the same shape and bounds as an error.
pub type Warning = Diagnostic;

/// A failed command: the first refusal, plus every degradation already seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The refusal that stopped the command.
    pub error: Diagnostic,
    /// Degradations observed before the refusal; never dropped.
    pub warnings: Vec<Warning>,
}

impl Failure {
    /// A failure that observed no degradation.
    #[must_use]
    pub const fn new(error: Diagnostic) -> Self {
        Self {
            error,
            warnings: Vec::new(),
        }
    }

    /// A failure carrying the degradations observed before it.
    #[must_use]
    pub const fn with_warnings(error: Diagnostic, warnings: Vec<Warning>) -> Self {
        Self { error, warnings }
    }
}

impl From<Diagnostic> for Failure {
    fn from(error: Diagnostic) -> Self {
        Self::new(error)
    }
}

/// A successful command: its data, plus every degradation observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome<T> {
    /// The command's result, which is not an error even when it reports one.
    pub data: T,
    /// Degradations observed while the command ran.
    pub warnings: Vec<Warning>,
}

/// The result of a command: an outcome with warnings, or a failure with them.
pub type Result<T> = std::result::Result<Outcome<T>, Failure>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bucket_maps_to_one_documented_exit_code() {
        for (code, bucket, exit) in [
            (codes::USAGE_ARGUMENTS, Bucket::Usage, 2),
            (codes::USAGE_ARCHIVE_ROOT_MISSING, Bucket::Usage, 2),
            (codes::INPUT_CAP_FILE_SIZE, Bucket::Input, 3),
            (codes::INPUT_CAP_IMPORT_BYTES, Bucket::Input, 3),
            (codes::INPUT_CAP_IMPORT_FILES, Bucket::Input, 3),
            (codes::INPUT_CAP_RECORD_SIZE, Bucket::Input, 3),
            (codes::INPUT_CAP_FILENAME_LENGTH, Bucket::Input, 3),
            (codes::INPUT_CAP_FIELD_LENGTH, Bucket::Input, 3),
            (codes::PATH_SYMLINK, Bucket::Path, 3),
            (codes::PATH_OVERWRITE, Bucket::Path, 3),
            (codes::PATH_CROSS_DEVICE, Bucket::Path, 3),
            (codes::ARCHIVE_MARKER_MISSING, Bucket::Archive, 4),
            (codes::ARCHIVE_MARKER_MALFORMED, Bucket::Archive, 4),
            (codes::ARCHIVE_ADOPT_REFUSED, Bucket::Archive, 4),
            (codes::ARCHIVE_SCHEMA_NEWER, Bucket::Archive, 4),
            (codes::ARCHIVE_SCHEMA_OLDER, Bucket::Archive, 4),
            (codes::ARCHIVE_PERMISSIONS_WIDE, Bucket::Archive, 4),
            (codes::ARCHIVE_MULTIPLE_FILESYSTEMS, Bucket::Archive, 4),
            (codes::LOCK_HELD, Bucket::Lock, 4),
            (codes::WRITE_INTERRUPTED, Bucket::Write, 4),
            (codes::RECORD_NOT_FOUND, Bucket::Record, 4),
            (codes::RECORD_MALFORMED, Bucket::Record, 4),
            (codes::RECORD_INCONSISTENT, Bucket::Record, 4),
            (codes::INTEGRITY_DIGEST_MISMATCH, Bucket::Integrity, 4),
            (codes::INTEGRITY_LENGTH_MISMATCH, Bucket::Integrity, 4),
            (codes::INTEGRITY_DANGLING_REFERENCE, Bucket::Integrity, 4),
            (codes::INTEGRITY_ORPHAN_OBJECT, Bucket::Integrity, 4),
            (codes::PLATFORM_FILESYSTEM_UNSUPPORTED, Bucket::Platform, 5),
            (codes::PLATFORM_NO_DIRECTORY_FSYNC, Bucket::Platform, 5),
            (codes::PLATFORM_OWNER_ONLY_VIA_ACL, Bucket::Platform, 5),
            (codes::INTERNAL_UNEXPECTED, Bucket::Internal, 6),
        ] {
            assert_eq!(Bucket::from_code(code), bucket, "bucket of {code}");
            assert_eq!(bucket.exit_code(), exit, "exit code of {code}");
            assert_ne!(bucket.exit_code(), 1, "exit code 1 is reserved");
        }
        assert_eq!(Bucket::Record.exit_code(), 4);
        assert_eq!(Bucket::Export.exit_code(), 4);
        assert_eq!(Bucket::Delete.exit_code(), 4);
        assert_eq!(Bucket::from_code("nonsense"), Bucket::Internal);
        assert_eq!(Bucket::Record.as_str(), "record");
    }

    #[test]
    fn details_are_bounded_and_always_name_their_bucket() {
        let mut details = Details::new();
        for key in [
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q",
            "r", "s",
        ] {
            details = details.text(key, "value");
        }
        assert_eq!(
            details.len(),
            MAX_DETAIL_KEYS - 1,
            "a slot is left for bucket"
        );
        let diagnostic =
            Diagnostic::new(codes::LOCK_HELD, "Another writer holds the lock.", details)
                .retryable();
        assert_eq!(diagnostic.details.len(), MAX_DETAIL_KEYS);
        let json = serde_json::to_value(&diagnostic).unwrap();
        assert_eq!(json["details"]["bucket"], "lock");
        assert_eq!(
            json["details"].as_object().unwrap().len(),
            MAX_DETAIL_KEYS,
            "details never exceed the contract's bound"
        );
        assert!(diagnostic.is_retryable());
        assert_eq!(diagnostic.exit_code(), 4);
        assert_eq!(diagnostic.bucket(), Bucket::Lock);
    }

    #[test]
    fn lists_are_truncated_and_scalars_serialize_untagged() {
        let details = Details::new()
            .list("ids", (0..40).map(|index| index.to_string()).collect())
            .int("count", 3)
            .flag("created_object", true);
        let json = serde_json::to_value(Diagnostic::new(
            codes::INTERNAL_UNEXPECTED,
            "An invariant was violated.",
            details,
        ))
        .unwrap();
        assert_eq!(
            json["details"]["ids"].as_array().unwrap().len(),
            MAX_DETAIL_LIST
        );
        assert_eq!(json["details"]["count"], 3);
        assert_eq!(json["details"]["created_object"], true);
        assert!(Details::new().is_empty());
        assert_eq!(Details::new().int("count", 1).len(), 1);
    }

    #[test]
    fn a_failure_keeps_the_warnings_observed_before_it() {
        let warning = Diagnostic::new(
            codes::PLATFORM_NO_DIRECTORY_FSYNC,
            "Directory durability is weaker on this platform.",
            Details::new().text("stage", "object_write"),
        );
        let failure = Failure::with_warnings(
            Diagnostic::new(
                codes::LOCK_HELD,
                "Another writer holds the lock.",
                Details::new(),
            ),
            vec![warning],
        );
        assert_eq!(failure.warnings.len(), 1);
        assert!(!failure.error.is_retryable());
        let from: Failure = Diagnostic::new(codes::LOCK_HELD, "Held.", Details::new()).into();
        assert!(from.warnings.is_empty());
    }
}
