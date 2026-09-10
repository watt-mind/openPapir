//! The `archive export` subcommand: a plain copy of a whole archive.
//!
//! It is `case export` taken over everything the archive holds: every object
//! in the store, every record of every kind, one manifest, and a copy of the
//! archive marker, so the schema version the copy was taken under travels
//! with it. The rule about what an export holds lives in
//! `openpapir_core::export::whole`, and the human lines live with every other
//! command's in `report::transfer`.
//!
//! `archive import` is the other direction. A directory this command wrote is
//! not a `case import` source and a `case export` directory is not an
//! `archive import` source: each refuses the other's export, because what the
//! two describe differs.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::ArchiveExported;
use openpapir_core::error::{Failure, Outcome};

/// The arguments `openpapir archive export` accepts.
#[derive(Args)]
pub struct Export {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// The destination directory, empty or not yet created.
    #[arg(long = "to", value_name = "DIR")]
    pub destination: PathBuf,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl Export {
    /// Copy the archive at `--archive` into the directory at `--to`.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::export_archive`.
    pub fn run(&self) -> Result<Outcome<ArchiveExported>, Failure> {
        openpapir_core::export_archive(&self.archive, &self.destination)
    }
}
