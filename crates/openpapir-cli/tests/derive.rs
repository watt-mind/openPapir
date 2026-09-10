//! Contract tests for `archive derive` and the additive derived fields.
//!
//! Every fixture here is synthetic and generated at test time from the
//! constants in this file. Nothing is derived from real correspondence. The
//! assertions are about the observable contract in `docs/architecture.md`:
//! the report's counts, the closed media-type table, the additive fields
//! `case show` and `receipt list` gained, the privacy rule, and the promise
//! that a derived record is disposable and asserts nothing about any file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// One payload of each shape the pinned cases need.
const TEXT: &[u8] = b"synthetic submission alpha\n";
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\ntrailer\n";
const BINARY: &[u8] = b"\x00\x01\x02\x03\x04\x05\x06\x07";

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

/// An archive holding the three payloads, and the paths to reach it.
fn archive() -> (TempDir, String) {
    let home = tempfile::tempdir().expect("a temporary directory");
    let root = home.path().join("archive");
    fs::create_dir(&root).expect("create the archive root");
    let root_text = root.to_string_lossy().into_owned();
    assert!(
        run(&["archive", "init", &root_text, "--json"])
            .status
            .success()
    );
    let inputs: Vec<PathBuf> = [
        ("alpha.txt", TEXT),
        ("beta.pdf", PDF),
        ("gamma.bin", BINARY),
    ]
    .into_iter()
    .map(|(name, payload)| {
        let path = home.path().join(name);
        fs::write(&path, payload).expect("write a synthetic input");
        path
    })
    .collect();
    let mut arguments = vec![
        "import".to_owned(),
        "--archive".to_owned(),
        root_text.clone(),
    ];
    arguments.extend(
        inputs
            .iter()
            .map(|path| path.to_string_lossy().into_owned()),
    );
    arguments.push("--json".to_owned());
    let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
    assert!(run(&borrowed).status.success(), "the import succeeds");
    (home, root_text)
}

/// The digest the import gave one payload, as the records spell it.
fn digest_of(root: &str, payload: &[u8]) -> String {
    // Re-importing the same bytes reports the digest again and creates no
    // second object: the store is content addressed, so the second import is
    // an event against the object that is already there.
    let home = tempfile::tempdir().expect("a temporary directory");
    let path = home.path().join("again.bin");
    fs::write(&path, payload).expect("write the same bytes again");
    let imported = stdout_json(&run(&[
        "import",
        "--archive",
        root,
        &path.to_string_lossy(),
        "--json",
    ]));
    imported["data"]["artefacts"][0]["digest"]
        .as_str()
        .expect("the import reports a digest")
        .to_owned()
}

/// The media type the report counts one of, or nothing.
fn count_of(report: &Value, media_type: &str) -> u64 {
    report["data"]["media_types"]
        .as_array()
        .expect("the report lists every media type")
        .iter()
        .find(|entry| entry["media_type"] == media_type)
        .and_then(|entry| entry["count"].as_u64())
        .expect("every value of the closed table is reported")
}

#[test]
fn deriving_names_the_media_type_of_every_stored_object() {
    let (_home, root) = archive();
    let report = stdout_json(&run(&["archive", "derive", "--archive", &root, "--json"]));
    assert_eq!(report["ok"], true);
    assert_eq!(report["command"], "archive.derive");
    assert_eq!(report["verified"], false, "nothing here is verified");
    assert_eq!(report["data"]["objects_checked"], 3);
    assert_eq!(report["data"]["objects_unchecked"], 0);
    assert_eq!(report["data"]["records_written"], 3);
    assert_eq!(report["data"]["records_removed"], 0);
    assert_eq!(count_of(&report, "text"), 1);
    assert_eq!(count_of(&report, "pdf"), 1);
    assert_eq!(count_of(&report, "unknown"), 1);
    assert_eq!(count_of(&report, "jpeg"), 0);
    assert_eq!(
        report["data"]["media_types"].as_array().unwrap().len(),
        7,
        "the table is closed and every value is reported"
    );
}

