//! The permission repair: the one action that narrows an existing archive.
//!
//! Ordinary copy tooling widens permissions on restore, which the owner-only
//! rule then refuses. `docs/archive-layout.md` gives one remedy: an explicit
//! action that narrows an archive back to owner-only and reports every path
//! it changed. It is a repair, not an escape hatch. There is no flag that
//! makes openPapir accept wide permissions, and this action only narrows.
//!
//! Narrowing is a mask, never an assignment: the new mode is the old mode
//! with every group bit, every other bit, and every set-user, set-group, and
//! sticky bit cleared, and for a stored object with owner write cleared as
//! well. A path can therefore only lose access, never gain it, so a
//! deliberately unreadable path stays unreadable.
//!
//! A symbolic link is refused rather than narrowed. Changing a link's
//! permissions changes its target's, which for a link out of the archive
//! would be a change to a file the archive does not own.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::archive::lock::LOCK_FILE;
use crate::archive::objects::{ALGORITHM, INCOMING_DIR, OBJECTS_DIR};
use crate::archive::paths;
use crate::archive::{CACHE_DIR, MARKER_FILE};
use crate::error::{Details, Diagnostic, codes};
use crate::export::KindCount;

/// The kinds of path the repair reports, in the order it reports them.
pub const KINDS: [&str; 7] = [
    "cache",
    "directory",
    "lock",
    "marker",
    "object",
    "record",
    "root",
];

/// The mode a directory keeps: owner read, write, and search.
const DIRECTORY_MASK: u32 = 0o700;
/// The mode a file keeps: owner read and write.
const FILE_MASK: u32 = 0o600;
/// The mode a stored object keeps: owner read, because it is write-once.
const OBJECT_MASK: u32 = 0o400;
/// How deep the repair follows a directory tree inside the archive.
const MAX_DEPTH: u32 = 8;

/// What one repair reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repaired {
    /// One entry per kind of path, ordered by kind, including the untouched.
    pub changed: Vec<KindCount>,
    /// How many paths were narrowed.
    pub paths_changed: u64,
    /// How many paths were examined.
    pub paths_checked: u64,
}

/// A running count of what the walk examined and what it narrowed.
#[derive(Debug, Default)]
struct Counts {
    changed: BTreeMap<&'static str, u64>,
    paths_changed: u64,
    paths_checked: u64,
}

impl Counts {
    fn note(&mut self, kind: &'static str, changed: bool) {
        self.paths_checked += 1;
        if changed {
            self.paths_changed += 1;
            *self.changed.entry(kind).or_default() += 1;
        }
    }

    fn finish(self) -> Repaired {
        let mut changed = Vec::with_capacity(KINDS.len());
        for kind in KINDS {
            changed.push(KindCount {
                count: self.changed.get(kind).copied().unwrap_or_default(),
                kind,
            });
        }
        Repaired {
            changed,
            paths_changed: self.paths_changed,
            paths_checked: self.paths_checked,
        }
    }
}

/// Narrow every path in the archive to the owner-only modes of the design.
///
/// `layout_dirs` is the archive's own list of layout directories, relative to
/// the root. The caller has already checked the root's shape, read the
/// marker, refused an unsupported schema version, refused a linked layout
/// directory, and taken the writer lock.
///
/// # Errors
///
/// Returns `path.symlink` when a path inside the archive is a symbolic link,
/// and `write.interrupted` when a directory cannot be read or a mode cannot
/// be set.
pub fn narrow_archive(root: &Path, layout_dirs: &[&str]) -> Result<Repaired, Diagnostic> {
    let mut counts = Counts::default();
    narrow(root, ".", DIRECTORY_MASK, "root", &mut counts)?;
    for (name, kind) in [(MARKER_FILE, "marker"), (LOCK_FILE, "lock")] {
        if root.join(name).exists() {
            narrow(&root.join(name), name, FILE_MASK, kind, &mut counts)?;
        }
    }
    for relative in layout_dirs {
        let path = root.join(relative);
        if path.exists() {
            narrow(&path, relative, DIRECTORY_MASK, "directory", &mut counts)?;
        }
        if let Some(kind) = contents_kind(relative) {
            walk(&path, relative, kind, MAX_DEPTH, &mut counts)?;
        }
    }
    Ok(counts.finish())
}

/// The kind of file a layout directory holds, when it holds files at all.
///
/// `objects/sha256` holds the fan-out directories and the stored objects
/// themselves, which are write-once and keep owner read alone. `objects` and
/// `records` hold only further layout directories, which the list already
/// names, so they are not walked twice.
fn contents_kind(relative: &str) -> Option<&'static str> {
    if relative == format!("{OBJECTS_DIR}/{ALGORITHM}") {
        return Some("object");
    }
    if relative == INCOMING_DIR {
        return Some("staging");
    }
    if relative == CACHE_DIR {
        return Some("cache");
    }
    relative
        .strip_prefix("records/")
        .filter(|kind| !kind.is_empty())
        .map(|_| "record")
}

/// The mode a file of this kind keeps.
fn mask_for(kind: &str) -> u32 {
    if kind == "object" {
        OBJECT_MASK
    } else {
        FILE_MASK
    }
}

