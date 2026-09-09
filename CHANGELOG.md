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

- A Rust edition 2024 workspace with two unpublished crates,
  `openpapir-core` and `openpapir-cli`, and the `--help`, `--version`, and
  `capabilities [--json]` commands. The JSON envelope reports
  `schema_version: 1`, an empty `operations` list, and `verified: false`,
  because no correspondence operation and no cryptographic check exist.
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
