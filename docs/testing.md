# Testing and fixture policy

## Scaffold checks

Run `./scripts/check.sh` from the repository root. CI adds operating-system and
MSRV coverage, dependency policy, security checks, and a 90% workspace line
coverage threshold for the bootstrap. Keep the threshold meaningful as the
implementation grows; coverage does not replace tests of failure boundaries.

Test the observable capabilities contract and CLI behaviour. Future changes
need tests for their actual risks: hostile archives, ambiguous associations,
resource bounds, storage interruption, and unintended disclosure. Do not write
tests that merely duplicate the implementation.

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
