//! The read-only archive summary and its receipt-retrieval reminders.
//!
//! `archive status` answers two questions without changing anything: how much
//! the archive holds, and which submissions the user has not yet recorded a
//! receipt against while the operator's stated retention window is still
//! open. It takes no writer lock, opens the archive read-only exactly as the
//! integrity check does, and writes nothing at all.
//!
//! # The retention window
//!
//! [`RETENTION_WINDOW_DAYS`] is 30. The operator's help page states that the
//! personal delivery storage retains incoming documents, *igazolások* and
//! *nyugták* for 30 days unless they are moved to permanent storage; it was
//! retrieved on 2026-09-09 and the claim is descriptive, not normative
//! (`docs/receipt-discovery.md`, E1). openPapir reads no mailbox and checks
//! no service, so a reminder here is arithmetic on the user's own stated date
//! and the operator's own published description, and nothing else.
//!
//! A reminder is a reminder to go and fetch a file from the delivery storage.
//! It is never a statement that anything was delivered, that a receipt
//! exists, that one was received by an authority, or that any legal effect
//! followed.
//!
//! # What the summary may carry
//!
//! Counts, dates, and the identifiers openPapir minted. No title, no
//! description, no notes, no statement, no digest, and no path
//! (`docs/error-contract.md`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::archive::Archive;
use crate::archive::objects::{ALGORITHM, OBJECTS_DIR};
use crate::clock;
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::records::association::Association;
use crate::records::case::{Case, Status as CaseStatus};
use crate::records::document;
use crate::records::is_digest;
use crate::records::receipt::Receipt;
use crate::records::submission::{self, Submission};

/// The retention window the reminders use, in days.
///
/// One constant, one provenance: the operator's help page states 30 days,
/// retrieved 2026-09-09, descriptive rather than normative
/// (`docs/receipt-discovery.md`, E1, and `docs/architecture.md`). openPapir
/// neither enforces it nor checks it against any service.
pub const RETENTION_WINDOW_DAYS: i64 = 30;

/// The two association outcomes that take a submission off the list.
///
/// `unassociated` and `contradictory` do not: the first records that the user
/// found nothing, and the second that what they found conflicts, so in
/// neither case has a receipt been tied to the submission.
const NAMING_OUTCOMES: [&str; 2] = ["associated", "candidate"];

/// The closed set of case statuses, in the order the summary reports them.
///
/// Every status is reported, including one no case holds, so a caller reads a
/// count rather than testing for a key's presence, exactly as it does with
/// the integrity check's `problems`.
const CASE_STATUSES: [CaseStatus; 2] = [CaseStatus::Open, CaseStatus::Closed];

/// How many cases hold one status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct StatusCount {
    /// How many cases hold it.
    pub count: u64,
    /// The status's own stable lowercase name.
    pub status: &'static str,
}

/// One reminder to retrieve a submission receipt while the window is open.
///
/// Identifiers and dates only. The submission's own description, and the
/// case's title, are the user's text and never reach this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Reminder {
    /// The case the submission belongs to.
    pub case_id: String,
    /// Whole days from the as-of date to `retrieve_by`, `0` on the last day.
    pub days_left: i64,
    /// The submission's own date plus the retention window.
    pub retrieve_by: String,
    /// The date the user stated for the submission, verbatim.
    pub submission_date: String,
    /// The submission this reminder is about.
    pub submission_id: String,
}

/// What one archive holds, and what is still worth fetching.
///
/// The struct is `#[non_exhaustive]`: the summary gains figures as openPapir
/// learns to count more of an archive, so a caller outside this crate matches
/// on the fields it knows and never builds one by literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Status {
    /// The date every window was measured against, `YYYY-MM-DD`.
    pub as_of: String,
    /// How many association records the archive holds, superseded included.
    pub associations: u64,
    /// How many cases the archive holds, whatever their status.
    pub cases: u64,
    /// One entry per case status, in the closed set's order, including a
    /// status no case holds. The counts sum to `cases`.
    pub cases_by_status: Vec<StatusCount>,
    /// How many receipt records the archive holds.
    pub receipts: u64,
    /// The submissions whose window is still open and which no association
    /// names, ordered by `retrieve_by` and then by identifier.
    pub receipts_to_retrieve: Vec<Reminder>,
    /// The window the reminders were computed with, in days.
    pub retention_window_days: i64,
    /// How many objects the artefact store holds.
    pub stored_objects: u64,
    /// How many submission records the archive holds.
    pub submissions: u64,
    /// How many submissions carry no date openPapir can measure a window
    /// from. They are counted and never listed, because without a date there
    /// is nothing to remind anybody of.
    pub undated_submissions: u64,
}

