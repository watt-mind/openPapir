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

- `openpapir receipt add --archive <root> --artefact <digest> [--import-event
  <id>] [--label <l>]` and `openpapir receipt list --archive <root>` record
  and list receipts under `records/receipts/`. A receipt is an artefact the
  user believes to be a receipt: it names a stored object and the import event
  that introduced it, never rewrites the artefact, and asserts nothing about
  the file's type or authenticity. With no `--import-event` the earliest event
  for the digest is recorded; a named event must record that same artefact.
- `openpapir association create --archive <root> --receipt <receipt-id>
  --outcome <outcome> [--candidate <submission-id>:<confidence>:<statement>]...
  [--supersedes <association-id>]` and `openpapir association list --archive
  <root> --receipt <receipt-id>` record and list user-asserted associations
  under `records/associations/`. All four outcomes, `unassociated`,
  `candidate`, `associated`, and `contradictory`, are results rather than
  errors and exit `0`. Confidence is the closed ordinal set `weak`,
  `moderate`, `strong` and never a number. Every evidence entry carries `kind`
  `user_assertion` and `source` `user`, and every record carries `created_by`
  `user`, because openPapir reads no artefact bytes and matches nothing on its
  own.
- Associations are append-only. `--supersedes` names an earlier association
  for the same receipt and never modifies it, and `association list` returns
  the whole history newest first, superseded records included, each showing
  what it supersedes. Nothing is collapsed, filtered, or presented as a single
  best guess.
- New error code `record.inconsistent`, a `record` refusal that exits `4` and
  is never retryable, for fields that are readable but cannot be true
  together. Its details carry `record_kind` and a short, stable `rule` name
  and nothing else. The rules are `unassociated_has_candidates`,
  `candidate_requires_candidates`, `associated_requires_one_candidate`,
  `contradictory_requires_two_candidates`, `duplicate_candidate_submission`,
  `supersedes_other_receipt`, and `import_event_digest_mismatch`.
- Field caps refused before any write: a receipt `label` of 200 bytes and an
  evidence `statement` of 512 bytes, both single lines, both reported through
  `input.cap.field_length`. `capabilities` now reports ten operations, adding
  `receipt.add`, `receipt.list`, `association.create`, and
  `association.list`.
- Opening an archive created by an earlier build adds `records/receipts/` and
  `records/associations/` if they are absent; nothing else changes.

Automatic association, derived metadata, extractors, receipt parsing, and
verification results stay unimplemented, and no output or field claims
delivery, receipt by an authority, authenticity, or legal effect.

- `openpapir case create --archive <root> --title <t> [--notes <n>]`,
  `openpapir case list --archive <root>`, and
  `openpapir case show --archive <root> <case-id>` record, list, and show
  cases under `records/cases/`. A case is the user's own folder of related
  correspondence and corresponds to nothing any government service issues.
- `openpapir submission add --archive <root> --case <case-id> --description
  <d> [--date <yyyy-mm-dd>] [--artefact <digest>[:<role>]]...` records a
  submission under `records/submissions/`, referencing its case by identifier
  and each artefact by the digest of an object already stored in the archive,
  with the short role label the user gave it. openPapir sends nothing, so a
  submission is what the user states they sent, and `--date` is stored
  verbatim, never interpreted, and never read as a delivery or receipt date.
- Records are one UTF-8, LF-terminated JSON document each, with sorted keys,
  carrying `id`, `record_kind`, `archive_schema_version`, and `created_at`,
  written through the existing atomic write procedure under the writer lock
  and owner-only permissions. Listing and showing take no lock.
- Field caps refused before any write, with the new `input.cap.field_length`
  code: `title` 200 bytes, `notes` 4096 bytes, `description` 1024 bytes, and
  an artefact `role` 64 bytes. `capabilities` now reports `archive.init`,
  `import`, `case.create`, `case.list`, `case.show`, and `submission.add`.
- New error code `record.not_found`, for a case identifier or artefact digest
  that names nothing in the archive. It is a `record` refusal that exits `4`
  and never echoes the reference the user supplied.
- `record.malformed`, published until now as reserved, is implemented and its
  condition decided: a record document a command must read that is not valid
  JSON, is missing a required field, claims an unknown record kind, does not
  name the file it lives in, is not a regular file, or is larger than the
  record cap. Its details carry `record_kind` and `path_count`; the
  `archive_path` key the reserved entry listed is deliberately not emitted,
  because it would name an identifier the caller never supplied and the count
  answers the only useful question.

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

- `docs/receipt-discovery.md` presents its evidence matrix as one subsection
  per source (E1 to E6) with a definition-style list of the same fields,
  instead of a nine-column table whose rows were unreadable in the raw
  Markdown. Every source keeps its facts, links, retrieval dates, and
  normative, descriptive, or unknown label. The F4 finding drops one of its
  two adjacent no-design-decision disclaimers.
- The Association records section of `docs/archive-layout.md` states the same
  field list and nesting as `docs/error-contract.md`: `evidence` and
  `confidence` sit inside each candidate, not at record level, and the
  document now says the error contract is authoritative for wire shapes. No
  rule changed.
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
