//! Contract tests for the read-only whole-archive integrity check.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence. The assertions
//! are about the observable contract in `docs/architecture.md` and
//! `docs/error-contract.md`: the report's counts, the codes, the precedence
//! between them, the exit codes, the privacy rule, and the promise that the
//! check reads the archive and changes nothing.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const PAYLOAD: &[u8] = b"synthetic bytes\n";
const PAYLOAD_DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
const ABSENT_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ABSENT_ID: &str = "00000000000000000000000000000000";

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

/// An archive holding one imported artefact and one case.
fn archive() -> (TempDir, PathBuf) {
    let home = tempfile::tempdir().expect("a temporary directory");
    let root = home.path().join("archive");
    fs::create_dir(&root).expect("create the archive root");
    let input = home.path().join("payload.bin");
    fs::write(&input, PAYLOAD).expect("write the synthetic payload");
    let root_text = root.to_string_lossy().into_owned();
    assert!(
        run(&["archive", "init", &root_text, "--json"])
            .status
            .success()
    );
    assert!(
        run(&[
            "import",
            "--archive",
            &root_text,
            &input.to_string_lossy(),
            "--json",
        ])
        .status
        .success()
    );
    (home, root)
}

fn check(root: &Path) -> Output {
    run(&[
        "archive",
        "check",
        "--archive",
        &root.to_string_lossy(),
        "--json",
    ])
}

/// The report's `problems` array as a map, asserting its documented shape.
fn problems(data: &Value) -> BTreeMap<String, u64> {
    let entries = data["problems"].as_array().expect("problems is an array");
    assert_eq!(
        entries.len(),
        6,
        "every code the check can report is listed"
    );
    let mut codes = Vec::new();
    let mut counts = BTreeMap::new();
    for entry in entries {
        let code = entry["code"].as_str().expect("a code").to_owned();
        codes.push(code.clone());
        counts.insert(code, entry["count"].as_u64().expect("a count"));
    }
    let mut sorted = codes.clone();
    sorted.sort();
    assert_eq!(codes, sorted, "the report is ordered by code");
    counts
}

/// Assert the envelope's fixed keys and that nothing private reached output.
///
/// `forbidden` holds fragments the privacy rule keeps out of every response,
/// such as the digest or the name of a damaged object. Only their absence is
/// asserted, and no fragment is printed, so nothing reaches a test log.
fn assert_report(output: &Output, ok: bool, code: Option<&str>, exit: i32) -> Value {
    let envelope = stdout_json(output);
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], ok);
    assert_eq!(envelope["command"], "archive.check");
    assert_eq!(envelope["verified"], false, "nothing is ever verified here");
    assert_eq!(output.status.code(), Some(exit), "exit code");
    assert_ne!(output.status.code(), Some(1), "exit code 1 is reserved");
    assert!(output.stderr.is_empty(), "the JSON form writes no stderr");
    match code {
        None => assert!(envelope.get("error").is_none()),
        Some(code) => {
            assert_eq!(envelope["error"]["code"], code);
            let details = envelope["error"]["details"]
                .as_object()
                .expect("details is an object");
            assert!(details.contains_key("bucket"));
            assert!(details.len() <= 16, "details hold at most sixteen keys");
        }
    }
    let data = envelope["data"].clone();
    assert!(
        data.is_object(),
        "the report stays in data whatever the outcome"
    );
    envelope
}

/// Assert that no forbidden fragment reached stdout.
fn assert_private(output: &Output, forbidden: &[&str]) {
    let rendered = String::from_utf8(output.stdout.clone()).unwrap();
    for (position, fragment) in forbidden.iter().enumerate() {
        assert!(
            !rendered.contains(fragment),
            "output carries the forbidden fragment at position {position}"
        );
    }
}

/// The only stored object's path.
fn object(root: &Path) -> PathBuf {
    root.join("objects/sha256")
        .join(&PAYLOAD_DIGEST[0..2])
        .join(&PAYLOAD_DIGEST[2..4])
        .join(PAYLOAD_DIGEST)
}

