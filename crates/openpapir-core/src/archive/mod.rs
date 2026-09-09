//! The local archive: creation, adoption rules, and the opened root.
//!
//! The archive root is supplied explicitly by the user. openPapir never
//! searches for an archive, never adopts a directory that has no marker, and
//! never creates one implicitly as a side effect of another operation
//! (`docs/archive-layout.md`).

pub mod import;
pub mod limits;
pub mod lock;
pub mod objects;
pub mod paths;
pub mod write;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::ident;

/// The archive schema version this build writes and supports.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;
/// The marker file, written first when an archive is created.
pub const MARKER_FILE: &str = "papir-archive.json";
/// The directory holding import-event records, relative to the root.
pub const IMPORTS_DIR: &str = "records/imports";
/// The directory holding case records, relative to the root.
pub const CASES_DIR: &str = crate::records::CASES_DIR;
/// The directory holding submission records, relative to the root.
pub const SUBMISSIONS_DIR: &str = crate::records::SUBMISSIONS_DIR;
/// The directory holding receipt records, relative to the root.
pub const RECEIPTS_DIR: &str = crate::records::RECEIPTS_DIR;
/// The directory holding association records, relative to the root.
pub const ASSOCIATIONS_DIR: &str = crate::records::ASSOCIATIONS_DIR;
/// The disposable, rebuildable index directory, relative to the root.
pub const CACHE_DIR: &str = "cache";

/// The directories an archive holds, relative to its root.
const LAYOUT_DIRS: [&str; 10] = [
    "objects",
    "objects/sha256",
    "objects/incoming",
    "records",
    IMPORTS_DIR,
    CASES_DIR,
    SUBMISSIONS_DIR,
    RECEIPTS_DIR,
    ASSOCIATIONS_DIR,
    CACHE_DIR,
];

/// The archive marker: the schema version, the archive's own opaque
/// identifier, and the openPapir version that created it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// The archive's own opaque identifier, minted by openPapir.
    pub archive_id: String,
    /// The schema version the archive was written under.
    pub archive_schema_version: u32,
    /// The openPapir version that created the archive.
    pub created_by: String,
}

/// What creating an archive reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Created {
    /// The archive's own opaque identifier.
    pub archive_id: String,
    /// The schema version written into the marker.
    pub archive_schema_version: u32,
}

/// An opened archive root, ready for a write.
#[derive(Debug)]
pub struct Archive {
    root: PathBuf,
    marker: Marker,
    warnings: Vec<Warning>,
}

impl Archive {
    /// The archive root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The archive's marker.
    #[must_use]
    pub const fn marker(&self) -> &Marker {
        &self.marker
    }

    /// Take the degradations observed while opening.
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }

    /// Report a degradation observed while the archive is open.
    pub fn warn(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }

    /// Open an existing archive, refusing every condition the design names.
    ///
    /// # Errors
    ///
    /// Returns `usage.archive_root_missing`, `usage.arguments`,
    /// `path.symlink`, `archive.marker_missing`, `archive.marker_malformed`,
    /// `archive.schema_newer`, `archive.schema_older`,
    /// `archive.permissions_wide`, or `archive.multiple_filesystems`.
    pub fn open(root: &Path) -> std::result::Result<Self, Diagnostic> {
        let mut warnings = Vec::new();
        let marker = Self::inspect(root, &mut warnings)?;
        ensure_layout(root, &mut warnings)?;
        check_single_filesystem(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            marker,
            warnings,
        })
    }

    /// Open an existing archive without changing anything inside it.
    ///
    /// The difference from [`Archive::open`] is deliberate and load-bearing
    /// for a read-only command: no layout directory is created, and no
    /// directory entry is flushed, so opening an archive to read it never
    /// writes to it and never fails because the root is not writable. The
    /// symbolic-link refusal on every layout directory is kept, because a
    /// reader that walked a linked `records/<kind>` would read outside the
    /// archive.
    ///
    /// A layout directory that is absent is left absent. Every reader here
    /// treats a missing directory as an empty one, so a read-only command
    /// reports what the archive holds rather than repairing its shape.
    ///
    /// # Errors
    ///
    /// Returns the same refusals as [`Archive::open`], except those of
    /// creating a directory, which it never does.
    pub fn open_read_only(root: &Path) -> std::result::Result<Self, Diagnostic> {
        let mut warnings = Vec::new();
        let marker = Self::inspect(root, &mut warnings)?;
        check_layout_links(root)?;
        check_single_filesystem(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            marker,
            warnings,
        })
    }

    /// The checks both open paths run before they differ: the root's shape,
    /// the marker, the permissions, and the schema version.
    fn inspect(
        root: &Path,
        warnings: &mut Vec<Warning>,
    ) -> std::result::Result<Marker, Diagnostic> {
        check_root_shape(root)?;
        if !cfg!(unix) {
            warnings.push(paths::owner_only_via_acl_warning());
            warnings.push(paths::no_follow_after_open_warning());
        }
        let marker = read_marker(root)?;
        check_permissions(root)?;
        check_schema_version(marker.archive_schema_version)?;
        Ok(marker)
    }
}

