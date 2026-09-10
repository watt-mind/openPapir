# Security policy

## Reporting

Report suspected vulnerabilities through
[GitHub private vulnerability reporting][advisory]. Do not open a public issue
or attach real correspondence. Describe the affected commit, expected and
observed behaviour, and a synthetic reproducer if possible. Never include
credentials, private keys, personal information, or real receipt identifiers.

[advisory]: https://github.com/watt-mind/openPapir/security/advisories/new

## Current scope

This repository has no released versions. The tool ingests local files into a
content-addressed archive, persists cases, submissions, receipts, and
associations as local records, checks the stored bytes against what is
recorded, exports a copy of one case, and deletes one case with an explicit
purge flag. It contacts no service, sends no telemetry, verifies no signature,
and delivers nothing to anyone.

No version receives security fixes, because no version is published. Security
fixes target the `develop` branch, and the only supported way to run the tool
is a build from a current checkout. When the first release is published, this
section states which released versions receive fixes; the release position and
the artefact policy are in [releasing](docs/releasing.md).

## Requirements already enforced

Imported documents and receipt metadata are treated as untrusted input. Import
bounds resource use before allocation and expansion, refuses path traversal
and symlink escape, and never replaces a stored object. Imported originals are
preserved byte for byte and anything derived from them is a separate record.
Stored files and directories are created with restrictive permissions, and
openPapir encrypts nothing it stores: the archive is plain files protected by
owner-only permissions and by whatever disk encryption the operating system
provides, so a copy taken out of it is plaintext until the encrypted backup
design below is built. The layout, the caps, and the permissions are in
[local archive layout and storage design](docs/archive-layout.md); the
refusals, their codes, and their exit statuses are in the
[import and association error contract](docs/error-contract.md).

Importing and matching a receipt are not authenticity verification, and no
code in this repository performs one. Report cryptographic results only for
checks actually performed, with their scope and trust context. Never infer
legal effect or delivery from an association.

Public fixtures must be synthetic. Real files, their names, paths, metadata,
payloads, hashes, and signer information must not enter logs, fixtures, issues,
or commits. Never commit secrets, `.env` files, private keys, or a complete PEM
private-key armour line. Generate test keys at runtime if they become necessary;
do not exempt keys from secret scanning.

## Requirements for future features

Government correspondence can contain sensitive personal and business data.
Local-first operation must not silently introduce uploads, telemetry, network
requests, or an automatic delivery path. Each external integration needs an
explicit interface, documented service contract, and user-controlled action.

Backup and migration are not implemented. Review their semantics, and any
change to storage permissions or deletion, before writing the code. An
encrypted backup is now designed in
[local archive layout and storage design](docs/archive-layout.md): it covers
the backup artefact and not the live archive, uses a published AEAD container
over a tarball of the export shape, derives its key from a passphrase the user
holds with a memory-hard KDF, and stores no key anywhere. openPapir must never
write a key, a recovery copy, or a passphrase to disk, and a container that
opens asserts the confidentiality of that copy and nothing about the
authenticity of the originals.
