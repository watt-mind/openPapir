# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

There are no published releases. Every entry below is unreleased work on
`develop`. The `capabilities` JSON envelope is versioned separately by its
`schema_version` field, which is `1`.

Every pull request adds an entry under Unreleased, in the category that
matches the observable difference, unless the changelog rules waive it. See
the Documentation section of [CONTRIBUTING.md](CONTRIBUTING.md).

## Unreleased

Automatic matching, derived metadata, extractors, receipt parsing, KRX and
`.es3` handling, verification results, and government delivery stay
unimplemented, and no output or field claims delivery, receipt by an
authority, authenticity, or legal effect. `verified` is `false` in every
envelope.

### Added

- `openpapir submission show --archive <root> <submission-id> [--json]`,
  `openpapir receipt show --archive <root> <receipt-id> [--json]`, and
  `openpapir association show --archive <root> <association-id> [--json]`,
  operations `submission.show`, `receipt.show`, and `association.show`, read
  one stored record back with what relates to it. Each takes no writer lock
  and changes nothing. `submission.show` reports `submission`,
  `associations`, and `association_count`, the associations being the ones
  naming the submission as a candidate or as the `submission_id` an
  `associated` outcome confirms, live heads first and superseded records after
  them. `receipt.show` reports `receipt` with the same history
  `association.list` reports for that receipt, newest first and superseded
  records included. `association.show` reports `association`, `live`, `chain`,
  and `chain_length`, the chain being what the record supersedes and what
  supersedes it, and `live` being true only when no stored record supersedes
  it. An identifier that names no record is `record.not_found`, naming the
  kind and how it was referenced and never the value the user supplied. The
  three join the operations `capabilities` reports. `case.show` gains an
  additive `receipts` array: every receipt whose live association names a
  submission of the case, each entry holding `receipt`, `association_id`,
  `outcome`, and `submission_ids`. Only the live head of a supersession chain
  is read, so withdrawing an assertion takes the receipt out of the section
  while both records stay stored, and every entry is the user's own assertion,
  claiming no delivery, receipt by an authority, authenticity, or legal
  effect. Goldens added for `submission.show`, `receipt.show`, and
  `association.show`, and `capabilities` and `case.show` regenerated.

- An ignored benchmark, `crates/openpapir-cli/tests/bench.rs`, times the
  linear scans on a synthetic archive of 10000 cases, 10000 submissions,
  10000 receipts, 10000 associations, and 20000 imported objects, and asserts
  that `case list`, `case list --query`, `archive check`, `archive status`,
  `case export`, and `case delete --purge` each stay under a documented
  ceiling. An operation the build does not implement yet is reported as absent
  rather than measured, so the benchmark covers one from the commit that adds
  it. No CI job runs it. The new Performance section of `docs/architecture.md`
  records the measured numbers, the machine class, and the date, and states
  that they are indicative; `docs/testing.md` says how to run it and how to
  change its size.
- Property and concurrency coverage for three boundaries the earlier suites
  left open. An import event is round-tripped with its `source` field both
  absent and present, so the field a build writes only when it is there
  survives as an absence rather than as a default. The export manifest gains a
  reader property beside its writer one: any bytes at `manifest.json` read
  back or are refused with a documented code and never panic, a manifest this
  build wrote reads back with exactly the rows it lists, and one carrying a
  dropped key, a value of the wrong type, an unknown record kind, a malformed
  digest, or a path-shaped identifier is refused with
  `export.manifest_malformed`. A new concurrency test runs several
  `submission add` processes against one archive at once and asserts each
  either succeeds or refuses with `lock.held`, that the archive then holds
  exactly what the successful ones reported, and that `archive check` is clean
  with no leftover staging file, and it takes `case list` listings while
  another process publishes and asserts every one of them is a whole set of
  whole records. No behaviour changes.
