//! Contract tests for case and submission records.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence, and no test
//! writes a key or a secret. The assertions are about the observable contract
//! in `docs/error-contract.md` and `docs/architecture.md`: the envelope, the
//! codes, the exit codes, the stored documents, and the privacy rule, rather
//! than about the implementation.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

/// A synthetic payload and its SHA-256 digest, computed independently.
const PAYLOAD: &[u8] = b"synthetic bytes\n";
const PAYLOAD_DIGEST: &str =
    "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
/// A well-formed digest that names no object in a fresh archive.
const ABSENT_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";
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

/// Assert the envelope's fixed keys and the rules that bind every response.
fn assert_envelope(envelope: &Value, command: &str, ok: bool) {
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], ok);
    assert_eq!(envelope["command"], command);
    assert_eq!(envelope["verified"], false, "nothing is ever verified here");
    for key in envelope.as_object().unwrap().keys() {
        assert!(
            matches!(
                key.as_str(),
                "schema_version" | "ok" | "command" | "data" | "verified" | "error" | "warnings"
            ),
            "unexpected envelope key {key}"
        );
    }
    if ok {
        assert!(envelope["data"].is_object());
        assert!(envelope.get("error").is_none());
        return;
    }
    assert_eq!(envelope["data"], serde_json::json!({}));
    let error = &envelope["error"];
    assert!(error["message"].is_string());
    let details = error["details"].as_object().expect("details is an object");
    assert!(details.contains_key("bucket"), "details name a bucket");
    assert!(details.len() <= 16, "details hold at most sixteen keys");
    for value in details.values() {
        assert!(
            value.is_string() || value.is_u64() || value.is_i64() || value.is_boolean(),
            "these details hold scalars only"
        );
    }
}

/// Assert a refusal's code and exit code, and that nothing private leaked.
///
/// `forbidden` holds fragments the privacy rule keeps out of output. Only
/// their absence is asserted, and no fragment is printed, so nothing private
/// can reach a test log even on a failure.
fn assert_refusal(output: &Output, command: &str, code: &str, exit: i32, forbidden: &[&str]) {
    let envelope = stdout_json(output);
    assert_envelope(&envelope, command, false);
    assert_eq!(envelope["error"]["code"], code, "refusal code");
    assert_eq!(output.status.code(), Some(exit), "exit code for {code}");
    assert_ne!(output.status.code(), Some(1), "exit code 1 is reserved");
    assert!(output.stderr.is_empty(), "the JSON form writes no stderr");
    let rendered = String::from_utf8(output.stdout.clone()).unwrap();
    for (position, fragment) in forbidden.iter().enumerate() {
        assert!(
            !rendered.contains(fragment),
            "output carries the forbidden fragment at position {position}"
        );
    }
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary paths are UTF-8")
}

/// An initialised archive holding one imported artefact.
fn archive() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("create a temporary directory");
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    assert!(output.status.success(), "the archive is created");
    let inputs = tempfile::tempdir().expect("create a temporary directory");
    let input = inputs.path().join("note.txt");
    fs::write(&input, PAYLOAD).expect("write a synthetic input");
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&input),
    ]);
    assert!(output.status.success(), "the artefact is imported");
    root
}

/// Create a case and return its identifier.
fn create_case(root: &Path, title: &str) -> String {
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root),
        "--title",
        title,
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "case.create", true);
    envelope["data"]["case"]["id"]
        .as_str()
        .expect("a minted identifier")
        .to_owned()
}

