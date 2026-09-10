//! Executable contract tests use synthetic arguments only.
use std::process::Command;

use openpapir_core::error::stages;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(args)
        .output()
        .expect("run bootstrap CLI")
}

#[test]
fn capabilities_are_honest_and_machine_readable() {
    let output = run(&["capabilities", "--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    let actual: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        actual,
        serde_json::json!({
            "schema_version": 1, "ok": true, "command": "capabilities",
            "data": {
                "project": "openPapir", "stage": "alpha",
                "operations": [
                    "archive.init", "import",
                    "case.create", "case.list", "case.show", "submission.add",
                    "receipt.add", "receipt.list",
                    "association.create", "association.list",
                    "association.retire", "archive.check", "archive.status",
                    "case.export", "case.import", "archive.repair_permissions",
                    "case.delete", "skill", "case.update",
                    "submission.show", "receipt.show", "association.show",
                    "completions", "manpage",
                    "archive.export", "archive.import", "archive.derive",
                    "search"
                ]
            },
            "verified": false
        })
    );
}

#[test]
fn human_status_names_only_what_is_implemented() {
    let output = run(&["capabilities"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(concat!(
        "archive.init, import, case.create, case.list, case.show, submission.add, ",
        "receipt.add, receipt.list, association.create, association.list, ",
        "association.retire, archive.check, archive.status, case.export, ",
        "case.import, archive.repair_permissions, case.delete, skill, case.update, ",
        "submission.show, receipt.show, association.show, completions, manpage, ",
        "archive.export, archive.import, archive.derive, search"
    )));
    assert!(text.contains("Nothing is verified"));
}

#[test]
fn help_and_version_are_available() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("capabilities")
    );
    let version = run(&["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        concat!("openpapir ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn missing_or_unimplemented_commands_fail() {
    for args in [&[][..], &["create"][..], &["capabilities", "--unknown"][..]] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

/// A machine caller that asked for JSON is answered in JSON, including when
/// the argument parser is what refused the invocation.
#[test]
fn an_argument_parse_failure_is_an_envelope_under_json() {
    for (args, command, argument) in [
        (&["import", "--json"][..], "import", Some("archive")),
        (
            &["archive", "init", "--json"][..],
            "archive.init",
            Some("root"),
        ),
        (
            &["capabilities", "--unknown", "--json"][..],
            "capabilities",
            None,
        ),
        (&["--bogus", "--json"][..], "openpapir", None),
        (&["bogus", "--json"][..], "openpapir", None),
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "usage exits 2 for {command}");
        assert!(
            output.stderr.is_empty(),
            "the JSON form writes no usage text to stderr"
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(text.lines().count(), 1, "exactly one line of JSON");
        let envelope: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(envelope["schema_version"], 1);
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["command"], command);
        assert_eq!(envelope["data"], serde_json::json!({}));
        assert_eq!(envelope["verified"], false);
        assert_eq!(envelope["error"]["code"], "usage.arguments");
        assert_eq!(envelope["error"]["details"]["bucket"], "usage");
        match argument {
            Some(name) => assert_eq!(envelope["error"]["details"]["argument"], name),
            None => assert!(
                envelope["error"]["details"].get("argument").is_none(),
                "a token this build does not define is never echoed"
            ),
        }
    }
}

/// A flag's value is consumed with the flag, and the names a refusal may echo
/// are the recognised command's own, so neither a value that spells a
/// subcommand nor a flag another subcommand defines reaches the envelope.
#[test]
fn no_token_the_recognised_command_does_not_define_reaches_the_envelope() {
    for (args, command, argument) in [
        // The value of `--title` is a value, not the `case list` subcommand.
        (
            &["case", "create", "--title", "list", "--json"][..],
            "case.create",
            Some("archive"),
        ),
        (
            &["case", "create", "--title=show", "--json"][..],
            "case.create",
            Some("archive"),
        ),
        // `--archive` is `import`'s flag, so `capabilities` never echoes it.
        (
            &["capabilities", "--archive", "import", "--json"][..],
            "capabilities",
            None,
        ),
        // `skill` defines neither, and a positional after `--` is a value.
        (&["skill", "--json"][..], "skill", None),
        // A flag another subcommand defines is still not this one's to echo
        // when it was written where a flag belongs and simply is not one.
        (
            &["case", "list", "--title", "x", "--json"][..],
            "case.list",
            None,
        ),
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "usage exits 2 for {command}");
        let text = String::from_utf8(output.stdout).unwrap();
        let envelope: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(envelope["command"], command, "the command the user named");
        assert_eq!(envelope["error"]["code"], "usage.arguments");
        match argument {
            Some(name) => assert_eq!(envelope["error"]["details"]["argument"], name),
            None => assert!(
                envelope["error"]["details"].get("argument").is_none(),
                "a name this command does not define is never echoed"
            ),
        }
    }
}

/// A flag written before the subcommand that takes it is steered rather than
/// only refused, and the value beside it is never echoed.
///
/// `--archive` and `--json` are defined per subcommand rather than globally,
/// so writing either one first is the likeliest flag-order mistake and the
/// parser on its own can say no more than that the token was unexpected.
#[test]
fn a_flag_written_before_its_subcommand_is_told_where_it_belongs() {
    // The value carries a marker no openPapir message uses, so finding it in
    // the output would mean the output quoted the caller.
    const VALUE: &str = "./qzmarker/archive";
    for (args, command, argument) in [
        (
            &["--archive", VALUE, "case", "list", "--json"][..],
            "openpapir",
            "archive",
        ),
        (
            &["--archive=./qzmarker/archive", "case", "list", "--json"][..],
            "case.list",
            "archive",
        ),
        (&["--json", "capabilities"][..], "capabilities", "json"),
        (
            &["case", "--archive", VALUE, "list", "--json"][..],
            "case",
            "archive",
        ),
        // Everything after `--` is a positional, so no subcommand is
        // recognised at all and `--json` is still the flag written too early.
        (
            &["--json", "--", "import", "--archive"][..],
            "openpapir",
            "json",
        ),
    ] {
        let output = run(args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "the steer is still a refusal"
        );
        assert!(output.stderr.is_empty(), "the JSON form writes no stderr");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains("qzmarker"), "the caller's value was echoed");
        let envelope: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(envelope["command"], command);
        assert_eq!(envelope["error"]["code"], "usage.arguments");
        assert_eq!(envelope["error"]["details"]["argument"], argument);
        assert_eq!(
            envelope["error"]["details"]["placement"], "after_subcommand",
            "the refusal says where the flag belongs"
        );
        let message = envelope["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("after the subcommand"),
            "the human line says where the flag belongs: {message}"
        );
    }
}

/// Without `--json` the same steer is one added line after the parser's own
/// usage text, naming the flag and nothing the caller typed.
#[test]
fn the_human_form_adds_the_steer_after_the_parsers_usage_text() {
    let output = run(&["--archive", "./qzmarker/archive", "case", "list"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage text is not a result");
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("Usage:"), "the parser's own text is kept");
    assert!(text.contains("--archive"), "the flag is named");
    assert!(
        text.contains("after the subcommand"),
        "the human line says where the flag belongs: {text}"
    );
    assert!(!text.contains("qzmarker"), "the caller's value was echoed");

    // A flag written where a flag belongs and still refused gains no steer.
    let plain = run(&["capabilities", "--unknown"]);
    let plain = String::from_utf8(plain.stderr).unwrap();
    assert!(!plain.contains("after the subcommand"), "no false steer");
}

/// The same failures without `--json` keep the parser's own usage text, and
/// help and version stay successes rather than becoming refusals.
#[test]
fn the_human_form_keeps_the_usage_text_and_help_still_succeeds() {
    let output = run(&["import"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage text is not a result");
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("Usage:"), "the parser's own text is kept");
    assert!(!text.contains("usage.arguments"), "no envelope on stderr");

    for args in [&["--help"][..], &["--version"][..], &["help"][..]] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(0), "help and version succeed");
        assert!(!output.stdout.is_empty());
    }
}

/// A user-supplied value after `--` is a positional, never the JSON flag, so
/// it cannot turn a human invocation into a machine one.
#[test]
fn a_positional_that_looks_like_the_json_flag_does_not_ask_for_json() {
    let output = run(&["bogus", "--", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "no envelope was asked for");
    assert!(!output.stderr.is_empty());
}

/// The embedded skill is written byte for byte, alone, and always succeeds.
///
/// The document is what an agent installs to learn how to drive this CLI, so
/// the bytes the binary carries and the bytes the repository holds have to be
/// the same bytes, with nothing added around them.
#[test]
fn the_skill_is_written_byte_for_byte_and_nothing_else() {
    let output = run(&["skill"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "the document is the whole output");
    let embedded = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/openpapir/SKILL.md"),
    )
    .expect("read the committed skill document");
    assert_eq!(output.stdout, embedded, "the binary carries these bytes");
    let text = String::from_utf8(output.stdout).expect("the skill is UTF-8");
    assert!(text.starts_with("---\nname: openpapir\n"));
    assert!(text.contains("openpapir skill"));
}

/// `skill` takes no file and no `--json`, and says so through the usual
/// usage refusal rather than by ignoring the token.
#[test]
fn the_skill_command_takes_no_file_and_no_json_flag() {
    let flagged = run(&["skill", "--json"]);
    assert_eq!(flagged.status.code(), Some(2));
    assert!(flagged.stderr.is_empty(), "a JSON caller reads an envelope");
    let envelope: serde_json::Value =
        serde_json::from_str(String::from_utf8(flagged.stdout).unwrap().trim()).unwrap();
    assert_eq!(envelope["command"], "skill");
    assert_eq!(envelope["error"]["code"], "usage.arguments");

    let extra = run(&["skill", "somefile"]);
    assert_eq!(extra.status.code(), Some(2));
    assert!(extra.stdout.is_empty(), "the document is not written");
    assert!(!extra.stderr.is_empty(), "the parser explains itself");
}

/// Split one Markdown table row into its cells.
///
/// A cell whose own text carries a pipe writes it as `\|`, so the split
/// honours that escape. Padding around a cell is presentation, so every cell
/// comes back trimmed and a row reads the same however its columns are
/// aligned.
fn table_cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut cells = vec![String::new()];
    let mut escaped = false;
    for character in inner.chars() {
        if escaped {
            cells
                .last_mut()
                .expect("a cell is always open")
                .push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '|' {
            cells.push(String::new());
        } else {
            cells
                .last_mut()
                .expect("a cell is always open")
                .push(character);
        }
    }
    cells.iter().map(|cell| cell.trim().to_owned()).collect()
}

/// The rows of the table with this header, inside the section with this
/// heading.
///
/// Everything here is fail-closed: a missing section, a header that no longer
/// matches, a header without its delimiter, or a table without rows is a
/// failure rather than an empty answer that would silently agree with
/// anything.
fn table_under(document: &str, heading: &str, header: &[&str]) -> Vec<Vec<String>> {
    let section = document
        .split_once(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("no section titled {heading}"))
        .1;
    let section = section.split("\n## ").next().expect("a section has a body");
    let mut lines = section.lines();
    let found = lines.any(|line| {
        line.starts_with('|')
            && table_cells(line)
                .iter()
                .map(String::as_str)
                .eq(header.iter().copied())
    });
    assert!(found, "no table headed {header:?} under {heading}");
    let delimiter = lines
        .next()
        .expect("a header row is followed by a delimiter");
    assert!(
        delimiter.starts_with('|'),
        "the header row needs a delimiter"
    );
    let rows: Vec<Vec<String>> = lines
        .take_while(|line| line.starts_with('|'))
        .map(table_cells)
        .collect();
    assert!(!rows.is_empty(), "the table under {heading} has no rows");
    rows
}

/// The operation an invocation names, or `None` when it names none.
///
/// The leading words of an invocation are its command path, and the first
/// token that is not a bare lowercase word starts the arguments. `--help` and
/// `--version` therefore name nothing, and a command path becomes an
/// operation name the way the binary spells it: dots between the words and an
/// underscore where the command has a dash.
fn operation_named_by(invocation: &str) -> Option<String> {
    let mut words = invocation.trim_matches('`').split_whitespace();
    assert_eq!(
        words.next(),
        Some("openpapir"),
        "every row invokes openpapir"
    );
    let path: Vec<&str> = words
        .take_while(|word| {
            word.starts_with(|first: char| first.is_ascii_lowercase())
                && word.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        })
        .collect();
    (!path.is_empty()).then(|| path.join(".").replace('-', "_"))
}

/// One invocation cell as the comparison reads it.
///
/// Backticks and the padding a Markdown table uses to align its columns are
/// presentation, and so is a run of spaces inside a cell, so an invocation
/// that reads the same reads equal however either document spaces it. Nothing
/// else is dropped: a flag one document names and the other does not stays a
/// difference.
fn normalised_invocation(cell: &str) -> String {
    cell.trim_matches('`')
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Both documented enumerations of the operations agree with the binary.
///
/// Prose everywhere else defers to the list `capabilities` reports, and the
/// two tables that still enumerate the operations are the table under Current
/// implementation in `docs/architecture.md`, which is the authoritative one,
/// and the Implemented today table in `docs/specification.md`. This test is
/// what keeps both honest, so an operation added later changes those tables
/// and nothing else that enumerates operations. The comparison is
/// order-independent, because each table is in the order its own document
/// reads in, which is not the order `capabilities` reports.
#[test]
fn the_documented_tables_list_exactly_the_reported_operations() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let architecture = std::fs::read_to_string(repository.join("docs/architecture.md"))
        .expect("read the architecture document");
    let mut documented: Vec<String> = table_under(
        &architecture,
        "Current implementation",
        &["Operation", "Invocation"],
    )
    .iter()
    .map(|row| row[0].trim_matches('`').to_owned())
    .collect();

    let specification = std::fs::read_to_string(repository.join("docs/specification.md"))
        .expect("read the specification index");
    let mut specified = Vec::new();
    let mut outside = 0;
    for row in table_under(
        &specification,
        "Implemented today",
        &["Invocation", "Result"],
    ) {
        match operation_named_by(&row[0]) {
            Some(name) if name != "capabilities" => specified.push(name),
            _ => outside += 1,
        }
    }
    assert_eq!(
        outside, 3,
        "only --help, --version and capabilities are not operations"
    );

    let output = run(&["capabilities", "--json"]);
    assert!(output.status.success());
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("capabilities is one JSON object");
    let mut reported: Vec<String> = envelope["data"]["operations"]
        .as_array()
        .expect("operations is an array")
        .iter()
        .map(|name| {
            name.as_str()
                .expect("every operation name is a string")
                .to_owned()
        })
        .collect();

    documented.sort();
    specified.sort();
    reported.sort();
    assert_eq!(
        documented, reported,
        "docs/architecture.md and capabilities disagree about the operations"
    );
    assert_eq!(
        specified, reported,
        "docs/specification.md and capabilities disagree about the operations"
    );
}

/// The two tables spell every operation's invocation the same way.
///
/// The operation names alone are guarded above, which leaves the invocation
/// columns free to drift: a flag added to one table and not the other
/// documents the same command two ways, and a reader who follows the wrong
/// row is told to run something the binary refuses. The rule is equality
/// after whitespace normalisation, not a prefix, because a terse row would
/// let the specification keep an invocation that is merely no longer wrong
/// while the flag it omits stays undocumented there. The Documentation
/// section of `CONTRIBUTING.md` records that rule for contributors.
#[test]
fn the_two_tables_document_the_same_invocation_for_every_operation() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let architecture = std::fs::read_to_string(repository.join("docs/architecture.md"))
        .expect("read the architecture document");
    let authoritative: std::collections::BTreeMap<String, String> = table_under(
        &architecture,
        "Current implementation",
        &["Operation", "Invocation"],
    )
    .iter()
    .map(|row| {
        (
            row[0].trim_matches('`').to_owned(),
            normalised_invocation(&row[1]),
        )
    })
    .collect();

    let specification = std::fs::read_to_string(repository.join("docs/specification.md"))
        .expect("read the specification index");
    let mut compared = 0;
    for row in table_under(
        &specification,
        "Implemented today",
        &["Invocation", "Result"],
    ) {
        let Some(operation) = operation_named_by(&row[0]) else {
            continue;
        };
        let Some(expected) = authoritative.get(&operation) else {
            continue;
        };
        assert_eq!(
            &normalised_invocation(&row[0]),
            expected,
            "docs/specification.md and docs/architecture.md document {operation} differently"
        );
        compared += 1;
    }
    assert_eq!(
        compared,
        authoritative.len(),
        "every operation the architecture table names has a specification row"
    );
}

/// The operations `capabilities` reports, as the binary reports them.
fn reported_operations() -> Vec<String> {
    let output = run(&["capabilities", "--json"]);
    assert!(output.status.success());
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("capabilities is one JSON object");
    envelope["data"]["operations"]
        .as_array()
        .expect("operations is an array")
        .iter()
        .map(|name| {
            name.as_str()
                .expect("every operation name is a string")
                .to_owned()
        })
        .collect()
}

/// Every fenced `json` block of a document, in the order it reads in.
fn fenced_json_blocks(document: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<String> = None;
    for line in document.lines() {
        match open.as_mut() {
            Some(block) if line.trim_end() == "```" => {
                blocks.push(std::mem::take(block));
                open = None;
            }
            Some(block) => {
                block.push_str(line);
                block.push('\n');
            }
            None if line.trim_end() == "```json" => open = Some(String::new()),
            None => {}
        }
    }
    assert!(open.is_none(), "a fenced block was left unclosed");
    blocks
}

/// Every documented `capabilities` sample is valid JSON and reports the
/// operations the binary reports.
///
/// A sample is what a reader copies before they run anything, so one that no
/// parser accepts is worse than no sample: a dropped comma between two
/// operation names turned both samples into text that only looks like JSON,
/// and nothing failed. Parsing each block as JSON is what catches that, and
/// comparing the array it holds is what keeps the samples from drifting the
/// way the tables can. The comparison is order-sensitive, because a sample
/// claims to be the output of one run.
#[test]
fn every_documented_capabilities_sample_is_json_and_reports_what_the_binary_does() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reported = reported_operations();
    let mut checked = 0;
    for name in ["README.md", "docs/architecture.md", "docs/specification.md"] {
        let document = std::fs::read_to_string(repository.join(name))
            .unwrap_or_else(|_| panic!("read {name}"));
        for (index, block) in fenced_json_blocks(&document).iter().enumerate() {
            if !block.contains("\"operations\"") {
                continue;
            }
            let sample: serde_json::Value = serde_json::from_str(block)
                .unwrap_or_else(|error| panic!("block {index} of {name} is not JSON: {error}"));
            let operations: Vec<String> = sample["data"]["operations"]
                .as_array()
                .unwrap_or_else(|| panic!("block {index} of {name} has no operations array"))
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .expect("every operation name is a string")
                        .to_owned()
                })
                .collect();
            assert_eq!(
                operations, reported,
                "the capabilities sample in {name} and the binary disagree"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 2,
        "README.md and docs/architecture.md each carry a capabilities sample"
    );
}

