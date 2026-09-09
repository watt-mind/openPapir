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
//! - `openpapir import --archive <root> <file>... [--json]`, artefact import.
//!
//! There is no case storage, no receipt matching, no association, no export,
//! no deletion, no integrity check, no signature verification, and no
//! government delivery.
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
//! # Boundaries
//!
//! No network access and no background work. Output never carries a
//! user-supplied path, an original filename, or a payload byte. KRX and
//! `.es3` handling belong to openKRX and openSzigno respectively; neither is
//! a dependency. No output may state or imply authenticity, successful
//! delivery, or legal effect.

mod envelope;
mod report;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use openpapir_core::error::{Failure, Outcome};
use openpapir_core::{Capabilities, capabilities};
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

fn main() {
    let code = match Args::parse().command {
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
    };
    std::process::exit(code);
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
    match result {
        Ok(outcome) => {
            if json {
                println!(
                    "{}",
                    envelope::success(command, &outcome.data, &outcome.warnings)
                );
            } else {
                for line in human(&outcome.data) {
                    println!("{line}");
                }
                for warning in &outcome.warnings {
                    eprintln!("warning {}: {}", warning.code, warning.message);
                }
            }
            0
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
