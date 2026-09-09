//! The `skill` command: write the embedded agent skill document to stdout,
//! byte for byte and with nothing added.
//!
//! The document is the agent-facing description of this CLI: when to reach
//! for it, the envelope, the exit codes, the privacy rule, and the boundary
//! between imported, matched, and authenticity-verified. It is embedded at
//! build time so the binary can hand it out with no file alongside it, and
//! the copy under `crates/openpapir-cli/skills/openpapir/` is the same bytes.

use std::io::Write;

/// The embedded agent skill, byte for byte as the repository holds it.
pub const SKILL: &str = include_str!("../skills/openpapir/SKILL.md");

/// Write the skill to stdout and return the process exit code.
///
/// The bytes are written through the raw handle rather than a formatting
/// macro, so nothing is added, removed, or re-encoded on any platform. A
/// stdout that a pager or `head` closed is not a failure of this command and
/// does not change the exit code: the run still reports success, exactly as a
/// reader that stopped reading intended.
pub fn emit() -> i32 {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(SKILL.as_bytes());
    let _ = stdout.flush();
    0
}

#[cfg(test)]
mod tests {
    use super::SKILL;

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
            "openpapir case list --archive ROOT --json",
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
}
