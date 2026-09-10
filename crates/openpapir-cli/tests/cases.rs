//! Contract tests for the case lifecycle: status, tags, `case update`, and
//! the filters `case list` accepts.
//!
//! Every fixture here is synthetic and built at test time from constants in
//! this file. The assertions are about the observable contract in
//! `docs/error-contract.md` and `docs/architecture.md`: the envelope, the
//! codes, the exit codes, the stored documents, and the privacy rule, rather
//! than about the implementation. In particular, a `--query` is the user's
//! own text and may never appear in any output.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

/// A well-formed identifier that names no record in a fresh archive.
const ABSENT_ID: &str = "0123456789abcdef0123456789abcdef";

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(args)
        .output()
        .expect("run the openpapir binary")
}

fn stdout_json(output: &Output) -> Value {
    let text = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
    assert_eq!(text.lines().count(), 1, "exactly one line of JSON");
    serde_json::from_str(text.trim()).expect("stdout is one JSON object")
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary paths are UTF-8")
}

fn archive() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("create a temporary directory");
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    assert!(output.status.success(), "the archive is created");
    root
}

/// Create a case and return its identifier.
fn create(root: &Path, extra: &[&str]) -> String {
    let mut arguments = vec!["case", "create", "--archive", path(root), "--json"];
    arguments.extend_from_slice(extra);
    let output = run(&arguments);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true, "the case is created");
    envelope["data"]["case"]["id"]
        .as_str()
        .expect("a minted identifier")
        .to_owned()
}

fn titles(envelope: &Value) -> Vec<String> {
    envelope["data"]["cases"]
        .as_array()
        .expect("cases is an array")
        .iter()
        .map(|case| case["title"].as_str().expect("a title").to_owned())
        .collect()
}

#[test]
fn a_created_case_carries_its_status_and_its_sorted_tags() {
    let root = archive();
    let id = create(
        root.path(),
        &[
            "--title",
            "Tax matter",
            "--status",
            "closed",
            "--tag",
            "tax",
            "--tag",
            "appeal",
            "--tag",
            "tax",
        ],
    );
    let envelope = stdout_json(&run(&[
        "case",
        "show",
        "--archive",
        path(root.path()),
        &id,
        "--json",
    ]));
    let case = &envelope["data"]["case"];
    assert_eq!(case["status"], "closed");
    assert_eq!(case["tags"], serde_json::json!(["appeal", "tax"]));
    assert!(
        case.get("updated_at").is_none(),
        "a case nobody changed has no update time"
    );

    let human = run(&["case", "show", "--archive", path(root.path()), &id]);
    let text = String::from_utf8(human.stdout).expect("stdout is UTF-8");
    assert!(text.contains("Status: closed"));
    assert!(text.contains("Tags: appeal, tax"));
}

#[test]
fn a_status_that_is_not_one_of_the_two_names_is_a_usage_refusal() {
    let root = archive();
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "T",
        "--status",
        "archived",
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "usage.arguments");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains("archived"),
        "a refusal never echoes the value the user typed"
    );
}

#[test]
fn a_tag_over_its_cap_or_past_the_count_is_refused_in_the_input_bucket() {
    let root = archive();
    let long = "t".repeat(65);
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "T",
        "--tag",
        &long,
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "input.cap.tag_length");
    assert_eq!(envelope["error"]["details"]["bucket"], "input");
    assert_eq!(output.status.code(), Some(3));
    assert!(!String::from_utf8(output.stdout).unwrap().contains(&long));

    let many: Vec<String> = (0..33).map(|index| format!("tag{index}")).collect();
    let mut arguments = vec![
        "case".to_owned(),
        "create".to_owned(),
        "--archive".to_owned(),
        path(root.path()).to_owned(),
        "--title".to_owned(),
        "T".to_owned(),
        "--json".to_owned(),
    ];
    for tag in &many {
        arguments.push("--tag".to_owned());
        arguments.push(tag.clone());
    }
    let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let output = run(&borrowed);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "input.cap.tag_count");
    assert_eq!(envelope["error"]["details"]["cap_count"], 32);
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn an_update_rewrites_the_case_and_names_the_fields_it_changed() {
    let root = archive();
    let id = create(
        root.path(),
        &[
            "--title",
            "Tax matter",
            "--notes",
            "First contact.",
            "--tag",
            "tax",
        ],
    );
    let output = run(&[
        "case",
        "update",
        "--archive",
        path(root.path()),
        &id,
        "--title",
        "Tax appeal",
        "--clear-notes",
        "--status",
        "closed",
        "--tag",
        "appeal",
        "--untag",
        "tax",
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "case.update");
    assert_eq!(output.status.code(), Some(0));
    let case = &envelope["data"]["case"];
    assert_eq!(case["id"], id.as_str(), "the identifier never moves");
    assert_eq!(case["title"], "Tax appeal");
    assert!(case.get("notes").is_none(), "the notes were cleared");
    assert_eq!(case["status"], "closed");
    assert_eq!(case["tags"], serde_json::json!(["appeal"]));
    assert!(case["updated_at"].is_string());
    assert_eq!(
        envelope["data"]["changed"],
        serde_json::json!(["notes", "status", "tags", "title"]),
        "field names only, sorted"
    );

    let listed = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--json",
    ]));
    assert_eq!(listed["data"]["count"], 1, "the rewrite left one record");
    assert_eq!(listed["data"]["cases"][0]["title"], "Tax appeal");
}

