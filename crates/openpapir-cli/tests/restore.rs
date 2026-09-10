//! Contract tests for `case import`, the other direction of `case export`.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence. The assertions
//! are about the observable contract in `docs/architecture.md` and
//! `docs/error-contract.md`: the round trip through an export, the codes and
//! exit codes of every refusal, the promise that a refused import leaves the
//! archive exactly as it was, the idempotence of a second import, and the
//! privacy rule.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const FIRST: &[u8] = b"first synthetic payload\n";
const RECEIPT: &[u8] = b"synthetic receipt payload\n";

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

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The code one refused envelope carries.
fn code(output: &Output) -> String {
    stdout_json(output)["error"]["code"]
        .as_str()
        .expect("a refusal names its code")
        .to_owned()
}

/// One archive holding one case with a submission, a receipt, and an
/// association over two stored objects.
struct Fixture {
    home: TempDir,
    root: PathBuf,
    case_id: String,
    receipt_id: String,
}

impl Fixture {
    fn build() -> Self {
        let home = tempfile::tempdir().expect("a temporary directory");
        let root = home.path().join("archive");
        fs::create_dir(&root).expect("create the archive root");
        let root_text = text(&root);
        assert!(
            run(&["archive", "init", &root_text, "--json"])
                .status
                .success()
        );

        let mut inputs = Vec::new();
        for (name, payload) in [("first.bin", FIRST), ("receipt.bin", RECEIPT)] {
            let path = home.path().join(name);
            fs::write(&path, payload).expect("write a synthetic payload");
            inputs.push(text(&path));
        }
        let imported = stdout_json(&run(&[
            "import",
            "--archive",
            &root_text,
            &inputs[0],
            &inputs[1],
            "--json",
        ]));
        let digests: Vec<String> = imported["data"]["artefacts"]
            .as_array()
            .expect("artefacts")
            .iter()
            .map(|artefact| artefact["digest"].as_str().expect("a digest").to_owned())
            .collect();

        // The case carries every field a case record can carry, the ones a
        // later build added included, so the round trip has to preserve them
        // rather than quietly dropping what it does not know about.
        let case_id = stdout_json(&run(&[
            "case",
            "create",
            "--archive",
            &root_text,
            "--title",
            "Synthetic matter",
            "--tag",
            "tax",
            "--tag",
            "office",
            "--json",
        ]))["data"]["case"]["id"]
            .as_str()
            .expect("an id")
            .to_owned();
        // `case update` is what writes `updated_at`, so the case has one.
        assert!(
            run(&[
                "case",
                "update",
                "--archive",
                &root_text,
                &case_id,
                "--status",
                "closed",
                "--json",
            ])
            .status
            .success()
        );
        let submission_id = stdout_json(&run(&[
            "submission",
            "add",
            "--archive",
            &root_text,
            "--case",
            &case_id,
            "--description",
            "First submission.",
            "--artefact",
            &digests[0],
            "--json",
        ]))["data"]["submission"]["id"]
            .as_str()
            .expect("an id")
            .to_owned();
        let receipt_id = stdout_json(&run(&[
            "receipt",
            "add",
            "--archive",
            &root_text,
            "--artefact",
            &digests[1],
            "--json",
        ]))["data"]["receipt"]["id"]
            .as_str()
            .expect("an id")
            .to_owned();
        let candidate = format!("{submission_id}:strong:The reference matches.");
        assert!(
            run(&[
                "association",
                "create",
                "--archive",
                &root_text,
                "--receipt",
                &receipt_id,
                "--outcome",
                "associated",
                "--candidate",
                &candidate,
                "--json",
            ])
            .status
            .success()
        );

        Self {
            home,
            root,
            case_id,
            receipt_id,
        }
    }

    /// Export the case into a directory of this fixture's own home.
    fn export(&self, name: &str) -> PathBuf {
        let destination = self.home.path().join(name);
        assert!(
            run(&[
                "case",
                "export",
                "--archive",
                &text(&self.root),
                "--case",
                &self.case_id,
                "--to",
                &text(&destination),
                "--json",
            ])
            .status
            .success(),
            "the fixture exports its own case"
        );
        destination
    }

