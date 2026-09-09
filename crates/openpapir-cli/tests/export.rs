//! Contract tests for `case export` and `archive repair-permissions`.
//!
//! Every fixture here is synthetic and generated at test time from constants
//! in this file. Nothing is derived from real correspondence. The assertions
//! are about the observable contract in `docs/architecture.md` and
//! `docs/error-contract.md`: what the destination holds, what the manifest
//! lists, the codes and exit codes of every refusal, the promise that the
//! archive is not modified, the promise that the repair only narrows, and the
//! privacy rule.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const FIRST: &[u8] = b"first synthetic payload\n";
const SECOND: &[u8] = b"second synthetic payload\n";
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

/// One case with two submissions, one receipt, and one association.
struct Fixture {
    home: TempDir,
    root: PathBuf,
    case_id: String,
    digests: Vec<String>,
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
        for (name, payload) in [
            ("first.bin", FIRST),
            ("second.bin", SECOND),
            ("receipt.bin", RECEIPT),
        ] {
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
            &inputs[2],
            "--json",
        ]));
        let digests: Vec<String> = imported["data"]["artefacts"]
            .as_array()
            .expect("artefacts")
            .iter()
            .map(|artefact| artefact["digest"].as_str().expect("a digest").to_owned())
            .collect();

        let case = stdout_json(&run(&[
            "case",
            "create",
            "--archive",
            &root_text,
            "--title",
            "Synthetic matter",
            "--json",
        ]));
        let case_id = case["data"]["case"]["id"]
            .as_str()
            .expect("an id")
            .to_owned();

        let mut submissions = Vec::new();
        for (index, description) in ["First submission.", "Second submission."]
            .into_iter()
            .enumerate()
        {
            let added = stdout_json(&run(&[
                "submission",
                "add",
                "--archive",
                &root_text,
                "--case",
                &case_id,
                "--description",
                description,
                "--artefact",
                &digests[index],
                "--json",
            ]));
            submissions.push(
                added["data"]["submission"]["id"]
                    .as_str()
                    .expect("an id")
                    .to_owned(),
            );
        }

        let receipt = stdout_json(&run(&[
            "receipt",
            "add",
            "--archive",
            &root_text,
            "--artefact",
            &digests[2],
            "--json",
        ]));
        let receipt_id = receipt["data"]["receipt"]["id"]
            .as_str()
            .expect("an id")
            .to_owned();
        let candidate = format!("{}:strong:The reference matches.", submissions[0]);
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
            digests,
        }
    }

    fn export_to(&self, destination: &Path) -> Output {
        run(&[
            "case",
            "export",
            "--archive",
            &text(&self.root),
            "--case",
            &self.case_id,
            "--to",
            &text(destination),
            "--json",
        ])
    }

    fn destination(&self, name: &str) -> PathBuf {
        self.home.path().join(name)
    }

    /// One stored object's path inside the archive.
    ///
    /// Only the tests that damage a stored object need it, and those need a
    /// mode change, so they run on Unix alone.
    #[cfg(unix)]
    fn stored_object(&self, digest: &str) -> PathBuf {
        self.root
            .join("objects/sha256")
            .join(&digest[0..2])
            .join(&digest[2..4])
            .join(digest)
    }

    /// The bare hexadecimal form of an algorithm-qualified digest.
    fn bare(&self, index: usize) -> String {
        self.digests[index]
            .strip_prefix("sha256:")
            .expect("an algorithm-qualified digest")
            .to_owned()
    }
}

/// Every regular file under a root, by relative path, with its bytes.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).expect("read a directory") {
            let entry = entry.expect("a directory entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("a path under the root")
                    .to_string_lossy()
                    .into_owned();
                found.insert(relative, fs::read(&path).expect("read a file"));
            }
        }
    }
    found
}

