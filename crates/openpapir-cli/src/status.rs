//! The `archive status` subcommand: what the archive holds, and what is left
//! to look for.
//!
//! This module holds the invocation and its human lines. The summary itself,
//! the retention window, and the rule about which submissions are reminded of
//! live in `openpapir_core::status`.
//!
//! The wording here is a reminder to go and look in the delivery storage for
//! a submission receipt while the operator says one would still be there. It
//! never presupposes that a receipt exists, and it is never a statement that
//! anything was delivered, that one was received by an authority, or that any
//! legal effect followed. openPapir opens no mailbox and checks no service.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::error::{Failure, Outcome};
use openpapir_core::status::Status as Summary;

/// The arguments `openpapir archive status` accepts.
#[derive(Args)]
pub struct Status {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// The date every window is measured against, `YYYY-MM-DD`, today when
    /// absent. A past or future date is a what-if and is accepted.
    #[arg(long = "as-of", value_name = "DATE")]
    pub as_of: Option<String>,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl Status {
    /// Summarise the archive without taking a lock or writing anything.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::status::status`.
    pub fn run(&self) -> Result<Outcome<Summary>, Failure> {
        openpapir_core::status::status(&self.archive, self.as_of.as_deref())
    }
}

/// The lines `archive status` prints without `--json`.
///
/// Counts, dates, and minted identifiers only. No title, no description, and
/// no path reaches this output, exactly as none reaches the JSON summary.
#[must_use]
pub fn lines(summary: &Summary) -> Vec<String> {
    let mut lines = vec![
        format!(
            "As of {}. Case(s): {}. Submission(s): {}. Receipt(s): {}. Association(s): {}. Stored object(s): {}.",
            summary.as_of,
            summary.cases,
            summary.submissions,
            summary.receipts,
            summary.associations,
            summary.stored_objects
        ),
        format!(
            "Case(s) by status: {}.",
            summary
                .cases_by_status
                .iter()
                .map(|entry| format!("{} {}", entry.status, entry.count))
                .collect::<Vec<String>>()
                .join(", ")
        ),
        format!(
            "Submission(s) with no usable date: {}.",
            summary.undated_submissions
        ),
        format!(
            "Reminder(s) to look for a submission receipt in the delivery storage while the {}-day window the operator describes is open: {}.",
            summary.retention_window_days,
            summary.receipts_to_retrieve.len()
        ),
    ];
    for reminder in &summary.receipts_to_retrieve {
        lines.push(format!(
            "case {}, submission {}, stated {}, look by {}, {} day(s) left.",
            reminder.case_id,
            reminder.submission_id,
            reminder.submission_date,
            reminder.retrieve_by,
            reminder.days_left
        ));
    }
    lines.push(
        "The window is the operator's own published description of their storage, not a rule openPapir applies or checks. Nothing was read from any mailbox or service, and nothing here states that a submission was delivered, that a receipt exists, that one was received by an authority, or that any legal effect followed."
            .to_owned(),
    );
    lines.push("The summary read the archive and changed nothing.".to_owned());
    lines
}