#[test]
fn a_case_round_trips_through_create_list_and_show() {
    let root = archive();
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "Tax matter",
        "--notes",
        "First contact.",
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "case.create", true);
    assert_eq!(output.status.code(), Some(0));
    let case = &envelope["data"]["case"];
    let id = case["id"].as_str().unwrap();
    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(case["title"], "Tax matter");
    assert_eq!(case["notes"], "First contact.");
    assert_eq!(case["record_kind"], "case");
    assert_eq!(case["archive_schema_version"], 1);
    assert!(case["created_at"].as_str().unwrap().ends_with('Z'));

    let stored =
        fs::read_to_string(root.path().join("records/cases").join(format!("{id}.json"))).unwrap();
    assert!(stored.ends_with("}\n"), "one LF-terminated document");
    assert_eq!(stored.lines().count(), 1);
    let keys: Vec<String> = serde_json::from_str::<Value>(&stored)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "record keys are stored sorted");

    let listed = stdout_json(&run(&[
        "case",
        "list",
        "--archive",
        path(root.path()),
        "--json",
    ]));
    assert_envelope(&listed, "case.list", true);
    assert_eq!(listed["data"]["count"], 1);
    assert_eq!(listed["data"]["cases"][0]["id"], id);

    let shown = stdout_json(&run(&[
        "case",
        "show",
        "--archive",
        path(root.path()),
        id,
        "--json",
    ]));
    assert_envelope(&shown, "case.show", true);
    assert_eq!(shown["data"]["case"]["title"], "Tax matter");
    assert_eq!(shown["data"]["submission_count"], 0);
    assert_eq!(shown["data"]["submissions"].as_array().unwrap().len(), 0);
}

#[test]
fn an_empty_archive_lists_no_case_and_still_succeeds() {
    let root = archive();
    let output = run(&["case", "list", "--archive", path(root.path()), "--json"]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "case.list", true);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(envelope["data"]["count"], 0);
    assert_eq!(envelope["data"]["cases"], serde_json::json!([]));
}

#[test]
fn a_submission_records_several_artefacts_with_their_roles() {
    let root = archive();
    let case_id = create_case(root.path(), "Tax matter");
    let with_role = format!("{PAYLOAD_DIGEST}:cover letter");
    let output = run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--description",
        "Posted the completed form.",
        "--date",
        "2026-01-13",
        "--artefact",
        &with_role,
        "--artefact",
        PAYLOAD_DIGEST,
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "submission.add", true);
    assert_eq!(output.status.code(), Some(0));
    let submission = &envelope["data"]["submission"];
    assert_eq!(submission["case_id"], case_id);
    assert_eq!(submission["description"], "Posted the completed form.");
    assert_eq!(submission["stated_date"], "2026-01-13");
    assert_eq!(submission["record_kind"], "submission");
    let artefacts = submission["artefacts"].as_array().unwrap();
    assert_eq!(artefacts.len(), 2);
    assert_eq!(artefacts[0]["digest"], PAYLOAD_DIGEST);
    assert_eq!(artefacts[0]["role"], "cover letter");
    assert!(
        artefacts[1].get("role").is_none(),
        "a reference with no role omits the field"
    );

    let shown = stdout_json(&run(&[
        "case",
        "show",
        "--archive",
        path(root.path()),
        &case_id,
        "--json",
    ]));
    assert_eq!(shown["data"]["submission_count"], 1);
    let listed = &shown["data"]["submissions"][0];
    assert_eq!(listed["id"], submission["id"]);
    assert_eq!(listed["artefacts"][0]["role"], "cover letter");
}

#[test]
fn a_submission_without_a_date_or_an_artefact_is_recorded() {
    let root = archive();
    let case_id = create_case(root.path(), "Plain case");
    let envelope = stdout_json(&run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--description",
        "Nothing attached.",
        "--json",
    ]));
    assert_envelope(&envelope, "submission.add", true);
    let submission = &envelope["data"]["submission"];
    assert_eq!(submission["artefacts"], serde_json::json!([]));
    assert!(submission.get("stated_date").is_none());
}

#[test]
fn an_unknown_case_or_digest_is_refused_as_a_record_reference() {
    let root = archive();
    let case_id = create_case(root.path(), "Tax matter");

    let output = run(&[
        "case",
        "show",
        "--archive",
        path(root.path()),
        ABSENT_ID,
        "--json",
    ]);
    assert_refusal(&output, "case.show", "record.not_found", 4, &[ABSENT_ID]);
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["record_kind"], "case");
    assert_eq!(details["reference_kind"], "case_id");
    assert_eq!(details.as_object().unwrap().len(), 3);

    let output = run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        ABSENT_ID,
        "--description",
        "Sent something.",
        "--json",
    ]);
    assert_refusal(
        &output,
        "submission.add",
        "record.not_found",
        4,
        &[ABSENT_ID, "Sent something"],
    );

    let output = run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--description",
        "Sent something.",
        "--artefact",
        ABSENT_DIGEST,
        "--json",
    ]);
    assert_refusal(
        &output,
        "submission.add",
        "record.not_found",
        4,
        &[ABSENT_DIGEST, "Sent something"],
    );
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["record_kind"], "artefact");
    assert_eq!(details["reference_kind"], "artefact_digest");
    assert_eq!(
        fs::read_dir(root.path().join("records/submissions"))
            .unwrap()
            .count(),
        0,
        "an unresolved reference writes nothing"
    );
}