    fn import_from(&self, source: &Path) -> Output {
        self.import_into(&self.root, source)
    }

    fn import_into(&self, root: &Path, source: &Path) -> Output {
        run(&[
            "case",
            "import",
            "--archive",
            &text(root),
            "--from",
            &text(source),
            "--json",
        ])
    }

    fn delete_with_purge(&self) {
        assert!(
            run(&[
                "case",
                "delete",
                "--archive",
                &text(&self.root),
                "--case",
                &self.case_id,
                "--purge",
                "--json",
            ])
            .status
            .success()
        );
    }

    fn show(&self) -> Value {
        stdout_json(&run(&[
            "case",
            "show",
            "--archive",
            &text(&self.root),
            &self.case_id,
            "--json",
        ]))
    }

    fn associations(&self) -> Value {
        stdout_json(&run(&[
            "association",
            "list",
            "--archive",
            &text(&self.root),
            "--receipt",
            &self.receipt_id,
            "--json",
        ]))
    }

    fn check(&self) -> Output {
        run(&["archive", "check", "--archive", &text(&self.root), "--json"])
    }

    /// Every import event the archive holds, in identifier order.
    fn import_events(&self) -> Vec<Value> {
        let mut events: Vec<Value> = fs::read_dir(self.root.join("records/imports"))
            .expect("the imports directory")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.extension().is_some_and(|suffix| suffix == "json"))
            .map(|path| {
                serde_json::from_str(&fs::read_to_string(&path).expect("read an event"))
                    .expect("an event document")
            })
            .collect();
        events.sort_by_key(|event: &Value| event["id"].as_str().expect("an identifier").to_owned());
        events
    }

    /// A second archive, to import into as another machine would.
    fn other_archive(&self) -> PathBuf {
        let root = self.home.path().join("other");
        fs::create_dir(&root).expect("create the second archive root");
        assert!(
            run(&["archive", "init", &text(&root), "--json"])
                .status
                .success()
        );
        root
    }
}

/// Write one owner-only file, as the archive's own writes leave it.
fn hold_owner_only(path: &Path) {
    fs::write(path, "{}\n").expect("write the file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .expect("narrow the file to owner-only");
    }
}

/// One record document inside an export.
fn exported_record(source: &Path, kind: &str, id: &str) -> PathBuf {
    source.join("records").join(kind).join(format!("{id}.json"))
}

/// The bytes of every file in a directory tree, by relative path.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(bytes) = fs::read(&path) {
                let relative = path
                    .strip_prefix(root)
                    .expect("a path under the root")
                    .to_string_lossy()
                    .into_owned();
                found.push((relative, bytes));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_case_survives_an_export_a_purging_deletion_and_an_import() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    let before_show = fixture.show();
    let before_associations = fixture.associations();
    let before_events = fixture.import_events().len();

    fixture.delete_with_purge();
    let imported = stdout_json(&fixture.import_from(&source));
    assert_eq!(imported["ok"], true);
    assert_eq!(imported["command"], "case.import");
    assert_eq!(imported["verified"], false);
    assert_eq!(imported["data"]["case_id"], fixture.case_id);
    assert_eq!(imported["data"]["objects_stored"], 2);
    assert_eq!(imported["data"]["objects_present"], 0);
    assert_eq!(imported["data"]["records_written"], 6);
    assert_eq!(imported["data"]["records_present"], 0);
    assert_eq!(imported["data"]["events_recorded"], 2);

    let check = fixture.check();
    assert_eq!(check.status.code(), Some(0), "the archive is clean again");
    assert_eq!(
        stdout_json(&check)["data"]["problems"]
            .as_array()
            .expect("the problem counts")
            .iter()
            .map(|problem| problem["count"].as_u64().expect("a count"))
            .sum::<u64>(),
        0
    );
    assert_eq!(fixture.show(), before_show, "the case is the case again");
    assert_eq!(fixture.associations(), before_associations);
    let case = &fixture.show()["data"]["case"];
    assert_eq!(case["status"], "closed", "a status survives the round trip");
    assert_eq!(
        case["tags"],
        serde_json::json!(["office", "tax"]),
        "the user's own tags survive the round trip"
    );
    assert!(
        case["updated_at"].is_string(),
        "the record's own update time survives the round trip"
    );

    let events = fixture.import_events();
    assert_eq!(
        events.len(),
        before_events + 2,
        "the restored events, plus one new event per stored object"
    );
    let restored: Vec<&Value> = events
        .iter()
        .filter(|event| event["source"] == "export")
        .collect();
    assert_eq!(restored.len(), 2, "each stored object gets its own event");
    for event in restored {
        assert_eq!(event["original_filename"], "");
        assert_eq!(event["record_kind"], "import_event");
        assert_eq!(event["created_object"], true);
    }
    assert!(
        events
            .iter()
            .any(|event| event["source"].is_null() && event["original_filename"] == "first.bin"),
        "the user's own import event survives the round trip"
    );
}

