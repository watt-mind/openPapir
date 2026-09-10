//! The record pass of the integrity check: what the archive's records claim.
//!
//! The pass reads every record directory with the bounded, no-follow reader
//! the record commands already use, and keeps only fixed-size keys: a
//! 32-byte digest and a 16-byte record identifier. No filename, no title, no
//! statement, and no path is retained, so the memory the check needs grows
//! with the number of records and never with their content.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::archive::import::ImportEvent;
use crate::integrity::store::Store;
use crate::records::association::{self, Association};
use crate::records::case::Case;
use crate::records::document::{self, Visited, is_identifier};
use crate::records::receipt::Receipt;
use crate::records::submission::Submission;
use crate::records::{DIGEST_PREFIX, is_digest};

/// A stored artefact's digest as a fixed-size key.
pub type DigestKey = [u8; 32];
/// A record identifier as a fixed-size key.
pub type IdKey = [u8; 16];

/// How a record kind is named in a diagnostic, in a fixed order.
pub const KINDS: [&str; 5] = [
    "import_event",
    "case",
    "submission",
    "receipt",
    "association",
];

/// Parse a lowercase hexadecimal string into a fixed-size key.
fn key<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }
    let bytes = hex.as_bytes();
    let mut out = [0_u8; N];
    for (index, slot) in out.iter_mut().enumerate() {
        let pair = std::str::from_utf8(&bytes[index * 2..index * 2 + 2]).ok()?;
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

/// The key of an algorithm-qualified digest a record names.
#[must_use]
pub fn digest_key(value: &str) -> Option<DigestKey> {
    let hex = value
        .strip_prefix(DIGEST_PREFIX)
        .filter(|hex| is_digest(hex))?;
    key::<32>(hex)
}

/// The key of a bare, lowercase hexadecimal object name.
#[must_use]
pub fn object_key(hex: &str) -> Option<DigestKey> {
    is_digest(hex).then(|| key::<32>(hex)).flatten()
}

/// The key of a record identifier.
#[must_use]
pub fn id_key(value: &str) -> Option<IdKey> {
    is_identifier(value).then(|| key::<16>(value)).flatten()
}

/// One dangling reference: the kind that held it and how it was named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dangling {
    /// The record kind whose document held the reference.
    pub record_kind: &'static str,
    /// How the reference was named, such as `artefact_digest`.
    pub reference_kind: &'static str,
}

/// What the records of an archive claim, as fixed-size keys and counts.
#[derive(Debug, Default)]
pub struct References {
    /// Every byte length an import event records for a digest.
    pub lengths: BTreeMap<DigestKey, BTreeSet<u64>>,
    /// Every digest an import event, a receipt, or a submission names.
    pub referenced: BTreeSet<DigestKey>,
    /// The identifiers of the records of each kind, in the order of [`KINDS`].
    pub ids: [BTreeSet<IdKey>; 5],
    /// The record each association says it supersedes, by the association's
    /// own identifier. Two fixed-size keys per edge and nothing else, so the
    /// supersession graph costs the check what a pair of identifiers costs.
    pub supersedes: BTreeMap<IdKey, IdKey>,
    /// How many documents of each kind could not be read as a record.
    pub malformed: [u64; 5],
    /// How many leftover staging files each kind's directory holds, in the
    /// order of [`KINDS`]. A staging file is openPapir's own transient
    /// artefact from an interrupted write, so it is neither a record nor a
    /// malformed one, and the check leaves it exactly where it is.
    pub staging: [u64; 5],
    /// Whether each kind's directory could not be listed, in the order of
    /// [`KINDS`]. A directory that is not there is not one of these: it
    /// genuinely holds no records.
    pub unchecked: [bool; 5],
    /// How many record documents were examined, readable or not.
    pub records_checked: u64,
}

impl References {
    /// Whether the archive holds a record of this kind: `Some(true)` when the
    /// identifier was read, `Some(false)` when the directory was read and does
    /// not hold it, and `None` when the directory could not be listed.
    ///
    /// The third answer mirrors [`Store::holds`]: a record the check could not
    /// look for is not a record the archive does not hold, so a reference to
    /// it is left unjudged rather than reported as dangling.
    fn holds(&self, kind: usize, id: &str) -> Option<bool> {
        if self.unchecked[kind] {
            return None;
        }
        Some(id_key(id).is_some_and(|key| self.ids[kind].contains(&key)))
    }

