# openPapir

A proposed local-first Rust library and CLI for organising Hungarian government
correspondence: cases, submissions, attachments, and receipts.

**Status: alpha**, which is the stage the tool itself reports. The
executable reports its capabilities, creates a local archive, imports files
into a content-addressed store that preserves the original bytes, organises
what it holds into cases and submissions, records receipts together with the
user's own assertions about whether a receipt relates to a submission, checks
a whole archive against what its records claim without changing anything,
copies one case out of the archive as plain files, narrows a restored
archive's permissions back to owner-only, deletes a case when asked, removing
stored bytes only on an explicit `--purge`, and writes the agent skill
document it carries. Automatic matching, derived metadata, receipt parsing,
KRX and `.es3` handling, import from an export, editing of a stored record,
deleting a single submission or receipt, deleting an archive, signature
verification, and government delivery are not implemented. There is no
published release.

openPapir is an independent open-source project. It is not the government's
e-Papír service, is not affiliated with its operators, and does not submit
correspondence to it.

## Try it

Build from this checkout with Rust 1.88 or newer:

```sh
cargo run --locked -p openpapir-cli -- --help
cargo run --locked -p openpapir-cli -- --version
cargo run --locked -p openpapir-cli -- capabilities --json
cargo run --locked -p openpapir-cli -- archive init ./my-archive --json
cargo run --locked -p openpapir-cli -- import --archive ./my-archive ./a-file --json
cargo run --locked -p openpapir-cli -- case create --archive ./my-archive --title "Tax matter" --json
cargo run --locked -p openpapir-cli -- case list --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- submission add --archive ./my-archive --case <case-id> --description "Posted the form." --json
cargo run --locked -p openpapir-cli -- case show --archive ./my-archive <case-id> --json
cargo run --locked -p openpapir-cli -- receipt add --archive ./my-archive --artefact sha256:<digest> --label "Envelope" --json
cargo run --locked -p openpapir-cli -- receipt list --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- association create --archive ./my-archive --receipt <receipt-id> --outcome candidate --candidate "<submission-id>:moderate:The reference matches." --json
cargo run --locked -p openpapir-cli -- association list --archive ./my-archive --receipt <receipt-id> --json
cargo run --locked -p openpapir-cli -- archive check --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- case export --archive ./my-archive --case <case-id> --to ./my-export --json
cargo run --locked -p openpapir-cli -- archive repair-permissions --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- case delete --archive ./my-archive --case <case-id> --purge --json
cargo run --locked -p openpapir-cli -- skill
```

