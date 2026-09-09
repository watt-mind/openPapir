//! Case and submission records: the user's own organisation of an archive.
//!
//! A case is a user-created folder of related correspondence. It corresponds
//! to nothing any government service issues. A submission is something the
//! user states they sent, recorded from what the user has locally: openPapir
//! sends nothing, so a submission is always user-asserted
//! (`docs/archive-layout.md`).
//!
//! Records reference each other, and reference stored artefacts, by identifier
//! only. Nothing here asserts that anything was received anywhere, and no
//! field, value, or message may be read as delivery, receipt by an authority,
//! authenticity, or legal effect.
//!
//! # Field caps
//!
//! Every user-supplied field is bounded in bytes, and the bound is checked
//! before the record is built. The caps are never relaxed to make one
//! particular input succeed (`AGENTS.md`).
//!
//! | Field | Cap | Shape |
//! | --- | --- | --- |
//! | `title` | 200 bytes | Required, single line |
//! | `notes` | 4096 bytes | Optional, newlines allowed |
//! | `description` | 1024 bytes | Required, newlines allowed |
//! | `role` | 64 bytes | Optional, single line |

pub mod case;
pub mod document;
pub mod submission;

use crate::error::{Details, Diagnostic, codes};

/// The directory holding case records, relative to the archive root.
pub const CASES_DIR: &str = "records/cases";
/// The directory holding submission records, relative to the archive root.
pub const SUBMISSIONS_DIR: &str = "records/submissions";

/// The largest case title, in bytes.
pub const MAX_TITLE_BYTES: u64 = 200;
/// The largest case notes field, in bytes.
pub const MAX_NOTES_BYTES: u64 = 4 * 1024;
/// The largest submission description, in bytes.
pub const MAX_DESCRIPTION_BYTES: u64 = 1024;
/// The largest artefact role label, in bytes.
pub const MAX_ROLE_BYTES: u64 = 64;

/// Whether a field may carry a line break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// One line: no control character at all.
    SingleLine,
    /// Several lines: no control character other than a line feed.
    MultiLine,
}

/// Refuse a user-supplied field longer than its cap.
///
/// The refusal names the field and the two lengths, never the value, and the
/// check runs before the record is built.
fn refuse_length(field: &'static str, observed: u64, cap: u64) -> Diagnostic {
    Diagnostic::new(
        codes::INPUT_CAP_FIELD_LENGTH,
        "A record field exceeds its length cap.",
        Details::new()
            .text("field", field)
            .int("cap_bytes", cap)
            .int("observed_bytes", observed),
    )
}

/// Refuse a field whose content openPapir cannot store as written.
fn refuse_content(field: &'static str, message: &str) -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        message,
        Details::new().text("argument", field),
    )
}

/// Check one field's cap and shape, returning it verbatim.
fn checked(field: &'static str, value: &str, cap: u64, shape: Shape) -> Result<String, Diagnostic> {
    let observed = value.len() as u64;
    if observed > cap {
        return Err(refuse_length(field, observed, cap));
    }
    let permitted = |character: char| {
        !character.is_control() || (shape == Shape::MultiLine && character == '\n')
    };
    if !value.chars().all(permitted) {
        return Err(refuse_content(
            field,
            "A record field carries a control character it may not carry.",
        ));
    }
    Ok(value.to_owned())
}

/// Check a required field, which may not be empty or only whitespace.
///
/// # Errors
///
/// Returns `input.cap.field_length` when the field exceeds its cap and
/// `usage.arguments` when it is empty or carries a forbidden character.
fn required(
    field: &'static str,
    value: &str,
    cap: u64,
    shape: Shape,
) -> Result<String, Diagnostic> {
    let value = checked(field, value, cap, shape)?;
    if value.trim().is_empty() {
        return Err(refuse_content(field, "A required record field is empty."));
    }
    Ok(value)
}

/// Check an optional field, treating an empty one as absent.
///
/// # Errors
///
/// Returns the same refusals as [`required`], without the empty-field one.
fn optional(
    field: &'static str,
    value: Option<&str>,
    cap: u64,
    shape: Shape,
) -> Result<Option<String>, Diagnostic> {
    match value {
        None => Ok(None),
        Some(value) => {
            let value = checked(field, value, cap, shape)?;
            Ok((!value.is_empty()).then_some(value))
        }
    }
}

/// Check a case title: required, single line, at most 200 bytes.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments`.
pub fn checked_title(value: &str) -> Result<String, Diagnostic> {
    required("title", value, MAX_TITLE_BYTES, Shape::SingleLine)
}