#[test]
fn an_exported_case_round_trips_its_objects_records_and_manifest() {
    let fixture = Fixture::build();
    let destination = fixture.destination("export");
    let output = fixture.export_to(&destination);
    assert!(output.status.success(), "the export succeeds");
    assert!(output.stderr.is_empty(), "nothing shares stdout with JSON");
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "case.export");
    assert_eq!(envelope["verified"], false);
    let data = &envelope["data"];
    assert_eq!(data["case_id"], fixture.case_id.as_str());
    assert_eq!(data["object_count"], 3);
    assert_eq!(data["record_count"], 8);
    assert_eq!(
        data["bytes_copied"],
        (FIRST.len() + SECOND.len() + RECEIPT.len()) as u64
    );

    // Every copied object holds the original bytes under its digest's name.
    for (index, payload) in [FIRST, SECOND, RECEIPT].into_iter().enumerate() {
        let copy = destination.join("objects").join(fixture.bare(index));
        assert_eq!(fs::read(&copy).expect("read a copy"), payload);
    }

    // Every record is parseable JSON of the kind its directory names.
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(destination.join("manifest.json")).unwrap())
            .expect("the manifest is JSON");
    assert_eq!(manifest["archive_schema_version"], 1);
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["case_id"], fixture.case_id.as_str());
    let mut listed = Vec::new();
    for entry in manifest["records"].as_array().expect("records") {
        let kind = entry["kind"].as_str().expect("a kind");
        let id = entry["id"].as_str().expect("an id");
        let path = destination
            .join("records")
            .join(kind)
            .join(format!("{id}.json"));
        let record: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .expect("a record document is parseable JSON");
        assert_eq!(record["record_kind"], kind);
        assert_eq!(record["id"], id);
        listed.push(path);
    }
    let mut object_paths = Vec::new();
    for entry in manifest["objects"].as_array().expect("objects") {
        let digest = entry["digest"].as_str().expect("a digest");
        assert!(entry["byte_length"].is_u64());
        assert_eq!(entry["algorithm"], "sha256");
        object_paths.push(destination.join("objects").join(digest));
    }

    // The manifest lists everything the destination holds, and nothing else.
    let mut expected: Vec<PathBuf> = listed;
    expected.extend(object_paths);
    expected.push(destination.join("manifest.json"));
    expected.sort();
    let mut written: Vec<PathBuf> = snapshot(&destination)
        .keys()
        .map(|relative| destination.join(relative))
        .collect();
    written.sort();
    assert_eq!(written, expected, "the manifest is authoritative");
}

#[test]
fn the_archive_is_not_modified_by_an_export() {
    let fixture = Fixture::build();
    let before = snapshot(&fixture.root);
    assert!(
        fixture
            .export_to(&fixture.destination("export"))
            .status
            .success()
    );
    assert_eq!(snapshot(&fixture.root), before, "the archive is untouched");
}

#[test]
fn a_destination_inside_the_archive_is_refused() {
    let fixture = Fixture::build();
    let output = fixture.export_to(&fixture.root.join("export"));
    assert_eq!(output.status.code(), Some(2));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "usage.arguments");
    assert_eq!(envelope["error"]["details"]["argument"], "destination");
    assert!(
        !fixture.root.join("export").exists(),
        "nothing is created inside the archive"
    );
}

#[test]
fn a_destination_that_already_holds_anything_is_refused() {
    let fixture = Fixture::build();
    let destination = fixture.destination("export");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("stray.txt"), b"synthetic\n").unwrap();
    let output = fixture.export_to(&destination);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "export.destination_conflict");
    assert_eq!(envelope["error"]["details"]["bucket"], "export");
    assert_eq!(envelope["error"]["details"]["conflict_count"], 1);
    assert_eq!(
        fs::read(destination.join("stray.txt")).unwrap(),
        b"synthetic\n",
        "the existing file is left exactly as it was"
    );
}

