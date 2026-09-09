//! Contract tests for archive creation and artefact import.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence, and no test
//! writes a key or a secret. The assertions are about the observable contract
//! in `docs/error-contract.md`: the envelope, the codes, the exit codes, the
//! warnings, and the privacy rule, rather than about the implementation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A synthetic payload and its SHA-256 digest, computed independently.
const PAYLOAD: &[u8] = b"synthetic bytes\n";
const PAYLOAD_DIGEST: &str =
    "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

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
    if ok {
        assert!(envelope["data"].is_object());
        assert!(envelope.get("error").is_none());
    } else {
        assert_eq!(envelope["data"], serde_json::json!({}));
        let error = &envelope["error"];
        assert!(error["code"].is_string());
        assert!(error["message"].is_string());
        let details = error["details"].as_object().expect("details is an object");
        assert!(
            details.contains_key("bucket"),
            "details always name a bucket"
        );
        assert!(details.len() <= 16, "details hold at most sixteen keys");
        for value in details.values() {
            assert!(
                value.is_string()
                    || value.is_i64()
                    || value.is_u64()
                    || value.is_boolean()
                    || value.as_array().is_some_and(|items| items.len() <= 16),
                "details hold scalars or bounded arrays only"
            );
        }
    }
    let keys: Vec<&String> = envelope.as_object().unwrap().keys().collect();
    for key in &keys {
        assert!(
            matches!(
                key.as_str(),
                "schema_version" | "ok" | "command" | "data" | "verified" | "error" | "warnings"
            ),
            "unexpected envelope key {key}"
        );
    }
}

/// Assert a refusal's code and exit code, and that nothing private leaked.
fn assert_refusal(output: &Output, command: &str, code: &str, exit: i32, secrets: &[&str]) {
    let envelope = stdout_json(output);
    assert_envelope(&envelope, command, false);
    assert_eq!(envelope["error"]["code"], code, "refusal code");
    assert_eq!(output.status.code(), Some(exit), "exit code for {code}");
    assert_ne!(output.status.code(), Some(1), "exit code 1 is reserved");
    assert!(output.stderr.is_empty(), "the JSON form writes no stderr");
    let rendered = String::from_utf8(output.stdout.clone()).unwrap();
    for secret in secrets {
        assert!(!rendered.contains(secret), "output must not carry {secret}");
    }
}

/// A new, empty directory that is not yet an archive.
fn empty_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create a temporary directory")
}

/// An initialised archive plus the directory holding synthetic inputs.
fn archive() -> (tempfile::TempDir, tempfile::TempDir) {
    let root = empty_dir();
    let inputs = empty_dir();
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    assert!(output.status.success(), "the archive is created");
    (root, inputs)
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary paths are UTF-8")
}

fn write_input(inputs: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let file = inputs.join(name);
    fs::write(&file, bytes).expect("write a synthetic input");
    file
}

/// Create a file of `bytes` length without allocating it, to test a cap.
fn sparse_input(inputs: &Path, name: &str, bytes: u64) -> PathBuf {
    let file = inputs.join(name);
    let handle = fs::File::create(&file).expect("create a sparse input");
    handle.set_len(bytes).expect("set the reported length");
    file
}

#[test]
fn creating_an_archive_writes_a_marker_and_an_owner_only_layout() {
    let root = empty_dir();
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "archive.init", true);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(envelope["data"]["archive_schema_version"], 1);
    let archive_id = envelope["data"]["archive_id"].as_str().unwrap();
    assert_eq!(archive_id.len(), 32);
    assert!(archive_id.chars().all(|c| c.is_ascii_hexdigit()));

    let marker: Value =
        serde_json::from_str(&fs::read_to_string(root.path().join("papir-archive.json")).unwrap())
            .unwrap();
    assert_eq!(marker["archive_id"], archive_id);
    assert_eq!(marker["archive_schema_version"], 1);
    for relative in [
        "objects/sha256",
        "objects/incoming",
        "records/imports",
        "cache",
    ] {
        assert!(root.path().join(relative).is_dir(), "{relative} exists");
        assert_owner_only(&root.path().join(relative));
    }
    assert_owner_only(root.path());
}

#[cfg(unix)]
fn assert_owner_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "{path:?} is owner-only");
}

#[cfg(not(unix))]
fn assert_owner_only(_path: &Path) {
    // Skipped with a reason: this platform expresses owner-only access as an
    // access-control list, which the command reports as a warning instead.
}

