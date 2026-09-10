//! openPapir's library crate: the local-first correspondence model.
//!
//! # Responsibility
//!
//! This crate owns the local case archive. Today that means the archive root
//! and its marker, the content-addressed artefact store, the atomic write
//! procedure, the single-writer lock, the input caps, the import-event
//! records that import writes, the case, submission, receipt, and
//! user-asserted association records, the read-only whole-archive
//! integrity check, the read-only summary and its receipt-retrieval
//! reminders, the export of one case or of a whole archive, the import of
//! either export back into an archive, the permission repair that restoring
//! an export or a backup needs, the derived-metadata records `derive`
//! computes on request, and the `search` scan over the user's own record
//! text. Verification results are designed in
//! `docs/archive-layout.md` and are not implemented.
//!
//! # Status
//!
//! The implemented operations are the ones [`capabilities`] reports, and
//! `docs/architecture.md` enumerates them beside their invocations. `skill`
//! is the one that touches no archive: it belongs to the CLI, which writes
//! the agent skill document it carries, and is reported here so that a
//! machine caller learns of it from the same list as every other operation.
//! Everything else in the design stays a plan: no editing of a stored record
//! other than the case record `case.update` rewrites, no deletion of a single
//! submission or receipt, no deletion of an
//! archive, no automatic matching, no receipt parsing, and no verification of
//! any kind. Derived metadata is the media type and the byte length of a
//! stored object, computed only when `derive` is asked for it, disposable,
//! and never authoritative. An export
//! copies the bytes the archive already holds and changes nothing inside it,
//! the permission repair only narrows, and a deletion removes an object only
//! on an explicit purge. The integrity check re-digests
//! stored bytes, which is a storage-layer identity check and never a
//! cryptographic verification. Every record here is the user's own local organisation:
//! openPapir sends nothing, and reads artefact bytes only to re-digest, to
//! copy, and to name a media type, so a submission, a receipt, and an
//! association are all user-asserted and assert no delivery, receipt by an
//! authority, authenticity, or legal effect.
//!
//! # Capabilities contract
//!
//! [`Capabilities`] serializes into the `data` object of the envelope that
//! `openpapir capabilities --json` prints, alongside `schema_version`, `ok`,
//! `command`, and `verified`. Two of its guarantees matter to a consumer:
//!
//! - `operations` lists exactly the operations that can process input.
//! - The envelope's `verified` is always `false`, because this crate performs
//!   no cryptographic check of any kind. A SHA-256 digest here is a
//!   storage-layer identity: it says two files hold the same bytes, and
//!   nothing about authenticity, origin, delivery, or legal effect.
//!
//! # Boundaries
//!
//! No network access and no background work. KRX container processing belongs
//! to openKRX and `.es3` processing to openSzigno; neither is a dependency of
//! this crate, and neither parser may be copied into it. Nothing here may
//! state or imply authenticity, delivery, or legal effect.

pub mod archive;
pub mod clock;
pub mod deletion;
pub mod derived;
pub mod error;
pub mod export;
pub mod ident;
pub mod integrity;
pub mod records;
pub mod status;

pub use archive::import::{Artefact, Imported, ImportedIntoCase, import, import_into_case};
pub use archive::{Created, init, repair_permissions};
pub use deletion::{Deleted, RemovedRecords, RetainedObjects, delete};
pub use derived::{Derived, MediaCount, derive};
pub use error::{Diagnostic, Failure, Outcome, Warning};
pub use export::repair::Repaired;
pub use export::restore::{ArchiveRestored, Restored, import_archive, import_case};
pub use export::whole::{ArchiveExported, export_archive};
pub use export::{Exported, KindCount, export_case};
pub use integrity::{Report, check};
pub use records::association::{
    Association, AssociationCreated, AssociationHistory, AssociationView, Candidate, Evidence,
};
pub use records::case::{
    Case, CaseCreated, CaseList, CaseReceipt, CaseUpdated, CaseView, Filter as CaseFilter,
    Status as CaseStatus,
};
pub use records::derived::{DerivedFacts, DerivedMetadata};
pub use records::receipt::{Receipt, ReceiptAdded, ReceiptList, ReceiptView};
pub use records::search::{Found, Hit, Kind as SearchKind, search};
pub use records::submission::{ArtefactRef, FileRef, Submission, SubmissionAdded, SubmissionView};