#[test]
fn a_second_import_of_one_export_changes_nothing() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    fixture.delete_with_purge();
    assert!(fixture.import_from(&source).status.success());
    let after_first = tree(&fixture.root);

    let again = stdout_json(&fixture.import_from(&source));
    assert_eq!(again["ok"], true);
    assert_eq!(again["data"]["objects_stored"], 0);
    assert_eq!(again["data"]["objects_present"], 2);
    assert_eq!(again["data"]["records_written"], 0);
    assert_eq!(again["data"]["records_present"], 6);
    assert_eq!(again["data"]["events_recorded"], 0);
    assert_eq!(
        tree(&fixture.root),
        after_first,
        "a second import writes no record and no object"
    );
    assert_eq!(fixture.check().status.code(), Some(0));
}

#[test]
fn an_export_moves_a_case_into_another_archive() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    let other = fixture.other_archive();
    let imported = stdout_json(&fixture.import_into(&other, &source));
    assert_eq!(imported["ok"], true);
    assert_eq!(imported["data"]["objects_stored"], 2);
    assert_eq!(imported["data"]["records_written"], 6);
    let shown = stdout_json(&run(&[
        "case",
        "show",
        "--archive",
        &text(&other),
        &fixture.case_id,
        "--json",
    ]));
    assert_eq!(shown["data"]["case"]["title"], "Synthetic matter");
    assert_eq!(
        run(&["archive", "check", "--archive", &text(&other), "--json"])
            .status
            .code(),
        Some(0)
    );
}

#[test]
fn an_export_the_manifest_does_not_describe_is_refused_and_changes_nothing() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    fixture.delete_with_purge();
    let before = tree(&fixture.root);
    let manifest = source.join("manifest.json");

    let empty = fixture.home.path().join("empty");
    fs::create_dir(&empty).expect("create an empty directory");
    let refused = fixture.import_from(&empty);
    assert_eq!(code(&refused), "export.manifest_missing");
    assert_eq!(refused.status.code(), Some(4));

    fs::write(&manifest, b"{ not a manifest").expect("damage the manifest");
    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "export.manifest_malformed");
    assert_eq!(refused.status.code(), Some(4));

    let newer = format!(
        "{{\"archive_schema_version\":2,\"case_id\":\"{}\",\
         \"exported_at\":\"2026-01-17T12:00:00Z\",\"objects\":[],\"records\":[],\
         \"schema_version\":1}}\n",
        fixture.case_id
    );
    fs::write(&manifest, newer).expect("write a newer manifest");
    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "archive.schema_newer");

    assert_eq!(tree(&fixture.root), before, "the archive is unchanged");
}

#[test]
fn a_record_the_manifest_names_and_the_export_lacks_is_refused() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    fixture.delete_with_purge();
    let before = tree(&fixture.root);
    fs::remove_file(exported_record(&source, "case", &fixture.case_id))
        .expect("remove the exported case record");

    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "export.record_missing");
    assert_eq!(refused.status.code(), Some(4));
    let json = stdout_json(&refused);
    assert_eq!(json["error"]["details"]["record_kind"], "case");
    assert_eq!(json["error"]["details"]["scope"], "export_source");
    assert_eq!(json["data"], serde_json::json!({}));
    assert_eq!(tree(&fixture.root), before, "the archive is unchanged");
}

