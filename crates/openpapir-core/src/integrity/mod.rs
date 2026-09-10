//! The read-only whole-archive integrity check.
//!
//! The check re-digests every stored object and compares what the store holds
//! with what the records claim (`docs/archive-layout.md`). It repairs
//! nothing, deletes nothing, and writes nothing: it takes no writer lock,
//! opens every file read-only, and reports counts.
//!
//! A re-computed digest is a storage-layer identity only. A passing check
//! says the stored bytes are the bytes their paths name and that every
//! reference resolves inside this archive. It asserts nothing about
//! authenticity, origin, delivery, or legal effect, so `verified` stays
//! `false` for a passing check exactly as it does for a failing one.
//!
//! # What the report may carry
//!
//! Counts, byte lengths, and stable code names, and nothing else. The path,
//! the filename, and the digest of a damaged object are all omitted, in the
//! report and in the diagnostic the check derives from it, because the count
//! answers the only question the output may answer
//! (`docs/error-contract.md`).

pub mod references;
pub mod store;

use std::path::Path;

use serde::Serialize;

use crate::archive::Archive;
use crate::error::{Bucket, Details, Diagnostic, Failure, Outcome, Result, codes};

/// How many of each problem the check found.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Counts {
    /// Paths inside `objects/` that are a link or another non-regular file.
    pub symlink: u64,
    /// Record documents that could not be read as a record of their kind.
    pub malformed: u64,
    /// Cycles among the `supersedes` references of the association records.
    pub supersedes_cycle: u64,
    /// Objects whose bytes no longer digest to their own path, and object
    /// entries whose name is not a digest or is filed under the wrong
    /// fan-out directories.
    pub digest_mismatch: u64,
    /// Objects whose length differs from every import event naming them.
    pub length_mismatch: u64,
    /// References that name no record or object in this archive.
    pub dangling: u64,
    /// Objects no import event, receipt, or submission references.
    pub orphan: u64,
}

impl Counts {
    /// The total number of problems found.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.symlink
            + self.malformed
            + self.supersedes_cycle
            + self.digest_mismatch
            + self.length_mismatch
            + self.dangling
            + self.orphan
    }
}

/// One entry of the report's `problems` array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Problem {
    /// The stable code the count belongs to.
    pub code: &'static str,
    /// How many times the check saw that condition.
    pub count: u64,
}

/// The whole-archive integrity report: counts and stable codes only.
///
/// The struct is `#[non_exhaustive]`: the check gains figures as it learns to
/// look at more of an archive, so a caller outside this crate matches on the
/// fields it knows and never builds one by literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Report {
    /// How many bytes were streamed through the digest.
    pub bytes_digested: u64,
    /// How many files the archive's `cache/` directory holds. A cache is
    /// openPapir's own rebuildable index, never authoritative, and safe to
    /// delete at any time, so an absent one, a stale one, and a damaged one
    /// are alike nothing at all. The figure is reported so that a cache is
    /// visible rather than invisible, and it is never a problem: the check
    /// reads no file here and concludes nothing from what it counts.
    pub cache_files: u64,
    /// How many derived-metadata records the archive holds. A derived record
    /// is openPapir's own disposable computation about a stored object, so a
    /// missing one is nothing at all rather than a problem. The count is of
    /// the files the derived directory holds under a digest name: the check
    /// does not parse one, because what a disposable computation contains
    /// decides nothing here and `archive derive` rewrites it either way. The
    /// count says how much of that work is on disk and nothing more.
    pub derived_records: u64,
    /// How many of those records name an object the store no longer holds.
    ///
    /// It is a count and never a problem: the record is disposable, nothing
    /// references it, and the next `archive derive` discards it. It is
    /// reported because it is the visible trace of a purge that stopped
    /// between its record pass and its object pass, which is otherwise
    /// invisible. A record whose object lies in a fan-out directory the check
    /// could not list is not counted, exactly as no reference into an unread
    /// directory is called dangling.
    pub derived_orphans: u64,
    /// How many object entries were examined.
    pub objects_checked: u64,
    /// How many objects no import event, receipt, or submission references.
    pub orphan_objects: u64,
    /// How many objects were not digested, because one exceeds the per-file
    /// cap or could not be read. Neither is asserted to be damaged.
    pub objects_unchecked: u64,
    /// One entry per code the check can report, ordered by code.
    pub problems: Vec<Problem>,
    /// How many record documents were examined, readable or not.
    pub records_checked: u64,
    /// How many record directories could not be listed. The records they may
    /// hold were not read, so nothing is concluded from their absence: no
    /// object is called an orphan when a directory that could reference one
    /// is unread, and no reference into an unread directory is called
    /// dangling. A directory that is simply not there is not counted here.
    pub records_unchecked: u64,
    /// How many leftover staging files the record directories hold together.
    /// A staging file is openPapir's own transient artefact from an
    /// interrupted write, so it is neither a record nor a malformed one and
    /// is not counted as a problem. It is counted and left where it is; the
    /// check deletes nothing.
    pub records_staging_files: u64,
    /// How many leftover staging files the incoming directory holds. They are
    /// counted and left where they are; the check deletes nothing.
    pub staging_files: u64,
    #[serde(skip)]
    counts: Counts,
    #[serde(skip)]
    malformed_kind: Option<&'static str>,
    #[serde(skip)]
    dangling: Option<references::Dangling>,
}

