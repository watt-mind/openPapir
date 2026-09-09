//! Random identifiers and lowercase hexadecimal rendering.
//!
//! Identifiers are 128-bit values from the operating system's cryptographically
//! secure random source, rendered as 32 lowercase hexadecimal characters.
//! Random rather than time-ordered: a sortable identifier would leak when
//! correspondence was imported to anyone who sees a filename listing or a
//! backup (`docs/archive-layout.md`).

use crate::error::{Details, Diagnostic, codes};

/// Render bytes as lowercase hexadecimal.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        // Writing to a String is infallible, so the result cannot be an error.
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Mint a 128-bit identifier as 32 lowercase hexadecimal characters.
///
/// # Errors
///
/// Returns `internal.unexpected` when the operating system's random source is
/// unavailable, because no other code in the contract covers that condition.
pub fn new_id() -> Result<String, Diagnostic> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| {
        Diagnostic::new(
            codes::INTERNAL_UNEXPECTED,
            "The operating system's random source was unavailable.",
            Details::new(),
        )
    })?;
    Ok(hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexadecimal_is_lowercase_and_zero_padded() {
        assert_eq!(hex(&[0, 15, 16, 255]), "000f10ff");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn identifiers_are_thirty_two_hex_characters_and_differ() {
        let first = new_id().unwrap();
        let second = new_id().unwrap();
        assert_eq!(first.len(), 32);
        assert!(
            first
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(first, second, "identifiers come from a random source");
    }
}
