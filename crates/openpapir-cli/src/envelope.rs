//! The response envelope of `docs/error-contract.md`.
//!
//! Exactly one JSON object is written to stdout, on one line, and nothing
//! else. `ok` says whether the command did its stated work; `warnings` says
//! what was weaker than the design promises, and neither implies the other, so
//! a degradation observed before a failure is still reported. `verified` is
//! `false` in every envelope this build emits, because no cryptographic check
//! is implemented.

use openpapir_core::error::{Diagnostic, Warning};
use serde::Serialize;

/// The envelope's version, independent of the archive's schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// One response envelope.
#[derive(Serialize)]
pub struct Envelope<'a, T: Serialize> {
    schema_version: u32,
    ok: bool,
    command: &'a str,
    data: T,
    verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a Diagnostic>,
    #[serde(skip_serializing_if = "is_empty")]
    warnings: &'a [Warning],
}

fn is_empty(warnings: &&[Warning]) -> bool {
    warnings.is_empty()
}

/// Render a successful command as one line of JSON.
pub fn success<T: Serialize>(command: &str, data: T, warnings: &[Warning]) -> String {
    render(&Envelope {
        schema_version: SCHEMA_VERSION,
        ok: true,
        command,
        data,
        verified: false,
        error: None,
        warnings,
    })
}

/// Render a command that ran to completion and found problems.
///
/// `data` carries the command's own report alongside the error, which the
/// whole-archive integrity check needs: its counts are the result the user
/// asked for, and the error names the first problem those counts describe.
/// Every other command renders [`failure`] instead, whose `data` is empty.
pub fn problem<T: Serialize>(
    command: &str,
    data: T,
    error: &Diagnostic,
    warnings: &[Warning],
) -> String {
    render(&Envelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        command,
        data,
        verified: false,
        error: Some(error),
        warnings,
    })
}

/// Render a failed command as one line of JSON, with an empty `data`.
pub fn failure(command: &str, error: &Diagnostic, warnings: &[Warning]) -> String {
    render(&Envelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        command,
        data: serde_json::Map::new(),
        verified: false,
        error: Some(error),
        warnings,
    })
}

/// Serialise an envelope, whose values are always representable as JSON.
fn render<T: Serialize>(envelope: &Envelope<'_, T>) -> String {
    serde_json::to_string(envelope).unwrap_or_else(|_| {
        String::from(
            r#"{"schema_version":1,"ok":false,"command":"unknown","data":{},"verified":false,"error":{"code":"internal.unexpected","message":"A response could not be serialised.","details":{"bucket":"internal"}}}"#,
        )
    })
}