#[test]
fn a_pre_existing_target_file_is_never_replaced() {
    let fixture = Fixture::build();
    let destination = fixture.destination("export");
    fs::create_dir_all(destination.join("objects")).unwrap();
    let target = destination.join("objects").join(fixture.bare(0));
    fs::write(&target, b"not the original bytes\n").unwrap();
    let output = fixture.export_to(&destination);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(
        stdout_json(&output)["error"]["code"],
        "export.destination_conflict"
    );
    assert_eq!(
        fs::read(&target).unwrap(),
        b"not the original bytes\n",
        "a file openPapir did not create is never replaced"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_destination_is_never_followed() {
    let fixture = Fixture::build();
    let elsewhere = fixture.destination("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let linked = fixture.destination("linked");
    std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();
    let output = fixture.export_to(&linked);
    assert_eq!(output.status.code(), Some(3));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "path.symlink");
    assert_eq!(envelope["error"]["details"]["scope"], "export_destination");
    assert_eq!(
        fs::read_dir(&elsewhere).unwrap().count(),
        0,
        "the link's target is never written to"
    );
}

#[cfg(unix)]
#[test]
fn a_corrupted_source_object_is_reported_as_a_copy_mismatch() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let digest = fixture.bare(0);
    let stored = fixture.stored_object(&digest);
    let original = fs::read(&stored).unwrap();
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&stored, b"corrupted synthetic bytes\n").unwrap();
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o400)).unwrap();

    let destination = fixture.destination("export");
    let output = fixture.export_to(&destination);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "export.copy_mismatch");
    assert_eq!(envelope["error"]["details"]["bucket"], "export");
    assert_eq!(envelope["error"]["details"]["digest"], digest.as_str());

    // A failed export leaves nothing behind, so the same command can be run
    // again once the archive is sound.
    assert!(
        !destination.exists(),
        "a destination the failed export created is removed again"
    );
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&stored, &original).unwrap();
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o400)).unwrap();
    let retried = fixture.export_to(&destination);
    assert!(retried.status.success(), "the retry is not refused");
    assert_eq!(
        fs::read(destination.join("objects").join(&digest)).unwrap(),
        original
    );
}

/// An interrupted object copy names the stage it was in. A stored object the
/// process cannot read stops the copy of that object, and the write bucket
/// names `object_write` for exactly that. Unix-only, because withholding read
/// access is a mode change.
#[cfg(unix)]
#[test]
fn an_interrupted_object_copy_reports_the_object_write_stage() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let stored = fixture.stored_object(&fixture.bare(0));
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o000))
        .expect("withhold access to the stored object");
    if fs::File::open(&stored).is_ok() {
        // The process can read the object anyway, which happens when the
        // tests run with privileges that ignore the permission bits.
        fs::set_permissions(&stored, fs::Permissions::from_mode(0o400)).unwrap();
        return;
    }
    let destination = fixture.destination("export");
    let output = fixture.export_to(&destination);
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o400)).expect("restore access");

    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "write.interrupted");
    assert_eq!(
        envelope["error"]["details"]["stage"], "object_write",
        "an interrupted object copy is never reported as a record write"
    );
    assert_eq!(envelope["error"]["details"]["bucket"], "write");
    assert!(
        !destination.exists(),
        "a destination the failed export created is removed again"
    );
}

/// A destination the export may read but not write to interrupts the copy at
/// the objects directory, which belongs to the object copy that asked for it.
#[cfg(unix)]
#[test]
fn a_destination_that_cannot_be_written_reports_the_stage_it_was_writing() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let destination = fixture.destination("read-only");
    fs::create_dir(&destination).unwrap();
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o500))
        .expect("withhold write access to the destination");
    if fs::create_dir(destination.join("probe")).is_ok() {
        // The process can write anyway, which happens when the tests run with
        // privileges that ignore the permission bits.
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let output = fixture.export_to(&destination);
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).expect("restore access");

    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "write.interrupted");
    assert_eq!(envelope["error"]["details"]["stage"], "object_write");
    assert_eq!(
        envelope["error"]["details"]["scope"], "export_destination",
        "a destination refusal stays a destination refusal"
    );
    assert!(
        envelope["error"]["details"].get("archive_path").is_none(),
        "a destination refusal never names an archive-relative path"
    );
}

