//! The `association` subcommands: what the user asserts about a receipt, and
//! the withdrawal of an assertion they no longer stand behind.
//!
//! Nothing here is a match openPapir made. Every record is the user's own
//! statement, and every one of these invocations only writes or reads a
//! record: none edits or removes one. Retiring an association writes a new
//! record superseding the one the user named, so the history keeps both.
//!
//! This module holds the invocations only. The rules live in
//! `openpapir_core::records::association`, and the human lines live with
//! every other command's in `report`.

use std::path::PathBuf;

use clap::Subcommand;
use openpapir_core::records::association;

use crate::{emit, report};

/// The invocations `openpapir association` accepts.
#[derive(Subcommand)]
pub enum AssociationCommand {
    /// Record what the user asserts about a receipt.
    Create {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The receipt the assertion is about.
        #[arg(long = "receipt", value_name = "RECEIPT_ID")]
        receipt_id: String,
        /// One of `unassociated`, `candidate`, `associated`, `contradictory`.
        #[arg(long, value_name = "OUTCOME")]
        outcome: String,
        /// A candidate, as `<submission-id>:<confidence>:<statement>`, where
        /// confidence is `weak`, `moderate`, or `strong`.
        #[arg(long = "candidate", value_name = "SUBMISSION_ID:CONFIDENCE:STATEMENT")]
        candidates: Vec<String>,
        /// An earlier association for the same receipt, which this replaces.
        #[arg(long, value_name = "ASSOCIATION_ID")]
        supersedes: Option<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// List one receipt's associations, newest first, with its whole history.
    List {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The receipt whose history to list.
        #[arg(long = "receipt", value_name = "RECEIPT_ID")]
        receipt_id: String,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Withdraw an assertion by superseding it with a record claiming nothing.
    Retire {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The association to retire, as it was reported when it was written.
        #[arg(value_name = "ASSOCIATION_ID")]
        association_id: String,
        /// The user's own reason, at most 512 bytes, stored and never
        /// repeated in a message.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

/// Dispatch one `association` subcommand and return the process exit code.
pub fn run(command: AssociationCommand) -> i32 {
    match command {
        AssociationCommand::Create {
            archive,
            receipt_id,
            outcome,
            candidates,
            supersedes,
            json,
        } => emit(
            "association.create",
            association::create(
                &archive,
                &receipt_id,
                &outcome,
                &candidates,
                supersedes.as_deref(),
            ),
            json,
            report::association_created,
        ),
        AssociationCommand::List {
            archive,
            receipt_id,
            json,
        } => emit(
            "association.list",
            association::list(&archive, &receipt_id),
            json,
            report::association_history,
        ),
        AssociationCommand::Retire {
            archive,
            association_id,
            reason,
            json,
        } => emit(
            "association.retire",
            association::retire(&archive, &association_id, reason.as_deref()),
            json,
            report::association_retired,
        ),
    }
}
