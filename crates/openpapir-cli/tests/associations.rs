//! Contract tests for receipt records and user-asserted association records.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence, and no test
//! writes a key or a secret. The assertions are about the observable contract
//! in `docs/error-contract.md` and `docs/architecture.md`: the envelope, the
//! codes, the exit codes, the stored documents, the consistency rules, and
//! the privacy rule, rather than about the implementation.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

/// A synthetic payload and its SHA-256 digest, computed independently.
const PAYLOAD: &[u8] = b"synthetic bytes\n";
const PAYLOAD_DIGEST: &str =
    "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
/// A second synthetic payload, for an import event of another artefact.
const OTHER_PAYLOAD: &[u8] = b"other synthetic bytes\n";
/// A well-formed digest that names no object in a fresh archive.
const ABSENT_DIGEST: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";
/// A well-formed identifier that names no record in a fresh archive.
const ABSENT_ID: &str = "0123456789abcdef0123456789abcdef";
/// A label and a statement no output outside `data` may ever carry.
const LABEL: &str = "Envelope the user kept";
const STATEMENT: &str = "The user stated the reference matches.";

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
    let details = envelope["error"]["details"]
        .as_object()
        .expect("details is an object");
    assert!(details.contains_key("bucket"), "details name a bucket");
    assert!(details.len() <= 16, "details hold at most sixteen keys");
}

/// Assert a refusal's code, exit code, and that nothing private leaked.
fn assert_refusal(output: &Output, command: &str, code: &str, exit: i32) {
    let envelope = stdout_json(output);
    assert_envelope(&envelope, command, false);
    assert_eq!(envelope["error"]["code"], code, "refusal code");
    assert_eq!(output.status.code(), Some(exit), "exit code for {code}");
    assert_ne!(output.status.code(), Some(1), "exit code 1 is reserved");
    assert!(output.stderr.is_empty(), "the JSON form writes no stderr");
    let rendered = String::from_utf8(output.stdout.clone()).unwrap();
    for (position, fragment) in [LABEL, STATEMENT, "records/", "passwd"].iter().enumerate() {
        assert!(
            !rendered.contains(fragment),
            "a refusal carries the forbidden fragment at position {position}"
        );
    }
}

/// Assert a `record.inconsistent` refusal naming one rule and nothing else.
fn assert_rule(output: &Output, command: &str, rule: &str) {
    assert_refusal(output, command, "record.inconsistent", 4);
    let envelope = stdout_json(output);
    let details = envelope["error"]["details"].as_object().unwrap();
    assert_eq!(details["rule"], rule);
    assert_eq!(details["bucket"], "record");
    assert_eq!(details.len(), 3, "bucket, record_kind, and rule only");
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary paths are UTF-8")
}

/// An archive holding one imported artefact, one case, and two submissions.
struct Fixture {
    root: tempfile::TempDir,
    first: String,
    second: String,
}

fn identifier(envelope: &Value, pointer: &str) -> String {
    envelope
        .pointer(pointer)
        .and_then(Value::as_str)
        .expect("the response names the record")
        .to_owned()
}

fn import(root: &Path, name: &str, payload: &[u8]) -> Value {
    let inputs = tempfile::tempdir().expect("create a temporary directory");
    let input = inputs.path().join(name);
    fs::write(&input, payload).expect("write a synthetic input");
    let output = run(&["import", "--archive", path(root), "--json", path(&input)]);
    assert!(output.status.success(), "the artefact is imported");
    stdout_json(&output)
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("create a temporary directory");
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    assert!(output.status.success(), "the archive is created");
    import(root.path(), "note.txt", PAYLOAD);
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "Case",
        "--json",
    ]);
    let case_id = identifier(&stdout_json(&output), "/data/case/id");
    let mut submissions = Vec::new();
    for description in ["First", "Second"] {
        let output = run(&[
            "submission",
            "add",
            "--archive",
            path(root.path()),
            "--case",
            &case_id,
            "--description",
            description,
            "--json",
        ]);
        submissions.push(identifier(&stdout_json(&output), "/data/submission/id"));
    }
    Fixture {
        root,
        first: submissions.remove(0),
        second: submissions.remove(0),
    }
}

/// Record a receipt for the imported artefact and return its identifier.
fn add_receipt(root: &Path) -> String {
    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(root),
        "--artefact",
        PAYLOAD_DIGEST,
        "--label",
        LABEL,
        "--json",
    ]);
    assert!(output.status.success(), "the receipt is recorded");
    identifier(&stdout_json(&output), "/data/receipt/id")
}

