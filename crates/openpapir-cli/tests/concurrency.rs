//! What happens when two openPapir processes touch one archive at once.
//!
//! These are not properties. A property generates an input and shrinks a
//! counterexample; the subject here is a schedule rather than an input, and
//! nothing shrinks a schedule. So the tests are example based, they run the
//! built binary rather than the library, and they assert the two guarantees
//! `docs/architecture.md` makes about concurrent access.
//!
//! The first is the single-writer lock. One `lock` file in the archive root
//! admits one writer, and a second writer refuses with `lock.held` rather
//! than waiting or taking over. Several processes writing at once must
//! therefore each either succeed outright or refuse with that one code, and
//! what the archive holds afterwards must be exactly what the successful ones
//! wrote: no half-written record, no leftover staging file, and a clean
//! `archive check`.
//!
//! The second is that a reader needs no lock at all. A record is published by
//! renaming a fully written staging file over its final name, which is atomic
//! on one filesystem, so a listing taken at any moment sees the set before
//! the publication or the set after it and never a document part way through
//! being written. The test asserts that by parsing every listing it takes
//! while a writer publishes into the same archive: every case in every
//! listing is a whole record, and no listing ever loses a case an earlier one
//! had.
//!
//! Neither test asserts that contention actually occurred. Whether two
//! processes overlap is the operating system's business, and a test that
//! demanded an overlap would fail on a machine that scheduled them apart.
//! What is asserted is that every observed outcome is a permitted one, which
//! holds whether they overlapped or not.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use serde_json::Value;

/// How many processes write into one archive at the same time.
const WRITERS: usize = 8;

/// How many cases the writer publishes while the reader lists.
///
/// The count is large enough that the reader takes many listings across the
/// writer's run and small enough that the whole file stays a fraction of the
/// suite's budget: every publication is one process.
const PUBLICATIONS: usize = 24;

/// The exit code of the `lock` bucket, which is where `lock.held` sits.
const LOCK_EXIT: i32 = 4;

/// The most listings the reader takes before it stops on its own.
///
/// The flag the reader waits on is released by a drop guard, so a writer that
/// panics stops the loop as surely as one that returns. The bound covers the
/// remaining way the loop could fail to end, a writer that neither returns
/// nor unwinds, and turns an unbounded spin into an assertion. It is three
/// orders of magnitude above the few dozen listings a real run takes.
const LISTING_LIMIT: usize = 10_000;

/// Releases the reader when the writer thread ends, however it ends.
///
/// It exists so that the reader's loop condition is never the only thing
/// standing between a failed writer and a test that hangs.
struct Finished(Arc<AtomicBool>);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn path(path: &Path) -> &str {
    path.to_str().expect("a temporary path is UTF-8")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(args)
        .output()
        .expect("run the openpapir binary")
}

/// Parse one response, asserting the envelope is whole and on one line.
///
/// Every assertion in this file goes through here, so a listing that arrived
/// truncated or interleaved with another writer's output fails at the parse
/// rather than silently reading as something smaller.
fn envelope(output: &Output) -> Value {
    let text = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
    assert_eq!(text.lines().count(), 1, "exactly one line of JSON");
    let envelope: Value = serde_json::from_str(text.trim()).expect("stdout is one JSON object");
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["verified"], false, "nothing is ever verified here");
    envelope
}

/// An initialised archive inside a temporary directory.
fn archive() -> (tempfile::TempDir, std::path::PathBuf) {
    let home = tempfile::tempdir().expect("a temporary home");
    let root = home.path().join("archive");
    std::fs::create_dir(&root).expect("an archive directory");
    let output = run(&["archive", "init", path(&root), "--json"]);
    assert!(output.status.success(), "an archive is created");
    (home, root)
}

