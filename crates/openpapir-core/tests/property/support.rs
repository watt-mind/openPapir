//! Shared strategies and helpers for the library's property suite.
//!
//! The generators here are deliberately small. A property that has to create
//! an archive on disk pays for every case, so the strategies stay inside the
//! record field caps and the suite bounds its case counts instead of relying
//! on proptest's default.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;

use openpapir_core::archive::SUPPORTED_SCHEMA_VERSION;
use openpapir_core::error::Diagnostic;
use openpapir_core::records::association::{
    Association, CONFIDENCES, CREATED_BY, Candidate, EVIDENCE_KIND, EVIDENCE_SOURCE, Evidence,
    OUTCOMES,
};
use openpapir_core::records::case::Case;
use openpapir_core::records::receipt::Receipt;
use openpapir_core::records::submission::{ArtefactRef, Submission};

/// The proptest configuration this suite uses.
///
/// `default_cases` is what continuous integration runs, chosen so that the
/// whole target finishes well inside a minute. `PROPTEST_CASES` overrides it,
/// which is how a contributor runs a long soak locally. Failure persistence is
/// off: a counterexample belongs in a named regression test in the repository,
/// not in a file proptest writes beside the source.
pub fn config(default_cases: u32) -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|cases| *cases > 0)
        .unwrap_or(default_cases);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// The keys a refusal's `details` object is allowed to carry.
///
/// The privacy rule is that a refusal names the shape of the failure and never
/// the value that caused it, so the set is fixed and every property that
/// inspects a refusal checks against it.
const PERMITTED_DETAIL_KEYS: [&str; 17] = [
    "archive_path",
    "argument",
    "bucket",
    "cap_bytes",
    "capability",
    "condition",
    "conflict_count",
    "export_path",
    "field",
    "input_index",
    "observed_bytes",
    "path_count",
    "record_kind",
    "reference_kind",
    "rule",
    "scope",
    "stage",
];

/// Assert that a refusal carries no detail key outside the permitted set.
pub fn details_are_permitted(refusal: &Diagnostic) -> Result<(), TestCaseError> {
    let rendered = serde_json::to_value(refusal).expect("a diagnostic serialises");
    let details = rendered["details"]
        .as_object()
        .expect("details is an object");
    for key in details.keys() {
        prop_assert!(
            PERMITTED_DETAIL_KEYS.contains(&key.as_str()),
            "a refusal carried an undocumented detail key"
        );
    }
    Ok(())
}

/// Assert that a refusal's exit code comes from the documented bucket table.
///
/// `1` is never an openPapir exit code, and `0` never accompanies a refusal.
pub fn exit_code_is_bucketed(refusal: &Diagnostic) -> Result<(), TestCaseError> {
    prop_assert!(
        [2, 3, 4, 5, 6].contains(&refusal.exit_code()),
        "a refusal used an exit code outside the bucket table"
    );
    Ok(())
}

/// A directory holding one empty record directory per kind.
pub fn record_directories(root: &Path) {
    for directory in [
        openpapir_core::archive::CASES_DIR,
        openpapir_core::archive::SUBMISSIONS_DIR,
        openpapir_core::archive::RECEIPTS_DIR,
        openpapir_core::archive::ASSOCIATIONS_DIR,
        openpapir_core::archive::IMPORTS_DIR,
    ] {
        fs::create_dir_all(root.join(directory)).expect("a record directory is created");
    }
}

/// Every path under `root`, relative and sorted, as a listing to compare.
pub fn listing(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                pending.push(path);
            }
            found.insert(relative);
        }
    }
    found
}

/// The alphabet an identifier and a digest are written in.
fn hexadecimal() -> Vec<u8> {
    b"0123456789abcdef".to_vec()
}

/// A well-formed record identifier: 32 lowercase hexadecimal characters.
pub fn identifier() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(hexadecimal()), 32)
        .prop_map(|bytes| String::from_utf8(bytes).expect("hexadecimal is ASCII"))
}

/// A well-formed artefact reference: `sha256:` and 64 hexadecimal characters.
pub fn digest() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(hexadecimal()), 64)
        .prop_map(|bytes| format!("sha256:{}", String::from_utf8(bytes).expect("ASCII")))
}

/// A short text field: printable, inside every field cap this suite uses.
pub fn text(max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(vec![' ', 'a', 'Z', '9', '.', ',', 'é', '中', '"', '\\']),
        1..max,
    )
    .prop_map(|characters| characters.into_iter().collect::<String>())
}

