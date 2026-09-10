//! Executable contract tests use synthetic arguments only.
use std::process::Command;

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
                    "archive.export", "archive.import"
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
        "submission.show, receipt.show, association.show, completions, manpage"
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
        (
            &["--json", "--", "import", "--archive"][..],
            "openpapir",
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