fn create_association(root: &Path, receipt: &str, outcome: &str, candidates: &[String]) -> Output {
    let mut args = vec![
        "association".to_owned(),
        "create".to_owned(),
        "--archive".to_owned(),
        path(root).to_owned(),
        "--receipt".to_owned(),
        receipt.to_owned(),
        "--outcome".to_owned(),
        outcome.to_owned(),
        "--json".to_owned(),
    ];
    for candidate in candidates {
        args.push("--candidate".to_owned());
        args.push(candidate.clone());
    }
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&borrowed)
}

fn candidate(submission_id: &str, confidence: &str) -> String {
    format!("{submission_id}:{confidence}:{STATEMENT}")
}

#[test]
fn a_receipt_records_its_artefact_and_import_event() {
    let f = fixture();
    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(f.root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--label",
        LABEL,
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "receipt.add", true);
    assert_eq!(output.status.code(), Some(0));
    let receipt = &envelope["data"]["receipt"];
    assert_eq!(receipt["artefact_digest"], PAYLOAD_DIGEST);
    assert_eq!(receipt["label"], LABEL);
    assert_eq!(receipt["record_kind"], "receipt");
    assert_eq!(receipt["archive_schema_version"], 1);
    assert_eq!(receipt["import_event_id"].as_str().unwrap().len(), 32);

    let id = identifier(&envelope, "/data/receipt/id");
    let stored = fs::read_to_string(
        f.root
            .path()
            .join("records/receipts")
            .join(format!("{id}.json")),
    )
    .expect("the record is stored where the design says");
    assert!(stored.ends_with("}\n"), "one LF-terminated document");

    let listed = stdout_json(&run(&[
        "receipt",
        "list",
        "--archive",
        path(f.root.path()),
        "--json",
    ]));
    assert_envelope(&listed, "receipt.list", true);
    assert_eq!(listed["data"]["count"], 1);
    assert_eq!(listed["data"]["receipts"][0]["id"], id.as_str());
}

#[test]
fn a_receipt_refuses_an_unknown_digest_or_a_mismatched_import_event() {
    let f = fixture();
    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(f.root.path()),
        "--artefact",
        ABSENT_DIGEST,
        "--json",
    ]);
    assert_refusal(&output, "receipt.add", "record.not_found", 4);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["record_kind"],
        "artefact"
    );

    let other = import(f.root.path(), "other.txt", OTHER_PAYLOAD);
    let event = identifier(&other, "/data/artefacts/0/import_event");
    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(f.root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--import-event",
        &event,
        "--json",
    ]);
    assert_rule(&output, "receipt.add", "import_event_digest_mismatch");
    assert_eq!(
        stdout_json(&output)["error"]["details"]["record_kind"],
        "receipt"
    );

    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(f.root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--import-event",
        ABSENT_ID,
        "--json",
    ]);
    assert_refusal(&output, "receipt.add", "record.not_found", 4);
    assert_eq!(
        fs::read_dir(f.root.path().join("records/receipts"))
            .unwrap()
            .count(),
        0,
        "no receipt survives a refusal"
    );
}

#[test]
fn all_four_outcomes_are_recorded_with_the_documented_shape() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    let one = [candidate(&f.first, "weak")];
    let two = [
        candidate(&f.first, "moderate"),
        candidate(&f.second, "strong"),
    ];
    for (outcome, candidates, expected) in [
        ("unassociated", &[][..], 0),
        ("candidate", &one[..], 1),
        ("associated", &one[..], 1),
        ("contradictory", &two[..], 2),
    ] {
        let output = create_association(f.root.path(), &receipt, outcome, candidates);
        let envelope = stdout_json(&output);
        assert_envelope(&envelope, "association.create", true);
        assert_eq!(output.status.code(), Some(0), "no outcome is an error");
        let association = &envelope["data"]["association"];
        assert_eq!(association["outcome"], outcome);
        assert_eq!(association["created_by"], "user");
        assert_eq!(association["record_kind"], "association");
        assert!(association["supersedes"].is_null());
        assert_eq!(
            association["candidates"].as_array().unwrap().len(),
            expected
        );
        if outcome == "associated" {
            assert_eq!(association["submission_id"], f.first.as_str());
        } else {
            assert!(
                association["submission_id"].is_null(),
                "only associated names a submission"
            );
        }
        for entry in association["candidates"].as_array().unwrap() {
            assert!(
                ["weak", "moderate", "strong"].contains(&entry["confidence"].as_str().unwrap())
            );
            let evidence = &entry["evidence"][0];
            assert_eq!(evidence["kind"], "user_assertion");
            assert_eq!(evidence["source"], "user");
            assert_eq!(evidence["statement"], STATEMENT);
        }
    }

    let listed = stdout_json(&run(&[
        "association",
        "list",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &receipt,
        "--json",
    ]));
    assert_envelope(&listed, "association.list", true);
    assert_eq!(listed["data"]["count"], 4, "history is never collapsed");
    assert_eq!(listed["data"]["receipt_id"], receipt.as_str());
}

