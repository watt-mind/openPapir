//! Invariant 2: the usage walker answers, and never quotes the caller.
//!
//! The argument-parser failure path is the one boundary that sees the raw
//! command line before anything has been validated. Its contract is that any
//! argument vector produces one envelope and one bucketed exit code rather
//! than a panic, that `details.argument` names only a flag or value this build
//! defines, and that no token the caller typed reaches the JSON form at all:
//! an unrecognised token may be a path, and a path is never printed
//! (`docs/error-contract.md`). Without `--json` the parser prints its own
//! usage text, exactly as it always has, so the property constrains the human
//! form by its exit code alone.
//!
//! The suite runs the binary rather than the module, because the walker is
//! private to the binary crate and because the invariant is about what a
//! caller can observe. Every case runs in its own empty working directory,
//! and the property asserts that directory is still empty afterwards, so a
//! generated command line can never leave anything behind.
//!
//! The case count is bounded because every case is a process; `PROPTEST_CASES`
//! raises it locally, and `docs/testing.md` says how.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;

use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;

/// The substring every generated caller value carries.
///
/// It is not a word openPapir writes, so finding it in the output means the
/// output quoted the caller.
const MARKER: &str = "qzmarker";

/// The exit codes the contract allows. `1` is deliberately absent, and `101`
/// is what a panicking Rust binary would report.
const PERMITTED_EXITS: [i32; 6] = [0, 2, 3, 4, 5, 6];

/// The subcommand names the walk may recognise.
///
/// `init` is left out on purpose: it is the one command that takes its archive
/// root as a positional and would create a directory, and this suite asserts
/// that no generated command line writes anything.
const SUBCOMMANDS: [&str; 15] = [
    "capabilities",
    "archive",
    "check",
    "repair-permissions",
    "case",
    "create",
    "list",
    "show",
    "export",
    "delete",
    "submission",
    "add",
    "receipt",
    "association",
    "import",
];

/// The long flags this build defines, plus two it does not.
const FLAGS: [&str; 17] = [
    "--json",
    "--archive",
    "--title",
    "--notes",
    "--case",
    "--to",
    "--artefact",
    "--description",
    "--date",
    "--receipt",
    "--outcome",
    "--candidate",
    "--supersedes",
    "--label",
    "--import-event",
    "--not-a-flag",
    "--",
];

fn config(default_cases: u32) -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|cases| *cases > 0)
        .unwrap_or(default_cases);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// A value a caller might type, always carrying the marker.
fn caller_value() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(format!("../../{MARKER}")),
        Just(format!("/{MARKER}/secret")),
        Just(format!("{MARKER}{}", "a".repeat(300))),
        Just(format!("--{MARKER}")),
        Just(format!("-{MARKER}")),
        Just(format!("{MARKER}\u{202e}")),
        Just(MARKER.to_owned()),
    ]
}

/// One token of a generated command line.
fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => proptest::sample::select(SUBCOMMANDS.to_vec()).prop_map(str::to_owned),
        3 => proptest::sample::select(FLAGS.to_vec()).prop_map(str::to_owned),
        2 => caller_value(),
    ]
}

/// A generated command line, always asking for the JSON form.
///
/// `--json` goes last, where a caller writes it: `openpapir` defines it per
/// subcommand rather than globally, so leading with it would make every single
/// case the same leading-flag parse failure and the walk would never be
/// exercised past the first token.
fn command_line() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(token(), 0..6).prop_map(|mut tokens| {
        tokens.push("--json".to_owned());
        tokens
    })
}

/// Every flag and value name this build defines, in the walker's bare form.
///
/// `details.argument` may name one of these and nothing else.
fn known_names() -> BTreeSet<String> {
    let mut names: BTreeSet<String> = FLAGS
        .iter()
        .filter_map(|flag| flag.strip_prefix("--"))
        .filter(|flag| !flag.is_empty())
        .map(str::to_owned)
        .collect();
    for name in [
        "file",
        "root",
        "case_id",
        "dir",
        "title",
        "notes",
        "date",
        "digest",
        "description",
        "label",
        "outcome",
        "candidate",
        "receipt_id",
        "import_event_id",
        "association_id",
        "artefact",
        "statement",
        "role",
        "destination",
        "archive_root",
    ] {
        names.insert(name.to_owned());
    }
    names
}

/// Whether the command line asked for the JSON form.
///
/// The rule is the binary's own: the scan stops at `--`, after which a token
/// is a positional the caller supplied and never a flag openPapir defines. A
/// command line that did not ask for the JSON form gets the parser's own usage
/// text on stderr, which is the documented human behaviour and is not this
/// property's subject.
fn json_requested(arguments: &[String]) -> bool {
    arguments
        .iter()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument == "--json")
}

fn run(directory: &Path, arguments: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(arguments.iter().map(OsString::from))
        .current_dir(directory)
        .output()
        .expect("the binary runs")
}

/// The property itself, in a module so that its reported name carries
/// `property` and the suite's own filter selects it.
mod property {
    use super::*;

    proptest! {
        #![proptest_config(config(64))]

        /// Any command line asking for the JSON form is answered by one envelope
        /// on stdout, with a bucketed exit code, an `argument` this build defines
        /// if it names one at all, and no token the caller typed.
        #[test]
        fn any_command_line_is_answered_without_quoting_the_caller(
            arguments in command_line(),
        ) {
            let directory = tempfile::tempdir().expect("a temporary working directory");
            let output = run(directory.path(), &arguments);

            let code = output.status.code().expect("the process was not signalled");
            prop_assert!(
                PERMITTED_EXITS.contains(&code),
                "the binary used an exit code outside the bucket table"
            );

            let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
            let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
            prop_assert!(
                !stderr.contains("panicked"),
                "the binary panicked instead of refusing"
            );

            if !json_requested(&arguments) {
                // The human form is the parser's own usage text, which this
                // property does not constrain beyond the exit code.
                prop_assert!(stdout.is_empty() || code == 0);
            } else {
                // The privacy rule and the envelope's shape hold whatever the
                // command line turned out to be: a caller's value is no more
                // printable on the path that succeeded than on the one that
                // refused.
                prop_assert!(
                    !stdout.contains(MARKER) && !stderr.contains(MARKER),
                    "the JSON form quoted a value the caller supplied"
                );
                prop_assert!(stderr.is_empty(), "the JSON form writes nothing to stderr");
                prop_assert_eq!(stdout.lines().count(), 1, "the JSON form is one line");
                let envelope: serde_json::Value =
                    serde_json::from_str(&stdout).expect("the JSON form is one JSON object");
                prop_assert_eq!(envelope["schema_version"].as_u64(), Some(1));
                prop_assert_eq!(envelope["ok"].as_bool(), Some(code == 0));
                prop_assert_eq!(envelope["verified"].as_bool(), Some(false));
                prop_assert!(envelope["command"].is_string());
                if code != 0 {
                    let error = &envelope["error"];
                    prop_assert!(error["code"].is_string(), "a refusal carries a code");
                    if let Some(argument) = error["details"]["argument"].as_str() {
                        prop_assert!(
                            known_names().contains(argument),
                            "details.argument named something this build does not define"
                        );
                    }
                }
            }

            prop_assert_eq!(
                fs::read_dir(directory.path())
                    .expect("the working directory is read")
                    .count(),
                0,
                "a generated command line wrote nothing"
            );
        }
    }
}