/// The stages the error contract's own table enumerates, in table order.
///
/// The parse is fail-closed the way [`table_under`] is: a document that holds
/// no stage table, or more than one, fails rather than agreeing with
/// anything. `docs/architecture.md` repeats the table and a test in
/// `export.rs` holds the two documents to the same paths per stage, so
/// reading one of them here is enough to pin the set.
fn documented_stages(document: &str) -> Vec<String> {
    let mut tables = 0;
    let mut stages = Vec::new();
    let mut lines = document.lines().peekable();
    while let Some(line) = lines.next() {
        if table_cells(line) != ["Stage", "What it names"] {
            continue;
        }
        tables += 1;
        assert!(
            lines
                .next()
                .is_some_and(|delimiter| delimiter.trim_start().starts_with('|')),
            "a header row is followed by a delimiter"
        );
        while lines
            .peek()
            .is_some_and(|next| next.trim_start().starts_with('|'))
        {
            let row = table_cells(lines.next().expect("the row was peeked"));
            stages.push(row[0].trim_matches('`').to_owned());
        }
    }
    assert_eq!(tables, 1, "a document holds exactly one stage table");
    assert!(!stages.is_empty(), "the stage table has rows");
    stages
}

/// Every value the contract enumerates for `stage`, read from the paragraph
/// that enumerates them.
///
/// The write stages have a table of their own, which [`documented_stages`]
/// reads; the deletion phases are named in prose beside it, so the whole set
/// is pinned rather than the tabled part of it alone. The count word is read
/// too, so a value added to one and not the other is a failure rather than a
/// sentence that quietly stops matching what it counts.
fn enumerated_stages(document: &str) -> Vec<String> {
    let counts = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight",
    ];
    let opening = format!("`stage` has exactly {} values.", counts[stages::ALL.len()]);
    let paragraph = document
        .split_once(&opening)
        .unwrap_or_else(|| panic!("the contract enumerates stage as: {opening}"))
        .1
        .split_once("\n\n")
        .expect("the enumeration is a paragraph")
        .0;
    let mut values = Vec::new();
    let mut rest = paragraph;
    while let Some((_, after)) = rest.split_once('`') {
        let (value, tail) = after.split_once('`').expect("a backtick is closed");
        values.push(value.to_owned());
        rest = tail;
    }
    assert!(!values.is_empty(), "the enumeration names its values");
    values
}