/// Create one case and return its identifier.
fn create_case(root: &Path, title: &str) -> String {
    let output = run(&[
        "case",
        "create",
        "--archive",
        path(root),
        "--title",
        title,
        "--json",
    ]);
    assert!(output.status.success(), "a case is created");
    envelope(&output)["data"]["case"]["id"]
        .as_str()
        .expect("a created case names itself")
        .to_owned()
}

/// Assert that `archive check` finds nothing wrong and nothing left behind.
fn check_is_clean(root: &Path, records: u64) {
    let output = run(&["archive", "check", "--archive", path(root), "--json"]);
    assert!(output.status.success(), "a clean archive checks clean");
    let data = envelope(&output)["data"].clone();
    for problem in data["problems"].as_array().expect("problems is a list") {
        assert_eq!(
            problem["count"], 0,
            "a concurrent run left a problem behind: {problem}"
        );
    }
    assert_eq!(data["records_checked"], records, "every record was read");
    assert_eq!(data["records_unchecked"], 0, "no record was skipped");
    assert_eq!(
        data["records_staging_files"], 0,
        "no staging file was left in a record directory"
    );
    assert_eq!(data["staging_files"], 0, "no staging file was left behind");
}

/// Every field a whole case record carries, asserted on one listing entry.
///
/// This is what "never a partial document" means for a reader: a case that
/// appears in a listing carries every key the record kind defines, with a
/// value of the right shape, and its title is one a writer actually wrote.
fn case_is_whole(case: &Value, titles: &BTreeSet<String>) {
    assert_eq!(case["archive_schema_version"], 1);
    assert_eq!(case["record_kind"], "case");
    let id = case["id"].as_str().expect("a case names itself");
    assert_eq!(id.len(), 32, "an identifier is 32 characters");
    assert!(
        id.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "an identifier is lowercase hexadecimal"
    );
    assert!(
        !case["created_at"]
            .as_str()
            .expect("a case is dated")
            .is_empty()
    );
    assert!(case["status"].is_string(), "a case carries its status");
    assert!(case["tags"].is_array(), "a case carries its tag list");
    let title = case["title"].as_str().expect("a case carries its title");
    assert!(
        titles.contains(title),
        "a listing showed a title no writer published"
    );
}

