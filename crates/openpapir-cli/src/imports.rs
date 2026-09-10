//! The `import` subcommand: local files into the artefact store.
//!
//! Import on its own stores bytes and records one import event per input.
//! With `--case` it is the one-step form of `submission add --file`, seen
//! from the other side: the same writer lock imports the files and records
//! one submission naming every one of them, so a user who has just sent
//! something states it once. `--case` needs `--description`, because a
//! submission is the user's own statement of what they sent and there is
//! nothing to record without one.
//!
//! The rules live in `openpapir_core::archive::import` and
//! `openpapir_core::records::submission`. The human lines would live with
//! every other command's in `report`, and are here only because that file is
//! at the source-length limit the repository enforces; they are bound by
//! exactly the rule stated there, so no line carries a user-supplied path, an
//! original filename, or a payload byte.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::ImportedIntoCase;
use openpapir_core::error::{Failure, Outcome};

use crate::{emit, report};

/// The arguments `openpapir import` accepts.
#[derive(Args)]
pub struct Import {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// Also record one submission against this case, naming every file.
    #[arg(long = "case", value_name = "CASE_ID", requires = "description")]
    pub case_id: Option<String>,
    /// The user's own description of the submission, at most 1024 bytes.
    #[arg(long, value_name = "DESCRIPTION", requires = "case_id")]
    pub description: Option<String>,
    /// The user's own date, `YYYY-MM-DD`, stored verbatim.
    #[arg(long, value_name = "DATE", requires = "case_id")]
    pub date: Option<String>,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
    /// The files to import.
    #[arg(value_name = "FILE", required = true)]
    pub files: Vec<PathBuf>,
}

impl Import {
    /// Import the named files, recording a submission when `--case` was given.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `openpapir_core::import` and, for the one-step
    /// form, those of `submission add` as well.
    pub fn recorded(&self, case_id: &str) -> Result<Outcome<ImportedIntoCase>, Failure> {
        openpapir_core::import_into_case(
            &self.archive,
            &self.files,
            case_id,
            self.description.as_deref().unwrap_or_default(),
            self.date.as_deref(),
        )
    }
}

/// Run one `import` invocation and return the process exit code.
pub fn run(arguments: Import) -> i32 {
    match arguments.case_id.clone() {
        Some(case_id) => emit(
            "import",
            arguments.recorded(&case_id),
            arguments.json,
            lines,
        ),
        None => emit(
            "import",
            openpapir_core::import(&arguments.archive, &arguments.files),
            arguments.json,
            report::imported,
        ),
    }
}

/// The lines `import --case` prints when it succeeds.
///
/// The import's own lines come first, because the files are what the user
/// named, and the record they were filed under follows. Both halves are the
/// lines the two separate commands print, so a reader of either recognises
/// this one.
fn lines(data: &ImportedIntoCase) -> Vec<String> {
    let mut lines = report::imported(&data.imported);
    lines.extend(report::submission_added(&data.submission));
    lines
}
