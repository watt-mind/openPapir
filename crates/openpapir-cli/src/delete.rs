//! The `case delete` subcommand: the one destructive invocation openPapir has.
//!
//! Deleting a case removes records. It removes an object only when the user
//! asks for a purge in so many words, with `--purge`, because an object holds
//! the original bytes of the user's own correspondence and nothing else in
//! this tool can bring them back.
//!
//! This module holds the invocation only. The rule about what a deletion
//! removes lives in `openpapir_core::deletion`, and the human lines live with
//! every other command's in `report`.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::deletion::Deleted;
use openpapir_core::error::{Diagnostic, Failure, Outcome};

/// The arguments `openpapir case delete` accepts.
#[derive(Args)]
pub struct Delete {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// The case to delete, as `case create` reported its identifier.
    #[arg(long = "case", value_name = "CASE_ID")]
    pub case_id: String,
    /// Also unlink every object no remaining record references.
    #[arg(long)]
    pub purge: bool,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl Delete {
    /// Delete the named case, purging objects only when `--purge` was given.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::delete`.
    pub fn run(&self) -> Result<Outcome<Deleted>, Failure> {
        openpapir_core::delete(&self.archive, &self.case_id, self.purge)
    }
}

/// The objects a purge could not remove, and the exit code they map to.
///
/// The deletion did the rest of its stated work, so its counts stay in `data`
/// and the error names only how many objects are still in the store.
#[must_use]
pub fn retained(deleted: &Deleted) -> Option<(Diagnostic, i32)> {
    deleted.objects_retained().map(|error| {
        let code = error.exit_code();
        (error, code)
    })
}
