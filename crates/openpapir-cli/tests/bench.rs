//! What the linear scans cost on an archive of ten thousand cases.
//!
//! Every listing and the integrity check read every record they could
//! report, because the archive keeps no index. This measurement says what
//! that costs at a size a user might reach, and asserts that each scan stays
//! under a ceiling documented in `docs/architecture.md`.
//!
//! The test is ignored, so it never runs in CI: it builds a temporary archive
//! of forty thousand records and twenty thousand objects first, which takes
//! minutes. `docs/testing.md` says how to run it. Timing is `std::time` only,
//! and no production code is measured through anything but the built binary.

mod bench_support;

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
    let (elapsed, output) = timed(arguments);
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
    let (elapsed, output) = timed(arguments);
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
fn timed(arguments: &[&str]) -> (Duration, Output) {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_openpapir"))
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
