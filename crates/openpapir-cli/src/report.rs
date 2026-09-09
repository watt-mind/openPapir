//! Human-readable output, bound by the same privacy rule as the JSON.
//!
//! No line here may carry a user-supplied path, an original filename, or any
//! payload byte. What a line may carry is what `docs/error-contract.md`
//! allows: counts, byte lengths, digests of stored artefacts, and identifiers
//! openPapir minted itself.

use openpapir_core::archive::Created;
use openpapir_core::archive::import::Imported;

/// The lines `archive init` prints when it succeeds.
#[must_use]
pub fn created(created: &Created) -> Vec<String> {
    vec![
        "Archive created at the supplied root.".to_owned(),
        format!(
            "Archive identifier: {}. Schema version: {}.",
            created.archive_id, created.archive_schema_version
        ),
    ]
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use openpapir_core::Artefact;

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
    fn human_creation_output_reports_the_identifier_not_the_root() {
        let lines = created(&Created {
            archive_id: "0123456789abcdef0123456789abcdef".to_owned(),
            archive_schema_version: 1,
        });
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("0123456789abcdef0123456789abcdef"));
        assert!(!lines.join("\n").contains('/'));
    }
}