#[test]
fn an_identifier_a_different_record_holds_is_refused_before_any_write() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    let before = tree(&fixture.root);
    let record = exported_record(&source, "case", &fixture.case_id);
    let document = fs::read_to_string(&record).expect("read the exported case");
    fs::write(
        &record,
        document.replace("Synthetic matter", "Different matter"),
    )
    .expect("rewrite the exported case");

    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "export.record_conflict");
    assert_eq!(refused.status.code(), Some(4));
    let json = stdout_json(&refused);
    assert_eq!(json["error"]["details"]["record_kind"], "case");
    assert_eq!(json["error"]["details"]["conflict_count"], 1);
    assert_eq!(tree(&fixture.root), before, "the archive is unchanged");
    assert_eq!(
        fixture.show()["data"]["case"]["title"],
        "Synthetic matter",
        "the stored record is never edited"
    );
}

#[test]
fn an_export_source_that_cannot_hold_an_export_is_a_usage_refusal() {
    let fixture = Fixture::build();
    let absent = fixture.home.path().join("absent");
    let refused = fixture.import_from(&absent);
    assert_eq!(code(&refused), "usage.arguments");
    assert_eq!(refused.status.code(), Some(2));
    let rendered = String::from_utf8(refused.stdout.clone()).expect("stdout is UTF-8");
    assert!(
        !rendered.contains("absent"),
        "a user-supplied path never reaches the envelope"
    );

    let inside = fixture.root.join("cache");
    assert_eq!(code(&fixture.import_from(&inside)), "usage.arguments");
}

#[test]
fn the_human_form_echoes_the_source_and_the_json_form_never_does() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    fixture.delete_with_purge();
    let human = run(&[
        "case",
        "import",
        "--archive",
        &text(&fixture.root),
        "--from",
        &text(&source),
    ]);
    assert!(human.status.success());
    let lines = String::from_utf8(human.stdout).expect("stdout is UTF-8");
    assert!(
        lines.contains(&text(&source)),
        "the argument is echoed back"
    );
    assert!(lines.contains("Recorded 2 import event(s) with source export."));
    assert!(
        lines.contains("never authenticity, delivery, or legal effect."),
        "nothing is presented as verified"
    );
    assert!(
        String::from_utf8(human.stderr)
            .expect("stderr is UTF-8")
            .is_empty()
            || !lines.is_empty()
    );

    let source = fixture.export("second");
    let json = String::from_utf8(fixture.import_from(&source).stdout).expect("stdout is UTF-8");
    assert!(
        !json.contains(&text(&source)),
        "no JSON field carries a user-supplied path"
    );
}

/// Damaging an exported copy needs a mode change, because `case export`
/// leaves every copy read-only.
#[cfg(unix)]
#[test]
fn an_exported_copy_whose_bytes_changed_is_refused_and_changes_nothing() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::build();
    let source = fixture.export("out");
    fixture.delete_with_purge();
    let before = tree(&fixture.root);
    let objects = source.join("objects");
    let copy = fs::read_dir(&objects)
        .expect("the exported objects")
        .map(|entry| entry.expect("an entry").path())
        .next()
        .expect("at least one copy");
    fs::set_permissions(&copy, fs::Permissions::from_mode(0o600)).expect("open the copy");
    let mut damaged = fs::read(&copy).expect("read the copy");
    damaged[0] = damaged[0].wrapping_add(1);
    fs::write(&copy, &damaged).expect("damage the copy");

    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "export.object_mismatch");
    assert_eq!(refused.status.code(), Some(4));
    let json = stdout_json(&refused);
    assert_eq!(json["error"]["details"]["reason"], "digest");
    assert_eq!(json["error"]["details"]["scope"], "export_source");
    assert_eq!(tree(&fixture.root), before, "the archive is unchanged");

    fs::remove_file(&copy).expect("remove the copy");
    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "export.object_mismatch");
    assert_eq!(
        stdout_json(&refused)["error"]["details"]["reason"],
        "absent"
    );
    assert_eq!(tree(&fixture.root), before, "the archive is unchanged");
}

