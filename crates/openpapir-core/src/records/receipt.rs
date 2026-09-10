//! Receipt records: an imported artefact the user believes to be a receipt.
//!
//! A receipt references its artefact by digest and the import event that
//! introduced it by identifier. It never rewrites the artefact and asserts
//! nothing about the file's type or authenticity: it records what the user
//! said when importing (`docs/archive-layout.md`). openPapir parses no
//! artefact bytes, so nothing here inspects what the file contains.
//!
//! Adding a receipt takes the archive's single-writer lock and runs the
//! archive's permission checks first. Listing takes no lock, because every
//! record file is written whole, so a reader sees one complete document or
//! another and never a partial one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::archive::import::ImportEvent;
use crate::archive::lock::WriterLock;
use crate::archive::{Archive, SUPPORTED_SCHEMA_VERSION};
use crate::clock;
use crate::error::{Diagnostic, Failure, Outcome, Result, Warning};
use crate::ident;
use crate::records::association::{self, Association};
use crate::records::document::{self, Record};
use crate::records::{
    RECEIPTS_DIR, checked_digest, checked_label, inconsistent, refuse_absent_object,
};

/// The value a receipt record carries in `record_kind`.
pub const KIND: &str = "receipt";

/// One receipt record, stored as `records/receipts/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// The archive schema version the record was written under.
    pub archive_schema_version: u32,
    /// The algorithm-qualified digest of the artefact, stored in this archive.
    pub artefact_digest: String,
    /// When openPapir recorded the receipt.
    pub created_at: String,
    /// The receipt's own identifier, minted by openPapir.
    pub id: String,
    /// The import event that introduced the artefact.
    pub import_event_id: String,
    /// The user's own label, absent when none was supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The record kind, always `receipt`.
    pub record_kind: String,
}

impl Record for Receipt {
    const KIND: &'static str = KIND;
    const DIRECTORY: &'static str = RECEIPTS_DIR;

    fn id(&self) -> &str {
        &self.id
    }

    fn record_kind(&self) -> &str {
        &self.record_kind
    }
}

/// What adding a receipt reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptAdded {
    /// The receipt as it was stored.
    pub receipt: Receipt,
}

/// What showing one receipt reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptView {
    /// Every association about the receipt, newest first and superseded
    /// records included, exactly as `association list` reports them. History
    /// is never collapsed or filtered.
    pub associations: Vec<Association>,
    /// How many associations the receipt holds.
    pub association_count: u64,
    /// The receipt itself, as stored.
    pub receipt: Receipt,
}

/// What listing receipts reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptList {
    /// How many receipts the archive holds.
    pub count: u64,
    /// What a derived-metadata record says about the artefacts the receipts
    /// below reference, ordered by digest. A receipt whose artefact has no
    /// derived record contributes no entry, and nothing here says an artefact
    /// is a receipt: a media type is a statement about leading bytes, and the
    /// receipt is the user's own assertion exactly as it was before.
    pub derived: Vec<crate::records::derived::DerivedFacts>,
    /// Every receipt in the archive, ordered by identifier.
    pub receipts: Vec<Receipt>,
}

