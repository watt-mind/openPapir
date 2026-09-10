//! Contract tests for `openpapir search`.
//!
//! Every fixture here is synthetic and built at test time from the constants
//! in this file. Nothing is derived from real correspondence. The assertions
//! are about the observable contract in `docs/architecture.md`: which fields
//! a search reads and which it deliberately does not, the folding rule it
//! shares with `case list --query`, the order the hits arrive in, the
//! additive envelope, the human form, the query cap, and the privacy rule
//! that keeps the query and the matched text out of every output.

use std::fs;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// The synthetic payloads the world imports, whose names and bytes carry a
/// word no record carries, so a hit on one would be a leak of a filename or
/// of object bytes rather than a match on the user's own text.
const SUBMITTED: &[u8] = b"synthetic payload marmalade\n";
const RECEIVED: &[u8] = b"synthetic payload rhubarb\n";

/// The word only the imported filenames and the object bytes carry.
const NEVER_SEARCHED: &str = "marmalade";

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

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

/// The string a JSON value holds.
fn text(value: &Value) -> String {
    value.as_str().expect("a reported string").to_owned()
}

/// One archive holding a case, a submission, a receipt, and a retired
/// association, each carrying one word of its own.
struct World {
    _home: TempDir,
    root: String,
    case_id: String,
    submission_id: String,
    receipt_id: String,
    association_id: String,
}

impl World {
    /// Run one search and return its envelope.
    fn search(&self, arguments: &[&str]) -> Value {
        let mut all = vec!["search", "--archive", &self.root];
        all.extend_from_slice(arguments);
        all.push("--json");
        stdout_json(&run(&all))
    }