- `openpapir case import --archive <root> --from <dir> [--json]`, operation
  `case.import`, reads a directory `case export` wrote back into an archive.
  The manifest is authoritative: every object it lists is re-digested from the
  export's own bytes and every record it lists is read and parsed before the
  archive is written to at all, and the record set is then staged and
  published under the writer lock all at once or not at all, so an interrupted
  import leaves the whole case or nothing of it and `archive check` stays
  clean. Every record keeps its original identifier; a record or an object the
  archive already holds is not an error and is not written again, so importing
  one export twice leaves the same archive. Each object the import stores gets
  one new import event carrying the additive `source` `export`, and the
  exported import event is restored beside it, so the provenance of the user's
  own import survives the round trip. Five codes are added in the `export`
  bucket, all exiting `4`: `export.manifest_missing`,
  `export.manifest_malformed`, `export.object_mismatch`,
  `export.record_missing`, and `export.record_conflict`. `case.import` joins
  the operations `capabilities` reports, and human output echoes the `--from`
  argument
  the user typed exactly as `case export` echoes `--to`; no JSON field carries
  either. A symbolic link anywhere in the source is `path.symlink` with
  `scope` `export_source`, every path there is opened without following a link
  and without waiting, and the per-operation byte cap binds a restore as it
  binds an import, so a case over 512 MiB in total is refused with
  `input.cap.import_bytes`. Goldens added for `case.import` and
  `case.import.manifest-missing`, and `capabilities` regenerated.

- `openpapir archive status --archive <root> [--as-of <yyyy-mm-dd>] [--json]`
  (operation `archive.status`) summarises one archive without changing it and
  reminds the user where to look in their delivery storage for a submission
  receipt while the operator says one would still be there. `data` carries
  `as_of`, the counts `cases`, `cases_by_status[]` (one `{count, status}` per
  status in the closed set, including a status no case holds, summing to
  `cases`),
  `submissions`, `receipts`, `associations`, and `stored_objects`,
  `undated_submissions`, `retention_window_days`, and
  `receipts_to_retrieve[]`, each entry with `case_id`, `submission_id`,
  `submission_date`, `retrieve_by`, and `days_left`, ordered by `retrieve_by`
  and then by identifier. A submission is listed when it carries a
  user-supplied date, no live association with outcome `associated` or
  `candidate` names it, and its date plus the window is on or after the as-of
  date; the last day of the window still counts as open. Live means the head
  of the supersession chain: a superseded record stays in the history and is
  still returned by `association list`, but it no longer says what the user
  asserts today, so `association retire` on an `associated` record withdraws
  the assertion and the reminder comes back. A submission with no usable
  date is counted and never listed. `--as-of` is validated exactly as
  `submission add --date` is, defaults to the clock's own date, and accepts a
  past or future date as a what-if; an unusable one is `usage.arguments` with
  `argument` `as_of`. The command takes no writer lock, opens the archive
  read-only exactly as `archive check` does, reads no artefact bytes, and
  writes nothing. The retention window is one constant of 30 days: the
  operator's help page states that the personal delivery storage retains
  incoming documents for 30 days unless they are moved to permanent storage,
  retrieved 2026-09-09, descriptive rather than normative. openPapir enforces
  nothing, reads no mailbox, and checks no service, and no output states
  delivery, receipt by an authority, authenticity, or legal effect. A case's
  status changes no reminder: closing a case is the user's own filing, so a
  submission in a closed case is listed exactly as one in an open case.
- `capabilities` now lists `archive.status`, and every document that states
  the number or the list of operations states the number reported here.
- `tests/golden/archive.status/` pins the JSON and human output of the summary
  against a fixture holding one submission whose window has elapsed, one whose
  window is open, one a live candidate association already names, and one with
  no date. `tests/golden/archive.status.retired/` pins the same fixture with
  that candidate record retired, where the submission is reminded of again.
- A property suite over the boundaries that accept input openPapir did not
  mint: `crates/openpapir-core/tests/property/` and
  `crates/openpapir-cli/tests/property.rs`. Each property states one
  invariant. Any byte sequence at a record path reads back as that record or
  is refused with a documented `record.*` code, and a document over the record
  cap is refused on the opened handle rather than read. Any byte sequence in
  the archive marker opens the archive or is refused with an `archive.*` code.
  Every generated case, submission, receipt and association record round-trips
  byte for byte through its own document, with sorted keys and one final
  newline. Every name carrying a `..`, a separator, a NUL, or an overlong
  component is refused before it is joined into a path, and the archive is
  left untouched; a record path that is a link or is occupied is refused with
  `path.symlink` or `path.overwrite`. An export writes a well-formed manifest
  or refuses with a documented code and leaves the destination as it found it.
  Any command line asking for the JSON form is answered by one envelope with a
  bucketed exit code, and `details.argument` never carries a value the caller
  typed. `proptest` is a pinned workspace dev-dependency of both crates. The
  suite runs in seconds at its committed case counts; `PROPTEST_CASES` raises
  them for a local soak, as [testing](docs/testing.md) describes.
