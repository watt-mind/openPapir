# openPapir

A proposed local-first Rust library and CLI for organising Hungarian government
correspondence: cases, submissions, attachments, and receipts.

**Status: scaffold.** The executable reports its capabilities, help, and
version. Case storage, package import, receipt matching, signature verification,
and government delivery are not implemented. There is no published release.

openPapir is an independent open-source project. It is not the government's
e-Papír service, is not affiliated with its operators, and does not submit
correspondence to it.

## Try the scaffold

Build from this checkout with Rust 1.88 or newer:

```sh
cargo run --locked -p openpapir-cli -- --help
cargo run --locked -p openpapir-cli -- --version
cargo run --locked -p openpapir-cli -- capabilities --json
```

The last command reports the current implementation honestly:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "scaffold",
    "operations": []
  },
  "verified": false
}
```

An empty operation list means no correspondence operations are available.
`verified: false` means no cryptographic verification was performed.

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

## Development

The workspace contains `openpapir-core` and `openpapir-cli`. Both crates are
unpublished, use Rust edition 2024, and are licensed under [MIT](LICENSE).

```sh
./scripts/check.sh
```

Pull requests target `develop`; `master` is reserved for stable releases.
See [contributing](CONTRIBUTING.md), [security](SECURITY.md), and the
[documentation index](docs/index.md). The [roadmap](docs/roadmap.md) records
bounded discovery work before implementation begins.
