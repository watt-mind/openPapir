//! What the linear scans cost on an archive of ten thousand cases.
//!
//! Every listing and the integrity check read every record they could
//! report, because the archive keeps no index. This measurement says what
//! that costs at a size a user might reach, and asserts that each scan stays
//! under a ceiling documented in `docs/architecture.md`.
//!
//! One import of a full batch of files is timed as well. It is not a listing,
//! but it is the one write whose cost could grow with what the archive
//! already holds, so it is measured against the archive the setup built
//! rather than against an empty one.
//!
//! The test is ignored, so it never runs in CI: it builds a temporary archive
//! of forty thousand records and twenty thousand objects first, which takes
//! minutes. `docs/testing.md` says how to run it. Timing is `std::time` only,
//! and no production code is measured through anything but the built binary.

mod bench_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// One timed invocation.
struct Measurement {
    /// The invocation, as the documentation names it.
    name: &'static str,
    /// The wall time the run took.
    elapsed: Duration,
    /// The ceiling the run must stay under.
    ceiling: Duration,
    /// What the run printed, so a measurement can be shown to have scanned
    /// what it claims to have scanned.
    stdout: Vec<u8>,
}

/// The ceilings the Performance section of `docs/architecture.md` documents.
///
/// They are deliberately loose, and they are not targets. The measured times
/// on the machine that section names are between one and two orders of
/// magnitude below them, because the same assertion has to hold on the
/// unoptimised test profile, on a slower disk, and on a machine that is doing
/// something else at the time. A run that crosses one of them says the cost
/// changed in kind, not that it drifted.
const LIST_CEILING: Duration = Duration::from_secs(5);
const CHECK_CEILING: Duration = Duration::from_secs(10);
const EXPORT_CEILING: Duration = Duration::from_secs(5);
const DELETE_CEILING: Duration = Duration::from_secs(10);
/// The ceiling for one import batch, which is tighter in proportion than the
/// scans above: an import of a batch this size costs seconds at most, and the
/// cost this asserts against grew with the archive rather than with the
/// batch, so a run that crosses it is well clear of any drift.
const IMPORT_CEILING: Duration = Duration::from_secs(10);

/// Files in the timed import, which is the per-import file cap itself.
const IMPORT_BATCH: usize = 1_000;

/// A search reads every record of all four kinds rather than one kind, so it
/// reads four times what a listing reads and is given its own ceiling.
const SEARCH_CEILING: Duration = Duration::from_secs(10);

/// Files in each of the repeated imports that follow it.
///
/// The repeats are there to time an import that follows many others rather
/// than to time a batch, so the batch is small: twenty of the full cap would
/// double the archive and measure the growth rather than the repetition.
const REPEAT_BATCH: usize = 100;

/// How many imports run back to back, the last of which is timed.
const REPEATS: usize = 20;

/// The ceiling for resolving an artefact to its earliest import event.
///
/// It is the listing ceiling: `receipt add` without `--import-event` asks one
/// question about the import events, and whether it is answered from the
/// rebuildable index or from a scan of the records, it stays a read of what
/// the archive holds.
const RECEIPT_CEILING: Duration = Duration::from_secs(5);

/// The exit code of the usage bucket, which an unimplemented subcommand is.
const USAGE_EXIT: i32 = 2;

