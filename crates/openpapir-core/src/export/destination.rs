//! The export destination: where openPapir may write, and how.
//!
//! The destination is the one path in this crate that lies outside the
//! archive root, so every rule the archive applies inward is applied outward
//! here. A symbolic link is refused rather than followed, a file that is
//! already there is refused rather than replaced, and a destination inside the
//! archive root is refused before anything is copied, because an export that
//! wrote into the archive would no longer be a copy of it.
//!
//! The archive's own publish step is a hard link, which is what keeps a write
//! inside the root from replacing a file openPapir did not create. It is
//! deliberately not used here: a destination may be a filesystem that cannot
//! create a hard link at all, and the design requires an export to work on
//! any of them. Each file is instead created at its final path with
//! create-new semantics, which refuses an existing path just as firmly, and
//! the export removes what it created when it fails, so an interrupted export
//! does not leave a destination a retry would then refuse.
//!
//! The destination path itself never reaches a diagnostic. What a refusal
//! names is the path relative to the destination, which is built from
//! openPapir's own fixed directory names plus a digest or an identifier
//! (`docs/error-contract.md`).

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::archive::paths;
use crate::error::{Details, Diagnostic, codes};

/// The directory holding copied objects, relative to the destination.
pub const OBJECTS_DIR: &str = "objects";
/// The directory holding exported records, relative to the destination.
pub const RECORDS_DIR: &str = "records";
/// The manifest's file name, relative to the destination.
pub const MANIFEST_FILE: &str = "manifest.json";

/// The stage an interrupted copy of a stored object reports.
///
/// It covers the objects directory as well as the copies in it: a failure to
/// make that directory interrupted the object copy that asked for it.
pub const OBJECT_WRITE: &str = "object_write";
/// The stage an interrupted write of a record document reports.
///
/// It covers the record directories for the same reason.
pub const RECORD_WRITE: &str = "record_write";
/// The stage the destination's own top-level writes report.
///
/// The destination directory itself and `manifest.json` are the export's
/// equivalent of the archive marker: they describe the export rather than
/// belonging to any one record or object (`docs/error-contract.md`).
pub const MARKER_WRITE: &str = "marker_write";

/// One path this export created, so that a failure can undo exactly it.
#[derive(Debug)]
enum Created {
    /// A directory this export made.
    Directory(PathBuf),
    /// A file this export wrote.
    File(PathBuf),
}