#[test]
fn every_consistency_rule_is_refused_and_names_itself() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    let one = [candidate(&f.first, "weak")];
    let two = [candidate(&f.first, "weak"), candidate(&f.second, "weak")];
    let duplicate = [candidate(&f.first, "weak"), candidate(&f.first, "strong")];
    for (outcome, candidates, rule) in [
        ("unassociated", &one[..], "unassociated_has_candidates"),
        ("candidate", &[][..], "candidate_requires_candidates"),
        ("associated", &[][..], "associated_requires_one_candidate"),
        ("associated", &two[..], "associated_requires_one_candidate"),
        (
            "contradictory",
            &one[..],
            "contradictory_requires_two_candidates",
        ),
        (
            "contradictory",
            &duplicate[..],
            "duplicate_candidate_submission",
        ),
    ] {
        let output = create_association(f.root.path(), &receipt, outcome, candidates);
        assert_rule(&output, "association.create", rule);
    }
    assert_eq!(
        fs::read_dir(f.root.path().join("records/associations"))
            .unwrap()
            .count(),
        0,
        "a rule violation writes nothing"
    );
}

#[test]
fn a_supersession_chain_lists_newest_first_and_leaves_the_old_record_alone() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    let first = identifier(
        &stdout_json(&create_association(
            f.root.path(),
            &receipt,
            "unassociated",
            &[],
        )),
        "/data/association/id",
    );
    let stored = f
        .root
        .path()
        .join("records/associations")
        .join(format!("{first}.json"));
    let before = fs::read_to_string(&stored).expect("the first record is stored");

    let output = run(&[
        "association",
        "create",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &receipt,
        "--outcome",
        "candidate",
        "--candidate",
        &candidate(&f.first, "moderate"),
        "--supersedes",
        &first,
        "--json",
    ]);
    let second = stdout_json(&output);
    assert_envelope(&second, "association.create", true);
    assert_eq!(second["data"]["association"]["supersedes"], first.as_str());
    assert_eq!(
        fs::read_to_string(&stored).unwrap(),
        before,
        "a superseded record is never modified"
    );

    let listed = stdout_json(&run(&[
        "association",
        "list",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &receipt,
        "--json",
    ]));
    let history = listed["data"]["associations"].as_array().unwrap();
    assert_eq!(history.len(), 2, "the superseded record is still listed");
    assert_eq!(
        history[0]["id"], second["data"]["association"]["id"],
        "newest first"
    );
    assert_eq!(history[0]["supersedes"], first.as_str());
    assert_eq!(history[1]["id"], first.as_str());
    assert!(history[1]["supersedes"].is_null());
}

#[test]
fn supersedes_may_not_name_another_receipts_association() {
    let f = fixture();
    let first = add_receipt(f.root.path());
    let second = add_receipt(f.root.path());
    let elsewhere = identifier(
        &stdout_json(&create_association(
            f.root.path(),
            &second,
            "unassociated",
            &[],
        )),
        "/data/association/id",
    );
    let output = run(&[
        "association",
        "create",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &first,
        "--outcome",
        "unassociated",
        "--supersedes",
        &elsewhere,
        "--json",
    ]);
    assert_rule(&output, "association.create", "supersedes_other_receipt");

    let output = run(&[
        "association",
        "create",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &first,
        "--outcome",
        "unassociated",
        "--supersedes",
        ABSENT_ID,
        "--json",
    ]);
    assert_refusal(&output, "association.create", "record.not_found", 4);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["record_kind"],
        "association"
    );
}

#[test]
fn a_missing_receipt_or_submission_is_refused_without_echoing_it() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    let output = create_association(f.root.path(), ABSENT_ID, "unassociated", &[]);
    assert_refusal(&output, "association.create", "record.not_found", 4);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["record_kind"],
        "receipt"
    );

    let absent = [candidate(ABSENT_ID, "weak")];
    let output = create_association(f.root.path(), &receipt, "candidate", &absent);
    assert_refusal(&output, "association.create", "record.not_found", 4);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["record_kind"],
        "submission"
    );

    let output = run(&[
        "association",
        "list",
        "--archive",
        path(f.root.path()),
        "--receipt",
        ABSENT_ID,
        "--json",
    ]);
    assert_refusal(&output, "association.list", "record.not_found", 4);
}

