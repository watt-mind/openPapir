# Golden output contract

Every file in this directory is captured output of the openPapir CLI. Taken
together they are the machine-checked form of the contract described in
[architecture and CLI contract](../../docs/architecture.md): the JSON
envelope, the human renderer's lines, the two streams they are written to, and
the exit codes.

The harness is `crates/openpapir-cli/tests/golden.rs`, with its synthetic world
in `crates/openpapir-cli/tests/golden_support/` and the file-by-file
comparison in `crates/openpapir-cli/tests/golden_support/compare.rs`, whose
own tests run against a temporary copy of one case and never touch this
directory. It runs as part of
`cargo test --workspace`, builds nothing of its own, and reaches no network.

## A difference here is a contract change

A golden file changes only because the tool's output changed. That is not a
test to be repaired; it is a change to what every consumer parses, and it is
reviewed under the `schema_version` rule.

| Change | What it requires |
| --- | --- |
| A new field, a new code, a new human line | Additive. `schema_version` stays `1`. Regenerate the goldens and add a `CHANGELOG.md` entry under Unreleased. |
| A removed or renamed field, a changed type, a changed meaning | Breaking. It needs a `schema_version` bump and the same pull request updating [architecture](../../docs/architecture.md). |
| A changed exit code for an existing outcome | Breaking. The exit code carries the error's bucket and nothing else. |
| A user-supplied path, an original filename, or a payload byte appearing anywhere | A bug, never an accepted diff. See [privacy of output](../../docs/architecture.md#privacy-of-output). |

Regenerating goldens to make a red build green, without deciding which row
above the change falls under, defeats the point of the directory.

## Regenerating deliberately

Set the environment variable and run the harness. Nothing regenerates on its
own, and CI never sets the variable: there the harness only compares.

```sh
OPENPAPIR_UPDATE_GOLDEN=1 cargo test --locked -p openpapir-cli --test golden
git diff tests/golden
```

Read the diff before committing it, row by row against the table above.

## Layout

```text
tests/golden/<case>/json.json
tests/golden/<case>/json.exit
tests/golden/<case>/human.txt
tests/golden/<case>/human.stderr.txt
tests/golden/<case>/human.exit
```

| File | Contents |
| --- | --- |
| `json.json` | Stdout of the `--json` run, re-rendered with sorted keys and an indent so a diff is readable. The binary writes one compact line. |
| `json.exit` | The exit code of that run. |
| `human.txt` | Stdout of the same invocation without `--json`. |
| `human.stderr.txt` | Stderr of that run: the warnings and the error, which the human form writes there so diagnostics never share stdout with a result. |
| `human.exit` | The exit code of that run. |

All five files are required. A file that is not on disk is reported as a
difference named `is missing`, never read as an empty expectation: most cases
pin an empty `human.stderr.txt`, so a deleted golden would otherwise compare
equal to the capture and pass in silence. Restore the file from git, or
regenerate the case deliberately.

Stderr of a `--json` run has no file, because the harness asserts it is empty
for every case. That is the contract: in the JSON form exactly one object
reaches stdout and nothing at all reaches stderr. The harness also asserts
that no `json.json` carries the `<root>` placeholder, because no JSON field
may carry a user-supplied path.

Each case runs twice against two separately built archives, so the human run
never observes what the JSON run wrote.

## The cases

| Case | Invocation |
| --- | --- |
| `capabilities` | `capabilities` |
| `usage.arguments` | `import`, with the required arguments missing |
| `archive.init` | `archive init <root>` on an existing empty directory |
| `import.success` | `import` of the three synthetic payloads |
| `import.duplicate` | `import` of the first payload again |
| `import.cap-refusal` | `import` of 1001 inputs, one over the per-import file cap |
| `case.create` | `case create --title --notes` |
| `case.list` | `case list` over one case |
| `case.show` | `case show` with one submission recorded |
| `submission.add` | `submission add` with a stated date and one artefact |
| `receipt.add` | `receipt add` with a label |
| `receipt.list` | `receipt list` over one receipt |
| `association.create.unassociated` | `association create --outcome unassociated` |
| `association.create.candidate` | The same with one candidate |
| `association.create.associated` | The same with exactly one candidate |
| `association.create.contradictory` | The same with two candidates |
| `association.list` | `association list` over a two-record history |
| `association.retire` | `association retire --reason` over the live record of that history |
| `archive.check.clean` | `archive check` on an undamaged archive |
| `archive.check.damaged` | `archive check` after one stored object's bytes were replaced |
| `case.delete` | `case delete` without `--purge`, which touches no object |
| `case.delete.purge` | `case delete --purge`, which unlinks the objects nothing left references |
| `case.export` | `case export --to` a destination the export creates |
| `archive.repair-permissions` | `archive repair-permissions` after one layout directory was widened |

Every archive is built from constants in
`crates/openpapir-cli/tests/golden_support/mod.rs` and nothing else: three
payloads of 27, 23, and 27 bytes, one case, one or two submissions, one
receipt, and two associations, always in that order. The damaged case replaces
the first stored object with 27 different bytes, so the check reports a digest
mismatch and nothing else. Nothing here is derived from real correspondence,
and the captured output is dedicated under CC0 like every other fixture
([fixture policy](../fixtures/README.md)).

## The placeholders, and nothing else

Four values move between runs. Each is replaced with a placeholder that is
deliberately not a valid value of its own kind, so a golden that still holds a
real reading is obvious on sight.

| Placeholder | What it replaces |
| --- | --- |
| `<id>` | A run of exactly 32 lowercase hexadecimal characters on a word boundary: an archive, case, submission, receipt, association, or import-event identifier, which is 128 random bits. |
| `<digest>` | A run of exactly 64 lowercase hexadecimal characters on a word boundary, so `sha256:<digest>` is what a stored artefact reads as. |
| `<time>` | An instant of the shape `YYYY-MM-DDTHH:MM:SSZ`, the only shape openPapir records. |
| `<root>` | The temporary directory the case was built in, which appears only in the human output of `case export`, because that line repeats the `--to` argument the user typed. |

Nothing else is normalised. Byte counts, cap values, every count in a report,
the user's own titles, notes, descriptions, labels, statements, and stated
dates, the order of every array, and every line of wording are part of the
contract and are compared exactly as they were produced. A stated date such as
`2026-01-13` is the user's own text and is never masked; only the instants
openPapir recorded itself are.

## Why the harness is captured on Unix

The harness is compiled only on Unix. On Windows the same commands correctly
report the `platform.owner_only_via_acl` and `platform.no_follow_after_open`
warnings, and the permission counts of `archive repair-permissions` describe
access-control lists rather than mode bits, so one pinned file could not
describe both platforms honestly. Windows behaviour is covered by the other
integration tests under `crates/openpapir-cli/tests/`, which assert the
contract rather than pin the bytes.
