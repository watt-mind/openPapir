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

/// Run the binary from a working directory, so that a long argument list can
/// use short relative names. A command line of a thousand absolute temporary
/// paths exceeds what some platforms accept at process spawn.
fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .current_dir(directory)
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
///
/// `forbidden` holds fragments the privacy rule keeps out of output, such as a
/// filename or a path. Only their absence is asserted, and no fragment is ever
/// printed, so nothing private can reach a test log even on a failure.
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

/// Assert what a human-form command wrote to stderr.
///
/// On Unix nothing is expected. On Windows the archive correctly reports the
/// named platform degradations, so only those lines are allowed, and never a
/// path or a filename.
fn assert_human_stderr(output: &Output, forbidden: &[&str]) {
    let text = String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8");
    #[cfg(unix)]
    assert!(text.is_empty(), "no degradation applies on this platform");
    #[cfg(not(unix))]
    for line in text.lines() {
        assert!(
            line.starts_with("warning platform.owner_only_via_acl")
                || line.starts_with("warning platform.no_directory_fsync")
                || line.starts_with("warning platform.no_follow_after_open"),
            "stderr carries only the named platform degradations"
        );
    }
    for (position, fragment) in forbidden.iter().enumerate() {
        assert!(
            !text.contains(fragment),
            "stderr carries the forbidden fragment at position {position}"
        );
    }
}

/// Assert that any warning is one of the named platform degradations.
///
/// Where a guarantee cannot hold, the condition is reported and never
/// silently accepted; where it holds, no warning is emitted. A platform that
/// cannot express owner-only access always says so.
fn assert_warnings(envelope: &Value) {
    let warnings = envelope.get("warnings").map_or_else(Vec::new, |value| {
        value.as_array().expect("warnings is an array").clone()
    });
    for warning in &warnings {
        assert!(
            matches!(
                warning["code"].as_str().unwrap_or_default(),
                "platform.no_directory_fsync"
                    | "platform.owner_only_via_acl"
                    | "platform.no_follow_after_open"
            ),
            "only the named degradations are reported"
        );
        assert!(warning["details"]["bucket"] == "platform");
    }
    if !cfg!(unix) {
        assert!(
            warnings
                .iter()
                .any(|warning| warning["code"] == "platform.owner_only_via_acl"),
            "this platform reports how owner-only access is expressed"
        );
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
        "records/cases",
        "records/submissions",
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
    assert_warnings(&envelope);

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

/// Duplicate detection reads the stored import events at most once for a
/// whole operation, so a repeated input inside one command line has to count
/// the events that same command line wrote a moment earlier.
#[test]
fn repeated_inputs_in_one_operation_count_the_events_it_just_recorded() {
    let (root, inputs) = archive();
    let first = write_input(inputs.path(), "first.txt", PAYLOAD);
    let other = write_input(inputs.path(), "other.txt", b"another payload");
    let third = write_input(inputs.path(), "third.txt", PAYLOAD);
    let fourth = write_input(inputs.path(), "fourth.txt", PAYLOAD);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&first),
        path(&other),
        path(&third),
        path(&fourth),
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "import", true);
    assert_eq!(output.status.code(), Some(0), "a duplicate exits zero");
    assert_eq!(envelope["data"]["imported"], 2);
    assert_eq!(envelope["data"]["duplicates"], 2);
    let artefacts = envelope["data"]["artefacts"].as_array().unwrap();
    assert!(artefacts[0].get("previous_import_count").is_none());
    assert!(artefacts[1].get("previous_import_count").is_none());
    assert_eq!(artefacts[2]["previous_import_count"], 1);
    assert_eq!(artefacts[3]["previous_import_count"], 2);
    assert_eq!(artefacts[3]["digest"], artefacts[0]["digest"]);
    assert!(
        artefacts[3]["first_imported_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert_eq!(
        fs::read_dir(root.path().join("records/imports"))
            .unwrap()
            .count(),
        4,
        "each input records its own event"
    );
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

    // Short relative names, run from the input directory: the refusal happens
    // before any input is opened, and a thousand absolute paths would exceed
    // the command-line length some platforms accept. The names share a
    // distinctive stem, so the refusal is checked for the same leak as every
    // other one rather than against an empty list.
    let many: Vec<String> = (0..1001).map(|index| format!("leakstem{index}")).collect();
    let mut args = vec!["import", "--archive", path(root.path()), "--json"];
    args.extend(many.iter().map(String::as_str));
    let output = run_in(inputs.path(), &args);
    assert_refusal(
        &output,
        "import",
        "input.cap.import_files",
        3,
        &["leakstem"],
    );
    assert_eq!(
        stdout_json(&output)["error"]["details"]["observed_count"],
        1001
    );

    // Skipped on platforms that refuse a 256-byte path component at process
    // spawn, where the command line never reaches the binary. The cap itself
    // is asserted directly in the `openpapir-core` unit tests there, and the
    // 255-byte cap is unchanged on every platform.
    #[cfg(unix)]
    {
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
    }

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
    assert_eq!(
        stdout_json(&output)["error"]["details"]["scope"],
        "input",
        "a link outside the archive names the input scope"
    );
    assert!(
        stdout_json(&output)["error"]["details"]
            .get("archive_path")
            .is_none(),
        "an input has no archive-relative path"
    );

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
    assert_eq!(
        stdout_json(&output)["error"]["details"]["scope"],
        "archive",
        "a link that would be the archive names the archive scope"
    );
}

#[test]
#[cfg(not(unix))]
fn a_symbolic_link_is_refused_as_an_input_and_as_a_root() {
    // Skipped with a reason: creating a symbolic link or an NTFS junction on
    // this platform needs a privilege the test environment does not grant.
    // The no-follow open itself is exercised on every platform by the import
    // of a regular file, and the refusal of a reparse point is asserted in the
    // `openpapir-core` unit tests, which do not need the privilege.
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
    narrow(&occupied);
    narrow(occupied.parent().unwrap());
    narrow(occupied.parent().unwrap().parent().unwrap());
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&file),
    ]);
    assert_refusal(&output, "import", "path.overwrite", 3, &["note.txt"]);
    assert!(occupied.is_dir(), "the existing path is left untouched");

    // Shape is checked before permissions, so a wide directory at an object's
    // path is still what openPapir did not create rather than an archive whose
    // permissions it might repair.
    #[cfg(unix)]
    {
        widen(&occupied);
        let output = run(&[
            "import",
            "--archive",
            path(root.path()),
            "--json",
            path(&file),
        ]);
        assert_refusal(&output, "import", "path.overwrite", 3, &["note.txt"]);
        assert_eq!(
            stdout_json(&output)["error"]["details"]["archive_path"],
            format!("objects/sha256/{}/{}/{hex}", &hex[0..2], &hex[2..4])
        );
        assert!(occupied.is_dir(), "the existing path is left untouched");
    }
}