impl Report {
    /// The counts behind the report.
    #[must_use]
    pub const fn counts(&self) -> Counts {
        self.counts
    }

    /// Whether the check found nothing wrong.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.counts.total() == 0
    }

    /// The process exit code a report with problems maps to.
    ///
    /// A check that reports several conditions still exits with one code, the
    /// highest of the exit-code groups the contract lists, so an archive whose
    /// only complaint is a link inside the store exits `3` with its `path`
    /// bucket, and one that also holds a damaged object exits `4`.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        let record_or_integrity = self.counts.malformed
            + self.counts.supersedes_cycle
            + self.counts.digest_mismatch
            + self.counts.length_mismatch
            + self.counts.dangling
            + self.counts.orphan;
        if record_or_integrity > 0 {
            return Bucket::Integrity.exit_code();
        }
        if self.counts.symlink > 0 {
            return Bucket::Path.exit_code();
        }
        0
    }

    /// The first problem, in the fixed precedence the contract documents.
    ///
    /// The order is `path.symlink`, `record.malformed`,
    /// `record.inconsistent`, `integrity.digest_mismatch`,
    /// `integrity.length_mismatch`, `integrity.dangling_reference`,
    /// `integrity.orphan_object`: the conditions that stop the check from
    /// reading something come before the ones it read and disbelieved, a
    /// record the check read and could not make sense of comes before the
    /// objects the records describe, and a reference that resolves nowhere
    /// comes before an object nothing references. The choice is fixed so that
    /// one archive always reports the same code.
    ///
    /// The diagnostic carries counts and kinds only. The path, the name, and
    /// the digest of a damaged object are never reported.
    #[must_use]
    pub fn first_problem(&self) -> Option<Diagnostic> {
        let counts = self.counts;
        if counts.symlink > 0 {
            return Some(Diagnostic::new(
                codes::PATH_SYMLINK,
                "A path inside the artefact store is a symbolic link or another non-regular file.",
                Details::new()
                    .text("scope", "archive")
                    .int("path_count", counts.symlink),
            ));
        }
        if counts.malformed > 0 {
            return Some(crate::records::document::malformed(
                self.malformed_kind.unwrap_or("record"),
                counts.malformed,
            ));
        }
        if counts.supersedes_cycle > 0 {
            return Some(crate::records::inconsistent(
                "association",
                "supersedes_cycle",
            ));
        }
        if counts.digest_mismatch > 0 {
            return Some(Diagnostic::new(
                codes::INTEGRITY_DIGEST_MISMATCH,
                "A stored object's bytes no longer digest to its own path.",
                Details::new().int("count", counts.digest_mismatch),
            ));
        }
        if counts.length_mismatch > 0 {
            return Some(Diagnostic::new(
                codes::INTEGRITY_LENGTH_MISMATCH,
                "A stored object's length differs from every import event naming it.",
                Details::new().int("count", counts.length_mismatch),
            ));
        }
        if counts.dangling > 0 {
            let dangling = self.dangling.unwrap_or(references::Dangling {
                record_kind: "record",
                reference_kind: "reference",
            });
            return Some(Diagnostic::new(
                codes::INTEGRITY_DANGLING_REFERENCE,
                "A record names a record or object this archive does not hold.",
                Details::new()
                    .text("record_kind", dangling.record_kind)
                    .text("reference_kind", dangling.reference_kind)
                    .int("path_count", counts.dangling),
            ));
        }
        (counts.orphan > 0).then(|| {
            Diagnostic::new(
                codes::INTEGRITY_ORPHAN_OBJECT,
                "A stored object is referenced by no record in this archive.",
                Details::new().int("count", counts.orphan),
            )
        })
    }
}

