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
//! Scaffold. Exactly three invocations exist:
//!
//! - `openpapir --help`, usage text from the argument parser.
//! - `openpapir --version`, the crate version.
//! - `openpapir capabilities [--json]`, the implementation status.
//!
//! There is no import, no case storage, no receipt matching, no signature
//! verification, and no government delivery. A successful command exits `0`
//! and an unrecognised invocation exits with the argument parser's usage
//! error; the error and exit-code contract in `docs/error-contract.md` is a
//! proposal that no code here implements.
//!
//! # Capabilities envelope
//!
//! `capabilities --json` prints exactly one object on stdout with
//! `schema_version: 1`, `ok: true`, `command: "capabilities"`, `data` holding
//! the core crate's project, stage, and `operations`, and `verified`.
//! `operations` is empty because no correspondence operation is implemented,
//! and `verified` is `false` because no cryptographic check took place. The
//! envelope describes capabilities only and is not a promised response schema
//! for future commands; see `docs/architecture.md`.
//!
//! # Boundaries
//!
//! No network access, no filesystem access, and no persistence. KRX and
//! `.es3` handling belong to openKRX and openSzigno respectively; neither is
//! a dependency. No output may state or imply authenticity, successful
//! delivery, or legal effect.
use clap::{Parser, Subcommand};
use openpapir_core::{Capabilities, capabilities};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "openpapir",
    version,
    about = "openPapir development scaffold; document operations are not implemented"
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
}

#[derive(Serialize)]
struct Response {
    schema_version: u32,
    ok: bool,
    command: &'static str,
    data: Capabilities,
    verified: bool,
}

fn main() {
    match Args::parse().command {
        Command::Capabilities { json } => {
            let data = capabilities();
            if json {
                let response = Response {
                    schema_version: 1,
                    ok: true,
                    command: "capabilities",
                    data,
                    verified: false,
                };
                // This response contains only strings, a bool, an integer and a
                // slice, so serialization cannot encounter unsupported values.
                println!(
                    "{}",
                    serde_json::to_string(&response).expect("serializable response")
                );
            } else {
                println!("{}: {}", data.project, data.stage);
                println!("Document operations: none implemented. Nothing is verified.");
            }
        }
    }
}
