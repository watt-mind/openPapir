//! The closed media-type table and the magic-byte sniff behind it.
//!
//! The table is closed on purpose (`docs/archive-layout.md`). A derived
//! record may carry one of exactly seven values, every one of them decided
//! from the first bytes of a stored object, and a shape the table does not
//! name reads as `unknown` rather than as a guess. Nothing here parses a
//! file, walks a container, or reads a filename: a media type is a statement
//! about the leading bytes and nothing else.
//!
//! A media type is never evidence. It says what the first bytes look like. It
//! says nothing about authenticity, origin, delivery, legal effect, or
//! whether the object is a receipt, and no caller may read one that way.

/// How many leading bytes the sniff ever looks at.
///
/// Every signature in the table is a handful of bytes long, and the text rule
/// needs a sample rather than the whole file, so one bounded read answers the
/// question for an object of any size.
pub const SNIFF_BYTES: usize = 4 * 1024;

/// The value a derived record carries when the leading bytes name no shape
/// the table knows, and the value an empty object carries.
pub const UNKNOWN: &str = "unknown";

/// Every value the table may report, in the order a report lists them.
///
/// The order is the value's own alphabetical order, so a report reads the
/// same way `archive check` orders its codes and a new value cannot quietly
/// change the position of an existing one.
pub const MEDIA_TYPES: [&str; 7] = ["jpeg", "pdf", "png", "text", UNKNOWN, "xml", "zip"];

/// One signature of the table: the leading bytes, and the value they name.
struct Signature {
    /// The bytes an object of this shape starts with.
    prefix: &'static [u8],
    /// The value the table reports for them.
    media_type: &'static str,
}

/// The signatures, tried in this order.
///
/// `zip` covers the whole zip family: the local-file header, the
/// end-of-central-directory record of an empty archive, and the spanned
/// marker. openPapir stops there and never refines the value from a
/// filename's extension: reading a container to tell one zip-family format
/// from another is openKRX's and openSzigno's work, not this project's
/// (`AGENTS.md`).
const SIGNATURES: [Signature; 6] = [
    Signature {
        prefix: b"%PDF-",
        media_type: "pdf",
    },
    Signature {
        prefix: b"\x89PNG\r\n\x1a\n",
        media_type: "png",
    },
    Signature {
        prefix: b"\xff\xd8\xff",
        media_type: "jpeg",
    },
    Signature {
        prefix: b"PK\x03\x04",
        media_type: "zip",
    },
    Signature {
        prefix: b"PK\x05\x06",
        media_type: "zip",
    },
    Signature {
        prefix: b"PK\x07\x08",
        media_type: "zip",
    },
];

/// The byte-order mark a UTF-8 document may begin with.
const BOM: &[u8] = b"\xef\xbb\xbf";

/// The declaration an XML document begins with.
const XML_PROLOGUE: &[u8] = b"<?xml";

/// The value the table reports for these leading bytes.
///
/// `sample` is the first [`SNIFF_BYTES`] of an object, or the whole object
/// when it is shorter. An object with no bytes at all names no shape, so it
/// reads as [`UNKNOWN`]: an empty file is not evidence of a type.
#[must_use]
pub fn sniff(sample: &[u8]) -> &'static str {
    if sample.is_empty() {
        return UNKNOWN;
    }
    for signature in &SIGNATURES {
        if sample.starts_with(signature.prefix) {
            return signature.media_type;
        }
    }
    let body = sample.strip_prefix(BOM).unwrap_or(sample);
    if body.starts_with(XML_PROLOGUE) {
        return "xml";
    }
    if is_text(body) { "text" } else { UNKNOWN }
}

/// Whether the sample reads as plain text.
///
/// The rule is deliberately strict: the sample must be UTF-8, and it may
/// carry no control character other than a tab, a line feed, or a carriage
/// return. A sample that ends inside a multi-byte character is judged on the
/// part that is whole, because the cut is the bounded read's doing and not
/// the file's.
fn is_text(sample: &[u8]) -> bool {
    let text = match std::str::from_utf8(sample) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => {
            // The only invalid part is an unfinished character at the end,
            // which is where the read stopped.
            std::str::from_utf8(&sample[..error.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return false,
    };
    !text.is_empty()
        && text
            .chars()
            .all(|character| !character.is_control() || matches!(character, '\t' | '\n' | '\r'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_signature_of_the_closed_table_is_reported_by_its_own_name() {
        for (sample, expected) in [
            (&b"%PDF-1.7\n1 0 obj\n"[..], "pdf"),
            (&b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"[..], "png"),
            (&b"\xff\xd8\xff\xe0\x00\x10JFIF"[..], "jpeg"),
            (&b"PK\x03\x04\x14\x00\x00\x00"[..], "zip"),
            (&b"PK\x05\x06\x00\x00\x00\x00"[..], "zip"),
            (&b"PK\x07\x08\x00\x00\x00\x00"[..], "zip"),
            (&b"<?xml version=\"1.0\"?><a/>"[..], "xml"),
            (&b"\xef\xbb\xbf<?xml version=\"1.0\"?>"[..], "xml"),
            (&b"synthetic submission alpha\n"[..], "text"),
            (&b"\xef\xbb\xbfplain text with a mark\n"[..], "text"),
            (&b"m\xc3\xa1s sz\xc3\xb6veg\n"[..], "text"),
            (&b""[..], UNKNOWN),
            (&b"\x00\x01\x02\x03"[..], UNKNOWN),
            (&b"text with a nul\x00inside"[..], UNKNOWN),
            (&b"\xff\xfe\x00\x00"[..], UNKNOWN),
        ] {
            assert_eq!(sniff(sample), expected, "{sample:?}");
            assert!(
                MEDIA_TYPES.contains(&sniff(sample)),
                "the table is closed and reports nothing outside it"
            );
        }
    }

    #[test]
    fn a_character_cut_by_the_bounded_read_still_reads_as_text() {
        let mut sample = "á".repeat(SNIFF_BYTES).into_bytes();
        sample.truncate(SNIFF_BYTES + 1);
        assert_eq!(sniff(&sample), "text");
        assert_eq!(sniff(b"\xc3"), UNKNOWN, "an unfinished character alone");
    }

    #[test]
    fn the_reported_values_are_ordered_and_hold_no_duplicate() {
        let mut sorted = MEDIA_TYPES;
        sorted.sort_unstable();
        assert_eq!(MEDIA_TYPES, sorted, "the table is in its own order");
        let mut seen = MEDIA_TYPES.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), MEDIA_TYPES.len(), "each value appears once");
        for signature in &SIGNATURES {
            assert!(MEDIA_TYPES.contains(&signature.media_type));
        }
    }
}
