//! The input caps, checked before allocation and again while streaming.
//!
//! The values are the proposals in `docs/archive-layout.md`. They bound
//! resource use and are never relaxed to make one particular input succeed,
//! so no flag, environment variable, or configuration file changes them.

use crate::error::{Details, Diagnostic, codes};

/// The largest single file that may be imported, in bytes.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// The largest total an import operation may read, in bytes.
pub const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;
/// The largest number of files one import operation may name.
pub const MAX_IMPORT_FILES: u64 = 1_000;
/// The largest record document, in bytes.
pub const MAX_RECORD_BYTES: u64 = 1024 * 1024;
/// The largest original filename kept as a record attribute, in bytes.
pub const MAX_FILENAME_BYTES: u64 = 255;
/// The largest number of distinct tags one case record may carry.
pub const MAX_TAG_COUNT: u64 = 32;
/// The largest single case tag, in bytes.
pub const MAX_TAG_BYTES: u64 = 64;

fn refusal(code: &'static str, message: &str, details: Details) -> Diagnostic {
    Diagnostic::new(code, message, details)
}

/// Refuse an import that names more files than the per-operation cap.
///
/// # Errors
///
/// Returns `input.cap.import_files`.
pub fn check_file_count(observed: u64) -> Result<(), Diagnostic> {
    if observed > MAX_IMPORT_FILES {
        return Err(refusal(
            codes::INPUT_CAP_IMPORT_FILES,
            "The import names more files than the per-operation cap allows.",
            Details::new()
                .int("cap_count", MAX_IMPORT_FILES)
                .int("observed_count", observed),
        ));
    }
    Ok(())
}

/// Refuse a file larger than the single-file cap.
///
/// # Errors
///
/// Returns `input.cap.file_size`, naming the input's position only.
pub fn check_file_size(observed: u64, input_index: u64) -> Result<(), Diagnostic> {
    if observed > MAX_FILE_BYTES {
        return Err(refusal(
            codes::INPUT_CAP_FILE_SIZE,
            "An input file exceeds the single-file size cap.",
            Details::new()
                .int("cap_bytes", MAX_FILE_BYTES)
                .int("observed_bytes", observed)
                .int("input_index", input_index),
        ));
    }
    Ok(())
}

/// Refuse an import whose total bytes exceed the per-operation cap.
///
/// # Errors
///
/// Returns `input.cap.import_bytes`.
pub fn check_import_bytes(observed: u64, input_index: u64) -> Result<(), Diagnostic> {
    if observed > MAX_IMPORT_BYTES {
        return Err(refusal(
            codes::INPUT_CAP_IMPORT_BYTES,
            "The import's total bytes exceed the per-operation cap.",
            Details::new()
                .int("cap_bytes", MAX_IMPORT_BYTES)
                .int("observed_bytes", observed)
                .int("input_index", input_index),
        ));
    }
    Ok(())
}

/// Refuse an original filename longer than the attribute cap.
///
/// # Errors
///
/// Returns `input.cap.filename_length`, reporting only the length.
pub fn check_filename_length(observed: u64, input_index: u64) -> Result<(), Diagnostic> {
    if observed > MAX_FILENAME_BYTES {
        return Err(refusal(
            codes::INPUT_CAP_FILENAME_LENGTH,
            "A supplied original filename exceeds the attribute length cap.",
            Details::new()
                .int("cap_bytes", MAX_FILENAME_BYTES)
                .int("observed_bytes", observed)
                .int("input_index", input_index),
        ));
    }
    Ok(())
}

/// Refuse a case that would carry more distinct tags than the cap allows.
///
/// The count is the one that would be stored, after the duplicates a user may
/// legitimately repeat on the command line have been removed, so a repeated
/// tag never spends part of the cap.
///
/// # Errors
///
/// Returns `input.cap.tag_count`, reporting the two counts and no tag value.
pub fn check_tag_count(observed: u64) -> Result<(), Diagnostic> {
    if observed > MAX_TAG_COUNT {
        return Err(refusal(
            codes::INPUT_CAP_TAG_COUNT,
            "A case carries more distinct tags than the per-record cap allows.",
            Details::new()
                .int("cap_count", MAX_TAG_COUNT)
                .int("observed_count", observed),
        ));
    }
    Ok(())
}

