//! The `openpapir` command-line interface.
//!
//! # Responsibility
//!
//! Parse arguments, call `openpapir-core`, and print the result as either
//! human-readable text or one JSON object. This crate holds no correspondence
//! logic of its own; it is the presentation layer over the library.
//!
//! # Status
//!
//! These invocations exist and nothing else:
//!
//! - `openpapir --help`, usage text from the argument parser.
//! - `openpapir --version`, the crate version.
//! - `openpapir capabilities [--json]`, the implementation status.
//! - `openpapir archive init <root> [--json]`, archive creation.
//! - `openpapir archive check --archive <root> [--json]`, the read-only
//!   whole-archive integrity check.
//! - `openpapir archive repair-permissions --archive <root> [--json]`, the
//!   only action besides `archive init` that narrows permissions.
//! - `openpapir import --archive <root> <file>... [--json]`, artefact import.
//! - `openpapir case create|list|show ... [--json]`, the user's own cases.
//! - `openpapir case export --archive <root> --case <id> --to <dir> [--json]`,
//!   a plain copy of one case out of the archive.
//! - `openpapir case delete --archive <root> --case <id> [--purge] [--json]`,
//!   deleting a case and, only with `--purge`, the objects nothing else
//!   references.
//! - `openpapir submission add ... [--json]`, what the user states they sent.
//! - `openpapir receipt add|list ... [--json]`, an artefact the user believes
//!   to be a receipt.
//! - `openpapir association create|list ... [--json]`, what the user asserts
//!   about whether a receipt relates to a submission.
//! - `openpapir skill`, the embedded agent skill document, written to stdout
//!   byte for byte. It takes no file, no `--json`, and always exits `0`.
//!
//! There is no automatic matching, no derived metadata, no receipt parsing,
//! no import from an export, no editing of a stored record, no deletion of a
//! single submission or receipt, no deletion of an archive, no
//! signature verification, and no government delivery. The integrity check
//! re-digests stored bytes, which is a storage-layer identity check and never
//! a cryptographic verification.
//!
//! # Envelope and exit codes
//!
//! Every command prints exactly one JSON object on stdout in its `--json`
//! form, with `schema_version`, `ok`, `command`, `data`, `verified`, an
//! `error` object exactly when `ok` is `false`, and a `warnings` array when a
//! platform degradation was observed. The exit code carries the error's bucket
//! and nothing else: `0` success, `2` usage, `3` refused input, `4` archive
//! state, `5` platform, `6` internal. `1` is never emitted. The catalogue is
//! `docs/error-contract.md`.
//!
//! An invocation the argument parser rejects is the same contract: with
//! `--json` it is one `usage.arguments` envelope on stdout, and without it the
//! parser's own usage text on stderr. Both exit `2`.
//!
//! # Boundaries
//!
//! No network access and no background work. Output never carries a
//! user-supplied path, an original filename, or a payload byte. The one
//! exception is the export destination, which human output echoes back
//! because the user just typed it; no JSON field ever carries it. It does carry
//! the titles, descriptions, roles, and dates the user typed into their own
//! records, and the identifiers and digests openPapir minted. KRX and
//! `.es3` handling belong to openKRX and openSzigno respectively; neither is
//! a dependency. No output may state or imply authenticity, successful
//! delivery, or legal effect.