#[test]
fn a_directory_that_is_not_an_empty_archive_is_never_adopted() {
    let root = empty_dir();
    fs::write(root.path().join("stray.txt"), b"unrelated").unwrap();
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    assert_refusal(
        &output,
        "archive.init",
        "archive.adopt_refused",
        4,
        &["stray"],
    );
    assert_eq!(stdout_json(&output)["error"]["details"]["entry_count"], 1);

    let missing = root.path().join("absent");
    let output = run(&["archive", "init", path(&missing), "--json"]);
    assert_refusal(
        &output,
        "archive.init",
        "usage.archive_root_missing",
        2,
        &["absent"],
    );

    let output = run(&["import", "--archive", path(root.path()), "--json", "x"]);
    assert_refusal(&output, "import", "archive.marker_missing", 4, &["stray"]);
}

#[test]
fn an_archive_is_never_initialised_over_an_existing_one() {
    let (root, _inputs) = archive();
    let output = run(&["archive", "init", path(root.path()), "--json"]);
    assert_refusal(&output, "archive.init", "path.overwrite", 3, &[]);
}

#[test]
fn importing_preserves_the_bytes_and_stores_a_read_only_object() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "import", true);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(envelope["data"]["imported"], 1);
    assert_eq!(envelope["data"]["duplicates"], 0);
    let artefact = &envelope["data"]["artefacts"][0];
    assert_eq!(artefact["digest"], PAYLOAD_DIGEST);
    assert_eq!(artefact["byte_length"], PAYLOAD.len());
    assert_eq!(artefact["created_object"], true);
    assert_eq!(artefact["import_event"].as_str().unwrap().len(), 32);
    assert!(envelope.get("warnings").is_none() == cfg!(unix));

    let hex = PAYLOAD_DIGEST.strip_prefix("sha256:").unwrap();
    let object = root
        .path()
        .join("objects/sha256")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(hex);
    assert_eq!(fs::read(&object).unwrap(), PAYLOAD, "bytes are preserved");
    assert_read_only(&object);

    let event = fs::read_dir(root.path().join("records/imports"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let record: Value = serde_json::from_str(&fs::read_to_string(event.path()).unwrap()).unwrap();
    assert_eq!(record["original_filename"], "note.txt");
    assert_eq!(record["record_kind"], "import_event");
    assert_eq!(record["digest"], PAYLOAD_DIGEST);
    assert_eq!(record["created_object"], true);
}

#[cfg(unix)]
fn assert_read_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o400, "a stored object is owner-read-only");
}

#[cfg(not(unix))]
fn assert_read_only(path: &Path) {
    assert!(
        fs::metadata(path).unwrap().permissions().readonly(),
        "a stored object is read-only"
    );
}

#[test]
fn a_duplicate_import_is_not_an_error_and_records_a_second_event() {
    let (root, inputs) = archive();
    let first = write_input(inputs.path(), "first.txt", PAYLOAD);
    let second = write_input(inputs.path(), "second.txt", PAYLOAD);
    run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&first),
    ]);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&second),
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "import", true);
    assert_eq!(output.status.code(), Some(0), "a duplicate exits zero");
    assert_eq!(envelope["data"]["imported"], 0);
    assert_eq!(envelope["data"]["duplicates"], 1);
    let artefact = &envelope["data"]["artefacts"][0];
    assert_eq!(artefact["created_object"], false);
    assert_eq!(artefact["previous_import_count"], 1);
    assert!(
        artefact["first_imported_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert_eq!(
        fs::read_dir(root.path().join("records/imports"))
            .unwrap()
            .count(),
        2,
        "an import-event count is history, not an anomaly"
    );
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(!rendered.contains("second.txt"), "no filename is reported");
}

#[test]
fn several_inputs_import_in_one_operation_under_one_lock() {
    let (root, inputs) = archive();
    let first = write_input(inputs.path(), "a.txt", b"first payload");
    let second = write_input(inputs.path(), "b.txt", b"second payload");
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&first),
        path(&second),
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "import", true);
    assert_eq!(envelope["data"]["imported"], 2);
    assert_eq!(envelope["data"]["artefacts"].as_array().unwrap().len(), 2);
    assert!(!root.path().join("lock").exists(), "the lock is released");
    assert_eq!(
        fs::read_dir(root.path().join("objects/incoming"))
            .unwrap()
            .count(),
        0,
        "no staging file survives a completed import"
    );
}