/// Replace a stored object's bytes, which openPapir itself never does.
fn damage(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .expect("make the object writable");
    }
    #[cfg(not(unix))]
    {
        // Owner-only access is an access-control list here, so clearing the
        // read-only attribute is what makes the stored object writable.
        #[allow(clippy::permissions_set_readonly_false)]
        {
            let mut permissions = fs::metadata(path).expect("the object exists").permissions();
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions).expect("make the object writable");
        }
    }
    fs::write(path, bytes).expect("damage the object");
}

/// Write one record document by hand, to build a reference to nothing.
fn write_record(root: &Path, directory: &str, id: &str, document: &Value) {
    let path = root.join(directory).join(format!("{id}.json"));
    fs::write(&path, format!("{document}\n")).expect("write the record document");
}

fn case_id(root: &Path) -> String {
    let output = run(&[
        "case",
        "create",
        "--archive",
        &root.to_string_lossy(),
        "--title",
        "Synthetic matter",
        "--json",
    ]);
    assert!(output.status.success());
    stdout_json(&output)["data"]["case"]["id"]
        .as_str()
        .expect("a case identifier")
        .to_owned()
}

#[test]
fn a_clean_archive_passes_and_reports_only_counts() {
    let (_home, root) = archive();
    let output = check(&root);
    let envelope = assert_report(&output, true, None, 0);
    let data = &envelope["data"];
    assert_eq!(data["objects_checked"], 1);
    assert_eq!(data["records_checked"], 1);
    assert_eq!(data["bytes_digested"], PAYLOAD.len() as u64);
    assert_eq!(data["orphan_objects"], 0);
    assert_eq!(data["staging_files"], 0);
    assert_eq!(data["objects_unchecked"], 0);
    assert!(problems(data).values().all(|count| *count == 0));
    assert_eq!(
        stdout_json(&check(&root)),
        envelope,
        "the report is stable between runs"
    );
}

#[test]
fn a_corrupted_object_is_reported_as_a_digest_mismatch_by_count_alone() {
    let (_home, root) = archive();
    let mut damaged = PAYLOAD.to_vec();
    damaged[0] = b'S';
    damage(&object(&root), &damaged);
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("integrity.digest_mismatch"), 4);
    let counts = problems(&envelope["data"]);
    assert_eq!(counts["integrity.digest_mismatch"], 1);
    assert_eq!(counts["integrity.length_mismatch"], 0);
    assert_eq!(envelope["data"]["objects_checked"], 1);
    assert_private(&output, &[PAYLOAD_DIGEST, "objects/sha256", "payload.bin"]);
}

#[test]
fn a_truncated_object_is_reported_as_a_length_mismatch() {
    let (_home, root) = archive();
    damage(&object(&root), b"synthetic");
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("integrity.digest_mismatch"), 4);
    let counts = problems(&envelope["data"]);
    assert_eq!(
        counts["integrity.length_mismatch"], 1,
        "the stored length differs from every import event naming it"
    );
    assert_eq!(counts["integrity.digest_mismatch"], 1);
    assert_private(&output, &[PAYLOAD_DIGEST]);
}

#[test]
fn an_object_no_record_references_is_reported_as_an_orphan() {
    let (_home, root) = archive();
    let imports = root.join("records/imports");
    for entry in fs::read_dir(&imports).expect("the import events exist") {
        fs::remove_file(entry.expect("an entry").path()).expect("remove the import event");
    }
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("integrity.orphan_object"), 4);
    let data = &envelope["data"];
    assert_eq!(data["orphan_objects"], 1);
    assert_eq!(data["records_checked"], 0);
    assert_eq!(problems(data)["integrity.orphan_object"], 1);
    assert_private(&output, &[PAYLOAD_DIGEST]);
}