/// The digests the export's manifest lists, in the order it lists them.
///
/// Only the cases that plant something at an object's own path need it, and
/// those need a mode change or a named pipe, so they run on Unix alone.
#[cfg(unix)]
fn manifest_digests(source: &Path) -> Vec<String> {
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(source.join("manifest.json")).expect("read it"))
            .expect("the manifest is JSON");
    manifest["objects"]
        .as_array()
        .expect("the manifest lists its objects")
        .iter()
        .map(|object| object["digest"].as_str().expect("a digest").to_owned())
        .collect()
}

/// An object pass that stops part way leaves nothing of itself behind, so the
/// archive is not left holding an orphan the integrity check would report.
///
/// The refusal is induced with a fan-out directory that is wider than
/// owner-only, which the store refuses before it publishes into it. That is a
/// mode change, so the case runs on Unix.
#[cfg(unix)]
#[test]
fn an_import_that_cannot_store_every_object_removes_the_ones_it_stored() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::build();
    let source = fixture.export("out");
    let other = fixture.other_archive();
    let digests = manifest_digests(&source);
    assert_eq!(digests.len(), 2, "the fixture exports two objects");
    assert_ne!(
        digests[0][0..2],
        digests[1][0..2],
        "the two objects fan out into different directories"
    );
    let wide = other.join("objects/sha256").join(&digests[1][0..2]);
    fs::create_dir_all(&wide).expect("create the fan-out directory");
    fs::set_permissions(&wide, fs::Permissions::from_mode(0o755)).expect("widen it");

    let refused = fixture.import_into(&other, &source);
    fs::set_permissions(&wide, fs::Permissions::from_mode(0o700)).expect("narrow it again");
    assert_eq!(code(&refused), "archive.permissions_wide");
    assert_eq!(refused.status.code(), Some(4));

    let check = run(&["archive", "check", "--archive", &text(&other), "--json"]);
    assert_eq!(check.status.code(), Some(0), "the archive is still clean");
    let report = stdout_json(&check);
    assert_eq!(
        report["data"]["objects_checked"], 0,
        "the object stored before the refusal was removed again"
    );
    assert_eq!(report["data"]["records_checked"], 0);
    assert_eq!(
        report["data"]["problems"]
            .as_array()
            .expect("the problem counts")
            .iter()
            .map(|problem| problem["count"].as_u64().expect("a count"))
            .sum::<u64>(),
        0
    );
}

/// A named pipe planted at an object's path would hold a blocking open open
/// for ever. The import opens without waiting and refuses it on its kind.
#[cfg(unix)]
#[test]
fn a_named_pipe_in_the_export_is_refused_rather_than_waited_on() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    let digests = manifest_digests(&source);
    let copy = source.join("objects").join(&digests[0]);
    fs::remove_file(&copy).expect("remove the copy");
    let made = Command::new("mkfifo")
        .arg(&copy)
        .status()
        .is_ok_and(|status| status.success());
    if !made {
        eprintln!("skipped the named-pipe case: this system has no usable mkfifo command");
        return;
    }
    let refused = fixture.import_into(&fixture.other_archive(), &source);
    assert_eq!(code(&refused), "export.object_mismatch");
    assert_eq!(refused.status.code(), Some(4));
    assert_eq!(
        stdout_json(&refused)["error"]["details"]["reason"],
        "unusable"
    );
}