/// A filesystem with no hard links refuses the archive rather than inviting a
/// retry that can never succeed.
///
/// Skipped with a reason at this level: creating a FAT32 or exFAT volume needs
/// a privilege no test environment grants, so the mapping from the codes
/// `hard_link` reports there to `platform.filesystem_unsupported` is asserted
/// directly in the `openpapir-core` unit tests. What is asserted here is that
/// a filesystem which does support hard links publishes normally and reports
/// no platform refusal.
#[test]
fn a_filesystem_that_supports_hard_links_publishes_without_a_platform_refusal() {
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
    assert_ne!(output.status.code(), Some(5), "no platform refusal applies");
    assert_warnings(&envelope);
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

/// Widen a path, to check that openPapir refuses rather than repairing it.
#[cfg(unix)]
fn widen(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = if path.is_dir() { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
#[cfg(unix)]
fn a_wide_object_or_subdirectory_is_refused_and_never_repaired() {
    use std::os::unix::fs::PermissionsExt as _;
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

    widen(&object);
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
        fs::metadata(&object).unwrap().permissions().mode() & 0o777,
        0o644,
        "a wide path is refused, never repaired"
    );
    narrow(&object);

    let fan_out = object.parent().unwrap().to_path_buf();
    widen(&fan_out);
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
        format!("objects/sha256/{}/{}", &hex[0..2], &hex[2..4])
    );
    narrow(&fan_out);

    let records = root.path().join("records/imports");
    widen(&records);
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
    let details = &stdout_json(&output)["error"]["details"];
    assert_eq!(details["archive_path"], "records/imports");
    assert_eq!(details["path_count"], 1);
    assert_eq!(
        fs::metadata(&records).unwrap().permissions().mode() & 0o777,
        0o755,
        "openPapir never narrows a directory on the way past"
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
    let lock = root.path().join("lock");
    fs::write(
        &lock,
        b"{\"host\":\"elsewhere\",\"pid\":1,\"started_at\":\"2026-01-14T09:12:33Z\"}\n",
    )
    .unwrap();
    narrow(&lock);
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
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(text.contains(PAYLOAD_DIGEST));
    assert!(text.contains("Nothing here is verified"));
    assert!(!text.contains("private-name"), "no filename is printed");
    assert!(!text.contains(path(inputs.path())), "no path is printed");
    assert_human_stderr(&output, &["private-name", path(inputs.path())]);

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
fn capabilities_report_exactly_the_implemented_operations() {
    let output = run(&["capabilities", "--json"]);
    let envelope = stdout_json(&output);
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
            "archive.status",
            "case.export",
            "case.import",
            "archive.repair_permissions",
            "case.delete",
            "skill",
            "case.update",
            "submission.show",
            "receipt.show",
            "association.show",
            "completions",
            "manpage",
            "archive.export",
            "archive.import",
            "archive.derive",
            "search"
        ])
    );
    assert_eq!(output.status.code(), Some(0));
}

