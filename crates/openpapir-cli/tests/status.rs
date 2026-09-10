//! Contract tests for the read-only archive summary.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence. The assertions
//! are about the observable contract in `docs/architecture.md`: the counts,
//! the reminder list and its order, the as-of flag, the privacy rule, and the
//! promise that the summary reads the archive and changes nothing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const PAYLOAD: &[u8] = b"synthetic submission alpha\n";
const OTHER_PAYLOAD: &[u8] = b"synthetic receipt beta\n";
const AS_OF: &str = "2026-02-10";
/// A date whose window is still open on [`AS_OF`], closing 2026-02-19.
const OPEN_DATE: &str = "2026-01-20";
/// A date whose window closed before [`AS_OF`].
const ELAPSED_DATE: &str = "2025-12-01";
const TITLE: &str = "Tax matter";
const NOTES: &str = "First contact with the office.";
const DESCRIPTION: &str = "Posted the completed form.";

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

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// An archive holding two imported artefacts and nothing else, with the
/// digests of the two stored objects in import order.
fn archive() -> (TempDir, PathBuf, Vec<String>) {
    let home = tempfile::tempdir().expect("a temporary directory");
    let root = home.path().join("archive");
    fs::create_dir(&root).expect("create the archive root");
    let first = home.path().join("alpha.bin");
    let second = home.path().join("beta.bin");
    fs::write(&first, PAYLOAD).expect("write a synthetic payload");
    fs::write(&second, OTHER_PAYLOAD).expect("write a synthetic payload");
    assert!(
        run(&["archive", "init", &path(&root), "--json"])
            .status
            .success()
    );
    let imported = stdout_json(&run(&[
        "import",
        "--archive",
        &path(&root),
        &path(&first),
        &path(&second),
        "--json",
    ]));
    let digests = imported["data"]["artefacts"]
        .as_array()
        .expect("the import reports its artefacts")
        .iter()
        .map(|artefact| artefact["digest"].as_str().expect("a digest").to_owned())
        .collect();
    (home, root, digests)
}

/// Record one case and return its identifier.
fn case(root: &Path) -> String {
    let created = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        &path(root),
        "--title",
        TITLE,
        "--notes",
        NOTES,
        "--json",
    ]));
    created["data"]["case"]["id"]
        .as_str()
        .expect("a created case has an identifier")
        .to_owned()
}

/// Record one submission and return its identifier.
fn submission(root: &Path, case_id: &str, date: Option<&str>) -> String {
    let mut arguments = vec![
        "submission".to_owned(),
        "add".to_owned(),
        "--archive".to_owned(),
        path(root),
        "--case".to_owned(),
        case_id.to_owned(),
        "--description".to_owned(),
        DESCRIPTION.to_owned(),
    ];
    if let Some(date) = date {
        arguments.extend(["--date".to_owned(), date.to_owned()]);
    }
    arguments.push("--json".to_owned());
    let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let added = stdout_json(&run(&borrowed));
    added["data"]["submission"]["id"]
        .as_str()
        .expect("a recorded submission has an identifier")
        .to_owned()
}

fn status_json(root: &Path) -> Value {
    stdout_json(&run(&[
        "archive",
        "status",
        "--archive",
        &path(root),
        "--as-of",
        AS_OF,
        "--json",
    ]))
}