- `.github/workflows/release.yml` builds the release artefacts described in
  `docs/releasing.md`: prebuilt binaries for `x86_64-unknown-linux-musl`,
  `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  and `x86_64-pc-windows-msvc`, each as an archive with a `.sha256` file that
  `sha256sum --check` reads. Every Linux and Windows artefact is built on a
  runner of its own architecture, so no cross-compilation tooling is involved,
  and every artefact the runner can execute is smoke tested by running
  `capabilities --json` on it. A `v*` tag whose commit is on `master` gives a
  draft GitHub release, attested with `actions/attest-build-provenance`, whose
  notes are the changelog section for that version; a manual dispatch with
  `dry_run` builds the same artefacts, uploads them to the run, and creates
  nothing. A pull request that changes the workflow file itself gets the same
  dry run, because GitHub registers a manual trigger only from the default
  branch and the pipeline would otherwise be unexercised until it is merged.
  The workflow never tags, never pushes, and never publishes a draft.
  Nothing is released, and no crate is published: `publish = false` stays.
- The Format and lint job additionally runs clippy for
  `x86_64-pc-windows-msvc` from its Linux runner, so the platform-specific
  code paths are linted on every pull request instead of only in the Windows
  test job. `docs/testing.md` records how to run the same lint locally. The
  required-check names are unchanged.
- `openpapir association retire --archive <root> <association-id>
  [--reason <text>] [--json]` withdraws an assertion the user no longer stands
  behind. It writes a new association record for the same receipt, with
  outcome `unassociated`, no candidate, and `supersedes` naming the record the
  user retired, so nothing is edited and nothing is removed and
  `association list` shows both. `--reason` is the user's own single line, at
  most 512 bytes; it is stored on the new record as the optional record-level
  `statement`, which only a retirement carries, and no message, warning, or
  count repeats it. Retiring a record something already supersedes is
  `record.inconsistent` with the new rule `already_superseded`. `capabilities`
  lists `association.retire`.
- `case update` rewrites one case record in place, keeping its `id` and its
  `created_at` and adding an `updated_at`. It is the one operation that
  rewrites a stored record, and it rewrites only the case record: a
  submission, a receipt, and an association are the user's evidence of what
  they recorded at the time, and evidence that can be edited is no longer
  evidence, while a case is the user's own folder label and carries none. The
  decision and its reasoning are recorded in `docs/archive-layout.md` and
  `docs/architecture.md`. It takes `--title`, `--notes` or `--clear-notes`,
  `--status`, and repeatable `--tag` and `--untag`, refuses with
  `usage.arguments` and `argument` `update` when nothing would change, and
  reports the changed fields by name only. A tag both added and removed in one
  invocation stays on the case, and a `--untag` value is checked against the
  record rather than against the tag caps, because a value no case could carry
  is simply not on this one. `capabilities` lists it. The rewrite is
  reachable only for a record kind that implements
  the `Rewritable` marker in `openpapir-core`, which the case record alone
  does, so the append-only rule holds at compile time rather than by
  convention.
- The case record gains `status`, `open` or `closed`, and `tags`, stored
  sorted and deduplicated. Both read through a serde default, so a case
  record an earlier build wrote reads as `open` with no tag and no archive
  needs migrating. `case create` takes `--status` and repeatable `--tag`, and
  `case show` prints the status, the tags, and the update time once there is
  one.
- `case list` takes `--status`, repeatable `--tag` where every tag must match,
  and `--query`, a case-insensitive substring of the title or notes. There is
  no index: the query is applied in one linear scan over the records the
  listing already read, so its cost grows with the number of cases. The query
  is the user's own text and is echoed in no output, in either form, whether
  it matched anything or not. The order of the listing is unchanged.
- `input.cap.tag_length` and `input.cap.tag_count` bound a case tag at 64
  bytes and a case at 32 distinct tags. Both are `input` refusals and exit
  `3`. The length refusal reports the tag's position among the tags supplied
  and never the tag itself, and the count is the one that would be stored, so
  a tag the user repeated never spends part of the cap.

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
- `capabilities` now lists `skill`. It is the one
  operation that touches no archive, and it is reported there so that a
  machine caller learns of it from the same list as every other operation.
- `tests/golden/` pins the output of twenty-eight invocations, in both the JSON
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
- `capabilities` now lists `case.export` and `archive.repair_permissions`
  among the operations it reports.
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
  `case.delete` among the operations it reports (#19).
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
  the store. `capabilities` now lists `archive.check` among the operations it
  reports. A passing check is storage integrity only: it asserts nothing
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
  `input.cap.field_length`. `capabilities` now reports `receipt.add`,
  `receipt.list`, `association.create`, and `association.list` too.
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

- The documentation names no operation count. `docs/architecture.md` gains
  one authoritative table under "Current implementation", listing each
  operation `capabilities` reports beside its invocation, and
  `crates/openpapir-cli/tests/contract.rs` parses that table and fails when it
  and the binary's list disagree, so an operation added later changes one
  table and one test. `README.md`, `AGENTS.md`, `docs/index.md`,
  `docs/receipt-discovery.md`, `docs/specification.md`, and the entries above
  now defer to the list `capabilities` reports instead of naming a number
  that goes stale. `docs/specification.md` no longer says the fifth milestone
  is unimplemented, since each item on the roadmap carries its own implemented
  or planned marker, and its `archive status` row now reads as `README.md`
  does. `README.md` also stops listing the import of an export and the update
  of a case record as unimplemented. No behaviour, contract, or output
  changes.
- `docs/archive-layout.md` decides the encrypted backup at rest, which no code
  implements and which adds no dependency: it covers the backup artefact and
  not the live archive, puts a standard AEAD container over a tarball of the
  export shape, derives the key from a passphrase the user holds with a
  memory-hard KDF and stores no key, says what a container does and does not
  hide, fixes the refusal and truncation behaviour of a restore, and records
  that an encrypted backup asserts the confidentiality of the copy and nothing
  about the authenticity of the originals. The roadmap item's gate,
  `SECURITY.md`, and the decided-designs table in `docs/specification.md`
  follow it.
- `docs/architecture.md` records the human output language decision in a new
  Human output language subsection: human output is English only, and the JSON
  form stays the scripting contract. The subsection sizes the surface, gives
  the five reasons, and names the compiled-in message table, one function per
  message with no format string taken from data and no new dependency, as the
  path a demand for Hungarian output would take, with the trigger that would
  start it. `README.md` states the decision in one sentence. No behaviour,
  contract, or output changes.
- `docs/receipt-discovery.md` records a second retrieval pass, dated
  2026-09-10, over the three sources the first pass left open. Every attempt
  is logged with its route and its result, the two sources that were
  unreachable were retrieved and are now full evidence-matrix entries, and the
  normative source was read in full, which establishes its redistribution
  terms as all rights reserved. The findings and the "What is and is not
  established" section say what this adds: a normative minimum of what a
  confirmation about an e-Papír submission evidences, a named reference number
  linking notifications to a submission in one authority's channel, and
  confirmation that "receipt" is a family of artefacts rather than one type.
  None of it states an encoding, so the receipt-format gap is narrowed and not
  closed, and receipt-shaped fixtures stay deferred. `docs/references.md`
  follows, requiring a retrieval's route to be recorded alongside its outcome.
- `case delete` decides an association's fate by supersession chain rather
  than by single record, so a retired history goes with the case it was about.
  A chain goes when one of its records names a departing submission and its
  live record, the one no other record supersedes, names none that remains;
  every record of a departing chain is counted under `records_removed` as
  `association`. `delete.record_entangled` stays the refusal while a live
  association still names submissions in two cases, its `retained_count` now
  counts the live records in the way rather than the history behind them, and
  its message says to retire the record first, naming no identifier. A case
  whose only obstacle was an association the user has since retired can now be
  deleted, and the archive stays clean afterwards.
- `docs/roadmap.md` gains a fifth milestone, the local organiser, which
  sequences the remaining local work and names the gate and the state of each
  item; none of the eleven items is implemented yet. The
  receipt-retrieval reminder is attributed to the operator's own descriptive
  30-day statement in `docs/receipt-discovery.md` source E1, retrieved
  2026-09-09. "Later, subject to evidence" now names receipt parsing, KRX
  package import, delegated `.es3` verification, and any service integration
  with their blockers. `docs/specification.md` and `README.md` point at the
  new milestone. No behaviour, contract, or output changes.
- The human form of `case list` now reads `N case(s) listed.` rather than
  `N case(s) in this archive.`, because a filtered listing holds what matched
  rather than what the archive holds, and each line carries the case's status
  between its creation time and its title. The JSON is unchanged but for the
  added fields.
- The case handlers moved out of `crates/openpapir-cli/src/main.rs` into
  `crates/openpapir-cli/src/cases.rs`, which now also holds the `case`
  subcommand definitions. `main.rs` keeps one hook. No behaviour changed.
- The changelog rules in `CONTRIBUTING.md` say which changes to
  `openpapir-core`'s public Rust API are logged. While the crate has no
  published version they need no entry, the CLI contract in
  [architecture](docs/architecture.md) is the only public contract, and every
  change to it is logged; from the first published version library API changes
  are logged under their own heading. `docs/releasing.md` states the same in
  its changelog cut step. No behaviour, contract, or output changes.
- The documentation states the stage the binary reports. `README.md` says
  `alpha` in its status line and in its sample `capabilities --json` output,
  which is regenerated from the binary and lists the same operations,
  and the remaining prose in `CONTRIBUTING.md`, `docs/roadmap.md`,
  `docs/index.md`, `docs/releasing.md`, `docs/references.md`,
  `docs/receipt-discovery.md`, `docs/testing.md`, and `docs/factory.md` names
  the alpha stage or the repository instead of calling the project a scaffold.
  In those files `scaffold` survives only as the first value of the documented
  `stage` vocabulary in [architecture](docs/architecture.md); `AGENTS.md` and
  `docs/orchestrator.md` are not swept here. The receipt discovery
  note no longer claims the tool exposes help, version, and `capabilities`
  alone. No behaviour, contract, or output changes.
- `AGENTS.md` and `SECURITY.md` describe the scope the binary actually has.
  The agent boundaries name the implemented operations, point at
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
- The archive layout document opens by saying what it is today, the
  implemented layout with the departures that building it produced recorded in
  the sections that decide them, and it points at the architecture document
  for the operations `capabilities` reports rather than describing the project
  as an unimplemented design. The stale pointers to closed issues for the
  stale-lock recovery flow and the degradation wire shapes go with it.
- `capabilities` reports `alpha` where it reported `scaffold`, in both the
  JSON and the human form, because the operations it lists are
  implemented, and the documents that still called the repository a scaffold
  when this landed were corrected afterwards.
  [architecture](docs/architecture.md) now documents `stage` beside the
  capabilities contract: a plain string from the closed set `scaffold`,
  `alpha`, `beta`, `stable`, which is not a version and not a support promise,
  which moves only by a release decision recorded here, and which a caller
  never reads in place of `operations` to learn what the binary can do. The
  field's name, type, and meaning are unchanged, so `schema_version` stays
  `1`, and no other envelope field changed. The `capabilities` golden case is
  regenerated for that one value.
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

- `archive check` reports the `supersedes` cycles the stored association
  records form as `record.inconsistent` with rule `supersedes_cycle`, so a
  cycle is found wherever it sits rather than only where a deletion would have
  touched it. The `problems` array gains the code with its count, ordered by
  code and reported as a count alone; the count is of cycles rather than of
  the records in them, and no identifier is named. The code ranks after
  `record.malformed` and before the object conditions in the check's fixed
  precedence, and exits `4` like the rest of its group.
- `case delete` refuses any supersession chain that holds a cycle anywhere in
  it and that the deletion would otherwise have removed, rather than only a
  chain with no live record. A live record that supersedes a cycle behind it
  used to be read as a head withdrawing its history, so the whole chain, cycle
  included, went silently and took a record about another case with it. The
  cycle is what makes that history unreadable in order, so what the head
  withdraws cannot be told either, and the chain is refused with the same
  `record.inconsistent` and rule `supersedes_cycle` before anything is
  touched. A cycle-free archive is unaffected: a chain with no live record was
  already refused, and every other chain is decided exactly as before.
- `case delete` refuses a `supersedes` cycle among the stored association
  records with `record.inconsistent` and the new rule `supersedes_cycle`
  instead of removing it. A cycle has no live record, so the entanglement rule
  that reads one can never fire for it, and a cycle naming both a submission
  of the departing case and one of a remaining case was taken as history
  nobody asserts and removed whole. No openPapir command writes a cycle, so
  one reaches an archive only by hand, and a deletion that cannot tell which
  record is the live one must not guess. Nothing is touched, the refusal
  carries the kind and the rule and never an identifier, and a cycle the
  deletion would not have removed is left to `archive check`. Live-record
  behaviour is unchanged.
- The release workflow reads a changelog section with
  `scripts/release-notes.py`, which ends a section at the next `##` heading
  outside a code fence, so a fenced line that looks like a heading no longer
  truncates the notes a draft release would carry. The dry run runs the same
  extraction against the `Unreleased` section under its read-only scope and
  shows the result in the run summary, so a changelog the extraction cannot
  read fails a dry run instead of surfacing on a tag; a tagged run still
  refuses a version with no section or an empty one. The `Unreleased` read is
  made with `--allow-empty`, because a changelog cut legitimately leaves that
  section empty until the next entry. `./scripts/check.sh` runs the
  extraction's own cases and the same `Unreleased` read locally.