/// Check one archive from end to end, without writing anything.
///
/// The records are read first, so that the object pass knows what the archive
/// claims, and every reference is resolved last, once both the identifiers
/// and the stored objects are known.
///
///
/// The archive is opened read-only: no missing layout directory is created,
/// nothing is flushed, and a root the user cannot write to is checked exactly
/// like any other. A layout directory that is absent is read as empty; one
/// that is present and could not be listed is counted as unchecked instead,
/// and leaves what it may hold unjudged.
///
/// # Errors
///
/// Returns the refusals of opening an archive: `usage.archive_root_missing`,
/// `usage.arguments`, `path.symlink`, `archive.marker_missing`,
/// `archive.marker_malformed`, `archive.schema_newer`,
/// `archive.schema_older`, `archive.permissions_wide`, or
/// `archive.multiple_filesystems`. A held writer lock is not one of them: the
/// check reads and never waits for a writer.
pub fn check(root: &Path) -> Result<Report> {
    let mut archive = Archive::open_read_only(root).map_err(Failure::new)?;
    let warnings = archive.take_warnings();
    Ok(Outcome {
        data: run(archive.root()),
        warnings,
    })
}

/// The three passes: records, objects, then the references between them.
fn run(root: &Path) -> Report {
    let found = references::collect(root);
    let mut counts = Counts::default();
    let store = store::walk(root, &found, &mut counts);
    let (dangling, first_dangling) = references::dangling(root, &found, &store);
    counts.dangling = dangling;
    counts.malformed = found.malformed.iter().sum();
    counts.supersedes_cycle = found.supersedes_cycles();
    let malformed_kind = references::KINDS
        .iter()
        .zip(found.malformed)
        .find(|(_, count)| *count > 0)
        .map(|(kind, _)| *kind);

    let derived = crate::records::derived::filed(root);
    Report {
        bytes_digested: store.bytes_digested,
        cache_files: crate::cache::file_count(root),
        derived_records: derived.len() as u64,
        derived_orphans: derived_orphans(&derived, &store),
        objects_checked: store.objects_checked,
        orphan_objects: counts.orphan,
        objects_unchecked: store.objects_unchecked,
        problems: problems(&counts),
        records_checked: found.records_checked,
        records_unchecked: found.records_unchecked(),
        records_staging_files: found.records_staging(),
        staging_files: store.staging_files,
        counts,
        malformed_kind,
        dangling: first_dangling,
    }
}

/// How many derived records describe an object the store no longer holds.
///
/// A derived record is filed under the digest of the object it describes, so
/// the question is answered from the digest alone and the file is never
/// opened. Only a store the pass actually listed can say an object is absent:
/// a digest whose fan-out directory could not be read is left uncounted,
/// because an object the check could not look for is not an object the
/// archive does not have.
fn derived_orphans(derived: &std::collections::BTreeSet<String>, store: &store::Store) -> u64 {
    derived
        .iter()
        .filter_map(|digest| references::digest_key(digest))
        .filter(|key| store.holds(key) == Some(false))
        .count() as u64
}

