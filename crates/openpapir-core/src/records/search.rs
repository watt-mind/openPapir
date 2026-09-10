//! Search over the user's own record text, and over nothing else.
//!
//! A search reads the text the user typed into their own records and the
//! identifiers openPapir minted for them. It reads no stored object, no
//! original filename, and nothing derived, so a hit says the user wrote a
//! word in one of their own records and says nothing about what any artefact
//! contains, and nothing about delivery, receipt by an authority,
//! authenticity, or legal effect.
//!
//! # What is read
//!
//! | Kind | Fields |
//! | --- | --- |
//! | `association` | `id`, `statement` |
//! | `case` | `id`, `notes`, `tags`, `title` |
//! | `receipt` | `id`, `label` |
//! | `submission` | `description`, `id` |
//!
//! An association's `statement` is the user's own reason, carried by a
//! retirement. The evidence statements inside an association's candidates are
//! not read, and neither is a submission's stated date, an artefact role, a
//! digest, a timestamp, or any derived-metadata record.
//!
//! # How it matches
//!
//! The comparison is the one `case list --query` makes: the query and the
//! field are folded with `str::to_lowercase` and the match is a substring of
//! the folded field. There is **no index**, so the search reads every record
//! of every kind it was asked for, in one linear scan per kind, and its cost
//! grows with what the archive holds.
//!
//! # What a hit carries
//!
//! The kind, the record's identifier, the case it belongs to when it belongs
//! to one, and the name of the field that matched. Never the matched text,
//! never the text of another record, and never the query: a refusal and a
//! result alike carry field names and identifiers only.

use std::path::Path;

use serde::Serialize;

use crate::archive::Archive;
use crate::error::{Details, Diagnostic, Failure, Outcome, Result, Warning, codes};
use crate::records::association::{self, Association};
use crate::records::case::{self, Case};
use crate::records::checked_query;
use crate::records::document::{self, Record};
use crate::records::receipt::{self, Receipt};
use crate::records::submission::{self, Submission};

/// One record kind a search may read.
///
/// The set is closed, and its order is the order hits are reported in, which
/// is the alphabetical order of the names the records themselves carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// What the user asserts about a receipt.
    Association,
    /// The user's own folder for a matter.
    Case,
    /// An artefact the user believes to be a receipt.
    Receipt,
    /// Something the user states they sent.
    Submission,
}

/// Every kind a search reads when it was told no kind at all.
pub const KINDS: [Kind; 4] = [
    Kind::Association,
    Kind::Case,
    Kind::Receipt,
    Kind::Submission,
];

impl Kind {
    /// The kind's stable name, which is the record's own `record_kind`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Association => association::KIND,
            Self::Case => case::KIND,
            Self::Receipt => receipt::KIND,
            Self::Submission => submission::KIND,
        }
    }

    /// The kind one of the four stable names spells.
    ///
    /// # Errors
    ///
    /// Returns `usage.arguments`, naming the argument and never the value:
    /// what the user typed is their own text even when this build cannot use
    /// it.
    pub fn parse(value: &str) -> std::result::Result<Self, Diagnostic> {
        KINDS
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                Diagnostic::new(
                    codes::USAGE_ARGUMENTS,
                    "A record kind must be `association`, `case`, `receipt`, or `submission`.",
                    Details::new().text("argument", "kind"),
                )
            })
    }
}

/// One record whose own text holds the query.
///
/// The entry names where the text was found and never what it was. Reporting
/// the matched text would put one record's words in an answer about another,
/// and the field name is what a reader needs to know which of their own
/// records to open next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// The case the record belongs to, absent when it belongs to none. Only a
    /// submission belongs to a case today: a case is one, and a receipt and
    /// an association are archive-wide records that name no case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    /// The name of the field whose text held the query, never the text.
    pub field: String,
    /// The record's own identifier, minted by openPapir.
    pub id: String,
    /// The record kind, one of the four stable names.
    pub kind: String,
}

/// What a search reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Found {
    /// How many hits the search reports, which is how many record-and-field
    /// pairs held the query rather than how many records the archive holds.
    pub count: u64,
    /// Every hit, ordered by kind, then by identifier, then by field name.
    pub hits: Vec<Hit>,
    /// The kinds this search read, ordered by name. It is what was asked for,
    /// or all four when nothing was.
    pub kinds: Vec<String>,
}

/// The record a scan is looking at, as a hit would name it.
struct Subject<'a> {
    kind: Kind,
    id: &'a str,
    case_id: Option<&'a str>,
}

impl Subject<'_> {
    /// The hit this record's `field` is.
    fn hit(&self, field: &'static str) -> Hit {
        Hit {
            case_id: self.case_id.map(str::to_owned),
            field: field.to_owned(),
            id: self.id.to_owned(),
            kind: self.kind.as_str().to_owned(),
        }
    }
}

/// Whether one field's text holds the already folded query.
fn holds(value: &str, folded_query: &str) -> bool {
    value.to_lowercase().contains(folded_query)
}