/// A case with no objects at all reaches the record write first, so the same
/// unwritable destination reports `record_write` instead.
#[cfg(unix)]
#[test]
fn an_interrupted_record_write_reports_the_record_write_stage() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let empty = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        &text(&fixture.root),
        "--title",
        "A matter with no artefacts",
        "--json",
    ]));
    let case_id = empty["data"]["case"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let destination = fixture.destination("records-only");
    fs::create_dir(&destination).unwrap();
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o500))
        .expect("withhold write access to the destination");
    if fs::create_dir(destination.join("probe")).is_ok() {
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let output = run(&[
        "case",
        "export",
        "--archive",
        &text(&fixture.root),
        "--case",
        &case_id,
        "--to",
        &text(&destination),
        "--json",
    ]);
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).expect("restore access");

    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "write.interrupted");
    assert_eq!(envelope["error"]["details"]["stage"], "record_write");
}

#[cfg(unix)]
#[test]
fn a_failed_export_leaves_a_destination_the_user_made_empty() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let digest = fixture.bare(0);
    let stored = fixture.stored_object(&digest);
    fs::set_permissions(&stored, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&stored, b"corrupted synthetic bytes\n").unwrap();

    let destination = fixture.destination("export");
    fs::create_dir(&destination).unwrap();
    let output = fixture.export_to(&destination);
    assert_eq!(
        stdout_json(&output)["error"]["code"],
        "export.copy_mismatch"
    );
    assert!(
        destination.is_dir(),
        "a destination the user made is never removed"
    );
    assert_eq!(
        fs::read_dir(&destination).unwrap().count(),
        0,
        "everything the failed export wrote is gone"
    );
}

#[test]
fn an_export_never_echoes_a_path_or_a_filename_in_its_envelope() {
    let fixture = Fixture::build();
    let destination = fixture.destination("export");
    let output = fixture.export_to(&destination);
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(
        !rendered.contains(&text(&destination)),
        "the destination never reaches the envelope"
    );
    assert!(
        !rendered.contains(&text(&fixture.root)),
        "the archive root never reaches the envelope"
    );
    for name in ["first.bin", "second.bin", "receipt.bin"] {
        assert!(!rendered.contains(name), "no original filename is echoed");
    }
    for word in ["delivered", "authentic", "legally"] {
        assert!(!rendered.contains(word), "no claim beyond a byte copy");
    }
}

#[test]
fn human_export_output_repeats_the_destination_the_user_supplied() {
    let fixture = Fixture::build();
    let destination = fixture.destination("export");
    let output = run(&[
        "case",
        "export",
        "--archive",
        &text(&fixture.root),
        "--case",
        &fixture.case_id,
        "--to",
        &text(&destination),
    ]);
    assert!(output.status.success());
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(rendered.contains(&format!(
        "Exported case {} to {}.",
        fixture.case_id,
        text(&destination)
    )));
    assert!(rendered.contains(&format!(
        "Copied 3 object(s), {} byte(s), and wrote 8 record(s).",
        FIRST.len() + SECOND.len() + RECEIPT.len()
    )));
    assert!(rendered.contains("The archive was not changed."));
    assert!(!rendered.contains(&text(&fixture.root)), "no archive path");
}