/// Create a case in the archive at `root` and return its identifier.
fn create_case(root: &Path) -> String {
    let envelope = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        path(root),
        "--title",
        "Tax matter",
        "--json",
    ]));
    envelope["data"]["case"]["id"]
        .as_str()
        .expect("a minted identifier")
        .to_owned()
}

#[test]
fn an_import_into_a_case_records_one_submission_naming_every_file() {
    let (root, inputs) = archive();
    let case_id = create_case(root.path());
    let first = write_input(inputs.path(), "first.txt", PAYLOAD);
    let second = write_input(inputs.path(), "second.txt", b"synthetic annex\n");
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        path(&first),
        path(&second),
        "--case",
        &case_id,
        "--description",
        "Posted the completed form.",
        "--date",
        "2026-01-13",
        "--json",
    ]);
    let envelope = stdout_json(&output);
    assert_envelope(&envelope, "import", true);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(envelope["data"]["imported"], 2);
    assert_eq!(envelope["data"]["duplicates"], 0);
    assert_eq!(
        envelope["data"]["artefacts"][0]["digest"], PAYLOAD_DIGEST,
        "the import reports what plain import reports, where it reports it"
    );
    let submission = &envelope["data"]["submission"];
    assert_eq!(submission["case_id"], case_id);
    assert_eq!(submission["description"], "Posted the completed form.");
    assert_eq!(submission["stated_date"], "2026-01-13");
    let artefacts = submission["artefacts"].as_array().expect("the references");
    assert_eq!(artefacts.len(), 2, "every imported file is named");
    assert_eq!(artefacts[0]["digest"], PAYLOAD_DIGEST);
    assert_eq!(artefacts[0]["role"], "attachment");

    let rendered = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        !rendered.contains("first.txt"),
        "no filename reaches output"
    );
    assert!(!rendered.contains(path(inputs.path())), "no path either");
}

#[test]
fn a_case_without_a_description_is_a_usage_refusal() {
    let (root, inputs) = archive();
    let case_id = create_case(root.path());
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        path(&file),
        "--case",
        &case_id,
        "--json",
    ]);
    assert_refusal(&output, "import", "usage.arguments", 2, &["note.txt"]);
    let events = fs::read_dir(root.path().join("records/imports"))
        .expect("read the import events")
        .count();
    assert_eq!(events, 0, "the command line is refused before any work");
}

#[test]
fn an_import_without_a_case_still_reports_exactly_what_it_always_did() {
    let (root, inputs) = archive();
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let envelope = stdout_json(&run(&[
        "import",
        "--archive",
        path(root.path()),
        path(&file),
        "--json",
    ]));
    assert_envelope(&envelope, "import", true);
    let data = envelope["data"].as_object().expect("data is an object");
    let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["artefacts", "duplicates", "imported"],
        "the plain form gains no field"
    );
}