/// Every code the check can report, ordered by code, with its count.
fn problems(counts: &Counts) -> Vec<Problem> {
    let mut problems = vec![
        Problem {
            code: codes::INTEGRITY_DANGLING_REFERENCE,
            count: counts.dangling,
        },
        Problem {
            code: codes::INTEGRITY_DIGEST_MISMATCH,
            count: counts.digest_mismatch,
        },
        Problem {
            code: codes::INTEGRITY_LENGTH_MISMATCH,
            count: counts.length_mismatch,
        },
        Problem {
            code: codes::INTEGRITY_ORPHAN_OBJECT,
            count: counts.orphan,
        },
        Problem {
            code: codes::PATH_SYMLINK,
            count: counts.symlink,
        },
        Problem {
            code: codes::RECORD_INCONSISTENT,
            count: counts.supersedes_cycle,
        },
        Problem {
            code: codes::RECORD_MALFORMED,
            count: counts.malformed,
        },
    ];
    problems.sort_by_key(|problem| problem.code);
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_report_names_every_code_with_a_count_of_zero() {
        let report = run(tempfile::tempdir().unwrap().path());
        assert!(report.is_clean());
        assert_eq!(report.first_problem(), None);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(report.problems.len(), 7);
        assert!(report.problems.iter().all(|problem| problem.count == 0));
        let codes: Vec<&str> = report.problems.iter().map(|problem| problem.code).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "the problems array is ordered by code");
        assert_eq!(report.counts().total(), 0);
    }

    #[test]
    fn each_problem_reports_counts_and_kinds_and_never_a_path() {
        let base = run(tempfile::tempdir().unwrap().path());
        for (counts, code) in [
            (
                Counts {
                    symlink: 1,
                    ..Counts::default()
                },
                codes::PATH_SYMLINK,
            ),
            (
                Counts {
                    malformed: 2,
                    ..Counts::default()
                },
                codes::RECORD_MALFORMED,
            ),
            (
                Counts {
                    supersedes_cycle: 7,
                    ..Counts::default()
                },
                codes::RECORD_INCONSISTENT,
            ),
            (
                Counts {
                    digest_mismatch: 3,
                    ..Counts::default()
                },
                codes::INTEGRITY_DIGEST_MISMATCH,
            ),
            (
                Counts {
                    length_mismatch: 4,
                    ..Counts::default()
                },
                codes::INTEGRITY_LENGTH_MISMATCH,
            ),
            (
                Counts {
                    dangling: 5,
                    ..Counts::default()
                },
                codes::INTEGRITY_DANGLING_REFERENCE,
            ),
            (
                Counts {
                    orphan: 6,
                    ..Counts::default()
                },
                codes::INTEGRITY_ORPHAN_OBJECT,
            ),
        ] {
            let report = Report {
                counts,
                ..base.clone()
            };
            let problem = report.first_problem().expect("a problem is reported");
            assert_eq!(problem.code, code);
            assert_eq!(problem.exit_code(), report.exit_code());
            assert!(!problem.is_retryable());
            let json = serde_json::to_value(&problem).unwrap();
            let details = json["details"].as_object().unwrap();
            assert!(details.len() <= 16);
            assert!(details.contains_key("bucket"));
            assert!(
                !json.to_string().contains("objects/sha256"),
                "no path reaches a diagnostic"
            );
        }
    }

    #[test]
    fn precedence_is_fixed_so_one_archive_always_reports_one_code() {
        let base = run(tempfile::tempdir().unwrap().path());
        let every = Counts {
            symlink: 1,
            malformed: 1,
            supersedes_cycle: 1,
            digest_mismatch: 1,
            length_mismatch: 1,
            dangling: 1,
            orphan: 1,
        };
        assert_eq!(every.total(), 7);
        let mut counts = every;
        for expected in [
            codes::PATH_SYMLINK,
            codes::RECORD_MALFORMED,
            codes::RECORD_INCONSISTENT,
            codes::INTEGRITY_DIGEST_MISMATCH,
            codes::INTEGRITY_LENGTH_MISMATCH,
            codes::INTEGRITY_DANGLING_REFERENCE,
            codes::INTEGRITY_ORPHAN_OBJECT,
        ] {
            let report = Report {
                counts,
                ..base.clone()
            };
            assert_eq!(report.first_problem().unwrap().code, expected);
            match expected {
                codes::PATH_SYMLINK => counts.symlink = 0,
                codes::RECORD_MALFORMED => counts.malformed = 0,
                codes::RECORD_INCONSISTENT => counts.supersedes_cycle = 0,
                codes::INTEGRITY_DIGEST_MISMATCH => counts.digest_mismatch = 0,
                codes::INTEGRITY_LENGTH_MISMATCH => counts.length_mismatch = 0,
                codes::INTEGRITY_DANGLING_REFERENCE => counts.dangling = 0,
                _ => counts.orphan = 0,
            }
        }
        let report = Report {
            counts,
            ..base.clone()
        };
        assert_eq!(report.first_problem(), None);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(
            Report {
                counts: Counts {
                    symlink: 1,
                    ..Counts::default()
                },
                ..base.clone()
            }
            .exit_code(),
            3,
            "a link alone exits with its own path bucket"
        );
        assert_eq!(
            Report {
                counts: every,
                ..base
            }
            .exit_code(),
            4,
            "several conditions exit with the highest group"
        );
    }

    #[test]
    fn an_archive_that_cannot_be_opened_is_a_refusal_rather_than_a_report() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            check(root.path()).unwrap_err().error.code,
            codes::ARCHIVE_MARKER_MISSING
        );
    }
}