/// A symbolic link anywhere in the export is refused rather than followed,
/// and every one of them is refused the same way.
#[cfg(unix)]
#[test]
fn a_linked_path_in_the_export_is_refused_wherever_it_is() {
    let fixture = Fixture::build();
    let elsewhere = fixture.home.path().join("elsewhere");
    fs::write(&elsewhere, b"not an export\n").expect("write the target");
    // One target archive serves both cases: each refusal comes before
    // anything is written, so the archive it was pointed at is untouched.
    let other = fixture.other_archive();

    for (name, relative) in [
        ("linked-manifest", "manifest.json".to_owned()),
        (
            "linked-record",
            format!("records/case/{}.json", fixture.case_id),
        ),
    ] {
        let source = fixture.export(name);
        let path = source.join(&relative);
        fs::remove_file(&path).expect("remove the real file");
        std::os::unix::fs::symlink(&elsewhere, &path).expect("plant the link");
        let refused = fixture.import_into(&other, &source);
        assert_eq!(code(&refused), "path.symlink", "refused: {relative}");
        assert_eq!(refused.status.code(), Some(3));
        assert_eq!(
            stdout_json(&refused)["error"]["details"]["scope"],
            "export_source"
        );
    }
}

/// A second writer is refused, because an import writes under the lock.
#[test]
fn an_import_needs_the_writer_lock() {
    let fixture = Fixture::build();
    let source = fixture.export("out");
    fixture.delete_with_purge();
    hold_owner_only(&fixture.root.join("lock"));
    let refused = fixture.import_from(&source);
    assert_eq!(code(&refused), "lock.held");
    assert_eq!(refused.status.code(), Some(4));
    fs::remove_file(fixture.root.join("lock")).expect("release the lock");
    assert!(fixture.import_from(&source).status.success());
}

/// A second case, and a retired association chain, so the whole-archive round
/// trip carries more than one case and a history that supersedes itself.
///
/// The chain is the demanding part: a retirement is a new record that
/// supersedes the live one, so an archive that restored only the head would
/// lose what the user withdrew.
fn build_second_case_and_retire(fixture: &Fixture) -> String {
    let root_text = text(&fixture.root);
    let payload = fixture.home.path().join("second.bin");
    fs::write(&payload, b"second synthetic payload\n").expect("write a synthetic payload");
    let digest = stdout_json(&run(&[
        "import",
        "--archive",
        &root_text,
        &text(&payload),
        "--json",
    ]))["data"]["artefacts"][0]["digest"]
        .as_str()
        .expect("a digest")
        .to_owned();
    let second = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        &root_text,
        "--title",
        "Second synthetic matter",
        "--notes",
        "Kept open.",
        "--json",
    ]))["data"]["case"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    assert!(
        run(&[
            "submission",
            "add",
            "--archive",
            &root_text,
            "--case",
            &second,
            "--description",
            "Second submission.",
            "--date",
            "2026-01-13",
            "--artefact",
            &digest,
            "--json",
        ])
        .status
        .success()
    );
    let live = stdout_json(&run(&[
        "association",
        "list",
        "--archive",
        &root_text,
        "--receipt",
        &fixture.receipt_id,
        "--json",
    ]))["data"]["associations"][0]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    assert!(
        run(&[
            "association",
            "retire",
            "--archive",
            &root_text,
            &live,
            "--reason",
            "The reference named another case.",
            "--json",
        ])
        .status
        .success()
    );
    second
}

/// Export a whole archive into a directory of this fixture's own home.
fn export_whole(fixture: &Fixture, name: &str) -> PathBuf {
    let destination = fixture.home.path().join(name);
    let exported = run(&[
        "archive",
        "export",
        "--archive",
        &text(&fixture.root),
        "--to",
        &text(&destination),
        "--json",
    ]);
    assert!(exported.status.success(), "the fixture exports its archive");
    destination
}

fn import_whole(root: &Path, source: &Path) -> Output {
    run(&[
        "archive",
        "import",
        "--archive",
        &text(root),
        "--from",
        &text(source),
        "--json",
    ])
}

/// One listing of everything a comparison of two archives may look at.
fn listings(root: &Path) -> Value {
    let root_text = text(root);
    serde_json::json!({
        "cases": stdout_json(&run(&["case", "list", "--archive", &root_text, "--json"]))["data"],
        "receipts": stdout_json(&run(&["receipt", "list", "--archive", &root_text, "--json"]))
            ["data"],
    })
}

