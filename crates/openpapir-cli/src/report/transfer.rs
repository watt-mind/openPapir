//! The lines the commands that move artefacts between an archive and a
//! directory print: `import`, `case export`, `case import`, `archive export`,
//! `archive import`, and the permission repair that follows one out of an
//! archive that was written to elsewhere.

use openpapir_core::archive::import::Imported;
use openpapir_core::{ArchiveExported, ArchiveRestored, Exported, Repaired, Restored};

/// The lines `import` prints when it succeeds.
#[must_use]
pub fn imported(imported: &Imported) -> Vec<String> {
    let mut lines = vec![format!(
        "Stored {} artefact(s); {} already present.",
        imported.imported, imported.duplicates
    )];
    for artefact in &imported.artefacts {
        let state = if artefact.created_object {
            "stored".to_owned()
        } else {
            format!(
                "already present since {} after {} earlier import(s)",
                artefact
                    .first_imported_at
                    .as_deref()
                    .unwrap_or("an unrecorded time"),
                artefact.previous_import_count.unwrap_or(0)
            )
        };
        lines.push(format!(
            "{} ({} bytes), import event {}, {state}.",
            artefact.digest, artefact.byte_length, artefact.import_event
        ));
    }
    lines.push(
        "A digest identifies bytes only. Nothing here is verified, matched, or delivered."
            .to_owned(),
    );
    lines
}

