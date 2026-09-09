//! Contract tests for `case delete` and its explicit purge.
//!
//! Every fixture here is synthetic and built at test time from constants in
//! this file. Nothing is derived from real correspondence. The assertions are
//! about the observable contract in `docs/architecture.md` and
//! `docs/error-contract.md`: what a deletion removes, what it leaves, the
//! counts it reports, the codes it refuses with, the promise that a refusal
//! removes nothing at all, and the privacy rule that keeps a digest, a path,
//! and a filename out of every deletion report.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const FIRST: &[u8] = b"synthetic submission bytes\n";
const SECOND: &[u8] = b"synthetic receipt bytes\n";
const SHARED: &[u8] = b"synthetic shared bytes\n";
const ABSENT_CASE: &str = "00000000000000000000000000000000";

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

/// Run one command that must succeed, returning its `data` object.
fn data(args: &[&str]) -> Value {
    let output = run(args);
    assert!(
        output.status.success(),
        "{} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_json(&output)["data"].clone()
}

/// An initialised archive and a place to write synthetic inputs.
struct Fixture {
    home: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("a temporary directory");
        let root = home.path().join("archive");
        fs::create_dir(&root).expect("create the archive root");
        let fixture = Self { home, root };
        assert!(
            run(&["archive", "init", &fixture.root_text(), "--json"])
                .status
                .success()
        );
        fixture
    }

    fn root_text(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// Import synthetic bytes and return the algorithm-qualified digest.
    fn import(&self, name: &str, bytes: &[u8]) -> String {
        let input = self.home.path().join(name);
        fs::write(&input, bytes).expect("write the synthetic input");
        let imported = data(&[
            "import",
            "--archive",
            &self.root_text(),
            &input.to_string_lossy(),
            "--json",
        ]);
        imported["artefacts"][0]["digest"]
            .as_str()
            .expect("a digest")
            .to_owned()
    }

    fn case(&self, title: &str) -> String {
        data(&[
            "case",
            "create",
            "--archive",
            &self.root_text(),
            "--title",
            title,
            "--json",
        ])["case"]["id"]
            .as_str()
            .expect("a case identifier")
            .to_owned()
    }

    fn submission(&self, case_id: &str, digest: &str) -> String {
        data(&[
            "submission",
            "add",
            "--archive",
            &self.root_text(),
            "--case",
            case_id,
            "--description",
            "The user states they sent this.",
            "--artefact",
            digest,
            "--json",
        ])["submission"]["id"]
            .as_str()
            .expect("a submission identifier")
            .to_owned()
    }

    fn receipt(&self, digest: &str) -> String {
        data(&[
            "receipt",
            "add",
            "--archive",
            &self.root_text(),
            "--artefact",
            digest,
            "--json",
        ])["receipt"]["id"]
            .as_str()
            .expect("a receipt identifier")
            .to_owned()
    }

    fn associate(&self, receipt_id: &str, submission_id: &str) {
        let candidate = format!("{submission_id}:moderate:The user stated a link.");
        data(&[
            "association",
            "create",
            "--archive",
            &self.root_text(),
            "--receipt",
            receipt_id,
            "--outcome",
            "associated",
            "--candidate",
            &candidate,
            "--json",
        ]);
    }

    fn delete(&self, case_id: &str, purge: bool) -> Output {
        let mut args = vec![
            "case".to_owned(),
            "delete".to_owned(),
            "--archive".to_owned(),
            self.root_text(),
            "--case".to_owned(),
            case_id.to_owned(),
            "--json".to_owned(),
        ];
        if purge {
            args.push("--purge".to_owned());
        }
        run(&args.iter().map(String::as_str).collect::<Vec<&str>>())
    }

    fn check(&self) -> Output {
        run(&["archive", "check", "--archive", &self.root_text(), "--json"])
    }

    fn object(&self, digest: &str) -> PathBuf {
        let hex = digest.trim_start_matches("sha256:");
        self.root
            .join("objects/sha256")
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(hex)
    }
}

/// One archive with a case, two submissions, a receipt, and an association.
fn populated() -> (Fixture, String, String, String) {
    let fixture = Fixture::new();
    let first = fixture.import("first.bin", FIRST);
    let second = fixture.import("second.bin", SECOND);
    let case_id = fixture.case("A local matter");
    let submission = fixture.submission(&case_id, &first);
    fixture.submission(&case_id, &second);
    let receipt = fixture.receipt(&second);
    fixture.associate(&receipt, &submission);
    (fixture, case_id, first, second)
}

/// The `records_removed` array as pairs, asserting its documented shape.
fn removed(data: &Value) -> Vec<(String, u64)> {
    let entries = data["records_removed"]
        .as_array()
        .expect("records_removed is an array");
    assert_eq!(entries.len(), 5, "every record kind is listed");
    entries
        .iter()
        .map(|entry| {
            (
                entry["kind"].as_str().expect("a kind").to_owned(),
                entry["count"].as_u64().expect("a count"),
            )
        })
        .collect()
}

/// The `objects_retained` array as pairs, asserting its documented shape.
fn retained(data: &Value) -> Vec<(String, u64)> {
    let entries = data["objects_retained"]
        .as_array()
        .expect("objects_retained is an array");
    assert_eq!(entries.len(), 4, "every reason is listed");
    entries
        .iter()
        .map(|entry| {
            (
                entry["reason"].as_str().expect("a reason").to_owned(),
                entry["count"].as_u64().expect("a count"),
            )
        })
        .collect()
}

/// Assert that no digest, path, or filename reached an envelope.
fn assert_private(envelope: &Value) {
    let text = envelope.to_string();
    assert!(!text.contains("sha256:"), "no digest reaches a deletion");
    assert!(!text.contains(".bin"), "no filename reaches a deletion");
    assert!(
        !text.contains("objects/") && !text.contains("records/"),
        "no path reaches a deletion"
    );
}

#[test]
fn a_deletion_without_purge_removes_records_and_keeps_every_object() {
    let (fixture, case_id, first, second) = populated();
    let output = fixture.delete(&case_id, false);
    assert!(output.status.success(), "the deletion did its stated work");
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "case.delete");
    assert_eq!(envelope["verified"], false);
    assert_private(&envelope);

    let data = &envelope["data"];
    assert_eq!(data["purge"], false);
    assert_eq!(data["objects_removed"], 0);
    assert_eq!(
        removed(data),
        vec![
            ("association".to_owned(), 1),
            ("case".to_owned(), 1),
            ("import_event".to_owned(), 0),
            ("receipt".to_owned(), 1),
            ("submission".to_owned(), 2),
        ]
    );
    assert_eq!(data["records_removed_total"], 5);
    assert_eq!(
        retained(data),
        vec![
            ("purge_not_requested".to_owned(), 2),
            ("records_retained".to_owned(), 0),
            ("referenced_elsewhere".to_owned(), 0),
            ("unremovable".to_owned(), 0),
        ]
    );
    assert_eq!(data["objects_retained_total"], 2);

    assert!(fixture.object(&first).is_file(), "the bytes are still here");
    assert!(fixture.object(&second).is_file());
    let imports = fs::read_dir(fixture.root.join("records/imports"))
        .expect("the import directory")
        .count();
    assert_eq!(imports, 2, "import events are kept as history");
    let checked = fixture.check();
    assert!(checked.status.success(), "the archive is still clean");
    assert_eq!(stdout_json(&checked)["data"]["orphan_objects"], 0);
}

