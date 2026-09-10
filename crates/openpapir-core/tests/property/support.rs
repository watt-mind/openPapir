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
use openpapir_core::archive::import::ImportEvent;
use openpapir_core::archive::limits;
use openpapir_core::error::Diagnostic;
use openpapir_core::records::MAX_STATEMENT_BYTES;
use openpapir_core::records::association::{
    Association, CONFIDENCES, CREATED_BY, Candidate, EVIDENCE_KIND, EVIDENCE_SOURCE, Evidence,
    OUTCOMES,
};
use openpapir_core::records::case::{Case, Status};
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
///
/// The first character is never a space, so a generated value is never blank
/// and a required field the write path checks would accept it. The pool
/// carries two multi-byte characters and the two characters JSON escapes, so
/// a byte cap and a character cap disagree on the value and a round-trip has
/// to survive escaping.
pub fn text(max: usize) -> impl Strategy<Value = String> {
    (
        proptest::sample::select(vec!['a', 'Z', '9', '.', 'é', '中', '"', '\\']),
        proptest::collection::vec(
            proptest::sample::select(vec![' ', 'a', 'Z', '9', '.', ',', 'é', '中', '"', '\\']),
            0..max,
        ),
    )
        .prop_map(|(first, rest)| std::iter::once(first).chain(rest).collect::<String>())
}

/// An RFC 3339 instant of the shape the clock module writes.
pub fn timestamp() -> impl Strategy<Value = String> {
    (2000_u32..2100, 1_u32..13, 1_u32..29, 0_u32..24, 0_u32..60).prop_map(
        |(year, month, day, hour, minute)| {
            format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z")
        },
    )
}

/// A generated case status.
pub fn status() -> impl Strategy<Value = Status> {
    prop_oneof![Just(Status::Open), Just(Status::Closed)]
}

/// A tag list the write path would accept, at and under both of its caps.
///
/// A tag is at most 64 bytes and a case carries at most 32 of them, and the
/// stored list is sorted and deduplicated. The generator covers ordinary tags,
/// multi-byte tags where a byte cap and a character cap disagree, one tag
/// sitting exactly on the length cap, and exactly 32 tags each sitting exactly
/// on it, which is both caps at once.
pub fn tags() -> impl Strategy<Value = Vec<String>> {
    let length = usize::try_from(limits::MAX_TAG_BYTES).expect("the cap fits in memory");
    let count = usize::try_from(limits::MAX_TAG_COUNT).expect("the cap fits in memory");
    prop_oneof![
        Just(Vec::new()),
        proptest::collection::vec(
            proptest::string::string_regex("[a-z0-9][a-z0-9 ._-]{0,20}")
                .expect("a tag pattern compiles"),
            1..6,
        ),
        proptest::collection::vec(
            proptest::string::string_regex("[éÁ中]{1,20}").expect("a multi-byte pattern compiles"),
            1..4,
        ),
        Just(vec!["a".repeat(length)]),
        Just(
            (0..count)
                .map(|index| format!("{index:0length$}"))
                .collect::<Vec<String>>()
        ),
    ]
    .prop_map(|mut tags| {
        tags.sort();
        tags.dedup();
        tags
    })
}

