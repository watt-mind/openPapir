//! Bootstrap command-line interface. No document processing is implemented.
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
