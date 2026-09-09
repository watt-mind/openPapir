# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

There are no published releases. Every entry below is unreleased work on
`develop`. The `capabilities` JSON envelope is versioned separately by its
`schema_version` field, which is `1`.

Every pull request adds an entry under Unreleased, in the category that
matches the observable difference. See the Documentation section of
[CONTRIBUTING.md](CONTRIBUTING.md).

## Unreleased

### Added

- `openpapir archive init <root>` creates a local archive in an existing,
  empty directory: the `papir-archive.json` marker is written first, then an
  owner-only layout of `objects/`, `records/`, and `cache/`. openPapir never
  searches for an archive and never adopts a directory that has no marker.
- `openpapir import --archive <root> <file>...` stores each file's original
  bytes, unchanged, in a write-once `objects/sha256/ab/cd/<digest>` store and
  records one import event per input under `records/imports/`. Re-importing
  bytes already present is not an error: the object is untouched, a second
  event is recorded, and the response reports `previous_import_count` and
  `first_imported_at`.
- Both commands accept `--json` and emit the response envelope of
  `docs/error-contract.md`, with `ok`, an `error` object carrying a stable
  code and bounded details, a `warnings` array for platform degradation, and
  exit codes `0`, `2`, `3`, `4`, `5`, or `6` by bucket. `1` is never emitted,
  and `verified` stays `false` everywhere.
- Storage guarantees behind both commands: atomic writes that leave the
  complete file or nothing, a publish step that cannot replace a file
  openPapir did not create, owner-only permissions with no override and
  read-only stored objects, a single-writer lock that refuses a second writer
  rather than waiting, input caps checked before allocation and again while
  streaming, and path safety that opens inputs without following a symbolic
  link and never joins a supplied filename into a path.

- A Rust edition 2024 workspace with two unpublished crates,
  `openpapir-core` and `openpapir-cli`, and the `--help`, `--version`, and
  `capabilities [--json]` commands.
- Project boundaries, privacy rules, and a discovery-first roadmap, together
  with the local check script and the continuous integration, dependency
  policy, and security workflows that enforce them.
- Portable master orchestrator instructions and runner setup, so an
  explicitly launched orchestration run has a documented, private-by-default
  workflow (#1).
- A discovery note recording what authoritative public sources state about
  one candidate receipt type, a proposed minimal local case model, and the
  questions that stay open (#2).
- A design for review of the local archive: storage technology, on-disk
  layout, record shapes, association states, and deletion semantics (#4).
- A specification for review of the import and association error model: the
  extended JSON envelope, the stable error-code catalogue, and the exit-code
  mapping. Nothing in it is implemented and the capabilities output is
  unchanged by it (#5).
- `docs/specification.md`, the top-level index of purpose, scope, non-goals,
  implemented behaviour, decided designs, deferred contracts, and the three
  separated receipt states.
- `docs/releasing.md`, recording that no release process exists, that
  promotion from `develop` to `master` is a human decision, and the checklist
  a first release would need.
- A Documentation section in `CONTRIBUTING.md` stating where each kind of
  content lives, the prose style rules, the rule that every file under
  `docs/` is listed in `docs/index.md`, the documents a behaviour change must
  update in the same pull request, and a table of every check with the
  command that runs it.
- `scripts/check-prose.py`, which fails on an em-dash (U+2014) in tracked
  Markdown outside a fenced code block. `scripts/check.sh` runs it.
- Crate-level documentation on `openpapir-core` and `openpapir-cli` stating
  each crate's responsibility, the scaffold status, the capabilities envelope
  contract, and the boundaries the crates must not cross.

### Changed

- `capabilities` now reports the operations that are implemented,
  `archive.init` and `import`, instead of an empty list. `verified` stays
  `false` and the envelope's shape is unchanged.
- `docs/architecture.md` documents the two commands as the implemented
  contract: the envelope, the storage guarantees, the caps, the implemented
  error codes with their exit codes, the platform degradations, and the
  privacy rule. It also records the three conditions the error contract
  deferred to this change and how each was decided.
- The workspace gains `sha2`, `getrandom`, `libc` (Unix only), and `tempfile`
  as a development dependency.
- The owner-only check covers the archive root, the marker, the lock file,
  every layout directory, and each stored object and fan-out directory an
  operation touches, and it refuses before anything is published. openPapir
  never narrows an existing path as a side effect: a wider one is reported,
  not repaired, because the design allows narrowing only through an explicit
  repair action that does not exist yet.
- A directory flush that fails is now reported as the
  `platform.no_directory_fsync` warning instead of being ignored.

- Tracked Markdown no longer uses em-dashes in prose, per the Documentation
  style rules in `CONTRIBUTING.md`. The three design documents are rewritten
  with periods, commas, colons, or parentheses; no decision, code name, cap
  value, status label, link, or JSON example changed.
- `docs/index.md` is now a two-table index that gives every document under
  `docs/` and every root policy file a one-line purpose.
- `AGENTS.md` gains a repository layout table naming what lives at each path
  and who may change it, plus the documentation-maintenance rule.
- `README.md` links the specification index and the documentation index.
- The pull request template requires the affected documents, any new code,
  flag, or field, a changelog entry, and any new fixture to be covered by the
  same pull request.

### Fixed

- The pinned minimum-supported-Rust action no longer receives an unsupported
  toolchain input, which had failed the MSRV job (#3).