#[test]
#[ignore = "builds a ten thousand case archive; run it deliberately"]
fn the_linear_scans_stay_under_their_documented_ceilings() {
    let cases = bench_support::requested_cases();
    let built = Instant::now();
    let archive = bench_support::build(cases);
    let setup = built.elapsed();
    let root = archive.archive_string();

    let mut measurements = vec![
        measure(
            "case list",
            LIST_CEILING,
            &["case", "list", "--archive", &root, "--json"],
        ),
        measure(
            "case list --query",
            LIST_CEILING,
            &[
                "case",
                "list",
                "--archive",
                &root,
                "--query",
                bench_support::QUERY_WORD,
                "--json",
            ],
        ),
        // `case show` reads every submission, association, and receipt the
        // archive holds to name the receipts a live association ties to this
        // one case, so it is timed on the same archive as the listings.
        measure(
            "case show",
            LIST_CEILING,
            &[
                "case",
                "show",
                "--archive",
                &root,
                &archive.case_ids[0],
                "--json",
            ],
        ),
        measure(
            "archive check",
            CHECK_CEILING,
            &["archive", "check", "--archive", &root, "--json"],
        ),
        // `search` reads every case, submission, receipt, and association the
        // archive holds, so it is the widest scan a reader can ask for.
        measure(
            "search",
            SEARCH_CEILING,
            &[
                "search",
                "--archive",
                &root,
                bench_support::QUERY_WORD,
                "--json",
            ],
        ),
    ];
    if let Some(status) = optional(
        "archive status",
        LIST_CEILING,
        &["archive", "status", "--archive", &root, "--json"],
    ) {
        measurements.push(status);
    }
    let destination = archive.destination("export");
    measurements.push(measure(
        "case export",
        EXPORT_CEILING,
        &[
            "case",
            "export",
            "--archive",
            &root,
            "--case",
            &archive.case_ids[0],
            "--to",
            &destination,
            "--json",
        ],
    ));
    measurements.push(measure(
        "case delete --purge",
        DELETE_CEILING,
        &[
            "case",
            "delete",
            "--archive",
            &root,
            "--case",
            &archive.case_ids[1],
            "--purge",
            "--json",
        ],
    ));

    // `receipt add` without `--import-event` resolves the artefact to its
    // earliest import event. The first one runs against no index, because the
    // setup names the event on every receipt it writes and an import of new
    // files leaves no index behind, so it is the rebuilding one; the second
    // runs against the index the first left. Both are timed, because the
    // first is the cost a user pays once and the second the cost of every
    // later one.
    for name in ["receipt add", "receipt add, index current"] {
        measurements.push(measure(
            name,
            RECEIPT_CEILING,
            &[
                "receipt",
                "add",
                "--archive",
                &root,
                "--artefact",
                &archive.artefact_digest,
                "--json",
            ],
        ));
    }

    measurements.push(measure_import(&archive));
    measurements.push(measure_repeated_imports(&archive));

    report(cases, setup, &measurements);
    // A scan that matched nothing would be fast for the wrong reason, so what
    // each listing reported is checked before its time is believed.
    assert_eq!(
        reported_count(&measurements[0]),
        cases as u64,
        "the listing did not report every case"
    );
    assert_eq!(
        reported_count(&measurements[1]),
        cases.div_ceil(bench_support::QUERY_ONE_IN) as u64,
        "the query matched a different share of the archive than it filed"
    );
    assert_eq!(
        reported_count(&measurements[4]),
        cases.div_ceil(bench_support::QUERY_ONE_IN) as u64,
        "the search matched a different share of the archive than it filed"
    );
    assert_eq!(
        reported_receipts(&measurements[2]),
        1,
        "the shown case named no receipt, so the section that reads the \
         associations was not measured"
    );
    for measurement in &measurements {
        assert!(
            measurement.elapsed <= measurement.ceiling,
            "{} took {:.3} s, over its {:.0} s ceiling",
            measurement.name,
            measurement.elapsed.as_secs_f64(),
            measurement.ceiling.as_secs_f64()
        );
    }
}

/// Time one invocation of the built binary and require it to succeed.
fn measure(name: &'static str, ceiling: Duration, arguments: &[&str]) -> Measurement {
    measure_in(name, ceiling, None, arguments)
}

/// Time one invocation, optionally from a working directory of its own.
fn measure_in(
    name: &'static str,
    ceiling: Duration,
    directory: Option<&Path>,
    arguments: &[&str],
) -> Measurement {
    let (elapsed, output) = timed(directory, arguments);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{name} did not succeed on the synthetic archive"
    );
    Measurement {
        name,
        elapsed,
        ceiling,
        stdout: output.stdout,
    }
}

/// Time one import of a full batch of new files into the built archive.
///
/// It runs last, so it imports against every import event the setup recorded.
/// Duplicate detection that re-read those events once per input would put
/// this run an order of magnitude over its ceiling, which is what makes the
/// ceiling worth asserting: the batch itself is a few hundred kilobytes.
///
/// The binary is run from the input directory, because a command line of a
/// thousand absolute temporary paths exceeds what some platforms accept at
/// process spawn.
fn measure_import(archive: &bench_support::Synthetic) -> Measurement {
    import_batch(
        archive,
        "import of one batch",
        "import-inputs",
        "late",
        IMPORT_BATCH,
    )
}

/// Time the last of twenty imports run back to back.
///
/// Each of them writes its own import events and leaves the archive one
/// batch larger, so the twentieth runs against everything the nineteen before
/// it wrote. It is timed because the rebuildable index is written by a write
/// as well as read by one: an import that had to rebuild it from every record
/// each time would show here as a cost that grew batch by batch, and one that
/// does not is the cost of the batch alone.
fn measure_repeated_imports(archive: &bench_support::Synthetic) -> Measurement {
    let mut last = None;
    for repeat in 0..REPEATS {
        last = Some(import_batch(
            archive,
            "twentieth import batch",
            &format!("repeat-inputs-{repeat:02}"),
            &format!("repeat-{repeat:02}"),
            REPEAT_BATCH,
        ));
    }
    last.expect("twenty imports leave a last one")
}

