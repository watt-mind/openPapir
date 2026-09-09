//! Submission records: something the user states they sent.
//!
//! openPapir sends nothing, so a submission is always imported or
//! user-asserted (`docs/archive-layout.md`). It never asserts that anything
//! was received anywhere. Its optional date is the user's own statement about
//! their submission: it is stored verbatim, never interpreted, never compared
//! with the timestamp openPapir recorded, and never described as a delivery or
//! receipt date.
//!
//! A submission references its case by identifier and each artefact by digest.
//! The digest is a storage-layer identity only: it says the bytes are present
//! in this archive, and nothing about authenticity, origin, or legal effect.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::lock::WriterLock;
use crate::archive::{Archive, SUPPORTED_SCHEMA_VERSION, objects, paths};
use crate::clock;
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::ident;
use crate::records::case::Case;
use crate::records::document::{self, Record};
use crate::records::{SUBMISSIONS_DIR, checked_description, checked_role};

/// The value a submission record carries in `record_kind`.
pub const KIND: &str = "submission";

/// The digest form a submission may reference.
const DIGEST_PREFIX: &str = "sha256:";
/// The number of hexadecimal characters a SHA-256 digest renders as.
const DIGEST_LENGTH: usize = 64;

/// One artefact reference: the stored bytes and the label the user gave them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtefactRef {
    /// The algorithm-qualified digest of an object stored in this archive.
    pub digest: String,
    /// The user's own short label, absent when none was supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// One submission record, stored as `records/submissions/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u32,
    /// The artefacts the user attached to this submission, possibly none.
    pub artefacts: Vec<ArtefactRef>,
    /// The identifier of the case this submission belongs to.
    pub case_id: String,
    /// When openPapir recorded the submission.
    pub created_at: String,
    /// The user's own description of what they state they sent.
    pub description: String,
    /// The submission's own identifier, minted by openPapir.
    pub id: String,
    /// The record kind, always `submission`.
    pub record_kind: String,
    /// The user's own date, stored verbatim and never interpreted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stated_date: Option<String>,
}

impl Record for Submission {
    const KIND: &'static str = KIND;
    const DIRECTORY: &'static str = SUBMISSIONS_DIR;

    fn id(&self) -> &str {
        &self.id
    }

    fn record_kind(&self) -> &str {
        &self.record_kind
    }
}

/// What adding a submission reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmissionAdded {
    /// The submission as it was stored.
    pub submission: Submission,
}

