//! The lines `archive check` prints.

use openpapir_core::Report;

/// The lines `archive check` prints, in either outcome.
///
/// Counts only. The path, the name, and the digest of a damaged object never
/// reach this output, exactly as they never reach the JSON report: the count
/// answers the only question the privacy rule allows an answer to.
#[must_use]
pub fn integrity(report: &Report) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Checked {} object(s) and {} record(s); {} byte(s) digested.",
            report.objects_checked, report.records_checked, report.bytes_digested
        ),
        if report.is_clean() {
            "No problem found.".to_owned()
        } else {
            "Problems found:".to_owned()
        },
    ];
    for problem in &report.problems {
        if problem.count > 0 {
            lines.push(format!("{} {}", problem.code, problem.count));
        }
    }
    lines.push(format!(
        "Orphan object(s): {}. Object(s) not digested: {}. Record directory(ies) not read: {}. Leftover staging file(s): {} incoming, {} in record directories.",
        report.orphan_objects,
        report.objects_unchecked,
        report.records_unchecked,
        report.staging_files,
        report.records_staging_files
    ));
    lines.push(format!(
        "Derived-metadata record(s): {}. A missing one is not a problem.",
        report.derived_records
    ));
    lines.push(
        "The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect."
            .to_owned(),
    );
    lines
}
