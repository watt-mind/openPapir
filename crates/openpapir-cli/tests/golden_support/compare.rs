//! Comparing one captured stream with the golden file that pins it, and the
//! deliberate rewrite that regenerates the file instead.
//!
//! The rule the rest of the harness rests on is here: a golden file that is
//! not on disk is a difference, never an empty expectation. Every passing
//! case has an empty `human.stderr.txt`, so reading a missing file as the
//! empty string would let a deleted golden compare equal and pass in silence.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// The environment variable that rewrites the golden files deliberately.
///
/// Nothing regenerates on its own and CI never sets it; the contract and the
/// regeneration path are in `tests/golden/README.md`.
pub const UPDATE: &str = "OPENPAPIR_UPDATE_GOLDEN";

/// What a golden file was found to hold, before anything is compared.
enum Expected {
    /// The file is there and holds this text.
    Present(String),
    /// The file is not there at all, which is a difference of its own.
    Missing,
}

/// Compare one file with what the run produced, or rewrite it on request.
pub fn settle(path: &Path, actual: &str, differences: &mut Vec<String>) {
    if std::env::var_os(UPDATE).is_some() {
        fs::create_dir_all(path.parent().expect("a golden file has a parent"))
            .expect("create the golden directory");
        fs::write(path, actual).expect("write the golden file");
        return;
    }
    match read(path) {
        Expected::Present(expected) => {
            if expected != actual {
                differences.push(report(path, &expected, actual));
            }
        }
        Expected::Missing => differences.push(missing(path, actual)),
    }
}

/// Read one golden file, telling an absent file apart from an empty one.
///
/// Any other failure is a broken checkout rather than a contract difference,
/// so it stops the run rather than being reported as a diff.
fn read(path: &Path) -> Expected {
    match fs::read_to_string(path) {
        Ok(text) => Expected::Present(text),
        Err(error) if error.kind() == ErrorKind::NotFound => Expected::Missing,
        Err(error) => panic!("read the golden file {}: {error}", path.display()),
    }
}

/// The difference a golden file that is not on disk stands for.
///
/// It is named as missing rather than shown as an empty side, so nobody reads
/// a deleted file as output that legitimately became empty.
fn missing(path: &Path, actual: &str) -> String {
    let mut lines = vec![
        format!("--- {} (golden) is missing", path.display()),
        "+++ this run".to_owned(),
    ];
    lines.extend(actual.lines().map(|line| format!("+{line}")));
    lines.join("\n")
}

/// A readable line-by-line difference between a golden file and a run.
fn report(path: &Path, expected: &str, actual: &str) -> String {
    let mut lines = vec![format!("--- {} (golden)", path.display())];
    lines.push("+++ this run".to_owned());
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    for index in 0..expected.len().max(actual.len()) {
        match (expected.get(index), actual.get(index)) {
            (Some(left), Some(right)) if left == right => lines.push(format!("  {left}")),
            (left, right) => {
                if let Some(left) = left {
                    lines.push(format!("-{left}"));
                }
                if let Some(right) = right {
                    lines.push(format!("+{right}"));
                }
            }
        }
    }
    lines.join("\n")
}

/// The directory holding the committed golden files.
#[must_use]
pub fn golden_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// Copy one committed golden case into a temporary directory.
///
/// The tests below delete a file from the copy, so `tests/golden/` itself is
/// never touched and a failing test cannot damage the contract it checks.
fn copy_case(name: &str, into: &Path) -> std::path::PathBuf {
    let source = golden_root().join(name);
    let copy = into.join(name);
    fs::create_dir(&copy).expect("create the copied case directory");
    for entry in fs::read_dir(&source).expect("read the golden case") {
        let entry = entry.expect("read a golden entry");
        fs::copy(entry.path(), copy.join(entry.file_name())).expect("copy a golden file");
    }
    copy
}

/// A regeneration run rewrites rather than compares, so the comparison tests
/// have nothing to assert while the variable is set.
fn comparing() -> bool {
    std::env::var_os(UPDATE).is_none()
}

/// A golden file that is on disk and unchanged is no difference, and one that
/// changed is reported with both sides.
#[test]
fn a_changed_golden_file_is_reported_with_both_sides() {
    if !comparing() {
        return;
    }
    let temporary = tempfile::tempdir().expect("create the temporary golden copy");
    let case = copy_case("capabilities", temporary.path());
    let path = case.join("human.exit");
    let expected = fs::read_to_string(&path).expect("read the copied golden file");

    let mut differences = Vec::new();
    settle(&path, &expected, &mut differences);
    assert!(
        differences.is_empty(),
        "an unchanged golden file is no difference"
    );

    settle(&path, "4\n", &mut differences);
    assert_eq!(differences.len(), 1, "a changed file is one difference");
    assert!(differences[0].contains("+4"), "the run's side is shown");
    assert!(
        !differences[0].contains("is missing"),
        "a file that is there is never reported as absent"
    );
}

/// A golden file that was deleted is a difference, never an empty
/// expectation that whatever the run produced can be compared against.
#[test]
fn a_missing_golden_file_is_a_difference_and_never_an_empty_expectation() {
    if !comparing() {
        return;
    }
    let temporary = tempfile::tempdir().expect("create the temporary golden copy");
    let case = copy_case("capabilities", temporary.path());
    let path = case.join("human.stderr.txt");
    let empty = fs::read_to_string(&path).expect("read the copied golden file");
    assert_eq!(empty, "", "this case pins an empty stderr, as most do");

    fs::remove_file(&path).expect("delete the copied golden file");
    let mut differences = Vec::new();
    settle(&path, &empty, &mut differences);
    assert_eq!(
        differences.len(),
        1,
        "a deleted golden file cannot compare equal to an empty capture"
    );
    assert!(differences[0].contains("human.stderr.txt"));
    assert!(differences[0].contains("is missing"));
}

/// The missing report still shows what the run produced, so the reader can
/// see whether the file should be restored or regenerated.
#[test]
fn a_missing_golden_file_still_shows_what_the_run_produced() {
    if !comparing() {
        return;
    }
    let temporary = tempfile::tempdir().expect("create the temporary golden copy");
    let path = temporary.path().join("never-written.txt");
    let mut differences = Vec::new();
    settle(&path, "first line\nsecond line\n", &mut differences);
    assert_eq!(differences.len(), 1);
    assert!(differences[0].contains("+first line"));
    assert!(differences[0].contains("+second line"));
}

/// A comparing run writes nothing at all: the file stays absent and the
/// absence is what is reported.
#[test]
fn a_comparing_run_writes_no_golden_file() {
    if !comparing() {
        return;
    }
    let temporary = tempfile::tempdir().expect("create the temporary golden copy");
    let path = temporary.path().join("new.case/human.exit");
    let mut differences = Vec::new();
    settle(&path, "0\n", &mut differences);
    assert!(!path.exists(), "nothing is written while comparing");
    assert_eq!(differences.len(), 1, "an absent file is the difference");
}