/// Several processes adding a submission to one archive at once each either
/// hold the lock or refuse with `lock.held`, and what they wrote is all that
/// the archive holds afterwards.
#[test]
fn concurrent_writers_either_hold_the_lock_or_refuse_with_lock_held() {
    let (_home, root) = archive();
    let case_id = create_case(&root, "Contended");

    let descriptions: Vec<String> = (0..WRITERS)
        .map(|index| format!("Concurrent submission {index}"))
        .collect();
    // Every child is started before any of them is waited on, so their
    // attempts on the lock overlap as far as the operating system allows.
    let children: Vec<_> = descriptions
        .iter()
        .map(|description| {
            Command::new(env!("CARGO_BIN_EXE_openpapir"))
                .args([
                    "submission",
                    "add",
                    "--archive",
                    path(&root),
                    "--case",
                    &case_id,
                    "--description",
                    description,
                    "--json",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("run the openpapir binary")
        })
        .collect();

    let mut written = BTreeSet::new();
    let mut refused = 0_usize;
    for child in children {
        let output = child.wait_with_output().expect("a writer finishes");
        let envelope = envelope(&output);
        if envelope["ok"] == Value::Bool(true) {
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(envelope["command"], "submission.add");
            let submission = &envelope["data"]["submission"];
            assert_eq!(submission["case_id"], Value::String(case_id.clone()));
            written.insert(
                submission["description"]
                    .as_str()
                    .expect("a submission carries its description")
                    .to_owned(),
            );
        } else {
            assert_eq!(
                envelope["error"]["code"], "lock.held",
                "a writer that did not hold the lock refused with the one code that says so"
            );
            assert_eq!(output.status.code(), Some(LOCK_EXIT));
            assert_eq!(
                envelope["error"]["details"]["bucket"], "lock",
                "a lock refusal is bucketed as one"
            );
            refused += 1;
        }
    }
    assert_eq!(
        written.len() + refused,
        WRITERS,
        "every writer either wrote or refused"
    );
    assert!(
        !written.is_empty(),
        "at least one writer holds the lock and writes"
    );

    // The record set is exactly what the successful writers reported, so no
    // attempt wrote a record it then reported as refused and none was lost.
    let output = run(&["case", "show", "--archive", path(&root), &case_id, "--json"]);
    assert!(output.status.success(), "the case reads back");
    let data = envelope(&output)["data"].clone();
    assert_eq!(data["submission_count"], written.len() as u64);
    let stored: BTreeSet<String> = data["submissions"]
        .as_array()
        .expect("submissions is a list")
        .iter()
        .map(|submission| {
            submission["description"]
                .as_str()
                .expect("a stored submission carries its description")
                .to_owned()
        })
        .collect();
    assert_eq!(
        stored, written,
        "the archive holds exactly what the successful writers wrote"
    );

    assert!(
        !root.join("lock").exists(),
        "every writer released the lock it held"
    );
    check_is_clean(&root, written.len() as u64 + 1);
}

/// A listing taken while another process publishes sees the set before the
/// publication or the set after it, and never a document part way written.
#[test]
fn a_listing_taken_during_a_publication_never_sees_a_partial_record() {
    let (_home, root) = archive();

    let titles: BTreeSet<String> = (0..PUBLICATIONS)
        .map(|index| format!("Published case {index}"))
        .collect();
    let done = Arc::new(AtomicBool::new(false));

    let writing = {
        let root = root.clone();
        let titles: Vec<String> = titles.iter().cloned().collect();
        let done = Arc::clone(&done);
        thread::spawn(move || {
            // The guard is what releases the reader, not a store at the end
            // of the body: an assertion inside `create_case` unwinds this
            // thread, and a flag set only on the way out would leave the
            // reader looping until the harness killed the whole run. Drop
            // runs on the unwinding path too, so the reader stops either way
            // and the panic surfaces at the join below.
            let _finished = Finished(done);
            for title in &titles {
                create_case(&root, title);
            }
        })
    };

    let mut counts = Vec::new();
    while !done.load(Ordering::Acquire) && counts.len() < LISTING_LIMIT {
        let output = run(&["case", "list", "--archive", path(&root), "--json"]);
        assert!(output.status.success(), "reading needs no lock at all");
        let envelope = envelope(&output);
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["command"], "case.list");
        let cases = envelope["data"]["cases"]
            .as_array()
            .expect("cases is a list")
            .clone();
        assert_eq!(
            envelope["data"]["count"],
            cases.len() as u64,
            "a listing counts what it lists"
        );
        assert!(
            cases.len() <= PUBLICATIONS,
            "a listing never shows more cases than were published"
        );
        for case in &cases {
            case_is_whole(case, &titles);
        }
        counts.push(cases.len());
    }
    writing.join().expect("the writer finishes");
    assert!(
        counts.len() < LISTING_LIMIT,
        "the reader gave up before the writer said it was done"
    );

    assert!(
        counts.len() >= 2,
        "the reader took several listings while the writer published"
    );
    assert!(
        counts.windows(2).all(|pair| pair[0] <= pair[1]),
        "a listing never loses a case an earlier listing showed"
    );

    let output = run(&["case", "list", "--archive", path(&root), "--json"]);
    let envelope = envelope(&output);
    assert_eq!(
        envelope["data"]["count"], PUBLICATIONS as u64,
        "every publication is there once the writer is done"
    );
    let listed: BTreeSet<String> = envelope["data"]["cases"]
        .as_array()
        .expect("cases is a list")
        .iter()
        .map(|case| {
            case["title"]
                .as_str()
                .expect("a case carries its title")
                .to_owned()
        })
        .collect();
    assert_eq!(
        listed, titles,
        "every published case is listed exactly once"
    );

    check_is_clean(&root, PUBLICATIONS as u64);
}