#[test]
fn a_case_the_archive_does_not_hold_is_not_found() {
    let fixture = Fixture::build();
    let output = run(&[
        "case",
        "export",
        "--archive",
        &text(&fixture.root),
        "--case",
        "00000000000000000000000000000000",
        "--to",
        &text(&fixture.destination("export")),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(stdout_json(&output)["error"]["code"], "record.not_found");
    assert!(
        !fixture.destination("export").exists(),
        "no destination is created for a case that does not exist"
    );
}

#[test]
fn repairing_a_root_without_a_marker_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let output = run(&[
        "archive",
        "repair-permissions",
        "--archive",
        &text(home.path()),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "archive.marker_missing");
    assert!(
        !home.path().join("lock").exists(),
        "no lock is left behind by a refusal"
    );
}

#[cfg(unix)]
#[test]
fn the_repair_narrows_a_widened_archive_and_widens_nothing() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let root_text = text(&fixture.root);

    // Ordinary copy tooling widens permissions on restore.
    let mut widened = vec![fixture.root.clone()];
    let mut stack = vec![fixture.root.clone()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            widened.push(path);
        }
    }
    for path in &widened {
        let mode = if path.is_dir() { 0o777 } else { 0o666 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    let listed = run(&["case", "list", "--archive", &root_text, "--json"]);
    assert_eq!(
        stdout_json(&listed)["error"]["code"],
        "archive.permissions_wide",
        "a widened archive is refused before the repair"
    );

    let output = run(&[
        "archive",
        "repair-permissions",
        "--archive",
        &root_text,
        "--json",
    ]);
    assert!(output.status.success());
    let envelope = stdout_json(&output);
    assert_eq!(envelope["command"], "archive.repair_permissions");
    assert_eq!(envelope["verified"], false);
    let data = &envelope["data"];
    let changed: BTreeMap<String, u64> = data["changed"]
        .as_array()
        .expect("a changed array")
        .iter()
        .map(|entry| {
            (
                entry["kind"].as_str().expect("a kind").to_owned(),
                entry["count"].as_u64().expect("a count"),
            )
        })
        .collect();
    assert_eq!(changed.len(), 6, "every kind is reported");
    assert!(
        !changed.contains_key("lock"),
        "the lock is never inspected, so it is never reported"
    );
    assert_eq!(changed["root"], 1);
    assert_eq!(changed["marker"], 1);
    assert_eq!(changed["record"], 8);
    assert_eq!(changed["object"], 3);
    assert!(changed["directory"] >= 10);
    assert!(data["paths_changed"].as_u64().unwrap() > 0);
    assert!(data["paths_checked"].as_u64().unwrap() >= data["paths_changed"].as_u64().unwrap());
    assert!(
        !String::from_utf8(output.stdout.clone())
            .unwrap()
            .contains(&root_text),
        "the archive root never reaches the envelope"
    );

    // The design's modes, and only those.
    for path in &widened {
        let mode = fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777;
        let expected = if path.is_dir() {
            0o700
        } else if path.starts_with(fixture.root.join("objects")) {
            0o400
        } else {
            0o600
        };
        assert_eq!(mode, expected, "every path is narrowed to its own mode");
    }

    // A write after the repair succeeds, and a second repair changes nothing.
    let input = fixture.home.path().join("after.bin");
    fs::write(&input, b"another synthetic payload\n").unwrap();
    assert!(
        run(&["import", "--archive", &root_text, &text(&input), "--json",])
            .status
            .success(),
        "a repaired archive accepts a write again"
    );
    let again = stdout_json(&run(&[
        "archive",
        "repair-permissions",
        "--archive",
        &root_text,
        "--json",
    ]));
    assert_eq!(
        again["data"]["paths_changed"], 0,
        "a narrow archive is left alone"
    );
}

/// A repair that cannot read a fan-out directory of the object store reports
/// the kind it was inspecting, an object, rather than a record.
#[cfg(unix)]
#[test]
fn a_repair_of_an_unreadable_object_reports_the_object_write_stage() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let fan_out = fixture
        .stored_object(&fixture.bare(0))
        .parent()
        .expect("the fan-out directory")
        .to_path_buf();
    fs::set_permissions(&fan_out, fs::Permissions::from_mode(0o000))
        .expect("withhold access to the fan-out directory");
    if fs::read_dir(&fan_out).is_ok() {
        // The process can read the directory anyway, which happens when the
        // tests run with privileges that ignore the permission bits.
        fs::set_permissions(&fan_out, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let output = run(&[
        "archive",
        "repair-permissions",
        "--archive",
        &text(&fixture.root),
        "--json",
    ]);
    fs::set_permissions(&fan_out, fs::Permissions::from_mode(0o700)).expect("restore access");

    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["error"]["code"], "write.interrupted");
    assert_eq!(
        envelope["error"]["details"]["stage"], "object_write",
        "the repair reports the kind of path it was inspecting"
    );
    assert!(
        envelope["error"]["details"]["archive_path"]
            .as_str()
            .expect("an archive-relative path")
            .starts_with("objects/sha256"),
        "the path is archive-relative and names the object store"
    );
}

#[cfg(unix)]
#[test]
fn the_repair_never_widens_a_deliberately_narrower_path() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::build();
    let marker = fixture.root.join("papir-archive.json");
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o400)).unwrap();
    let output = run(&[
        "archive",
        "repair-permissions",
        "--archive",
        &text(&fixture.root),
        "--json",
    ]);
    assert!(output.status.success());
    assert_eq!(stdout_json(&output)["data"]["paths_changed"], 0);
    assert_eq!(
        fs::metadata(&marker).unwrap().permissions().mode() & 0o7777,
        0o400,
        "a narrower mode is kept rather than raised to the owner-only mode"
    );
}