#[test]
fn an_empty_archive_reports_zero_of_everything_and_exits_zero() {
    let (_home, root, _digests) = archive();
    let output = run(&["archive", "status", "--archive", &path(&root), "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "the JSON form writes no stderr");
    let envelope = stdout_json(&output);
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "archive.status");
    assert_eq!(envelope["verified"], false);
    assert!(envelope.get("error").is_none());
    let data = &envelope["data"];
    assert_eq!(data["cases"], 0);
    assert_eq!(data["submissions"], 0);
    assert_eq!(data["receipts"], 0);
    assert_eq!(data["associations"], 0);
    assert_eq!(data["stored_objects"], 2);
    assert_eq!(data["undated_submissions"], 0);
    assert_eq!(data["retention_window_days"], 30);
    assert_eq!(data["receipts_to_retrieve"].as_array().unwrap().len(), 0);
}

#[test]
fn an_absent_as_of_is_the_current_date() {
    let (_home, root, _digests) = archive();
    let envelope = stdout_json(&run(&[
        "archive",
        "status",
        "--archive",
        &path(&root),
        "--json",
    ]));
    let as_of = envelope["data"]["as_of"].as_str().expect("an as-of date");
    assert_eq!(as_of.len(), 10);
    assert!(as_of > "2020-01-01", "the clock is plausible");
}

#[test]
fn a_dated_submission_inside_the_window_is_listed_with_its_computed_dates() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    let submission_id = submission(&root, &case_id, Some(OPEN_DATE));
    let data = status_json(&root)["data"].clone();
    assert_eq!(data["as_of"], AS_OF);
    assert_eq!(data["cases"], 1);
    assert_eq!(data["submissions"], 1);
    assert_eq!(data["undated_submissions"], 0);
    let listed = data["receipts_to_retrieve"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["case_id"], case_id.as_str());
    assert_eq!(listed[0]["submission_id"], submission_id.as_str());
    assert_eq!(listed[0]["submission_date"], OPEN_DATE);
    assert_eq!(listed[0]["retrieve_by"], "2026-02-19");
    assert_eq!(listed[0]["days_left"], 9);
}

#[test]
fn a_submission_whose_window_has_elapsed_is_not_listed() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, Some(ELAPSED_DATE));
    let data = status_json(&root)["data"].clone();
    assert_eq!(data["submissions"], 1);
    assert_eq!(data["undated_submissions"], 0);
    assert_eq!(data["receipts_to_retrieve"].as_array().unwrap().len(), 0);
}

#[test]
fn an_undated_submission_is_counted_and_never_listed() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, None);
    let data = status_json(&root)["data"].clone();
    assert_eq!(data["submissions"], 1);
    assert_eq!(data["undated_submissions"], 1);
    assert_eq!(data["receipts_to_retrieve"].as_array().unwrap().len(), 0);
}

#[test]
fn an_association_naming_a_submission_takes_it_off_the_list() {
    let (_home, root, digests) = archive();
    let case_id = case(&root);
    let submission_id = submission(&root, &case_id, Some(OPEN_DATE));
    let added = stdout_json(&run(&[
        "receipt",
        "add",
        "--archive",
        &path(&root),
        "--artefact",
        &digests[1],
        "--json",
    ]));
    let receipt_id = added["data"]["receipt"]["id"]
        .as_str()
        .expect("a recorded receipt has an identifier")
        .to_owned();
    assert!(
        run(&[
            "association",
            "create",
            "--archive",
            &path(&root),
            "--receipt",
            &receipt_id,
            "--outcome",
            "candidate",
            "--candidate",
            &format!("{submission_id}:moderate:The reference matches."),
            "--json",
        ])
        .status
        .success()
    );

    let data = status_json(&root)["data"].clone();
    assert_eq!(data["receipts"], 1);
    assert_eq!(data["associations"], 1);
    assert_eq!(
        data["receipts_to_retrieve"].as_array().unwrap().len(),
        0,
        "a candidate association ends the reminder"
    );
}

#[test]
fn the_list_is_ordered_by_the_date_the_window_closes() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, Some("2026-02-01"));
    submission(&root, &case_id, Some(OPEN_DATE));
    let listed = status_json(&root)["data"]["receipts_to_retrieve"]
        .as_array()
        .unwrap()
        .clone();
    let order: Vec<&str> = listed
        .iter()
        .map(|entry| entry["retrieve_by"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["2026-02-19", "2026-03-03"]);
}