#[test]
fn every_field_cap_is_refused_before_anything_is_written() {
    let root = archive();
    let case_id = create_case(root.path(), "Tax matter");
    let long_title = "t".repeat(201);
    let long_notes = "n".repeat(4097);
    let long_description = "d".repeat(1025);
    let long_role = format!("{PAYLOAD_DIGEST}:{}", "r".repeat(65));

    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        &long_title,
        "--json",
    ]);
    assert_refusal(
        &output,
        "case.create",
        "input.cap.field_length",
        3,
        &["ttttt"],
    );
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["field"], "title");
    assert_eq!(details["cap_bytes"], 200);
    assert_eq!(details["observed_bytes"], 201);

    for (args, field, cap, forbidden) in [
        (
            vec![
                "case",
                "create",
                "--archive",
                path(root.path()),
                "--title",
                "Fine",
                "--notes",
                &long_notes,
                "--json",
            ],
            "notes",
            4096,
            "nnnnn",
        ),
        (
            vec![
                "submission",
                "add",
                "--archive",
                path(root.path()),
                "--case",
                &case_id,
                "--description",
                &long_description,
                "--json",
            ],
            "description",
            1024,
            "ddddd",
        ),
        (
            vec![
                "submission",
                "add",
                "--archive",
                path(root.path()),
                "--case",
                &case_id,
                "--description",
                "Fine",
                "--artefact",
                &long_role,
                "--json",
            ],
            "role",
            64,
            "rrrrr",
        ),
    ] {
        let output = run(&args);
        let command = if args[0] == "case" {
            "case.create"
        } else {
            "submission.add"
        };
        assert_refusal(&output, command, "input.cap.field_length", 3, &[forbidden]);
        let details = &stdout_json(&output)["error"]["details"];
        assert_eq!(details["field"], field);
        assert_eq!(details["cap_bytes"], cap);
    }

    assert_eq!(
        fs::read_dir(root.path().join("records/cases"))
            .unwrap()
            .count(),
        1,
        "only the one valid case was written"
    );
    assert_eq!(
        fs::read_dir(root.path().join("records/submissions"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn an_unusable_field_or_reference_is_a_usage_refusal() {
    let root = archive();
    let case_id = create_case(root.path(), "Tax matter");
    for (args, command) in [
        (
            vec![
                "case",
                "create",
                "--archive",
                path(root.path()),
                "--title",
                "one\ntwo",
                "--json",
            ],
            "case.create",
        ),
        (
            vec![
                "case",
                "create",
                "--archive",
                path(root.path()),
                "--title",
                "   ",
                "--json",
            ],
            "case.create",
        ),
        (
            vec![
                "submission",
                "add",
                "--archive",
                path(root.path()),
                "--case",
                &case_id,
                "--description",
                "Fine",
                "--date",
                "2026-02-30",
                "--json",
            ],
            "submission.add",
        ),
        (
            vec![
                "submission",
                "add",
                "--archive",
                path(root.path()),
                "--case",
                &case_id,
                "--description",
                "Fine",
                "--artefact",
                "../../etc/passwd",
                "--json",
            ],
            "submission.add",
        ),
    ] {
        let output = run(&args);
        assert_refusal(&output, command, "usage.arguments", 2, &["passwd", "two"]);
    }
}

#[test]
fn a_malformed_record_document_is_refused_without_naming_a_path() {
    let root = archive();
    let case_id = create_case(root.path(), "Readable");
    let stray = root
        .path()
        .join("records/cases")
        .join(format!("{ABSENT_ID}.json"));
    fs::write(&stray, b"{ not a record document").unwrap();
    let output = run(&["case", "list", "--archive", path(root.path()), "--json"]);
    assert_refusal(
        &output,
        "case.list",
        "record.malformed",
        4,
        &[ABSENT_ID, path(root.path()), "records/cases", "Readable"],
    );
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["record_kind"], "case");
    assert_eq!(details["path_count"], 1);
    assert_eq!(details.as_object().unwrap().len(), 3);
    fs::remove_file(&stray).unwrap();

    let broken = root
        .path()
        .join("records/cases")
        .join(format!("{case_id}.json"));
    make_writable(&broken);
    fs::write(&broken, b"{\"record_kind\":\"case\"}\n").unwrap();
    let output = run(&[
        "case",
        "show",
        "--archive",
        path(root.path()),
        &case_id,
        "--json",
    ]);
    assert_refusal(
        &output,
        "case.show",
        "record.malformed",
        4,
        &[path(root.path()), "records/cases"],
    );
}

#[test]
fn a_leftover_staging_file_is_neither_a_record_nor_a_malformed_one() {
    let root = archive();
    let case_id = create_case(root.path(), "Readable");
    // What an interrupted write leaves behind. It can never be adopted as a
    // record, because its name is not an identifier, and a reader deletes
    // nothing: removing it is a write, and only the writer lock permits one.
    let staging = root
        .path()
        .join("records/cases")
        .join(".papir-staging-000000000000");
    fs::write(&staging, b"{ partial").unwrap();
    let output = run(&["case", "list", "--archive", path(root.path()), "--json"]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "case.list", true);
    assert_eq!(envelope["data"]["count"], 1, "the one real case is listed");
    assert_eq!(envelope["data"]["cases"][0]["id"], case_id);
    assert!(staging.exists(), "a listing deletes nothing");
    assert!(
        !String::from_utf8(output.stdout.clone())
            .unwrap()
            .contains("papir-staging"),
        "a transient artefact is never named in the output"
    );
}

#[test]
fn a_record_document_larger_than_the_record_cap_is_refused_promptly() {
    let root = archive();
    create_case(root.path(), "Readable");
    let oversized = root
        .path()
        .join("records/cases")
        .join(format!("{ABSENT_ID}.json"));
    // Sparse, so the cap is exercised from the size the filesystem reports
    // without a megabyte of test data being written or read.
    let handle = fs::File::create(&oversized).expect("create a sparse document");
    handle.set_len(1024 * 1024 + 1).expect("set the length");
    drop(handle);

    let started = std::time::Instant::now();
    let output = run(&["case", "list", "--archive", path(root.path()), "--json"]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "the document is refused rather than read"
    );
    assert_refusal(
        &output,
        "case.list",
        "record.malformed",
        4,
        &[ABSENT_ID, path(root.path()), "records/cases"],
    );
    assert_eq!(stdout_json(&output)["error"]["details"]["path_count"], 1);
}

#[test]
#[cfg(unix)]
fn a_record_document_that_is_a_symbolic_link_is_never_followed() {
    let root = archive();
    create_case(root.path(), "Readable");
    // A link to an endless device would hang a reader that followed it, and a
    // link to a host file would read bytes the archive does not own.
    std::os::unix::fs::symlink(
        "/dev/zero",
        root.path()
            .join("records/cases")
            .join(format!("{ABSENT_ID}.json")),
    )
    .unwrap();
    let started = std::time::Instant::now();
    let output = run(&["case", "list", "--archive", path(root.path()), "--json"]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "the link is refused rather than followed"
    );
    assert_refusal(
        &output,
        "case.list",
        "record.malformed",
        4,
        &[ABSENT_ID, path(root.path()), "records/cases", "/dev/zero"],
    );
}

#[test]
#[cfg(not(unix))]
fn a_record_document_that_is_a_symbolic_link_is_never_followed() {
    // Skipped with a reason: creating a symbolic link on this platform needs a
    // privilege the test environment does not grant, so neither this test nor
    // the `openpapir-core` unit test of the same rule can build the case. The
    // reader opens every record document without following a link on every
    // platform, with the weaker guarantee `docs/architecture.md` names where
    // the operating system offers no no-follow flag.
}

fn make_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    {
        // On Windows clearing the read-only attribute clears it for everyone
        // rather than for the owner alone, and it is what makes the file
        // writable here; owner-only access stays the access-control list.
        #[allow(clippy::permissions_set_readonly_false)]
        {
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions).unwrap();
        }
    }
}