    /// The `(kind, id, field)` triples one search reported, in order.
    fn hits(&self, arguments: &[&str]) -> Vec<(String, String, String)> {
        let envelope = self.search(arguments);
        assert_eq!(envelope["ok"], true, "a search that ran is a success");
        assert_eq!(envelope["command"], "search");
        assert_eq!(envelope["verified"], false);
        let hits = envelope["data"]["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|hit| (text(&hit["kind"]), text(&hit["id"]), text(&hit["field"])))
            .collect::<Vec<_>>();
        assert_eq!(
            envelope["data"]["count"].as_u64(),
            Some(hits.len() as u64),
            "the count is how many hits were reported"
        );
        hits
    }
}

/// Build the world every case below runs against.
fn world() -> World {
    let home = tempfile::tempdir().expect("a temporary directory");
    let root_path = home.path().join("archive");
    fs::create_dir(&root_path).expect("create the archive root");
    let root = root_path.to_string_lossy().into_owned();
    assert!(
        run(&["archive", "init", &root, "--json"]).status.success(),
        "the archive is created"
    );

    // The input filenames carry the word no search may find.
    let mut digests = Vec::new();
    for (name, payload) in [
        (format!("{NEVER_SEARCHED}-one.txt"), SUBMITTED),
        (format!("{NEVER_SEARCHED}-two.txt"), RECEIVED),
    ] {
        let path = home.path().join(&name);
        fs::write(&path, payload).expect("write a synthetic input");
        let imported = stdout_json(&run(&[
            "import",
            "--archive",
            &root,
            &path.to_string_lossy(),
            "--json",
        ]));
        digests.push(text(&imported["data"]["artefacts"][0]["digest"]));
    }

    let case = stdout_json(&run(&[
        "case",
        "create",
        "--archive",
        &root,
        "--title",
        "Tax matter",
        "--notes",
        "First contact with the office.",
        "--tag",
        "revenue",
        "--json",
    ]));
    let case_id = text(&case["data"]["case"]["id"]);

    let submission = stdout_json(&run(&[
        "submission",
        "add",
        "--archive",
        &root,
        "--case",
        &case_id,
        "--description",
        "Posted the completed form.",
        "--date",
        "2026-01-13",
        "--artefact",
        &format!("{}:cover letter", digests[0]),
        "--json",
    ]));
    let submission_id = text(&submission["data"]["submission"]["id"]);

    let receipt = stdout_json(&run(&[
        "receipt",
        "add",
        "--archive",
        &root,
        "--artefact",
        &digests[1],
        "--label",
        "Envelope from the post",
        "--json",
    ]));
    let receipt_id = text(&receipt["data"]["receipt"]["id"]);

    let created = stdout_json(&run(&[
        "association",
        "create",
        "--archive",
        &root,
        "--receipt",
        &receipt_id,
        "--outcome",
        "candidate",
        "--candidate",
        &format!("{submission_id}:moderate:The reference matches."),
        "--json",
    ]));
    let live = text(&created["data"]["association"]["id"]);
    let retired = stdout_json(&run(&[
        "association",
        "retire",
        "--archive",
        &root,
        &live,
        "--reason",
        "The user withdrew the mistaken statement.",
        "--json",
    ]));
    let association_id = text(&retired["data"]["association"]["id"]);

    World {
        _home: home,
        root,
        case_id,
        submission_id,
        receipt_id,
        association_id,
    }
}

/// Every kind reports the field it matched, ordered by kind then identifier.
#[test]
fn one_query_finds_a_field_of_every_kind_in_the_documented_order() {
    let world = world();
    let hits = world.hits(&["the"]);
    assert_eq!(
        hits,
        vec![
            (
                "association".to_owned(),
                world.association_id.clone(),
                "statement".to_owned()
            ),
            ("case".to_owned(), world.case_id.clone(), "notes".to_owned()),
            (
                "receipt".to_owned(),
                world.receipt_id.clone(),
                "label".to_owned()
            ),
            (
                "submission".to_owned(),
                world.submission_id.clone(),
                "description".to_owned()
            ),
        ],
        "hits are ordered by kind, then by identifier"
    );
    let envelope = world.search(&["the"]);
    assert_eq!(
        envelope["data"]["kinds"],
        serde_json::json!(["association", "case", "receipt", "submission"])
    );
    assert_eq!(envelope["data"]["hits"][3]["case_id"], world.case_id);
    assert!(
        envelope["data"]["hits"][1].get("case_id").is_none(),
        "a case belongs to no other case"
    );
}

/// The match folds both sides, exactly as `case list --query` does.
#[test]
fn the_match_ignores_case_on_both_sides() {
    let world = world();
    for query in ["TAX MATTER", "tax matter", "Tax Matter"] {
        assert_eq!(
            world.hits(&[query]),
            vec![("case".to_owned(), world.case_id.clone(), "title".to_owned())],
            "the query folds the same way whatever was typed"
        );
    }
}

/// A tag is one field, however many of a case's tags hold the query.
#[test]
fn a_tag_is_matched_and_reported_as_one_field() {
    let world = world();
    assert_eq!(
        world.hits(&["REVENUE"]),
        vec![("case".to_owned(), world.case_id.clone(), "tags".to_owned())]
    );
}

/// An identifier is the user's way back to their own record, so it matches.
#[test]
fn a_record_identifier_matches_its_own_record_and_nothing_else() {
    let world = world();
    assert_eq!(
        world.hits(&[&world.receipt_id]),
        vec![(
            "receipt".to_owned(),
            world.receipt_id.clone(),
            "id".to_owned()
        )]
    );
}

/// The evidence rule: nothing outside the user's own record text is read.
#[test]
fn no_filename_no_object_byte_and_nothing_derived_is_searched() {
    let world = world();
    assert!(
        run(&["archive", "derive", "--archive", &world.root, "--json"])
            .status
            .success(),
        "the derived records exist before the search runs"
    );
    for query in [
        NEVER_SEARCHED, // an original filename, and the object bytes
        "text",         // a derived media type
        "2026-01-13",   // the user's own stated date, deliberately not read
        "cover letter", // an artefact role
        "sha256",       // a digest openPapir minted
        "reference",    // an evidence statement inside a candidate
    ] {
        assert!(
            world.hits(&[query]).is_empty(),
            "a search found text it may not read: {query}"
        );
    }
}

/// `--kind` narrows what is read, repeats fold together, and an unknown kind
/// is a usage refusal that never echoes the value.
#[test]
fn the_kind_filter_narrows_the_scan_and_refuses_what_it_cannot_read() {
    let world = world();
    assert_eq!(
        world.hits(&["the", "--kind", "case"]),
        vec![("case".to_owned(), world.case_id.clone(), "notes".to_owned())]
    );
    let envelope = world.search(&["the", "--kind", "case", "--kind", "case"]);
    assert_eq!(envelope["data"]["kinds"], serde_json::json!(["case"]));
    assert_eq!(envelope["data"]["count"], 1);

    let refused = run(&[
        "search",
        "--archive",
        &world.root,
        "the",
        "--kind",
        "CASE",
        "--json",
    ]);
    assert_eq!(refused.status.code(), Some(2), "usage exits 2");
    let envelope = stdout_json(&refused);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["command"], "search");
    assert_eq!(envelope["data"], serde_json::json!({}));
    assert_eq!(envelope["error"]["code"], "usage.arguments");
    assert_eq!(envelope["error"]["details"]["argument"], "kind");
    assert!(
        !stdout_text(&refused).contains("CASE"),
        "a refusal never echoes the value the user typed"
    );
}