#[test]
fn a_reference_that_names_nothing_is_reported_as_dangling() {
    let (_home, root) = archive();
    let case = case_id(&root);
    let submission = "1111111111111111111111111111111f";
    write_record(
        &root,
        "records/submissions",
        submission,
        &serde_json::json!({
            "archive_schema_version": 1,
            "artefacts": [{ "digest": format!("sha256:{ABSENT_DIGEST}") }],
            "case_id": case,
            "created_at": "2026-01-14T09:12:33Z",
            "description": "A synthetic submission.",
            "id": submission,
            "record_kind": "submission"
        }),
    );
    write_record(
        &root,
        "records/associations",
        "2222222222222222222222222222222f",
        &serde_json::json!({
            "archive_schema_version": 1,
            "candidates": [],
            "created_at": "2026-01-14T09:12:33Z",
            "created_by": "user",
            "id": "2222222222222222222222222222222f",
            "outcome": "unassociated",
            "receipt_id": ABSENT_ID,
            "record_kind": "association",
            "submission_id": null,
            "supersedes": null
        }),
    );
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("integrity.dangling_reference"), 4);
    assert_eq!(
        problems(&envelope["data"])["integrity.dangling_reference"],
        2,
        "the missing artefact and the missing receipt are both counted"
    );
    let details = &envelope["error"]["details"];
    assert_eq!(details["record_kind"], "submission");
    assert_eq!(details["reference_kind"], "artefact_digest");
    assert_eq!(details["path_count"], 2);
    assert_private(&output, &[ABSENT_DIGEST, ABSENT_ID, "records/submissions"]);
}

#[test]
fn every_kind_of_reference_a_record_holds_is_resolved() {
    let (_home, root) = archive();
    let submission = "3333333333333333333333333333333f";
    write_record(
        &root,
        "records/submissions",
        submission,
        &serde_json::json!({
            "archive_schema_version": 1,
            "artefacts": [],
            "case_id": ABSENT_ID,
            "created_at": "2026-01-14T09:12:33Z",
            "description": "A synthetic submission.",
            "id": submission,
            "record_kind": "submission"
        }),
    );
    let receipt = "4444444444444444444444444444444f";
    write_record(
        &root,
        "records/receipts",
        receipt,
        &serde_json::json!({
            "archive_schema_version": 1,
            "artefact_digest": format!("sha256:{PAYLOAD_DIGEST}"),
            "created_at": "2026-01-14T09:12:33Z",
            "id": receipt,
            "import_event_id": ABSENT_ID,
            "record_kind": "receipt"
        }),
    );
    let association = "5555555555555555555555555555555f";
    write_record(
        &root,
        "records/associations",
        association,
        &serde_json::json!({
            "archive_schema_version": 1,
            "candidates": [{
                "confidence": "moderate",
                "evidence": [{
                    "kind": "user_assertion",
                    "source": "user",
                    "statement": "A synthetic assertion."
                }],
                "submission_id": ABSENT_ID
            }],
            "created_at": "2026-01-14T09:12:33Z",
            "created_by": "user",
            "id": association,
            "outcome": "associated",
            "receipt_id": receipt,
            "record_kind": "association",
            "submission_id": ABSENT_ID,
            "supersedes": ABSENT_ID
        }),
    );
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("integrity.dangling_reference"), 4);
    assert_eq!(
        problems(&envelope["data"])["integrity.dangling_reference"],
        5,
        "the case, the import event, the confirmed submission, the candidate, and the superseded record"
    );
    assert_eq!(envelope["error"]["details"]["record_kind"], "submission");
    assert_eq!(envelope["error"]["details"]["reference_kind"], "case_id");
    assert_eq!(envelope["data"]["records_checked"], 4);
    assert_private(&output, &[ABSENT_ID]);
}

#[test]
fn a_document_that_is_not_a_record_is_reported_as_malformed() {
    let (_home, root) = archive();
    fs::write(
        root.join("records/cases").join(format!("{ABSENT_ID}.json")),
        b"{ not json",
    )
    .expect("write the unreadable document");
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("record.malformed"), 4);
    assert_eq!(problems(&envelope["data"])["record.malformed"], 1);
    assert_eq!(envelope["error"]["details"]["record_kind"], "case");
    assert_eq!(envelope["error"]["details"]["path_count"], 1);
    assert_private(&output, &[ABSENT_ID, "records/cases"]);
}

