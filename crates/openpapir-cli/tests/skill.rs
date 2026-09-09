//! `openpapir skill` against real destinations: a reader that closed the
//! pipe, and a destination that cannot take the bytes.
//!
//! The documented install path is `openpapir skill > .claude/skills/…`, so a
//! destination that refuses the write has to be reported. A closed reader is
//! the opposite case: `openpapir skill | head -3` is in the documentation and
//! is not a failure of the command.
//!
//! The unit tests in `src/skill.rs` pin the same rule against writers that
//! fail deterministically; these run the binary itself.
#![cfg(unix)]

use std::io::Read;
use std::process::{Command, Stdio};

/// Run the binary with stdout inherited from `destination`.
fn skill_into(destination: Stdio) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .arg("skill")
        .stdout(destination)
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the openpapir binary under test")
        .wait()
        .expect("wait for the openpapir binary under test")
}

/// A reader that closed the pipe asked the command to stop, so the run still
/// reports success. This is `openpapir skill | head -3` from the docs.
#[test]
fn a_closed_reader_leaves_the_exit_code_at_zero() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .arg("skill")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the openpapir binary under test");
    let mut stdout = child.stdout.take().expect("the child's stdout is piped");
    let mut first = [0_u8; 3];
    stdout.read_exact(&mut first).expect("read the first bytes");
    assert_eq!(&first, b"---", "the document starts with its front matter");
    drop(stdout);
    let output = child.wait_with_output().expect("wait for the child");
    assert_eq!(
        output.status.code(),
        Some(0),
        "a reader that stopped reading is not a failure of this command"
    );
    assert!(
        output.stderr.is_empty(),
        "a closed reader is not explained on stderr"
    );
}

/// A destination that cannot take the bytes is a failing install, not a
/// success with a truncated file: the run exits with the `write` bucket's
/// code and says so in one line that carries no path.
#[test]
#[cfg(target_os = "linux")]
fn a_destination_that_cannot_take_the_bytes_exits_four() {
    let full = match std::fs::OpenOptions::new().write(true).open("/dev/full") {
        Ok(full) => full,
        Err(error) => {
            println!("skipped: /dev/full is not writable here ({error})");
            return;
        }
    };
    let child = Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .arg("skill")
        .stdout(Stdio::from(full))
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the openpapir binary under test");
    let output = child
        .wait_with_output()
        .expect("wait for the openpapir binary under test");
    assert_eq!(
        output.status.code(),
        Some(4),
        "a failing stdout is the write bucket's exit code"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert_eq!(stderr.lines().count(), 1, "one line and nothing else");
    assert!(stderr.contains("could not be written"));
    assert!(
        !stderr.contains('/'),
        "no path reaches the message: {stderr}"
    );
}

/// Everywhere without `/dev/full` the failing-destination case is stated
/// rather than silently absent, and the deterministic form of it lives in the
/// unit tests of `src/skill.rs`.
#[test]
#[cfg(not(target_os = "linux"))]
fn the_failing_destination_case_is_covered_by_the_unit_tests_here() {
    println!(
        "skipped: this platform has no /dev/full; the failing-writer rule is \
         pinned by the unit tests in src/skill.rs"
    );
}

/// A destination that takes every byte is the ordinary success, and the
/// document is the whole output.
#[test]
fn a_destination_that_takes_the_bytes_exits_zero() {
    let status = skill_into(Stdio::null());
    assert_eq!(status.code(), Some(0));
}
