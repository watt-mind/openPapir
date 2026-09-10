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

use std::collections::{BTreeMap, BTreeSet};
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
    NAMES.get_or_init(|| argument_names(manpage()))
}

/// The whole man page stream, written once for the suite.
fn manpage() -> &'static str {
    static MANPAGE: OnceLock<String> = OnceLock::new();
    MANPAGE.get_or_init(|| generated(&["manpage".to_owned()], "a generated man page"))
}

/// The bash completion script, written once for the suite.
///
/// It is generated from the same command definition, by a generator that
/// renders every subcommand and every flag whether or not it is marked
/// hidden, so it is the second opinion the derivation guard compares against.
fn completions() -> &'static str {
    static COMPLETIONS: OnceLock<String> = OnceLock::new();
    COMPLETIONS.get_or_init(|| {
        generated(
            &["completions".to_owned(), "bash".to_owned()],
            "a generated completion script",
        )
    })
}

/// Run one document-writing command and return what it wrote.
fn generated(arguments: &[String], document: &str) -> String {
    let directory = tempfile::tempdir().expect("a temporary working directory");
    let output = run(directory.path(), arguments);
    assert!(output.status.success(), "{document} is written");
    String::from_utf8(output.stdout).unwrap_or_else(|_| panic!("{document} is UTF-8"))
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
/// first. The token is then trimmed twice: once with the characters the binary
/// trims from the name it is willing to echo, plus the comma the man page
/// writes between a short and a long flag, so a bracketed value name such as
/// `DIGEST[:ROLE]` keeps its closing bracket the way the walker keeps it while
/// `\fB\-h\fR,` still reduces to `h` rather than to `h,`; and once more with
/// the brackets the man page adds around an optional positional. Both forms
/// are known; the walker reports one of them.
fn bare_names(token: &str) -> [String; 2] {
    let plain = plain(token);
    let walker = plain
        .trim_matches(|character: char| matches!(character, '-' | '<' | '>' | '.' | '=' | ','))
        .to_owned();
    let wide = plain
        .trim_matches(|character: char| {
            matches!(character, '-' | '<' | '>' | '[' | ']' | '.' | '=' | ',')
        })
        .to_owned();
    [walker, wide]
}

/// One rendered token with the roff font escapes and escaped dashes undone.
fn plain(token: &str) -> String {
    token
        .replace("\\fB", "")
        .replace("\\fI", "")
        .replace("\\fR", "")
        .replace("\\-", "-")
        .to_lowercase()
}

/// The separator the completion script writes between command words.
///
/// `openpapir archive init` is one `openpapir__subcmd__archive__subcmd__init`
/// there, and `openpapir-archive-init` in the man page, so one replacement
/// turns a declared command into the name its page is filed under.
const COMPLETION_SEPARATOR: &str = "__subcmd__";

/// Every command the completion script declares, keyed by the name a page for
/// it is filed under, with the long flags declared for each.
///
/// The script holds one `case` block per command, labelled with that command,
/// and the block sets `opts` to every flag and subcommand name the command
/// takes. A command is entered into the map by its own label, as the label is
/// read, so a command declares itself whether or not a later line in its block
/// turns out to name a flag: a block whose `opts` were missed would otherwise
/// leave the command out of the comparison altogether.
fn declared_commands(completions: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut declared: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current = String::new();
    for line in completions.lines() {
        let line = line.trim();
        if let Some(label) = line.strip_suffix(')')
            && label.starts_with("openpapir")
            && !label.contains(',')
        {
            current = label.replace(COMPLETION_SEPARATOR, "-");
            declared.entry(current.clone()).or_default();
        } else if let Some(list) = line
            .strip_prefix("opts=\"")
            .and_then(|list| list.strip_suffix('"'))
            && let Some(flags) = declared.get_mut(&current)
        {
            flags.extend(long_flags(list.split_whitespace()));
        }
    }
    declared
}

/// Whether `name` is one of the pages clap's own `help` command mirrors.
///
/// clap gives every command a `help` subcommand, and gives that one a mirror
/// of every command below its parent, so the completion script declares
/// `openpapir-help`, `openpapir-archive-help`, `openpapir-help-case-create`
/// and `openpapir-archive-help-check` while the man page renders a page for
/// none of them. A mirror is therefore left out of the comparison, and it has
/// to be shown to be one rather than assumed from the word alone: a hidden
/// subcommand a later change names `help-topics` would carry the word too, and
/// exempting it would hide exactly what this guard is for.
///
/// A name qualifies only when it can be derived from commands that are
/// themselves declared: at the first `help` in the path, the part before it
/// has to be a declared command, and the part after it has to be empty, or
/// `help` again, or name a declared command under that same parent. A hidden
/// `openpapir help-topics` fails on the last of those, because the tree holds
/// no `openpapir topics`.
fn is_help_mirror(name: &str, declared: &BTreeMap<String, BTreeSet<String>>) -> bool {
    let words: Vec<&str> = name.split('-').collect();
    let Some(at) = words.iter().position(|word| *word == "help") else {
        return false;
    };
    let parent = words[..at].join("-");
    if !declared.contains_key(&parent) {
        return false;
    }
    let rest = words[at + 1..].join("-");
    rest.is_empty() || rest == "help" || declared.contains_key(&format!("{parent}-{rest}"))
}

/// Every page the man page stream renders, keyed by its own name, with the
/// long flags each page's `OPTIONS` section declares.
fn rendered_pages(manpage: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut pages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut page = String::new();
    let mut in_options = false;
    let mut after_tag = false;
    for line in manpage.lines() {
        if let Some(title) = line.strip_prefix(".TH ") {
            page = title
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            pages.entry(page.clone()).or_default();
            in_options = false;
            after_tag = false;
        } else if let Some(section) = line.strip_prefix(".SH ") {
            in_options = section.trim() == "OPTIONS";
            after_tag = false;
        } else if line.trim() == ".TP" {
            after_tag = true;
        } else if std::mem::take(&mut after_tag) && in_options {
            let flags = long_flags(line.split_whitespace());
            pages.entry(page.clone()).or_default().extend(flags);
        }
    }
    pages
}

/// The long flags a run of rendered or declared tokens names.
///
/// A positional, a value name, and a short flag are left out: a long flag is
/// the one form both the man page and the completion script spell the same
/// way once the roff escapes and the separating comma are gone.
fn long_flags<'a, I: Iterator<Item = &'a str>>(tokens: I) -> BTreeSet<String> {
    tokens
        .map(|token| plain(token).trim_end_matches(',').to_owned())
        .filter(|token| token.len() > 2 && token.starts_with("--"))
        .collect()
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

    /// The man page renders every command and every argument the definition
    /// declares, so nothing this suite reads its names from can be hidden.
    ///
    /// The derivation skips what is marked hidden twice over: `manpage.rs`
    /// leaves out a hidden subcommand, and `clap_mangen` leaves out a hidden
    /// argument. A command or a flag marked that way would still be accepted
    /// at the command line and would appear in no page, so the walk that
    /// `known_names` reads would not know a name the usage walker can report
    /// and this suite would call a real name an undefined one.
    ///
    /// The shell completions are the second opinion: they are generated from
    /// the same definition by a generator that renders every subcommand and
    /// every long flag whether or not it is hidden. Every command they
    /// declare is required to have a page of its own, and every long flag
    /// they declare for it is required to be in that page.
    ///
    /// Two differences between the two are the generators' own and are not
    /// hidden anything. `help` is clap's own command, and the completions
    /// declare it and one mirror of it under every command while the man page
    /// renders no page for any of them, so a mirror is left out of the
    /// comparison; `is_help_mirror` requires a name to be derivable from
    /// commands that are themselves declared before it is left out, so a
    /// hidden subcommand that merely carries the word, `help-topics` for one,
    /// is compared like any other. `--version` is added to every page by the
    /// derivation itself, so that a page read on its own names the build it
    /// came from, and it is the one flag a page may carry that the definition
    /// does not declare there.
    #[test]
    fn the_command_tree_declares_no_hidden_argument_or_subcommand() {
        let declared = declared_commands(completions());
        let rendered = rendered_pages(manpage());
        assert!(
            declared.len() > SUBCOMMANDS.len(),
            "the completion script declares the whole command tree"
        );

        let mut compared = 0;
        for (name, flags) in &declared {
            if is_help_mirror(name, &declared) {
                continue;
            }
            let page = rendered
                .get(name)
                .unwrap_or_else(|| panic!("the man page stream holds a page for {name}"));
            let hidden: Vec<&String> = flags.difference(page).collect();
            assert!(
                hidden.is_empty(),
                "{name} declares an argument no page renders: {hidden:?}"
            );
            let added: Vec<&String> = page
                .difference(flags)
                .filter(|flag| *flag != "--version")
                .collect();
            assert!(
                added.is_empty(),
                "the page for {name} renders an argument the definition does \
                 not declare there: {added:?}"
            );
            compared += 1;
        }

        for name in rendered.keys() {
            assert!(
                declared.contains_key(name),
                "{name} has a page but is not a command the definition declares"
            );
        }
        assert_eq!(
            compared,
            rendered.len(),
            "every page was compared against the command that owns it"
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
