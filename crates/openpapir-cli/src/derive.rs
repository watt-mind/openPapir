//! The `archive derive` subcommand: derived metadata on explicit request.
//!
//! This module holds the invocation and its human lines. What a derived
//! record is, what the closed media-type table holds, and why a derived
//! record is disposable and never authoritative all live in
//! `openpapir_core::derived`.
//!
//! A media type is a statement about the leading bytes of a stored object. It
//! is not a verification and not evidence, and no line here may present it as
//! a reason to believe an artefact is a receipt, was delivered, is authentic,
//! or has any legal effect.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::Derived;
use openpapir_core::error::{Failure, Outcome};

/// The arguments `openpapir archive derive` accepts.
#[derive(Args)]
pub struct Derive {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl Derive {
    /// Compute the derived record of every stored object, under the lock.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::derive`.
    pub fn run(&self) -> Result<Outcome<Derived>, Failure> {
        openpapir_core::derive(&self.archive)
    }
}

/// The lines `archive derive` prints without `--json`.
///
/// Counts and the values of the closed table only. No path, no original
/// filename, and no digest of any particular object reaches this output: what
/// was computed is a count per media type, which is the whole of what the
/// privacy rule allows an answer to.
#[must_use]
pub fn lines(derived: &Derived) -> Vec<String> {
    vec![
        format!(
            "Examined {} stored object(s); {} not read; {} byte(s) read to name a type.",
            derived.objects_checked, derived.objects_unchecked, derived.bytes_sniffed
        ),
        format!(
            "Derived record(s) written: {}. Stale record(s) removed: {}.",
            derived.records_written, derived.records_removed
        ),
        format!(
            "By media type: {}.",
            derived
                .media_types
                .iter()
                .map(|entry| format!("{} {}", entry.media_type, entry.count))
                .collect::<Vec<String>>()
                .join(", ")
        ),
        "A media type names what the first bytes look like. It is openPapir's own disposable computation, and it reports no authenticity, no delivery, and no legal effect."
            .to_owned(),
    ]
}

// `Derived` is `#[non_exhaustive]`, so no test outside the library can build
// one by literal. These lines are therefore pinned where they are produced:
// by the real binary against a real archive, in `tests/derive.rs` and in the
// golden files, exactly as `archive check`'s lines are.
