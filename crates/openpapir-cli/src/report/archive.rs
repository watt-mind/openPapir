//! The lines the archive-wide commands print, `archive check` excepted.

use openpapir_core::archive::Created;

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

#[cfg(test)]
mod tests {
    use super::*;

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