#[test]
fn a_cap_or_an_unusable_value_is_refused_before_any_write() {
    let f = fixture();
    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(f.root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--label",
        &"l".repeat(201),
        "--json",
    ]);
    assert_refusal(&output, "receipt.add", "input.cap.field_length", 3);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["field"],
        "label",
        "the refusal names the field, never its value"
    );

    let receipt = add_receipt(f.root.path());
    let long = format!("{}:weak:{}", f.first, "s".repeat(513));
    let output = create_association(f.root.path(), &receipt, "candidate", &[long]);
    assert_refusal(&output, "association.create", "input.cap.field_length", 3);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["field"],
        "statement"
    );

    for (outcome, entry) in [
        ("delivered", None),
        (
            "candidate",
            Some(format!("{}:certain:{STATEMENT}", f.first)),
        ),
        ("candidate", Some(format!("{}:weak", f.first))),
        (
            "candidate",
            Some(format!("../../etc/passwd:weak:{STATEMENT}")),
        ),
    ] {
        let candidates: Vec<String> = entry.into_iter().collect();
        let output = create_association(f.root.path(), &receipt, outcome, &candidates);
        assert_refusal(&output, "association.create", "usage.arguments", 2);
    }
    assert_eq!(
        fs::read_dir(f.root.path().join("records/associations"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn a_malformed_document_is_refused_without_naming_a_path() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    create_association(f.root.path(), &receipt, "unassociated", &[]);
    for directory in ["records/receipts", "records/associations"] {
        let stray = f
            .root
            .path()
            .join(directory)
            .join("ffffffffffffffffffffffffffffffff.json");
        fs::write(&stray, b"{ not a record").expect("write a malformed document");
        let output = if directory.ends_with("receipts") {
            run(&[
                "receipt",
                "list",
                "--archive",
                path(f.root.path()),
                "--json",
            ])
        } else {
            run(&[
                "association",
                "list",
                "--archive",
                path(f.root.path()),
                "--receipt",
                &receipt,
                "--json",
            ])
        };
        let command = if directory.ends_with("receipts") {
            "receipt.list"
        } else {
            "association.list"
        };
        assert_refusal(&output, command, "record.malformed", 4);
        let details = stdout_json(&output)["error"]["details"].clone();
        assert_eq!(details["path_count"], 1);
        assert!(details.get("archive_path").is_none(), "no path is named");
        fs::remove_file(&stray).expect("remove the malformed document");
    }
}

/// Narrow a path to owner-only, as the archive requires of every file.
fn narrow(target: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(target, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = target;
}

#[test]
fn a_held_lock_refuses_a_write_and_permits_every_read() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    let lock = f.root.path().join("lock");
    fs::write(
        &lock,
        b"{\"host\":\"elsewhere\",\"pid\":1,\"started_at\":\"2026-01-14T09:12:33Z\"}\n",
    )
    .expect("a second writer holds the lock");
    narrow(&lock);

    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(f.root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--json",
    ]);
    assert_refusal(&output, "receipt.add", "lock.held", 4);
    let output = create_association(f.root.path(), &receipt, "unassociated", &[]);
    assert_refusal(&output, "association.create", "lock.held", 4);
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(!rendered.contains("elsewhere"), "no holder is named");

    let listed = run(&[
        "receipt",
        "list",
        "--archive",
        path(f.root.path()),
        "--json",
    ]);
    assert!(listed.status.success(), "a read needs no lock");
    let listed = run(&[
        "association",
        "list",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &receipt,
        "--json",
    ]);
    assert!(listed.status.success(), "a read needs no lock");
    assert!(lock.exists(), "the lock is not broken");
}

#[test]
fn the_human_form_prints_the_same_fields_and_no_path() {
    let f = fixture();
    let receipt = add_receipt(f.root.path());
    let output = run(&[
        "association",
        "create",
        "--archive",
        path(f.root.path()),
        "--receipt",
        &receipt,
        "--outcome",
        "associated",
        "--candidate",
        &candidate(&f.first, "strong"),
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(text.contains("Outcome: associated"));
    assert!(text.contains(&f.first), "the data fields are printed");
    assert!(text.contains(STATEMENT));
    assert!(!text.contains(path(f.root.path())), "no path is printed");
    assert!(!text.contains("records/"), "no archive path is printed");
    let lower = text.to_lowercase();
    for word in ["delivered", "accepted", "official", "legally effective"] {
        assert!(!lower.contains(word), "no wording implies an authority");
    }

    let listed = run(&["receipt", "list", "--archive", path(f.root.path())]);
    let text = String::from_utf8(listed.stdout).expect("stdout is UTF-8");
    assert!(text.contains("1 receipt(s) in this archive."));
    assert!(text.contains(LABEL) || text.contains(PAYLOAD_DIGEST));
}