/// Summarise one archive and list the receipts still worth retrieving.
///
/// `as_of` is the date every window is measured against; when it is absent
/// the clock's own date is used. A date in the past or the future is
/// accepted, because asking what the archive looked like, or will look like,
/// on some other day is the point of the flag.
///
/// The archive is opened read-only, exactly as `archive check` opens it: no
/// writer lock is taken, so a held lock never stops it, no missing layout
/// directory is created, and nothing inside the root is written, renamed, or
/// removed.
///
/// # Errors
///
/// Returns `usage.arguments` when `as_of` is not a calendar date,
/// `record.malformed` when a stored document cannot be read as a record of
/// its kind, and the refusals of opening an archive:
/// `usage.archive_root_missing`, `path.symlink`, `archive.marker_missing`,
/// `archive.marker_malformed`, `archive.schema_newer`,
/// `archive.schema_older`, `archive.permissions_wide`, or
/// `archive.multiple_filesystems`.
pub fn status(root: &Path, as_of: Option<&str>) -> Result<Status> {
    let mut warnings = Vec::new();
    match summarise(root, as_of, &mut warnings) {
        Ok(status) => Ok(Outcome {
            data: status,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn summarise(
    root: &Path,
    as_of: Option<&str>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Status, Diagnostic> {
    let as_of = checked_as_of(as_of)?;
    let mut archive = Archive::open_read_only(root)?;
    warnings.extend(archive.take_warnings());
    let root = archive.root();

    let cases = document::list_records::<Case>(root)?;
    let submissions = document::list_records::<Submission>(root)?;
    let receipts = document::list_records::<Receipt>(root)?;
    let associations = document::list_records::<Association>(root)?;

    let named = named_submissions(&associations);
    let (receipts_to_retrieve, undated_submissions) =
        reminders(&submissions, &named, days_of(&as_of));

    Ok(Status {
        as_of,
        associations: associations.len() as u64,
        cases_by_status: cases_by_status(&cases),
        cases: cases.len() as u64,
        receipts: receipts.len() as u64,
        receipts_to_retrieve,
        retention_window_days: RETENTION_WINDOW_DAYS,
        stored_objects: count_objects(root),
        submissions: submissions.len() as u64,
        undated_submissions,
    })
}

/// How many cases hold each status, in the closed set's order.
///
/// A status no case holds is reported as `0` rather than left out, so the
/// shape of the array does not depend on what the archive happens to hold.
fn cases_by_status(cases: &[Case]) -> Vec<StatusCount> {
    CASE_STATUSES
        .into_iter()
        .map(|status| StatusCount {
            count: cases.iter().filter(|case| case.status == status).count() as u64,
            status: status.as_str(),
        })
        .collect()
}

/// Check the as-of date, defaulting to the clock's own date.
///
/// The shape is the one `submission add --date` accepts, so one date rule
/// holds everywhere. The value is not otherwise interpreted.
fn checked_as_of(value: Option<&str>) -> std::result::Result<String, Diagnostic> {
    match value.filter(|value| !value.is_empty()) {
        None => Ok(clock::today()),
        Some(value) if submission::is_calendar_date(value) => Ok(value.to_owned()),
        Some(_) => Err(Diagnostic::new(
            codes::USAGE_ARGUMENTS,
            "An as-of date is not a calendar date of the form YYYY-MM-DD.",
            Details::new().text("argument", "as_of"),
        )),
    }
}

/// Every submission a live `associated` or `candidate` association names.
///
/// Only the live head of each supersession chain is read. A record another
/// record supersedes is history: it is never modified, never removed, and
/// `association list` still shows it, but it no longer says what the user
/// asserts today. So a user who retracts an `associated` assertion by
/// superseding it with an `unassociated` one gets the reminder back, which is
/// the whole point of being able to retract one.
fn named_submissions(associations: &[Association]) -> BTreeSet<&str> {
    let superseded: BTreeSet<&str> = associations
        .iter()
        .filter_map(|association| association.supersedes.as_deref())
        .filter(|id| !id.is_empty())
        .collect();
    let mut named = BTreeSet::new();
    for association in associations {
        if superseded.contains(association.id.as_str())
            || !NAMING_OUTCOMES.contains(&association.outcome.as_str())
        {
            continue;
        }
        if let Some(submission_id) = association.submission_id.as_deref() {
            named.insert(submission_id);
        }
        for candidate in &association.candidates {
            named.insert(candidate.submission_id.as_str());
        }
    }
    named
}

/// The reminders and the count of submissions without a usable date.
///
/// A submission is listed when it carries a date, no naming association
/// mentions it, and its date plus the window is on or after the as-of date.
/// The last day of the window counts as open: `days_left` is then `0`.
fn reminders(
    submissions: &[Submission],
    named: &BTreeSet<&str>,
    as_of: i64,
) -> (Vec<Reminder>, u64) {
    let mut reminders = Vec::new();
    let mut undated = 0;
    for submission in submissions {
        let Some(stated) = usable_date(submission.stated_date.as_deref()) else {
            undated += 1;
            continue;
        };
        if named.contains(submission.id.as_str()) {
            continue;
        }
        let retrieve_by = days_of(&stated) + RETENTION_WINDOW_DAYS;
        let days_left = retrieve_by - as_of;
        if days_left < 0 {
            continue;
        }
        reminders.push(Reminder {
            case_id: submission.case_id.clone(),
            days_left,
            retrieve_by: clock::date_from_days(retrieve_by),
            submission_date: stated,
            submission_id: submission.id.clone(),
        });
    }
    reminders.sort_by(|left, right| {
        left.retrieve_by
            .cmp(&right.retrieve_by)
            .then_with(|| left.submission_id.cmp(&right.submission_id))
    });
    (reminders, undated)
}

/// A stored date openPapir can measure a window from.
///
/// A stored value that is not a calendar date is left out of the arithmetic
/// rather than guessed at, and the submission is counted as undated: the date
/// is the user's own text, stored verbatim, and openPapir never repairs one.
fn usable_date(stated: Option<&str>) -> Option<String> {
    stated
        .filter(|value| submission::is_calendar_date(value))
        .map(str::to_owned)
}

/// The day count of a value already checked as a calendar date.
///
/// Every caller has run the value through [`usable_date`] or
/// [`checked_as_of`] first, so the precondition is asserted rather than
/// re-checked: an unchecked value reaching here is a bug in this module, not
/// input to be interpreted.
fn days_of(date: &str) -> i64 {
    debug_assert!(
        submission::is_calendar_date(date),
        "days_of is only ever given a checked calendar date"
    );
    let part = |range: std::ops::Range<usize>| {
        date.get(range)
            .and_then(|part| part.parse::<i64>().ok())
            .unwrap_or(0)
    };
    let (year, month, day) = (part(0..4), part(5..7), part(8..10));
    clock::days_from_date(
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// How many objects the artefact store holds.
///
/// The walk counts entries and reads no bytes: judging what the store holds
/// is `archive check`'s work, not this command's. An entry that is not a
/// regular file, or whose name is not a digest, is not an object and is not
/// counted, and a directory that cannot be listed contributes nothing.
fn count_objects(root: &Path) -> u64 {
    let mut count = 0;
    let store = root.join(OBJECTS_DIR).join(ALGORITHM);
    for high in directories(&store) {
        for low in directories(&high) {
            let Ok(entries) = fs::read_dir(&low) else {
                continue;
            };
            count += entries
                .filter_map(std::result::Result::ok)
                .filter(|entry| {
                    entry.file_name().to_str().is_some_and(is_digest)
                        && fs::symlink_metadata(entry.path())
                            .is_ok_and(|metadata| metadata.is_file())
                })
                .count() as u64;
        }
    }
    count
}

/// The subdirectories of one fan-out level, ordered by name.
///
/// A link is not followed: the fan-out directories openPapir creates are
/// directories, and anything else there is not one of its own.
fn directories(path: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut found: Vec<std::path::PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests;
