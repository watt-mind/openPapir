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
    let stored = fixture
        .root
        .join("objects/sha256")
        .join(&digest[0..2])
        .join(&digest[2..4])
        .join(&digest);
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
    assert!(
        !destination.join("objects").join(&digest).exists(),
        "the partial copy is removed"
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
    assert_eq!(changed.len(), 7, "every kind is reported");
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