- `case delete`'s `record.malformed` warning now reports both counts it is
  built from. The warning is raised only where an unresolvable reference held
  a candidate object of this deletion back, and its message says "for this
  case", but `malformed_count` is the archive-wide number of such references
  the scan read, so the message could carry a number larger than what this
  deletion lost. `malformed_count` keeps that archive-wide meaning and a new
  `withheld_count` carries the per-case number the message describes. No
  existing key changed meaning, and nothing that was retained becomes
  removable.
- `case delete` scopes the `record.malformed` warning to the deletion it
  actually changed. The scan reads the whole archive, so one hand-edited
  document anywhere made every later deletion carry the warning, including
  deletions with no candidate object and deletions that asked for no purge.
  It is now emitted only where the unresolvable reference held an object of
  this deletion back, which needs `--purge` and a candidate the purge would
  otherwise have removed, and its message says "for this case". Finding such
  a document wherever it sits stays `archive check`'s work.
- `case delete` without `--purge` again reports a candidate object as
  `purge_not_requested` when a record it keeps holds a reference that is not
  a digest. No purge was going to unlink anything, so the unresolvable
  reference decided nothing, and the reason now names what actually held the
  object. `referenced_elsewhere` is reserved for a candidate a remaining
  record names outright, and, for an unresolvable reference, for one a purge
  would otherwise have removed. Totals are unchanged and nothing that was
  retained becomes removable.