/// Record a submission against a case.
///
/// `artefacts` holds one entry per reference, each `<digest>` or
/// `<digest>:<role>`. Every field cap is checked before the archive is
/// opened, and every reference is resolved before anything is written.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments` for a field that
/// breaks its cap or shape, `record.not_found` when the case identifier names
/// no case or a digest names no stored object, `record.malformed` for an
/// unreadable case document, and any archive, lock, path, or write refusal.
pub fn add(
    root: &Path,
    case_id: &str,
    description: &str,
    stated_date: Option<&str>,
    artefacts: &[String],
) -> Result<SubmissionAdded> {
    let mut warnings = Vec::new();
    match add_record(
        root,
        case_id,
        description,
        stated_date,
        artefacts,
        &mut warnings,
    ) {
        Ok(submission) => Ok(Outcome {
            data: SubmissionAdded { submission },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn add_record(
    root: &Path,
    case_id: &str,
    description: &str,
    stated_date: Option<&str>,
    artefacts: &[String],
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Submission, Diagnostic> {
    let description = checked_description(description)?;
    let stated_date = checked_stated_date(stated_date)?;
    let references = parse_references(artefacts)?;

    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let case = document::read_record::<Case>(archive.root(), case_id, "case_id")?;
    for reference in &references {
        refuse_absent_object(archive.root(), &reference.digest)?;
    }
    let submission = Submission {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        artefacts: references,
        case_id: case.id,
        created_at: clock::now_rfc3339(),
        description,
        id: ident::new_id()?,
        record_kind: KIND.to_owned(),
        stated_date,
    };
    warnings.extend(document::write_record(archive.root(), &submission)?);
    Ok(submission)
}

/// Parse `<digest>[:<role>]` for each reference, checking shape and caps.
fn parse_references(artefacts: &[String]) -> std::result::Result<Vec<ArtefactRef>, Diagnostic> {
    let mut references = Vec::with_capacity(artefacts.len());
    for entry in artefacts {
        let Some(rest) = entry.strip_prefix(DIGEST_PREFIX) else {
            return Err(unusable_reference());
        };
        let (hex, role) = match rest.split_once(':') {
            Some((hex, role)) => (hex, Some(role)),
            None => (rest, None),
        };
        if !is_digest(hex) {
            return Err(unusable_reference());
        }
        references.push(ArtefactRef {
            digest: format!("{DIGEST_PREFIX}{hex}"),
            role: checked_role(role)?,
        });
    }
    Ok(references)
}

/// Whether a value is 64 lowercase hexadecimal characters.
fn is_digest(value: &str) -> bool {
    value.len() == DIGEST_LENGTH
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

/// The refusal for a reference openPapir cannot read as a digest.
///
/// The supplied value is not echoed: it is user text, and the argument name
/// is enough to say which part of the invocation was unusable.
fn unusable_reference() -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "An artefact reference is not a sha256 digest with an optional role.",
        Details::new().text("argument", "artefact"),
    )
}

/// Refuse a digest that names no object stored in this archive.
///
/// The object's bytes are never opened here: only its presence, its link
/// state, and the permissions of its fan-out directories are checked, exactly
/// as import checks them before it publishes.
fn refuse_absent_object(root: &Path, digest: &str) -> std::result::Result<(), Diagnostic> {
    let hex = digest.strip_prefix(DIGEST_PREFIX).unwrap_or(digest);
    let path = objects::absolute_path(root, hex);
    let relative = objects::archive_path(hex);
    if paths::is_symlink(&path) {
        return Err(paths::symlink_refusal(
            Details::new()
                .text("scope", "archive")
                .text("archive_path", relative),
        ));
    }
    objects::check_object_permissions(root, hex)?;
    if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(document::not_found("artefact", "artefact_digest"));
    }
    paths::refuse_if_wide(&path, &relative)
}

/// Check a user-supplied date: `YYYY-MM-DD`, stored verbatim.
///
/// The shape and the calendar are checked so that nothing unreadable is
/// stored. Nothing else happens to the value: it is never compared with the
/// timestamp openPapir recorded, and it is never read as a delivery date.
///
/// # Errors
///
/// Returns `usage.arguments` when the value is not a plausible calendar date.
pub fn checked_stated_date(value: Option<&str>) -> std::result::Result<Option<String>, Diagnostic> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if is_calendar_date(value) {
        return Ok(Some(value.to_owned()));
    }
    Err(Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "A stated date is not a calendar date of the form YYYY-MM-DD.",
        Details::new().text("argument", "date"),
    ))
}

/// Whether a value is a plausible `YYYY-MM-DD` calendar date.
fn is_calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| {
        value
            .get(range)
            .filter(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|part| part.parse::<u32>().ok())
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

/// The number of days in a month of the proleptic Gregorian calendar.
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
                29
            } else {
                28
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;
    use crate::records::case;
    use std::path::PathBuf;

    const PAYLOAD: &[u8] = b"synthetic bytes\n";
    const PAYLOAD_DIGEST: &str =
        "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

    /// An archive holding one case and, optionally, one imported artefact.
    fn archive_with_case(import: bool) -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        archive::init(root.path()).unwrap();
        if import {
            let inputs = tempfile::tempdir().unwrap();
            let file = inputs.path().join("note.txt");
            fs::write(&file, PAYLOAD).unwrap();
            archive::import::import(root.path(), &[PathBuf::from(&file)]).unwrap();
        }
        let case = case::create(root.path(), "Case", None).unwrap().data.case;
        (root, case.id)
    }

    #[test]
    fn a_submission_records_its_case_description_and_artefacts() {
        let (root, case_id) = archive_with_case(true);
        let added = add(
            root.path(),
            &case_id,
            "Sent by post.",
            Some("2026-01-14"),
            &[
                format!("{PAYLOAD_DIGEST}:cover letter"),
                PAYLOAD_DIGEST.to_owned(),
            ],
        )
        .unwrap()
        .data
        .submission;
        assert_eq!(added.case_id, case_id);
        assert_eq!(added.description, "Sent by post.");
        assert_eq!(added.stated_date.as_deref(), Some("2026-01-14"));
        assert_eq!(added.record_kind, KIND);
        assert_eq!(added.artefacts.len(), 2);
        assert_eq!(added.artefacts[0].digest, PAYLOAD_DIGEST);
        assert_eq!(added.artefacts[0].role.as_deref(), Some("cover letter"));
        assert_eq!(added.artefacts[1].role, None);
        assert!(added.created_at.ends_with('Z'));

        let stored = fs::read_to_string(
            root.path()
                .join(SUBMISSIONS_DIR)
                .join(format!("{}.json", added.id)),
        )
        .unwrap();
        assert!(stored.ends_with("}\n"));
        assert!(stored.starts_with("{\"archive_schema_version\""));
    }

    #[test]
    fn a_submission_without_artefacts_or_a_date_is_recorded() {
        let (root, case_id) = archive_with_case(false);
        let added = add(root.path(), &case_id, "Nothing attached.", None, &[])
            .unwrap()
            .data
            .submission;
        assert!(added.artefacts.is_empty());
        assert_eq!(added.stated_date, None);
        let shown = case::show(root.path(), &case_id).unwrap().data;
        assert_eq!(shown.submission_count, 1);
    }

    #[test]
    fn an_unknown_case_or_digest_is_refused_before_anything_is_written() {
        let (root, case_id) = archive_with_case(false);
        let refusal = add(
            root.path(),
            "0123456789abcdef0123456789abcdef",
            "Description",
            None,
            &[],
        )
        .unwrap_err()
        .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "case");
        assert_eq!(json["details"]["reference_kind"], "case_id");

        let refusal = add(
            root.path(),
            &case_id,
            "Description",
            None,
            &[PAYLOAD_DIGEST.to_owned()],
        )
        .unwrap_err()
        .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "artefact");
        assert_eq!(json["details"]["reference_kind"], "artefact_digest");
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
        assert_eq!(
            fs::read_dir(root.path().join(SUBMISSIONS_DIR))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn a_reference_that_is_not_a_digest_is_a_usage_refusal() {
        let (root, case_id) = archive_with_case(true);
        for reference in [
            "not-a-digest",
            "sha256:short",
            "sha256:A002FD0595C559505437CE754971D911B703373ADDF2B59E425EC057D631614F",
            "md5:a002fd0595c559505437ce754971d911",
            "../../etc/passwd",
        ] {
            let refusal = add(
                root.path(),
                &case_id,
                "Description",
                None,
                &[reference.to_owned()],
            )
            .unwrap_err()
            .error;
            assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
            let rendered = serde_json::to_string(&refusal).unwrap();
            assert!(!rendered.contains("passwd"), "no reference is echoed");
        }
    }

    #[test]
    fn a_role_that_breaks_its_cap_is_refused() {
        let (root, case_id) = archive_with_case(true);
        let refusal = add(
            root.path(),
            &case_id,
            "Description",
            None,
            &[format!("{PAYLOAD_DIGEST}:{}", "r".repeat(65))],
        )
        .unwrap_err()
        .error;
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["field"],
            "role"
        );
    }

    #[test]
    fn a_stated_date_is_a_calendar_date_stored_verbatim() {
        for value in ["2026-01-14", "2024-02-29", "2000-02-29", "1999-12-31"] {
            assert_eq!(
                checked_stated_date(Some(value)).unwrap().as_deref(),
                Some(value)
            );
        }
        for value in [
            "2026-02-30",
            "2023-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-01-00",
            "14/01/2026",
            "2026-1-4",
            "2026-01-14T00:00:00Z",
            "abcd-ef-gh",
        ] {
            let refusal = checked_stated_date(Some(value)).unwrap_err();
            assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
            assert_eq!(
                serde_json::to_value(&refusal).unwrap()["details"]["argument"],
                "date"
            );
        }
        assert_eq!(checked_stated_date(None).unwrap(), None);
        assert_eq!(checked_stated_date(Some("")).unwrap(), None);
    }

    #[test]
    fn adding_a_submission_needs_the_writer_lock() {
        let (root, case_id) = archive_with_case(false);
        let _held = WriterLock::acquire(root.path()).unwrap();
        let refusal = add(root.path(), &case_id, "Description", None, &[])
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::LOCK_HELD);
    }

    #[test]
    fn a_description_that_breaks_its_cap_is_refused_before_the_archive_opens() {
        let root = tempfile::tempdir().unwrap();
        let refusal = add(
            root.path(),
            "0123456789abcdef0123456789abcdef",
            &"d".repeat(1025),
            None,
            &[],
        )
        .unwrap_err()
        .error;
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        assert!(
            !root.path().join("papir-archive.json").exists(),
            "the archive was never touched"
        );
    }
}