#[test]
fn a_purge_removes_what_nothing_else_references_and_keeps_what_it_shares() {
    let fixture = Fixture::new();
    let own = fixture.import("own.bin", FIRST);
    let shared = fixture.import("shared.bin", SHARED);
    let case_id = fixture.case("A local matter");
    fixture.submission(&case_id, &own);
    fixture.submission(&case_id, &shared);
    let other = fixture.case("Another local matter");
    fixture.submission(&other, &shared);

    let output = fixture.delete(&case_id, true);
    assert!(output.status.success());
    let envelope = stdout_json(&output);
    assert_private(&envelope);
    let data = &envelope["data"];
    assert_eq!(data["purge"], true);
    assert_eq!(data["objects_removed"], 1);
    assert_eq!(
        retained(data),
        vec![
            ("purge_not_requested".to_owned(), 0),
            ("records_retained".to_owned(), 0),
            ("referenced_elsewhere".to_owned(), 1),
            ("unremovable".to_owned(), 0),
        ]
    );
    assert_eq!(data["objects_retained_total"], 1);
    assert_eq!(
        removed(data)
            .into_iter()
            .find(|(kind, _)| kind == "import_event"),
        Some(("import_event".to_owned(), 1)),
        "the import event of a purged object goes with it"
    );

    assert!(!fixture.object(&own).exists(), "the purged bytes are gone");
    assert!(
        fixture.object(&shared).is_file(),
        "an object another case references is kept"
    );
    let checked = fixture.check();
    assert!(
        checked.status.success(),
        "a purge leaves a clean archive: {}",
        String::from_utf8_lossy(&checked.stdout)
    );
    let report = stdout_json(&checked);
    assert_eq!(report["data"]["orphan_objects"], 0);
    assert_eq!(report["data"]["objects_checked"], 1);
}

