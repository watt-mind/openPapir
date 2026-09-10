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
//! The names `details.argument` may carry are read from the command
//! definition itself rather than kept by hand, so a flag a later change adds
//! cannot fall out of sync with this suite; `known_names` says how it reaches
//! a definition that lives in the binary crate.
//!
//! The case count is bounded because every case is a process; `PROPTEST_CASES`
//! raises it locally, and `docs/testing.md` says how.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

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
const SUBCOMMANDS: [&str; 17] = [
    "capabilities",
    "archive",
    "check",
    "repair-permissions",
    "case",
    "create",
    "list",
    "show",
    "update",
    "export",
    "delete",
    "submission",
    "add",
    "receipt",
    "association",
    "retire",
    "import",
];

/// Long flags for the generator to draw on, plus two this build does not
/// define.
///
/// The list is a palette, not a source of truth: what the assertion accepts is
/// read from the command definition itself by `known_names`, so a flag missing
/// here only narrows what the generator reaches and can never make the
/// property refuse a name this build defines.
const FLAGS: [&str; 27] = [
    "--json",
    "--archive",
    "--title",
    "--notes",
    "--case",
    "--to",
    "--from",
    "--as-of",
    "--purge",
    "--query",
    "--file",
    "--artefact",
    "--description",
    "--date",
    "--receipt",
    "--outcome",
    "--candidate",
    "--supersedes",
    "--label",
    "--status",
    "--tag",
    "--untag",
    "--clear-notes",
    "--import-event",
    "--reason",
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
///
/// The names are read from the command definition rather than kept by hand, so
/// a flag added to the parser cannot fall out of sync with this suite. The
/// definition itself lives in the binary crate, which an integration test
/// cannot import, so it is reached through `openpapir manpage`: that command
/// renders the same `clap::Command` tree the parser and the usage walker use,
/// for every command at every depth. In each `OPTIONS` section `clap_mangen`
/// writes one argument per `.TP` block, and the line straight after `.TP` is
/// the argument's own rendering rather than its description, so every token of
/// that line is a name this build defines once the roff formatting is removed.
///
/// The walk is run once for the whole suite, because every case is a process
/// already.
fn known_names() -> &'static BTreeSet<String> {
    static NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let directory = tempfile::tempdir().expect("a temporary working directory");
        let output = run(directory.path(), &["manpage".to_owned()]);
        assert!(output.status.success(), "the man page is written");
        let manpage = String::from_utf8(output.stdout).expect("a generated man page is UTF-8");
        argument_names(&manpage)
    })
}

/// The argument names a rendered man page stream declares.
fn argument_names(manpage: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut in_options = false;
    let mut after_tag = false;
    for line in manpage.lines() {
        if let Some(section) = line.strip_prefix(".SH ") {
            in_options = section.trim() == "OPTIONS";
            after_tag = false;
        } else if line.trim() == ".TP" {
            after_tag = true;
        } else if std::mem::take(&mut after_tag) && in_options {
            names.extend(
                line.split_whitespace()
                    .flat_map(bare_names)
                    .filter(|name| !name.is_empty()),
            );
        }
    }
    names
}

/// Reduce one rendered token to the bare names the walker could report.
///
/// A man page writes an argument as `\fB\-\-archive\fR \fI<ROOT>\fR` or
/// `[\fIFILE\fR]`, so the roff font escapes and the escaped dashes are undone
/// first. The token is then trimmed twice: once with exactly the characters
/// the binary trims from the name it is willing to echo, so a bracketed
/// value name such as `DIGEST[:ROLE]` keeps its closing bracket the way the
/// walker keeps it, and once more with the brackets and commas the man page
/// adds around an optional positional or between a short and a long flag.
/// Both forms are known; the walker reports one of them.
fn bare_names(token: &str) -> [String; 2] {
    let plain = token
        .replace("\\fB", "")
        .replace("\\fI", "")
        .replace("\\fR", "")
        .replace("\\-", "-")
        .to_lowercase();
    let walker = plain
        .trim_matches(|character: char| matches!(character, '-' | '<' | '>' | '.' | '='))
        .to_owned();
    let wide = plain
        .trim_matches(|character: char| {
            matches!(character, '-' | '<' | '>' | '[' | ']' | '.' | '=' | ',')
        })
        .to_owned();
    [walker, wide]
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

/// The cases the property found, pinned by name so that they run every time.
///
/// `docs/testing.md` asks for a counterexample to be added here rather than
/// left to a generated file, and for the derivation the property leans on to
/// be checked in its own right.
mod regression {
    use super::*;

    /// The names the assertion accepts come from the command definition, so a
    /// flag a later change adds is known here without anyone editing a list.
    ///
    /// `--from` is the one that was missing while the list was kept by hand.
    #[test]
    fn the_known_names_are_read_from_the_command_definition() {
        let names = known_names();
        for name in ["json", "archive", "from", "to", "file", "root", "case_id"] {
            assert!(names.contains(name), "the definition declares {name}");
        }
        assert!(
            !names.contains("not-a-flag"),
            "a flag this build does not define is not known"
        );
        assert!(
            !names.iter().any(|name| name.starts_with("openpapir")),
            "only arguments are read, never the subcommands a page lists"
        );
    }

    /// `archive import` refuses a command line that names no source, and the
    /// argument it names is `--from`, which the parser defines. The property
    /// generates this line, and refused it while the known names were kept by
    /// hand and `--from` was not among them.
    #[test]
    fn archive_import_without_a_source_names_an_argument_this_build_defines() {
        let directory = tempfile::tempdir().expect("a temporary working directory");
        let arguments = [
            "archive".to_owned(),
            "import".to_owned(),
            "--archive".to_owned(),
            format!("../../{MARKER}"),
            "--json".to_owned(),
        ];
        let output = run(directory.path(), &arguments);

        assert_eq!(output.status.code(), Some(2), "a usage refusal exits 2");
        let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
        assert!(stderr.is_empty(), "the JSON form writes nothing to stderr");
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        assert!(
            !stdout.contains(MARKER),
            "the JSON form quoted a value the caller supplied"
        );
        assert_eq!(stdout.lines().count(), 1, "the JSON form is one line");

        let envelope: serde_json::Value =
            serde_json::from_str(&stdout).expect("the JSON form is one JSON object");
        assert_eq!(envelope["schema_version"].as_u64(), Some(1));
        assert_eq!(envelope["ok"].as_bool(), Some(false));
        assert_eq!(envelope["verified"].as_bool(), Some(false));
        assert_eq!(envelope["command"].as_str(), Some("archive.import"));
        assert_eq!(
            envelope["error"]["code"].as_str(),
            Some("usage.arguments"),
            "a malformed command line is a usage refusal"
        );
        let argument = envelope["error"]["details"]["argument"]
            .as_str()
            .expect("the refusal names the argument that is missing");
        assert_eq!(argument, "from");
        assert!(
            known_names().contains(argument),
            "details.argument named something this build does not define"
        );

        assert_eq!(
            fs::read_dir(directory.path())
                .expect("the working directory is read")
                .count(),
            0,
            "a refused command line wrote nothing"
        );
    }
}
