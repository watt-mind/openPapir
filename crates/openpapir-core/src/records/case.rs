//! Case records: a user-created folder of related correspondence.
//!
//! A case is purely local. It corresponds to nothing any government service
//! issues, and holding correspondence in one asserts nothing about delivery,
//! receipt by an authority, authenticity, or legal effect.
//!
//! Creating a case takes the archive's single-writer lock and runs the
//! archive's permission checks first. Listing and showing a case take no lock,
//! because a reader sees one whole document or another and never a partial
//! one.
//!
//! # The one record that may be rewritten
//!
//! The case record is the only kind openPapir rewrites in place. `case update`
//! writes the whole record again through the atomic write procedure, keeping
//! `id` and `created_at` and adding `updated_at`. Every other kind stays
//! append-only. The reason is what each record is for: a submission, a
//! receipt, and an association are evidence of what the user recorded at the
//! time, and evidence that can be edited is no longer evidence, while a case
//! is the user's own folder label and carries none. The decision and its
//! reasoning are recorded in `docs/archive-layout.md` and
//! `docs/architecture.md`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::lock::WriterLock;
use crate::archive::{Archive, SUPPORTED_SCHEMA_VERSION, limits};
use crate::clock;
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::ident;
use crate::records::document::{self, Record};
use crate::records::submission::Submission;
use crate::records::{CASES_DIR, checked_notes, checked_title};

/// The value a case record carries in `record_kind`.
pub const KIND: &str = "case";

/// Whether the user still considers a case live.
///
/// The set is closed and means nothing beyond the user's own filing: a closed
/// case is one the user stopped working on. It says nothing about delivery,
/// receipt by an authority, authenticity, or legal effect, and openPapir never
/// sets it on its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The user is still working on the matter. The default.
    #[default]
    Open,
    /// The user stopped working on the matter.
    Closed,
}

impl Status {
    /// The status's stable lowercase name, as it appears in the record.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// The status one of the two stable names spells.
    ///
    /// # Errors
    ///
    /// Returns `usage.arguments`, naming the argument and never the value.
    pub fn parse(value: &str) -> std::result::Result<Self, Diagnostic> {
        match value {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            _ => Err(Diagnostic::new(
                codes::USAGE_ARGUMENTS,
                "A case status must be `open` or `closed`.",
                Details::new().text("argument", "status"),
            )),
        }
    }
}

/// One case record, stored as `records/cases/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u32,
    /// When openPapir recorded the case.
    pub created_at: String,
    /// The case's own identifier, minted by openPapir.
    pub id: String,
    /// The user's own notes, absent when none were supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// The record kind, always `case`.
    pub record_kind: String,
    /// Whether the user still considers the case live. A record written
    /// before this field existed reads as `open`, so an archive an earlier
    /// build wrote needs no migration to be read by this one.
    #[serde(default)]
    pub status: Status,
    /// The user's own tags, sorted and deduplicated. A record written before
    /// this field existed reads as carrying no tag at all.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The user's own title for the case.
    pub title: String,
    /// When `case update` last rewrote the record. Absent until the user
    /// changes something, so the field states what happened rather than
    /// repeating `created_at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl Record for Case {
    const KIND: &'static str = KIND;
    const DIRECTORY: &'static str = CASES_DIR;

    fn id(&self) -> &str {
        &self.id
    }

    fn record_kind(&self) -> &str {
        &self.record_kind
    }
}

/// What creating a case reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseCreated {
    /// The case as it was stored.
    pub case: Case,
}

/// What updating a case reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseUpdated {
    /// The case as it now stands.
    pub case: Case,
    /// The names of the fields the update changed, sorted. Field names only:
    /// what a value was before is the user's own text, and the record itself
    /// already carries what it is now.
    pub changed: Vec<String>,
}

/// What listing cases reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseList {
    /// The cases the listing holds, ordered by identifier.
    pub cases: Vec<Case>,
    /// How many cases the listing holds, which under a filter is how many
    /// matched it rather than how many the archive holds.
    pub count: u64,
}

/// What showing one case reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseView {
    /// The case itself.
    pub case: Case,
    /// The submissions recorded against it, ordered by identifier.
    pub submissions: Vec<Submission>,
    /// How many submissions the case holds.
    pub submission_count: u64,
}

/// Which cases a listing keeps.
///
/// Every part is optional and every supplied part must match. The filter is
/// applied to records the listing has already read, in one linear scan: the
/// archive keeps no index, and building one would be a second copy of the
/// user's own text.
#[derive(Debug, Default, Clone, Copy)]
pub struct Filter<'a> {
    /// Keep only cases with this status.
    pub status: Option<Status>,
    /// Keep only cases carrying every one of these tags.
    pub tags: &'a [String],
    /// Keep only cases whose title or notes contain this text, compared
    /// without regard to case. The text is never echoed in any output.
    pub query: Option<&'a str>,
}