#[test]
fn an_unknown_case_is_refused_and_the_archive_is_untouched() {
    let (fixture, case_id, _, _) = populated();
    let before = snapshot(&fixture.root);
    let output = fixture.delete(ABSENT_CASE, true);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "record.not_found");
    assert_eq!(envelope["error"]["details"]["bucket"], "record");
    assert_eq!(envelope["data"], serde_json::json!({}));
    assert_eq!(snapshot(&fixture.root), before, "nothing was removed");
    assert!(fixture.delete(&case_id, false).status.success());
}

#[test]
fn a_malformed_record_aborts_the_deletion_before_anything_is_removed() {
    let (fixture, case_id, _, _) = populated();
    let stray = fixture
        .root
        .join("records/submissions")
        .join("ffffffffffffffffffffffffffffffff.json");
    hold_owner_only(&stray);
    let before = snapshot(&fixture.root);

    for purge in [false, true] {
        let output = fixture.delete(&case_id, purge);
        assert_eq!(output.status.code(), Some(4));
        let envelope = stdout_json(&output);
        assert_eq!(envelope["error"]["code"], "record.malformed");
        assert_eq!(envelope["error"]["details"]["record_kind"], "submission");
        assert_private(&envelope);
        assert_eq!(
            snapshot(&fixture.root),
            before,
            "the scan aborts before the first unlink"
        );
    }

    fs::remove_file(&stray).expect("remove the malformed document");
    assert!(fixture.delete(&case_id, true).status.success());
}

#[test]
fn a_held_writer_lock_refuses_the_deletion() {
    let (fixture, case_id, _, _) = populated();
    let lock = fixture.root.join("lock");
    hold_owner_only(&lock);
    let before = snapshot(&fixture.root);
    let output = fixture.delete(&case_id, true);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "lock.held");
    assert_eq!(envelope["error"]["details"]["bucket"], "lock");
    assert_eq!(snapshot(&fixture.root), before, "nothing was removed");
    fs::remove_file(&lock).expect("release the writer lock");
    assert!(fixture.delete(&case_id, false).status.success());
}

#[cfg(unix)]
#[test]
fn an_object_that_cannot_be_unlinked_is_reported_as_a_count() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::new();
    let digest = fixture.import("own.bin", FIRST);
    let case_id = fixture.case("A local matter");
    fixture.submission(&case_id, &digest);
    let fan_out = fixture
        .object(&digest)
        .parent()
        .expect("the fan-out directory")
        .to_path_buf();
    fs::set_permissions(&fan_out, fs::Permissions::from_mode(0o500))
        .expect("make the fan-out directory read-only");

    let output = fixture.delete(&case_id, true);
    fs::set_permissions(&fan_out, fs::Permissions::from_mode(0o700)).expect("restore the mode");
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "delete.objects_retained");
    assert_eq!(envelope["error"]["details"]["bucket"], "delete");
    assert_eq!(envelope["error"]["details"]["retained_count"], 1);
    assert_eq!(envelope["error"]["details"]["reason"], "unremovable");
    assert_private(&envelope);

    let data = &envelope["data"];
    assert_eq!(data["objects_removed"], 0);
    assert_eq!(data["records_removed_total"], 2, "the records still went");
    assert_eq!(
        retained(data),
        vec![
            ("purge_not_requested".to_owned(), 0),
            ("records_retained".to_owned(), 0),
            ("referenced_elsewhere".to_owned(), 0),
            ("unremovable".to_owned(), 1),
        ]
    );
    assert!(fixture.object(&digest).is_file(), "the object stayed");
}

