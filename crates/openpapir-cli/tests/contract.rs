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
                    "case.export", "archive.repair_permissions", "case.delete",
                    "skill", "case.update"
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
        "association.retire, archive.check, archive.status, ",
        "case.export, archive.repair_permissions, case.delete, skill, case.update"
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