/// A prepared destination directory, and what this export put in it.
#[derive(Debug)]
pub struct Destination {
    path: PathBuf,
    created_root: bool,
    created: RefCell<Vec<Created>>,
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
        self.directory(OBJECTS_DIR, OBJECT_WRITE)
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
        self.directory(RECORDS_DIR, RECORD_WRITE)?;
        self.directory(&format!("{RECORDS_DIR}/{kind}"), RECORD_WRITE)
    }

    /// Create one directory inside the destination, owner-only.
    ///
    /// A directory this call makes is remembered, so that a failed export can
    /// remove it again. One that was already there is left in the record
    /// untouched, because the export did not create it and may not remove it.
    fn directory(&self, relative: &str, stage: &'static str) -> Result<PathBuf, Diagnostic> {
        let path = self.path.join(relative);
        if paths::is_symlink(&path) {
            return Err(symlink_refusal(relative));
        }
        if path.is_dir() {
            return Ok(path);
        }
        paths::create_dir_owner_only(&path).map_err(|error| refusal(&error, relative, stage))?;
        self.created
            .borrow_mut()
            .push(Created::Directory(path.clone()));
        Ok(path)
    }

    /// Create one file at its final path, replacing nothing.
    ///
    /// The file is opened with create-new semantics, which fails rather than
    /// replacing anything already there, a symbolic link included. The caller
    /// streams into the returned handle and flushes it.
    ///
    /// # Errors
    ///
    /// Returns `path.symlink`, `export.destination_conflict`, or
    /// `write.interrupted`.
    pub fn create(
        &self,
        directory: &Path,
        file_name: &str,
        relative: &str,
        stage: &'static str,
    ) -> Result<File, Diagnostic> {
        let target = directory.join(file_name);
        refuse_existing(&target, relative)?;
        let file = paths::create_file_owner_only(&target).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                conflict(relative, 1)
            } else {
                refusal(&error, relative, stage)
            }
        })?;
        self.created.borrow_mut().push(Created::File(target));
        Ok(file)
    }

    /// Write one whole document into the destination, replacing nothing.
    ///
    /// # Errors
    ///
    /// Returns the same refusals as [`Destination::create`].
    pub fn write_new(
        &self,
        directory: &Path,
        file_name: &str,
        relative: &str,
        stage: &'static str,
        content: &[u8],
    ) -> Result<(), Diagnostic> {
        let mut file = self.create(directory, file_name, relative, stage)?;
        file.write_all(content)
            .and_then(|()| file.sync_all())
            .map_err(|error| refusal(&error, relative, stage))
    }

    /// Remove one file this export created, before the export goes on.
    ///
    /// A copy that turned out not to hold the bytes its name describes goes
    /// at once rather than waiting for the export to unwind.
    pub fn remove_created(&self, path: &Path) {
        remove_created_file(path);
        self.created
            .borrow_mut()
            .retain(|entry| !matches!(entry, Created::File(created) if created == path));
    }

    /// Remove exactly what this export created, and nothing else.
    ///
    /// Files go before the directories that hold them, in the reverse of the
    /// order they were created, and a directory the export found already
    /// there is never removed. Removal is best effort: the export is already
    /// failing with a refusal of its own, and a destination that cannot be
    /// tidied is not a second, different failure to report.
    pub fn discard(&self) {
        for entry in self.created.borrow().iter().rev() {
            match entry {
                Created::File(path) => remove_created_file(path),
                Created::Directory(path) => {
                    let _ = fs::remove_dir(path);
                }
            }
        }
        self.created.borrow_mut().clear();
        if self.created_root {
            let _ = fs::remove_dir(&self.path);
        }
    }
}

/// Remove a file this export created, whatever mode it was given.
///
/// A copied object is owner-read-only, which on a platform that honours a
/// read-only attribute would otherwise refuse the removal and leave the
/// destination half written.
fn remove_created_file(path: &Path) {
    if paths::is_symlink(path) {
        return;
    }
    #[cfg(not(unix))]
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
    let _ = fs::remove_file(path);
}

/// Refuse the destination before anything is copied, then make it ready.
///
/// The destination is an existing empty directory or one this call creates,
/// owner-only. It is never inside the archive root, and it is never a
/// symbolic link.
///
/// # Errors
///
/// Returns `usage.arguments` when the destination is unusable, when it lies
/// inside the archive root, or when either path cannot be resolved,
/// `path.symlink` when it is a symbolic link,
/// `export.destination_conflict` when it already holds anything, and
/// `write.interrupted` when it cannot be created.
pub fn prepare(archive_root: &Path, destination: &Path) -> Result<Destination, Diagnostic> {
    if paths::is_symlink(destination) {
        return Err(symlink_refusal("."));
    }
    refuse_inside_archive(archive_root, destination)?;
    let mut created_root = false;
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_dir() => refuse_non_empty(destination)?,
        Ok(_) => return Err(unusable()),
        Err(_) => {
            paths::create_dir_owner_only(destination)
                .map_err(|error| refusal(&error, ".", MARKER_WRITE))?;
            created_root = true;
        }
    }
    Ok(Destination {
        path: destination.to_path_buf(),
        created_root,
        created: RefCell::new(Vec::new()),
    })
}