mod delete;
mod envelope;
mod report;
mod skill;
mod usage;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use openpapir_core::error::{Diagnostic, Failure, Outcome};
use openpapir_core::records;
use openpapir_core::{Capabilities, Report, capabilities};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "openpapir",
    version,
    about = "openPapir local correspondence archive; import preserves original bytes"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report implemented operations and development status.
    Capabilities {
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Create and inspect a local archive.
    Archive {
        #[command(subcommand)]
        command: ArchiveCommand,
    },
    /// Create, list, and show local cases.
    Case {
        #[command(subcommand)]
        command: CaseCommand,
    },
    /// Record what the user states they sent, against a case.
    Submission {
        #[command(subcommand)]
        command: SubmissionCommand,
    },
    /// Record and list artefacts the user believes to be receipts.
    Receipt {
        #[command(subcommand)]
        command: ReceiptCommand,
    },
    /// Record and list what the user asserts about a receipt.
    Association {
        #[command(subcommand)]
        command: AssociationCommand,
    },
    /// Write the embedded agent skill document to stdout and nothing else.
    Skill,
    /// Import local files into the archive's artefact store.
    Import {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
        /// The files to import.
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ArchiveCommand {
    /// Check the whole archive against what its records claim, read-only.
    Check {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Narrow every path in the archive back to owner-only.
    RepairPermissions {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Create an archive in an existing, empty directory.
    Init {
        /// The archive root, which must exist and be empty.
        #[arg(value_name = "ROOT")]
        root: PathBuf,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum CaseCommand {
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
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// List every case in the archive.
    List {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
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
}

#[derive(Subcommand)]
enum SubmissionCommand {
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
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ReceiptCommand {
    /// Record an artefact the user believes to be a receipt.
    Add {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// A stored artefact, as `sha256:<digest>`.
        #[arg(long, value_name = "DIGEST")]
        artefact: String,
        /// The import event to record, the earliest one when absent.
        #[arg(long = "import-event", value_name = "IMPORT_EVENT_ID")]
        import_event: Option<String>,
        /// The user's own label, at most 200 bytes.
        #[arg(long, value_name = "LABEL")]
        label: Option<String>,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// List every receipt in the archive.
    List {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// Emit one JSON object instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AssociationCommand {
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
}

fn main() {
    // The arguments are kept before the parse, because an invocation the
    // parser rejects still has to say whether it asked for the JSON form.
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let code = match Args::try_parse() {
        Ok(parsed) => run(parsed.command),
        Err(error) => usage::report::<Args>(&error, &arguments),
    };
    std::process::exit(code);
}

/// Dispatch one parsed command and return the process exit code.
fn run(command: Command) -> i32 {
    match command {
        Command::Capabilities { json } => emit(
            "capabilities",
            Ok(Outcome {
                data: capabilities(),
                warnings: Vec::new(),
            }),
            json,
            capability_lines,
        ),
        Command::Archive {
            command: ArchiveCommand::Init { root, json },
        } => emit(
            "archive.init",
            openpapir_core::init(&root),
            json,
            report::created,
        ),
        Command::Archive {
            command: ArchiveCommand::Check { archive, json },
        } => emit_with_problems(
            "archive.check",
            openpapir_core::check(&archive),
            json,
            report::integrity,
            integrity_problem,
        ),
        Command::Archive {
            command: ArchiveCommand::RepairPermissions { archive, json },
        } => emit(
            "archive.repair_permissions",
            openpapir_core::repair_permissions(&archive),
            json,
            report::repaired,
        ),
        Command::Import {
            archive,
            json,
            files,
        } => emit(
            "import",
            openpapir_core::import(&archive, &files),
            json,
            report::imported,
        ),
        Command::Skill => skill::emit(),
        Command::Case { command } => run_case(command),
        Command::Submission {
            command:
                SubmissionCommand::Add {
                    archive,
                    case_id,
                    description,
                    date,
                    artefacts,
                    json,
                },
        } => emit(
            "submission.add",
            records::submission::add(
                &archive,
                &case_id,
                &description,
                date.as_deref(),
                &artefacts,
            ),
            json,
            report::submission_added,
        ),
        Command::Receipt { command } => run_receipt(command),
        Command::Association { command } => run_association(command),
    }
}

/// Dispatch one `case` subcommand and return the process exit code.
fn run_case(command: CaseCommand) -> i32 {
    match command {
        CaseCommand::Create {
            archive,
            title,
            notes,
            json,
        } => emit(
            "case.create",
            records::case::create(&archive, &title, notes.as_deref()),
            json,
            report::case_created,
        ),
        CaseCommand::List { archive, json } => emit(
            "case.list",
            records::case::list(&archive),
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
    }
}

/// Dispatch one `receipt` subcommand and return the process exit code.
fn run_receipt(command: ReceiptCommand) -> i32 {
    match command {
        ReceiptCommand::Add {
            archive,
            artefact,
            import_event,
            label,
            json,
        } => emit(
            "receipt.add",
            records::receipt::add(
                &archive,
                &artefact,
                import_event.as_deref(),
                label.as_deref(),
            ),
            json,
            report::receipt_added,
        ),
        ReceiptCommand::List { archive, json } => emit(
            "receipt.list",
            records::receipt::list(&archive),
            json,
            report::receipt_list,
        ),
    }
}

/// Dispatch one `association` subcommand and return the process exit code.
fn run_association(command: AssociationCommand) -> i32 {
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
            records::association::create(
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
            records::association::list(&archive, &receipt_id),
            json,
            report::association_history,
        ),
    }
}

/// The first problem an integrity report holds, and the exit code it maps to.
///
/// The check completed its stated work, so the report stays in `data`; the
/// error names the first problem in the fixed precedence the contract
/// documents, and the exit code is the highest group of the buckets the
/// report's problems belong to.
fn integrity_problem(report: &Report) -> Option<(Diagnostic, i32)> {
    report
        .first_problem()
        .map(|error| (error, report.exit_code()))
}

/// The two lines `capabilities` prints without `--json`.
fn capability_lines(data: &Capabilities) -> Vec<String> {
    vec![
        format!("{}: {}", data.project, data.stage),
        format!(
            "Implemented operations: {}. Nothing is verified.",
            data.operations.join(", ")
        ),
    ]
}

/// Print one envelope or the human form, and return the process exit code.
///
/// In the `--json` form exactly one object reaches stdout and nothing reaches
/// stderr. In the human form the result goes to stdout while warnings and the
/// error go to stderr, so diagnostic text never shares stdout with the JSON.
fn emit<T: Serialize>(
    command: &str,
    result: Result<Outcome<T>, Failure>,
    json: bool,
    human: fn(&T) -> Vec<String>,
) -> i32 {
    emit_with_problems(command, result, json, human, |_| None)
}

/// Print one envelope for a command that may complete and still find
/// problems, and return the process exit code.
///
/// A command whose stated work is to look for problems reports them in `data`
/// and names the first of them in `error`, so the counts the user asked for
/// survive the refusal. `problem` returns nothing for every other command.
fn emit_with_problems<T: Serialize>(
    command: &str,
    result: Result<Outcome<T>, Failure>,
    json: bool,
    human: fn(&T) -> Vec<String>,
    problem: fn(&T) -> Option<(Diagnostic, i32)>,
) -> i32 {
    match result {
        Ok(outcome) => {
            let found = problem(&outcome.data);
            let (found, code) = match found {
                Some((error, code)) => (Some(error), code),
                None => (None, 0),
            };
            if json {
                println!(
                    "{}",
                    match &found {
                        Some(error) =>
                            envelope::problem(command, &outcome.data, error, &outcome.warnings),
                        None => envelope::success(command, &outcome.data, &outcome.warnings),
                    }
                );
            } else {
                for line in human(&outcome.data) {
                    println!("{line}");
                }
                for warning in &outcome.warnings {
                    eprintln!("warning {}: {}", warning.code, warning.message);
                }
                if let Some(error) = &found {
                    eprintln!("error {}: {}", error.code, error.message);
                }
            }
            found.map_or(0, |_| code)
        }
        Err(failure) => {
            if json {
                println!(
                    "{}",
                    envelope::failure(command, &failure.error, &failure.warnings)
                );
            } else {
                for warning in &failure.warnings {
                    eprintln!("warning {}: {}", warning.code, warning.message);
                }
                eprintln!("error {}: {}", failure.error.code, failure.error.message);
            }
            failure.error.exit_code()
        }
    }
}
