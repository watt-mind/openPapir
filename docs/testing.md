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
`crates/openpapir-cli/tests/property.rs` for the binary, and every test states
one invariant in its own documentation comment: a record document reader
answers or refuses and never panics, a document over the record cap is refused
before its bytes are read, a valid record round-trips byte for byte with
sorted keys, a name carrying a `..`, a separator, a NUL, or an overlong
component never becomes a path, an export writes a well-formed manifest or a
documented refusal, and the argument parser answers with one envelope that
never quotes the caller.

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
