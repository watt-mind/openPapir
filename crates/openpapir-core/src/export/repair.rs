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
//!
//! The lock file is deliberately not inspected. The repair holds the writer
//! lock while it runs, so the only lock file that can exist while it walks is
//! the one it created itself, owner-only by construction; a lock file another
//! writer left refuses the repair with `lock.held` before the walk begins. A
//! `lock` count would therefore always be zero, and the report says only what
//! it actually looked at.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::archive::objects::{ALGORITHM, INCOMING_DIR, OBJECTS_DIR};
use crate::archive::paths;
use crate::archive::{CACHE_DIR, MARKER_FILE};
use crate::error::{Details, Diagnostic, codes};
use crate::export::KindCount;

/// A stage of the write bucket, the three the error contract names.
///
/// The stage names what was being written, never which module reported it
/// (`docs/error-contract.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// A stored object, or a leftover staging file inside the object store.
    ObjectWrite,
    /// A record document, a cached file, a layout directory, or the root.
    RecordWrite,
    /// The archive marker.
    MarkerWrite,
}

impl Stage {
    /// Every stage, in the order the error contract's table lists them.
    pub const ALL: [Self; 3] = [Self::ObjectWrite, Self::RecordWrite, Self::MarkerWrite];

    /// The name the `stage` detail carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObjectWrite => "object_write",
            Self::RecordWrite => "record_write",
            Self::MarkerWrite => "marker_write",
        }
    }
}

/// A kind of path the repair inspects.
///
/// The set is closed, so a new kind cannot reach a stage by falling through a
/// catch-all: [`Kind::stage`] matches every variant by name. Six of the
/// variants are reported by name in [`KINDS`]; `Staging` is not, because a
/// leftover staging file is openPapir's own transient artefact inside the
/// object store and counts as an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A cached file.
    Cache,
    /// A layout directory, or a directory inside one.
    Directory,
    /// The archive marker.
    Marker,
    /// A stored object.
    Object,
    /// A record document.
    Record,
    /// The archive root.
    Root,
    /// A leftover staging file inside the object store.
    Staging,
}

impl Kind {
    /// The name the report uses for this kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Directory => "directory",
            Self::Marker => "marker",
            Self::Object => "object",
            Self::Record => "record",
            Self::Root => "root",
            Self::Staging => "staging",
        }
    }

    /// The stage a refusal reports while this kind of path is inspected.
    ///
    /// The match is exhaustive by variant, so adding a kind is a compile
    /// error until its stage is decided.
    #[must_use]
    pub const fn stage(self) -> Stage {
        match self {
            Self::Object | Self::Staging => Stage::ObjectWrite,
            Self::Marker => Stage::MarkerWrite,
            Self::Cache | Self::Directory | Self::Record | Self::Root => Stage::RecordWrite,
        }
    }

    /// The kind this one is counted under in the report.
    ///
    /// A leftover staging file counts as an object, so `Staging` never
    /// reaches the report.
    #[must_use]
    pub const fn reported(self) -> Self {
        match self {
            Self::Staging => Self::Object,
            other => other,
        }
    }
}

/// The kinds of path the repair reports, in the order it reports them.
pub const REPORTED: [Kind; 6] = [
    Kind::Cache,
    Kind::Directory,
    Kind::Marker,
    Kind::Object,
    Kind::Record,
    Kind::Root,
];

/// The names of [`REPORTED`], in the same order: the report's kind column.
pub const KINDS: [&str; REPORTED.len()] = {
    let mut names = [""; REPORTED.len()];
    let mut index = 0;
    while index < REPORTED.len() {
        names[index] = REPORTED[index].name();
        index += 1;
    }
    names
};

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
    changed: BTreeMap<Kind, u64>,
    paths_changed: u64,
    paths_checked: u64,
}

impl Counts {
    fn note(&mut self, kind: Kind, changed: bool) {
        self.paths_checked += 1;
        if changed {
            self.paths_changed += 1;
            *self.changed.entry(kind.reported()).or_default() += 1;
        }
    }