#[test]
fn an_as_of_that_is_not_a_calendar_date_is_refused_with_usage_arguments() {
    let (_home, root, _digests) = archive();
    let output = run(&[
        "archive",
        "status",
        "--archive",
        &path(&root),
        "--as-of",
        "2026-02-30",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "usage.arguments");
    assert_eq!(envelope["error"]["details"]["argument"], "as_of");
    assert_eq!(envelope["error"]["details"]["bucket"], "usage");
    assert_eq!(envelope["data"], serde_json::json!({}));
}

#[test]
fn a_past_or_future_as_of_is_a_what_if_rather_than_a_refusal() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, Some(OPEN_DATE));
    for (as_of, listed) in [("2020-01-01", 1), ("2099-01-01", 0)] {
        let envelope = stdout_json(&run(&[
            "archive",
            "status",
            "--archive",
            &path(&root),
            "--as-of",
            as_of,
            "--json",
        ]));
        assert_eq!(envelope["ok"], true);
        assert_eq!(
            envelope["data"]["receipts_to_retrieve"]
                .as_array()
                .unwrap()
                .len(),
            listed,
            "as of {as_of}"
        );
    }
}

#[test]
fn the_output_carries_no_user_text_and_no_path() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, Some(OPEN_DATE));
    let json = stdout_text(&run(&[
        "archive",
        "status",
        "--archive",
        &path(&root),
        "--as-of",
        AS_OF,
        "--json",
    ]));
    let human = stdout_text(&run(&[
        "archive",
        "status",
        "--archive",
        &path(&root),
        "--as-of",
        AS_OF,
    ]));
    for text in [json, human] {
        for forbidden in [TITLE, NOTES, DESCRIPTION, "alpha.bin", &path(&root)] {
            assert!(
                !text.contains(forbidden),
                "the summary carries no user text and no path"
            );
        }
    }
}

/// The human form never words a reminder as delivery, receipt by an
/// authority, or legal effect.
#[test]
fn the_human_form_words_a_reminder_as_a_reminder_to_fetch() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, Some(OPEN_DATE));
    let output = run(&[
        "archive",
        "status",
        "--archive",
        &path(&root),
        "--as-of",
        AS_OF,
    ]);
    assert_eq!(output.status.code(), Some(0));
    let reported = String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8");
    assert!(
        reported
            .lines()
            .all(|line| line.starts_with("warning platform.")),
        "the human form reports no error; Windows adds platform warnings here"
    );
    let text = stdout_text(&output);
    assert!(text.contains("fetch a submission receipt from the delivery storage"));
    assert!(text.contains("30-day window the operator describes"));
    assert!(text.contains("fetch by 2026-02-19, 9 day(s) left."));
    assert!(text.contains("changed nothing"));
    for forbidden in ["was delivered to", "legally", "verified", "authentic"] {
        assert!(!text.contains(forbidden), "no claim of {forbidden}");
    }
}

#[test]
fn the_summary_takes_no_lock_and_writes_nothing() {
    let (_home, root, _digests) = archive();
    let case_id = case(&root);
    submission(&root, &case_id, Some(OPEN_DATE));
    let before = fingerprint(&root);
    assert!(
        run(&[
            "archive",
            "status",
            "--archive",
            &path(&root),
            "--as-of",
            AS_OF,
            "--json",
        ])
        .status
        .success()
    );
    assert_eq!(before, fingerprint(&root), "the archive is unchanged");
    assert!(!root.join("lock").exists(), "no lock file is left behind");
}

#[test]
fn a_root_that_is_not_an_archive_is_a_refusal_without_a_path() {
    let home = tempfile::tempdir().unwrap();
    let output = run(&[
        "archive",
        "status",
        "--archive",
        &path(home.path()),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "archive.marker_missing");
    assert!(!stdout_text(&output).contains(&path(home.path())));
}

/// Every path under the archive with its length, so a write is visible.
fn fingerprint(root: &Path) -> Vec<(String, u64)> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(path);
            } else {
                found.push((
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    metadata.len(),
                ));
            }
        }
    }
    found.sort();
    found
}
