//! The `skill` command: write the embedded agent skill document to stdout,
//! byte for byte and with nothing added.
//!
//! The document is the agent-facing description of this CLI: when to reach
//! for it, the envelope, the exit codes, the privacy rule, and the boundary
//! between imported, matched, and authenticity-verified. It is embedded at
//! build time so the binary can hand it out with no file alongside it, and
//! the copy under `crates/openpapir-cli/skills/openpapir/` is the same bytes.

use std::io::{self, Write};

use crate::stdout;

/// The embedded agent skill, byte for byte as the repository holds it.
pub const SKILL: &str = include_str!("../skills/openpapir/SKILL.md");

/// The kind of document a failing stdout names on stderr.
const DOCUMENT: &str = "skill document";

/// Write the skill to stdout and return the process exit code.
///
/// The bytes go through the shared stdout path, so this command's rule for a
/// destination that cannot take them is the one `completions` and `manpage`
/// follow and there is one implementation of it. The bytes are written
/// through the raw handle rather than a formatting macro, so nothing is
/// added, removed, or re-encoded on any platform.
pub fn emit() -> i32 {
    stdout::emit(DOCUMENT, document)
}

/// Write the embedded document, which is the whole of this command's output.
fn document(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(SKILL.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{SKILL, document};

    /// The document is a skill an agent can install, so its front matter and
    /// its boundary have to survive any edit to it.
    #[test]
    fn the_embedded_skill_declares_its_front_matter_and_its_boundary() {
        assert!(SKILL.starts_with("---\nname: openpapir\n"));
        assert!(SKILL.contains("\nlicense: MIT\n"));
        assert!(SKILL.contains("  author: watt-mind\n"));
        assert!(SKILL.contains("  source: https://github.com/watt-mind/openPapir\n"));
        assert!(SKILL.contains("`verified` is `false` in every envelope"));
        assert!(SKILL.contains("It opens no socket."));
        assert!(SKILL.ends_with('\n'));
    }

    /// Every operation `capabilities` reports is described for the agent, so
    /// the skill cannot fall behind the implementation.
    #[test]
    fn the_embedded_skill_names_every_implemented_command() {
        for invocation in [
            "openpapir capabilities --json",
            "openpapir archive init ROOT --json",
            "openpapir archive check --archive ROOT --json",
            "openpapir archive repair-permissions --archive ROOT --json",
            "openpapir import --archive ROOT FILE... --json",
            "openpapir case list --archive ROOT [--status open|closed] [--tag TAG]...",
            "openpapir case update --archive ROOT CASE_ID [--title T]",
            "openpapir case show --archive ROOT CASE_ID --json",
            "openpapir case delete --archive ROOT --case CASE_ID [--purge] --json",
            "openpapir receipt list --archive ROOT --json",
            "openpapir association list --archive ROOT --receipt RECEIPT_ID --json",
            "openpapir skill",
        ] {
            assert!(SKILL.contains(invocation), "the skill omits {invocation}");
        }
    }

    /// The privacy rule binds the document as firmly as it binds the output.
    #[test]
    fn the_embedded_skill_states_the_privacy_rule() {
        assert!(SKILL.contains("no original filename"));
        assert!(SKILL.contains("no payload byte"));
    }

    /// The renderer hands the shared write path the whole embedded document
    /// and nothing else, so what stdout receives is byte for byte the copy
    /// the repository holds.
    #[test]
    fn the_renderer_writes_the_embedded_document_byte_for_byte() {
        let mut written = Vec::new();
        document(&mut written).expect("a vector takes every byte");
        assert_eq!(written, SKILL.as_bytes(), "the bytes are the same bytes");
    }
}
