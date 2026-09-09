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
                "project": "openPapir", "stage": "scaffold",
                "operations": [
                    "archive.init", "import",
                    "case.create", "case.list", "case.show", "submission.add",
                    "receipt.add", "receipt.list",
                    "association.create", "association.list", "archive.check"
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
        "receipt.add, receipt.list, association.create, association.list, archive.check"
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