    fn finish(self) -> Repaired {
        let mut changed = Vec::with_capacity(REPORTED.len());
        for kind in REPORTED {
            changed.push(KindCount {
                count: self.changed.get(&kind).copied().unwrap_or_default(),
                kind: kind.name(),
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
    narrow(root, ".", DIRECTORY_MASK, Kind::Root, &mut counts)?;
    if root.join(MARKER_FILE).exists() {
        narrow(
            &root.join(MARKER_FILE),
            MARKER_FILE,
            FILE_MASK,
            Kind::Marker,
            &mut counts,
        )?;
    }
    for relative in layout_dirs {
        let path = root.join(relative);
        if path.exists() {
            narrow(
                &path,
                relative,
                DIRECTORY_MASK,
                Kind::Directory,
                &mut counts,
            )?;
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
fn contents_kind(relative: &str) -> Option<Kind> {
    if relative == format!("{OBJECTS_DIR}/{ALGORITHM}") {
        return Some(Kind::Object);
    }
    if relative == INCOMING_DIR {
        return Some(Kind::Staging);
    }
    if relative == CACHE_DIR {
        return Some(Kind::Cache);
    }
    relative
        .strip_prefix("records/")
        .filter(|kind| !kind.is_empty())
        .map(|_| Kind::Record)
}

/// The mode a file of this kind keeps.
const fn mask_for(kind: Kind) -> u32 {
    match kind {
        Kind::Object => OBJECT_MASK,
        Kind::Cache
        | Kind::Directory
        | Kind::Marker
        | Kind::Record
        | Kind::Root
        | Kind::Staging => FILE_MASK,
    }
}

/// Narrow every entry of a directory, and every entry below it.
fn walk(
    directory: &Path,
    relative: &str,
    kind: Kind,
    depth: u32,
    counts: &mut Counts,
) -> Result<(), Diagnostic> {
    if depth == 0 {
        return Ok(());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(unreadable(relative, kind.stage())),
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| unreadable(relative, kind.stage()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    for name in names {
        let path = directory.join(&name);
        let child = format!("{relative}/{name}");
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| unreadable(relative, kind.stage()))?;
        if metadata.file_type().is_symlink() {
            return Err(linked(&child));
        }
        if metadata.is_dir() {
            narrow(&path, &child, DIRECTORY_MASK, Kind::Directory, counts)?;
            walk(&path, &child, kind, depth - 1, counts)?;
        } else {
            narrow(&path, &child, mask_for(kind), kind, counts)?;
        }
    }
    Ok(())
}

/// Narrow one path, counting whether the mode actually changed.
fn narrow(
    path: &Path,
    relative: &str,
    mask: u32,
    kind: Kind,
    counts: &mut Counts,
) -> Result<(), Diagnostic> {
    if paths::is_symlink(path) {
        return Err(linked(relative));
    }
    counts.note(kind, apply(path, mask, relative, kind.stage())?);
    Ok(())
}

/// Apply the mask, reporting whether anything changed.
///
/// On a platform without permission bits nothing is changed and nothing is
/// reported as changed: the caller reports `platform.owner_only_via_acl`
/// instead, because owner-only access there is an access-control list.
fn apply(path: &Path, mask: u32, relative: &str, stage: Stage) -> Result<bool, Diagnostic> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let metadata = fs::symlink_metadata(path).map_err(|_| unreadable(relative, stage))?;
        let mode = metadata.permissions().mode() & 0o7777;
        let narrowed = mode & mask;
        if narrowed == mode {
            return Ok(false);
        }
        fs::set_permissions(path, fs::Permissions::from_mode(narrowed))
            .map_err(|_| unreadable(relative, stage))?;
        Ok(true)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mask, relative, stage);
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
///
/// The stage is the kind of path the repair was inspecting, so a stored
/// object it could not narrow is reported as an object write rather than as a
/// record write.
fn unreadable(relative: &str, stage: Stage) -> Diagnostic {
    Diagnostic::new(
        codes::WRITE_INTERRUPTED,
        "A path in the archive could not be read or narrowed.",
        Details::new()
            .text("stage", stage.as_str())
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
        assert_eq!(repaired.changed.len(), REPORTED.len());
        assert_eq!(
            KINDS,
            ["cache", "directory", "marker", "object", "record", "root"],
            "the report's kind names and their order are the contract"
        );
        assert!(
            !KINDS.contains(&"lock"),
            "the lock is never inspected, so it is never reported"
        );
        assert_eq!(repaired.paths_changed, 0);
        assert_eq!(repaired.paths_checked, 0);
        let kinds: Vec<&str> = repaired.changed.iter().map(|entry| entry.kind).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        assert_eq!(kinds, sorted, "the report is ordered by kind");
    }

    #[test]
    fn a_layout_directory_names_the_kind_of_file_it_holds() {
        assert_eq!(contents_kind("objects/sha256"), Some(Kind::Object));
        assert_eq!(contents_kind("objects/incoming"), Some(Kind::Staging));
        assert_eq!(contents_kind("records/cases"), Some(Kind::Record));
        assert_eq!(contents_kind("cache"), Some(Kind::Cache));
        assert_eq!(contents_kind("objects"), None);
        assert_eq!(contents_kind("records"), None);
        assert_eq!(mask_for(Kind::Object), OBJECT_MASK);
        assert_eq!(mask_for(Kind::Record), FILE_MASK);
        assert_eq!(mask_for(Kind::Staging), FILE_MASK);
        assert_eq!(Kind::Staging.reported(), Kind::Object);
        assert_eq!(Kind::Record.reported(), Kind::Record);
    }

    #[test]
    fn a_staging_file_is_counted_as_an_object_and_never_reported_by_its_own_name() {
        let mut counts = Counts::default();
        counts.note(Kind::Staging, true);
        let repaired = counts.finish();
        let object = repaired
            .changed
            .iter()
            .find(|entry| entry.kind == "object")
            .expect("object is reported");
        assert_eq!(object.count, 1);
        assert!(
            !KINDS.contains(&Kind::Staging.name()),
            "staging is a walk kind, never a reported one"
        );
    }

    #[cfg(unix)]
    #[test]
    fn narrowing_clears_every_bit_outside_the_mask_and_widens_nothing() {
        use std::os::unix::fs::PermissionsExt as _;
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("wide");
        fs::write(&file, b"x").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o4777)).unwrap();
        assert!(apply(&file, FILE_MASK, "wide", Stage::RecordWrite).unwrap());
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        assert!(
            !apply(&file, FILE_MASK, "wide", Stage::RecordWrite).unwrap(),
            "already narrow"
        );

        let unreadable_by_choice = home.path().join("closed");
        fs::write(&unreadable_by_choice, b"x").unwrap();
        fs::set_permissions(&unreadable_by_choice, fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            !apply(
                &unreadable_by_choice,
                FILE_MASK,
                "closed",
                Stage::RecordWrite
            )
            .unwrap()
        );
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
        assert!(apply(&object, OBJECT_MASK, "object", Stage::ObjectWrite).unwrap());
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
            Kind::Record,
            &mut counts,
        )
        .unwrap_err();
        assert_eq!(refusal.code, codes::PATH_SYMLINK);
        assert_eq!(counts.paths_checked, 0);
    }

    #[test]
    fn an_unreadable_path_is_an_interrupted_write() {
        let refusal = unreadable("records/cases", Stage::RecordWrite);
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        assert!(refusal.is_retryable());
        assert_eq!(refusal.exit_code(), 4);
    }

    #[test]
    fn a_refusal_reports_the_kind_of_path_the_repair_was_inspecting() {
        assert_eq!(Kind::Object.stage(), Stage::ObjectWrite);
        assert_eq!(Kind::Staging.stage(), Stage::ObjectWrite);
        assert_eq!(Kind::Marker.stage(), Stage::MarkerWrite);
        for kind in [Kind::Cache, Kind::Directory, Kind::Record, Kind::Root] {
            assert_eq!(kind.stage(), Stage::RecordWrite);
        }
        // The set of kinds is closed, so listing every one of them here is
        // the whole mapping: a kind added without a stage is a compile
        // error, and a kind added without a line here fails this assertion.
        let all = [
            Kind::Cache,
            Kind::Directory,
            Kind::Marker,
            Kind::Object,
            Kind::Record,
            Kind::Root,
            Kind::Staging,
        ];
        for kind in all {
            assert!(
                Stage::ALL.contains(&kind.stage()),
                "{} maps to a stage the write bucket names",
                kind.name()
            );
        }
        for kind in REPORTED {
            assert!(all.contains(&kind), "{} is one of the kinds", kind.name());
        }
        assert_eq!(
            Stage::ALL.map(Stage::as_str),
            ["object_write", "record_write", "marker_write"]
        );
    }

    #[test]
    fn an_object_the_repair_cannot_list_is_reported_as_an_object_write() {
        let home = tempfile::tempdir().unwrap();
        // A fan-out path that is a file rather than a directory cannot be
        // listed, which is the same failure an unreadable one produces.
        let fan_out = home.path().join("ab");
        fs::write(&fan_out, b"x").unwrap();
        let mut counts = Counts::default();
        let refusal = walk(
            &fan_out,
            "objects/sha256/ab",
            Kind::Object,
            MAX_DEPTH,
            &mut counts,
        )
        .unwrap_err();
        assert_eq!(refusal.code, codes::WRITE_INTERRUPTED);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["stage"], "object_write");
        assert_eq!(json["details"]["archive_path"], "objects/sha256/ab");

        let mut counts = Counts::default();
        let refusal = walk(
            &fan_out,
            "records/cases",
            Kind::Record,
            MAX_DEPTH,
            &mut counts,
        )
        .unwrap_err();
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["stage"],
            "record_write"
        );
    }
}