/// The kind a file is counted under. A leftover staging file is openPapir's
/// own transient artefact inside the object store, so it counts as an object.
fn file_kind(kind: &'static str) -> &'static str {
    if kind == "staging" { "object" } else { kind }
}

/// Narrow every entry of a directory, and every entry below it.
fn walk(
    directory: &Path,
    relative: &str,
    kind: &'static str,
    depth: u32,
    counts: &mut Counts,
) -> Result<(), Diagnostic> {
    if depth == 0 {
        return Ok(());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(unreadable(relative)),
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| unreadable(relative))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    for name in names {
        let path = directory.join(&name);
        let child = format!("{relative}/{name}");
        let metadata = fs::symlink_metadata(&path).map_err(|_| unreadable(relative))?;
        if metadata.file_type().is_symlink() {
            return Err(linked(&child));
        }
        if metadata.is_dir() {
            narrow(&path, &child, DIRECTORY_MASK, "directory", counts)?;
            walk(&path, &child, kind, depth - 1, counts)?;
        } else {
            narrow(&path, &child, mask_for(kind), file_kind(kind), counts)?;
        }
    }
    Ok(())
}

/// Narrow one path, counting whether the mode actually changed.
fn narrow(
    path: &Path,
    relative: &str,
    mask: u32,
    kind: &'static str,
    counts: &mut Counts,
) -> Result<(), Diagnostic> {
    if paths::is_symlink(path) {
        return Err(linked(relative));
    }
    counts.note(kind, apply(path, mask, relative)?);
    Ok(())
}

/// Apply the mask, reporting whether anything changed.
///
/// On a platform without permission bits nothing is changed and nothing is
/// reported as changed: the caller reports `platform.owner_only_via_acl`
/// instead, because owner-only access there is an access-control list.
fn apply(path: &Path, mask: u32, relative: &str) -> Result<bool, Diagnostic> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let metadata = fs::symlink_metadata(path).map_err(|_| unreadable(relative))?;
        let mode = metadata.permissions().mode() & 0o7777;
        let narrowed = mode & mask;
        if narrowed == mode {
            return Ok(false);
        }
        fs::set_permissions(path, fs::Permissions::from_mode(narrowed))
            .map_err(|_| unreadable(relative))?;
        Ok(true)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mask, relative);
        Ok(false)
    }
}

/// The refusal for a path inside the archive that is a symbolic link.
fn linked(relative: &str) -> Diagnostic {
    paths::symlink_refusal(
        Details::new()
            .text("scope", "archive")
            .text("archive_path", relative.to_owned()),
    )
}

/// The refusal for a path the repair could not read or could not change.
fn unreadable(relative: &str) -> Diagnostic {
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "A path in the archive could not be read or narrowed.",
        Details::new()
            .text("stage", "record_write")
            .text("archive_path", relative.to_owned()),
    )
    .retryable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_reported_even_when_nothing_of_it_changed() {
        let repaired = Counts::default().finish();
        assert_eq!(repaired.changed.len(), KINDS.len());
        assert_eq!(repaired.paths_changed, 0);
        assert_eq!(repaired.paths_checked, 0);
        let kinds: Vec<&str> = repaired.changed.iter().map(|entry| entry.kind).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        assert_eq!(kinds, sorted, "the report is ordered by kind");
    }

    #[test]
    fn a_layout_directory_names_the_kind_of_file_it_holds() {
        assert_eq!(contents_kind("objects/sha256"), Some("object"));
        assert_eq!(contents_kind("objects/incoming"), Some("staging"));
        assert_eq!(contents_kind("records/cases"), Some("record"));
        assert_eq!(contents_kind("cache"), Some("cache"));
        assert_eq!(contents_kind("objects"), None);
        assert_eq!(contents_kind("records"), None);
        assert_eq!(mask_for("object"), OBJECT_MASK);
        assert_eq!(mask_for("record"), FILE_MASK);
        assert_eq!(file_kind("staging"), "object");
        assert_eq!(file_kind("record"), "record");
    }

    #[cfg(unix)]
    #[test]
    fn narrowing_clears_every_bit_outside_the_mask_and_widens_nothing() {
        use std::os::unix::fs::PermissionsExt as _;
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("wide");
        fs::write(&file, b"x").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o4777)).unwrap();
        assert!(apply(&file, FILE_MASK, "wide").unwrap());
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        assert!(!apply(&file, FILE_MASK, "wide").unwrap(), "already narrow");

        let unreadable_by_choice = home.path().join("closed");
        fs::write(&unreadable_by_choice, b"x").unwrap();
        fs::set_permissions(&unreadable_by_choice, fs::Permissions::from_mode(0o000)).unwrap();
        assert!(!apply(&unreadable_by_choice, FILE_MASK, "closed").unwrap());
        assert_eq!(
            fs::metadata(&unreadable_by_choice)
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o000,
            "a narrower mode is never widened to the mask"
        );

        let object = home.path().join("object");
        fs::write(&object, b"x").unwrap();
        fs::set_permissions(&object, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(apply(&object, OBJECT_MASK, "object").unwrap());
        assert_eq!(
            fs::metadata(&object).unwrap().permissions().mode() & 0o7777,
            0o400,
            "a stored object keeps owner read alone"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_refused_rather_than_narrowed() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("target");
        fs::write(&target, b"x").unwrap();
        let link = home.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut counts = Counts::default();
        let refusal = narrow(
            &link,
            "records/cases/link",
            FILE_MASK,
            "record",
            &mut counts,
        )
        .unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        assert_eq!(counts.paths_checked, 0);
    }

    #[test]
    fn an_unreadable_path_is_an_interrupted_write() {
        let refusal = unreadable("records/cases");
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert!(refusal.is_retryable());
        assert_eq!(refusal.exit_code(), 4);
    }
}