    /// How many record directories could not be listed.
    #[must_use]
    pub fn records_unchecked(&self) -> u64 {
        self.unchecked
            .iter()
            .filter(|unchecked| **unchecked)
            .count() as u64
    }

    /// How many leftover staging files the record directories hold together.
    #[must_use]
    pub fn records_staging(&self) -> u64 {
        self.staging.iter().sum()
    }

    /// Whether a stored object that no record names may be called an orphan.
    ///
    /// Only an import event, a receipt, or a submission references an object,
    /// so if any of those three directories could not be listed the check
    /// cannot say that nothing references a digest: the records that would
    /// were never read.
    #[must_use]
    pub fn may_judge_orphans(&self) -> bool {
        !self.unchecked[IMPORTS] && !self.unchecked[SUBMISSIONS] && !self.unchecked[RECEIPTS]
    }

    /// Whether a stored object is referenced by no record the check read.
    #[must_use]
    pub fn orphaned(&self, key: &DigestKey) -> bool {
        self.may_judge_orphans() && !self.referenced.contains(key)
    }

    /// How many `supersedes` cycles the stored association records form.
    ///
    /// The walk is [`association::supersession_cycles`], shared with the
    /// deletion planner so that the two agree on what a cycle is. The keys
    /// this pass holds are record identifiers, so a cycle here is counted
    /// wherever it sits rather than tied to any one command.
    ///
    /// The count is of cycles rather than of the records in them, and it is
    /// the whole of what the check reports: naming a record would say which
    /// of the user's assertions is the damaged one
    /// (`docs/error-contract.md`).
    #[must_use]
    pub fn supersedes_cycles(&self) -> u64 {
        association::supersession_cycles(&self.supersedes).len() as u64
    }
}

/// Which slot of [`KINDS`] a record kind occupies.
const IMPORTS: usize = 0;
const CASES: usize = 1;
const SUBMISSIONS: usize = 2;
const RECEIPTS: usize = 3;
const ASSOCIATIONS: usize = 4;

/// Read every record and keep only the keys the object pass needs.
///
/// A record directory that is not there holds no records. One that is there
/// and could not be listed is recorded as unchecked instead, so that nothing
/// is concluded from records the check never read. A leftover staging file is
/// counted per kind and left where it is: it is openPapir's own transient
/// artefact rather than a record, and the check writes nothing.
#[must_use]
pub fn collect(root: &Path) -> References {
    let mut found = References::default();
    let count = |records: u64, visited: Visited, kind: usize, found: &mut References| {
        found.records_checked += records + visited.unreadable;
        found.malformed[kind] = visited.unreadable;
        found.staging[kind] = visited.staging;
        found.unchecked[kind] = visited.unchecked;
    };

    let mut records = 0_u64;
    let visited = document::visit_records_checked::<ImportEvent, _>(root, |event| {
        records += 1;
        if let Some(id) = id_key(&event.id) {
            found.ids[IMPORTS].insert(id);
        }
        if let Some(digest) = digest_key(&event.digest) {
            found.referenced.insert(digest);
            found
                .lengths
                .entry(digest)
                .or_default()
                .insert(event.byte_length);
        }
    });
    count(records, visited, IMPORTS, &mut found);

    let mut records = 0_u64;
    let visited = document::visit_records_checked::<Case, _>(root, |case| {
        records += 1;
        if let Some(id) = id_key(&case.id) {
            found.ids[CASES].insert(id);
        }
    });
    count(records, visited, CASES, &mut found);

    let mut records = 0_u64;
    let visited = document::visit_records_checked::<Submission, _>(root, |submission| {
        records += 1;
        if let Some(id) = id_key(&submission.id) {
            found.ids[SUBMISSIONS].insert(id);
        }
        for artefact in &submission.artefacts {
            if let Some(digest) = digest_key(&artefact.digest) {
                found.referenced.insert(digest);
            }
        }
    });
    count(records, visited, SUBMISSIONS, &mut found);

    let mut records = 0_u64;
    let visited = document::visit_records_checked::<Receipt, _>(root, |receipt| {
        records += 1;
        if let Some(id) = id_key(&receipt.id) {
            found.ids[RECEIPTS].insert(id);
        }
        if let Some(digest) = digest_key(&receipt.artefact_digest) {
            found.referenced.insert(digest);
        }
    });
    count(records, visited, RECEIPTS, &mut found);

    let mut records = 0_u64;
    let visited = document::visit_records_checked::<Association, _>(root, |association| {
        records += 1;
        if let Some(id) = id_key(&association.id) {
            found.ids[ASSOCIATIONS].insert(id);
            if let Some(previous) = association.supersedes.as_deref().and_then(id_key) {
                found.supersedes.insert(id, previous);
            }
        }
    });
    count(records, visited, ASSOCIATIONS, &mut found);

    found
}

