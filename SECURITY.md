# Security policy

## Reporting

Report suspected vulnerabilities through
[GitHub private vulnerability reporting][advisory]. Do not open a public issue
or attach real correspondence. Describe the affected commit, expected and
observed behaviour, and a synthetic reproducer if possible. Never include
credentials, private keys, personal information, or real receipt identifiers.

[advisory]: https://github.com/watt-mind/openPapir/security/advisories/new

## Current scope

This repository is a scaffold with no released versions. Only help, version,
and capabilities reporting are implemented. It does not yet ingest files,
persist correspondence, contact government services, or verify signatures.
Security fixes currently target the `develop` branch.

## Requirements for future features

Government correspondence can contain sensitive personal and business data.
Local-first operation must not silently introduce uploads, telemetry, network
requests, or an automatic delivery path. Each external integration needs an
explicit interface, documented service contract, and user-controlled action.

Treat imported documents and receipt metadata as untrusted input. Bound resource
use before allocation and expansion; prevent path traversal, symlink escape,
and unintended replacement of stored files. Preserve imported originals and
record derived data separately. Review storage permissions, backup, migration,
and deletion semantics before implementing persistent cases.

Importing and matching a receipt are not authenticity verification. Report
cryptographic results only for checks actually performed, with their scope and
trust context. Never infer legal effect or delivery from an association.

Public fixtures must be synthetic. Real files, their names, paths, metadata,
payloads, hashes, and signer information must not enter logs, fixtures, issues,
or commits. Never commit secrets, `.env` files, private keys, or a complete PEM
private-key armour line. Generate test keys at runtime if they become necessary;
do not exempt keys from secret scanning.