/// A generated case record whose fields are all within their caps.
///
/// `status` and `tags` are written by every build that has them, and a record
/// an earlier build wrote carries neither, so both have a serde default and
/// the reader properties cover their absence. `updated_at` is absent until
/// `case update` rewrites the record, so it is generated as an option.
pub fn case_record() -> impl Strategy<Value = Case> {
    (
        identifier(),
        timestamp(),
        text(40),
        proptest::option::of(text(40)),
        status(),
        tags(),
        proptest::option::of(timestamp()),
    )
        .prop_map(
            |(id, created_at, title, notes, status, tags, updated_at)| Case {
                archive_schema_version: SUPPORTED_SCHEMA_VERSION,
                created_at,
                id,
                notes,
                record_kind: openpapir_core::records::case::KIND.to_owned(),
                status,
                tags,
                title,
                updated_at,
            },
        )
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

/// A statement the write path would accept, up to and including its cap.
///
/// `checked_statement` asks for a single line, no control character, and at
/// most 512 bytes, and the cap is counted in bytes rather than characters. So
/// the generator carries ordinary text, multi-byte text, and two values that
/// sit exactly on the cap, one of them in two-byte characters, which is where
/// a byte cap and a character cap would disagree.
pub fn statement() -> impl Strategy<Value = String> {
    let cap = usize::try_from(MAX_STATEMENT_BYTES).expect("the cap fits in memory");
    prop_oneof![
        proptest::string::string_regex("[a-zA-Z0-9 .,()'\"-]{0,60}[a-z]")
            .expect("a printable pattern compiles"),
        proptest::string::string_regex("[éÁ中]{1,40}").expect("a multi-byte pattern compiles"),
        Just("a".repeat(cap)),
        Just("é".repeat(cap / 2)),
    ]
}

/// A generated association record, with between zero and three candidates.
///
/// `statement` is the retirement reason. A record that is not a retirement
/// carries none, so the field is generated as an option and the round-trip
/// has to survive a key that is written only when it is there.
pub fn association_record() -> impl Strategy<Value = Association> {
    (
        identifier(),
        identifier(),
        timestamp(),
        proptest::sample::select(OUTCOMES.to_vec()),
        proptest::collection::vec(candidate(), 0..3),
        proptest::option::of(identifier()),
        proptest::option::of(identifier()),
        proptest::option::of(statement()),
    )
        .prop_map(
            |(
                id,
                receipt_id,
                created_at,
                outcome,
                candidates,
                submission_id,
                supersedes,
                statement,
            )| {
                Association {
                    archive_schema_version: SUPPORTED_SCHEMA_VERSION,
                    candidates,
                    created_at,
                    created_by: CREATED_BY.to_owned(),
                    id,
                    outcome: outcome.to_owned(),
                    receipt_id,
                    record_kind: openpapir_core::records::association::KIND.to_owned(),
                    statement,
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

/// A generated import-event record.
///
/// Import events are records like any other, so the reader properties cover
/// them alongside the four the archive design names.
pub fn import_event_record() -> impl Strategy<Value = ImportEvent> {
    (
        identifier(),
        digest(),
        timestamp(),
        0_u64..1_000_000,
        any::<bool>(),
        proptest::string::string_regex("[a-z0-9._-]{1,30}").expect("a filename pattern compiles"),
    )
        .prop_map(
            |(id, digest, imported_at, byte_length, created_object, original_filename)| {
                ImportEvent {
                    archive_schema_version: u64::from(SUPPORTED_SCHEMA_VERSION),
                    byte_length,
                    created_object,
                    digest,
                    id,
                    imported_at,
                    original_filename,
                    record_kind: openpapir_core::archive::import::EVENT_KIND.to_owned(),
                }
            },
        )
}

/// One way a stored record document can differ from the one openPapir wrote.
///
/// The two groups are the point of the enumeration. A reader checks three
/// things and no more: that the document parses as its own kind, that its
/// `record_kind` is that kind, and that its `id` is the name of the file it
/// was found in. A difference that leaves all three true must still read back,
/// and one that breaks any of them must be refused. Splitting the enumeration
/// this way makes both arms of the reader reachable by construction rather
/// than by luck, and pins what the reader deliberately does not re-check: a
/// field length cap belongs to the write path, not to the read path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Damage {
    /// Nothing is changed.
    None,
    /// A key no version of this record defines is added. Serde ignores it.
    UnknownKey,
    /// A capped text field is set to exactly its cap.
    FieldAtCap,
    /// A capped text field is set to one byte over its cap. The reader does
    /// not re-check field caps, so the document still reads back.
    FieldOverCap,
    /// A key the record cannot be parsed without is removed.
    MissingKey,
    /// A value is replaced by one of the wrong JSON type.
    WrongType,
    /// `record_kind` names a different kind of record.
    ForeignKind,
    /// `id` names a record other than the file the document is stored in.
    ForeignIdentifier,
}

impl Damage {
    /// Whether a document carrying this damage must still read back.
    #[must_use]
    pub const fn tolerated(self) -> bool {
        matches!(
            self,
            Self::None | Self::UnknownKey | Self::FieldAtCap | Self::FieldOverCap
        )
    }
}

/// The damages a reader must tolerate.
pub fn tolerated_damage() -> impl Strategy<Value = Damage> {
    prop_oneof![
        Just(Damage::None),
        Just(Damage::UnknownKey),
        Just(Damage::FieldAtCap),
        Just(Damage::FieldOverCap),
    ]
}

/// The damages a reader must refuse.
pub fn breaking_damage() -> impl Strategy<Value = Damage> {
    prop_oneof![
        Just(Damage::MissingKey),
        Just(Damage::WrongType),
        Just(Damage::ForeignKind),
        Just(Damage::ForeignIdentifier),
    ]
}

/// The keys every record kind carries and cannot be parsed without.
pub const REQUIRED_KEYS: [&str; 3] = ["archive_schema_version", "id", "record_kind"];

/// An identifier no generated record uses, for the foreign-identifier damage.
pub const FOREIGN_ID: &str = "ffffffffffffffffffffffffffffffff";

/// A top-level capped field of one record kind, and the shape it holds.
///
/// The two field-length damages need to know the shape, because setting a
/// list-valued field to a string would change the document's type rather than
/// its length and would land in the wrong half of [`Damage`].
#[derive(Debug, Clone, Copy)]
pub enum Capped {
    /// One text value, with a byte cap.
    Text(&'static str, usize),
    /// A list of text values, with a byte cap on each and a cap on the count.
    TextList(&'static str, usize, usize),
}

impl Capped {
    /// The value this field takes at its cap, or one step over it.
    fn filled(self, over: bool) -> (&'static str, serde_json::Value) {
        let step = usize::from(over);
        match self {
            Self::Text(field, cap) => (field, serde_json::Value::String("a".repeat(cap + step))),
            Self::TextList(field, cap, count) => {
                let width = cap + step;
                let entries: Vec<serde_json::Value> = (0..count + step)
                    .map(|index| serde_json::Value::String(format!("{index:0width$}")))
                    .collect();
                (field, serde_json::Value::Array(entries))
            }
        }
    }
}

/// Render one record as the document that would be stored at `id`, damaged.
///
/// `capped` lists this kind's top-level capped fields, for the two
/// field-length damages; a kind whose only capped text lives inside a nested
/// value passes an empty slice and those two damages leave the document alone.
/// `key` is generated, so the choice of which required key goes missing and
/// which value takes the wrong type shrinks like any other input.
pub fn damaged_document<R: serde::Serialize>(
    record: &R,
    id: &str,
    damage: Damage,
    capped: &[Capped],
    key: &str,
) -> String {
    let mut value = serde_json::to_value(record).expect("a record serialises");
    let object = value.as_object_mut().expect("a record is an object");
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    match damage {
        Damage::None => {}
        Damage::UnknownKey => {
            object.insert(
                "papir_unknown_key".to_owned(),
                serde_json::Value::String("ignored".to_owned()),
            );
        }
        Damage::FieldAtCap | Damage::FieldOverCap => {
            for field in capped {
                let (name, filled) = field.filled(damage == Damage::FieldOverCap);
                object.insert(name.to_owned(), filled);
            }
        }
        Damage::MissingKey => {
            object.remove(key);
        }
        Damage::WrongType => {
            object.insert(key.to_owned(), serde_json::json!([1, 2, 3]));
        }
        Damage::ForeignKind => {
            object.insert(
                "record_kind".to_owned(),
                serde_json::Value::String("papir_not_a_kind".to_owned()),
            );
        }
        Damage::ForeignIdentifier => {
            object.insert(
                "id".to_owned(),
                serde_json::Value::String(FOREIGN_ID.to_owned()),
            );
        }
    }
    format!("{value}\n")
}