/// An RFC 3339 instant of the shape the clock module writes.
pub fn timestamp() -> impl Strategy<Value = String> {
    (2000_u32..2100, 1_u32..13, 1_u32..29, 0_u32..24, 0_u32..60).prop_map(
        |(year, month, day, hour, minute)| {
            format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z")
        },
    )
}

/// A generated case record whose fields are all within their caps.
pub fn case_record() -> impl Strategy<Value = Case> {
    (
        identifier(),
        timestamp(),
        text(40),
        proptest::option::of(text(40)),
    )
        .prop_map(|(id, created_at, title, notes)| Case {
            archive_schema_version: SUPPORTED_SCHEMA_VERSION,
            created_at,
            id,
            notes,
            record_kind: openpapir_core::records::case::KIND.to_owned(),
            title,
        })
}

/// A generated submission record, with between zero and three artefacts.
pub fn submission_record() -> impl Strategy<Value = Submission> {
    (
        identifier(),
        identifier(),
        timestamp(),
        text(40),
        proptest::option::of(text(12)),
        proptest::collection::vec(
            (digest(), proptest::option::of(text(12)))
                .prop_map(|(digest, role)| ArtefactRef { digest, role }),
            0..3,
        ),
    )
        .prop_map(
            |(id, case_id, created_at, description, stated_date, artefacts)| Submission {
                archive_schema_version: SUPPORTED_SCHEMA_VERSION,
                artefacts,
                case_id,
                created_at,
                description,
                id,
                record_kind: openpapir_core::records::submission::KIND.to_owned(),
                stated_date,
            },
        )
}

/// A generated receipt record.
pub fn receipt_record() -> impl Strategy<Value = Receipt> {
    (
        identifier(),
        identifier(),
        digest(),
        timestamp(),
        proptest::option::of(text(20)),
    )
        .prop_map(
            |(id, import_event_id, artefact_digest, created_at, label)| Receipt {
                archive_schema_version: SUPPORTED_SCHEMA_VERSION,
                artefact_digest,
                created_at,
                id,
                import_event_id,
                label,
                record_kind: openpapir_core::records::receipt::KIND.to_owned(),
            },
        )
}

/// A generated association record, with between zero and three candidates.
pub fn association_record() -> impl Strategy<Value = Association> {
    (
        identifier(),
        identifier(),
        timestamp(),
        proptest::sample::select(OUTCOMES.to_vec()),
        proptest::collection::vec(candidate(), 0..3),
        proptest::option::of(identifier()),
        proptest::option::of(identifier()),
    )
        .prop_map(
            |(id, receipt_id, created_at, outcome, candidates, submission_id, supersedes)| {
                Association {
                    archive_schema_version: SUPPORTED_SCHEMA_VERSION,
                    candidates,
                    created_at,
                    created_by: CREATED_BY.to_owned(),
                    id,
                    outcome: outcome.to_owned(),
                    receipt_id,
                    record_kind: openpapir_core::records::association::KIND.to_owned(),
                    submission_id,
                    supersedes,
                }
            },
        )
}

fn candidate() -> impl Strategy<Value = Candidate> {
    (
        identifier(),
        proptest::sample::select(CONFIDENCES.to_vec()),
        text(30),
    )
        .prop_map(|(submission_id, confidence, statement)| Candidate {
            confidence: confidence.to_owned(),
            evidence: vec![Evidence {
                kind: EVIDENCE_KIND.to_owned(),
                source: EVIDENCE_SOURCE.to_owned(),
                statement,
            }],
            submission_id,
        })
}

/// A name no archive path may ever hold.
///
/// Every generated value carries at least one of the four hostile traits the
/// invariant names: a `..` traversal, a path separator, a NUL, or a component
/// longer than the filename cap. The benign head and tail exist so that
/// shrinking has somewhere to go and so that a hostile fragment is also tried
/// in the middle of an otherwise plausible name.
pub fn hostile_name() -> impl Strategy<Value = String> {
    (benign_fragment(), hostile_fragment(), benign_fragment())
        .prop_map(|(head, core, tail)| format!("{head}{core}{tail}"))
}

fn hostile_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("..".to_owned()),
        Just("../..".to_owned()),
        Just("/".to_owned()),
        Just("\\".to_owned()),
        Just("\u{0}".to_owned()),
        Just("/etc/passwd".to_owned()),
        Just("a".repeat(300)),
        Just("0".repeat(300)),
    ]
}

fn benign_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("0123456789abcdef".to_owned()),
        Just("record".to_owned()),
        proptest::string::string_regex("[a-z0-9]{0,8}").expect("a printable pattern compiles"),
    ]
}
