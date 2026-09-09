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

Automatic matching, derived metadata, extractors, receipt parsing, KRX and
`.es3` handling, verification results, and government delivery stay
unimplemented, and no output or field claims delivery, receipt by an
authority, authenticity, or legal effect. `verified` is `false` in every
envelope.

### Added

- `openpapir skill` writes the agent skill document the binary carries to
  stdout, byte for byte and with nothing added. It takes no file and no
  `--json`, touches no archive, and exits `0`, so an agent can install the
  document with `openpapir skill > .claude/skills/openpapir/SKILL.md` and no
  checkout. The same bytes are committed as
  `crates/openpapir-cli/skills/openpapir/SKILL.md` and are embedded with
  `include_str!`. The document describes when to reach for openPapir, every
  implemented command with its exact invocation and the `data` fields to read,
  the envelope, the exit codes by bucket, the privacy rule, the input caps,
  and the separation of imported, matched, and authenticity-verified (#22).
- `capabilities` now lists `skill` as the fifteenth operation. It is the one
  operation that touches no archive, and it is reported there so that a
  machine caller learns of it from the same list as every other operation.
- `tests/golden/` pins the output of twenty-three invocations, in both the JSON
  and the human form, with both streams and the exit code of each. The harness
  is `crates/openpapir-cli/tests/golden.rs`; it builds every archive from
  constants, normalises the four values that legitimately move between runs
  through the documented placeholders `<id>`, `<digest>`, `<time>`, and
  `<root>`, and compares byte for byte. Regeneration is deliberate and never
  automatic: `OPENPAPIR_UPDATE_GOLDEN=1` rewrites the files, and CI never sets
  it. `tests/golden/README.md` states the contract, the placeholders, and which
  kind of change each difference is.
- README.md gains "Agents and automation" and "People", which show the same
  command in its two modes with captured output and link the agent skill and
  the golden output contract.
- `openpapir case export --archive <root> --case <case-id> --to <dir>` copies
  one case out of the archive as plain files. Every object the case's
  submissions and receipts reference is copied byte for byte to
  `objects/<digest>`, every case, submission, receipt, association, and
  import-event record that belongs to the case is written to
  `records/<kind>/<id>.json` exactly as the archive stores it, and
  `manifest.json`, with sorted keys, lists every copied object with its digest
  and byte length and every record with its kind and identifier. The archive
  is opened read-only and is not modified: no lock is taken and nothing inside
  the root is written. The destination is an existing empty directory or one
  the export creates, is never inside the archive root, and may be on any
  filesystem, because a copy is a plain copy and never a hard link. A
  symbolic link in the destination is refused rather than followed, and a file
  already at a target path is refused rather than replaced. Every copy is
  re-digested as it is written and its partial file removed when it differs.
  An export that fails removes exactly what it created, so a destination the
  export made is gone again and one the user made is left empty, and a retry
  is not refused for a leftover from the attempt before it.
  An exported object is named by its digest alone: no original filename is a
  file name, a directory name, or a manifest field. Human output repeats the
  destination the user supplied; no JSON field carries it (#20).
- `openpapir archive repair-permissions --archive <root>` narrows the root,
  the marker, every layout directory, every record, and every object back to
  the owner-only modes of the design, and reports the count of paths it
  changed per kind. The lock file is not inspected and not reported: the only
  lock file that can exist while the repair runs is the one it took itself,
  and a lock another writer holds refuses the repair with `lock.held`. It
  only ever narrows: a path already narrower than the design's mode is left
  as it is, and nothing is ever widened. It
  refuses a root without a marker, takes the writer lock, reads no file
  content, and refuses a symbolic link inside the archive rather than
  narrowing it. It is the documented remedy for copy tooling that widens
  permissions when a backup or an export is restored.
- `export.destination_conflict` and `export.copy_mismatch` are now emitted
  rather than reserved. Both are `export` refusals that exit `4`, and both
  report counts, a digest, or a destination-relative path, never the
  destination the user supplied. Every refusal raised inside a destination
  carries `scope` `export_destination` and no `archive_path`.
- `capabilities` now lists `case.export` and `archive.repair_permissions` as
  the twelfth and thirteenth operations.
- `openpapir case delete --archive <root> --case <id> [--purge]` deletes one
  case, every submission recorded against it, and the receipts and
  associations tied only to those submissions. A receipt or an association
  that still references a submission or case the deletion leaves behind is
  kept, and so is an association a surviving record supersedes. Without
  `--purge` no object is removed and the objects that would become
  unreferenced are counted as retained; with `--purge` every object no
  remaining import event, receipt, or submission references is unlinked, and
  the import events naming a purged object go with it once that object has
  actually gone. The whole archive is scanned under the writer lock before
  anything is removed, so a malformed record document aborts the deletion with
  `record.malformed` while the archive is still untouched, and an unknown case
  is `record.not_found`. Every removal is the unlink of one file openPapir
  created, never a recursive directory removal, and nothing outside `records/`
  and `objects/sha256/` is touched. The report is counts, record kinds, and
  the reason an object stayed, and never a digest, a path, or a filename;
  nothing about the deletion is persisted. `capabilities` now lists
  `case.delete` as the fourteenth operation (#19).
- `delete.objects_retained` is now emitted, with the additive `reason` detail
  key, when a purge could not unlink an object. The deletion completes what it
  can, its counts stay in `data`, and the error carries the count alone.
  `platform.replace_while_open` is emitted by `case delete` as a warning where
  the platform defers an unlink, carrying the additive `read_only_restored`
  flag: where the platform needs a read-only attribute cleared before an
  unlink, it is put back when the unlink still fails, so a surviving object
  keeps the access it had. The warning is reported once however many objects
  deferred, and carries the worst outcome any of them saw.
- `delete.records_retained` and `delete.record_entangled` are new error codes
  in the `delete` bucket. A record document the filesystem refuses to unlink
  stops the object pass entirely rather than purging around the record that
  stayed, and reports how many documents the deletion planned to remove and
  did not. A record that has to survive the deletion and names a record the
  deletion would remove, which today means an association naming submissions
  in two cases, refuses the deletion in the scan before anything is unlinked,
  because openPapir edits no stored record and will neither corrupt the
  association nor delete an assertion about a case the user did not name. A
  deletion therefore never leaves a record or an object naming something the
  archive no longer holds: an archive `archive check` found clean stays clean
  after a deletion and after every refusal.
- `openpapir archive check --archive <root>` re-digests every stored object
  and compares the artefact store with what the records claim. It is
  read-only: it takes no writer lock, so a held lock never stops it, opens
  every file read-only and without following a link, and creates, renames,
  removes, and repairs nothing, including a leftover staging file and a
  missing layout directory, which it reads as empty. It therefore completes
  on an archive whose root the user cannot write to. An entry the check could
  not read, including any directory under `objects/` it could not list, is
  counted as unchecked rather than reported as damage or as a missing
  object. The report
  is counts and stable codes only, and never the path, name, or digest of a
  damaged object. A clean archive exits
  `0`; otherwise the report stays in `data`, `ok` is `false`, and `error`
  names the first problem in a fixed precedence, exiting `4` for a `record`
  or `integrity` condition and `3` where the only complaint is a link inside
  the store. `capabilities` now lists `archive.check` as the eleventh
  operation. A passing check is storage integrity only: it asserts nothing
  about authenticity, origin, delivery, or legal effect, and `verified` stays
  `false` (#15).
- `integrity.dangling_reference` is a new error code for a record that names
  a digest, case, submission, receipt, import event, or association the
  archive does not hold. `integrity.digest_mismatch` and
  `integrity.orphan_object`, both previously reserved, are now emitted, and
  an orphan is a counted report entry rather than a refusal of anything.
- `openpapir receipt add --archive <root> --artefact <digest> [--import-event
  <id>] [--label <l>]` and `openpapir receipt list --archive <root>` record
  and list receipts under `records/receipts/`. A receipt is an artefact the
  user believes to be a receipt: it names a stored object and the import event
  that introduced it, never rewrites the artefact, and asserts nothing about
  the file's type or authenticity. With no `--import-event` the earliest event
  for the digest is recorded; a named event must record that same artefact
  (#12).
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
- `openpapir case create --archive <root> --title <t> [--notes <n>]`,
  `openpapir case list --archive <root>`, and
  `openpapir case show --archive <root> <case-id>` record, list, and show
  cases under `records/cases/`. A case is the user's own folder of related
  correspondence and corresponds to nothing any government service issues
  (#8).
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
  searches for an archive and never adopts a directory that has no marker
  (#7).
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
  separated receipt states (#6).
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

- `AGENTS.md` and `SECURITY.md` describe the scope the binary actually has.
  The agent boundaries name the fifteen implemented operations, point at
  architecture as the implemented contract and the specification index as the
  index of everything else, and list what remains unimplemented, including
  receipt parsing, automatic matching, derived metadata, delegated
  verification, migration, KRX and `.es3` handling, and any government
  integration or delivery. The repository layout table gains the embedded
  agent skill and the golden outputs, and names the core modules. The security
  policy's current scope states that the tool ingests files, persists records,
  checks integrity, exports, and deletes with an explicit purge, and still
  contacts no service, verifies no signature, and delivers nothing. Its
  requirements are split into those already enforced, with pointers to the
  archive layout and the error contract, and those still ahead. No policy,
  reporting channel, or behaviour changes.
- The `archive check` report counts the leftover staging files the record
  directories hold, in a new additive `records_staging_files` field beside the
  existing `staging_files`, which keeps its meaning and still counts
  `objects/incoming/` alone. The record reader already counted such a file
  rather than passing over it, and the figure now reaches the report, so an
  interrupted record write is visible to a caller. It is not a problem: no
  code is reported for it, the exit code is unaffected, and the check still
  deletes nothing. The human form names both figures on its staging line.
- The archive's no-follow open has one implementation again. `open_no_follow`
  and the new `open_no_follow_nonblocking` in the archive's path module share
  it, and the record reader uses the second instead of repeating the Unix arm
  with the non-blocking flag added. No behaviour changed on any platform: the
  rule that a link fails at the open rather than after a check on the path is
  the same one, expressed once.
- [architecture](docs/architecture.md) now states the outward no-follow rule
  the export applies to a destination directory: each directory component the
  export would create is tested with a no-follow stat and refused when it is a
  symbolic link, and each leaf file is created with create-new semantics,
  which the system call itself refuses on an existing path. It also states why
  that is weaker than the archive-side rule, which reaches every path through
  a no-follow open, and what the residual check-then-use gap can and cannot
  reach. No behaviour changed.
- The `record.inconsistent` rule tables in `docs/error-contract.md` and
  `docs/architecture.md` now use one polarity. Both state the violation the
  rule reports, matching the rule names, under the column heading `Violation
  reported`, instead of the contract stating the satisfied invariant and the
  architecture the violation. No rule name, code, or behaviour changed
  (#16).
- `path.symlink` gained the `scope` value `input`, and an input that is a
  symbolic link now carries it. The refusal previously carried its bucket
  alone, because the value set named nothing for a path outside the archive.
  The path itself is still never echoed and no `archive_path` is reported
  (#18).
- `platform.filesystem_unsupported` is reachable. A filesystem that cannot
  create the hard link the archive's write procedure publishes with, FAT32 and
  exFAT among them, is now refused with that code and exit `5`, naming
  `capability` `hard_link` and the `stage`. It was previously reported as the
  retryable `write.interrupted` with exit `4`, which invited a retry that could
  never succeed. Every other failure of the same call keeps `write.interrupted`.
- `docs/receipt-discovery.md` presents its evidence matrix as one subsection
  per source (E1 to E6) with a definition-style list of the same fields,
  instead of a nine-column table whose rows were unreadable in the raw
  Markdown. Every source keeps its facts, links, retrieval dates, and
  normative, descriptive, or unknown label. The F4 finding drops one of its
  two adjacent no-design-decision disclaimers (#11).
- The Association records section of `docs/archive-layout.md` states the same
  field list and nesting as `docs/error-contract.md`: `evidence` and
  `confidence` sit inside each candidate, not at record level, and the
  document now says the error contract is authoritative for wire shapes. No
  rule changed (#9).
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
- The CI Documentation job now runs `python3 scripts/check-prose.py` and
  passes the same markdownlint exclusions as `scripts/check.sh`, so a local
  run and CI accept exactly the same tree. The `CONTRIBUTING.md` checks table
  records the prose check as running locally and in CI (#13).
- `scripts/check-prose.py` and `scripts/check-doc-links.py` share one
  skip-prefix list, spelled identically in both files with a comment pointing
  at the other. The list is the union of the two previous lists, which selects
  the same tracked files as before.
- The MSRV CI job states the version once, in a job-level `MSRV` environment
  variable used by the toolchain selection, the banner assertion, and the
  `Cargo.toml` guard; only the job name and the action pin comment still
  spell it out.
- The Association records section of `docs/archive-layout.md` states the
  candidate count per outcome exactly as `docs/error-contract.md` does: one or
  more candidates for `candidate`, two or more for `contradictory`, exactly
  one for `associated`, and none for `unassociated`. The design previously
  said "several" for both `candidate` and `contradictory`. No rule changed
  (#14).

### Fixed

- `case delete` no longer unlinks part of its records before refusing. The
  record pass is now all or nothing per case: before the first unlink, every
  record directory the deletion would remove an entry from is opened without
  following a link and its mode checked for owner write and search, and every
  planned document is checked to be either already absent or a regular file.
  Nothing destructive is attempted by the probe. A deletion that cannot run
  in full is refused with `delete.records_retained` while the archive is
  exactly as it was, so `records_removed_total` is `0` and re-running the
  command after clearing the cause completes the whole deletion. Previously a
  record directory made unwritable part way through the order, such as
  `records/submissions`, let the association and receipt records go before
  the refusal; a re-run then reported success while an object whose last
  referencing record had already been removed stayed in the store with its
  import event, which `archive check` reads as clean, and nothing told the
  user. The probe reads permission bits before the unlink rather than
  performing it, so a filesystem that changes in between, or that refuses
  through an access-control list or an immutable flag, still stops the pass
  part way; that limit is documented in `docs/architecture.md` and the
  stop-at-first-refusal rule still holds behind it.
- `platform.replace_while_open` from `case delete` now carries
  `read_only_restored` only where the read-only attribute was actually
  cleared. Where reading the permissions or clearing the attribute failed,
  nothing was cleared and there was nothing to put back, and the flag is now
  absent rather than reported as `true`, which claimed a restore that never
  happened. Absence says the attribute was never cleared and nothing was
  widened; `false` still says an object was left writable, and a warning
  carrying no flag never displaces one that reports `false`.
- An interrupted write during a `case export` or an
  `archive repair-permissions` now names the stage it was actually in.
  `write.interrupted` from an export carries `object_write` while an object is
  being copied, `record_write` while a record document is being written, and
  `marker_write` for the destination directory itself and for
  `manifest.json`; the repair carries the stage of the kind of path it was
  inspecting, `object_write` for a stored object or a fan-out directory,
  `marker_write` for the marker, and `record_write` for a record document, a
  cached file, a layout directory, or the root. Both previously reported
  `record_write` for every failure, so an interrupted object copy was
  described as a record write. The write bucket's three stage names are now
  listed with what each one covers in
  [the error contract](docs/error-contract.md).
- `openpapir skill` no longer reports success when the document could not be
  written. Only a closed reader is swallowed: `openpapir skill | head -3`
  still exits `0`, because a reader that stopped reading asked for exactly
  that. Every other write or flush failure, a full disk during the documented
  `openpapir skill > SKILL.md` install path above all, exits `4`, the `write`
  bucket's code, with one line on stderr that names no path and repeats no
  argument. Previously the result of both the write and the flush was
  discarded, so a truncated document was installed and reported as success.
- The golden harness reports a golden file that is not on disk as a difference
  named `is missing`, instead of reading it as an empty expectation. Most
  cases pin an empty `human.stderr.txt`, so a deleted golden compared equal to
  the capture and the case passed in silence. The file-by-file comparison now
  lives in `crates/openpapir-cli/tests/golden_support/compare.rs`, with its own
  tests over a temporary copy of one case that never touch `tests/golden/`.
- A hard link the publish step could not create no longer claims the
  filesystem is unsupported when the system did not say so. Unix `EPERM`,
  which `link(2)` returns both for a filesystem without hard links and for an
  immutable or append-only file, is now `write.interrupted` (exit `4`)
  carrying the additive details `capability` `hard_link` and `condition`
  `link_refused`, and a message that states the observed condition.
  `platform.filesystem_unsupported` (exit `5`) is kept for the codes that
  state the operation is unsupported: `Unsupported`, Unix `EOPNOTSUPP`, and
  Windows `ERROR_INVALID_FUNCTION` and `ERROR_NOT_SUPPORTED`. Distinguishing
  the two `EPERM` causes needs a `statfs` or an attribute `ioctl`, and the
  workspace forbids `unsafe`, so openPapir reports what it observed rather
  than a cause it cannot prove.
- A no-follow open on Windows now refuses the reparse point it opened through
  a sentinel error type of its own, so only that refusal is reported as
  `path.symlink`. Any other `InvalidInput` from the same open keeps its own
  meaning, where previously every one of them read as a link refusal.
- A target that is neither Unix nor Windows now fails to compile with a stated
  reason instead of falling back to checking the path before opening it. The
  fallback was the time-of-check-to-time-of-use gap the no-follow rule exists
  to remove, and the platform warning did not describe it.
- An argument-parser refusal now names the command the user actually invoked
  and echoes only names that command defines. The recognition walks the raw
  arguments as the parser would and consumes each flag's value with the flag,
  so `case create --title list` reports `case.create` rather than `case.list`,
  and the `details.argument` allowlist is the recognised command and its
  parents rather than every name in the build, so a flag only another
  subcommand defines is no longer echoed back.
- A record document is now opened once and judged on that opened handle. The
  reader opens it with the platform's no-follow flag and takes both the file
  kind and the length from the handle it will read from, instead of checking
  the path and opening it afterwards, so a local writer can no longer swap a
  regular file for a symbolic link between the check and the read. The read is
  capped as well as checked. Refusals are `record.not_found` for a document
  that is not there, `record.malformed` for one that is not a regular file, is
  not a valid record, or could not be opened for a reason other than absence
  (previously such a document read as `record.not_found`), and
  `input.cap.record_size` for one over the record cap (#21).
- A leftover staging file in a record directory is counted rather than
  silently passed over. `Visited`, the result of reading one record
  directory, gained a `staging` count beside its `unreadable` count, so an
  interrupted write is visible to a caller. The file is still never adopted as
  a record and never reported as a malformed one, and no reader removes it:
  removing it is a write, and only the holder of the writer lock may write.
  The count now reaches the `archive check` report as `records_staging_files`;
  the report's `staging_files` still counts `objects/incoming/` alone.
- `association create --supersedes ""` is refused with `record.not_found`
  instead of being read as no supersession. An empty value is not an
  identifier, so it names no association, exactly like any other value that
  does. Only omitting the flag records no supersession.
- `archive check` no longer reads a record directory it cannot list as an
  empty one. A `records/<kind>` directory whose listing fails for any reason
  other than being absent now increments the report's new `records_unchecked`
  count, and what the unread records could have named is left unjudged: no
  stored object is reported as `integrity.orphan_object` while
  `records/imports`, `records/receipts`, or `records/submissions` is unread,
  and no reference into an unread kind is reported as
  `integrity.dangling_reference`. A directory that is simply absent still
  reads as empty. Previously `chmod 000 records/imports` reported every
  stored object as an orphan and exited `4` over a permission error. The
  record listings the record commands use are unchanged: they still report
  the records that are there (#17).
- An argument-parser failure under `--json` is now the response envelope, with
  `usage.arguments` and exit `2`, instead of usage text on stderr and no
  envelope at all. A machine caller therefore reads one shape for every
  refusal. Without `--json` the parser's own usage text is unchanged, and
  `--help` and `--version` still print to stdout and exit `0`. `details`
  names the flag or value name the parser complained about only when this
  build defines it, so a token the user invented is never echoed back.
- A no-follow open on Windows carries `FILE_FLAG_OPEN_REPARSE_POINT` and
  refuses the opened reparse point, instead of stat-ing the path first and
  then opening it. The pre-check was a time-of-check-to-time-of-use gap rather
  than a defence, and it also missed an NTFS junction, which the standard
  library's symbolic-link test does not report. A junction, and every other
  reparse point, inside the archive is now refused as `path.symlink`. The
  degradation that remains is reported as the new
  `platform.no_follow_after_open` warning. Unix behaviour is unchanged.
- `archive.permissions_wide` can no longer name the archive path `.` with a
  `path_count` of `0`. The refusal takes the path it names plus the further
  paths it counts, so the refused set is non-empty by the signature.
- A directory at a stored object's path is reported as `path.overwrite` rather
  than `archive.permissions_wide` when it is also wider than owner-only. The
  shape of the path is now checked before its permissions, because a directory
  openPapir did not create is the problem, not the permissions it never set.
- The MSRV CI job passes a `prefix-key` naming the MSRV to
  `Swatinem/rust-cache`, whose key otherwise derived from the runner's stable
  `rustc` while the job compiles under 1.88.0, so the cache never hit.
- The Security workflow retries the pinned `actionlint` download up to five
  times with backoff. A single HTTP 500 from GitHub Releases had turned
  `develop` red for an unrelated merge. The version pin and the checksum
  verification against the release's published checksums file are unchanged.
- The pinned minimum-supported-Rust action no longer receives an unsupported
  toolchain input, which had failed the MSRV job (#3).
- The minimum-supported-Rust CI job now compiles on Rust 1.88 rather than on
  the runner's stable toolchain. `rust-toolchain.toml` pins the channel to
  stable and overrides `rustup default`, so the job sets `RUSTUP_TOOLCHAIN`
  on its steps, prints the resolved `cargo` and `rustc` versions, and fails
  if the banner is not 1.88 or if `Cargo.toml` stops declaring
  `rust-version = "1.88"` (#10).