/// Write `count` distinct inputs into a directory of their own and time one
/// import of all of them.
///
/// The binary is run from the input directory, because a command line of a
/// thousand absolute temporary paths exceeds what some platforms accept at
/// process spawn.
fn import_batch(
    archive: &bench_support::Synthetic,
    name: &'static str,
    folder: &str,
    prefix: &str,
    count: usize,
) -> Measurement {
    let directory = PathBuf::from(archive.destination(folder));
    fs::create_dir(&directory).expect("create the import input directory");
    let names: Vec<String> = (0..count)
        .map(|index| {
            let name = format!("{prefix}-{index:07}.bin");
            fs::write(
                directory.join(&name),
                format!("benchmark import {prefix} {index:07}\n"),
            )
            .expect("write a synthetic input");
            name
        })
        .collect();
    let root = archive.archive_string();
    let mut arguments = vec!["import", "--archive", root.as_str(), "--json"];
    arguments.extend(names.iter().map(String::as_str));
    let measurement = measure_in(name, IMPORT_CEILING, Some(&directory), &arguments);
    let envelope: serde_json::Value =
        serde_json::from_slice(&measurement.stdout).expect("import prints one envelope");
    assert_eq!(
        envelope["data"]["imported"].as_u64(),
        Some(count as u64),
        "the timed import did not store every input"
    );
    assert_eq!(
        envelope["data"]["duplicates"].as_u64(),
        Some(0),
        "every input of the timed import is distinct"
    );
    measurement
}

/// Time one invocation the build may not implement yet.
///
/// An invocation this build does not recognise is refused in the usage
/// bucket, which is `2`, and is reported as absent rather than measured. Any
/// other outcome is a real measurement, so an operation is timed from the
/// commit that adds it without this file changing again.
///
/// The one run is both the probe and the measurement. Running it a second
/// time to measure it would time a warm run and put it in the same table as
/// the others, which are timed once each.
fn optional(name: &'static str, ceiling: Duration, arguments: &[&str]) -> Option<Measurement> {
    let (elapsed, output) = timed(None, arguments);
    if output.status.code() == Some(USAGE_EXIT) {
        println!("{name}: not implemented in this build, not measured");
        return None;
    }
    assert_eq!(
        output.status.code(),
        Some(0),
        "{name} did not succeed on the synthetic archive"
    );
    Some(Measurement {
        name,
        elapsed,
        ceiling,
        stdout: output.stdout,
    })
}

/// Run the built binary once and return how long it took and what it wrote.
fn timed(directory: Option<&Path>, arguments: &[&str]) -> (Duration, Output) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_openpapir"));
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let started = Instant::now();
    let output = command
        .args(arguments)
        .output()
        .expect("run the openpapir binary under test");
    (started.elapsed(), output)
}

/// The `count` one listing envelope reports.
fn reported_count(measurement: &Measurement) -> u64 {
    let envelope: serde_json::Value =
        serde_json::from_slice(&measurement.stdout).expect("a listing prints one envelope");
    envelope["data"]["count"]
        .as_u64()
        .expect("a listing reports its count")
}

/// How many receipts one shown case reported.
fn reported_receipts(measurement: &Measurement) -> usize {
    let envelope: serde_json::Value =
        serde_json::from_slice(&measurement.stdout).expect("a shown case prints one envelope");
    envelope["data"]["receipts"]
        .as_array()
        .expect("a shown case reports the receipts section")
        .len()
}

/// Print what was measured, on the machine it was measured on.
///
/// Only counts and durations are printed. No path, no identifier, and no
/// stored value reaches the output.
fn report(cases: usize, setup: Duration, measurements: &[Measurement]) {
    println!("cases: {cases}");
    println!("objects: {}", 2 * cases);
    println!(
        "records: {} (four per case, and one import event per object)",
        6 * cases
    );
    println!("setup: {:.1} s", setup.as_secs_f64());
    for measurement in measurements {
        println!(
            "{}: {:.3} s (ceiling {:.0} s)",
            measurement.name,
            measurement.elapsed.as_secs_f64(),
            measurement.ceiling.as_secs_f64()
        );
    }
}
