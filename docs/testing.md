# Testing and fixture policy

## Scaffold checks

Run `./scripts/check.sh` from the repository root. CI adds operating-system and
MSRV coverage, dependency policy, security checks, and a 90% workspace line
coverage threshold for the bootstrap. Keep the threshold meaningful as the
implementation grows; coverage does not replace tests of failure boundaries.

CI also lints the Windows target from its Linux runner, because clippy checks
without linking and the platform-specific code paths otherwise reach a Windows
compiler only in the Windows test job. Run the same lint locally with:

```sh
rustup target add x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --locked \
  --target x86_64-pc-windows-msvc -- -D warnings
```

`./scripts/check.sh` does not run it, so that a checkout without the extra
target still passes the baseline; run it by hand when a change touches
`cfg(windows)` or `cfg(unix)` code.

Test the observable capabilities contract and CLI behaviour. Future changes
need tests for their actual risks: hostile archives, ambiguous associations,
resource bounds, storage interruption, and unintended disclosure. Do not write
tests that merely duplicate the implementation.

## Property tests

The boundaries that accept input openPapir did not mint are covered
generatively as well as by example. The suite lives in
`crates/openpapir-core/tests/property/` for the library and in
`crates/openpapir-cli/tests/property.rs` for the binary. Every property module
and every property in it states its invariant in a documentation comment: a
record document reader answers or refuses and never panics, a document over
the record cap is refused before its bytes are read while one inside it is
read and then refused on its content, a valid record round-trips byte for byte
with sorted keys, a name carrying a `..`, a separator, a NUL, or an overlong
component never becomes a path, an export writes a well-formed manifest or a
documented refusal, an export manifest read back from a user-supplied
directory answers or refuses with a documented code and never panics, and the
argument parser answers with one envelope that never quotes the caller.

The manifest is a boundary in both directions, so it carries a property for
each. The writer's property asserts the manifest an export leaves behind; the
reader's asserts that any bytes at `manifest.json` read back as a manifest or
are refused, whether the manifest is there as a regular file, as a symbolic
link, or not at all; that a manifest this build wrote reads back with exactly
the rows it lists; and that a manifest carrying one difference the reader
checks for is refused with the code that check answers with. The differences
are the ones the reader documents: a key the document cannot be parsed
without, a value of the wrong type, a record kind this build does not know, a
digest that is not the store's own shape, an identifier shaped like a path,
and an archive schema version this build does not support. The first five are
`export.manifest_malformed`; the last is refused by the archive's own schema
rules, because a manifest from a build this one does not support is a document
this build has no business reading rather than a broken one. Every code the
reader may answer is reached by a generated case and pinned to the case that
must produce it, so no admitted code goes untested. An import event is
round-tripped with its `source` field both absent and present, because the
field is written only when it is there and an absent key has to survive the
round trip as an absence rather than as a default the reader filled in.

A property whose subject has two answers needs both of them reached. Random
bytes are never a valid record, so the reader properties pair the random-byte
generator with a generated valid record carrying one generated difference:
a difference the reader is documented to ignore must still read back, and one
it checks must be refused. Prefer that shape to a property that accepts either
answer, which can pass without ever reaching one of them.

Run the suite with:

```sh
cargo test --workspace --locked property
```

Each property carries its own committed case count, chosen so that the whole
suite finishes in seconds in continuous integration and well inside a minute
on a slow runner. The counts are deliberate: a property that creates an
archive or spawns a process pays for every case, so it runs fewer of them than
a pure one.

`PROPTEST_CASES` overrides every committed count, which is how a contributor
runs a longer soak locally or in a scheduled job:

```sh
PROPTEST_CASES=2000 cargo test --workspace --locked property
```

Failure persistence is off, so a counterexample is never written beside the
source. When a property finds one, proptest prints the shrunk input; add it to
the suite as a named regression test with the fix, so the case is pinned by
name rather than by a generated file.

