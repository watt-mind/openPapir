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

use std::process::Command;
use std::time::{Duration, Instant};

/// One timed invocation.
struct Measurement {
    /// The invocation, as the documentation names it.
    name: &'static str,
    /// The wall time the run took.
    elapsed: Duration,
    /// The ceiling the run must stay under.
    ceiling: Duration,
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
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(arguments)
        .output()
        .expect("run the openpapir binary under test");
    let elapsed = started.elapsed();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{name} did not succeed on the synthetic archive"
    );
    Measurement {
        name,
        elapsed,
        ceiling,
    }
}

/// Time one invocation the build may not implement yet.
///
/// An invocation this build does not recognise is refused in the usage
/// bucket, which is `2`, and is reported as absent rather than measured. Any
/// other outcome is a real measurement, so an operation is timed from the
/// commit that adds it without this file changing again.
fn optional(name: &'static str, ceiling: Duration, arguments: &[&str]) -> Option<Measurement> {
    let probe = Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(arguments)
        .output()
        .expect("run the openpapir binary under test");
    if probe.status.code() == Some(USAGE_EXIT) {
        println!("{name}: not implemented in this build, not measured");
        return None;
    }
    Some(measure(name, ceiling, arguments))
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