/// Count the references that name a record or object the archive does not
/// hold, in a fixed order, keeping only the first one's kinds.
///
/// A digest whose fan-out directory could not be listed is not counted: the
/// check could not look for it, which is not the same as the archive not
/// holding it. A reference into a record directory that could not be listed
/// is left unjudged for the same reason.
///
/// The pass reads the records a second time rather than holding them, because
/// a reference can only be judged once every identifier is known.
#[must_use]
pub fn dangling(root: &Path, found: &References, objects: &Store) -> (u64, Option<Dangling>) {
    let mut count = 0_u64;
    let mut first = None;
    let mut note = |record_kind: &'static str, reference_kind: &'static str| {
        count += 1;
        if first.is_none() {
            first = Some(Dangling {
                record_kind,
                reference_kind,
            });
        }
    };

    // A digest the store could not be searched for is left uncounted: an
    // object the check could not look for is not an object the archive does
    // not hold.
    let missing = |digest: &str| {
        digest_key(digest).map_or(Some(true), |key| objects.holds(&key).map(|held| !held))
    };
    let stored = |digest: &str| missing(digest) != Some(true);

    document::visit_records::<ImportEvent, _>(root, |event| {
        if !stored(&event.digest) {
            note("import_event", "artefact_digest");
        }
    });
    document::visit_records::<Submission, _>(root, |submission| {
        if found.holds(CASES, &submission.case_id) == Some(false) {
            note("submission", "case_id");
        }
        for artefact in &submission.artefacts {
            if !stored(&artefact.digest) {
                note("submission", "artefact_digest");
            }
        }
    });
    document::visit_records::<Receipt, _>(root, |receipt| {
        if !stored(&receipt.artefact_digest) {
            note("receipt", "artefact_digest");
        }
        if found.holds(IMPORTS, &receipt.import_event_id) == Some(false) {
            note("receipt", "import_event_id");
        }
    });
    document::visit_records::<Association, _>(root, |association| {
        if found.holds(RECEIPTS, &association.receipt_id) == Some(false) {
            note("association", "receipt_id");
        }
        if let Some(submission_id) = &association.submission_id
            && found.holds(SUBMISSIONS, submission_id) == Some(false)
        {
            note("association", "submission_id");
        }
        for candidate in &association.candidates {
            if found.holds(SUBMISSIONS, &candidate.submission_id) == Some(false) {
                note("association", "submission_id");
            }
        }
        if let Some(supersedes) = &association.supersedes
            && found.holds(ASSOCIATIONS, supersedes) == Some(false)
        {
            note("association", "association_id");
        }
    });

    (count, first)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_key_is_fixed_size_and_rejects_anything_else() {
        assert_eq!(std::mem::size_of::<DigestKey>(), 32);
        assert_eq!(std::mem::size_of::<IdKey>(), 16);
        assert!(digest_key(&format!("sha256:{DIGEST}")).is_some());
        assert!(digest_key(DIGEST).is_none(), "the prefix is required");
        assert!(digest_key("sha256:not-a-digest").is_none());
        assert!(object_key(DIGEST).is_some());
        assert!(object_key("../..").is_none());
        assert!(id_key(ID).is_some());
        assert!(id_key("0123456789ABCDEF0123456789ABCDEF").is_none());
        assert!(id_key("").is_none());
    }

    #[test]
    fn an_empty_archive_claims_nothing() {
        let root = tempfile::tempdir().unwrap();
        let found = collect(root.path());
        assert_eq!(found.records_checked, 0);
        assert!(found.referenced.is_empty());
        assert_eq!(found.malformed, [0; 5]);
        assert_eq!(found.staging, [0; 5]);
        assert_eq!(found.records_staging(), 0);
        let (count, first) = dangling(root.path(), &found, &Store::default());
        assert_eq!(count, 0);
        assert_eq!(first, None);
        assert_eq!(KINDS.len(), 5);
        assert_eq!(found.records_unchecked(), 0);
        assert!(found.may_judge_orphans());
        assert!(found.orphaned(&object_key(DIGEST).unwrap()));
    }

    #[test]
    fn a_leftover_staging_file_in_a_record_directory_is_counted_not_malformed() {
        use crate::records::document::Record as _;

        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Case::DIRECTORY);
        std::fs::create_dir_all(&directory).unwrap();
        let staging = directory.join(".papir-staging-abc");
        std::fs::write(&staging, b"partial").unwrap();
        let found = collect(root.path());
        assert_eq!(found.staging[CASES], 1);
        assert_eq!(found.records_staging(), 1);
        assert_eq!(found.malformed, [0; 5], "a staging file is not a record");
        assert_eq!(found.records_checked, 0);
        assert_eq!(found.records_unchecked(), 0);
        assert!(staging.exists(), "the check deletes nothing");
        assert_eq!(References::default().records_staging(), 0);
    }

    /// The supersession graph has out-degree one, so the counter has to tell
    /// a walk that ends from one that closes, count a cycle once however many
    /// records lead into it, and count two separate cycles separately.
    #[test]
    fn a_closed_supersession_walk_is_counted_once_and_a_finite_one_never() {
        let key = |seed: u8| id_key(&format!("{seed:02x}").repeat(16)).unwrap();
        let graph = |edges: &[(u8, u8)]| References {
            supersedes: edges
                .iter()
                .map(|(from, to)| (key(*from), key(*to)))
                .collect(),
            ..References::default()
        };
        assert_eq!(References::default().supersedes_cycles(), 0);
        assert_eq!(
            graph(&[(3, 2), (2, 1)]).supersedes_cycles(),
            0,
            "an ordinary history ends at the record it supersedes"
        );
        assert_eq!(
            graph(&[(2, 1), (1, 2)]).supersedes_cycles(),
            1,
            "two records that supersede each other"
        );
        assert_eq!(
            graph(&[(1, 1)]).supersedes_cycles(),
            1,
            "a record on itself"
        );
        assert_eq!(
            graph(&[(4, 1), (3, 1), (2, 1), (1, 2)]).supersedes_cycles(),
            1,
            "a cycle several live records lead into is still one cycle"
        );
        assert_eq!(
            graph(&[(1, 2), (2, 1), (3, 4), (4, 3)]).supersedes_cycles(),
            2,
            "two cycles are two"
        );
        assert_eq!(
            graph(&[(1, 9)]).supersedes_cycles(),
            0,
            "a reference nothing stored answers is dangling, not a cycle"
        );
    }

    #[test]
    fn a_kind_that_could_not_be_listed_leaves_what_it_would_name_unjudged() {
        let key = object_key(DIGEST).unwrap();
        for kind in [IMPORTS, SUBMISSIONS, RECEIPTS] {
            let mut found = References::default();
            found.unchecked[kind] = true;
            assert!(!found.may_judge_orphans(), "{}", KINDS[kind]);
            assert!(
                !found.orphaned(&key),
                "an object whose referencing records were not read is not an orphan"
            );
            assert_eq!(found.records_unchecked(), 1);
            assert_eq!(found.holds(kind, ID), None);
        }
        for kind in [CASES, ASSOCIATIONS] {
            let mut found = References::default();
            found.unchecked[kind] = true;
            assert!(
                found.may_judge_orphans(),
                "no object is referenced from {}",
                KINDS[kind]
            );
            assert!(found.orphaned(&key));
            assert_eq!(found.holds(kind, ID), None, "the kind was not read");
        }
        let found = References::default();
        assert_eq!(found.holds(CASES, ID), Some(false), "the kind was read");
        let mut found = References::default();
        found.ids[CASES].insert(id_key(ID).unwrap());
        assert_eq!(found.holds(CASES, ID), Some(true));
        assert_eq!(
            References {
                unchecked: [true; 5],
                ..References::default()
            }
            .records_unchecked(),
            5
        );
    }
}
