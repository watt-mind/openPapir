//! Writing one generated document to stdout, and nothing else.
//!
//! `skill`, `completions` and `manpage` each hand the caller a document rather
//! than a result: there is no envelope to render and nothing to report in two
//! forms. What they share is the destination and its failure rule, which lives
//! here so no command carries its own copy of it.
//!
//! The rule is the one `skill` documents. The documented install path is a
//! redirection, so a destination that cannot take the bytes is a failure of
//! the command: it would otherwise leave a truncated file behind and still
//! exit `0`. A reader that closed the pipe is the opposite case and is not a
//! failure at all.

use openpapir_core::error::Bucket;
use std::io::{self, ErrorKind, Write};

/// The exit code of the `write` bucket, which a stdout that failed mid-way
/// falls under: the document reached the destination incompletely or not at
/// all. It is taken from the bucket table, and the catalogue is
/// `docs/error-contract.md`.
const WRITE_BUCKET_EXIT: i32 = Bucket::Write.exit_code();

/// Write one document to stdout and return the process exit code.
///
/// `render` writes the document into the handle it is given; `document` names
/// the kind of document for the one line a failing stdout puts on stderr.
/// That line names no path and repeats no argument, because the destination
/// is the caller's own redirection and openPapir never echoes one back.
pub fn emit(document: &str, render: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> i32 {
    let mut handle = io::stdout().lock();
    exit_code(document, write_document(&mut handle, render))
}

/// Write the document to `out`, treating only a closed reader as success.
///
/// The flush is part of the write: a buffered handle can accept every byte
/// and only then fail to hand them on, which is exactly what a full disk
/// looks like from here.
fn write_document(
    out: &mut dyn Write,
    render: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    match render(out).and_then(|()| out.flush()) {
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        outcome => outcome,
    }
}

/// The exit code one write outcome stands for, with its stderr line.
fn exit_code(document: &str, outcome: io::Result<()>) -> i32 {
    if outcome.is_ok() {
        return 0;
    }
    eprintln!("error: the {document} could not be written to stdout");
    WRITE_BUCKET_EXIT
}

#[cfg(test)]
mod tests {
    use super::{WRITE_BUCKET_EXIT, exit_code, write_document};
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

    /// Write a fixed document, as any of the commands' renderers does.
    fn render(out: &mut dyn Write) -> io::Result<()> {
        out.write_all(b"generated")
    }

    /// A destination that took every byte is the ordinary success.
    #[test]
    fn a_destination_that_accepts_the_document_is_a_success() {
        let mut written = Vec::new();
        assert!(write_document(&mut written, render).is_ok());
        assert_eq!(written, b"generated", "the bytes are the same bytes");
        assert_eq!(exit_code("man page", Ok(())), 0);
    }

    /// A reader that closed the pipe asked for exactly this and gets it: the
    /// command reports success and the exit code does not move. This is
    /// `openpapir skill | head -3` and its equivalent for either generated
    /// document.
    #[test]
    fn a_closed_reader_is_not_a_failure_of_the_command() {
        let outcome = write_document(&mut Refusing(ErrorKind::BrokenPipe), render);
        assert!(outcome.is_ok(), "only a closed pipe is swallowed");
        assert_eq!(exit_code("completion script", outcome), 0);
    }

    /// Every other refusal is a failing install path, not a closed reader.
    #[test]
    fn any_other_write_failure_exits_with_the_write_bucket_code() {
        for kind in [
            ErrorKind::StorageFull,
            ErrorKind::PermissionDenied,
            ErrorKind::Other,
        ] {
            let outcome = write_document(&mut Refusing(kind), render);
            assert_eq!(
                outcome.expect_err("the writer refused").kind(),
                kind,
                "the kind is reported as it came back"
            );
            for document in ["completion script", "man page", "skill document"] {
                assert_eq!(
                    exit_code(document, Err(io::Error::new(kind, ""))),
                    WRITE_BUCKET_EXIT,
                    "every document the path writes fails the same way"
                );
            }
        }
    }

    /// A buffered destination that only fails when it is handed on is the
    /// full-disk case, and it is a failure just the same.
    #[test]
    fn a_destination_that_fails_only_on_the_flush_is_a_failure() {
        let outcome = write_document(&mut FailingFlush, render);
        assert_eq!(
            outcome.expect_err("the flush refused").kind(),
            ErrorKind::StorageFull
        );
    }
}