#[test]
fn a_held_lock_refuses_a_writer_but_never_a_reader() {
    let root = archive();
    let case_id = create_case(root.path(), "Tax matter");
    let lock = root.path().join("lock");
    fs::write(
        &lock,
        b"{\"host\":\"elsewhere\",\"pid\":1,\"started_at\":\"2026-01-14T09:12:33Z\"}\n",
    )
    .unwrap();
    narrow(&lock);

    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "Blocked",
        "--json",
    ]);
    assert_refusal(
        &output,
        "case.create",
        "lock.held",
        4,
        &["elsewhere", "pid"],
    );

    let output = run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--description",
        "Blocked",
        "--json",
    ]);
    assert_refusal(
        &output,
        "submission.add",
        "lock.held",
        4,
        &["elsewhere", "pid"],
    );

    for args in [
        vec!["case", "list", "--archive", path(root.path()), "--json"],
        vec![
            "case",
            "show",
            "--archive",
            path(root.path()),
            &case_id,
            "--json",
        ],
    ] {
        let output = run(&args);
        assert!(output.status.success(), "reading needs no lock at all");
    }
    assert!(lock.exists(), "the lock is never broken");
}

/// Narrow a path a test created to owner-only, so that the fixture itself is
/// not what the permission check refuses.
fn narrow(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[test]
fn a_record_command_refuses_an_archive_it_may_not_open() {
    let root = tempfile::tempdir().unwrap();
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "No archive here",
        "--json",
    ]);
    assert_refusal(
        &output,
        "case.create",
        "archive.marker_missing",
        4,
        &[path(root.path())],
    );

    let missing = root.path().join("absent");
    let output = run(&["case", "list", "--archive", path(&missing), "--json"]);
    assert_refusal(
        &output,
        "case.list",
        "usage.archive_root_missing",
        2,
        &["absent"],
    );
}