- `case delete` no longer purges past a record it keeps whose artefact
  reference is not a digest. Such a reference names an object the archive
  cannot identify, and it was previously read as a reference to nothing, so
  the record protected no object from a purge. Every object the purge had
  considered is now retained with the reason `referenced_elsewhere`, and the
  number of such references is reported as a `record.malformed` warning
  carrying `stage` and `malformed_count`, as scoped above. The
  records the deletion planned to remove still go and `ok` stays `true`. No
  openPapir command writes such a record, because a digest is validated on
  every write; a document edited outside openPapir can hold one.
- `case delete` now counts the import events a refused deletion never reached
  in `retained_count` and in `data.records_retained`. An event that would have
  gone with a purged object is a document the deletion planned to remove, so a
  record pass that stopped before the object stage under-reported by exactly
  those events, most visibly when `records/imports` was the directory the
  probe refused.
- `case delete` builds the path of a record document in one place, used by
  both the all-or-nothing probe and the unlink pass, so the file the probe
  approves and the file the pass removes cannot drift apart.
- A repeated `platform.replace_while_open` warning from one purge now keeps
  the copy that answers the most. A first deferral that never cleared the
  read-only attribute, and so carries no `read_only_restored` flag, is
  replaced by a later one that cleared it and put it back; an object left
  writable, which reports the flag as `false`, still outranks both.
- `archive repair-permissions` no longer decides a refusal's `stage` from a
  catch-all. The kinds of path the repair walks are a closed enum, and each
  one names its stage in a match the compiler checks for exhaustiveness, so a
  kind added or renamed without a decided stage does not build instead of
  silently reporting `record_write`. The reported kind names, their order, the
  counts, and every stage a refusal can carry are unchanged; the human and
  JSON output of the command is byte for byte what it was. The write-stage
  table now appears once in each of `docs/error-contract.md` and
  `docs/architecture.md`, naming the same paths per stage, with the prose that
  used to repeat a slightly different list replaced by a link to it. A test
  parses both tables and fails when they drift apart or when they stop naming
  exactly the stages the implementation can report.
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