/// The whole archive survives an export into a fresh archive on another
/// machine: every case, the retired chain included, with a clean check and
/// the same listings on both sides.
#[test]
fn a_whole_archive_survives_an_export_into_a_fresh_archive() {
    let fixture = Fixture::build();
    let second = build_second_case_and_retire(&fixture);
    let before = listings(&fixture.root);
    let history = fixture.associations();
    let source = export_whole(&fixture, "whole");

    let other = fixture.other_archive();
    let restored = import_whole(&other, &source);
    assert!(restored.status.success());
    let data = &stdout_json(&restored)["data"];
    assert_eq!(data["case_count"], 2);
    assert_eq!(data["records_present"], 0);
    assert_eq!(data["objects_present"], 0);
    assert!(
        data["records_written"].as_u64().expect("a count") > 0,
        "a fresh archive holds none of the export yet"
    );

    let checked = run(&["archive", "check", "--archive", &text(&other), "--json"]);
    assert!(checked.status.success(), "the restored archive is clean");
    let report = &stdout_json(&checked)["data"];
    assert_eq!(report["orphan_objects"], 0);
    assert_eq!(report["objects_unchecked"], 0);

    assert_eq!(listings(&other), before, "every listing survives the trip");
    assert_eq!(
        stdout_json(&run(&[
            "association",
            "list",
            "--archive",
            &text(&other),
            "--receipt",
            &fixture.receipt_id,
            "--json",
        ]))["data"],
        history["data"],
        "the retired chain is restored whole, not only its head"
    );
    assert!(
        run(&[
            "case",
            "show",
            "--archive",
            &text(&other),
            &second,
            "--json"
        ])
        .status
        .success()
    );

    // Only the import events differ: the restore records one of its own per
    // object it stored, over and above the events the export carried.
    let events = fs::read_dir(other.join("records/imports"))
        .expect("the imports directory")
        .count();
    assert_eq!(
        events,
        fixture.import_events().len() + data["objects_stored"].as_u64().expect("a count") as usize
    );
}

/// A second import of one whole-archive export writes nothing at all.
#[test]
fn importing_one_whole_archive_export_twice_leaves_the_same_archive() {
    let fixture = Fixture::build();
    build_second_case_and_retire(&fixture);
    let source = export_whole(&fixture, "whole");
    let other = fixture.other_archive();
    assert!(import_whole(&other, &source).status.success());
    let after_first = tree(&other);

    let again = import_whole(&other, &source);
    assert!(again.status.success());
    let data = &stdout_json(&again)["data"];
    assert_eq!(data["records_written"], 0);
    assert_eq!(data["objects_stored"], 0);
    assert_eq!(data["events_recorded"], 0);
    assert_eq!(tree(&other), after_first, "nothing was written again");
}

/// Each import reads its own export and refuses the other's, so what a record
/// identifier already in the archive means never depends on which of the two
/// directories was handed over.
#[test]
fn neither_import_reads_the_other_scope_of_export() {
    let fixture = Fixture::build();
    let whole = export_whole(&fixture, "whole");
    let one = fixture.export("one");
    let other = fixture.other_archive();

    let refused = fixture.import_into(&other, &whole);
    assert_eq!(code(&refused), "export.manifest_malformed");
    assert_eq!(refused.status.code(), Some(4));
    assert!(
        String::from_utf8(refused.stdout.clone())
            .expect("stdout is UTF-8")
            .contains("whole archive"),
        "the refusal says which of the two the directory holds"
    );

    let refused = import_whole(&other, &one);
    assert_eq!(code(&refused), "export.manifest_malformed");
    assert_eq!(refused.status.code(), Some(4));
}

