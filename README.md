# openPapir

A proposed local-first Rust library and CLI for organising Hungarian government
correspondence: cases, submissions, attachments, and receipts.

**Status: scaffold**, which is the stage the tool itself reports. The
executable reports its capabilities, creates a local archive, imports files
into a content-addressed store that preserves the original bytes, organises
what it holds into cases and submissions, records receipts together with the
user's own assertions about whether a receipt relates to a submission, and
checks a whole archive against what its records claim without changing
anything, copies one case out of the archive as plain files, narrows a
restored archive's permissions back to owner-only, and deletes a case when
asked, removing stored bytes only on an explicit `--purge`. Automatic
matching, derived metadata, receipt parsing, import from an export, editing of
a stored record, deleting a single submission or receipt, deleting an archive,
signature verification, and government delivery are not implemented. There is
no published release.

openPapir is an independent open-source project. It is not the government's
e-Papír service, is not affiliated with its operators, and does not submit
correspondence to it.

## Try the scaffold

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
```

The capabilities command reports the current implementation honestly:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "scaffold",
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
      "case.delete"
    ]
  },
  "verified": false
}
```

The operation list names exactly what can process input today. `archive init`
needs an existing, empty directory and refuses to adopt anything else.
`import` stores each file's bytes unchanged, records one import event per
input, and reports a re-import of the same bytes as a duplicate rather than an
error. Every record is the user's own local record: openPapir sends nothing
and reads no artefact bytes, so a submission is what the user states they
sent, a receipt is an artefact the user believes to be one, an association is
what the user asserts about it, and a date they supply is stored verbatim and
never read as a delivery or receipt date. `archive check` re-digests what the
store holds and reports counts only; it takes no lock, changes nothing, and a
passing check is storage integrity rather than authenticity. `case export`
copies one case out as plain files, the original bytes named by their digest
plus readable JSON records and a manifest, without changing the archive, and
re-digests every copy. `archive repair-permissions` narrows a restored
archive back to owner-only; it never widens anything. `case delete` is the one
destructive command: it removes a case and its submissions, and the receipts
and associations tied only to them, but it removes no stored bytes unless
`--purge` is given, and even then only bytes nothing that remains references.
It reports counts and record kinds, never a digest or a filename, records no
deletion anywhere, and does not erase data from the storage medium.
`verified: false` means no cryptographic verification was performed: a
digest identifies bytes, and says nothing about authenticity or delivery.
The full contract, including the error codes and exit codes, is in
[architecture and CLI contract](docs/architecture.md).

## Intended responsibilities

- Preserve original submission and receipt bytes in a local case archive.
- Associate submissions, attachments, and receipts with explicit provenance.
- Expose searchable case information through a CLI and structured JSON.
- Delegate KRX container processing to
  [openKRX](https://github.com/watt-mind/openKRX) and `.es3` processing to
  [openSzigno](https://github.com/watt-mind/openSzigno).

Those are roadmap goals. Neither sibling project is a build dependency of this
scaffold. Importing a receipt, matching it to a submission, and verifying its
cryptographic authenticity remain separate records: an association changes
nothing about the artefact and creates no verification result. Matching alone
must never assert legal effect or successful delivery.

## Documentation

The [specification index](docs/specification.md) is the single entry point to
the project's scope, what is implemented today, which designs are decided but
not built, and which contracts are still blocked. The
[documentation index](docs/index.md) gives every document and root policy file
a one-line purpose.

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