#[test]
fn human_repair_output_reports_counts_and_no_path() {
    let fixture = Fixture::build();
    let output = run(&[
        "archive",
        "repair-permissions",
        "--archive",
        &text(&fixture.root),
    ]);
    assert!(output.status.success());
    let rendered = String::from_utf8(output.stdout).unwrap();
    assert!(rendered.contains("archive path(s) to owner-only."));
    assert!(rendered.contains("Permissions are only ever narrowed here"));
    assert!(
        !rendered.contains(&text(&fixture.root)),
        "no archive path reaches human output"
    );
}

/// The stage table of a document: each stage, and the set of paths it names.
///
/// The table is the one whose header is `Stage | What it names`, wherever it
/// sits and however it is indented, so a document may keep it inside a list.
/// Each cell is a sentence listing paths, so the paths are its comma-separated
/// clauses with the joining `and` and the closing full stop removed. Comparing
/// sets rather than sentences lets the two documents order their clauses
/// differently while still naming the same paths.
fn stage_table(markdown: &str) -> BTreeMap<String, Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut inside = false;
    for line in markdown.lines() {
        let line = line.trim();
        if line.starts_with("| Stage | What it names |") {
            assert!(!inside, "a document holds exactly one stage table");
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if !line.starts_with('|') {
            inside = false;
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        assert_eq!(cells.len(), 2, "the stage table has two columns");
        if cells[0].chars().all(|c| c == '-') {
            continue;
        }
        rows.push(vec![cells[0].to_owned(), cells[1].to_owned()]);
    }
    assert!(!rows.is_empty(), "the stage table was found");
    let mut table = BTreeMap::new();
    for row in rows {
        let stage = row[0].trim_matches('`').to_owned();
        let mut paths: Vec<String> = row[1]
            .trim_end_matches('.')
            .split(',')
            .map(|clause| {
                clause
                    .trim()
                    .trim_start_matches("and ")
                    .trim()
                    .to_lowercase()
            })
            .collect();
        paths.sort();
        assert!(
            table.insert(stage, paths).is_none(),
            "each stage appears once"
        );
    }
    table
}

fn doc(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs")
        .join(name);
    fs::read_to_string(&path).expect("the document is readable")
}

#[test]
fn both_documents_name_the_same_paths_for_each_write_stage() {
    let contract = stage_table(&doc("error-contract.md"));
    let architecture = stage_table(&doc("architecture.md"));
    assert_eq!(
        contract, architecture,
        "docs/error-contract.md and docs/architecture.md must list the same \
         paths for each write stage"
    );
    let documented: Vec<&str> = contract.keys().map(String::as_str).collect();
    let mut implemented: Vec<&str> = openpapir_core::export::repair::Stage::ALL
        .iter()
        .map(|stage| stage.as_str())
        .collect();
    implemented.sort_unstable();
    assert_eq!(
        documented, implemented,
        "the tables name exactly the stages the implementation can report"
    );
}

#[test]
fn every_kind_the_repair_walks_maps_to_a_documented_stage() {
    use openpapir_core::export::repair::{KINDS, Kind, Stage};

    let stages: Vec<&str> = Stage::ALL.iter().map(|stage| stage.as_str()).collect();
    for kind in [
        Kind::Cache,
        Kind::Directory,
        Kind::Marker,
        Kind::Object,
        Kind::Record,
        Kind::Root,
        Kind::Staging,
    ] {
        assert!(
            stages.contains(&kind.stage().as_str()),
            "{} maps to a stage the write bucket names",
            kind.name()
        );
    }
    assert_eq!(
        KINDS,
        ["cache", "directory", "marker", "object", "record", "root"],
        "the report's kind names and their order do not change"
    );
}