#[test]
fn the_report_carries_counts_only_and_no_path_or_digest() {
    let (_home, root) = archive();
    let output = run(&["archive", "derive", "--archive", &root, "--json"]);
    let rendered = stdout_text(&output);
    assert!(!rendered.contains("sha256"), "no digest reaches the report");
    assert!(!rendered.contains(&root), "no path reaches the report");
    assert!(!rendered.contains("alpha.txt"), "no filename either");
    assert!(output.stderr.is_empty(), "the JSON form writes no stderr");

    let human = stdout_text(&run(&["archive", "derive", "--archive", &root]));
    assert!(human.contains("By media type: jpeg 0, pdf 1"));
    assert!(!human.contains('/'), "no path reaches the human form");
    for forbidden in ["authentic", "delivered", "legally"] {
        assert!(
            !human.to_lowercase().contains(forbidden) || human.contains("reports no authenticity"),
            "no line claims what a media type cannot say"
        );
    }
}

#[test]
fn deriving_twice_recomputes_and_leaves_one_record_per_object() {
    let (_home, root) = archive();
    run(&["archive", "derive", "--archive", &root, "--json"]);
    let again = stdout_json(&run(&["archive", "derive", "--archive", &root, "--json"]));
    assert_eq!(again["data"]["records_written"], 3);
    assert_eq!(again["data"]["records_removed"], 0);
    let checked = stdout_json(&run(&["archive", "check", "--archive", &root, "--json"]));
    assert_eq!(checked["data"]["derived_records"], 3);
    assert_eq!(checked["ok"], true, "derived records are not a problem");
}

#[test]
fn an_archive_that_has_never_been_derived_holds_no_derived_record() {
    let (_home, root) = archive();
    let checked = stdout_json(&run(&["archive", "check", "--archive", &root, "--json"]));
    assert_eq!(checked["data"]["derived_records"], 0);
    assert_eq!(checked["ok"], true, "a missing record is nothing at all");
    let human = stdout_text(&run(&["archive", "check", "--archive", &root]));
    assert!(human.contains("Derived-metadata record(s): 0. A missing one is not a problem."));
}

#[test]
fn a_receipt_listing_shows_the_media_type_and_the_length_when_one_is_derived() {
    let (_home, root) = archive();
    let digest = digest_of(&root, PDF);
    assert!(
        run(&[
            "receipt",
            "add",
            "--archive",
            &root,
            "--artefact",
            &digest,
            "--json",
        ])
        .status
        .success()
    );

    let before = stdout_json(&run(&["receipt", "list", "--archive", &root, "--json"]));
    assert_eq!(
        before["data"]["derived"].as_array().unwrap().len(),
        0,
        "nothing has been derived yet"
    );

    run(&["archive", "derive", "--archive", &root, "--json"]);
    let after = stdout_json(&run(&["receipt", "list", "--archive", &root, "--json"]));
    let facts = after["data"]["derived"].as_array().unwrap();
    assert_eq!(facts.len(), 1, "one entry for the receipt's own artefact");
    assert_eq!(facts[0]["artefact_digest"], digest);
    assert_eq!(facts[0]["media_type"], "pdf");
    assert_eq!(facts[0]["byte_length"], PDF.len() as u64);
    assert_eq!(after["data"]["count"], 1, "the listing is unchanged");

    let human = stdout_text(&run(&["receipt", "list", "--archive", &root]));
    assert!(human.contains(&format!("{digest} pdf {} byte(s)", PDF.len())));
    assert!(human.contains("asserting nothing about any file"));
    assert!(
        human.contains("openPapir checked nothing about the file"),
        "the assertion disclaimer still closes the listing"
    );
}

