//! The two import subcommands: an export directory read back in.
//!
//! It is the one invocation that writes into an archive from a directory the
//! user names rather than from files they list, so the source is checked in
//! full before the archive is written to at all: the manifest is
//! authoritative for what the export holds, and everything it names is read
//! and re-digested first.
//!
//! `archive import` is the same invocation at the other scope: it reads a
//! directory `archive export` wrote and restores every case in it as one set.
//! Neither command reads the other's export.
//!
//! The rule about what an import restores lives in
//! `openpapir_core::export::restore`, and the human lines live with every
//! other command's in `report::transfer`.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::error::{Failure, Outcome};
use openpapir_core::{ArchiveRestored, Restored};

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

/// The arguments `openpapir archive import` accepts.
///
/// It is the same source rule at the other scope: a directory
/// `archive export` wrote, restored as one set or not at all.
#[derive(Args)]
pub struct ArchiveImport {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// The export directory to read, as `archive export` wrote it.
    #[arg(long = "from", value_name = "DIR")]
    pub source: PathBuf,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl ArchiveImport {
    /// Restore the whole-archive export at `--from` into `--archive`.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::import_archive`.
    pub fn run(&self) -> Result<Outcome<ArchiveRestored>, Failure> {
        openpapir_core::import_archive(&self.archive, &self.source)
    }
}
