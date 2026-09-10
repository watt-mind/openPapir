//! The `case import` subcommand: a `case export` directory read back in.
//!
//! It is the one invocation that writes into an archive from a directory the
//! user names rather than from files they list, so the source is checked in
//! full before the archive is written to at all: the manifest is
//! authoritative for what the export holds, and everything it names is read
//! and re-digested first.
//!
//! The rule about what an import restores lives in
//! `openpapir_core::export::restore`. The human lines would live with every
//! other command's in `report`, and are here only because that file is at the
//! source-length limit the repository enforces; they are bound by exactly the
//! rule stated there, so no line carries a user-supplied path except the
//! `--from` argument the user typed in the same invocation, and no line
//! carries an original filename or a payload byte.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::Restored;
use openpapir_core::error::{Failure, Outcome};

/// The arguments `openpapir case import` accepts.
#[derive(Args)]
pub struct Import {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// The export directory to read, as `case export` wrote it.
    #[arg(long = "from", value_name = "DIR")]
    pub source: PathBuf,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl Import {
    /// Restore the export at `--from` into the archive at `--archive`.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::import_case`.
    pub fn run(&self) -> Result<Outcome<Restored>, Failure> {
        openpapir_core::import_case(&self.archive, &self.source)
    }
}

/// The lines `case import` prints when it succeeds.
///
/// The source is the one thing here the JSON does not carry, exactly as the
/// destination is for an export: the line repeats the `--from` argument the
/// user typed in the same invocation and nothing else ever echoes it.
#[must_use]
pub fn lines(restored: &Restored) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Imported case {} from {}.",
            restored.case_id, restored.source
        ),
        format!(
            "Stored {} object(s), {} byte(s); {} already present.",
            restored.objects_stored, restored.bytes_stored, restored.objects_present
        ),
        format!(
            "Wrote {} record(s); {} already present.",
            restored.records_written, restored.records_present
        ),
    ];
    for kind in &restored.records {
        lines.push(format!("{} {}", kind.kind, kind.count));
    }
    lines.push(format!(
        "Recorded {} import event(s) with source export.",
        restored.events_recorded
    ));
    lines.push(
        "Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}
