# openPapir

A proposed local-first Rust library and CLI for organising Hungarian government
correspondence: cases, submissions, attachments, and receipts.

**Status: early.** The executable reports its capabilities, creates a local
archive, and imports files into a content-addressed store that preserves the
original bytes. Case storage, receipt matching, association, export, deletion,
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
    "operations": ["archive.init", "import"]
  },
  "verified": false
}
```

The operation list names exactly what can process input today. `archive init`
needs an existing, empty directory and refuses to adopt anything else.
`import` stores each file's bytes unchanged, records one import event per
input, and reports a re-import of the same bytes as a duplicate rather than an
error. `verified: false` means no cryptographic verification was performed: a
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
cryptographic authenticity will remain separate states. Matching alone must
never assert legal effect or successful delivery.

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