Property tests do not replace the example-based tests of failure boundaries,
and they are never the place to weaken an assertion so that a generator can
pass.

## Scan benchmark

Every listing and the integrity check are linear scans over the records they
could report, because the archive keeps no index, so what they cost grows with
what the archive holds. `crates/openpapir-cli/tests/bench.rs` measures that on
a synthetic archive of ten thousand cases and asserts that each scan stays
under the ceiling in the Performance section of
[architecture](architecture.md), which also records the last measured numbers.

The test is ignored, so no CI job runs it and `./scripts/check.sh` only
compiles it. Run it deliberately:

```sh
cargo test --release --locked -p openpapir-cli --test bench -- --ignored --nocapture
```

`--release` measures the optimised binary, which is the one a user runs. The
same command without it measures the unoptimised test profile, which is
slower and which the ceilings still allow.

`OPENPAPIR_BENCH_CASES` sets the size, and the default is 10000. Building the
archive is the slow part, not the measurements: it imports 20000 objects and
writes 10000 cases, submissions, receipts, and associations, which took about
11 minutes with `--release` on the machine the Performance section names. Use
`--release` at the default size. Without it the same setup takes hours, for
the reason that section records: recording an import event reads the import
events already stored, so the cost of importing grows with what the archive
already holds. At `OPENPAPIR_BENCH_CASES=2000` the unoptimised profile builds
its archive in about a minute, which is enough to watch the ceilings hold.

The archive is a temporary directory and is removed when the test ends. It
needs about 230 MB of free space in the temporary directory at the default
size, most of it filesystem overhead for 20000 small objects and their
records.

The generator in `crates/openpapir-cli/tests/bench_support/` builds that
archive through the library API rather than the binary, because forty thousand
process starts would measure the process starts. Every byte it stores comes
from a counter and a mixing function, so it is wholly synthetic in the sense
the fixture policy below requires and is reproducible from this repository
alone. It stays inside the input caps: the objects are imported in batches of
the per-import file cap.

## Concurrency tests

`crates/openpapir-cli/tests/concurrency.rs` covers what happens when two
openPapir processes touch one archive at once. These are example-based tests
rather than properties: a property generates an input and shrinks a
counterexample, and the subject here is a schedule, which nothing shrinks.

Two guarantees are asserted. Several processes running `submission add`
against one archive at the same time each either succeed or refuse with
`lock.held`, and afterwards the archive holds exactly what the successful ones
reported, the lock file is gone, and `archive check` is clean with no leftover
staging file. A reader running `case list` while another process publishes
takes a listing that shows the set before the publication or the set after it,
never a document part way through being written, which is what the atomic
rename of a fully written staging file buys; the test asserts it by parsing
every listing, checking every field of every case in it, and requiring that no
listing loses a case an earlier one showed.

Neither test asserts that contention actually occurred. Whether two processes
overlap is the operating system's business, and a test that demanded an
overlap would fail on a machine that scheduled them apart. What is asserted is
that every observed outcome is a permitted one, which holds either way.

## Fixtures

Public fixtures live under `tests/fixtures/` and must be wholly synthetic and
redistributable under the fixture directory's CC0 policy. The repository
contains no correspondence fixture. Record each future fixture's generator or
source, intended case, and expected outcome in that directory's README.

Never derive fixtures from private correspondence, even after redaction.
Generate any needed cryptographic test keys at runtime and never commit keys or
complete private-key PEM armour lines. Avoid external network dependencies in
tests; use synthetic service doubles when integrations are eventually added.

## Private inputs

Private testing is not implemented. If added, it must be explicit opt-in,
outside public fixtures, and absent from CI. Never enumerate or expose private
paths, filenames, contents, metadata, hashes, certificates, or receipt identifiers.
Report only aggregate counts and stable error-code buckets. A failing private
sample must be reproduced synthetically before adding a public regression test.