/// Push one hit per named field whose text holds the query.
fn collect(
    hits: &mut Vec<Hit>,
    folded_query: &str,
    subject: &Subject<'_>,
    fields: &[(&'static str, Option<&str>)],
) {
    for (field, value) in fields {
        if value.is_some_and(|value| holds(value, folded_query)) {
            hits.push(subject.hit(field));
        }
    }
}

/// Search the archive at `root` for `query` across `kinds`, taking no lock.
///
/// `kinds` empty means every kind. A reader sees one whole document or
/// another and never a partial one, so no lock is taken, exactly as a listing
/// takes none.
///
/// # Errors
///
/// Returns `input.cap.field_length` for a query over the cap and
/// `usage.arguments` for an empty query or one carrying a forbidden
/// character, `record.malformed` when a stored document cannot be read as the
/// record it claims to be, and any archive or path refusal of
/// `docs/error-contract.md`.
pub fn search(root: &Path, query: &str, kinds: &[Kind]) -> Result<Found> {
    let mut warnings = Vec::new();
    match find(root, query, kinds, &mut warnings) {
        Ok(found) => Ok(Outcome {
            data: found,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

/// The kinds to read: the ones asked for, sorted and deduplicated, or all.
fn selected(kinds: &[Kind]) -> Vec<Kind> {
    if kinds.is_empty() {
        return KINDS.to_vec();
    }
    let mut selected = kinds.to_vec();
    selected.sort_unstable();
    selected.dedup();
    selected
}

fn find(
    root: &Path,
    query: &str,
    kinds: &[Kind],
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Found, Diagnostic> {
    // The query is checked and folded once, before the archive is opened, so
    // an oversized query is refused before anything is read and the folding
    // is not repeated for every record of every kind.
    let folded = checked_query(query)?.to_lowercase();
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let root = archive.root();
    let selected = selected(kinds);
    let mut hits = Vec::new();
    for kind in &selected {
        match kind {
            Kind::Association => scan::<Association, _>(root, |record| {
                association_hits(record, &folded, &mut hits);
            })?,
            Kind::Case => scan::<Case, _>(root, |record| case_hits(record, &folded, &mut hits))?,
            Kind::Receipt => {
                scan::<Receipt, _>(root, |record| receipt_hits(record, &folded, &mut hits))?;
            }
            Kind::Submission => scan::<Submission, _>(root, |record| {
                submission_hits(record, &folded, &mut hits);
            })?,
        }
    }
    // The kinds are read in their own order and each kind's records arrive in
    // the order the directory listed them, so the ordering the contract
    // promises is established once, here.
    hits.sort_by(|left, right| {
        (&left.kind, &left.id, &left.field).cmp(&(&right.kind, &right.id, &right.field))
    });
    Ok(Found {
        count: hits.len() as u64,
        hits,
        kinds: selected
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect(),
    })
}

/// Read every record of one kind, one document at a time.
///
/// The visiting reader holds one record rather than the whole directory, so a
/// search over an archive of many records never accumulates the documents
/// themselves: only the hits survive the scan.
fn scan<R: Record, F: FnMut(&R)>(root: &Path, mut visit: F) -> std::result::Result<(), Diagnostic> {
    let unreadable = document::visit_records::<R, _>(root, |record| visit(&record));
    if unreadable > 0 {
        return Err(document::malformed(R::KIND, unreadable));
    }
    Ok(())
}

/// The hits one case record holds.
fn case_hits(case: &Case, folded_query: &str, hits: &mut Vec<Hit>) {
    let subject = Subject {
        kind: Kind::Case,
        id: &case.id,
        case_id: None,
    };
    collect(
        hits,
        folded_query,
        &subject,
        &[
            ("id", Some(case.id.as_str())),
            ("notes", case.notes.as_deref()),
            ("title", Some(case.title.as_str())),
        ],
    );
    // The tags are one field of the record, so a case whose query is on two
    // of its tags is one hit rather than two. Each tag is tested on its own
    // rather than as one joined string, so nothing matches across the join.
    if case.tags.iter().any(|tag| holds(tag, folded_query)) {
        hits.push(subject.hit("tags"));
    }
}

/// The hits one submission record holds.
fn submission_hits(submission: &Submission, folded_query: &str, hits: &mut Vec<Hit>) {
    collect(
        hits,
        folded_query,
        &Subject {
            kind: Kind::Submission,
            id: &submission.id,
            case_id: Some(&submission.case_id),
        },
        &[
            ("description", Some(submission.description.as_str())),
            ("id", Some(submission.id.as_str())),
        ],
    );
}

/// The hits one receipt record holds.
fn receipt_hits(receipt: &Receipt, folded_query: &str, hits: &mut Vec<Hit>) {
    collect(
        hits,
        folded_query,
        &Subject {
            kind: Kind::Receipt,
            id: &receipt.id,
            case_id: None,
        },
        &[
            ("id", Some(receipt.id.as_str())),
            ("label", receipt.label.as_deref()),
        ],
    );
}

/// The hits one association record holds.
fn association_hits(association: &Association, folded_query: &str, hits: &mut Vec<Hit>) {
    collect(
        hits,
        folded_query,
        &Subject {
            kind: Kind::Association,
            id: &association.id,
            case_id: None,
        },
        &[
            ("id", Some(association.id.as_str())),
            ("statement", association.statement.as_deref()),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::MAX_NOTES_BYTES;

    fn case_record(id: &str, title: &str, notes: Option<&str>, tags: &[&str]) -> Case {
        Case {
            archive_schema_version: 1,
            created_at: "2026-01-15T10:00:00Z".to_owned(),
            id: id.to_owned(),
            notes: notes.map(str::to_owned),
            record_kind: case::KIND.to_owned(),
            status: case::Status::Open,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            title: title.to_owned(),
            updated_at: None,
        }
    }

    #[test]
    fn every_kind_keeps_its_documented_name_and_parses_back() {
        for kind in KINDS {
            assert_eq!(Kind::parse(kind.as_str()).unwrap(), kind);
        }
        let refusal = Kind::parse("Case").unwrap_err();
        assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        assert_eq!(refusal.exit_code(), 2);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["argument"], "kind");
        assert!(
            !serde_json::to_string(&refusal).unwrap().contains("Case"),
            "a refusal never echoes the value the user typed"
        );
    }

    #[test]
    fn nothing_asked_for_reads_every_kind_and_a_repeat_reads_it_once() {
        assert_eq!(selected(&[]), KINDS.to_vec());
        assert_eq!(
            selected(&[Kind::Submission, Kind::Case, Kind::Submission]),
            vec![Kind::Case, Kind::Submission]
        );
    }

    #[test]
    fn a_case_reports_one_hit_per_field_and_one_for_all_its_tags() {
        let case = case_record(
            "0123456789abcdef0123456789abcdef",
            "Tax MATTER",
            Some("The office replied about the matter."),
            &["matter", "Matters"],
        );
        let mut hits = Vec::new();
        case_hits(&case, "matter", &mut hits);
        let mut fields: Vec<&str> = hits.iter().map(|hit| hit.field.as_str()).collect();
        fields.sort_unstable();
        assert_eq!(fields, ["notes", "tags", "title"]);
        for hit in &hits {
            assert_eq!(hit.kind, "case");
            assert_eq!(hit.id, case.id);
            assert_eq!(hit.case_id, None, "a case belongs to no other case");
        }
    }

    #[test]
    fn a_field_the_record_does_not_carry_is_never_a_hit() {
        let case = case_record("a".repeat(32).as_str(), "Parking notice", None, &[]);
        let mut hits = Vec::new();
        case_hits(&case, "notice", &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].field, "title");
    }

    #[test]
    fn a_submission_hit_names_the_case_it_belongs_to() {
        let submission = Submission {
            archive_schema_version: 1,
            artefacts: Vec::new(),
            case_id: "0123456789abcdef0123456789abcdef".to_owned(),
            created_at: "2026-01-15T10:00:00Z".to_owned(),
            description: "Posted the completed FORM.".to_owned(),
            id: "fedcba9876543210fedcba9876543210".to_owned(),
            record_kind: submission::KIND.to_owned(),
            stated_date: Some("2026-01-13".to_owned()),
        };
        let mut hits = Vec::new();
        submission_hits(&submission, "form", &mut hits);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].field, "description");
        assert_eq!(
            hits[0].case_id.as_deref(),
            Some(submission.case_id.as_str())
        );
        // The stated date is the user's own text and is deliberately not read.
        hits.clear();
        submission_hits(&submission, "2026-01-13", &mut hits);
        assert!(hits.is_empty(), "a stated date is not searched");
    }

    #[test]
    fn a_hit_carries_the_field_name_and_never_the_matched_text() {
        let case = case_record(
            "0123456789abcdef0123456789abcdef",
            "Tax matter",
            Some("First contact with the office."),
            &[],
        );
        let mut hits = Vec::new();
        case_hits(&case, "office", &mut hits);
        let rendered = serde_json::to_string(&hits[0]).unwrap();
        assert!(rendered.contains("\"field\":\"notes\""));
        assert!(!rendered.contains("office"), "no matched text is reported");
    }

    #[test]
    fn the_query_is_bounded_by_the_cap_and_may_not_be_empty() {
        let refusal = checked_query(&"q".repeat(MAX_NOTES_BYTES as usize + 1)).unwrap_err();
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        assert_eq!(refusal.exit_code(), 3);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["field"], "query");
        assert_eq!(json["details"]["cap_bytes"], MAX_NOTES_BYTES);
        assert!(checked_query(&"q".repeat(MAX_NOTES_BYTES as usize)).is_ok());
        assert_eq!(
            checked_query("  ").unwrap_err().code,
            codes::USAGE_ARGUMENTS
        );
    }
}