#[test]
fn every_cap_is_refused_before_the_input_is_read() {
    let (root, inputs) = archive();
    let over_file_cap = sparse_input(inputs.path(), "large.bin", 64 * 1024 * 1024 + 1);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&over_file_cap),
    ]);
    assert_refusal(&output, "import", "input.cap.file_size", 3, &["large.bin"]);
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["cap_bytes"], 64 * 1024 * 1024);
    assert_eq!(details["input_index"], 0);

    let mut over_total: Vec<String> = Vec::new();
    for index in 0..9 {
        let file = sparse_input(inputs.path(), &format!("part{index}.bin"), 60 * 1024 * 1024);
        over_total.push(file.to_str().unwrap().to_owned());
    }
    let mut args = vec!["import", "--archive", path(root.path()), "--json"];
    args.extend(over_total.iter().map(String::as_str));
    let output = run(&args);
    assert_refusal(
        &output,
        "import",
        "input.cap.import_bytes",
        3,
        &["part0.bin"],
    );

    let many: Vec<String> = (0..1001)
        .map(|index| format!("{}/absent{index}", path(inputs.path())))
        .collect();
    let mut args = vec!["import", "--archive", path(root.path()), "--json"];
    args.extend(many.iter().map(String::as_str));
    let output = run(&args);
    assert_refusal(&output, "import", "input.cap.import_files", 3, &["absent0"]);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["observed_count"],
        1001
    );

    let long_name = format!("{}/{}", path(inputs.path()), "n".repeat(256));
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        &long_name,
    ]);
    assert_refusal(&output, "import", "input.cap.filename_length", 3, &["nnnn"]);
    assert_eq!(
        stdout_json(&output)["error"]["details"]["observed_bytes"],
        256
    );

    assert_eq!(
        fs::read_dir(root.path().join("objects/sha256"))
            .unwrap()
            .count(),
        0,
        "no cap refusal stored anything"
    );
}

#[test]
fn a_traversal_looking_filename_is_stored_as_an_attribute_only() {
    let (root, inputs) = archive();
    let hostile = write_input(inputs.path(), "..%2f..%2fetc%2fpasswd", PAYLOAD);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&hostile),
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "import", true);
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(!rendered.contains("passwd"), "no filename reaches output");
    assert_eq!(
        fs::read_dir(root.path().join("records/imports"))
            .unwrap()
            .count(),
        1
    );
    assert!(
        !inputs.path().join("etc").exists(),
        "nothing is written outside the archive"
    );
}

#[test]
#[cfg(unix)]
fn a_symbolic_link_is_refused_as_an_input_and_as_a_root() {
    let (root, inputs) = archive();
    let target = write_input(inputs.path(), "target.txt", PAYLOAD);
    let link = inputs.path().join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&link),
    ]);
    assert_refusal(&output, "import", "path.symlink", 3, &["link.txt"]);

    let linked_root = inputs.path().join("linked-root");
    std::os::unix::fs::symlink(root.path(), &linked_root).unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(&linked_root),
        "--json",
        path(&target),
    ]);
    assert_refusal(&output, "import", "path.symlink", 3, &["linked-root"]);
}

#[test]
#[cfg(not(unix))]
fn a_symbolic_link_is_refused_as_an_input_and_as_a_root() {
    // Skipped with a reason: creating a symbolic link on this platform needs a
    // privilege the test environment does not grant, and the no-follow flag
    // the design requires has no portable equivalent here.
}

#[test]
fn an_object_path_holding_something_else_is_never_replaced() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let hex = PAYLOAD_DIGEST.strip_prefix("sha256:").unwrap();
    let occupied = root
        .path()
        .join("objects/sha256")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(hex);
    fs::create_dir_all(&occupied).unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(&output, "import", "path.overwrite", 3, &["note.txt"]);
    assert!(occupied.is_dir(), "the existing path is left untouched");
}

#[test]
fn a_stored_object_of_a_different_length_is_reported_as_damage() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    let hex = PAYLOAD_DIGEST.strip_prefix("sha256:").unwrap();
    let object = root
        .path()
        .join("objects/sha256")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(hex);
    make_writable(&object);
    fs::write(&object, b"a damaged store").unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(
        &output,
        "import",
        "integrity.length_mismatch",
        4,
        &["note.txt"],
    );
    assert_eq!(
        fs::read(&object).unwrap(),
        b"a damaged store",
        "damage is reported, never overwritten"
    );
}

fn make_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
    }
}

