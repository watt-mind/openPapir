//! Golden output tests: the CLI contract, pinned byte for byte.
//!
//! Every case below builds a synthetic archive from constants in this file,
//! runs one invocation twice (once with `--json`, once without), normalises
//! the values that legitimately move between runs, and compares the result
//! with the files committed under `tests/golden/`. A difference is a change
//! to what every consumer parses, not a test to be repaired; the contract and
//! the deliberate regeneration path are in `tests/golden/README.md`.
//!
//! The goldens are captured on Unix. On Windows the same commands correctly
//! report `platform.owner_only_via_acl` and `platform.no_follow_after_open`
//! warnings, and the permission counts have no meaning at all, so a single
//! pinned file could not describe both platforms honestly. The Windows
//! behaviour is covered by the other integration tests instead.
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

mod golden_support;

use golden_support::compare::{golden_root, settle};
use golden_support::{Captured, Stage, World, normalise, payloads};

/// One pinned invocation.
struct Case {
    /// The directory under `tests/golden/` that holds this case's files.
    name: &'static str,
    /// How far the synthetic archive is built before the run.
    stage: Stage,
    /// Whether the case's world holds a second submission.
    second_submission: bool,
    /// Work done to the built world before the run, such as damaging one
    /// stored object or writing the inputs of a refused import.
    prepare: fn(&World),
    /// The invocation, without `--json`.
    arguments: Arguments,
}

/// How one case spells its invocation, given the world it runs against.
type Arguments = fn(&World) -> Vec<String>;

/// The world needs nothing beyond the stage it was built to.
fn ready(_: &World) {}

fn cases() -> Vec<Case> {
    let mut cases = archive_cases();
    cases.extend(record_cases());
    cases.extend(association_cases());
    cases
}

/// The cases that exercise the archive itself: creation, import, the check,
/// the export, the permission repair, and the two commands that need no
/// archive at all.
fn archive_cases() -> Vec<Case> {
    vec![
        Case {
            name: "capabilities",
            stage: Stage::Empty,
            second_submission: false,
            prepare: ready,
            arguments: |_| vec!["capabilities".to_owned()],
        },
        Case {
            name: "usage.arguments",
            stage: Stage::Empty,
            second_submission: false,
            prepare: ready,
            arguments: |_| vec!["import".to_owned()],
        },
        Case {
            name: "archive.init",
            stage: Stage::Empty,
            second_submission: false,
            prepare: ready,
            arguments: |world| {
                vec![
                    "archive".to_owned(),
                    "init".to_owned(),
                    world.archive_string(),
                ]
            },
        },
        Case {
            name: "import.success",
            stage: Stage::Initialised,
            second_submission: false,
            prepare: ready,
            arguments: |world| {
                let mut arguments = world.import_arguments();
                arguments.extend(world.input_strings());
                arguments
            },
        },
        Case {
            name: "import.duplicate",
            stage: Stage::Imported,
            second_submission: false,
            prepare: ready,
            arguments: |world| {
                let mut arguments = world.import_arguments();
                arguments.push(world.input_strings()[0].clone());
                arguments
            },
        },
        Case {
            name: "import.cap-refusal",
            stage: Stage::Initialised,
            second_submission: false,
            prepare: golden_support::write_over_the_import_file_cap,
            arguments: |world| {
                let mut arguments = world.import_arguments();
                arguments.extend(golden_support::over_the_import_file_cap(world));
                arguments
            },
        },
        Case {
            name: "archive.check.clean",
            stage: Stage::Associated,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.command(&["archive", "check"]),
        },
        Case {
            name: "archive.check.damaged",
            stage: Stage::Associated,
            second_submission: false,
            prepare: golden_support::damage_one_object,
            arguments: |world| world.command(&["archive", "check"]),
        },
        Case {
            name: "archive.status",
            stage: Stage::Associated,
            second_submission: true,
            prepare: golden_support::record_window_submissions,
            arguments: |world| {
                let mut arguments = world.command(&["archive", "status"]);
                arguments.extend([
                    "--as-of".to_owned(),
                    golden_support::STATUS_AS_OF.to_owned(),
                ]);
                arguments
            },
        },
        Case {
            name: "archive.repair-permissions",
            stage: Stage::Associated,
            second_submission: false,
            prepare: golden_support::widen_one_directory,
            arguments: |world| world.command(&["archive", "repair-permissions"]),
        },
        Case {
            name: "case.delete",
            stage: Stage::Associated,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.delete_arguments(false),
        },
        Case {
            name: "case.delete.purge",
            stage: Stage::Associated,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.delete_arguments(true),
        },
        Case {
            name: "case.export",
            stage: Stage::Associated,
            second_submission: false,
            prepare: ready,
            arguments: |world| {
                let mut arguments = world.command(&["case", "export"]);
                arguments.extend([
                    "--case".to_owned(),
                    world.case_id.clone(),
                    "--to".to_owned(),
                    world.export_destination(),
                ]);
                arguments
            },
        },
    ]
}

/// The cases that pin the user's own records.
fn record_cases() -> Vec<Case> {
    vec![
        Case {
            name: "case.create",
            stage: Stage::Imported,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.case_create_arguments(),
        },
        Case {
            name: "case.list",
            stage: Stage::Cased,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.command(&["case", "list"]),
        },
        Case {
            name: "case.show",
            stage: Stage::Submitted,
            second_submission: false,
            prepare: ready,
            arguments: |world| {
                let mut arguments = world.command(&["case", "show"]);
                arguments.push(world.case_id.clone());
                arguments
            },
        },
        Case {
            name: "submission.add",
            stage: Stage::Cased,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.submission_arguments(0),
        },
        Case {
            name: "receipt.add",
            stage: Stage::Submitted,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.receipt_arguments(),
        },
        Case {
            name: "receipt.list",
            stage: Stage::Receipted,
            second_submission: false,
            prepare: ready,
            arguments: |world| world.command(&["receipt", "list"]),
        },
    ]
}