use serde::Serialize;
use std::fmt;

/// The operations that can process input today.
const OPERATIONS: &[&str] = &[
    "archive.init",
    "import",
    "case.create",
    "case.list",
    "case.show",
    "submission.add",
    "receipt.add",
    "receipt.list",
    "association.create",
    "association.list",
    "association.retire",
    "archive.check",
    "archive.status",
    "case.export",
    "case.import",
    "archive.repair_permissions",
    "case.delete",
    "skill",
    "case.update",
    "submission.show",
    "receipt.show",
    "association.show",
    "completions",
    "manpage",
    "archive.export",
    "archive.import",
    "archive.derive",
    "search",
];

/// The closed set of implementation stages `capabilities` may report, in
/// order. The reported `stage` is always one of these variants, and the set
/// grows or shrinks only with a documented release decision. Every variant
/// serializes and displays as its lowercase name. See the capabilities
/// contract in `docs/architecture.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// Nothing processes input yet.
    Scaffold,
    /// Operations are implemented against a layout that may still change.
    Alpha,
    /// The layout is settled and the implemented operations are hardening.
    Beta,
    /// The contract is supported and grows only additively.
    Stable,
}

impl Stage {
    /// The stage's stable lowercase name, as it appears in the envelope.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scaffold => "scaffold",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Stable => "stable",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Machine-readable implementation status; never a verification verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Public project name.
    pub project: &'static str,
    /// Current implementation stage, one of the closed [`Stage`] set.
    pub stage: Stage,
    /// Implemented document or workflow operations.
    pub operations: &'static [&'static str],
}

/// Return the current implementation status without I/O or side effects.
#[must_use]
pub const fn capabilities() -> Capabilities {
    Capabilities {
        project: "openPapir",
        stage: Stage::Alpha,
        operations: OPERATIONS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_report_exactly_the_implemented_operations() {
        let reported = capabilities();
        assert_eq!(
            reported.operations,
            [
                "archive.init",
                "import",
                "case.create",
                "case.list",
                "case.show",
                "submission.add",
                "receipt.add",
                "receipt.list",
                "association.create",
                "association.list",
                "association.retire",
                "archive.check",
                "archive.status",
                "case.export",
                "case.import",
                "archive.repair_permissions",
                "case.delete",
                "skill",
                "case.update",
                "submission.show",
                "receipt.show",
                "association.show",
                "completions",
                "manpage",
                "archive.export",
                "archive.import",
                "archive.derive",
                "search"
            ]
        );
        assert_eq!(reported.project, "openPapir");
        assert_eq!(reported.stage, Stage::Alpha);
    }

    /// The type closes the vocabulary, so the only thing left to pin is the
    /// wording of each variant. The match is exhaustive: a new stage does not
    /// compile until it is given a name here and documented.
    #[test]
    fn every_stage_keeps_its_documented_lowercase_name() {
        for stage in [Stage::Scaffold, Stage::Alpha, Stage::Beta, Stage::Stable] {
            let name = match stage {
                Stage::Scaffold => "scaffold",
                Stage::Alpha => "alpha",
                Stage::Beta => "beta",
                Stage::Stable => "stable",
            };
            assert_eq!(stage.as_str(), name);
            assert_eq!(stage.to_string(), name);
            assert_eq!(
                serde_json::to_value(stage).expect("a stage serializes"),
                serde_json::Value::String(name.to_owned())
            );
        }
    }
}
