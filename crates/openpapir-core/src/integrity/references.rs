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
use crate::records::association::Association;
use crate::records::case::Case;
use crate::records::document::{self, is_identifier};
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
    /// How many documents of each kind could not be read as a record.
    pub malformed: [u64; 5],
    /// How many record documents were examined, readable or not.
    pub records_checked: u64,
}

impl References {
    fn holds(&self, kind: usize, id: &str) -> bool {
        id_key(id).is_some_and(|key| self.ids[kind].contains(&key))
    }
}

/// Which slot of [`KINDS`] a record kind occupies.
const IMPORTS: usize = 0;
const CASES: usize = 1;
const SUBMISSIONS: usize = 2;
const RECEIPTS: usize = 3;
const ASSOCIATIONS: usize = 4;

/// Read every record and keep only the keys the object pass needs.
#[must_use]
pub fn collect(root: &Path) -> References {
    let mut found = References::default();
    let count = |records: u64, unreadable: u64, kind: usize, found: &mut References| {
        found.records_checked += records + unreadable;
        found.malformed[kind] = unreadable;
    };

    let mut records = 0_u64;
    let unreadable = document::visit_records::<ImportEvent, _>(root, |event| {
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
    count(records, unreadable, IMPORTS, &mut found);

    let mut records = 0_u64;
    let unreadable = document::visit_records::<Case, _>(root, |case| {
        records += 1;
        if let Some(id) = id_key(&case.id) {
            found.ids[CASES].insert(id);
        }
    });
    count(records, unreadable, CASES, &mut found);

    let mut records = 0_u64;
    let unreadable = document::visit_records::<Submission, _>(root, |submission| {
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
    count(records, unreadable, SUBMISSIONS, &mut found);

    let mut records = 0_u64;
    let unreadable = document::visit_records::<Receipt, _>(root, |receipt| {
        records += 1;
        if let Some(id) = id_key(&receipt.id) {
            found.ids[RECEIPTS].insert(id);
        }
        if let Some(digest) = digest_key(&receipt.artefact_digest) {
            found.referenced.insert(digest);
        }
    });
    count(records, unreadable, RECEIPTS, &mut found);

    let mut records = 0_u64;
    let unreadable = document::visit_records::<Association, _>(root, |association| {
        records += 1;
        if let Some(id) = id_key(&association.id) {
            found.ids[ASSOCIATIONS].insert(id);
        }
    });
    count(records, unreadable, ASSOCIATIONS, &mut found);

    found
}

/// Count the references that name a record or object the archive does not
/// hold, in a fixed order, keeping only the first one's kinds.
///
/// A digest whose fan-out directory could not be listed is not counted: the
/// check could not look for it, which is not the same as the archive not
/// holding it.
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
        if !found.holds(CASES, &submission.case_id) {
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
        if !found.holds(IMPORTS, &receipt.import_event_id) {
            note("receipt", "import_event_id");
        }
    });
    document::visit_records::<Association, _>(root, |association| {
        if !found.holds(RECEIPTS, &association.receipt_id) {
            note("association", "receipt_id");
        }
        if let Some(submission_id) = &association.submission_id
            && !found.holds(SUBMISSIONS, submission_id)
        {
            note("association", "submission_id");
        }
        for candidate in &association.candidates {
            if !found.holds(SUBMISSIONS, &candidate.submission_id) {
                note("association", "submission_id");
            }
        }
        if let Some(supersedes) = &association.supersedes
            && !found.holds(ASSOCIATIONS, supersedes)
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
        let (count, first) = dangling(root.path(), &found, &Store::default());
        assert_eq!(count, 0);
        assert_eq!(first, None);
        assert_eq!(KINDS.len(), 5);
    }
}
