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
//! - `openpapir archive status --archive <root> [--as-of <yyyy-mm-dd>]
//!   [--json]`, the read-only summary of what the archive holds and which
//!   submission receipts are still worth fetching from the delivery storage.
//! - `openpapir archive repair-permissions --archive <root> [--json]`, the
//!   only action besides `archive init` that narrows permissions.
//! - `openpapir import --archive <root> <file>... [--json]`, artefact import.
//! - `openpapir case create|list|show|update ... [--json]`, the user's own
//!   cases. `case update` is the one invocation that rewrites a stored
//!   record, and it rewrites only the case record.
//! - `openpapir case export --archive <root> --case <id> --to <dir> [--json]`,
//!   a plain copy of one case out of the archive.
//! - `openpapir case import --archive <root> --from <dir> [--json]`, the same
//!   copy read back in, checked against the export's manifest before anything
//!   is written.
//! - `openpapir case delete --archive <root> --case <id> [--purge] [--json]`,
//!   deleting a case and, only with `--purge`, the objects nothing else
//!   references.
//! - `openpapir submission add|show ... [--json]`, what the user states they
//!   sent, and one such record with the associations naming it.
//! - `openpapir receipt add|list|show ... [--json]`, an artefact the user
//!   believes to be a receipt, and one such record with its association
//!   history.
//! - `openpapir association create|list|show ... [--json]`, what the user
//!   asserts about whether a receipt relates to a submission, and one such
//!   record with the supersession chain it belongs to.
//! - `openpapir association retire --archive <root> <association-id>
//!   [--reason <text>] [--json]`, the withdrawal of one assertion, written as
//!   a new record superseding it.
//! - `openpapir skill`, the embedded agent skill document, written to stdout
//!   byte for byte. It takes no file and no `--json`. A reader that closed the
//!   pipe still exits `0`; any other failing write exits `4`.
//!
//! There is no automatic matching, no derived metadata, no receipt parsing,
//! no export of a whole archive, no editing of a stored record other than the
//! case record `case update` rewrites, no deletion of a
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
//! exception is the export destination and the import source, which human
//! output echoes back because the user just typed one of them; no JSON field
//! ever carries either. It does carry
//! the titles, descriptions, roles, and dates the user typed into their own
//! records, and the identifiers and digests openPapir minted. KRX and
//! `.es3` handling belong to openKRX and openSzigno respectively; neither is
//! a dependency. No output may state or imply authenticity, successful
//! delivery, or legal effect.

mod associations;
mod cases;
mod delete;
mod envelope;
mod report;
mod restore;
mod skill;
mod status;
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
    /// Create, list, show, and update local cases.
    Case {
        #[command(subcommand)]
        command: cases::CaseCommand,
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
    /// Record, list, and retire what the user asserts about a receipt.
    Association {
        #[command(subcommand)]
        command: associations::AssociationCommand,
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
    /// Summarise the archive and the receipts still worth fetching.
    Status(status::Status),
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
    /// Show one receipt and its whole association history.
    Show {
        /// The archive root, which is always supplied explicitly.
        #[arg(long, value_name = "ROOT")]
        archive: PathBuf,
        /// The receipt's own identifier, as `receipt add` reported it.
        #[arg(value_name = "RECEIPT_ID")]
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
            command: ArchiveCommand::Status(arguments),
        } => emit(
            "archive.status",
            arguments.run(),
            arguments.json,
            status::lines,
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
        Command::Case { command } => cases::run(command),
        Command::Submission { command } => run_submission(command),
        Command::Receipt { command } => run_receipt(command),
        Command::Association { command } => associations::run(command),
    }
}

/// Dispatch one `submission` subcommand and return the process exit code.
fn run_submission(command: SubmissionCommand) -> i32 {
    match command {
        SubmissionCommand::Add {
            archive,
            case_id,
            description,
            date,
            artefacts,
            json,
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
        SubmissionCommand::Show {
            archive,
            submission_id,
            json,
        } => emit(
            "submission.show",
            records::submission::show(&archive, &submission_id),
            json,
            report::submission_shown,
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
        ReceiptCommand::Show {
            archive,
            receipt_id,
            json,
        } => emit(
            "receipt.show",
            records::receipt::show(&archive, &receipt_id),
            json,
            report::receipt_shown,
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
