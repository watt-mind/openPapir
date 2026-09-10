//! The `submission` subcommands: what the user states they sent.
//!
//! This module holds the invocation only. What a submission is, and why it
//! asserts no delivery, receipt by an authority, authenticity, or legal
//! effect, lives in `openpapir_core::records::submission`.
//!
//! `--file` is the one-step form: the file is imported under the same writer
//! lock as the record and its digest becomes an artefact reference, so the
//! user never has to copy a digest between two commands. `--artefact` and
//! `--file` may be combined; the artefacts come first in the record, then the
//! files in the order they were given.
//!
//! The human lines would live with every other command's in `report`, and the
//! one line this form adds is here only because that file is at the
//! source-length limit the repository enforces. It is bound by exactly the
//! rule stated there: it carries a count and nothing else, never a path and
//! never an original filename.

use std::path::PathBuf;

use clap::Subcommand;
use openpapir_core::records::submission::{self, FileRef};
use openpapir_core::{Imported, SubmissionAdded};

use crate::{emit, report};

/// The separator between a path and the role the file is referenced with.
const ROLE_SEPARATOR: char = ':';

#[derive(Subcommand)]
pub enum SubmissionCommand {
    /// Record a submission the user states they sent.
    Add {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The identifier of the case the submission belongs to.
        #[arg(long = "case", value_name = "CASE_ID")]
        case_id: String,
        /// The user's own description, at most 1024 bytes.
        #[arg(long, value_name = "DESCRIPTION")]
        description: String,
        /// The user's own date, `YYYY-MM-DD`, stored verbatim.
        #[arg(long, value_name = "DATE")]
        date: Option<String>,
        /// A stored artefact, as `sha256:<digest>` or `sha256:<digest>:<role>`.
        #[arg(long = "artefact", value_name = "DIGEST[:ROLE]")]
        artefacts: Vec<String>,
        /// A local file to import and reference, as `<path>` or `<path>:<role>`.
        #[arg(long = "file", value_name = "PATH[:ROLE]")]
        files: Vec<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Show one submission and the associations naming it.
    Show {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The submission's own identifier, as `submission add` reported it.
        #[arg(value_name = "SUBMISSION_ID")]
        submission_id: String,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

/// Dispatch one `submission` subcommand and return the process exit code.
pub fn run(command: SubmissionCommand) -> i32 {
    match command {
        SubmissionCommand::Add {
            archive,
            case_id,
            description,
            date,
            artefacts,
            files,
            json,
        } => {
            let files: Vec<FileRef> = files.iter().map(|value| file_reference(value)).collect();
            emit(
                "submission.add",
                submission::add_with_files(
                    &archive,
                    &case_id,
                    &description,
                    date.as_deref(),
                    &artefacts,
                    &files,
                ),
                json,
                lines,
            )
        }
        SubmissionCommand::Show {
            archive,
            submission_id,
            json,
        } => emit(
            "submission.show",
            submission::show(&archive, &submission_id),
            json,
            report::submission_shown,
        ),
    }
}

/// Read one `--file` value as a path and, when one is there, a role.
///
/// The role is the text after the last colon, and only when that text is not
/// empty and carries no path separator. A Windows drive letter and a colon
/// inside a directory name are therefore part of the path, which is what the
/// user meant by typing them, and a file whose own name ends in `:<word>`
/// keeps its role syntax rather than a second meaning.
fn file_reference(value: &str) -> FileRef {
    match value.rsplit_once(ROLE_SEPARATOR) {
        Some((path, role))
            if !path.is_empty()
                && !role.is_empty()
                && !role.contains('/')
                && !role.contains('\\') =>
        {
            FileRef {
                path: PathBuf::from(path),
                role: Some(role.to_owned()),
            }
        }
        _ => FileRef {
            path: PathBuf::from(value),
            role: None,
        },
    }
}

/// The lines `submission add` prints when it succeeds.
///
/// The record's own lines are the same whichever form was used. A run that
/// imported files says how many, and never which: a filename is the user's
/// own and reaches no output.
fn lines(added: &SubmissionAdded) -> Vec<String> {
    let mut lines = report::submission_added(&added.submission);
    if let Some(imported) = &added.imported {
        // The record's closing disclaimer stays the last line.
        let before_disclaimer = lines.len() - 1;
        lines.insert(before_disclaimer, imported_line(imported));
    }
    lines
}

/// The one line that says how many files the submission imported itself.
///
/// It is a count of files and a count of the bytes already stored, and
/// nothing else. Which file it was is the user's own and reaches no output.
fn imported_line(imported: &Imported) -> String {
    format!(
        "Files imported with this submission: {}; {} already present.",
        imported.imported + imported.duplicates,
        imported.duplicates
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_value_keeps_its_path_and_takes_a_role_only_when_one_is_there() {
        assert_eq!(file_reference("/tmp/a.pdf").role, None);
        assert_eq!(
            file_reference("/tmp/a.pdf").path,
            PathBuf::from("/tmp/a.pdf")
        );
        let with_role = file_reference("/tmp/a.pdf:cover letter");
        assert_eq!(with_role.path, PathBuf::from("/tmp/a.pdf"));
        assert_eq!(with_role.role.as_deref(), Some("cover letter"));
        assert_eq!(file_reference("C:\\docs\\a.pdf").role, None);
        assert_eq!(file_reference("/tmp/od:d/a.pdf").role, None);
        assert_eq!(file_reference("/tmp/a.pdf:").role, None);
        assert_eq!(file_reference(":a.pdf").role, None);
    }

    #[test]
    fn the_import_count_is_a_count_and_never_a_name() {
        let line = imported_line(&Imported {
            imported: 2,
            duplicates: 1,
            artefacts: Vec::new(),
        });
        assert_eq!(
            line,
            "Files imported with this submission: 3; 1 already present."
        );
    }
}