/// A record identifier a different record already holds refuses the whole
/// import, before anything is written.
#[test]
fn a_record_conflict_refuses_the_whole_archive_import() {
    let fixture = Fixture::build();
    build_second_case_and_retire(&fixture);
    let source = export_whole(&fixture, "whole");
    let other = fixture.other_archive();

    // The same identifier, holding a different case record.
    let planted = other
        .join("records/cases")
        .join(format!("{}.json", fixture.case_id));
    let mut document: Value = serde_json::from_str(
        &fs::read_to_string(exported_record(&source, "case", &fixture.case_id))
            .expect("read the exported case"),
    )
    .expect("a case document");
    document["title"] = Value::String("Another matter entirely".to_owned());
    fs::create_dir_all(planted.parent().expect("a parent")).expect("create the directory");
    fs::write(&planted, format!("{document}\n")).expect("plant the conflicting record");
    let before = tree(&other);

    let refused = import_whole(&other, &source);
    assert_eq!(code(&refused), "export.record_conflict");
    assert_eq!(refused.status.code(), Some(4));
    assert_eq!(
        stdout_json(&refused)["error"]["details"]["record_kind"],
        "case"
    );
    assert_eq!(tree(&other), before, "a refused import writes nothing");
}

/// The restore ceiling of `docs/architecture.md`, and the object count that
/// crosses it at the single-file cap: 257 objects of 64 MiB come to 16.06
/// GiB, which is the smallest whole number of full-sized objects above it.
const MAX_RESTORE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const FULL_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
const OBJECTS_OVER_THE_CEILING: u64 = 257;

/// Replace the manifest's object rows with rows that come to more than the
/// restore ceiling.
///
/// Nothing is written into the export beside the manifest, and no copy is
/// planted: the ceiling is checked from the sum the manifest names, before a
/// single copy is opened, which is exactly what these cases assert. Every
/// row stays inside the single-file cap, so the only cap the sum crosses is
/// the restore one.
fn overstate_the_objects(source: &Path) -> u64 {
    let path = source.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the manifest"))
            .expect("the manifest is JSON");
    let objects: Vec<Value> = (0..OBJECTS_OVER_THE_CEILING)
        .map(|index| {
            serde_json::json!({
                "algorithm": "sha256",
                "byte_length": FULL_OBJECT_BYTES,
                "digest": format!("{index:064x}"),
            })
        })
        .collect();
    manifest["objects"] = Value::Array(objects);
    fs::write(
        &path,
        serde_json::to_string(&manifest).expect("serialise it"),
    )
    .expect("write the manifest back");
    OBJECTS_OVER_THE_CEILING * FULL_OBJECT_BYTES
}

/// Both restores are bounded by `input.cap.restore_bytes` rather than by the
/// per-operation import cap, the refusal keeps the shape every cap refusal
/// has, and it comes before anything is written.
///
/// One target archive serves both cases, because a refused restore writes
/// nothing: it is still empty when the second one runs.
#[test]
fn a_restore_over_its_own_ceiling_is_refused_before_anything_is_written() {
    let fixture = Fixture::build();
    let other = fixture.other_archive();
    let case_source = fixture.export("case-out");
    let archive_source = export_whole(&fixture, "archive-out");
    let observed = OBJECTS_OVER_THE_CEILING * FULL_OBJECT_BYTES;
    assert!(observed > MAX_RESTORE_BYTES);

    for source in [&case_source, &archive_source] {
        assert_eq!(overstate_the_objects(source), observed);
        let output = if source == &case_source {
            fixture.import_into(&other, source)
        } else {
            import_whole(&other, source)
        };

        assert_eq!(code(&output), "input.cap.restore_bytes");
        assert_eq!(output.status.code(), Some(3), "an input refusal exits 3");
        let error = &stdout_json(&output)["error"];
        let details = error["details"]
            .as_object()
            .expect("a refusal carries its details")
            .clone();
        assert_eq!(details["bucket"], "input");
        assert_eq!(details["cap_bytes"], MAX_RESTORE_BYTES);
        assert_eq!(details["observed_bytes"], observed);
        assert_eq!(
            details.len(),
            3,
            "the sum is over the whole manifest, so no position is named"
        );
        assert!(
            !serde_json::to_string(error)
                .expect("the refusal serialises")
                .contains("out"),
            "no user-supplied path reaches the envelope"
        );

        let check = run(&["archive", "check", "--archive", &text(&other), "--json"]);
        assert_eq!(check.status.code(), Some(0));
        let report = stdout_json(&check);
        assert_eq!(report["data"]["objects_checked"], 0);
        assert_eq!(report["data"]["records_checked"], 0);
    }
}
