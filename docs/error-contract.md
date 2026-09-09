# Import and association error, JSON, and exit-code contract

## Status and scope

This document is a **specification for review**, and much of it now has code
behind it: every implemented operation that touches an archive, which is
archive creation, artefact import, the case, submission, receipt, and
user-asserted association records, the read-only whole-archive integrity
check, the export of one case, the permission repair, and the deletion of a
case with its explicit purge, emits the envelope below and the codes
[architecture](architecture.md) lists. The one operation that does not is
`skill`, which writes a document rather than an envelope.
Architecture is the canonical description of implemented behaviour, and where
the two disagree it is authoritative and this page is a defect. Every other
command name, flag, field name, error code, and exit code below is
**proposed**, and the section
[What of this is implemented](#what-of-this-is-implemented) says which is
which.

It is follow-up 5 of the
[receipt evidence and local case model note](receipt-discovery.md), and the
[local archive layout and storage design](archive-layout.md) that decides the
archive design deliberately defers every wire name, JSON shape, and exit code
to this document. It was agreed before artefact import printed anything
machine-readable, as that sequencing required.

Every code and shape below traces to a rule already recorded in
[archive-layout](archive-layout.md), [SECURITY.md](../SECURITY.md), or
[AGENTS.md](../AGENTS.md). Where the design leaves a condition undecided, the
code is named and **reserved** and the condition is marked deferred; this
document invents no new refusal.

Out of scope: any command implementation, any parser, any storage or matching
behaviour, any verification, and any government format, identifier, or API.
Container and `.es3` handling belong to openKRX and openSzigno
([AGENTS.md](../AGENTS.md)).

Wording rule, inherited unchanged: importing bytes, associating records, and
verifying authenticity are three separate records. No field, code, message, or
exit status defined here may be read as evidence of delivery, receipt by an
authority, authenticity, or legal effect.

## The response envelope

The envelope is an evolution of the one `capabilities --json` already emits
([architecture](architecture.md),
[`crates/openpapir-cli/src/main.rs`](../crates/openpapir-cli/src/main.rs)). It
keeps the five existing keys and adds two:

- `schema_version`: integer, currently `1`. Unchanged in meaning.
- `ok`: boolean. `true` only when the command completed its stated work.
- `command`: string, the invoked command's stable name.
- `data`: object. Command results, including outcomes that are not errors.
  Present and possibly empty when `ok` is `true`; `{}` when `ok` is `false`,
  unless a command's own contract names a report it carries with its refusal.
  The whole-archive integrity check is that command and, so far, the only
  one: its counts are the result the user asked for, so they stay in `data`
  while `error` names the first problem those counts describe.
- `verified`: boolean.
- `error`: object or absent. Present exactly when `ok` is `false`.
- `warnings`: array, possibly absent. Allowed whether `ok` is `true` or
  `false`; a degradation observed before a failure is still reported.

Exactly one JSON object is written to stdout, on one line, and nothing else.
Human-readable output goes to stdout only in the non-`--json` form; diagnostic
text never shares stdout with the JSON object.

### The `error` object

Three keys, no more:

- `code`: a stable string identifier from the catalogue below.
- `message`: one human sentence, written for a person, never parsed.
- `details`: an object, bounded and privacy-constrained (see
  [Privacy rule](#privacy-rule-for-all-output)). At most 16 keys; values are
  strings, integers, booleans, or arrays of at most 16 such scalars; no
  nesting below that. A missing `details` is equivalent to `{}`.

`code` is the only field a caller may branch on. `message` wording may change
at any time within a `schema_version`.

### The `warnings` array

Each entry has the same three keys as `error` and the same bounds. Warnings
report a condition that did not stop the command at the point it was observed:
the named platform degradations, and nothing else until this document names
more. A warning never changes `ok` and never changes the exit code.

`warnings` may accompany an `error`. A degradation observed before a later
failure is still reported, because
[archive-layout](archive-layout.md) requires that each named weakening be
reported and never silently accepted; a failed command must not swallow the
degradation it already observed. The array is therefore independent of `ok`:
`ok` says whether the command did its work, `warnings` says what was weaker
than the design promises, and neither implies the other. Standing degradations
of an archive are re-reported by archive health output on every run, so a
warning lost with a crashed process is recoverable by asking again.

### Compatibility rule for `schema_version`

Within one `schema_version`, changes are **additive only**: new commands, new
keys in `data`, new error codes, new keys inside `details`. A published error
code is never removed, never renamed, and never has its bucket changed. A key
present in `data` is never removed and never changes type. A caller must
ignore keys it does not recognise and must not fail on an unknown error code;
it may fall back to the bucket prefix and to the exit code.

Removing or renaming anything, or changing a code's bucket or a field's type,
requires incrementing `schema_version`. `schema_version` is the envelope's
version and is independent of `archive_schema_version` in
`papir-archive.json` ([archive-layout](archive-layout.md)).

### Rule for `verified`

`verified` is `false` in every envelope unless a named cryptographic check
actually ran and passed, with its verifier, scope, and supplied trust context
recorded in `data` ([architecture](architecture.md),
[SECURITY.md](../SECURITY.md)). Import, duplicate detection, digest equality,
association, and the integrity check are **not** cryptographic verification:
a digest is a storage-layer identity only
([archive-layout](archive-layout.md)), so all of these leave `verified` at
`false`. No error and no warning ever sets `verified` to `true`.

### Examples (all proposed, none implemented)

Success:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "import",
  "data": {
    "imported": 1,
    "duplicates": 0,
    "artefacts": [
      {
        "digest": "sha256:0f1e...",
        "byte_length": 20481,
        "import_event": "9d0a2c4b6e8f1a3c5d7e9f0b2c4d6e8f",
        "created_object": true
      }
    ]
  },
  "verified": false
}
```

Error:

```json
{
  "schema_version": 1,
  "ok": false,
  "command": "import",
  "data": {},
  "error": {
    "code": "input.cap.file_size",
    "message": "An input file exceeds the single-file size cap.",
    "details": {
      "bucket": "input",
      "cap_bytes": 67108864,
      "observed_bytes": 91234567,
      "input_index": 3
    }
  },
  "verified": false
}
```

Success with warnings:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "import",
  "data": { "imported": 1, "duplicates": 0 },
  "warnings": [
    {
      "code": "platform.no_directory_fsync",
      "message": "Directory durability is weaker on this platform.",
      "details": { "bucket": "platform", "scope": "record_write" }
    }
  ],
  "verified": false
}
```

## Error-code catalogue

### Naming and stability

Codes are lowercase, dot-separated, and prefixed by their bucket, for example
`archive.marker_missing`, `input.cap.file_size`, `path.symlink`. A code is
stable once published: additive-only within a `schema_version`, as above. The
bucket prefix is load-bearing (a caller that does not recognise a code may
branch on the prefix), so a code never moves between buckets.

Buckets are `usage`, `input`, `path`, `archive`, `lock`, `write`, `record`,
`integrity`, `export`, `delete`, `platform`, and `internal`. Every `details`
object carries `bucket` as a string; the entries below list what else it may
carry. Nothing in `details` is required: a field may be omitted when it is
unknown or when including it would breach the privacy rule.

The `details` keys this contract defines are exactly `bucket`,
`archive_schema_version`, `archive_path`, `argument`, `capability`,
`cap_bytes`, `cap_count`, `conflict_count`, `count`, `digest`, `entry_count`,
`evidence`, `expected_bytes`, `export_path`, `field`, `input_index`,
`observed_bytes`, `observed_count`, `orphan_count`, `path_count`, `reason`,
`read_only_restored`, `record_kind`, `reference_kind`, `referencing_record_ids`,
`retained_count`, `rule`, `scope`, `stage`, and `supported_schema_version`. Of
those, `evidence`, `orphan_count`, and `referencing_record_ids` belong to
reserved or deliberately unemitted conditions and no build writes them. A new
key is an additive change like a new code.

`scope` has exactly three values: `archive` for a path inside the archive
root, `export_destination` for one inside an export destination, and `input`
for a path the user named on the command line. A new value is additive.

### `usage`: the invocation itself

- **`usage.arguments`**: usage, not retryable. The command line is
  malformed, or a flag's value is unusable, before any archive is touched.
  Details: `bucket`, `argument` (the flag or positional name, never its
  value). An invocation the argument parser itself rejects is this code too:
  with `--json` it is rendered as the envelope like any other refusal, and
  without `--json` the parser's own usage text is written to stderr instead.
  `argument` is then reported only when the parser named a flag or value name
  the build defines; a token the user invented is text they typed, so it is
  omitted rather than echoed. `--help` and `--version` are not refusals: they
  are written to stdout and exit `0`.
- **`usage.archive_root_missing`**: usage, not retryable. No archive root was
  supplied, or the supplied path does not exist. The root is always supplied
  explicitly; openPapir never searches for an archive
  ([archive-layout](archive-layout.md)). Details: `bucket` only. The supplied
  path is user-supplied and outside the archive, so it is not echoed.

### `archive`: state of the archive itself

- **`archive.marker_missing`**: archive, not retryable. The root exists but
  holds no `papir-archive.json` marker, so there is no archive to open.
  Details: `bucket`.
- **`archive.marker_malformed`**: archive, not retryable. **Decided by
  archive creation.** The marker file exists but cannot be read as a valid
  marker. [archive-layout](archive-layout.md) fixes what the marker records
  but not what happens when it is unreadable; that condition was deferred to
  the artefact-import issue and decided there. It is reported, never repaired,
  and every operation on that archive is refused. Details: `bucket`.
- **`archive.adopt_refused`**: archive, not retryable. Initialisation was
  asked for on a directory that is not empty and has no marker. openPapir
  never adopts such a directory; only an existing empty directory may be
  initialised explicitly ([archive-layout](archive-layout.md)). Details:
  `bucket`, `entry_count`.
- **`archive.schema_newer`**: archive, not retryable. The marker's
  `archive_schema_version` is newer than this build supports. Every
  operation is refused, including read-only ones; there is no best-effort
  read, partial listing, or repair. Details: `bucket`,
  `archive_schema_version`, `supported_schema_version`.
- **`archive.schema_older`**: archive, not retryable by the same invocation.
  The archive predates this build; writes are refused and an explicit
  migration is required. Migration never runs as a side effect. Details:
  `bucket`, `archive_schema_version`, `supported_schema_version`.
- **`archive.permissions_wide`**: archive, not retryable. Permissions on the
  archive root or a path inside it are wider than owner-only. There is no
  override flag; the only remedy is the explicit repair action, which only
  narrows ([archive-layout](archive-layout.md)). Details: `bucket`,
  `archive_path` (archive-relative), `path_count`.
- **`archive.multiple_filesystems`**: archive, not retryable. The archive
  root spans more than one filesystem, which the atomic write procedure
  forbids. Details: `bucket`, `archive_path`.

### `input`: bounds refused before allocation

Every cap is checked before allocation, from the size the filesystem reports,
and enforced again while streaming ([archive-layout](archive-layout.md)); a
mid-stream breach aborts the write and removes the staging file. The record
cap binds a stored document on the way in and on the way out: a record is
checked against it before it is written and again, from the reported size,
before it is read back. All six are refusals of the input, never archive
damage, and all carry `bucket` with a cap and an observed value, `cap_bytes`
or `cap_count` with `observed_bytes` or `observed_count`.

The five import caps also carry `input_index`, the position of the offending
input in the invocation, never its name. `input.cap.field_length` carries
`field` instead, because a record field has no position in an input list.

- **`input.cap.file_size`**: input, not retryable. One file exceeds the
  single-file cap (proposed 64 MiB).
- **`input.cap.import_bytes`**: input, not retryable. The import's total
  bytes exceed the per-operation cap (proposed 512 MiB).
- **`input.cap.import_files`**: input, not retryable. The import names more
  files than the per-operation cap (proposed 1000).
- **`input.cap.record_size`**: input, not retryable. A record document,
  including derived metadata, would exceed the record cap (proposed 1 MiB).
- **`input.cap.filename_length`**: input, not retryable. A supplied original
  filename exceeds the attribute cap (proposed 255 bytes). Only the length is
  reported, never the name.
- **`input.cap.field_length`**: input, not retryable. One user-supplied field
  of a record exceeds its own cap. Added additively by the case and submission
  records, which need to say which field was refused and which of several
  bounds applied; `input.cap.record_size` reports one cap, the record cap, and
  bounds the whole document instead. Details: `bucket`, `field` (the field's
  fixed openPapir name, never its value), `cap_bytes`, `observed_bytes`. The
  implemented fields and caps are in
  [architecture](architecture.md).

Caps are never relaxed to make one input succeed ([AGENTS.md](../AGENTS.md)).
The cap values themselves are proposals in
[archive-layout](archive-layout.md) and adjustable by review; the codes are
not.

### `path`: path-safety refusals

A violating path is refused and reported, never repaired, resolved, or
followed ([archive-layout](archive-layout.md)).

- **`path.symlink`**: input, not retryable. A path that must not be a
  symbolic link is one: the archive root, a directory inside it, an object, a
  record, or a component of an export destination. Details: `bucket`,
  `archive_path` when the path is inside the archive; otherwise `bucket` and
  `scope` (`archive`, `export_destination`, or `input`) only. `input` names a
  path the user supplied on the command line, which is outside the archive and
  has no archive-relative form. The whole-archive
  integrity check adds `path_count` additively and omits `archive_path`,
  because it may find several such paths and may name none of them.
- **`path.traversal`**: input, not retryable. A path derivation would leave
  the archive root. Reserved: user-supplied filenames are stored as attributes
  and never joined into a path, so this is a defence-in-depth code for a
  derivation bug or a hostile identifier, not an expected outcome. Details:
  `bucket`.
- **`path.overwrite`**: input, not retryable. A write would replace a file
  openPapir did not create: a stored object, a record, or an existing file at
  an export destination. Details: `bucket`, `archive_path` when inside the
  archive, `scope` otherwise.
- **`path.cross_device`**: input, not retryable. A rename would cross a
  device boundary; the write is abandoned and the archive is refused as
  misconfigured. Details: `bucket`, `archive_path`.

### `lock`: the single-writer lock

- **`lock.held`**: archive, **retryable**. Another writer holds the advisory
  lock. A second writer refuses rather than waiting indefinitely. Details:
  `bucket`. The lock file records the holder's process identifier, host, and
  start time; none of that is echoed, because a hostname is environment data
  the design gives no reason to publish.
- **`lock.stale`**: archive, not retryable by the same invocation. The lock's
  holder appears to be gone. A stale lock is **never** broken silently and
  never on a timeout; only an explicit user action takes it over, and it says
  what it found. Details: `bucket`, `evidence` (a short bucket name for what
  made the holder appear gone). What counts as proof is deferred to the
  artefact-import issue ([archive-layout](archive-layout.md)); until that is
  decided this code is reserved and its `evidence` values are unfixed.

### `write`: interrupted or incomplete writes

- **`write.interrupted`**: archive, **retryable**. A write was interrupted
  before its rename. The staging file is abandoned and never adopted, so the
  archive holds the complete artefact or nothing. Details: `bucket`,
  `stage` (`object_write`, `record_write`, or `marker_write`). The stage names
  what was being written, never which module reported it:

  | Stage | What it names |
  | --- | --- |
  | `object_write` | A stored object or an exported copy of one, the directory a copy is created in, a fan-out directory the repair cannot list, and a leftover staging file inside the object store. |
  | `record_write` | A record document, the directory one is written into, a cached file, a layout directory, a fan-out directory the repair cannot narrow, and the archive root. |
  | `marker_write` | The archive marker, and outside the archive the export destination itself and its `manifest.json`, which describe the export rather than any one record. |

  A refusal in an export destination carries `scope` `export_destination` and
  never an `archive_path` ([architecture](architecture.md)).
- **`write.incomplete`**: archive, not retryable. **Reserved.** A write
  completed fewer bytes than expected, or a stream ended early, and the
  partial file is removed. The design states only that an interrupted import
  leaves the complete artefact or nothing; how a short write is distinguished
  from an interruption is deferred to the artefact-import issue. Details:
  `bucket`, `stage`, `expected_bytes`, `observed_bytes`.

### `record`: record documents

- **`record.malformed`**: record, not retryable. **Decided by the case and
  submission records.** A record document a command must read exists but is
  not valid JSON, is missing a required field, claims a record kind this build
  does not know, or does not name the file it lives in. It is reported rather
  than repaired, skipped, or guessed at. Details: `bucket`, `record_kind`, and
  `path_count`, the number of documents that could not be read. The document's
  path and content are never reported: the path would name an identifier the
  caller never supplied, and the count answers the only useful question. A
  document that is not a regular file, and one larger than the record cap,
  report the same code and are never opened: the link is not followed and the
  bytes are never allocated.
- **`record.inconsistent`**: record, not retryable. **Added additively by the
  receipt and association records.** A record's fields exist and are each
  readable, but they cannot be true together under a rule
  [archive-layout](archive-layout.md) already states: an outcome whose
  candidate count the design forbids, one submission named as a candidate
  twice, a superseded record belonging to another receipt, or a named import
  event recording another artefact. The refusal happens before anything is
  written. Details: `bucket`, `record_kind` (the kind whose creation was
  refused, such as `association` or `receipt`) and `rule`, a short, stable
  snake_case name for the broken invariant. Nothing else: no identifier, no
  statement, no label, and no path. The implemented rules are

  | `rule` | Violation reported |
  | --- | --- |
  | `unassociated_has_candidates` | `unassociated` was given a candidate. |
  | `candidate_requires_candidates` | `candidate` was given no candidate. |
  | `associated_requires_one_candidate` | `associated` was not given exactly one candidate. |
  | `contradictory_requires_two_candidates` | `contradictory` was given fewer than two candidates. |
  | `duplicate_candidate_submission` | One submission was named as a candidate more than once. |
  | `supersedes_other_receipt` | The superseded record belongs to another receipt. |
  | `import_event_digest_mismatch` | A named import event records another artefact. |

  A rule name is stable once published, and a new rule is an additive change
  like a new code. This is a separate code from `record.not_found`, which
  answers a different question: a reference that names nothing at all, rather
  than a set of fields that cannot be true together.
- **`record.not_found`**: record, not retryable. A reference names no record
  or object in this archive: a case identifier with no case record, or an
  artefact digest with no stored object. An identifier that cannot name a
  record at all, because it is not openPapir's 32-character hexadecimal form,
  is refused with the same code and is never joined into a path. Details:
  `bucket`, `record_kind` (the kind that was not found, such as `case` or
  `artefact`) and `reference_kind` (how it was named, such as `case_id` or
  `artefact_digest`). The value the user supplied is never echoed.

### `integrity`: stored bytes disagree with what is recorded

- **`integrity.digest_mismatch`**: archive, not retryable. **Implemented by
  the whole-archive integrity check.** A stored object's bytes no longer
  digest to its own path. The object is reported as damaged and never
  overwritten, repaired, or removed. An entry under `objects/` whose name is
  not a digest, or which is filed under fan-out directories that do not match
  its name, is a malformed object entry and reports the same code, because an
  object's expected digest is its own path and such an entry disagrees with
  the path it has. Details: `bucket`, `archive_path`, `digest` (the expected
  digest, which is already the object's path). The whole-archive check omits
  both and carries `count` instead: it may find several damaged objects, and
  naming one of them would publish the path and digest of a file the report
  otherwise reduces to a count.
- **`integrity.length_mismatch`**: archive, not retryable. On a duplicate
  import the stored object's byte length differs from the incoming length,
  which means the store is damaged; it is reported, never overwritten
  ([archive-layout](archive-layout.md)). The whole-archive check reports the
  same code where a stored object's length differs from every import event
  that names it. Details: `bucket`, `archive_path`, `expected_bytes`,
  `observed_bytes`; the whole-archive check carries `count` alone, for the
  reason above.
- **`integrity.orphan_object`**: archive, not retryable. **Decided by the
  whole-archive integrity check.** A stored object that no import event, no
  receipt, and no submission references. It is a report entry with a count
  rather than a refusal of anything: the check names it only when nothing
  ahead of it in the precedence below was found, and it never removes an
  orphan, because removal is the explicit purge the deletion design describes
  and no code here implements. Details: `bucket`, `count`.
- **`integrity.dangling_reference`**: archive, not retryable. **Added
  additively by the whole-archive integrity check.** A record names a digest,
  case, submission, receipt, import event, or association this archive does
  not hold. It is the mirror of `integrity.orphan_object`, which is an object
  no record names, and a separate code from `record.not_found`, which answers
  a reference the user supplied rather than one already stored. Details:
  `bucket`, `record_kind` (the kind of record that held the reference),
  `reference_kind` (how it was named, such as `artefact_digest` or `case_id`),
  and `path_count`, the number of dangling references found. Nothing else: no
  identifier, no digest, and no path.

### `export`: writing outside the archive

- **`export.destination_conflict`**: archive, not retryable. The export
  destination already holds a file openPapir would have to replace. Export
  never overwrites and refuses instead ([archive-layout](archive-layout.md)).
  Details: `bucket`, `scope` (`export_destination`), `conflict_count`, and
  the additive `export_path`, the path relative to the destination, which is
  built from openPapir's own directory names plus a digest or an identifier.
  The destination path itself is user-supplied and outside the archive, so it
  is not echoed.
- **`export.copy_mismatch`**: archive, not retryable. An exported copy
  re-digested to something other than the original. Details: `bucket`,
  `digest`, `conflict_count`.

Both are implemented; [architecture](architecture.md) is authoritative for
what emits them. Every refusal raised inside an export destination carries
`scope` `export_destination`, and none carries `archive_path`: the path it
would name belongs to an archive the refusal is not about.

### `delete`: deletion and purge

- **`delete.objects_retained`**: archive, not retryable. **Decided by
  `case delete`.** A purge could not remove every object it planned to
  remove. Details: `bucket`, `retained_count`, and the additive `reason`,
  whose only implemented value is `unremovable`. Nothing else: no digest, no
  path, and no identifier. `referencing_record_ids` and `orphan_count` are
  **not** emitted. An object a remaining record still references is not a
  refusal at all, because the purge was never entitled to remove it: it is
  reported in `data` as a retained object with the reason
  `referenced_elsewhere`, alongside `purge_not_requested` for an object left
  because no purge was asked for. Reporting the identifier of a record that
  survived would also say which surviving record points at content the user
  asked to purge, which the privacy rule below does not allow.
  `case delete` is the second command whose `data` survives a failure: the
  deletion completed everything else, so the counts stay in `data` and the
  error names only how many objects are still in the store
  ([architecture](architecture.md)).
- **`delete.records_retained`**: archive, not retryable. **Added additively by
  `case delete`.** A record document the filesystem refused to unlink. It
  stops the object pass entirely rather than purging around the record that
  stayed: a record that is still there still names its artefacts, so removing
  them would leave it naming bytes the archive no longer holds. Details:
  `bucket` and `retained_count`, the number of documents the deletion planned
  to remove and did not, the ones it never reached included. Nothing else: no
  record kind that would narrow it to one document, no identifier, no path.
  Like `delete.objects_retained` it keeps the deletion's counts in `data`, and
  it is reported ahead of it, because it is the reason nothing was purged.
  The record pass is all or nothing per case: every directory it would remove
  an entry from, and every document it would unlink, is probed before the
  first unlink, so this refusal normally arrives with `records_removed_total`
  at `0` and the archive exactly as it was. A deletion that removed part of
  its records and then stopped could not be finished by re-running it, and an
  object whose last referencing record went in that pass would silently stop
  being a purge candidate; refusing first is the only outcome the user can
  undo. The probe is not a promise: it reads permission bits before the
  unlink rather than performing it, so a filesystem that changes in between,
  or refuses through an access-control list or an immutable flag, still stops
  the pass part way and reports the same code with a non-zero
  `records_removed_total` ([architecture](architecture.md)).
- **`delete.record_entangled`**: archive, not retryable. **Added additively by
  `case delete`.** A record that has to survive the deletion names a record
  the deletion would remove. Only an association reaches it today, by naming
  submissions in two cases: it references a submission that remains, so it
  cannot go, and one that is going, so keeping it whole would leave a dangling
  reference. openPapir edits no stored record, so the deletion is refused in
  the scan, before anything is unlinked, rather than corrupting the
  association or deleting the user's assertion about a case they did not name.
  Details: `bucket`, `record_kind`, `retained_count`; never an identifier.
  `data` is `{}` like any other refusal.

Deletion itself is real and its summary is **not persisted**: counts and
record kinds only, never filenames, digests, or titles
([archive-layout](archive-layout.md)). Deletion does not erase data from the
storage medium and no message may claim that it does.

### `platform`: environment cannot provide a guarantee

- **`platform.filesystem_unsupported`**: platform, not retryable. The
  filesystem cannot provide a guarantee the archive requires, so the archive
  is unsupported there. Two conditions reach it: the filesystem cannot express
  owner-only access, and the filesystem cannot create the hard link the atomic
  write procedure publishes with, which is the case on FAT32 and exFAT. This
  is a deliberate refusal, not a degradation, and not retryable: the same call
  on the same filesystem never succeeds
  ([archive-layout](archive-layout.md)). Details: `bucket`, `capability`
  (`hard_link` or `owner_only`), `stage`. The codes that map to it are
  whatever the platform reports for an unsupported operation: on Unix `EPERM`,
  which `link(2)` documents as the filesystem not supporting hard links, and
  `EOPNOTSUPP`; on Windows `ERROR_INVALID_FUNCTION` (1) and
  `ERROR_NOT_SUPPORTED` (50). Every other failure of the same call keeps its
  own code, so a permission or space failure is still `write.interrupted`.
- **`platform.no_directory_fsync`**: platform. Used as a **warning**, never
  as an error; see below.
- **`platform.no_follow_after_open`**: platform. Used as a **warning**, never
  as an error; see below.
- **`platform.replace_while_open`**: platform, **retryable**. Replacing or
  removing a file failed because another process holds it open. As an error
  this stops the operation; the retry is the user's, after closing the other
  process. `case delete` emits it as a **warning** instead, because a
  deferred unlink stops that one object rather than the purge: the command
  completes, counts the object as `unremovable`, and reports
  `delete.objects_retained` ([architecture](architecture.md)). Details:
  `bucket`, `stage`, and, from `case delete`, the additive
  `read_only_restored`. Where a platform needs a read-only attribute cleared
  before a file can be unlinked, it is put back when the unlink still fails,
  so a surviving object keeps the access it had; the flag says whether the
  restore succeeded, because a failure to restore it is a weakening the
  caller must be told about. The flag is emitted **only on the path that
  actually cleared the attribute**. Where reading the permissions or clearing
  the attribute failed, nothing was cleared, the file is exactly as it was,
  and the flag is **absent** rather than `true`: absence says the attribute
  was never cleared and nothing was widened, which is a different statement
  from a restore that succeeded, and a caller must not read one as the other.
  One purge may defer several objects, and the code is still reported once:
  the entry carries the worst outcome any of them saw, so
  `read_only_restored` is `false` whenever a single object was left writable,
  whichever object it was and in whatever order it was reached, and a
  deferral that carried no flag at all never displaces one that reports
  `false`.
- **`platform.owner_only_via_acl`**: platform. Used as a **warning**, never
  as an error; see below.

### `internal`: a bug in openPapir

- **`internal.unexpected`**: internal, not retryable. An invariant this
  document or [archive-layout](archive-layout.md) states was violated. The
  message asks for a report under [SECURITY.md](../SECURITY.md) when the
  condition may be security-relevant. Details: `bucket` only; never a
  backtrace, never a path, never any input-derived value.

## Exit codes

The exit code carries the bucket and nothing else. It **never** encodes a
count, an identifier, a digest, a cap value, or the number of failed inputs.

- `0`: success. `ok` is `true`. Duplicate imports and every association
  outcome exit `0`, and a warning alone never raises the exit code above `0`.
- `2`: usage error. Bucket `usage`.
- `3`: refused input. Buckets `input` and `path`.
- `4`: archive state. Buckets `archive`, `lock`, `write`, `record`,
  `integrity`, `export`, and `delete`.
- `5`: platform or environment. Bucket `platform`.
- `6`: internal error. Bucket `internal`.

`1` is reserved and not emitted, so that a code of `1` is recognisable as
something other than a contract failure. `101`, the Rust panic exit, is
likewise never a contract value; if it appears it is `internal` by definition
and a bug.

A command that reports several failures still exits with one code: the highest
of the exit-code groups above, in the order `6` > `5` > `4` > `3` > `2`. That
covers all twelve buckets, because every bucket maps to exactly one group.
The envelope's single `error` object names the first refusal; the rest, when a
command is defined to continue past one, appear in `data` as counts and code
buckets, never as an expanded per-input list of names.

## Reporting platform degradation

[archive-layout](archive-layout.md) names three weakenings, and the
implementation added a fourth, `platform.no_follow_after_open`. Each must be
reported at the point of the write and in archive health output, never
silently accepted and never described as equivalent, and each is a **warning
inside the envelope**: on its own it leaves `ok` at `true` and the exit code
at `0`, and the operation is not retried or downgraded. A degradation observed before
a later step fails is still reported, in the `warnings` array of the failing
envelope; it is never dropped because the command ended badly.

- **`platform.no_directory_fsync`**: the directory entry created by a rename
  may not be durable after power loss, although the file content was flushed.
  Details: `bucket`, `stage` (`object_write`, `record_write`, or
  `marker_write`).
- **`platform.replace_while_open`**: emitted as a warning only where the
  design permits the operation to continue; where it stops the write it is the
  error of the same name. Details: `bucket`, `stage`.
- **`platform.owner_only_via_acl`**: owner-only access is expressed as an
  access-control list rather than a permission bit and therefore depends on
  the underlying filesystem. Details: `bucket`.
- **`platform.no_follow_after_open`**: the platform has a no-follow open flag,
  so no path is stat-ed before it is opened, but the flag opens the link
  itself rather than failing, so the refusal comes from the handle openPapir
  opened rather than from the system call. The reparse tag is not
  distinguished either, so a junction is refused as `path.symlink` like a
  symbolic link. Emitted on Windows. Details: `bucket`.

A warning is never omitted because the command otherwise succeeded, and
success is never reported without the warning that applies. Where an
environment cannot provide owner-only access at all, the outcome is the
`platform.filesystem_unsupported` error, not a warning.

## Whole-archive integrity report

The integrity check re-digests stored objects and reports damage, orphans, and
references that resolve nowhere ([archive-layout](archive-layout.md)). It is
implemented as `archive check`
([architecture](architecture.md)), which is the canonical description. Its
`data` is **counts and buckets only**, never a list of damaged paths, digests,
or record titles:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "archive.check",
  "data": {
    "bytes_digested": 16,
    "objects_checked": 1,
    "orphan_objects": 0,
    "objects_unchecked": 0,
    "problems": [
      { "code": "integrity.dangling_reference", "count": 0 },
      { "code": "integrity.digest_mismatch", "count": 0 },
      { "code": "integrity.length_mismatch", "count": 0 },
      { "code": "integrity.orphan_object", "count": 0 },
      { "code": "path.symlink", "count": 0 },
      { "code": "record.malformed", "count": 0 }
    ],
    "records_checked": 1,
    "records_unchecked": 0,
    "records_staging_files": 0,
    "staging_files": 0
  },
  "verified": false
}
```

`problems` lists every code the check can report, including the ones it did
not see, ordered by code, so a caller reads a count rather than testing for a
key. `objects_unchecked`, `records_unchecked`, `staging_files`, and
`records_staging_files` were added additively by the implementation. The first
counts what the check could not read under `objects/`: an object over the
single-file cap, an entry whose metadata could not be read, and each directory
under `objects/` that could not be listed, including the store itself. The
second counts each `records/<kind>` directory that is there and could not be
listed; a directory that is absent reads as empty and is not counted. The
third counts what `objects/incoming/` still holds. The fourth counts the
leftover staging files the `records/<kind>` directories hold together, and
leaves the meaning of the third unchanged: a staging file is openPapir's own
transient artefact from an interrupted write, never a record and never a
malformed one, so no code is reported for it and it does not affect the exit
code. None of them is damage, and the check removes none of them. A
digest under a directory that could not be listed is left uncounted rather
than reported as a dangling reference, because an object the check could not
look for is not an object the archive does not hold. The same rule governs an
unread record directory: while `records/imports`, `records/receipts`, or
`records/submissions` is unread no object is reported as
`integrity.orphan_object`, and a reference into any unread kind is left
unjudged rather than reported as `integrity.dangling_reference`.

`ok` is `true` and the exit code `0` when the check found nothing. When it
found something, the check still completed its stated work, so the report
stays in `data` while `ok` becomes `false` and `error` names the first problem
in this fixed precedence:

`path.symlink`, `record.malformed`, `integrity.digest_mismatch`,
`integrity.length_mismatch`, `integrity.dangling_reference`,
`integrity.orphan_object`.

The order runs from what stopped the check reading something, through what it
read and disbelieved, to what is merely unreferenced, and it is fixed so that
one archive always reports one code. The exit code is the highest of the
buckets' groups, as this document already requires of a command that reports
several conditions: `4` whenever a `record` or `integrity` condition was
found, and `3` for an archive whose only complaint is a link inside the store.

The `error` object carries counts and kinds and no path, name, or digest,
even where the same code carries `archive_path` elsewhere. A check that could
not run at all (a missing marker, a newer schema version) is an error in its
own bucket with an empty `data`, exactly like any other refusal. A held writer
lock is not such a condition: the check never takes the lock and never waits
for a writer. Neither is a root the user cannot write to, or a layout
directory that is absent: the check opens the archive without creating or
flushing anything and reads a missing directory as an empty one. `verified`
stays `false` because re-digesting is a storage-layer identity check, not a
cryptographic verification.

## Results that are not errors

### Duplicate import

Re-importing bytes already present is **not an error**
([archive-layout](archive-layout.md)). `ok` is `true`, the exit code is `0`,
and the outcome is a field in `data`. The object is untouched and a second
import event is recorded, so an import-event count is history, not an anomaly:

```json
{
  "digest": "sha256:0f1e...",
  "created_object": false,
  "import_event": "3b7f5c1d9e2a4b6c8d0e2f4a6b8c0d1e",
  "previous_import_count": 2,
  "first_imported_at": "2026-01-14T09:12:33Z"
}
```

A length mismatch on a duplicate is the separate
`integrity.length_mismatch` error, because it means the store is damaged.

### Association outcomes

`unassociated`, `candidate`, `associated`, and `contradictory` are the four
recorded outcomes ([archive-layout](archive-layout.md)). All four are results
in `data`, all exit `0`, and none is an error, `contradictory` least of all,
since retaining conflicting evidence is the designed behaviour. Their minimal
shape follows the association record:

```json
{
  "association": {
    "id": "5c9d1e3f7a2b4c6d8e0f1a2b3c4d5e6f",
    "receipt_id": "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d",
    "submission_id": null,
    "outcome": "candidate",
    "created_by": "user",
    "created_at": "2026-01-14T09:12:33Z",
    "supersedes": null,
    "candidates": [
      {
        "submission_id": "7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b",
        "confidence": "moderate",
        "evidence": [
          {
            "kind": "user_assertion",
            "source": "user",
            "statement": "The reference matches the submission."
          }
        ]
      }
    ]
  }
}
```

`receipt_id` here is openPapir's own minted receipt-record identifier, which
the privacy rule allows, as distinct from any receipt identifier issued by an
authority, which is never emitted.

The shape above is what this build writes. `created_by` `automatic`, an
evidence `kind` other than `user_assertion`, a `source` other than `user`, and
an `extractor` field remain **proposed**: no automatic matching, derived
metadata, or extractor exists, so no other value could be recorded honestly
([architecture](architecture.md)).

`submission_id` is `null` for `unassociated`, `candidate`, and
`contradictory`, and non-null only for `associated`, where it equals the one
candidate the user confirmed. Naming a submission for `contradictory` would
resolve the contradiction the record exists to retain. `candidates` holds one
entry for `associated`, one or more for `candidate`, two or more for
`contradictory`, and none for `unassociated`. `confidence` is the closed
ordinal set `weak`, `moderate`, `strong` and is never a number, because no
calibration data exists. A candidate set is never collapsed to a single best
guess automatically. No outcome, and no wording around one, implies delivery,
receipt by an authority, authenticity, or legal effect
([receipt-discovery](receipt-discovery.md)); `verified` stays `false` for all
four.

## Privacy rule for all output

Real correspondence is private ([AGENTS.md](../AGENTS.md),
[SECURITY.md](../SECURITY.md)). The rule binds `message`, `details`, `data`,
human-readable output, and anything written to stderr.

Allowed, and only where the design already permits it:

- Counts, byte lengths, cap values, indexes, and durations.
- Bucket names, error codes, record kinds, and outcome labels.
- Archive-relative internal paths such as `objects/sha256/ab/cd/…` or
  `records/receipts/<id>.json`, which contain only digests and
  openPapir-minted identifiers.
- Identifiers openPapir minted itself: record identifiers, import-event
  identifiers, the archive's own opaque identifier.
- SHA-256 digests of stored artefacts, in the contexts where
  [archive-layout](archive-layout.md) already exposes them (object paths,
  export manifests, integrity reporting) and never presented as verification.
- Timestamps openPapir recorded.

Never, by default and with no flag to enable it:

- Original filenames, or any sanitised or truncated form of one.
- User-supplied paths outside the archive, including the archive root itself.
  The one exception is the export destination in human-readable output, which
  repeats the `--to` argument the user typed in the same invocation back to
  them; it never enters `data`, `message`, `details`, or stderr, and no other
  user-supplied path is echoed anywhere.
- Payload bytes, excerpts, extracted text, or parsed field values.
- Signer information, certificate data, subject or issuer names.
- Any government identifier, or any receipt identifier issued by an
  authority, as distinct from openPapir's own minted receipt-record
  identifier, which is allowed.
- User-supplied titles, labels, descriptions, and notes.
- Hostnames, usernames, or process owners.

Private-corpus reporting stays aggregate: counts and stable error-code buckets
only, never an enumeration ([AGENTS.md](../AGENTS.md),
[testing](testing.md)). A message that would otherwise need a forbidden value
omits it and relies on the code and the counts instead; "which file" is
answered by `input_index`, not by a name.

## What of this is implemented

Part of this contract now has code behind it.
[architecture](architecture.md) is the canonical description of that part and
lists exactly which codes are emitted; where the two disagree, architecture is
authoritative and this page is a defect. `capabilities --json` reports the
implemented operations, and `verified` stays `false` in every envelope.

The whole-archive integrity check is implemented, so the report above, the
precedence between its codes, `integrity.digest_mismatch`,
`integrity.length_mismatch`, `integrity.dangling_reference`, and
`integrity.orphan_object` are contract rather than proposal.

The receipt and user-asserted association records are implemented, so the four
association outcomes and the association shape above are contract rather than
proposal, and `record.inconsistent` is emitted. Automatic association,
derived metadata, and any extractor stay unimplemented: every evidence entry
this build writes carries `kind` `user_assertion` and `source` `user`, and
every association carries `created_by` `user`.

Case export and the permission repair are implemented, so
`export.destination_conflict` and `export.copy_mismatch` are contract rather
than proposal, and `archive.permissions_wide` now has a documented remedy
rather than only a refusal.

Deleting a case is implemented, so `delete.objects_retained`, its `reason`
detail, the additive `delete.records_retained` and `delete.record_entangled`,
and the rule that a deletion reports counts and record kinds and persists
nothing are contract rather than proposal. A deletion never leaves a record or
an object naming something the archive no longer holds: it refuses instead,
and every refusal above leaves the archive as `archive check` found it.

Everything else here is still a proposal. Of the catalogue, exactly three
codes are unreachable in this build and remain reserved: `lock.stale`,
`path.traversal`, and `write.incomplete`. Every other code in the catalogue is
emitted, and [architecture](architecture.md) says by what. Agreeing a code
here creates no capability and no obligation on a user's archive.

## Origin and unblocked work

This is follow-up 5 of [archive-layout](archive-layout.md), listed there under
its unblocked follow-ups and required before import printed anything
machine-readable. With it agreed, these bounded implementation issues were
written and reviewed, and all but one are now implemented:

- **Artefact import with byte preservation**: implemented as `archive init`
  and `import`, using the `input`, `path`, `archive`, `lock`, `write`, and
  `platform` codes ([architecture](architecture.md)).
- **Association records with candidate and contradictory outcomes**: had the
  outcome shape and the rule that no outcome is an error, and is now
  implemented for user-asserted associations
  ([architecture](architecture.md)).
- **Whole-archive integrity check**: implemented as `archive check`, with the
  counts-and-buckets report shape above.
- **Derived-metadata staleness and recompute-on-request**: not implemented.
  It has the record and cap codes it needs.
- **Case deletion with an explicit purge**: implemented as `case delete`,
  with the counts-and-reasons report shape and `delete.objects_retained`.
- **Export, backup, and the permission-repair action**: implemented as
  `case export` and `archive repair-permissions`, using
  `export.destination_conflict`, `export.copy_mismatch`, and
  `archive.permissions_wide` ([architecture](architecture.md)).

Unchanged blockers: receipt parsing still needs the format gap closed, and the
delegated verification boundary still needs published, versioned contracts
from openKRX and openSzigno ([archive-layout](archive-layout.md)). No
verification code is specified here for that reason.

## Limits of this specification

It fixes wire names, not behaviour, and no capability follows from it. It
assumes the archive design as written; if a review changes an adoption rule, a
cap, a lock semantic, or the deletion rule, the affected codes change with it.
`lock.stale`, `path.traversal`, and `write.incomplete` are reserved against
conditions the design has not fully decided, and they are the only reserved
codes left; `archive.marker_malformed`, `record.malformed`,
`integrity.orphan_object`, and `delete.objects_retained` have since been
decided by the implementations that reached them, and their catalogue entries
say so. Command
names and flags are proposals only. It fixes no field, identifier, or format
of any government artefact, and assumes nothing about what a receipt
contains, because nothing is yet established about that
([receipt-discovery](receipt-discovery.md)).
