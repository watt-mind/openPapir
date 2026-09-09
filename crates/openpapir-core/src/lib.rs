//! openPapir development foundation.
//!
//! No document processing is implemented. Capabilities describe only operations
//! that can actually process input; bootstrap introspection is not one of them.

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