/// The query is bounded by the same cap as the longest field it reads.
#[test]
fn the_query_is_bounded_and_may_not_be_empty() {
    let world = world();
    let at_the_cap = "q".repeat(4096);
    let envelope = world.search(&[&at_the_cap]);
    assert_eq!(envelope["ok"], true, "a query at the cap is read");
    assert_eq!(envelope["data"]["count"], 0);

    let over_the_cap = "q".repeat(4097);
    let refused = run(&["search", "--archive", &world.root, &over_the_cap, "--json"]);
    assert_eq!(refused.status.code(), Some(3), "a cap is an input refusal");
    let envelope = stdout_json(&refused);
    assert_eq!(envelope["error"]["code"], "input.cap.field_length");
    assert_eq!(envelope["error"]["details"]["field"], "query");
    assert_eq!(envelope["error"]["details"]["cap_bytes"], 4096);
    assert_eq!(envelope["error"]["details"]["observed_bytes"], 4097);

    let empty = run(&["search", "--archive", &world.root, "   ", "--json"]);
    assert_eq!(empty.status.code(), Some(2));
    assert_eq!(stdout_json(&empty)["error"]["details"]["argument"], "query");
}

/// A search that matches nothing is a success with an empty report.
#[test]
fn a_query_that_matches_nothing_is_not_an_error() {
    let world = world();
    let envelope = world.search(&["nothing here holds this"]);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["count"], 0);
    assert_eq!(envelope["data"]["hits"], serde_json::json!([]));
}

/// The human form is one line per hit, then the count, then the boundary, and
/// it carries neither the query nor the matched text nor a path.
#[test]
fn the_human_form_names_the_field_and_never_the_text_or_the_query() {
    let world = world();
    let output = run(&["search", "--archive", &world.root, "office"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "a result is not a diagnostic");
    let printed = stdout_text(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 3, "one hit, the count, and the boundary");
    assert_eq!(lines[0], format!("case {} notes", world.case_id));
    assert!(lines[1].starts_with("1 hit(s) in the record kind(s) read:"));
    assert!(lines[2].starts_with("Search reads the user's own record text"));
    assert!(!printed.contains("office"), "the query is never echoed");
    assert!(
        !printed.contains("First contact"),
        "the matched text is never echoed"
    );
    assert!(!printed.contains(&world.root), "no path reaches the output");
}

/// A submission's line names the case it belongs to.
#[test]
fn a_submission_line_names_its_case() {
    let world = world();
    let output = run(&["search", "--archive", &world.root, "completed"]);
    let printed = stdout_text(&output);
    assert_eq!(
        printed.lines().next(),
        Some(
            format!(
                "submission {} description (case {})",
                world.submission_id, world.case_id
            )
            .as_str()
        )
    );
}

/// A directory that is not an archive is the shared archive refusal, not an
/// empty result that would read as "nothing is filed under that word".
#[test]
fn a_directory_that_is_not_an_archive_is_refused() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let root = home.path().to_string_lossy().into_owned();
    let output = run(&["search", "--archive", &root, "matter", "--json"]);
    assert_eq!(output.status.code(), Some(4));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["command"], "search");
    assert_eq!(envelope["error"]["code"], "archive.marker_missing");
}