/// Create an archive at an existing, empty directory.
///
/// # Errors
///
/// Returns `usage.archive_root_missing` when the directory does not exist,
/// `usage.arguments` when the path is not a directory, `path.symlink`,
/// `archive.adopt_refused` when the directory is not empty and has no marker,
/// `path.overwrite` when it already holds a marker, and the write refusals of
/// the atomic write procedure.
pub fn init(root: &Path) -> Result<Created> {
    check_root_shape(root).map_err(Failure::new)?;
    let mut warnings = Vec::new();
    if !cfg!(unix) {
        warnings.push(paths::owner_only_via_acl_warning());
    }
    check_empty_for_init(root).map_err(|error| Failure::with_warnings(error, warnings.clone()))?;
    let created = init_layout(root, &mut warnings)
        .map_err(|error| Failure::with_warnings(error, warnings.clone()))?;
    Ok(Outcome {
        data: created,
        warnings,
    })
}

/// Narrow every path in the archive at `root` back to owner-only.
///
/// This is the only action besides [`init`] that narrows permissions, and it
/// is the documented remedy for copy tooling that widened them on restore
/// (`docs/archive-layout.md`). It only ever narrows: a path can lose access
/// and never gain it. It takes the writer lock, because it writes modes, and
/// it deliberately does not run the archive's permission check first, since
/// the wide permissions that check refuses are exactly what it repairs.
///
/// # Errors
///
/// Returns `usage.archive_root_missing`, `usage.arguments`, `path.symlink`,
/// `archive.marker_missing`, `archive.marker_malformed`,
/// `archive.schema_newer`, `archive.schema_older`, `lock.held`, or
/// `write.interrupted`.
pub fn repair_permissions(root: &Path) -> Result<crate::export::repair::Repaired> {
    let mut warnings = Vec::new();
    match repair(root, &mut warnings) {
        Ok(data) => Ok(Outcome { data, warnings }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn repair(
    root: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<crate::export::repair::Repaired, Diagnostic> {
    check_root_shape(root)?;
    if !cfg!(unix) {
        warnings.push(paths::owner_only_via_acl_warning());
    }
    let marker = read_marker(root)?;
    check_schema_version(marker.archive_schema_version)?;
    check_layout_links(root)?;
    let _lock = lock::WriterLock::acquire(root)?;
    crate::export::repair::narrow_archive(root, &LAYOUT_DIRS)
}

/// Narrow the root, write the marker first, then create the layout.
fn init_layout(
    root: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Created, Diagnostic> {
    paths::narrow_to_owner_only(root)
        .map_err(|error| paths::publish_refusal(&error, ".", "marker_write"))?;
    let marker = Marker {
        archive_id: ident::new_id()?,
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        created_by: format!("openpapir {}", env!("CARGO_PKG_VERSION")),
    };
    let document = marker_document(&marker)?;
    warnings.extend(write::write_document(
        root,
        MARKER_FILE,
        MARKER_FILE,
        document.as_bytes(),
        "marker_write",
    )?);
    ensure_layout(root, warnings)?;
    check_single_filesystem(root)?;
    Ok(Created {
        archive_id: marker.archive_id,
        archive_schema_version: marker.archive_schema_version,
    })
}

/// Serialise the marker as one document with sorted keys and a final newline.
fn marker_document(marker: &Marker) -> std::result::Result<String, Diagnostic> {
    let document = serde_json::to_string(marker).map_err(|_| {
        Diagnostic::new(
            codes::INTERNAL_UNEXPECTED,
            "An archive marker could not be serialised.",
            Details::new(),
        )
    })?;
    let document = format!("{document}\n");
    limits::check_record_size(document.len() as u64)?;
    Ok(document)
}

/// Refuse a root that is a symbolic link, missing, or not a directory.
fn check_root_shape(root: &Path) -> std::result::Result<(), Diagnostic> {
    if paths::is_symlink(root) {
        return Err(paths::symlink_refusal(
            Details::new().text("scope", "archive"),
        ));
    }
    let metadata = fs::symlink_metadata(root).map_err(|_| {
        Diagnostic::new(
            codes::USAGE_ARCHIVE_ROOT_MISSING,
            "The archive root was not supplied or does not exist.",
            Details::new(),
        )
    })?;
    if !metadata.is_dir() {
        return Err(Diagnostic::new(
            codes::USAGE_ARGUMENTS,
            "The archive root is not a directory.",
            Details::new().text("argument", "archive_root"),
        ));
    }
    Ok(())
}

/// Refuse initialisation on a directory that is not empty.
fn check_empty_for_init(root: &Path) -> std::result::Result<(), Diagnostic> {
    let entries = fs::read_dir(root).map_err(|_| {
        Diagnostic::new(
            codes::USAGE_ARGUMENTS,
            "The archive root could not be read.",
            Details::new().text("argument", "archive_root"),
        )
    })?;
    let entry_count = entries.count() as u64;
    if entry_count == 0 {
        return Ok(());
    }
    if root.join(MARKER_FILE).exists() {
        return Err(Diagnostic::new(
            codes::PATH_OVERWRITE,
            "The directory already holds an archive marker.",
            Details::new().text("archive_path", MARKER_FILE),
        ));
    }
    Err(Diagnostic::new(
        codes::ARCHIVE_ADOPT_REFUSED,
        "The directory is not empty and holds no archive marker.",
        Details::new().int("entry_count", entry_count),
    ))
}

/// Refuse an archive whose permissions are wider than owner-only.
///
/// The root, the marker, the lock file when one is present, and every layout
/// directory are checked. Stored objects and their fan-out directories are
/// checked as the operation reaches them, before anything is published.
fn check_permissions(root: &Path) -> std::result::Result<(), Diagnostic> {
    let mut wide = Vec::new();
    if paths::is_wider_than_owner_only(root) {
        wide.push(".".to_owned());
    }
    let mut checked: Vec<&str> = vec![MARKER_FILE, lock::LOCK_FILE];
    checked.extend(LAYOUT_DIRS);
    for relative in checked {
        let path = root.join(relative);
        if path.exists() && paths::is_wider_than_owner_only(&path) {
            wide.push(relative.to_owned());
        }
    }
    let Some((first, others)) = wide.split_first() else {
        return Ok(());
    };
    Err(paths::wide_permissions_refusal(first, others))
}

/// Read and parse the marker, refusing a missing or unreadable one.
fn read_marker(root: &Path) -> std::result::Result<Marker, Diagnostic> {
    let path = root.join(MARKER_FILE);
    if paths::is_symlink(&path) {
        return Err(paths::symlink_refusal(
            Details::new()
                .text("scope", "archive")
                .text("archive_path", MARKER_FILE),
        ));
    }
    if !path.is_file() {
        return Err(Diagnostic::new(
            codes::ARCHIVE_MARKER_MISSING,
            "The directory holds no archive marker, so there is no archive to open.",
            Details::new(),
        ));
    }
    let text = fs::read_to_string(&path).map_err(|_| malformed_marker())?;
    serde_json::from_str(&text).map_err(|_| malformed_marker())
}

fn malformed_marker() -> Diagnostic {
    Diagnostic::new(
        codes::ARCHIVE_MARKER_MALFORMED,
        "The archive marker cannot be read as a valid marker.",
        Details::new(),
    )
}

/// Refuse a schema version this build does not support.
fn check_schema_version(version: u32) -> std::result::Result<(), Diagnostic> {
    if version > SUPPORTED_SCHEMA_VERSION {
        return Err(Diagnostic::new(
            codes::ARCHIVE_SCHEMA_NEWER,
            "The archive's schema version is newer than this build supports.",
            Details::new()
                .int("archive_schema_version", u64::from(version))
                .int(
                    "supported_schema_version",
                    u64::from(SUPPORTED_SCHEMA_VERSION),
                ),
        ));
    }
    if version < SUPPORTED_SCHEMA_VERSION {
        return Err(Diagnostic::new(
            codes::ARCHIVE_SCHEMA_OLDER,
            "The archive predates this build and requires an explicit migration.",
            Details::new()
                .int("archive_schema_version", u64::from(version))
                .int(
                    "supported_schema_version",
                    u64::from(SUPPORTED_SCHEMA_VERSION),
                ),
        ));
    }
    Ok(())
}

/// Refuse a layout directory that is a symbolic link, creating nothing.
///
/// A reader joins a record directory and lists it, so a linked one would take
/// the reader outside the archive root. The link is refused rather than
/// followed, exactly as every other path inside the archive is.
fn check_layout_links(root: &Path) -> std::result::Result<(), Diagnostic> {
    for relative in LAYOUT_DIRS {
        if paths::is_symlink(&root.join(relative)) {
            return Err(paths::symlink_refusal(
                Details::new()
                    .text("scope", "archive")
                    .text("archive_path", relative),
            ));
        }
    }
    Ok(())
}

/// Create every layout directory owner-only, refusing a symbolic link.
fn ensure_layout(root: &Path, warnings: &mut Vec<Warning>) -> std::result::Result<(), Diagnostic> {
    check_layout_links(root)?;
    for relative in LAYOUT_DIRS {
        let path = root.join(relative);
        paths::create_dir_owner_only(&path)
            .map_err(|error| paths::publish_refusal(&error, relative, "record_write"))?;
    }
    if let Some(warning) = paths::sync_directory(root, "record_write") {
        warnings.push(warning);
    }
    Ok(())
}

/// Refuse an archive root that spans more than one filesystem.
fn check_single_filesystem(root: &Path) -> std::result::Result<(), Diagnostic> {
    let Some(root_device) = paths::device_of(root) else {
        return Ok(());
    };
    for relative in LAYOUT_DIRS {
        let path = root.join(relative);
        if paths::device_of(&path).is_some_and(|device| device != root_device) {
            return Err(Diagnostic::new(
                codes::ARCHIVE_MULTIPLE_FILESYSTEMS,
                "The archive root spans more than one filesystem.",
                Details::new().text("archive_path", relative),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_archive_writes_its_marker_and_layout() {
        let root = tempfile::tempdir().unwrap();
        let outcome = init(root.path()).unwrap();
        assert_eq!(outcome.data.archive_schema_version, 1);
        assert_eq!(outcome.data.archive_id.len(), 32);
        assert!(
            outcome
                .warnings
                .iter()
                .all(|warning| warning.bucket() == crate::error::Bucket::Platform),
            "only platform degradations are reported"
        );
        for relative in LAYOUT_DIRS {
            assert!(root.path().join(relative).is_dir(), "{relative} exists");
        }
        let marker: Marker =
            serde_json::from_str(&fs::read_to_string(root.path().join(MARKER_FILE)).unwrap())
                .unwrap();
        assert_eq!(marker.archive_id, outcome.data.archive_id);
        let opened = Archive::open(root.path()).unwrap();
        assert_eq!(opened.marker().archive_id, outcome.data.archive_id);
        assert_eq!(opened.root(), root.path());
    }

    #[test]
    fn a_read_only_open_creates_nothing_and_still_refuses_a_link() {
        let root = tempfile::tempdir().unwrap();
        init(root.path()).unwrap();
        fs::remove_dir(root.path().join(CACHE_DIR)).unwrap();
        let opened = Archive::open_read_only(root.path()).unwrap();
        assert_eq!(opened.root(), root.path());
        assert!(
            !root.path().join(CACHE_DIR).exists(),
            "a read-only open never creates a layout directory"
        );
        assert!(Archive::open(root.path()).is_ok());
        assert!(
            root.path().join(CACHE_DIR).is_dir(),
            "a writing open still creates it"
        );

        #[cfg(unix)]
        {
            let linked = root.path().join(CASES_DIR);
            fs::remove_dir(&linked).unwrap();
            std::os::unix::fs::symlink(root.path().join(CACHE_DIR), &linked).unwrap();
            assert_eq!(
                check_layout_links(root.path()).unwrap_err().code,
                codes::PATH_SYMLINK,
                "a reader never follows a linked record directory"
            );
            assert!(
                Archive::open_read_only(root.path()).is_err(),
                "the link is refused, whichever check reaches it first"
            );
        }
    }

    #[test]
    fn a_schema_version_this_build_does_not_support_is_refused() {
        assert_eq!(
            check_schema_version(SUPPORTED_SCHEMA_VERSION + 1)
                .unwrap_err()
                .code,
            codes::ARCHIVE_SCHEMA_NEWER
        );
        assert_eq!(
            check_schema_version(0).unwrap_err().code,
            codes::ARCHIVE_SCHEMA_OLDER
        );
        assert!(check_schema_version(SUPPORTED_SCHEMA_VERSION).is_ok());
    }

    #[test]
    fn a_directory_without_a_marker_is_never_adopted() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            Archive::open(root.path()).unwrap_err().code,
            codes::ARCHIVE_MARKER_MISSING
        );
        fs::write(root.path().join("stray.txt"), b"content").unwrap();
        assert_eq!(
            init(root.path()).unwrap_err().error.code,
            codes::ARCHIVE_ADOPT_REFUSED
        );
    }

    #[test]
    fn an_existing_archive_is_never_initialised_twice() {
        let root = tempfile::tempdir().unwrap();
        init(root.path()).unwrap();
        assert_eq!(
            init(root.path()).unwrap_err().error.code,
            codes::PATH_OVERWRITE
        );
    }

    #[test]
    fn a_malformed_marker_is_reported_rather_than_repaired() {
        let root = tempfile::tempdir().unwrap();
        init(root.path()).unwrap();
        fs::write(root.path().join(MARKER_FILE), b"not json").unwrap();
        assert_eq!(
            Archive::open(root.path()).unwrap_err().code,
            codes::ARCHIVE_MARKER_MALFORMED
        );
    }

    #[test]
    fn a_missing_root_is_a_usage_refusal_that_never_echoes_the_path() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("absent");
        let refusal = Archive::open(&missing).unwrap_err();
        assert_eq!(refusal.code, codes::USAGE_ARCHIVE_ROOT_MISSING);
        assert_eq!(refusal.exit_code(), 2);
        let rendered = serde_json::to_string(&refusal).unwrap();
        assert!(!rendered.contains("absent"), "the path is never echoed");

        let file = root.path().join("plain");
        fs::write(&file, b"x").unwrap();
        assert_eq!(
            Archive::open(&file).unwrap_err().code,
            codes::USAGE_ARGUMENTS
        );
        assert_eq!(init(&file).unwrap_err().error.code, codes::USAGE_ARGUMENTS);
    }
}