/// The lines `case export` prints when it succeeds.
///
/// The destination is the one path any output here may carry: the user
/// supplied it in this invocation, and the line repeats their own argument
/// back to them. It never reaches the JSON envelope, where the privacy rule
/// admits no user-supplied path at all.
#[must_use]
pub fn exported(exported: &Exported) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Exported case {} to {}.",
            exported.case_id, exported.destination
        ),
        format!(
            "Copied {} object(s), {} byte(s), and wrote {} record(s).",
            exported.object_count, exported.bytes_copied, exported.record_count
        ),
    ];
    for kind in &exported.records {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(
        "The archive was not changed. Every copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}

/// The lines `archive export` prints when it succeeds.
///
/// It is the case export's report with the one case replaced by a count of
/// them, and the destination is the one path any line here may carry, for the
/// reason [`exported`] may carry it.
#[must_use]
pub fn archive_exported(exported: &ArchiveExported) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Exported {} case(s) to {}.",
            exported.case_count, exported.destination
        ),
        format!(
            "Copied {} object(s), {} byte(s), and wrote {} record(s).",
            exported.object_count, exported.bytes_copied, exported.record_count
        ),
    ];
    for kind in &exported.records {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(
        "The archive marker travelled with the copy, so the schema version is in the export."
            .to_owned(),
    );
    lines.push(
        "The archive was not changed. Every copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}

/// The lines `archive import` prints when it succeeds.
#[must_use]
pub fn archive_restored(restored: &ArchiveRestored) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Imported {} case(s) from {}.",
            restored.case_count, restored.source
        ),
        format!(
            "Stored {} object(s), {} byte(s); {} already present.",
            restored.objects_stored, restored.bytes_stored, restored.objects_present
        ),
        format!(
            "Wrote {} record(s); {} already present.",
            restored.records_written, restored.records_present
        ),
    ];
    for kind in &restored.records {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(format!(
        "Recorded {} import event(s) with source export.",
        restored.events_recorded
    ));
    lines.push(
        "Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}

/// The lines `archive repair-permissions` prints when it succeeds.
#[must_use]
pub fn repaired(repaired: &Repaired) -> Vec<String> {
    let mut lines = vec![format!(
        "Narrowed {} of {} archive path(s) to owner-only.",
        repaired.paths_changed, repaired.paths_checked
    )];
    for kind in &repaired.changed {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(
        "Permissions are only ever narrowed here; nothing was widened and no content was read or changed."
            .to_owned(),
    );
    lines
}

/// The lines `case import` prints when it succeeds.
///
/// The source is the one thing here the JSON does not carry, exactly as the
/// destination is for an export: the line repeats the `--from` argument the
/// user typed in the same invocation and nothing else ever echoes it.
#[must_use]
pub fn restored(restored: &Restored) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Imported case {} from {}.",
            restored.case_id, restored.source
        ),
        format!(
            "Stored {} object(s), {} byte(s); {} already present.",
            restored.objects_stored, restored.bytes_stored, restored.objects_present
        ),
        format!(
            "Wrote {} record(s); {} already present.",
            restored.records_written, restored.records_present
        ),
    ];
    for kind in &restored.records {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(format!(
        "Recorded {} import event(s) with source export.",
        restored.events_recorded
    ));
    lines.push(
        "Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpapir_core::{Artefact, KindCount};

    #[test]
    fn human_import_output_names_no_file_and_promises_nothing() {
        let lines = imported(&Imported {
            imported: 1,
            duplicates: 1,
            artefacts: vec![
                Artefact {
                    digest: "sha256:aa".to_owned(),
                    byte_length: 4,
                    import_event: "0123".to_owned(),
                    created_object: true,
                    previous_import_count: None,
                    first_imported_at: None,
                },
                Artefact {
                    digest: "sha256:bb".to_owned(),
                    byte_length: 4,
                    import_event: "4567".to_owned(),
                    created_object: false,
                    previous_import_count: Some(2),
                    first_imported_at: Some("2026-01-14T09:12:33Z".to_owned()),
                },
            ],
        });
        let text = lines.join("\n");
        assert!(text.contains("Stored 1 artefact(s); 1 already present."));
        assert!(text.contains("already present since 2026-01-14T09:12:33Z after 2"));
        assert!(text.contains("Nothing here is verified"));
        assert!(!text.contains('/'), "no path ever reaches human output");
    }

    #[test]
    fn human_archive_transfer_output_counts_cases_and_names_no_path_of_its_own() {
        let exported = archive_exported(&ArchiveExported {
            bytes_copied: 8,
            case_count: 3,
            object_count: 2,
            record_count: 9,
            records: vec![KindCount {
                count: 3,
                kind: "case",
            }],
            destination: "backups/archive".to_owned(),
        });
        let text = exported.join("\n");
        assert!(text.contains("Exported 3 case(s) to backups/archive."));
        assert!(text.contains("Copied 2 object(s), 8 byte(s), and wrote 9 record(s)."));
        assert!(text.contains("case 3"));
        assert!(text.contains("The archive marker travelled with the copy"));
        assert!(text.contains("never authenticity, delivery, or legal effect"));

        let restored = archive_restored(&ArchiveRestored {
            bytes_stored: 8,
            case_count: 3,
            events_recorded: 2,
            object_count: 2,
            objects_present: 0,
            objects_stored: 2,
            records: vec![KindCount {
                count: 3,
                kind: "case",
            }],
            records_present: 1,
            records_written: 8,
            source: "backups/archive".to_owned(),
        });
        let text = restored.join("\n");
        assert!(text.contains("Imported 3 case(s) from backups/archive."));
        assert!(text.contains("Stored 2 object(s), 8 byte(s); 0 already present."));
        assert!(text.contains("Wrote 8 record(s); 1 already present."));
        assert!(text.contains("Recorded 2 import event(s) with source export."));
    }

    #[test]
    fn human_case_import_output_repeats_only_the_source_the_user_typed() {
        let lines = restored(&Restored {
            case_id: "0123456789abcdef0123456789abcdef".to_owned(),
            source: "exports/case".to_owned(),
            objects_stored: 2,
            bytes_stored: 8,
            object_count: 3,
            objects_present: 1,
            records_written: 3,
            records_present: 1,
            records: vec![KindCount {
                count: 1,
                kind: "submission",
            }],
            events_recorded: 2,
        });
        let text = lines.join("\n");
        assert!(text.contains("Imported case 0123456789abcdef0123456789abcdef from exports/case."));
        assert!(text.contains("Stored 2 object(s), 8 byte(s); 1 already present."));
        assert!(text.contains("Wrote 3 record(s); 1 already present."));
        assert!(text.contains("submission 1"));
        assert!(text.contains("Recorded 2 import event(s) with source export."));
        assert!(text.contains("never authenticity, delivery, or legal effect"));
    }
}