/// A symbolic link inside the store is refused rather than followed. The test
/// is Unix-only because creating one elsewhere needs a privilege the test
/// environment does not have; the refusal itself is platform-independent, and
/// on Windows the archive already reports the weaker no-follow guarantee as a
/// named degradation.
#[test]
#[cfg(unix)]
fn a_link_inside_the_store_is_never_followed() {
    let (home, root) = archive();
    let elsewhere = home.path().join("outside.bin");
    fs::write(&elsewhere, PAYLOAD).expect("write a file outside the archive");
    let planted = object(&root)
        .parent()
        .expect("the fan-out directory")
        .join(ABSENT_DIGEST);
    std::os::unix::fs::symlink(&elsewhere, &planted).expect("plant a link");
    let output = check(&root);
    let envelope = assert_report(&output, false, Some("path.symlink"), 3);
    assert_eq!(problems(&envelope["data"])["path.symlink"], 1);
    assert_eq!(envelope["error"]["details"]["scope"], "archive");
    assert_eq!(envelope["error"]["details"]["path_count"], 1);
    assert_private(&output, &["outside.bin", ABSENT_DIGEST]);
    assert!(
        fs::symlink_metadata(&planted)
            .expect("the link is still there")
            .file_type()
            .is_symlink(),
        "the check removes nothing"
    );
}

/// Every path, length, and modification time under the archive root.
fn snapshot(root: &Path) -> BTreeMap<String, (u64, Option<std::time::SystemTime>)> {
    let mut entries = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).expect("read a directory") {
            let path = entry.expect("an entry").path();
            let metadata = fs::symlink_metadata(&path).expect("stat an entry");
            let relative = path
                .strip_prefix(root)
                .expect("a path inside the root")
                .to_string_lossy()
                .into_owned();
            entries.insert(relative, (metadata.len(), metadata.modified().ok()));
            if metadata.is_dir() {
                stack.push(path);
            }
        }
    }
    entries
}

#[test]
fn the_check_reads_the_archive_and_changes_nothing() {
    let (_home, root) = archive();
    let case = case_id(&root);
    assert!(!case.is_empty());
    let before = snapshot(&root);
    assert!(check(&root).status.success());
    assert!(check(&root).status.success());
    assert_eq!(snapshot(&root), before, "no file, size, or time changed");
}

#[test]
fn a_missing_layout_directory_is_read_as_empty_and_never_created() {
    let (_home, root) = archive();
    for relative in ["records/associations", "objects/incoming"] {
        fs::remove_dir(root.join(relative)).expect("remove a layout directory");
    }
    let before = snapshot(&root);
    let output = check(&root);
    assert_report(&output, true, None, 0);
    assert_eq!(
        snapshot(&root),
        before,
        "the check creates no directory it found missing"
    );
    assert!(!root.join("records/associations").exists());
    assert!(!root.join("objects/incoming").exists());
}

/// The check must not need to write to the root it reads. The test is
/// Unix-only because a permission bit is the only portable way to withhold
/// write access here; on Windows owner-only access is an access-control list,
/// which the archive already reports as a named degradation.
#[test]
#[cfg(unix)]
fn a_root_that_cannot_be_written_to_is_still_checked() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_home, root) = archive();
    fs::remove_dir(root.join("cache")).expect("remove a layout directory");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).expect("withhold write access");
    let output = check(&root);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("restore the root");
    assert_report(&output, true, None, 0);
    assert!(!root.join("cache").exists(), "nothing was created");
}

