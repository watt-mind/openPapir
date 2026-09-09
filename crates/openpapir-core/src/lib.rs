//! openPapir's library crate: the local-first correspondence model.
//!
//! # Responsibility
//!
//! This crate owns the local case archive. Today that means the archive root
//! and its marker, the content-addressed artefact store, the atomic write
//! procedure, the single-writer lock, the input caps, the import-event
//! records that import writes, and the case and submission records. Receipts,
//! associations, derived metadata, and verification results are designed in
//! `docs/archive-layout.md` and are not implemented.
//!
//! # Status
//!
//! Six operations are implemented, `archive.init`, `import`, `case.create`,
//! `case.list`, `case.show`, and `submission.add`, and they are the six
//! [`capabilities`] reports. Everything else in the design stays a plan: no
//! export, no deletion, no editing, no integrity check, no matching, no
//! receipt parsing, and no verification of any kind. A case and a submission
//! are the user's own local organisation: openPapir sends nothing, so a
//! submission is always user-asserted and asserts no delivery, receipt by an
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
pub mod error;
pub mod ident;
pub mod records;

pub use archive::import::{Artefact, Imported, import};
pub use archive::{Created, init};
pub use error::{Diagnostic, Failure, Outcome, Warning};
pub use records::case::{Case, CaseCreated, CaseList, CaseView};
pub use records::submission::{ArtefactRef, Submission, SubmissionAdded};

use serde::Serialize;

/// The operations that can process input today.
const OPERATIONS: &[&str] = &[
    "archive.init",
    "import",
    "case.create",
    "case.list",
    "case.show",
    "submission.add",
];

/// Machine-readable implementation status; never a verification verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Public project name.
    pub project: &'static str,
    /// Current implementation stage.
    pub stage: &'static str,
    /// Implemented document or workflow operations.
    pub operations: &'static [&'static str],
}

/// Return the current implementation status without I/O or side effects.
#[must_use]
pub const fn capabilities() -> Capabilities {
    Capabilities {
        project: "openPapir",
        stage: "scaffold",
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
                "submission.add"
            ]
        );
        assert_eq!(reported.project, "openPapir");
        assert_eq!(reported.stage, "scaffold");
    }
}