/// Every `.rs` file under `crates/openpapir-core/src`, in no fixed order.
fn core_sources() -> Vec<std::path::PathBuf> {
    let mut pending =
        vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../openpapir-core/src")];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read a source directory") {
            let path = entry.expect("read a source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|suffix| suffix == "rs") {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 20, "the core sources were found");
    files
}

/// The arguments of the call whose opening parenthesis is at `open`, split at
/// the commas of that call alone.
///
/// Nesting and string literals are tracked, so an argument that is itself a
/// call, a macro, or a text holding a comma or a parenthesis stays one
/// argument. `None` means the call was left unclosed, which no compiling
/// source does.
fn call_arguments(source: &str, open: usize) -> Option<Vec<String>> {
    let mut arguments = vec![String::new()];
    let mut depth = 0_usize;
    let mut in_text = false;
    let mut escaped = false;
    for character in source[open..].chars() {
        let current = arguments.last_mut().expect("an argument is always open");
        if in_text {
            current.push(character);
            match character {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_text = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => {
                in_text = true;
                current.push(character);
            }
            '(' | '[' | '{' => {
                depth += 1;
                if depth > 1 {
                    current.push(character);
                }
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    let mut arguments: Vec<String> = arguments
                        .iter()
                        .map(|argument| argument.trim().to_owned())
                        .collect();
                    // A trailing comma leaves an empty argument behind, and
                    // the stage is then the one before it.
                    if arguments.last().is_some_and(String::is_empty) && arguments.len() > 1 {
                        arguments.pop();
                    }
                    return Some(arguments);
                }
                current.push(character);
            }
            ',' if depth == 1 => arguments.push(String::new()),
            _ => current.push(character),
        }
    }
    None
}

/// Every emitter of a stage in the core names a documented stage.
///
/// The `stage` a refusal or a warning carries is a closed set the contract
/// enumerates, and a value outside it is a word no caller can branch on. The
/// evidence is read from the sources rather than from a list an emitter added
/// later would not join: every call that takes a stage as its last argument
/// is found, and a stage spelled as a literal there is held to the
/// contract's enumeration. A stage passed as a constant is checked where the
/// constant is defined, by the same rule.
#[test]
fn every_stage_emitter_in_the_core_names_a_documented_stage() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let contract = std::fs::read_to_string(repository.join("docs/error-contract.md"))
        .expect("read the error contract");
    let mut writes = documented_stages(&contract);
    writes.sort();
    let mut implemented: Vec<String> = stages::WRITES
        .iter()
        .map(|stage| (*stage).to_owned())
        .collect();
    implemented.sort();
    assert_eq!(
        writes, implemented,
        "the contract's stage table and the write stages disagree"
    );
    let mut documented = enumerated_stages(&contract);
    documented.sort();
    let mut every: Vec<String> = stages::ALL
        .iter()
        .map(|stage| (*stage).to_owned())
        .collect();
    every.sort();
    assert_eq!(
        documented, every,
        "the contract's enumeration of stage and the implementation disagree"
    );

    // The calls that take the stage as their last argument, and `.text`,
    // which names it as the key of the detail it writes.
    let calls = [
        "publish_refusal(",
        "link_refusal(",
        "sync_directory(",
        "no_directory_fsync_warning(",
        "Staging::create(",
        ".finish(",
        ".publish(",
        "replace_document(",
        "write_document(",
        ".text(",
    ];
    let mut sites = 0;
    let mut literals = 0;
    let mut constants = 0;
    for path in core_sources() {
        let source = std::fs::read_to_string(&path).expect("read a core source");
        // A stage a call passes as a constant is spelled once, where the
        // constant is defined, and that definition is held to the same rule.
        for line in source.lines() {
            let Some((declaration, value)) = line.split_once(": &str = \"") else {
                continue;
            };
            let name = declaration.trim().rsplit(' ').next().unwrap_or_default();
            if !(name == "STAGE" || name.ends_with("_WRITE")) {
                continue;
            }
            constants += 1;
            let value = value.split('"').next().expect("the literal is closed");
            assert!(
                documented.contains(&value.to_owned()),
                "{} defines the stage {value:?}, which the contract does not enumerate",
                path.display()
            );
        }
        for call in calls {
            for (index, _) in source.match_indices(call) {
                // A name that merely ends with one of these, such as
                // `symlink_refusal`, is a different call.
                if !call.starts_with('.')
                    && source[..index]
                        .chars()
                        .next_back()
                        .is_some_and(|before| before.is_alphanumeric() || before == '_')
                {
                    continue;
                }
                let open = index + call.len() - 1;
                let arguments = call_arguments(&source, open).expect("the call is closed");
                if call == ".text(" && arguments.first().map(String::as_str) != Some("\"stage\"") {
                    continue;
                }
                sites += 1;
                let last = arguments.last().expect("a call has an argument").clone();
                if let Some(stage) = last
                    .strip_prefix('"')
                    .and_then(|rest| rest.strip_suffix('"'))
                {
                    literals += 1;
                    assert!(
                        documented.contains(&stage.to_owned()),
                        "{} passes the stage {stage:?}, which the contract does not enumerate",
                        path.display()
                    );
                }
            }
        }
    }
    assert!(sites >= 25, "the stage-taking calls were found: {sites}");
    assert!(
        literals >= 10,
        "stages spelled in place were found: {literals}"
    );
    assert!(constants >= 4, "stage constants were found: {constants}");
}