#[test]
fn an_update_that_would_change_nothing_is_refused_as_usage() {
    let root = archive();
    let id = create(root.path(), &["--title", "Same", "--status", "open"]);
    for arguments in [
        vec![
            "case",
            "update",
            "--archive",
            path(root.path()),
            &id,
            "--json",
        ],
        vec![
            "case",
            "update",
            "--archive",
            path(root.path()),
            &id,
            "--title",
            "Same",
            "--status",
            "open",
            "--json",
        ],
    ] {
        let output = run(&arguments);
        let envelope = stdout_json(&output);
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["error"]["code"], "usage.arguments");
        assert_eq!(envelope["error"]["details"]["argument"], "update");
        assert_eq!(output.status.code(), Some(2));
        assert_ne!(output.status.code(), Some(1));
    }
}

#[test]
fn an_update_of_an_unknown_case_is_refused_without_echoing_the_reference() {
    let root = archive();
    let output = run(&[
        "case",
        "update",
        "--archive",
        path(root.path()),
        ABSENT_ID,
        "--status",
        "closed",
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "record.not_found");
    assert_eq!(output.status.code(), Some(4));
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains(ABSENT_ID)
    );
}

#[test]
fn notes_and_clear_notes_may_not_be_given_together() {
    let root = archive();
    let id = create(root.path(), &["--title", "T"]);
    let output = run(&[
        "case",
        "update",
        "--archive",
        path(root.path()),
        &id,
        "--notes",
        "New",
        "--clear-notes",
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "usage.arguments");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_listing_filters_by_status_by_every_tag_and_by_a_query() {
    let root = archive();
    create(
        root.path(),
        &[
            "--title",
            "Tax matter",
            "--notes",
            "The office wrote back.",
            "--tag",
            "tax",
            "--tag",
            "urgent",
        ],
    );
    create(
        root.path(),
        &[
            "--title",
            "Parking appeal",
            "--status",
            "closed",
            "--tag",
            "tax",
        ],
    );

    let all = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--json",
    ]));
    assert_eq!(all["data"]["count"], 2);

    let open = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--status",
        "open",
        "--json",
    ]));
    assert_eq!(titles(&open), vec!["Tax matter".to_owned()]);

    let tagged = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--tag",
        "tax",
        "--tag",
        "urgent",
        "--json",
    ]));
    assert_eq!(
        titles(&tagged),
        vec!["Tax matter".to_owned()],
        "every supplied tag must match"
    );

    let one_tag = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--tag",
        "tax",
        "--json",
    ]));
    assert_eq!(one_tag["data"]["count"], 2);

    let together = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--status",
        "closed",
        "--tag",
        "tax",
        "--query",
        "APPEAL",
        "--json",
    ]));
    assert_eq!(titles(&together), vec!["Parking appeal".to_owned()]);

    let none = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--status",
        "closed",
        "--query",
        "tax matter",
        "--json",
    ]));
    assert_eq!(none["data"]["count"], 0);
    assert!(none["data"]["cases"].as_array().unwrap().is_empty());
}

#[test]
fn a_query_matches_the_notes_and_is_never_echoed_back() {
    let root = archive();
    create(
        root.path(),
        &[
            "--title",
            "A matter",
            "--notes",
            "A private detail the user typed.",
        ],
    );
    let query = "PRIVATE DETAIL";
    let output = run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--query",
        query,
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["data"]["count"], 1, "the query reads the notes");
    let rendered = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        !rendered.contains(query),
        "the query the user typed is never echoed back"
    );

    let miss = run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--query",
        query,
        "--status",
        "closed",
        "--json",
    ]);
    let rendered = String::from_utf8(miss.stdout).expect("stdout is UTF-8");
    assert!(
        !rendered.contains(query),
        "a listing that matched nothing echoes it no more than one that did"
    );

    let human = run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--query",
        query,
    ]);
    let rendered = String::from_utf8(human.stdout).expect("stdout is UTF-8");
    assert!(!rendered.contains(query), "nor does the human form");
    // Stderr carries only the platform warnings, which differ by platform, so
    // it is checked for the query rather than for being empty.
    let diagnostics = String::from_utf8(human.stderr).expect("stderr is UTF-8");
    assert!(!diagnostics.contains(query), "nor any diagnostic line");
}

#[test]
fn a_case_written_before_the_new_fields_still_lists_and_shows() {
    let root = archive();
    let id = create(root.path(), &["--title", "Older"]);
    let document = root.path().join("records/cases").join(format!("{id}.json"));
    let earlier = serde_json::json!({
        "archive_schema_version": 1,
        "created_at": "2026-01-14T09:12:33Z",
        "id": id,
        "record_kind": "case",
        "title": "Older",
    });
    std::fs::write(&document, format!("{earlier}\n")).expect("write the earlier document");

    let envelope = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--status",
        "open",
        "--json",
    ]));
    assert_eq!(
        envelope["data"]["count"], 1,
        "a record without the field reads as open"
    );
    assert_eq!(envelope["data"]["cases"][0]["status"], "open");
    assert_eq!(envelope["data"]["cases"][0]["tags"], serde_json::json!([]));
}