/// A store the check cannot list at all leaves every digest unjudged. A
/// directory the process cannot search reports as missing when it is asked
/// whether it exists, so the check has to decide from the failure itself.
/// Unix-only for the same reason as the test above.
#[test]
#[cfg(unix)]
fn an_unreadable_object_store_is_counted_rather_than_read_as_empty() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_home, root) = archive();
    let objects = root.join("objects");
    fs::set_permissions(&objects, fs::Permissions::from_mode(0o000))
        .expect("withhold access to the store");
    if fs::read_dir(root.join("objects/sha256")).is_ok() {
        // The process can read the store anyway, which happens when the tests
        // run with privileges that ignore the permission bits.
        fs::set_permissions(&objects, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let output = check(&root);
    fs::set_permissions(&objects, fs::Permissions::from_mode(0o700)).expect("restore access");
    let envelope = assert_report(&output, true, None, 0);
    let data = &envelope["data"];
    assert_eq!(data["objects_unchecked"], 1, "the store is counted unread");
    assert_eq!(data["objects_checked"], 0);
    assert!(
        problems(data).values().all(|count| *count == 0),
        "an unread store is neither damage nor a missing object"
    );
}

/// A fan-out directory the check cannot list leaves the objects under it
/// unjudged rather than reported as missing. Unix-only for the same reason as
/// the test above.
#[test]
#[cfg(unix)]
fn an_unlistable_fan_out_directory_is_counted_rather_than_called_dangling() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_home, root) = archive();
    let directory = object(&root).parent().expect("the fan-out").to_path_buf();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o000))
        .expect("withhold access to the fan-out directory");
    if fs::read_dir(&directory).is_ok() {
        // The process can read the directory anyway, which happens when the
        // tests run with privileges that ignore the permission bits.
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let output = check(&root);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("restore access");
    let envelope = assert_report(&output, true, None, 0);
    let data = &envelope["data"];
    assert_eq!(data["objects_unchecked"], 1, "the directory is counted");
    assert_eq!(data["objects_checked"], 0);
    let counts = problems(data);
    assert_eq!(
        counts["integrity.dangling_reference"], 0,
        "an object the check could not look for is not a missing object"
    );
    assert_eq!(counts["integrity.digest_mismatch"], 0);
    assert_eq!(counts["path.symlink"], 0);
}

#[test]
fn the_check_runs_while_a_writer_lock_file_is_present() {
    let (_home, root) = archive();
    let lock = root.join("lock");
    fs::write(&lock, b"held").expect("write a lock file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o600))
            .expect("keep the lock file owner-only");
    }
    let output = check(&root);
    assert_report(&output, true, None, 0);
}

#[test]
fn a_large_archive_reports_counters_and_nothing_per_object() {
    let (home, root) = archive();
    let inputs: Vec<PathBuf> = (0..200)
        .map(|index| {
            let path = home.path().join(format!("input-{index}.bin"));
            fs::write(&path, format!("synthetic {index}\n")).expect("write an input");
            path
        })
        .collect();
    let root_text = root.to_string_lossy().into_owned();
    let mut arguments = vec!["import", "--archive", &root_text];
    let names: Vec<String> = inputs
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    arguments.extend(names.iter().map(String::as_str));
    arguments.push("--json");
    assert!(run(&arguments).status.success());

    let output = check(&root);
    let envelope = assert_report(&output, true, None, 0);
    let data = envelope["data"].as_object().expect("a report object");
    assert_eq!(data["objects_checked"], 201);
    assert_eq!(data["records_checked"], 201);
    for (key, value) in data {
        assert!(
            value.is_u64() || key == "problems",
            "the report holds counters only, never a list of objects"
        );
    }
    assert_eq!(problems(&envelope["data"]).len(), 6);
}

#[test]
fn the_human_form_prints_the_same_counts_and_no_path() {
    let (_home, root) = archive();
    damage(&object(&root), b"synthetic");
    let output = run(&["archive", "check", "--archive", &root.to_string_lossy()]);
    assert_eq!(output.status.code(), Some(4));
    let text = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(text.contains("Checked 1 object(s) and 1 record(s)"));
    assert!(text.contains("integrity.digest_mismatch 1"));
    assert!(text.contains("integrity.length_mismatch 1"));
    assert!(!text.contains(PAYLOAD_DIGEST), "no digest reaches a human");
    assert!(!text.contains('/'), "no path reaches a human");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(stderr.contains("error integrity.digest_mismatch"));
    assert!(!stderr.contains(PAYLOAD_DIGEST));
}

#[test]
fn an_archive_that_cannot_be_opened_is_refused_without_a_report() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let output = check(home.path());
    let envelope = assert_report(&output, false, Some("archive.marker_missing"), 4);
    assert_eq!(envelope["data"], serde_json::json!({}));
    let missing = home.path().join("absent");
    let output = check(&missing);
    assert_report(&output, false, Some("usage.archive_root_missing"), 2);
    assert_private(&output, &["absent"]);
}