impl Filter<'_> {
    /// Whether one case matches every supplied part of the filter.
    fn matches(&self, case: &Case) -> bool {
        if self.status.is_some_and(|status| status != case.status) {
            return false;
        }
        if !self.tags.iter().all(|tag| case.tags.contains(tag)) {
            return false;
        }
        match self.query {
            None => true,
            Some(query) => {
                let query = query.to_lowercase();
                case.title.to_lowercase().contains(&query)
                    || case
                        .notes
                        .as_deref()
                        .is_some_and(|notes| notes.to_lowercase().contains(&query))
            }
        }
    }
}

/// What an update does to the notes field.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NotesChange<'a> {
    /// Leave the notes exactly as they are.
    #[default]
    Keep,
    /// Replace the notes with this text.
    Set(&'a str),
    /// Remove the notes entirely.
    Clear,
}

/// What one `case update` invocation asks for.
///
/// Every part is optional, and an update that asks for nothing at all, or for
/// only values the record already holds, is refused rather than written.
#[derive(Debug, Default, Clone, Copy)]
pub struct Change<'a> {
    /// A new title.
    pub title: Option<&'a str>,
    /// What to do with the notes.
    pub notes: NotesChange<'a>,
    /// A new status.
    pub status: Option<Status>,
    /// Tags to add.
    pub add_tags: &'a [String],
    /// Tags to remove.
    pub remove_tags: &'a [String],
}

impl Change<'_> {
    /// Whether the invocation named nothing to change at all.
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.notes == NotesChange::Keep
            && self.status.is_none()
            && self.add_tags.is_empty()
            && self.remove_tags.is_empty()
    }
}

/// The refusal for an update that would leave the record exactly as it is.
///
/// It names the operation and nothing else. What the user supplied is their
/// own text and is never echoed back in a refusal.
fn no_change() -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "The update names nothing that would change in this case.",
        Details::new().text("argument", "update"),
    )
}

/// Refuse a tag openPapir cannot store as written.
fn refuse_tag(message: &str) -> Diagnostic {
    Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        message,
        Details::new().text("argument", "tag"),
    )
}

/// Check one list of tags, then sort and deduplicate it.
///
/// A tag is 1 to 64 bytes and carries no control character. The count cap is
/// applied to what would be stored, so a tag the user repeated on the command
/// line never spends part of it.
///
/// # Errors
///
/// Returns `input.cap.tag_length` for a tag over its cap,
/// `input.cap.tag_count` for too many distinct tags, and `usage.arguments` for
/// an empty tag or one carrying a control character. None of them echoes a
/// tag.
fn checked_tags(values: &[String]) -> std::result::Result<Vec<String>, Diagnostic> {
    let mut tags = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        limits::check_tag_length(value.len() as u64, index as u64)?;
        if value.trim().is_empty() {
            return Err(refuse_tag("A case tag is empty."));
        }
        if value.chars().any(char::is_control) {
            return Err(refuse_tag(
                "A case tag carries a control character it may not carry.",
            ));
        }
        tags.push(value.clone());
    }
    tags.sort();
    tags.dedup();
    limits::check_tag_count(tags.len() as u64)?;
    Ok(tags)
}

/// Create a case in the archive at `root`, open and with no tag of its own.
///
/// # Errors
///
/// Returns the refusals of [`create_with`].
pub fn create(root: &Path, title: &str, notes: Option<&str>) -> Result<CaseCreated> {
    create_with(root, title, notes, Status::Open, &[])
}