#[test]
fn a_leftover_staging_file_is_never_adopted() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let leftover = root
        .path()
        .join("objects/incoming/.papir-staging-interrupted");
    fs::write(&leftover, b"a partial write from an interrupted import").unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert!(
        output.status.success(),
        "an interrupted write blocks nothing"
    );
    let hex = PAYLOAD_DIGEST.strip_prefix("sha256:").unwrap();
    let object = root
        .path()
        .join("objects/sha256")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(hex);
    assert_eq!(
        fs::read(&object).unwrap(),
        PAYLOAD,
        "the archive holds the complete artefact or nothing"
    );
    assert!(leftover.exists(), "a staging file is never adopted");
}

#[test]
fn a_held_lock_refuses_a_second_writer_without_naming_the_holder() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    fs::write(
        root.path().join("lock"),
        b"{\"host\":\"elsewhere\",\"pid\":1,\"started_at\":\"2026-01-14T09:12:33Z\"}\n",
    )
    .unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(&output, "import", "lock.held", 4, &["elsewhere", "pid"]);
    assert!(root.path().join("lock").exists(), "the lock is not broken");
}

#[test]
fn a_schema_version_this_build_does_not_support_refuses_every_operation() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let marker = root.path().join("papir-archive.json");
    let mut document: Value = serde_json::from_str(&fs::read_to_string(&marker).unwrap()).unwrap();
    document["archive_schema_version"] = Value::from(2);
    fs::write(&marker, serde_json::to_string(&document).unwrap()).unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(&output, "import", "archive.schema_newer", 4, &["note.txt"]);
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["archive_schema_version"], 2);
    assert_eq!(details["supported_schema_version"], 1);

    fs::write(&marker, b"{ not a marker").unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(&output, "import", "archive.marker_malformed", 4, &[]);
}

#[test]
#[cfg(unix)]
fn an_archive_wider_than_owner_only_is_refused_with_no_override() {
    use std::os::unix::fs::PermissionsExt as _;
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(
        &output,
        "import",
        "archive.permissions_wide",
        4,
        &["note.txt"],
    );
    assert_eq!(
        stdout_json(&output)["error"]["details"]["archive_path"],
        "."
    );
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
#[cfg(not(unix))]
fn an_archive_wider_than_owner_only_is_refused_with_no_override() {
    // Skipped with a reason: this platform expresses owner-only access as an
    // access-control list, reported as the platform.owner_only_via_acl
    // warning, so there is no permission bit to widen here.
}

#[test]
fn a_cross_device_destination_is_refused_rather_than_copied() {
    // Skipped with a reason: mounting a second filesystem inside the archive
    // root needs a privilege no test environment grants, so the mapping from a
    // cross-device error to `path.cross_device` is asserted directly in the
    // `openpapir-core` unit tests instead of through the binary.
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert!(
        output.status.success(),
        "a single-filesystem archive imports normally"
    );
}

#[test]
fn an_unusable_input_is_a_usage_refusal_that_never_echoes_the_path() {
    let (root, inputs) = archive();
    let absent = inputs.path().join("no-such-file.txt");
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&absent),
    ]);
    assert_refusal(
        &output,
        "import",
        "usage.arguments",
        2,
        &["no-such-file", path(inputs.path())],
    );
    assert_eq!(stdout_json(&output)["error"]["details"]["argument"], "file");
}

#[test]
fn human_output_reports_the_same_facts_without_a_path_or_a_filename() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "private-name.txt", PAYLOAD);
    let output = run(&["import", "--archive", path(root.path()), path(&file)]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(PAYLOAD_DIGEST));
    assert!(text.contains("Nothing here is verified"));
    assert!(!text.contains("private-name"), "no filename is printed");
    assert!(!text.contains(path(inputs.path())), "no path is printed");
    assert!(output.stderr.is_empty());

    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        path(&absent(&inputs)),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "a failure prints no result");
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("usage.arguments"));
    assert!(!text.contains("no-such-file"), "stderr carries no filename");
}

fn absent(inputs: &tempfile::TempDir) -> PathBuf {
    inputs.path().join("no-such-file.txt")
}

#[test]
fn capabilities_report_exactly_the_two_implemented_operations() {
    let output = run(&["capabilities", "--json"]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "capabilities", true);
    assert_eq!(
        envelope["data"]["operations"],
        serde_json::json!(["archive.init", "import"])
    );
    assert_eq!(output.status.code(), Some(0));
}
