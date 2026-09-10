//! The `case` subcommands: the user's own folders for their own matters.
//!
//! This module holds the invocations only. What a case is, what may be
//! changed in one, and why the case record is the single kind openPapir
//! rewrites in place all live in `openpapir_core::records::case`, and the
//! human lines live with every other command's in `report`.
//!
//! `case update` is the one invocation that rewrites a stored record. It
//! reports the names of the fields it changed and nothing more: what a value
//! was before is the user's own text, and the record it prints already
//! carries what each value is now. `case list --query` compares without
//! regard to case over the title and the notes in one linear scan, and the
//! query text is never echoed back.

use std::path::PathBuf;

use clap::Subcommand;
use openpapir_core::error::Failure;
use openpapir_core::records;
use openpapir_core::records::case::{Change, Filter, NotesChange, Status};

use crate::{delete, emit, emit_with_problems, report};

#[derive(Subcommand)]
pub enum CaseCommand {
    /// Delete a case and its submissions; objects go only with `--purge`.
    Delete(delete::Delete),
    /// Record a new case, which is local organisation and nothing else.
    Create {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The user's own title for the case, at most 200 bytes.
        #[arg(long, value_name = "TITLE")]
        title: String,
        /// The user's own notes, at most 4096 bytes.
        #[arg(long, value_name = "NOTES")]
        notes: Option<String>,
        /// The user's own tag, at most 64 bytes; repeat for more, at most 32.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// `open` or `closed`; `open` when it is not given.
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// List the cases in the archive, filtered by what was asked for.
    List {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// Keep only cases with this status, `open` or `closed`.
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Keep only cases carrying this tag; repeat, and all must match.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// Keep only cases whose title or notes contain this text, compared
        /// without regard to case. The text is never echoed back.
        #[arg(long, value_name = "TEXT")]
        query: Option<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Copy one case, its records, and its objects out of the archive.
    Export {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The case to export, as `case create` reported it.
        #[arg(long = "case", value_name = "CASE_ID")]
        case_id: String,
        /// The destination directory, empty or not yet created.
        #[arg(long = "to", value_name = "DIR")]
        destination: PathBuf,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Show one case and the submissions recorded against it.
    Show {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The case's own identifier, as `case create` reported it.
        #[arg(value_name = "CASE_ID")]
        case_id: String,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Change the user's own filing of one case: title, notes, status, tags.
    Update {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The case's own identifier, as `case create` reported it.
        #[arg(value_name = "CASE_ID")]
        case_id: String,
        /// A new title, at most 200 bytes.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// New notes, at most 4096 bytes.
        #[arg(long, value_name = "NOTES", conflicts_with = "clear_notes")]
        notes: Option<String>,
        /// Remove the notes entirely.
        #[arg(long = "clear-notes")]
        clear_notes: bool,
        /// A new status, `open` or `closed`.
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// A tag to add; repeat for more.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// A tag to remove; repeat for more.
        #[arg(long = "untag", value_name = "TAG")]
        untags: Vec<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

/// The status one of the two stable names spells, when one was supplied.
///
/// The refusal is the library's own `usage.arguments`, so an unusable status
/// reads the same whether it reached the library or stopped here, and the
/// value the user typed is not echoed back.
fn parsed_status(value: Option<&String>) -> Result<Option<Status>, Failure> {
    value
        .map(|value| Status::parse(value))
        .transpose()
        .map_err(Failure::new)
}

/// What one `case update` invocation asks for, or the refusal it is.
fn notes_change(notes: Option<&str>, clear: bool) -> NotesChange<'_> {
    match (notes, clear) {
        (_, true) => NotesChange::Clear,
        (Some(text), false) => NotesChange::Set(text),
        (None, false) => NotesChange::Keep,
    }
}

/// Dispatch one `case` subcommand and return the process exit code.
pub fn run(command: CaseCommand) -> i32 {
    match command {
        CaseCommand::Create {
            archive,
            title,
            notes,
            tags,
            status,
            json,
        } => emit(
            "case.create",
            parsed_status(status.as_ref()).and_then(|status| {
                records::case::create_with(
                    &archive,
                    &title,
                    notes.as_deref(),
                    status.unwrap_or_default(),
                    &tags,
                )
            }),
            json,
            report::case_created,
        ),
        CaseCommand::List {
            archive,
            status,
            tags,
            query,
            json,
        } => emit(
            "case.list",
            parsed_status(status.as_ref()).and_then(|status| {
                records::case::list(
                    &archive,
                    &Filter {
                        status,
                        tags: &tags,
                        query: query.as_deref(),
                    },
                )
            }),
            json,
            report::case_list,
        ),
        CaseCommand::Export {
            archive,
            case_id,
            destination,
            json,
        } => emit(
            "case.export",
            openpapir_core::export_case(&archive, &case_id, &destination),
            json,
            report::exported,
        ),
        CaseCommand::Delete(arguments) => emit_with_problems(
            "case.delete",
            arguments.run(),
            arguments.json,
            report::case_deleted,
            delete::retained,
        ),
        CaseCommand::Show {
            archive,
            case_id,
            json,
        } => emit(
            "case.show",
            records::case::show(&archive, &case_id),
            json,
            report::case_shown,
        ),
        CaseCommand::Update {
            archive,
            case_id,
            title,
            notes,
            clear_notes,
            status,
            tags,
            untags,
            json,
        } => emit(
            "case.update",
            parsed_status(status.as_ref()).and_then(|status| {
                records::case::update(
                    &archive,
                    &case_id,
                    &Change {
                        title: title.as_deref(),
                        notes: notes_change(notes.as_deref(), clear_notes),
                        status,
                        add_tags: &tags,
                        remove_tags: &untags,
                    },
                )
            }),
            json,
            report::case_updated,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_reaches_the_library_or_is_refused_the_librarys_way() {
        assert_eq!(parsed_status(None).unwrap(), None);
        assert_eq!(
            parsed_status(Some(&"closed".to_owned())).unwrap(),
            Some(Status::Closed)
        );
        let failure = parsed_status(Some(&"CLOSED".to_owned())).unwrap_err();
        assert_eq!(failure.error.exit_code(), 2);
        assert!(
            !serde_json::to_string(&failure.error)
                .unwrap()
                .contains("CLOSED"),
            "a refusal never echoes the value the user typed"
        );
    }

    #[test]
    fn clearing_the_notes_wins_over_setting_them() {
        assert_eq!(notes_change(None, false), NotesChange::Keep);
        assert_eq!(notes_change(Some("text"), false), NotesChange::Set("text"));
        assert_eq!(notes_change(None, true), NotesChange::Clear);
        assert_eq!(notes_change(Some("text"), true), NotesChange::Clear);
    }
}