/// Refuse a destination inside the archive root.
///
/// The comparison is made on resolved paths, so a relative path, a `..`
/// segment, or a link on the way to the destination cannot hide the fact that
/// the export would write into the archive it is copying. A path that cannot
/// be resolved at all is refused rather than let through: the check must fail
/// closed, because the question it answers is whether the export is about to
/// write inside the archive.
fn refuse_inside_archive(archive_root: &Path, destination: &Path) -> Result<(), Diagnostic> {
    let Ok(archive) = archive_root.canonicalize() else {
        return Err(unresolvable());
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

/// Refuse a target path that already exists, whatever it is.
///
/// The create-new open refuses one as well. This check runs first so that a
/// link and a file are told apart, which the open's single error kind cannot
/// do.
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

/// The refusal for an archive root that cannot be resolved.
///
/// Without it the export cannot tell whether the destination lies inside the
/// archive, so it refuses rather than assuming that it does not.
fn unresolvable() -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "The archive root could not be resolved, so the destination cannot be checked against it.",
        Details::new()
            .text("argument", "archive_root")
            .text("scope", "export_destination"),
    )
}

/// Map an I/O failure in the destination into the contract.
///
/// An existing path is a conflict rather than an overwrite, because the
/// destination is outside the archive; everything else interrupted the write,
/// and the export removes what it had already created.
///
/// The stage is the caller's, because only the caller knows what it was
/// writing: an object copy reports [`OBJECT_WRITE`], a record document
/// [`RECORD_WRITE`], and the destination itself or the manifest
/// [`MARKER_WRITE`]. Reporting one stage for all three would describe an
/// interrupted object copy as a record write.
#[must_use]
pub fn refusal(error: &io::Error, relative: &str, stage: &'static str) -> Diagnostic {
    if error.kind() == io::ErrorKind::AlreadyExists {
        return conflict(relative, 1);
    }
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "A write into the export destination was interrupted before it could be completed.",
        Details::new()
            .text("stage", stage)
            .text("scope", "export_destination"),
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
    fn an_archive_root_that_cannot_be_resolved_fails_closed() {
        let (home, root) = archive_and_home();
        let refusal = refuse_inside_archive(&root.join("absent"), &home.path().join("export"))
            .expect_err("an unresolvable root is refused rather than assumed to be elsewhere");
        assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["argument"],
            "archive_root"
        );
        assert_eq!(
            prepare(&root.join("absent"), &home.path().join("export"))
                .unwrap_err()
                .code,
            codes::USAGE_ARGUMENTS
        );
        assert!(
            !home.path().join("export").exists(),
            "no destination is created when the check cannot be made"
        );
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
        prepared
            .write_new(&objects, "name", "objects/name", OBJECT_WRITE, b"first\n")
            .unwrap();
        assert_eq!(fs::read(objects.join("name")).unwrap(), b"first\n");
        let refusal = prepared
            .write_new(&objects, "name", "objects/name", OBJECT_WRITE, b"second\n")
            .unwrap_err();
        assert_eq!(refusal.code, codes::EXPORT_DESTINATION_CONFLICT);
        assert_eq!(
            fs::read(objects.join("name")).unwrap(),
            b"first\n",
            "the existing file is left exactly as it was"
        );
    }

    #[test]
    fn discarding_removes_what_the_export_made_and_nothing_else() {
        let (home, root) = archive_and_home();
        let destination = home.path().join("export");
        let prepared = prepare(&root, &destination).unwrap();
        let objects = prepared.objects_directory().unwrap();
        prepared
            .write_new(&objects, "name", "objects/name", OBJECT_WRITE, b"copied\n")
            .unwrap();
        let records = prepared.records_directory("case").unwrap();
        prepared
            .write_new(
                &records,
                "id.json",
                "records/case/id.json",
                RECORD_WRITE,
                b"{}\n",
            )
            .unwrap();
        prepared.discard();
        assert!(
            !destination.exists(),
            "a destination the export created is removed again"
        );

        let existing = home.path().join("existing");
        fs::create_dir(&existing).unwrap();
        let prepared = prepare(&root, &existing).unwrap();
        let objects = prepared.objects_directory().unwrap();
        prepared
            .write_new(&objects, "name", "objects/name", OBJECT_WRITE, b"copied\n")
            .unwrap();
        prepared.discard();
        assert!(
            existing.is_dir(),
            "a destination the user made is never removed"
        );
        assert_eq!(
            fs::read_dir(&existing).unwrap().count(),
            0,
            "everything the export put in it is gone"
        );
    }

    #[test]
    fn a_file_removed_during_the_export_is_not_removed_twice() {
        let (home, root) = archive_and_home();
        let destination = home.path().join("export");
        let prepared = prepare(&root, &destination).unwrap();
        let objects = prepared.objects_directory().unwrap();
        prepared
            .write_new(&objects, "name", "objects/name", OBJECT_WRITE, b"copied\n")
            .unwrap();
        prepared.remove_created(&objects.join("name"));
        assert!(!objects.join("name").exists());
        prepared.discard();
        assert!(!destination.exists());
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
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["scope"],
            "export_destination"
        );

        let destination = home.path().join("export");
        let prepared = prepare(&root, &destination).unwrap();
        let objects = prepared.objects_directory().unwrap();
        let target = objects.join("name");
        std::os::unix::fs::symlink(home.path().join("outside"), &target).unwrap();
        let refusal = prepared
            .write_new(&objects, "name", "objects/name", OBJECT_WRITE, b"x")
            .unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        assert!(
            !home.path().join("outside").exists(),
            "the link's target is never created"
        );
        prepared.discard();
        assert!(
            target.symlink_metadata().is_ok(),
            "a link the export did not create is left alone"
        );
    }

    /// The outward no-follow rule on a directory component is a check before
    /// the create, not a no-follow open: a directory the export would have to
    /// descend through is refused when it is a link, so no write ever
    /// descends through one (`docs/architecture.md`).
    #[cfg(unix)]
    #[test]
    fn a_linked_directory_component_is_refused_rather_than_descended_into() {
        let (home, root) = archive_and_home();
        let elsewhere = home.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let destination = home.path().join("export");
        let prepared = prepare(&root, &destination).unwrap();
        std::os::unix::fs::symlink(&elsewhere, destination.join(OBJECTS_DIR)).unwrap();
        let refusal = prepared.objects_directory().unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["export_path"],
            OBJECTS_DIR
        );

        std::os::unix::fs::symlink(&elsewhere, destination.join(RECORDS_DIR)).unwrap();
        assert_eq!(
            prepared.records_directory("case").unwrap_err().code,
            codes::PATH_SYMLINK
        );
        assert_eq!(
            fs::read_dir(&elsewhere).unwrap().count(),
            0,
            "nothing is ever written through the link"
        );
    }

    #[test]
    fn an_interrupted_write_is_reported_without_an_archive_path() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let interrupted = refusal(&error, "objects/name", OBJECT_WRITE);
        assert_eq!(interrupted.code, codes::WRITE_INTERRUPTED);
        assert!(interrupted.is_retryable());
        let json = serde_json::to_value(&interrupted).unwrap();
        assert_eq!(json["details"]["scope"], "export_destination");
        assert_eq!(
            json["details"]["stage"], "object_write",
            "the caller's stage is reported rather than one fixed name"
        );
        assert!(
            json["details"].get("archive_path").is_none(),
            "a destination refusal never names an archive-relative path"
        );
        for stage in [RECORD_WRITE, MARKER_WRITE] {
            let reported = serde_json::to_value(refusal(&error, ".", stage)).unwrap();
            assert_eq!(reported["details"]["stage"], stage);
        }
        assert_eq!(
            refusal(
                &io::Error::new(io::ErrorKind::AlreadyExists, "exists"),
                ".",
                RECORD_WRITE
            )
            .code,
            codes::EXPORT_DESTINATION_CONFLICT
        );
    }
}