/// Check case notes: optional, newlines allowed, at most 4 KiB.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments`.
pub fn checked_notes(value: Option<&str>) -> Result<Option<String>, Diagnostic> {
    optional("notes", value, MAX_NOTES_BYTES, Shape::MultiLine)
}

/// Check a submission description: required, newlines allowed, at most 1 KiB.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments`.
pub fn checked_description(value: &str) -> Result<String, Diagnostic> {
    required(
        "description",
        value,
        MAX_DESCRIPTION_BYTES,
        Shape::MultiLine,
    )
}

/// Check an artefact role label: optional, single line, at most 64 bytes.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments`.
pub fn checked_role(value: Option<&str>) -> Result<Option<String>, Diagnostic> {
    optional("role", value, MAX_ROLE_BYTES, Shape::SingleLine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cap_is_refused_with_its_field_and_its_two_lengths() {
        for (refusal, field, cap) in [
            (
                checked_title(&"t".repeat(201)).err(),
                "title",
                MAX_TITLE_BYTES,
            ),
            (
                checked_notes(Some(&"n".repeat(4097))).err(),
                "notes",
                MAX_NOTES_BYTES,
            ),
            (
                checked_description(&"d".repeat(1025)).err(),
                "description",
                MAX_DESCRIPTION_BYTES,
            ),
            (
                checked_role(Some(&"r".repeat(65))).err(),
                "role",
                MAX_ROLE_BYTES,
            ),
        ] {
            let refusal = refusal.expect("the cap refuses");
            assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
            assert_eq!(refusal.exit_code(), 3);
            assert!(!refusal.is_retryable(), "a cap is never retried");
            let json = serde_json::to_value(&refusal).unwrap();
            assert_eq!(json["details"]["field"], field);
            assert_eq!(json["details"]["cap_bytes"], cap);
            assert_eq!(json["details"]["bucket"], "input");
        }
    }

    #[test]
    fn a_field_at_its_cap_is_accepted_and_stored_verbatim() {
        assert_eq!(checked_title(&"t".repeat(200)).unwrap().len(), 200);
        assert_eq!(
            checked_notes(Some(&"n".repeat(4096)))
                .unwrap()
                .unwrap()
                .len(),
            4096
        );
        assert_eq!(checked_description(&"d".repeat(1024)).unwrap().len(), 1024);
        assert_eq!(
            checked_role(Some(&"r".repeat(64))).unwrap().unwrap().len(),
            64
        );
        assert_eq!(checked_title("  Tax matter ").unwrap(), "  Tax matter ");
    }

    #[test]
    fn a_cap_counts_bytes_rather_than_characters() {
        let two_bytes_each = "á".repeat(101);
        assert_eq!(two_bytes_each.len(), 202);
        let refusal = checked_title(&two_bytes_each).unwrap_err();
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["observed_bytes"],
            202
        );
        assert!(checked_title(&"á".repeat(100)).is_ok());
    }

    #[test]
    fn a_single_line_field_refuses_a_control_character() {
        for value in ["one\ntwo", "one\ttwo", "one\rtwo", "one\u{0}two"] {
            let refusal = checked_title(value).unwrap_err();
            assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
            assert_eq!(refusal.exit_code(), 2);
            assert_eq!(
                serde_json::to_value(&refusal).unwrap()["details"]["argument"],
                "title"
            );
        }
        assert_eq!(
            checked_role(Some("a\nb")).unwrap_err().code,
            codes::USAGE_ARGUMENTS
        );
    }

    #[test]
    fn a_multi_line_field_permits_a_line_feed_and_nothing_else() {
        assert_eq!(
            checked_description("first line\nsecond line").unwrap(),
            "first line\nsecond line"
        );
        assert_eq!(
            checked_notes(Some("a\nb")).unwrap(),
            Some("a\nb".to_owned())
        );
        assert_eq!(
            checked_description("a\tb").unwrap_err().code,
            codes::USAGE_ARGUMENTS
        );
    }

    #[test]
    fn a_required_field_may_not_be_empty_and_an_optional_one_may() {
        for value in ["", "   ", "\n"] {
            assert_eq!(
                checked_description(value).unwrap_err().code,
                codes::USAGE_ARGUMENTS
            );
        }
        assert_eq!(checked_title("").unwrap_err().code, codes::USAGE_ARGUMENTS);
        assert_eq!(checked_notes(Some("")).unwrap(), None);
        assert_eq!(checked_notes(None).unwrap(), None);
        assert_eq!(checked_role(None).unwrap(), None);
        assert_eq!(checked_role(Some("")).unwrap(), None);
    }
}
