//! Publishing a restored case, all of it or none of it.
//!
//! Every record this import writes is staged first, in the directory it will
//! be published into, and only then put in place, exactly as every other
//! write in the archive is (`docs/archive-layout.md`). Staging the whole set
//! before publishing any of it is what makes the record pass all or nothing:
//! a document that cannot be staged at all stops the import before a single
//! record is visible, and a publish that fails part way removes the records
//! this import had already published and the objects it had created.
//!
//! An import event is written for every object this import stored, with
//! `source` `export`, so an object restored from an export carries its own
//! provenance. The import event the export itself holds is restored as a
//! record beside it, so the history of the original import survives the round
//! trip rather than being replaced by it. An object the archive already held
//! keeps the events it already had and gains none, which is what makes a
//! second import of one export write nothing at all.
//!
//! The event openPapir writes here carries no original filename: the bytes
//! came from an export, which names its objects by digest alone and holds no
//! filename of its own. The filename the user's own import recorded stays
//! where it has always been, an attribute inside the restored import-event
//! record.

use std::fs;
use std::path::{Path, PathBuf};

use crate::archive::import::{EVENT_KIND, ImportEvent, SOURCE_EXPORT};
use crate::archive::write::Staging;
use crate::archive::{SUPPORTED_SCHEMA_VERSION, objects, paths};
use crate::clock;
use crate::error::{Diagnostic, Warning};
use crate::export::restore::objects::Placements;
use crate::export::restore::records::Plan;
use crate::ident;
use crate::records::document::{self, Record};

/// The stage every write of this pass reports when it is interrupted.
const STAGE: &str = "record_write";

/// What the record pass wrote.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Published {
    /// How many import events this import recorded, over and above the
    /// records the export itself holds.
    pub events: u64,
    /// How many of the export's own records this import wrote.
    pub records: u64,
}

/// One staged document, waiting for the pass to publish the whole set.
#[derive(Debug)]
struct Staged {
    archive_path: String,
    destination: PathBuf,
    staging: Staging,
}

/// Write every record of one import, or none of them.
///
/// # Errors
///
/// Returns `input.cap.record_size`, `internal.unexpected`, `path.symlink`,
/// `path.overwrite`, `path.cross_device`, `platform.filesystem_unsupported`,
/// or `write.interrupted`.
pub fn run(
    root: &Path,
    plan: &Plan,
    placed: &Placements,
    warnings: &mut Vec<Warning>,
) -> Result<Published, Diagnostic> {
    let events = events_for(placed)?;
    let mut staged = Vec::with_capacity(plan.absent.len() + events.len());
    for record in &plan.absent {
        staged.push(stage(
            root,
            record.directory(),
            record.id(),
            &record.document()?,
        )?);
    }
    for event in &events {
        staged.push(stage(
            root,
            ImportEvent::DIRECTORY,
            event.id(),
            &document::document(event)?,
        )?);
    }
    let mut published: Vec<PathBuf> = Vec::with_capacity(staged.len());
    for item in staged {
        match item
            .staging
            .publish(&item.destination, &item.archive_path, STAGE)
        {
            Ok(observed) => {
                warnings.extend(observed);
                published.push(item.destination);
            }
            Err(refusal) => {
                withdraw(&published);
                return Err(refusal);
            }
        }
    }
    Ok(Published {
        events: events.len() as u64,
        records: plan.absent.len() as u64,
    })
}

/// Remove the objects one failed import created.
///
/// Only an object this import created is removed, and only after its records
/// have gone, so nothing that survives names bytes the archive no longer
/// holds. An object that was already there is left exactly as it was: it
/// belongs to whatever put it there, not to this import.
pub fn discard(root: &Path, placed: &Placements) {
    for object in placed.created_objects() {
        let path = objects::absolute_path(root, &object.digest);
        #[cfg(not(unix))]
        let _ = paths::clear_read_only(&path);
        let _ = fs::remove_file(&path);
    }
}

