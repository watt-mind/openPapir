//! The `search` command: the user's own record text, and nothing else.
//!
//! This module holds the invocation and its human lines. Which fields a
//! search reads, how it folds the query, and why it reads no artefact byte
//! all live in `openpapir_core::records::search`.
//!
//! The query text is the user's own and never reaches any output, in either
//! form, whether it matched anything or not. A line names the record kind,
//! the identifier openPapir minted, the case the record belongs to when it
//! belongs to one, and the name of the field that matched. It never carries
//! the matched text, so one record's words never appear in an answer about
//! another.

use std::path::PathBuf;

use clap::Args;
use openpapir_core::error::{Failure, Outcome};
use openpapir_core::records::search::{self, Found, Kind};

/// The arguments `openpapir search` accepts.
#[derive(Args)]
pub struct Search {
    /// The archive root, which is always supplied explicitly.
    #[arg(long, value_name = "ROOT")]
    pub archive: PathBuf,
    /// The text to look for, compared without regard to case, at most 4096
    /// bytes. It is never echoed back.
    #[arg(value_name = "TEXT")]
    pub text: String,
    /// Read only this record kind, one of `case`, `submission`, `receipt`, or
    /// `association`; repeat for more, and all four when it is not given.
    #[arg(long = "kind", value_name = "KIND")]
    pub kinds: Vec<String>,
    /// Emit one JSON object instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

impl Search {
    /// Read the records of the named kinds and report where the text is.
    ///
    /// # Errors
    ///
    /// Returns `usage.arguments` for a kind this build does not define, and
    /// the refusals of `openpapir_core::search`.
    pub fn run(&self) -> Result<Outcome<Found>, Failure> {
        let kinds = self.parsed_kinds().map_err(Failure::new)?;
        search::search(&self.archive, &self.text, &kinds)
    }

    /// The kinds the invocation named, or the library's own refusal.
    ///
    /// The refusal is the library's, so an unusable kind reads the same
    /// whether it reached the library or stopped here, and the value the user
    /// typed is not echoed back.
    fn parsed_kinds(&self) -> Result<Vec<Kind>, openpapir_core::error::Diagnostic> {
        self.kinds.iter().map(|value| Kind::parse(value)).collect()
    }
}

/// The closing line every search prints.
///
/// It is the boundary of the answer: a hit says the user wrote a word in one
/// of their own records, and nothing about what any stored file contains.
const SEARCH_DISCLAIMER: &str = "Search reads the user's own record text and the identifiers openPapir minted, never a stored object, an original filename, or anything derived.";

/// The lines `search` prints without `--json`.
///
/// One line per hit, then the count, then the boundary of the answer. No line
/// carries the query, the matched text, or a path.
#[must_use]
pub fn lines(found: &Found) -> Vec<String> {
    let mut lines: Vec<String> = found
        .hits
        .iter()
        .map(|hit| match &hit.case_id {
            Some(case_id) => format!("{} {} {} (case {case_id})", hit.kind, hit.id, hit.field),
            None => format!("{} {} {}", hit.kind, hit.id, hit.field),
        })
        .collect();
    lines.push(format!(
        "{} hit(s) in the record kind(s) read: {}.",
        found.count,
        found.kinds.join(", ")
    ));
    lines.push(SEARCH_DISCLAIMER.to_owned());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpapir_core::Hit;

    fn found(hits: Vec<Hit>) -> Found {
        Found {
            count: hits.len() as u64,
            hits,
            kinds: vec!["case".to_owned(), "submission".to_owned()],
        }
    }

    fn hit(kind: &str, id: &str, field: &str, case_id: Option<&str>) -> Hit {
        Hit {
            case_id: case_id.map(str::to_owned),
            field: field.to_owned(),
            id: id.to_owned(),
            kind: kind.to_owned(),
        }
    }

    #[test]
    fn a_hit_line_names_the_field_and_the_case_when_there_is_one() {
        let lines = lines(&found(vec![
            hit("case", "0123456789abcdef0123456789abcdef", "notes", None),
            hit(
                "submission",
                "fedcba9876543210fedcba9876543210",
                "description",
                Some("0123456789abcdef0123456789abcdef"),
            ),
        ]));
        assert_eq!(lines[0], "case 0123456789abcdef0123456789abcdef notes");
        assert_eq!(
            lines[1],
            "submission fedcba9876543210fedcba9876543210 description \
             (case 0123456789abcdef0123456789abcdef)"
        );
        assert_eq!(
            lines[2],
            "2 hit(s) in the record kind(s) read: case, submission."
        );
        assert_eq!(lines[3], SEARCH_DISCLAIMER);
    }

    #[test]
    fn a_search_that_found_nothing_still_reports_its_count() {
        let lines = lines(&found(Vec::new()));
        assert_eq!(lines.len(), 2, "the count and the boundary, and no hit");
        assert!(lines[0].starts_with("0 hit(s)"));
    }

    #[test]
    fn a_kind_this_build_does_not_define_is_refused_without_being_echoed() {
        let search = Search {
            archive: PathBuf::from("archive"),
            text: "matter".to_owned(),
            kinds: vec!["CASE".to_owned()],
            json: false,
        };
        let refusal = search.parsed_kinds().unwrap_err();
        assert_eq!(refusal.exit_code(), 2);
        assert!(
            !serde_json::to_string(&refusal).unwrap().contains("CASE"),
            "a refusal never echoes the value the user typed"
        );
    }
}
