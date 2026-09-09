# Synthetic fixtures

No correspondence fixtures are included. The tests for archive creation and
artefact import generate every input at test time, from constants in the test
files themselves, into a temporary directory that is removed afterwards.

| Fixture | Generator | Purpose | Expected outcome |
| --- | --- | --- | --- |
| A 16-byte text payload | `crates/openpapir-cli/tests/archive.rs`, the `PAYLOAD` constant | Import, byte preservation, and duplicate import | Stored under its SHA-256 digest, byte for byte, with one import event per import |
| Sparse files of 64 MiB and 60 MiB | The same file, `sparse_input` | The single-file and per-import byte caps | Refused before any byte is read, with `input.cap.file_size` or `input.cap.import_bytes` |
| Names that look like traversal, and over-long names | The same file, `write_input` and a 256-byte name | Path safety and the filename cap | Stored as an attribute only, or refused with `input.cap.filename_length`; never joined into a path |

The captured CLI output under [tests/golden](../golden/README.md) is covered
by the same policy. Every byte in it comes from an archive the golden harness
builds from constants in `crates/openpapir-cli/tests/golden_support/`, no part
of it is derived from real correspondence, and it is dedicated under CC0 like
every other fixture. It is regenerated only deliberately, never automatically.

Any future fixture committed here must be created independently from invented
data and covered by [CC0](LICENSE). Record its provenance or generation
method, purpose, and expected outcome in the table above. Never adapt or
redact a private document into a public fixture. Generate any test private
keys at runtime; never commit keys or complete private-key PEM armour lines.