/// The cases that pin every association outcome and the whole history.
fn association_cases() -> Vec<Case> {
    let outcomes: [(&'static str, Arguments); 4] = [
        ("association.create.unassociated", |world| {
            world.association_arguments("unassociated", &[])
        }),
        ("association.create.candidate", |world| {
            world.association_arguments("candidate", &[(0, "moderate", "The reference matches.")])
        }),
        ("association.create.associated", |world| {
            world.association_arguments(
                "associated",
                &[(0, "strong", "The case number is the same.")],
            )
        }),
        ("association.create.contradictory", |world| {
            world.association_arguments(
                "contradictory",
                &[
                    (0, "weak", "The date is close."),
                    (1, "moderate", "The reference matches instead."),
                ],
            )
        }),
    ];
    let mut cases: Vec<Case> = Vec::with_capacity(6);
    for (name, arguments) in outcomes {
        cases.push(Case {
            name,
            stage: Stage::Receipted,
            second_submission: name.ends_with("contradictory"),
            prepare: ready,
            arguments,
        });
    }
    cases.push(Case {
        name: "association.retire",
        stage: Stage::Associated,
        second_submission: false,
        prepare: ready,
        arguments: |world| {
            let mut arguments = world.command(&["association", "retire"]);
            arguments.push(world.association_ids[1].clone());
            arguments.extend([
                "--reason".to_owned(),
                "The user withdrew the statement.".to_owned(),
            ]);
            arguments
        },
    });
    cases.push(Case {
        name: "association.list",
        stage: Stage::Associated,
        second_submission: false,
        prepare: ready,
        arguments: |world| {
            let mut arguments = world.command(&["association", "list"]);
            arguments.extend(["--receipt".to_owned(), world.receipt_id.clone()]);
            arguments
        },
    });
    cases
}

/// Run one invocation and normalise both of its streams.
fn capture(case: &Case, json: bool) -> Captured {
    let world = World::build(case.stage, case.second_submission);
    (case.prepare)(&world);
    let mut arguments = (case.arguments)(&world);
    if json {
        arguments.push("--json".to_owned());
    }
    let output: Output = Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(&arguments)
        .output()
        .expect("run the openpapir binary under test");
    world.captured(&output, json)
}

/// Every pinned invocation, in both modes, against the committed files.
#[test]
fn the_captured_output_matches_the_golden_files() {
    let root = golden_root();
    let mut differences = Vec::new();
    for case in cases() {
        let directory = root.join(case.name);
        let json = capture(&case, true);
        assert_eq!(
            json.stderr, "",
            "the JSON form of {} writes nothing to stderr",
            case.name
        );
        assert!(
            !json.stdout.contains("<root>"),
            "no JSON field of {} may carry a user-supplied path",
            case.name
        );
        settle(&directory.join("json.json"), &json.stdout, &mut differences);
        settle(&directory.join("json.exit"), &json.exit, &mut differences);
        let human = capture(&case, false);
        settle(
            &directory.join("human.txt"),
            &human.stdout,
            &mut differences,
        );
        settle(
            &directory.join("human.stderr.txt"),
            &human.stderr,
            &mut differences,
        );
        settle(&directory.join("human.exit"), &human.exit, &mut differences);
    }
    assert!(
        differences.is_empty(),
        "the CLI output no longer matches the golden files. Read \
         tests/golden/README.md before regenerating them.\n\n{}",
        differences.join("\n\n")
    );
}

/// The golden directory holds exactly the cases this file drives, so a
/// removed case cannot leave an unread file behind claiming to be a contract.
#[test]
fn the_golden_directory_holds_no_case_this_file_does_not_drive() {
    let root = golden_root();
    let expected: Vec<&str> = cases().iter().map(|case| case.name).collect();
    let mut found: Vec<String> = fs::read_dir(&root)
        .expect("read the golden directory")
        .map(|entry| entry.expect("read a golden entry"))
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut expected: Vec<String> = expected.into_iter().map(str::to_owned).collect();
    expected.sort();
    assert_eq!(found, expected, "a golden directory has no case driving it");
}

/// The normalisation is what makes a run reproducible, so its own rules are
/// checked rather than assumed.
#[test]
fn the_placeholders_replace_exactly_what_moves_between_runs() {
    let text = concat!(
        "Case 6b73d041fb6bed26be75545fabfd45bc, recorded 2026-01-14T09:12:33Z.\n",
        "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f\n",
        "Date stated by the user: 2026-01-13. Title: Tax matter\n",
        "/tmp/example/export\n"
    );
    let normalised = normalise(text, Path::new("/tmp/example"));
    assert!(normalised.contains("Case <id>, recorded <time>."));
    assert!(normalised.contains("sha256:<digest>"));
    assert!(
        normalised.contains("Date stated by the user: 2026-01-13."),
        "a user's own date is part of the contract and is never masked"
    );
    assert!(normalised.contains("Title: Tax matter"));
    assert!(normalised.contains("<root>/export"));
}

/// The synthetic payloads are fixed, because a changed byte would change
/// every digest and every byte count in the golden files at once.
#[test]
fn the_synthetic_payloads_are_pinned() {
    let [alpha, beta, gamma] = payloads();
    assert_eq!(alpha.len(), 27);
    assert_eq!(beta.len(), 23);
    assert_eq!(gamma.len(), 27);
    assert_ne!(
        alpha, gamma,
        "two inputs of one length are still two objects"
    );
}