/// Stage one document in the directory it will be published into.
///
/// The staged file is flushed and closed at once, so a case with many records
/// never holds one open handle per record while the set is being built.
fn stage(root: &Path, directory: &str, id: &str, content: &str) -> Result<Staged, Diagnostic> {
    let target = root.join(directory);
    let mut staging = Staging::create(&target, STAGE)?;
    let archive_path = format!("{directory}/{id}.json");
    use std::io::Write as _;
    staging
        .file()
        .write_all(content.as_bytes())
        .map_err(|error| paths::publish_refusal(&error, &archive_path, STAGE))?;
    staging.finish(STAGE)?;
    Ok(Staged {
        destination: target.join(format!("{id}.json")),
        archive_path,
        staging,
    })
}

/// Unlink every record this import had already published.
///
/// It is best effort. The import is already reporting a refusal of its own,
/// and a document that cannot be unlinked is not a second one; what it leaves
/// behind is a record of a case the archive already held every other record
/// of, which the integrity check reports rather than hides.
fn withdraw(published: &[PathBuf]) {
    for path in published {
        let _ = fs::remove_file(path);
    }
}

/// One import event per object this import stored, with `source` `export`.
fn events_for(placed: &Placements) -> Result<Vec<ImportEvent>, Diagnostic> {
    placed
        .created_objects()
        .map(|object| {
            Ok(ImportEvent {
                archive_schema_version: u64::from(SUPPORTED_SCHEMA_VERSION),
                byte_length: object.byte_length,
                created_object: true,
                digest: format!("{}:{}", objects::ALGORITHM, object.digest),
                id: ident::new_id()?,
                imported_at: clock::now_rfc3339(),
                original_filename: String::new(),
                record_kind: EVENT_KIND.to_owned(),
                source: Some(SOURCE_EXPORT.to_owned()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;
    use crate::export::restore::records::Held;
    use crate::records::case::Case;

    const CASE: &str = "0123456789abcdef0123456789abcdef";
    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    fn case() -> Case {
        Case {
            archive_schema_version: 1,
            created_at: "2026-01-17T12:00:00Z".to_owned(),
            id: CASE.to_owned(),
            notes: None,
            record_kind: "case".to_owned(),
            title: "Tax matter".to_owned(),
        }
    }

    fn archive_root(home: &Path) -> PathBuf {
        let root = home.join("archive");
        fs::create_dir(&root).unwrap();
        archive::init(&root).unwrap();
        root
    }

    #[test]
    fn a_stored_object_gets_one_event_whose_source_is_the_export() {
        let home = tempfile::tempdir().unwrap();
        let root = archive_root(home.path());
        let mut placed = Placements::default();
        placed.push_for_test(DIGEST, 16, true);
        placed.push_for_test(DIGEST, 16, false);
        let plan = Plan {
            absent: vec![Held::Case(case())],
            present: 0,
        };
        let mut warnings = Vec::new();
        let published = run(&root, &plan, &placed, &mut warnings).unwrap();
        assert_eq!(published.events, 1, "only a stored object gets an event");
        assert_eq!(
            published.records, 1,
            "the export's own records are counted apart"
        );
        let events = document::list_records::<ImportEvent>(&root).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.as_deref(), Some("export"));
        assert_eq!(events[0].digest, format!("sha256:{DIGEST}"));
        assert_eq!(
            events[0].original_filename, "",
            "an export holds no filename of its own"
        );
        assert!(
            root.join(Case::DIRECTORY)
                .join(format!("{CASE}.json"))
                .is_file()
        );
    }

    #[test]
    fn nothing_is_written_for_an_import_that_restores_nothing() {
        let home = tempfile::tempdir().unwrap();
        let root = archive_root(home.path());
        let mut warnings = Vec::new();
        let published = run(
            &root,
            &Plan {
                absent: Vec::new(),
                present: 3,
            },
            &Placements::default(),
            &mut warnings,
        )
        .unwrap();
        assert_eq!(published, Published::default());
        assert!(
            document::list_records::<ImportEvent>(&root)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_discarded_import_removes_only_the_objects_it_created() {
        let home = tempfile::tempdir().unwrap();
        let root = archive_root(home.path());
        let path = objects::absolute_path(&root, DIGEST);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"synthetic bytes\n").unwrap();
        let mut placed = Placements::default();
        placed.push_for_test(DIGEST, 16, false);
        discard(&root, &placed);
        assert!(path.is_file(), "an object that was already there stays");

        let mut placed = Placements::default();
        placed.push_for_test(DIGEST, 16, true);
        discard(&root, &placed);
        assert!(!path.exists(), "an object this import created goes");
    }
}