#[test]
fn a_shown_case_shows_the_facts_of_the_artefacts_its_submissions_name() {
    let (_home, root) = archive();
    let digest = digest_of(&root, TEXT);
    let created = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        &root,
        "--title",
        "Tax matter",
        "--json",
    ]));
    let case_id = created["data"]["case"]["id"].as_str().unwrap().to_owned();
    assert!(
        run(&[
            "submission",
            "add",
            "--archive",
            &root,
            "--case",
            &case_id,
            "--description",
            "Posted the completed form.",
            "--artefact",
            &digest,
            "--json",
        ])
        .status
        .success()
    );

    run(&["archive", "derive", "--archive", &root, "--json"]);
    let shown = stdout_json(&run(&[
        "case",
        "show",
        "--archive",
        &root,
        &case_id,
        "--json",
    ]));
    let facts = shown["data"]["derived"].as_array().unwrap();
    assert_eq!(facts.len(), 1, "only the artefacts this case names");
    assert_eq!(facts[0]["artefact_digest"], digest);
    assert_eq!(facts[0]["media_type"], "text");
    assert_eq!(facts[0]["byte_length"], TEXT.len() as u64);

    let human = stdout_text(&run(&["case", "show", "--archive", &root, &case_id]));
    assert!(human.contains(&format!("{digest} text {} byte(s)", TEXT.len())));
    assert!(human.contains("Nothing here is verified, matched, or delivered."));
}

#[test]
fn a_case_with_no_derived_record_prints_no_derived_block_at_all() {
    let (_home, root) = archive();
    let created = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        &root,
        "--title",
        "Parking notice",
        "--json",
    ]));
    let case_id = created["data"]["case"]["id"].as_str().unwrap();
    let shown = stdout_json(&run(&[
        "case",
        "show",
        "--archive",
        &root,
        case_id,
        "--json",
    ]));
    assert_eq!(shown["data"]["derived"].as_array().unwrap().len(), 0);
    let human = stdout_text(&run(&["case", "show", "--archive", &root, case_id]));
    assert!(!human.contains("Derived metadata"));
}

#[test]
fn a_derived_record_is_disposable_and_its_loss_refuses_nothing() {
    let (_home, root) = archive();
    run(&["archive", "derive", "--archive", &root, "--json"]);
    let directory = Path::new(&root).join("records/derived");
    for entry in fs::read_dir(&directory).expect("the derived directory exists") {
        fs::remove_file(entry.expect("a derived record").path()).expect("remove it");
    }
    let checked = stdout_json(&run(&["archive", "check", "--archive", &root, "--json"]));
    assert_eq!(
        checked["ok"], true,
        "losing every one of them is not damage"
    );
    assert_eq!(checked["data"]["derived_records"], 0);

    let again = stdout_json(&run(&["archive", "derive", "--archive", &root, "--json"]));
    assert_eq!(again["data"]["records_written"], 3, "they compute again");
}

#[test]
fn a_derived_record_that_cannot_be_read_is_replaced_rather_than_reported() {
    let (_home, root) = archive();
    run(&["archive", "derive", "--archive", &root, "--json"]);
    let directory = Path::new(&root).join("records/derived");
    let first = fs::read_dir(&directory)
        .expect("the derived directory exists")
        .next()
        .expect("at least one record")
        .expect("a directory entry")
        .path();
    fs::write(&first, b"{ not a record").expect("damage the disposable record");

    let checked = stdout_json(&run(&["archive", "check", "--archive", &root, "--json"]));
    assert_eq!(checked["ok"], true, "a disposable record is never damage");
    assert_eq!(checked["data"]["derived_records"], 2, "it is not counted");

    let again = stdout_json(&run(&["archive", "derive", "--archive", &root, "--json"]));
    assert_eq!(again["data"]["records_written"], 3);
    let checked = stdout_json(&run(&["archive", "check", "--archive", &root, "--json"]));
    assert_eq!(checked["data"]["derived_records"], 3);
}

#[test]
fn deriving_refuses_a_root_that_is_not_an_archive_and_names_no_path() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let root = home.path().to_string_lossy().into_owned();
    let output = run(&["archive", "derive", "--archive", &root, "--json"]);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["command"], "archive.derive");
    assert_eq!(envelope["error"]["code"], "archive.marker_missing");
    assert!(
        !stdout_text(&output).contains(&root),
        "a refusal never echoes the path the user typed"
    );
}