/// Record a receipt against an artefact already stored in the archive.
///
/// `import_event` names the import event to record. When it is absent the
/// earliest import event for the digest is used, because an artefact may have
/// been imported more than once and that history is meaningful rather than an
/// anomaly. The label cap is checked before the archive is opened.
///
/// # Errors
///
/// Returns `input.cap.field_length` or `usage.arguments` for a label or a
/// digest that breaks its cap or shape, `record.not_found` when the digest
/// names no stored object or the identifier names no import event,
/// `record.inconsistent` when a named import event records another artefact,
/// `record.malformed` for an unreadable import-event document, and any
/// archive, lock, path, or write refusal of `docs/error-contract.md`.
pub fn add(
    root: &Path,
    artefact: &str,
    import_event: Option<&str>,
    label: Option<&str>,
) -> Result<ReceiptAdded> {
    let mut warnings = Vec::new();
    match add_record(root, artefact, import_event, label, &mut warnings) {
        Ok(receipt) => Ok(Outcome {
            data: ReceiptAdded { receipt },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn add_record(
    root: &Path,
    artefact: &str,
    import_event: Option<&str>,
    label: Option<&str>,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Receipt, Diagnostic> {
    let artefact_digest = checked_digest("artefact", artefact)?;
    let label = checked_label(label)?;

    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let _lock = WriterLock::acquire(archive.root())?;
    refuse_absent_object(archive.root(), &artefact_digest)?;
    let import_event_id = resolve_import_event(archive.root(), &artefact_digest, import_event)?;

    let receipt = Receipt {
        archive_schema_version: SUPPORTED_SCHEMA_VERSION,
        artefact_digest,
        created_at: clock::now_rfc3339(),
        id: ident::new_id()?,
        import_event_id,
        label,
        record_kind: KIND.to_owned(),
    };
    warnings.extend(document::write_record(archive.root(), &receipt)?);
    Ok(receipt)
}

/// Find the import event a receipt records, by name or by history.
///
/// A named event must exist and must record this artefact. An unnamed one is
/// the earliest event for the digest, by `imported_at` and then by identifier
/// so that the choice is the same on every run.
fn resolve_import_event(
    root: &Path,
    digest: &str,
    named: Option<&str>,
) -> std::result::Result<String, Diagnostic> {
    if let Some(named) = named {
        let event = document::read_record::<ImportEvent>(root, named, "import_event_id")?;
        if event.digest != digest {
            return Err(inconsistent(KIND, rules::IMPORT_EVENT_DIGEST_MISMATCH));
        }
        return Ok(event.id);
    }
    document::list_records::<ImportEvent>(root)?
        .into_iter()
        .filter(|event| event.digest == digest)
        .min_by(|left, right| (&left.imported_at, &left.id).cmp(&(&right.imported_at, &right.id)))
        .map(|event| event.id)
        .ok_or_else(|| document::not_found("import_event", "artefact_digest"))
}

/// The consistency rules a receipt can break, as stable `rule` values.
pub mod rules {
    /// A named import event records another artefact than the one supplied.
    pub const IMPORT_EVENT_DIGEST_MISMATCH: &str = "import_event_digest_mismatch";
}

/// List every receipt in the archive at `root`, without taking the lock.
///
/// # Errors
///
/// Returns any archive refusal, or `record.malformed` when a stored document
/// cannot be read as a receipt.
pub fn list(root: &Path) -> Result<ReceiptList> {
    let mut warnings = Vec::new();
    match list_records(root, &mut warnings) {
        Ok((receipts, derived)) => Ok(Outcome {
            data: ReceiptList {
                count: receipts.len() as u64,
                derived,
                receipts,
            },
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

type Listing = (Vec<Receipt>, Vec<crate::records::derived::DerivedFacts>);

fn list_records(
    root: &Path,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<Listing, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let receipts = document::list_records::<Receipt>(archive.root())?;
    let derived = crate::records::derived::facts_for(
        archive.root(),
        receipts
            .iter()
            .map(|receipt| receipt.artefact_digest.as_str()),
    );
    Ok((receipts, derived))
}

/// Show one receipt with its whole association history, without the lock.
///
/// The history is the one `association list` reports for the same receipt:
/// newest first, superseded records included, nothing collapsed and nothing
/// filtered. The receipt itself is reported exactly as it is stored.
///
/// # Errors
///
/// Returns `record.not_found` when the identifier names no receipt,
/// `record.malformed` for an unreadable document, and any archive refusal.
pub fn show(root: &Path, receipt_id: &str) -> Result<ReceiptView> {
    let mut warnings = Vec::new();
    match show_record(root, receipt_id, &mut warnings) {
        Ok(view) => Ok(Outcome {
            data: view,
            warnings,
        }),
        Err(error) => Err(Failure::with_warnings(error, warnings)),
    }
}

fn show_record(
    root: &Path,
    receipt_id: &str,
    warnings: &mut Vec<Warning>,
) -> std::result::Result<ReceiptView, Diagnostic> {
    let mut archive = Archive::open(root)?;
    warnings.extend(archive.take_warnings());
    let receipt = document::read_record::<Receipt>(archive.root(), receipt_id, "receipt_id")?;
    let associations = association::history_of(archive.root(), &receipt.id)?;
    Ok(ReceiptView {
        association_count: associations.len() as u64,
        associations,
        receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive;
    use crate::archive::IMPORTS_DIR;
    use crate::error::codes;
    use std::fs;
    use std::path::PathBuf;

    const PAYLOAD: &[u8] = b"synthetic bytes\n";
    const PAYLOAD_DIGEST: &str =
        "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";
    const ABSENT_DIGEST: &str =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    const ABSENT_ID: &str = "0123456789abcdef0123456789abcdef";

    /// An archive holding one imported artefact, imported `times` times.
    fn archive_with_artefact(times: usize) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        archive::init(root.path()).unwrap();
        let inputs = tempfile::tempdir().unwrap();
        let file = inputs.path().join("note.txt");
        fs::write(&file, PAYLOAD).unwrap();
        for _ in 0..times {
            archive::import::import(root.path(), &[PathBuf::from(&file)]).unwrap();
        }
        root
    }

    /// Showing a receipt reports the record as stored beside the history
    /// `association list` reports, and needs no writer lock to do it.
    #[test]
    fn a_shown_receipt_carries_its_record_and_its_history_without_the_lock() {
        let root = archive_with_artefact(1);
        let receipt = add(root.path(), PAYLOAD_DIGEST, None, Some("Envelope"))
            .unwrap()
            .data
            .receipt;
        let _held = WriterLock::acquire(root.path()).unwrap();
        let view = show(root.path(), &receipt.id).unwrap().data;
        assert_eq!(view.receipt, receipt);
        assert_eq!(view.association_count, 0);
        assert!(view.associations.is_empty());

        let refusal = show(root.path(), ABSENT_ID).unwrap_err().error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "receipt");
        assert_eq!(json["details"]["reference_kind"], "receipt_id");
    }

    #[test]
    fn a_receipt_names_its_artefact_and_its_import_event() {
        let root = archive_with_artefact(1);
        let receipt = add(root.path(), PAYLOAD_DIGEST, None, Some("Envelope"))
            .unwrap()
            .data
            .receipt;
        assert_eq!(receipt.artefact_digest, PAYLOAD_DIGEST);
        assert_eq!(receipt.label.as_deref(), Some("Envelope"));
        assert_eq!(receipt.record_kind, KIND);
        assert_eq!(receipt.archive_schema_version, 1);
        assert_eq!(receipt.id.len(), 32);
        assert_eq!(receipt.import_event_id.len(), 32);
        assert!(receipt.created_at.ends_with('Z'));

        let listed = list(root.path()).unwrap().data;
        assert_eq!(listed.count, 1);
        assert_eq!(listed.receipts[0], receipt);

        let stored = fs::read_to_string(
            root.path()
                .join(RECEIPTS_DIR)
                .join(format!("{}.json", receipt.id)),
        )
        .unwrap();
        assert!(stored.ends_with("}\n"), "one LF-terminated document");
        assert!(stored.starts_with("{\"archive_schema_version\""), "sorted");
    }

    #[test]
    fn a_receipt_without_a_label_omits_the_field_entirely() {
        let root = archive_with_artefact(1);
        let receipt = add(root.path(), PAYLOAD_DIGEST, None, None)
            .unwrap()
            .data
            .receipt;
        assert_eq!(receipt.label, None);
        let stored = fs::read_to_string(
            root.path()
                .join(RECEIPTS_DIR)
                .join(format!("{}.json", receipt.id)),
        )
        .unwrap();
        assert!(!stored.contains("label"));
    }

    #[test]
    fn the_earliest_import_event_is_recorded_when_none_is_named() {
        let root = archive_with_artefact(3);
        let events = document::list_records::<ImportEvent>(root.path()).unwrap();
        assert_eq!(events.len(), 3, "a re-import is history, not an anomaly");
        let earliest = events
            .iter()
            .min_by(|left, right| {
                (&left.imported_at, &left.id).cmp(&(&right.imported_at, &right.id))
            })
            .unwrap();
        let receipt = add(root.path(), PAYLOAD_DIGEST, None, None)
            .unwrap()
            .data
            .receipt;
        assert_eq!(receipt.import_event_id, earliest.id);

        let named = events.iter().find(|event| event.id != earliest.id).unwrap();
        let receipt = add(root.path(), PAYLOAD_DIGEST, Some(&named.id), None)
            .unwrap()
            .data
            .receipt;
        assert_eq!(receipt.import_event_id, named.id, "the user's choice wins");
    }

    #[test]
    fn an_unknown_digest_or_import_event_is_refused_without_echoing_it() {
        let root = archive_with_artefact(1);
        let refusal = add(root.path(), ABSENT_DIGEST, None, None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        assert_eq!(refusal.exit_code(), 4);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "artefact");
        assert_eq!(json["details"]["reference_kind"], "artefact_digest");

        let refusal = add(root.path(), PAYLOAD_DIGEST, Some(ABSENT_ID), None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "import_event");
        assert_eq!(json["details"]["reference_kind"], "import_event_id");
        assert_eq!(
            fs::read_dir(root.path().join(RECEIPTS_DIR))
                .unwrap()
                .count(),
            0,
            "no record survives a refused reference"
        );
    }

    #[test]
    fn an_import_event_for_another_artefact_is_inconsistent() {
        let root = archive_with_artefact(1);
        let inputs = tempfile::tempdir().unwrap();
        let other = inputs.path().join("other.txt");
        fs::write(&other, b"other synthetic bytes\n").unwrap();
        let imported = archive::import::import(root.path(), &[PathBuf::from(&other)]).unwrap();
        let other_event = imported.data.artefacts[0].import_event.clone();

        let refusal = add(root.path(), PAYLOAD_DIGEST, Some(&other_event), None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_INCONSISTENT);
        assert_eq!(refusal.exit_code(), 4);
        assert!(!refusal.is_retryable());
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "receipt");
        assert_eq!(json["details"]["rule"], "import_event_digest_mismatch");
        assert_eq!(json["details"].as_object().unwrap().len(), 3);
    }

    #[test]
    fn a_stored_object_with_no_import_event_has_no_receipt_to_record() {
        let root = archive_with_artefact(1);
        for entry in fs::read_dir(root.path().join(IMPORTS_DIR)).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let refusal = add(root.path(), PAYLOAD_DIGEST, None, None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["record_kind"],
            "import_event"
        );
    }

    #[test]
    fn a_label_or_a_reference_that_breaks_its_shape_is_refused_first() {
        let root = archive_with_artefact(1);
        let refusal = add(root.path(), PAYLOAD_DIGEST, None, Some(&"l".repeat(201)))
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["field"],
            "label"
        );

        let refusal = add(root.path(), PAYLOAD_DIGEST, None, Some("one\ntwo"))
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);

        for reference in ["../../etc/passwd", "sha256:short", "not-a-digest"] {
            let refusal = add(root.path(), reference, None, None).unwrap_err().error;
            assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
            assert_eq!(refusal.exit_code(), 2);
            let rendered = serde_json::to_string(&refusal).unwrap();
            assert!(!rendered.contains("passwd"), "no reference is echoed");
        }
        assert_eq!(
            fs::read_dir(root.path().join(RECEIPTS_DIR))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn adding_a_receipt_needs_the_writer_lock_and_listing_needs_none() {
        let root = archive_with_artefact(1);
        let _held = WriterLock::acquire(root.path()).unwrap();
        let refusal = add(root.path(), PAYLOAD_DIGEST, None, None)
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::LOCK_HELD);
        assert!(refusal.is_retryable());
        assert_eq!(list(root.path()).unwrap().data.count, 0);
    }

    #[test]
    fn a_malformed_receipt_document_is_reported_rather_than_skipped() {
        let root = archive_with_artefact(1);
        fs::write(
            root.path()
                .join(RECEIPTS_DIR)
                .join("ffffffffffffffffffffffffffffffff.json"),
            b"{ not a record",
        )
        .unwrap();
        let refusal = list(root.path()).unwrap_err().error;
        assert_eq!(refusal.code, codes::RECORD_MALFORMED);
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["record_kind"], "receipt");
        assert_eq!(json["details"]["path_count"], 1);
    }

    #[test]
    fn an_empty_archive_lists_no_receipt() {
        let root = archive_with_artefact(0);
        let listed = list(root.path()).unwrap().data;
        assert_eq!(listed.count, 0);
        assert!(listed.receipts.is_empty());
    }
}