/// Refuse a single tag longer than the per-tag cap.
///
/// # Errors
///
/// Returns `input.cap.tag_length`, reporting the two lengths and the tag's
/// position among the supplied tags, never the tag itself: a tag is the
/// user's own word for their own matter.
pub fn check_tag_length(observed: u64, input_index: u64) -> Result<(), Diagnostic> {
    if observed > MAX_TAG_BYTES {
        return Err(refusal(
            codes::INPUT_CAP_TAG_LENGTH,
            "A case tag exceeds the per-tag length cap.",
            Details::new()
                .int("cap_bytes", MAX_TAG_BYTES)
                .int("observed_bytes", observed)
                .int("input_index", input_index),
        ));
    }
    Ok(())
}

/// Refuse a record document larger than the record cap.
///
/// # Errors
///
/// Returns `input.cap.record_size`.
pub fn check_record_size(observed: u64) -> Result<(), Diagnostic> {
    if observed > MAX_RECORD_BYTES {
        return Err(refusal(
            codes::INPUT_CAP_RECORD_SIZE,
            "A record document exceeds the record size cap.",
            Details::new()
                .int("cap_bytes", MAX_RECORD_BYTES)
                .int("observed_bytes", observed),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_cap_refuses_only_above_its_documented_value() {
        assert!(check_file_count(MAX_IMPORT_FILES).is_ok());
        assert!(check_file_size(MAX_FILE_BYTES, 0).is_ok());
        assert!(check_import_bytes(MAX_IMPORT_BYTES, 0).is_ok());
        assert!(check_filename_length(MAX_FILENAME_BYTES, 0).is_ok());
        assert!(check_record_size(MAX_RECORD_BYTES).is_ok());
        assert!(check_tag_count(MAX_TAG_COUNT).is_ok());
        assert!(check_tag_length(MAX_TAG_BYTES, 0).is_ok());

        for (refusal, code) in [
            (
                check_file_count(MAX_IMPORT_FILES + 1),
                codes::INPUT_CAP_IMPORT_FILES,
            ),
            (
                check_file_size(MAX_FILE_BYTES + 1, 3),
                codes::INPUT_CAP_FILE_SIZE,
            ),
            (
                check_import_bytes(MAX_IMPORT_BYTES + 1, 3),
                codes::INPUT_CAP_IMPORT_BYTES,
            ),
            (
                check_filename_length(MAX_FILENAME_BYTES + 1, 3),
                codes::INPUT_CAP_FILENAME_LENGTH,
            ),
            (
                check_record_size(MAX_RECORD_BYTES + 1),
                codes::INPUT_CAP_RECORD_SIZE,
            ),
            (
                check_tag_count(MAX_TAG_COUNT + 1),
                codes::INPUT_CAP_TAG_COUNT,
            ),
            (
                check_tag_length(MAX_TAG_BYTES + 1, 3),
                codes::INPUT_CAP_TAG_LENGTH,
            ),
        ] {
            let refusal = refusal.unwrap_err();
            assert_eq!(refusal.code, code);
            assert_eq!(refusal.exit_code(), 3);
            assert!(!refusal.is_retryable(), "a cap is never retried");
        }
    }

    #[test]
    fn a_cap_refusal_names_the_position_of_the_input_and_never_its_name() {
        let refusal = check_file_size(MAX_FILE_BYTES + 1, 7).unwrap_err();
        let json = serde_json::to_value(&refusal).unwrap();
        assert_eq!(json["details"]["input_index"], 7);
        assert_eq!(json["details"]["cap_bytes"], MAX_FILE_BYTES);
        assert_eq!(json["details"]["bucket"], "input");
        assert_eq!(json["details"].as_object().unwrap().len(), 4);
    }
}