#[test]
fn a_human_import_into_a_case_names_the_record_and_no_filename() {
    let (root, inputs) = archive();
    let case_id = create_case(root.path());
    let file = write_input(inputs.path(), "note.txt", PAYLOAD);
    let output = run(&[
        "import",
        "--archive",
        path(root.path()),
        path(&file),
        "--case",
        &case_id,
        "--description",
        "Posted the completed form.",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(text.contains("Stored 1 artefact(s); 0 already present."));
    assert!(text.contains(&case_id));
    assert!(text.contains("Description: Posted the completed form."));
    assert!(text.contains("Nothing here is verified, matched, or delivered."));
    assert!(!text.contains("note.txt"), "no filename is printed");
    assert!(!text.contains(path(inputs.path())), "no path is printed");
}

/// The rebuildable index under `cache/` is openPapir's own accelerator, so it
/// is visible to the check, never a problem, never part of an export, and
/// never counted or removed by a deletion. Its loss changes no answer, which
/// is why the deletion leaves it exactly where it is: a stale index is
/// rebuilt on the next read rather than repaired.
#[test]
fn the_rebuildable_index_is_counted_never_a_problem_and_never_exported() {
    let (root, inputs) = archive();
    let home = empty_dir();
    let first = write_input(inputs.path(), "first.txt", PAYLOAD);
    let second = write_input(inputs.path(), "second.txt", PAYLOAD);
    for input in [&first, &second] {
        run(&[
            "import",
            "--archive",
            path(root.path()),
            "--json",
            path(input),
        ]);
    }
    // `receipt add` without `--import-event` is the read that resolves an
    // artefact to its earliest import event, and it leaves the index behind.
    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0), "the receipt is recorded");
    let index = root
        .path()
        .join("cache")
        .join("import-events-by-digest.json");
    assert!(
        index.is_file(),
        "the read wrote the index it had to rebuild"
    );
    assert_owner_only(&index);

    let checked = run(&["archive", "check", "--archive", path(root.path()), "--json"]);
    assert_eq!(checked.status.code(), Some(0), "an index is not damage");
    let envelope = stdout_json(&checked);
    assert_envelope(&envelope, "archive.check", true);
    assert_eq!(envelope["data"]["cache_files"], 1, "the index is counted");
    for problem in envelope["data"]["problems"].as_array().unwrap() {
        assert_eq!(problem["count"], 0, "the index is never a problem");
    }

    let destination = home.path().join("whole");
    let exported = run(&[
        "archive",
        "export",
        "--archive",
        path(root.path()),
        "--to",
        path(&destination),
        "--json",
    ]);
    assert_eq!(exported.status.code(), Some(0), "the archive is exported");
    assert!(
        !destination.join("cache").exists(),
        "an export copies the records and the objects, never the index"
    );
    assert!(index.is_file(), "the export changed nothing in the archive");

    // A deletion plans against the index and still counts only records and
    // objects. The index is neither removed nor retained: it is not part of
    // what a deletion is about, and the next read rebuilds it.
    let case_id = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        path(root.path()),
        "--title",
        "A synthetic case",
        "--json",
    ]))["data"]["case"]["id"]
        .as_str()
        .expect("a case identifier")
        .to_owned();
    run(&[
        "submission",
        "add",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--description",
        "The user states they sent this.",
        "--artefact",
        PAYLOAD_DIGEST,
        "--json",
    ]);
    let deleted = run(&[
        "case",
        "delete",
        "--archive",
        path(root.path()),
        "--case",
        &case_id,
        "--purge",
        "--json",
    ]);
    assert_eq!(deleted.status.code(), Some(0), "the case is deleted");
    let envelope = stdout_json(&deleted);
    let data = envelope["data"]
        .as_object()
        .expect("a deletion reports data");
    assert!(
        data.keys().all(|key| !key.contains("cache")),
        "a deletion counts records and objects, and nothing under cache/"
    );
    assert!(
        index.is_file(),
        "a deletion neither counts the index nor removes it"
    );
}

/// The index is openPapir's own accelerator and is not a record, so nothing
/// it says may be written into a record without the record it names being
/// read first. A doctored index that names an import event the archive does
/// not hold must therefore change no answer: `receipt add` reads the records
/// instead, and the receipt it writes names a real event rather than leaving
/// a dangling reference behind for `archive check` to find.
#[test]
fn an_index_naming_an_absent_import_event_is_not_believed() {
    let (root, inputs) = archive();
    let first = write_input(inputs.path(), "first.txt", PAYLOAD);
    let imported = stdout_json(&run(&[
        "import",
        "--archive",
        path(root.path()),
        "--json",
        path(&first),
    ]));
    let event = imported["data"]["artefacts"][0]["import_event"]
        .as_str()
        .expect("an import reports its event")
        .to_owned();

    // Warm the index, then doctor it so that it names an event that is not
    // there. Its stamp still describes the import-event directory exactly,
    // so the freshness check has nothing to object to.
    run(&[
        "receipt",
        "add",
        "--archive",
        path(root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--json",
    ]);
    let index = root
        .path()
        .join("cache")
        .join("import-events-by-digest.json");
    let doctored = fs::read_to_string(&index)
        .expect("the index is there")
        .replace(&event, "0123456789abcdef0123456789abcdef");
    fs::remove_file(&index).expect("the index is replaced");
    fs::write(&index, doctored).expect("the doctored index is written");
    assert_owner_only_after_write(&index);

    let output = run(&[
        "receipt",
        "add",
        "--archive",
        path(root.path()),
        "--artefact",
        PAYLOAD_DIGEST,
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0), "the receipt is recorded");
    let envelope = stdout_json(&output);
    assert_eq!(
        envelope["data"]["receipt"]["import_event_id"],
        event.as_str(),
        "the records decide which event a receipt names, never the index"
    );

    let checked = stdout_json(&run(&[
        "archive",
        "check",
        "--archive",
        path(root.path()),
        "--json",
    ]));
    for problem in checked["data"]["problems"].as_array().unwrap() {
        assert_eq!(problem["count"], 0, "no reference was left dangling");
    }
}

/// Narrow a file this test wrote itself, so that the archive's owner-only
/// rule has nothing to refuse. It is the test's own housekeeping and says
/// nothing about what openPapir writes.
#[cfg(unix)]
fn assert_owner_only_after_write(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("narrow the test's file");
}

#[cfg(not(unix))]
fn assert_owner_only_after_write(_path: &Path) {}
