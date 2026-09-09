//! The `skill` command: write the embedded agent skill document to stdout,
//! byte for byte and with nothing added.
//!
//! The document is the agent-facing description of this CLI: when to reach
//! for it, the envelope, the exit codes, the privacy rule, and the boundary
//! between imported, matched, and authenticity-verified. It is embedded at
//! build time so the binary can hand it out with no file alongside it, and
//! the copy under `crates/openpapir-cli/skills/openpapir/` is the same bytes.

use openpapir_core::error::Bucket;
use std::io::{self, ErrorKind, Write};

/// The embedded agent skill, byte for byte as the repository holds it.
pub const SKILL: &str = include_str!("../skills/openpapir/SKILL.md");

/// The exit code of the `write` bucket, which a stdout that failed mid-way
/// falls under: the document reached the destination incompletely or not at
/// all. It is taken from the bucket table, which is the single source, and
/// the catalogue is `docs/error-contract.md`.
const WRITE_BUCKET_EXIT: i32 = Bucket::Write.exit_code();

/// The one line a failing stdout puts on stderr.
///
/// It names no path and repeats no argument, because the destination is the
/// caller's own redirection and openPapir never echoes one back.
const WRITE_FAILED: &str = "error: the skill document could not be written to stdout";

/// Write the skill to stdout and return the process exit code.
///
/// The bytes are written through the raw handle rather than a formatting
/// macro, so nothing is added, removed, or re-encoded on any platform. A
/// stdout that a pager or `head` closed is not a failure of this command and
/// does not change the exit code: the run still reports success, exactly as a
/// reader that stopped reading intended. Every other write or flush failure
/// is a failure of the documented `openpapir skill > SKILL.md` install path
/// and exits with the `write` bucket's code, so a full disk cannot leave a
/// truncated document behind and still report success.
pub fn emit() -> i32 {
    let mut stdout = io::stdout().lock();
    exit_code(write_document(&mut stdout))
}

/// Write the document to `out`, treating only a closed reader as success.
///
/// The flush is part of the write: a buffered handle can accept every byte
/// and only then fail to hand them on, which is exactly what a full disk
/// looks like from here.
fn write_document(out: &mut impl Write) -> io::Result<()> {
    match out.write_all(SKILL.as_bytes()).and_then(|()| out.flush()) {
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        outcome => outcome,
    }
}

/// The exit code one write outcome stands for, with its stderr line.
fn exit_code(outcome: io::Result<()>) -> i32 {
    if outcome.is_ok() {
        return 0;
    }
    eprintln!("{WRITE_FAILED}");
    WRITE_BUCKET_EXIT
}

#[cfg(test)]
mod tests {
    use super::{SKILL, WRITE_BUCKET_EXIT, exit_code, write_document};
    use std::io::{self, ErrorKind, Write};

    /// A writer that refuses every byte with one fixed kind.
    struct Refusing(ErrorKind);

    impl Write for Refusing {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(self.0, "refused"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(self.0, "refused"))
        }
    }

    /// A writer that accepts every byte and fails only when it is flushed,
    /// which is what a buffered handle over a full disk does.
    struct FailingFlush;

    impl Write for FailingFlush {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(ErrorKind::StorageFull, "no space"))
        }
    }

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

    /// A destination that took every byte is the ordinary success.
    #[test]
    fn a_destination_that_accepts_the_document_is_a_success() {
        let mut written = Vec::new();
        assert!(write_document(&mut written).is_ok());
        assert_eq!(written, SKILL.as_bytes(), "the bytes are the same bytes");
        assert_eq!(exit_code(Ok(())), 0);
    }

    /// A reader that closed the pipe asked for exactly this and gets it: the
    /// command reports success and the exit code does not move.
    #[test]
    fn a_closed_reader_is_not_a_failure_of_this_command() {
        let outcome = write_document(&mut Refusing(ErrorKind::BrokenPipe));
        assert!(outcome.is_ok(), "only a closed pipe is swallowed");
        assert_eq!(exit_code(outcome), 0);
    }

    /// Every other refusal is a failing install path, not a closed reader.
    #[test]
    fn any_other_write_failure_exits_with_the_write_bucket_code() {
        for kind in [
            ErrorKind::StorageFull,
            ErrorKind::PermissionDenied,
            ErrorKind::Other,
        ] {
            let outcome = write_document(&mut Refusing(kind));
            assert_eq!(
                outcome.expect_err("the writer refused").kind(),
                kind,
                "the kind is reported as it came back"
            );
            assert_eq!(exit_code(Err(io::Error::new(kind, ""))), WRITE_BUCKET_EXIT);
        }
    }

    /// A buffered destination that only fails when it is handed on is the
    /// full-disk case, and it is a failure just the same.
    #[test]
    fn a_destination_that_fails_only_on_the_flush_is_a_failure() {
        let outcome = write_document(&mut FailingFlush);
        assert_eq!(
            outcome.expect_err("the flush refused").kind(),
            ErrorKind::StorageFull
        );
    }
}