The capabilities command reports the current implementation honestly:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "alpha",
    "operations": [
      "archive.init",
      "import",
      "case.create",
      "case.list",
      "case.show",
      "submission.add",
      "receipt.add",
      "receipt.list",
      "association.create",
      "association.list",
      "archive.check",
      "case.export",
      "archive.repair_permissions",
      "case.delete",
      "skill"
    ]
  },
  "verified": false
}
```

The operation list names exactly what is implemented today, one sentence
each. All but the last can process input:

- `archive init` creates an archive in an existing, empty directory, writing
  the marker first and refusing to adopt anything else.
- `import` stores each file's bytes unchanged, records one import event per
  input, and reports a re-import of the same bytes as a duplicate rather than
  an error.
- `case create` records one case, the user's own folder of related
  correspondence.
- `case list` lists every case in the archive.
- `case show` shows one case with the submissions recorded against it and
  their artefact references.
- `submission add` records one submission the user states they sent, against
  a case.
- `receipt add` records that the user believes one stored artefact to be a
  receipt.
- `receipt list` lists every receipt in the archive.
- `association create` records what the user asserts about one receipt, with
  one of the outcomes `unassociated`, `candidate`, `associated`, and
  `contradictory`.
- `association list` lists one receipt's whole association history, newest
  first, superseded records included.
- `archive check` re-digests what the store holds and reports counts only; it
  takes no lock, changes nothing, and a passing check is storage integrity
  rather than authenticity.
- `case export` copies one case out as plain files, the original bytes named
  by their digest plus readable JSON records and a manifest, without changing
  the archive, and re-digests every copy.
- `archive repair-permissions` narrows a restored archive back to owner-only;
  it never widens anything.
- `case delete` is the one destructive command: it removes a case and its
  submissions, and the receipts and associations tied only to them, but it
  removes no stored bytes unless `--purge` is given, and even then only bytes
  nothing that remains references. It reports counts and record kinds, never a
  digest or a filename, records no deletion anywhere, and does not erase data
  from the storage medium.
- `skill` writes the embedded agent skill document to stdout byte for byte and
  adds nothing. It takes no file and no `--json`, touches no archive, and is
  the one operation that processes no input.

Every record is the user's own local record: openPapir sends nothing and reads
no artefact bytes, so a submission is what the user states they sent, a
receipt is an artefact the user believes to be one, an association is what the
user asserts about it, and a date they supply is stored verbatim and never
read as a delivery or receipt date. `verified: false` means no cryptographic
verification was performed: a digest identifies bytes, and says nothing about
authenticity or delivery. The full contract, including the error codes and
exit codes, is in [architecture and CLI contract](docs/architecture.md).

## Intended responsibilities

| Responsibility | State |
| --- | --- |
| Preserve original submission and receipt bytes in a local case archive. | Implemented by `import` and the write-once artefact store. |
| Associate submissions, attachments, and receipts with explicit provenance. | Implemented for the user's own assertions; automatic matching, derived metadata, and receipt parsing are not implemented. |
| Expose case information through a CLI and structured JSON. | Implemented by the list, show, and export commands; search is not implemented. |
| Delegate KRX container processing to [openKRX](https://github.com/watt-mind/openKRX) and `.es3` processing to [openSzigno](https://github.com/watt-mind/openSzigno). | Not implemented. Neither sibling project is a build dependency of this project, and neither parser is copied into it. |
| Delegated authenticity verification, reported with its exact scope and trust context. | Not implemented. No cryptographic check of any kind exists here. |
| Government submission and delivery. | Not implemented and out of scope for now. |

Importing a receipt, matching it to a submission, and verifying its
cryptographic authenticity remain separate records: an association changes
nothing about the artefact and creates no verification result. Matching alone
must never assert legal effect or successful delivery.

## Agents and automation

Every command has two modes, and they are the same contract seen twice. With
`--json` exactly one object reaches stdout and nothing reaches stderr, so a
program reads a result without parsing prose. Branch on the exit code first,
which carries the error's bucket and nothing else, then on `error.code`, which
is stable within a `schema_version`; `message` wording is not. `verified` is
`false` in every envelope this build emits.

```console
$ openpapir receipt add --archive ./archive --artefact sha256:5e0d7d51... \
    --label "Envelope from the post" --json
{"schema_version":1,"ok":true,"command":"receipt.add","data":{"receipt":{"archive_schema_version":1,"artefact_digest":"sha256:5e0d7d511a1df60c20dc2d589f7f1d13b87019603a724a938c71e81a8d7e1dac","created_at":"2026-09-09T16:10:57Z","id":"71df7d0ed619cc29d4478e0db8d014b6","import_event_id":"db11ed295d6b8d7c3edebf4ab251de78","label":"Envelope from the post","record_kind":"receipt"}},"verified":false}
```

The binary carries an agent skill document describing when to reach for
openPapir, every command's exact invocation, the envelope, the exit codes, the
privacy rule, and the boundary between imported, matched, and
authenticity-verified. `openpapir skill` writes it to stdout byte for byte and
nothing else, so installing it needs no checkout:

```sh
mkdir -p .claude/skills/openpapir
openpapir skill > .claude/skills/openpapir/SKILL.md
```

Use `.codex/skills/openpapir/` for Codex, or `~/.claude/skills/openpapir/` to
install it for every project instead of one. The same bytes are committed as
[the agent skill](crates/openpapir-cli/skills/openpapir/SKILL.md).

Both modes are pinned byte for byte by the captured output under
[tests/golden](tests/golden/README.md), so a change to what a caller parses is
a reviewed change rather than an accident.

## People

Without `--json` the same command prints lines meant to be read, with the
result on stdout and any warning or error on stderr, so diagnostic text never
shares stdout with a result. The lines carry the user's own titles, labels, and
statements, the identifiers and digests openPapir minted, and no path,
filename, or payload byte.

```console
$ openpapir receipt add --archive ./archive --artefact sha256:5e0d7d51... \
    --label "Envelope from the post"
Receipt 8bbfed851cf9dedefc80442ca4f80a0d, recorded 2026-09-09T16:10:57Z.
Artefact: sha256:5e0d7d511a1df60c20dc2d589f7f1d13b87019603a724a938c71e81a8d7e1dac
Import event: db11ed295d6b8d7c3edebf4ab251de78
Label: Envelope from the post
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The closing line is not decoration. A receipt is a file the user believes to
be one, an association is what the user asserts about it, and neither is a
verification: openPapir reads no artefact bytes and opens no socket.

## Documentation

The [specification index](docs/specification.md) is the single entry point to
the project's scope, what is implemented today, which designs are decided but
not built, and which contracts are still blocked. The
[documentation index](docs/index.md) gives every document and root policy file
a one-line purpose, including
[the agent skill](crates/openpapir-cli/skills/openpapir/SKILL.md) and the
[golden output contract](tests/golden/README.md).

## Development

The workspace contains `openpapir-core` and `openpapir-cli`. Both crates are
unpublished, use Rust edition 2024, and are licensed under [MIT](LICENSE).

```sh
./scripts/check.sh
```

Pull requests target `develop`; `master` is reserved for stable releases.
See [contributing](CONTRIBUTING.md), whose Documentation section states how
the documents are kept correct, and [security](SECURITY.md). The
[roadmap](docs/roadmap.md) records bounded discovery work before
implementation begins.