/// Create a case in the archive at `root`.
///
/// The field caps are checked before the archive is opened, so an oversized
/// field is refused before anything is written.
///
/// # Errors
///
/// Returns `input.cap.field_length`, `input.cap.tag_length`,
/// `input.cap.tag_count`, or `usage.arguments` for a field that breaks its cap
/// or its shape, and any archive, lock, path, or write refusal of
/// `docs/error-contract.md`.
pub fn create_with(
    root: &Path,
    title: &str,
    notes: Option<&str>,
    status: Status,
    tags: &[String],
) -> Result<CaseCreated> {
    let mut warnings = Vec::new();
    match create_record(root, title, notes, status, tags, &mut warnings) {
        Ok(case) => Ok(Outcome {
            data: CaseCreated { case },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn create_record(
    root: &Path,
    title: &str,
    notes: Option<&str>,
    status: Status,
    tags: &[String],
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Case, Diagnostic> {
    let title = checked_title(title)?;
    let notes = checked_notes(notes)?;
    let tags = checked_tags(tags)?;
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let case = Case {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        created_at: clock::now_rfc3339(),
        id: ident::new_id()?,
        notes,
        record_kind: KIND.to_owned(),
        status,
        tags,
        title,
        updated_at: None,
    };
    warnings.extend(document::write_record(archive.root(), &case)?);
    Ok(case)
}

/// Rewrite one case record in place, keeping its `id` and its `created_at`.
///
/// This is the one operation that rewrites a stored record. What it may change
/// is the user's own filing: the title, the notes, the status, and the tags.
/// It refuses rather than writing when the invocation names nothing to change,
/// and equally when it names only values the record already holds, so no
/// record is rewritten and no `updated_at` moves for nothing.
///
/// # Errors
///
/// Returns `usage.arguments` when nothing would change or a field breaks its
/// shape, `input.cap.field_length`, `input.cap.tag_length`, or
/// `input.cap.tag_count` for a field over its cap, `record.not_found` when the
/// identifier names no case, and any archive, lock, path, or write refusal of
/// `docs/error-contract.md`.
pub fn update(root: &Path, case_id: &str, change: &Change<'_>) -> Result<CaseUpdated> {
    let mut warnings = Vec::new();
    match update_record(root, case_id, change, &mut warnings) {
        Ok(updated) => Ok(Outcome {
            data: updated,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn update_record(
    root: &Path,
    case_id: &str,
    change: &Change<'_>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<CaseUpdated, Diagnostic> {
    if change.is_empty() {
        return Err(no_change());
    }
    // Every supplied field is checked before the archive is opened, so an
    // update that breaks a cap is refused before the lock is even taken.
    let title = change.title.map(checked_title).transpose()?;
    let notes = match change.notes {
        NotesChange::Keep => None,
        NotesChange::Set(text) => Some(checked_notes(Some(text))?),
        NotesChange::Clear => Some(None),
    };
    let added = checked_tags(change.add_tags)?;
    let removed = checked_tags(change.remove_tags)?;

    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    let stored = document::read_record::<Case>(archive.root(), case_id, "case_id")?;

    let mut case = stored.clone();
    if let Some(title) = title {
        case.title = title;
    }
    if let Some(notes) = notes {
        case.notes = notes;
    }
    if let Some(status) = change.status {
        case.status = status;
    }
    case.tags.retain(|tag| !removed.contains(tag));
    case.tags.extend(added);
    case.tags.sort();
    case.tags.dedup();
    limits::check_tag_count(case.tags.len() as u64)?;

    let changed = changed_fields(&stored, &case);
    if changed.is_empty() {
        return Err(no_change());
    }
    case.updated_at = Some(clock::now_rfc3339());
    warnings.extend(document::replace_record(archive.root(), &case)?);
    Ok(CaseUpdated { case, changed })
}

/// The names of the fields two versions of one case disagree about, sorted.
fn changed_fields(before: &Case, after: &Case) -> Vec<String> {
    let mut changed = Vec::new();
    if before.notes != after.notes {
        changed.push("notes".to_owned());
    }
    if before.status != after.status {
        changed.push("status".to_owned());
    }
    if before.tags != after.tags {
        changed.push("tags".to_owned());
    }
    if before.title != after.title {
        changed.push("title".to_owned());
    }
    changed
}

/// List the cases in the archive at `root` that match `filter`, taking no
/// lock.
///
/// The filter is applied in one linear scan over the records the listing has
/// already read. There is no index, so the cost grows with the number of cases
/// the archive holds.
///
/// # Errors
///
/// Returns any archive refusal, or `record.malformed` when a stored document
/// cannot be read as a case.
pub fn list(root: &Path, filter: &Filter<'_>) -> Result<CaseList> {
    let mut warnings = Vec::new();
    match list_records(root, filter, &mut warnings) {
        Ok(cases) => Ok(Outcome {
            data: CaseList {
                count: cases.len() as u64,
                cases,
            },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn list_records(
    root: &Path,
    filter: &Filter<'_>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Vec<Case>, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let mut cases = document::list_records::<Case>(archive.root())?;
    cases.retain(|case| filter.matches(case));
    Ok(cases)
}

/// Show one case and the submissions recorded against it.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no case,
/// `record.malformed` when a stored document cannot be read, and any archive
/// refusal.
pub fn show(root: &Path, case_id: &str) -> Result<CaseView> {
    let mut warnings = Vec::new();
    match show_record(root, case_id, &mut warnings) {
        Ok(view) => Ok(Outcome {
            data: view,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn show_record(
    root: &Path,
    case_id: &str,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<CaseView, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let case = document::read_record::<Case>(archive.root(), case_id, "case_id")?;
    let submissions: Vec<Submission> = document::list_records::<Submission>(archive.root())?
        .into_iter()
        .filter(|submission| submission.case_id == case.id)
        .collect();
    Ok(CaseView {
        case,
        submission_count: submissions.len() as u64,
        submissions,
    })
}

#[cfg(test)]
mod tests;