#[test]
fn an_association_spanning_two_cases_refuses_the_deletion_and_touches_nothing() {
    let fixture = Fixture::new();
    let first = fixture.import("first.bin", FIRST);
    let second = fixture.import("second.bin", SECOND);
    let going = fixture.case("A local matter");
    let staying = fixture.case("Another local matter");
    let departing = fixture.submission(&going, &first);
    let remaining = fixture.submission(&staying, &second);
    let receipt = fixture.receipt(&second);
    data(&[
        "association",
        "create",
        "--archive",
        &fixture.root_text(),
        "--receipt",
        &receipt,
        "--outcome",
        "contradictory",
        "--candidate",
        &format!("{departing}:weak:The user stated one link."),
        "--candidate",
        &format!("{remaining}:weak:The user stated another link."),
        "--json",
    ]);
    let before = snapshot(&fixture.root);

    for purge in [false, true] {
        let output = fixture.delete(&going, purge);
        assert_eq!(output.status.code(), Some(4));
        let envelope = stdout_json(&output);
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["error"]["code"], "delete.record_entangled");
        assert_eq!(envelope["error"]["details"]["bucket"], "delete");
        assert_eq!(envelope["error"]["details"]["record_kind"], "association");
        assert_eq!(envelope["error"]["details"]["retained_count"], 1);
        assert_eq!(envelope["data"], serde_json::json!({}));
        assert_private(&envelope);
        assert_eq!(
            snapshot(&fixture.root),
            before,
            "the refusal comes before anything is unlinked"
        );
    }

    assert!(
        fixture.check().status.success(),
        "the archive is left clean rather than holding a dangling reference"
    );
    // The entanglement is symmetric: the association names one submission
    // from each case, so deleting either case would leave it naming a
    // submission the archive no longer holds, and both are refused until the
    // user resolves the association themselves.
    let other = fixture.delete(&staying, true);
    assert_eq!(other.status.code(), Some(4));
    assert_eq!(
        stdout_json(&other)["error"]["code"],
        "delete.record_entangled"
    );
    assert_eq!(snapshot(&fixture.root), before, "still nothing was removed");
}

/// A record directory the process cannot write to is the one way to refuse a
/// record unlink from outside. Windows expresses the same refusal through an
/// access-control list, which this test cannot set portably, so the case is
/// exercised on Unix alone and the skip is recorded here.
#[cfg(unix)]
#[test]
fn a_refused_record_unlink_stops_the_purge_before_it_touches_an_object() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::new();
    let own = fixture.import("own.bin", FIRST);
    let shared = fixture.import("shared.bin", SHARED);
    let case_id = fixture.case("A local matter");
    fixture.submission(&case_id, &own);
    fixture.submission(&case_id, &shared);
    let other = fixture.case("Another local matter");
    fixture.submission(&other, &shared);

    let submissions = fixture.root.join("records/submissions");
    let objects = fixture.root.join("objects");
    let before = snapshot(&objects);
    fs::set_permissions(&submissions, fs::Permissions::from_mode(0o500))
        .expect("make the submission directory read-only");
    let output = fixture.delete(&case_id, true);
    fs::set_permissions(&submissions, fs::Permissions::from_mode(0o700)).expect("restore the mode");

    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "delete.records_retained");
    assert_eq!(envelope["error"]["details"]["bucket"], "delete");
    assert_eq!(
        envelope["error"]["details"]["retained_count"], 3,
        "two submissions and the case it holds are all still there"
    );
    assert_private(&envelope);

    let data = &envelope["data"];
    assert_eq!(data["objects_removed"], 0, "no object may be touched");
    assert_eq!(data["records_retained"], 3);
    assert_eq!(data["records_removed_total"], 0);
    assert_eq!(
        retained(data),
        vec![
            ("purge_not_requested".to_owned(), 0),
            ("records_retained".to_owned(), 1),
            ("referenced_elsewhere".to_owned(), 1),
            ("unremovable".to_owned(), 0),
        ],
        "the objects the purge had planned are all still in the store"
    );
    assert_eq!(
        removed(data)
            .into_iter()
            .find(|(kind, _)| kind == "import_event"),
        Some(("import_event".to_owned(), 0)),
        "no import event goes when no object went"
    );

    assert_eq!(
        snapshot(&objects),
        before,
        "every stored object is exactly where it was"
    );
    let checked = fixture.check();
    assert!(
        checked.status.success(),
        "the archive stays clean: {}",
        String::from_utf8_lossy(&checked.stdout)
    );
    assert_eq!(stdout_json(&checked)["data"]["orphan_objects"], 0);

    // With the directory writable again the same invocation finishes the job,
    // so the refusal cost the user nothing but the retry.
    let retry = fixture.delete(&case_id, true);
    assert!(retry.status.success(), "the retry does the work");
    assert_eq!(retry.status.code(), Some(0));
    let data = stdout_json(&retry)["data"].clone();
    assert_eq!(data["records_retained"], 0);
    assert_eq!(
        removed(&data),
        vec![
            ("association".to_owned(), 0),
            ("case".to_owned(), 1),
            ("import_event".to_owned(), 1),
            ("receipt".to_owned(), 0),
            ("submission".to_owned(), 2),
        ]
    );
    assert_eq!(data["records_removed_total"], 4);
    assert_eq!(
        data["objects_removed"], 1,
        "only the case's own object goes"
    );
    assert_eq!(
        retained(&data),
        vec![
            ("purge_not_requested".to_owned(), 0),
            ("records_retained".to_owned(), 0),
            ("referenced_elsewhere".to_owned(), 1),
            ("unremovable".to_owned(), 0),
        ]
    );
    assert!(!fixture.object(&own).exists(), "the purged bytes are gone");
    assert!(
        fixture.object(&shared).is_file(),
        "the object the other case references is kept"
    );
    assert!(fixture.check().status.success(), "and the archive is clean");
}