#[test]
fn human_output_reports_the_same_records_without_a_path() {
    let root = archive();
    let case_id = create_case(root.path(), "Tax matter");
    let with_role = format!("{PAYLOAD_DIGEST}:cover letter");
    let output = run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--description",
        "Posted the completed form.",
        "--date",
        "2026-01-13",
        "--artefact",
        &with_role,
    ]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(&case_id));
    assert!(text.contains("Posted the completed form."));
    assert!(text.contains("Date stated by the user: 2026-01-13."));
    assert!(text.contains("cover letter"));
    assert!(text.contains("Nothing here is verified, matched, or delivered."));
    assert!(!text.contains(path(root.path())), "no path is printed");
    assert!(!text.contains("records/"), "no archive path is printed");

    let output = run(&["case", "show", "--archive", path(root.path()), &case_id]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Title: Tax matter"));
    assert!(text.contains("Submissions recorded: 1."));
    assert!(!text.contains(path(root.path())));

    let output = run(&["case", "list", "--archive", path(root.path())]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("1 case(s) in this archive."));
    assert!(!text.contains(path(root.path())));

    let output = run(&["case", "show", "--archive", path(root.path()), ABSENT_ID]);
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty(), "a failure prints no result");
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("record.not_found"));
    assert!(!text.contains(path(root.path())));
}

#[test]
fn capabilities_report_exactly_the_implemented_operations() {
    let envelope = stdout_json(&run(&["capabilities", "--json"]));
    assert_envelope(&envelope, "capabilities", true);
    assert_eq!(
        envelope["data"]["operations"],
        serde_json::json!([
            "archive.init",
            "import",
            "case.create",
            "case.list",
            "case.show",
            "submission.add",
            "receipt.add",
            "receipt.list",
            "association.create",
            "association.list",
            "association.retire",
            "archive.check",
            "case.export",
            "archive.repair_permissions",
            "case.delete",
            "skill"
        ])
    );
}
