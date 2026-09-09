//! openPapir's library crate: the local-first correspondence model.
//!
//! # Responsibility
//!
//! This crate will own the local case archive: cases, submissions,
//! attachments, receipts, the preserved original bytes, and the evidence
//! recorded for every association between them. It is the place where that
//! model lives, so that the command-line crate stays a thin presentation
//! layer over it.
//!
//! # Status
//!
//! Scaffold. None of that model exists yet. The crate's entire public surface
//! is [`Capabilities`] and [`capabilities`], which report the implementation
//! stage honestly and perform no I/O and no allocation.
//!
//! # Capabilities contract
//!
//! [`Capabilities`] serializes into the `data` object of the envelope that
//! `openpapir capabilities --json` prints, alongside `schema_version`, `ok`,
//! `command`, and `verified`. Two of its guarantees matter to a consumer:
//!
//! - `operations` is an empty slice, because no correspondence operation is
//!   implemented. It lists operations that can actually process input;
//!   introspection is not one of them.
//! - The envelope's `verified` is always `false`, because this crate performs
//!   no cryptographic check of any kind.
//!
//! The envelope describes capabilities only. It is not a promised response
//! schema for future commands; see `docs/architecture.md`.
//!
//! # Boundaries
//!
//! No correspondence operation, no persistence, no network access, and no
//! filesystem access. KRX container processing belongs to openKRX and `.es3`
//! processing to openSzigno; neither is a dependency of this crate, and
//! neither parser may be copied into it. Nothing here may state or imply
//! authenticity, delivery, or legal effect.

use serde::Serialize;

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
pub const fn capabilities() -> Capabilities {
    Capabilities {
        project: "openPapir",
        stage: "scaffold",
        operations: &[],
    }
}