/// On Windows an unlink another process defers is reported as a warning
/// rather than counted as a removal. The condition cannot be produced on
/// Unix, where an open file is unlinked immediately, so the case is exercised
/// on Windows alone and the skip is recorded here.
#[cfg(not(unix))]
#[test]
fn a_deferred_unlink_is_a_platform_warning_rather_than_a_removal() {
    let fixture = Fixture::new();
    let digest = fixture.import("own.bin", FIRST);
    let case_id = fixture.case("A local matter");
    fixture.submission(&case_id, &digest);
    let held = fs::File::open(fixture.object(&digest)).expect("hold the object open");
    let output = fixture.delete(&case_id, true);
    drop(held);
    let envelope = stdout_json(&output);
    let codes: Vec<&str> = envelope["warnings"]
        .as_array()
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(|warning| warning["code"].as_str())
                .collect()
        })
        .unwrap_or_default();
    if output.status.code() == Some(4) {
        assert_eq!(envelope["error"]["code"], "delete.objects_retained");
        assert!(codes.contains(&"platform.replace_while_open"));
    } else {
        assert_eq!(envelope["data"]["objects_removed"], 1);
    }
    assert_private(&envelope);
}

#[test]
fn human_output_prints_the_same_counts_and_no_path() {
    let (fixture, case_id, _, _) = populated();
    let output = run(&[
        "case",
        "delete",
        "--archive",
        &fixture.root_text(),
        "--case",
        &case_id,
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(text.contains("Removed 5 record(s): association 1, case 1, import_event 0"));
    assert!(text.contains("Removed 0 object(s); 2 retained: purge_not_requested 2"));
    assert!(text.contains("No purge was requested, so no object was removed."));
    assert!(text.contains("does not erase data from the storage medium"));
    assert!(!text.contains('/'), "no path reaches human output");
    assert!(!text.contains("sha256"), "no digest reaches human output");
    for forbidden in ["delivered", "authentic", "legally effective", "verified"] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "no line claims something deletion cannot claim"
        );
    }
}

#[test]
fn deleting_one_case_leaves_every_other_case_exactly_as_it_was() {
    let fixture = Fixture::new();
    let digest = fixture.import("own.bin", FIRST);
    let kept = fixture.case("A local matter to keep");
    fixture.submission(&kept, &digest);
    let going = fixture.case("A local matter to delete");
    let before = snapshot(&fixture.root.join("records/cases"));

    assert!(fixture.delete(&going, true).status.success());
    let after = snapshot(&fixture.root.join("records/cases"));
    assert_eq!(after.len() + 1, before.len(), "one case document went");
    let listed = data(&["case", "list", "--archive", &fixture.root_text(), "--json"]);
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["cases"][0]["id"], kept);
    assert!(fixture.object(&digest).is_file());
    assert!(fixture.check().status.success());
}

/// Write an owner-only file, so that holding the lock is not itself a
/// permission refusal: the archive is owner-only and stays that way.
fn hold_owner_only(path: &Path) {
    fs::write(path, "{}\n").expect("write the file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .expect("narrow the file to owner-only");
    }
}

/// Every file under a root, by archive-relative path and byte length.
///
/// The snapshot never leaves the test process: it is compared with another
/// snapshot to prove that a refusal removed nothing, and never printed.
fn snapshot(root: &Path) -> Vec<(String, u64)> {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(listing) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let metadata = entry.metadata().expect("entry metadata");
            if metadata.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("a path inside the root")
                    .to_string_lossy()
                    .into_owned();
                entries.push((relative, metadata.len()));
            }
        }
    }
    entries.sort();
    entries
}
