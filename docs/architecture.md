# Architecture and CLI contract

## Documentation map

This document is the canonical contract of what openPapir actually does. For
the wider picture, purpose, scope, non-goals, which designs are decided but
not built, and which contracts are still blocked, start at the
[specification index](specification.md). Every document has a one-line purpose
in the [documentation index](index.md).

## Current implementation

The Rust edition 2024 workspace has an MSRV of 1.88 and two unpublished crates:

| Crate | Current responsibility |
| --- | --- |
| `openpapir-core` | The local archive: marker, artefact store, atomic writes, single-writer lock, input caps, path safety, import-event records, the case, submission, receipt, and association records, and the read-only whole-archive integrity check. |
| `openpapir-cli` | Argument parsing, the response envelope, and the exit-code mapping. |

Only these invocations are supported:

```sh
openpapir --help
openpapir --version
openpapir capabilities [--json]
openpapir archive init <root> [--json]
openpapir archive check --archive <root> [--json]
openpapir import --archive <root> <file>... [--json]
openpapir case create --archive <root> --title <t> [--notes <n>] [--json]
openpapir case list --archive <root> [--json]
openpapir case show --archive <root> <case-id> [--json]
openpapir submission add --archive <root> --case <case-id> --description <d> [--date <yyyy-mm-dd>] [--artefact <digest>[:<role>]]... [--json]
openpapir receipt add --archive <root> --artefact <digest> [--import-event <id>] [--label <l>] [--json]
openpapir receipt list --archive <root> [--json]
openpapir association create --archive <root> --receipt <receipt-id> --outcome <outcome> [--candidate <submission-id>:<confidence>:<statement>]... [--supersedes <association-id>] [--json]
openpapir association list --archive <root> --receipt <receipt-id> [--json]
```

Eleven operations are implemented, `archive.init`, `import`, `case.create`,
`case.list`, `case.show`, `submission.add`, `receipt.add`, `receipt.list`,
`association.create`, `association.list`, and `archive.check`, and those are
the eleven names `capabilities` reports. Everything else in
[local archive layout and storage design](archive-layout.md) and
[import error, JSON, and exit-code contract](error-contract.md) remains a
design: no derived-metadata or verification records; no automatic matching, no
receipt parsing, and no export, deletion, editing, repair, or migration.
openPapir reads artefact bytes only to re-digest a stored object during the
integrity check, and never to form an opinion about what an artefact says, so
an association is only ever the user's own assertion.

## The response envelope

Every command prints exactly one JSON object on stdout in its `--json` form,
on one line, and nothing else. Human-readable output goes to stdout only in the
non-`--json` form; warnings and errors go to stderr there, so diagnostic text
never shares stdout with the JSON object.

| Key | Meaning |
| --- | --- |
| `schema_version` | The envelope's version, currently `1`, independent of `archive_schema_version`. |
| `ok` | `true` only when the command completed its stated work. |
| `command` | The invoked command's stable name: `capabilities`, `archive.init`, or `import`. |
| `data` | The command's result. `{}` when `ok` is `false`, except `archive check`, whose report is the result the user asked for and stays in `data` beside the error. |
| `verified` | Always `false`. No cryptographic check is implemented. |
| `error` | Present exactly when `ok` is `false`: `code`, `message`, `details`. |
| `warnings` | Present only when a platform degradation was observed. |

`code` is the only field a caller may branch on. `message` wording may change
within a `schema_version`. `details` carries at most 16 keys, whose values are
strings, integers, booleans, or arrays of at most 16 such scalars, and always
carries `bucket`. Changes within `schema_version` are additive only.

The capabilities response is unchanged in shape and now lists the eleven
implemented operations:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "scaffold",
    "operations": [
      "archive.init",
      "import",
      "case.create",
      "case.list",
      "case.show",
      "submission.add",
      "receipt.add",
      "receipt.list",
      "association.create",
      "association.list",
      "archive.check"
    ]
  },
  "verified": false
}
```

`verified` is false because no cryptographic check took place. A SHA-256
digest here is a storage-layer identity: it says two files hold the same
bytes, and nothing about authenticity, origin, delivery, or legal effect.

## `archive init`

`openpapir archive init <root>` creates an archive in an existing, empty
directory. openPapir never searches for an archive, never adopts a directory
that has no marker, and never creates one as a side effect of another command.

The marker `papir-archive.json` is written first, through the atomic write
procedure, and records the archive's own opaque identifier, its schema
version, and the creating openPapir version. The directories below are then
created owner-only:

```text
<archive-root>/
    papir-archive.json
    objects/sha256/
    objects/incoming/
    records/imports/
    records/cases/
    records/submissions/
    records/receipts/
    records/associations/
    cache/
```

Creation narrows the supplied root to owner-only permissions. It only ever
narrows; there is no flag that widens anything.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "archive.init",
  "data": {
    "archive_id": "a1ed055fda816d568861282470872eea",
    "archive_schema_version": 1
  },
  "verified": false
}
```

## `import`

`openpapir import --archive <root> <file>...` stores each file's bytes in the
content-addressed artefact store and records one import event per input.

- The object path is `objects/sha256/ab/cd/<digest>`. Objects are write-once
  and become owner-read-only; an existing object is never modified, truncated,
  or replaced.
- The bytes are preserved exactly. Nothing is converted, re-encoded, or
  normalised.
- Each import writes `records/imports/<id>.json`, a JSON document with sorted
  keys holding the digest, the byte length, the import timestamp, whether the
  object was created, and the original filename as an attribute.
- Re-importing bytes already present is not an error. The object is left
  untouched, a second import event is recorded against it, and the response
  reports `previous_import_count` and `first_imported_at`. An import-event
  count is history, not an anomaly.
- A stored object whose length differs from the incoming length is reported as
  `integrity.length_mismatch` and never overwritten.

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
        "digest": "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f",
        "byte_length": 16,
        "import_event": "95cab2db3374dc619f89e5cf2494039e",
        "created_object": true
      }
    ]
  },
  "verified": false
}
```

A duplicate adds `previous_import_count` and `first_imported_at` to the
artefact entry and still exits `0`.

## `archive check`

`openpapir archive check --archive <root>` re-digests every stored object and
compares what the artefact store holds with what the records claim. It is
read-only in the strongest sense the design allows: it takes no writer lock,
so a held lock never stops it; it opens every file read-only and with the
platform's no-follow flag; and it creates, renames, removes, and repairs
nothing, including a leftover staging file, which it counts and leaves alone.
Opening an archive to check it differs from opening one to write to it in
exactly that way: no missing layout directory is created and no directory
entry is flushed, so the check completes on a root the user cannot write to
and reports what the archive holds rather than repairing its shape. A layout
directory that is absent is read as empty. The refusal of a layout directory
that is a symbolic link is kept, because a reader that walked a linked
`records/<kind>` would read outside the archive.

A re-computed digest is a storage-layer identity. A check that finds nothing
says the stored bytes are the bytes their paths name and that every reference
resolves inside this archive. It says nothing about authenticity, origin,
delivery, or legal effect, so `verified` stays `false` whatever the outcome.

The check reads the records first, keeping only fixed-size keys (a 32-byte
digest, a 16-byte identifier) rather than the documents, then streams each
object through SHA-256 in 64 KiB chunks, then resolves every reference. Its
memory therefore grows with the number of records and objects an archive
holds, never with their size.

| Condition | Code |
| --- | --- |
| A stored object's bytes no longer digest to its own path. | `integrity.digest_mismatch` |
| An entry under `objects/` whose name is not a digest, or which is filed under fan-out directories that do not match its name. | `integrity.digest_mismatch` |
| A stored object's byte length differs from every import event that names it. | `integrity.length_mismatch` |
| A record names a digest, case, submission, receipt, import event, or association this archive does not hold. | `integrity.dangling_reference` |
| A stored object that no import event, receipt, or submission references. | `integrity.orphan_object` |
| A record document that cannot be read as a record of its kind. | `record.malformed` |
| A symbolic link, or any other non-regular file, inside `objects/`. | `path.symlink` |

`data` is the whole-archive integrity report of
[error-contract](error-contract.md): counts and stable codes only. It never
carries the path, the name, or the digest of a damaged object, and neither
does the `error` object derived from it, because the count answers the only
question the privacy rule allows an answer to.

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
    "staging_files": 0
  },
  "verified": false
}
```

`problems` lists every code the check can report, including the ones it did
not see, ordered by code, so a caller reads a count rather than testing for a
key's presence. `objects_unchecked` counts what the check could not read: an
object over the single-file cap, an entry whose metadata could not be read,
and each directory under `objects/` that could not be listed, including the
store itself. None of them is reported as damage, because the check did not
read them to say so, and a digest under a directory that could not be listed
is not counted as a dangling reference either: an object the check could not
look for is not an object the archive does not hold. Whether a directory is
absent or merely unreadable is taken from the failure itself, because a
directory the process cannot search reports as missing when it is asked
whether it exists. `records_unchecked` counts the same condition on the record
side: each `records/<kind>` directory that is there and could not be listed.
The records it may hold were not read, so nothing is concluded from their
absence. While `records/imports`, `records/receipts`, or `records/submissions`
is unread no stored object is reported as `integrity.orphan_object`, because
only those three kinds reference an object; while `records/cases` or
`records/associations` is unread a reference that would name one of them is
left unjudged rather than counted as a dangling reference. A record directory
that is simply absent is read as empty and is not counted here.
`staging_files` counts what `objects/incoming/` still holds.

A clean archive exits `0` with `ok` `true`. When the check finds something,
`ok` is `false`, the report stays in `data`, and `error` names the first
problem in this fixed precedence:

`path.symlink`, `record.malformed`, `integrity.digest_mismatch`,
`integrity.length_mismatch`, `integrity.dangling_reference`,
`integrity.orphan_object`.

The order runs from what stopped the check reading something, through what it
read and disbelieved, to what is merely unreferenced, and it is fixed so that
one archive always reports one code. The exit code is the highest of the
buckets' groups, as the contract requires of any command that reports several
conditions: `4` whenever a `record` or `integrity` condition was found, and
`3` for an archive whose only complaint is a link inside the store.

Human output prints the same counts in the same order and no path.

```text
Checked 1 object(s) and 1 record(s); 16 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

An archive that cannot be opened at all is a refusal rather than a report,
with the codes `archive init` and `import` already use, and `data` is then
`{}` like any other refusal.

## Records

A case, a submission, a receipt, and an association are the user's own local
organisation. A case corresponds to nothing any government service issues, and
a submission is something the user states they sent: openPapir sends nothing,
so a submission is always user-asserted. A receipt records that the user
believes a stored artefact to be a receipt, and an association records what
the user asserts about whether that receipt relates to a submission. None of
them asserts delivery, receipt by an authority, authenticity, or legal effect,
and openPapir reads no artefact bytes to form an opinion of its own.

Imported, matched, and authenticity-verified stay separate records. A receipt
may exist with no association, and creating an association creates no
verification result, because none exists to create.

Every record is one UTF-8, LF-terminated JSON document with sorted keys,
holding `id` (a 128-bit random identifier as 32 lowercase hexadecimal
characters), `record_kind`, `archive_schema_version`, and `created_at`. Records
reference each other, and reference stored artefacts, by identifier only.
Records are written through the atomic write procedure, under the writer lock,
into owner-only directories. Opening an archive created by an earlier build
adds the two record directories if they are absent; nothing else changes.

| Record | Path | Fields |
| --- | --- | --- |
| Case | `records/cases/<id>.json` | `title`, optional `notes`, plus the four common fields. |
| Submission | `records/submissions/<id>.json` | `case_id`, `description`, optional `stated_date`, `artefacts`, plus the four common fields. |
| Receipt | `records/receipts/<id>.json` | `artefact_digest`, `import_event_id`, optional `label`, plus the four common fields. |
| Association | `records/associations/<id>.json` | `receipt_id`, `outcome`, `candidates`, `created_by`, `submission_id`, `supersedes`, plus the four common fields. |
| Import event | `records/imports/<id>.json` | `digest`, `byte_length`, `imported_at`, `created_object`, `original_filename`, `id`, `record_kind`, `archive_schema_version`. |

Each entry of `artefacts` is an object with `digest`, the algorithm-qualified
digest of an object already stored in this archive, and an optional `role`, the
short label the user gave that artefact. `role` is omitted when the user
supplied none, so an entry is `{"digest": "sha256:..."}` or
`{"digest": "sha256:...", "role": "cover letter"}`.

`stated_date` is the user's own statement about their own submission. It is
accepted as `YYYY-MM-DD` only, checked for calendar plausibility, stored
verbatim, never compared with `created_at`, and never interpreted. It is not a
delivery date, a receipt date, or evidence of anything.

Every user-supplied field is bounded in bytes, and the bound is checked before
the record is built, so an oversized field is refused before the archive is
touched. `title`, `role`, `label`, and `statement` are single lines and carry no
control character; `notes` and `description` may carry a line feed and no
other control character.

| Field | Cap | Required | Code when it is too long |
| --- | --- | --- | --- |
| `title` | 200 bytes | Yes | `input.cap.field_length` |
| `notes` | 4096 bytes | No | `input.cap.field_length` |
| `description` | 1024 bytes | Yes | `input.cap.field_length` |
| `role` | 64 bytes | No | `input.cap.field_length` |
| `label` | 200 bytes | No | `input.cap.field_length` |
| `statement` | 512 bytes | Yes, per candidate | `input.cap.field_length` |

An empty required field, a forbidden control character, an unusable artefact
reference, and a date that is not a calendar date are `usage.arguments`,
naming the argument and never its value.

## `case create`

`openpapir case create --archive <root> --title <t> [--notes <n>]` writes one
case record. It takes the writer lock and runs the archive's permission checks
first.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "case.create",
  "data": {
    "case": {
      "archive_schema_version": 1,
      "created_at": "2026-01-14T09:12:33Z",
      "id": "6b73d041fb6bed26be75545fabfd45bc",
      "notes": "First contact.",
      "record_kind": "case",
      "title": "Tax matter"
    }
  },
  "verified": false
}
```

## `case list`

`openpapir case list --archive <root>` reads every case record. It takes no
lock, because no record file is ever modified in place. `data` holds `cases`,
ordered by identifier, and `count`. An archive with no case is not an error:
`cases` is empty, `count` is `0`, and the exit code is `0`.

## `case show`

`openpapir case show --archive <root> <case-id>` reads one case and the
submissions that name it. `data` holds `case`, `submissions` ordered by
identifier, and `submission_count`. An identifier that names no case is
`record.not_found`.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "case.show",
  "data": {
    "case": {
      "archive_schema_version": 1,
      "created_at": "2026-01-14T09:12:33Z",
      "id": "6b73d041fb6bed26be75545fabfd45bc",
      "record_kind": "case",
      "title": "Tax matter"
    },
    "submissions": [
      {
        "archive_schema_version": 1,
        "artefacts": [
          {
            "digest": "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f",
            "role": "cover letter"
          }
        ],
        "case_id": "6b73d041fb6bed26be75545fabfd45bc",
        "created_at": "2026-01-15T10:00:00Z",
        "description": "Posted the completed form.",
        "id": "78957f91825e701adf0d9eecd7f7af50",
        "record_kind": "submission",
        "stated_date": "2026-01-13"
      }
    ],
    "submission_count": 1
  },
  "verified": false
}
```

## `submission add`

`openpapir submission add --archive <root> --case <case-id> --description <d>`
writes one submission record against an existing case. `--date` is optional and
takes `YYYY-MM-DD`. `--artefact` may be repeated; each value is
`sha256:<64 lowercase hex>` or `sha256:<64 lowercase hex>:<role>`. The command
takes the writer lock, then resolves every reference before it writes
anything:

- A case identifier that names no case record is `record.not_found` with
  `record_kind` `case`.
- A digest that names no stored object is `record.not_found` with
  `record_kind` `artefact`. The object's bytes are never opened: only its
  presence, its link state, and the permissions of its fan-out directories are
  checked, exactly as import checks them.
- A value that is not a digest of that form is `usage.arguments`.

`data` holds `submission`, whose shape is the submission entry shown above.

## `receipt add`

`openpapir receipt add --archive <root> --artefact <digest>` records that the
user believes one stored artefact to be a receipt. It takes the writer lock,
runs the archive's permission checks, and resolves every reference before it
writes.

- `--artefact` is `sha256:<64 lowercase hex>` and must name an object already
  stored in this archive. A digest that names no object is `record.not_found`
  with `record_kind` `artefact`; a value of another shape is
  `usage.arguments`. The object's bytes are never opened, and the receipt
  never rewrites them.
- `--import-event` names the import event to record. It must exist and must
  record this artefact: an unknown identifier is `record.not_found` with
  `record_kind` `import_event`, and one recording another artefact is
  `record.inconsistent` with rule `import_event_digest_mismatch`.
- With no `--import-event`, the earliest import event for the digest is
  recorded, by `imported_at` and then by identifier. An artefact may have been
  imported more than once, and that history is meaningful rather than an
  anomaly. An artefact with no readable import event is `record.not_found`.
- `--label` is the user's own single line, at most 200 bytes, omitted from the
  document when absent.

A receipt asserts nothing about the file's type, its origin, or its
authenticity. openPapir parses no artefact bytes, so it records only what the
user said.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "receipt.add",
  "data": {
    "receipt": {
      "archive_schema_version": 1,
      "artefact_digest": "sha256:a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f",
      "created_at": "2026-09-09T13:56:31Z",
      "id": "e08f5afc0f80f190ce229e7cc17c6458",
      "import_event_id": "a5a22fa9e926e98c80d109546e90214d",
      "label": "Envelope",
      "record_kind": "receipt"
    }
  },
  "verified": false
}
```

## `receipt list`

`openpapir receipt list --archive <root>` reads every receipt record. It takes
no lock. `data` holds `receipts`, ordered by identifier, and `count`. An
archive with no receipt is not an error: `receipts` is empty, `count` is `0`,
and the exit code is `0`.

## `association create`

`openpapir association create --archive <root> --receipt <receipt-id>
--outcome <outcome>` records what the user asserts about one receipt. The
outcome is one of `unassociated`, `candidate`, `associated`, and
`contradictory`. All four are results, none is an error, and all four exit
`0`, `contradictory` least of all an error, since retaining conflicting
evidence is the designed behaviour.

`--candidate` may be repeated. Each value is
`<submission-id>:<confidence>:<statement>`, split on the first two colons
only, so a statement may contain a colon. `confidence` is the closed ordinal
set `weak`, `moderate`, `strong` and is never a number, because no calibration
data exists and a number would imply one. The statement is the user's own
single line, at most 512 bytes.

Every evidence entry this build writes carries `kind` `user_assertion` and
`source` `user`, and every record carries `created_by` `user`. Automatic
matching, derived metadata, and receipt parsing do not exist, so no other
value could be recorded honestly.

Records are append-only. `--supersedes` names an earlier association for the
same receipt; the superseded record is never modified, moved, or removed, and
history stays inspectable. Only an absent `--supersedes` records no
supersession. A supplied value is always resolved, so an empty one is
`record.not_found` like any other value that names no association, rather than
a silent no-op.

The consistency rules below are enforced before anything is written. Each
refusal is the additive `record.inconsistent`, whose `details` carry
`record_kind` and `rule` and nothing else:

| Rule | Violation reported |
| --- | --- |
| `unassociated_has_candidates` | `unassociated` was given a candidate. |
| `candidate_requires_candidates` | `candidate` was given no candidate. |
| `associated_requires_one_candidate` | `associated` was not given exactly one candidate. |
| `contradictory_requires_two_candidates` | `contradictory` was given fewer than two candidates. |
| `duplicate_candidate_submission` | One submission was named as a candidate more than once. |
| `supersedes_other_receipt` | The superseded record belongs to another receipt. |
| `import_event_digest_mismatch` | A named import event records another artefact (`receipt.add`). |

`submission_id` is the confirmed submission and equals the single candidate
when the outcome is `associated`. It is `null` for `unassociated`,
`candidate`, and `contradictory`. A receipt, a candidate submission, or a
superseded association that does not exist is `record.not_found`, naming the
kind and how it was referenced and never the value the user supplied. An
outcome, a confidence, or a candidate of another shape is `usage.arguments`.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "association.create",
  "data": {
    "association": {
      "archive_schema_version": 1,
      "candidates": [
        {
          "confidence": "moderate",
          "evidence": [
            {
              "kind": "user_assertion",
              "source": "user",
              "statement": "The reference matches the submission."
            }
          ],
          "submission_id": "ee4bc4254c0852e6bd3cbcc94c1cb221"
        }
      ],
      "created_at": "2026-09-09T13:56:31Z",
      "created_by": "user",
      "id": "eadd68ec590ad23b48831560a2d15e24",
      "outcome": "candidate",
      "receipt_id": "e08f5afc0f80f190ce229e7cc17c6458",
      "record_kind": "association",
      "submission_id": null,
      "supersedes": null
    }
  },
  "verified": false
}
```

## `association list`

`openpapir association list --archive <root> --receipt <receipt-id>` reads one
receipt's whole history. It takes no lock. `data` holds `associations`,
`count`, and `receipt_id`. Superseded records are included and each entry
carries its own `supersedes`, so nothing is collapsed, filtered, or presented
as a single best guess. An identifier that names no receipt is
`record.not_found`.

Newest first means ordered by `created_at`, then by how many records a record
supersedes, then by identifier, all descending. The middle key matters because
openPapir records whole seconds: two records written in the same second would
otherwise order arbitrarily, and a record that supersedes another is by
construction the later of the two.

## Storage guarantees

| Guarantee | How it is kept |
| --- | --- |
| Atomic write | The content is written to a staging file inside the archive root, flushed, linked into place, and the destination directory is flushed. The staging name is then removed. |
| Never overwrite | The publish step is a hard link, which fails rather than replacing an existing file, so a destination openPapir did not create is refused as `path.overwrite`. A filesystem that cannot create a hard link at all, FAT32 and exFAT among them, cannot host an archive and is refused as `platform.filesystem_unsupported` rather than as the retryable `write.interrupted`. |
| Interrupted write | A leftover staging file is never adopted, so the archive holds the complete file or nothing. |
| One filesystem | The root and its layout directories must share one device. A cross-device publish is refused as `path.cross_device`. |
| Owner-only | Directories are created `0o700`, files `0o600`, and stored objects become `0o400`. The root, the marker, the lock file, every layout directory, and each stored object and fan-out directory the operation touches are checked before anything is published; a wider one is refused as `archive.permissions_wide`, naming the archive-relative path. There is no override flag, and nothing is ever narrowed implicitly: an existing path is refused, not repaired. |
| Path safety | Input files are opened with the platform's no-follow flag, `O_NOFOLLOW` on Unix and `FILE_FLAG_OPEN_REPARSE_POINT` on Windows, and no path is stat-ed before it is opened. Symbolic links inside the archive are refused, on Windows together with NTFS junctions and every other reparse point, and a user-supplied filename is never joined into a path. |
| Single writer | A `lock` file recording the holder's process identifier, host, and start time admits one writer. A second writer refuses with `lock.held` rather than waiting. |

## Input caps

Every cap is checked before allocation and enforced again while streaming.
Exceeding one aborts the write and removes the staging file. No flag,
environment variable, or configuration relaxes a cap.

| Cap | Value | Code |
| --- | --- | --- |
| Single file | 64 MiB | `input.cap.file_size` |
| Total bytes per import | 512 MiB | `input.cap.import_bytes` |
| Files per import | 1000 | `input.cap.import_files` |
| Record document | 1 MiB | `input.cap.record_size` |
| Original filename | 255 bytes | `input.cap.filename_length` |
| Case title | 200 bytes | `input.cap.field_length` |
| Case notes | 4096 bytes | `input.cap.field_length` |
| Submission description | 1024 bytes | `input.cap.field_length` |
| Artefact role | 64 bytes | `input.cap.field_length` |
| Receipt label | 200 bytes | `input.cap.field_length` |
| Evidence statement | 512 bytes | `input.cap.field_length` |

`input.cap.record_size` bounds a whole document and reports one cap, the
record cap. A per-field cap has to say which field it refused and which of the
six bounds applied, which that code cannot carry, so the field caps use the
additive `input.cap.field_length` instead. Both are `input` refusals and both
exit `3`.

## Implemented codes and exit codes

The exit code carries the error's bucket and nothing else. `1` is never
emitted.

| Exit | Buckets | Implemented codes |
| --- | --- | --- |
| `0` | Success, including a duplicate import and a warning | |
| `2` | `usage` | `usage.arguments`, `usage.archive_root_missing` |
| `3` | `input`, `path` | the six cap codes above, `path.symlink`, `path.overwrite`, `path.cross_device` |
| `4` | `archive`, `lock`, `write`, `record`, `integrity` | `record.not_found`, `record.malformed`, `record.inconsistent`, `archive.marker_missing`, `archive.marker_malformed`, `archive.adopt_refused`, `archive.schema_newer`, `archive.schema_older`, `archive.permissions_wide`, `archive.multiple_filesystems`, `lock.held`, `write.interrupted`, `integrity.digest_mismatch`, `integrity.length_mismatch`, `integrity.dangling_reference`, `integrity.orphan_object` |
| `5` | `platform` | `platform.filesystem_unsupported`, for a filesystem that cannot create the hard link the publish step needs. The named degradations are warnings, and the owner-only condition of the same code is not detected yet. |
| `6` | `internal` | `internal.unexpected` |

An invocation the argument parser rejects exits `2` as `usage.arguments`.
With `--json` it is one envelope on stdout and nothing on stderr, so a machine
caller reads the same shape it reads for every other refusal; without `--json`
it is the parser's own usage text on stderr and no envelope. `--json` is found
in the raw arguments, because the parse that would have reported the flag is
the one that failed, and a token after `--` is a positional value rather than
the flag. `details.argument` names the flag or value name the parser
complained about, and only when this build defines it: an invented token is
text the user typed and is never echoed. The envelope's `command` is the
subcommand path that was recognised, or `openpapir` when none was.
`--help` and `--version` are not refusals and still exit `0`.

Every other code in [error-contract](error-contract.md) is unimplemented,
including all `export` and `delete` codes, `lock.stale`, `path.traversal`, and
`write.incomplete`.

### Decisions this implementation had to make

The error contract deferred four conditions to an implementing change, listed
first below, and the integrity check decided two more. `record.not_found` is
not one of them: it is a new code, added additively under the contract's
compatibility rule. They are decided as
follows, and no other reserved code became reachable:

1. A marker that cannot be read is `archive.marker_malformed`. It is reported,
   never repaired, and every operation on that archive is refused.
2. Initialising a directory that already holds a marker is `path.overwrite`,
   because the marker would have to be replaced. A non-empty directory with no
   marker stays `archive.adopt_refused`.
3. A lock whose holder is gone is still `lock.held`. No takeover exists, silent
   or explicit, so `lock.stale` stays reserved.
4. A record document that cannot be read as a valid record of its kind is
   `record.malformed`: it is not valid JSON, it is missing a required field,
   it claims another record kind, or it does not name the file it lives in.
   Its `details` carry `record_kind` and `path_count`, the number of documents
   that could not be read, and never the document's path or content. Reading a
   directory of records reports it rather than passing over the document
   silently; the one exception is a staging file, which is openPapir's own
   transient artefact and never a record. A leftover staging file is counted
   as such, so an interrupted write is visible rather than ignored, and it is
   left exactly where it is: removing one is a write, and a reader holds no
   writer lock. It is never a malformed record, because its name is not an
   identifier and it can never be adopted as a record. A stored document is
   untrusted input: it is opened with the platform's no-follow flag, and both
   its kind and its length are taken from that opened handle rather than from
   a separate look at the path, so the file that is checked is the file that
   is read. An entry that is not a regular file, and one larger than the
   record cap, count as unreadable, so a symbolic link in a record directory
   is refused rather than followed and an oversized document is refused before
   its bytes are read. The read is capped as well as checked, so a document
   that grows between the two yields no more than the record cap. Reading one
   record by identifier refuses an oversized document as
   `input.cap.record_size`, the cap that bounds it.
5. A reference that names no record or object is the additive
   `record.not_found`, whose `details` carry the kind that was not found and
   how it was referenced, never the value the user supplied. An identifier
   that is not 32 lowercase hexadecimal characters cannot name a record, so it
   is refused with the same code and is never joined into a path.
6. A record whose fields exist but break a rule of the design is the additive
   `record.inconsistent`, a `record` refusal that exits `4` and is never
   retryable. Its `details` carry `record_kind` and `rule` and nothing else:
   the rule names the broken invariant without echoing an identifier, a
   statement, or a path. The rules are listed under
   [`association create`](#association-create). It is a separate code from
   `record.not_found`, which answers a different question: a reference that
   names nothing, rather than a set of fields that cannot be true together.

7. A stored object that no import event, receipt, or submission references is
   `integrity.orphan_object`, which decides the condition the contract
   reserved: an orphan is a report entry with a count, and it is the last
   problem in the check's precedence rather than a refusal of anything. The
   check never removes one.
8. A record that names a digest, case, submission, receipt, import event, or
   association this archive does not hold is the additive
   `integrity.dangling_reference`, an `integrity` refusal that exits `4` and
   is never retryable. Its `details` carry `record_kind`, `reference_kind`,
   and `path_count`, and nothing else: no identifier, no digest, and no path.
9. An entry under `objects/` whose name is not a digest, or whose name does
   not match the fan-out directories it sits in, is a malformed object entry
   and counts as `integrity.digest_mismatch`, because an object's expected
   digest is its own path and such an entry disagrees with the path it has.
   An object the check could not read, because it exceeds the single-file cap
   or the open failed, is counted in `objects_unchecked` and is never
   reported as damaged: the check did not read the bytes to say so. A record
   directory the check could not list is counted in `records_unchecked` for
   the same reason, and suppresses the orphan and dangling judgements the
   records it may hold would have settled.
10. `archive check` is the one command whose `data` is not `{}` when `ok` is
    `false`. Its stated work is to produce the report, which it completed, so
    the counts stay in `data` and the error names the first of them. Every
    other command keeps the contract's original rule.

Permissions are never repaired as a side effect. `archive init` narrows the
supplied root once, deliberately, as part of creating the archive; after that
every wider path is refused. The explicit repair action the design describes,
which only narrows and reports every path it changed, is not implemented.

An input that is a symbolic link is refused with `path.symlink` carrying
`scope` `input`. The path is outside the archive, so it has no
archive-relative form and no `archive_path` is reported; the path itself is
user-supplied and is never echoed. The refusal comes from the no-follow open
itself, not from a check that precedes it, so nothing is stat-ed first.

An object's path that holds something openPapir did not create is checked for
its shape before its permissions. A directory at an object's path is
`path.overwrite`, not `archive.permissions_wide`, however wide it is: it is
not an object whose permissions the archive could have set, so naming its
permissions would describe the wrong problem.

An import-event record that cannot be parsed is skipped when counting a
duplicate's history rather than reported, so `previous_import_count` is a count
of the readable events. Import keeps that behaviour: `record.malformed` names
the case and submission documents a command must read to do its work, and an
unreadable import event does not stop bytes from being stored.

## Platform degradation

Where a guarantee weakens, the condition is reported as a warning in the
envelope and never silently accepted. `warnings` never changes `ok` and never
changes the exit code.

| Code | Condition |
| --- | --- |
| `platform.no_directory_fsync` | The directory entry a publish created may not be durable, although the file content was flushed. Emitted where the platform has no directory flush, and also where the flush was attempted and failed, with `stage`. |
| `platform.owner_only_via_acl` | Owner-only access is an access-control list rather than a permission bit, so it depends on the filesystem. Emitted on Windows. |
| `platform.no_follow_after_open` | The no-follow flag opens the link itself rather than failing, so the refusal comes from the handle openPapir opened, and the reparse tag is not distinguished. Emitted on Windows, once per archive opened. |

On Windows a no-follow open carries `FILE_FLAG_OPEN_REPARSE_POINT`, so the
reparse point is opened and never its target, and the handle is then refused
when the file it names is one. No path is stat-ed before it is opened, so the
time-of-check-to-time-of-use gap a preceding check would leave does not exist.
What remains weaker is named above and reported: the refusal is of the opened
link rather than of the open, and openPapir refuses every reparse point as
`path.symlink` without saying whether it was a symbolic link, an NTFS
junction, or another tag. Refusing all of them is deliberate: the archive
creates no reparse point of its own, so one it finds inside the archive is
something it did not create.

## Privacy of output

`message`, `details`, `data`, human-readable output, and stderr may carry
counts, byte lengths, cap values, input positions, bucket names, error codes,
archive-relative paths, digests of stored artefacts, identifiers openPapir
minted, and timestamps openPapir recorded.

They never carry an original filename or any form of one, a user-supplied path
including the archive root, payload bytes or excerpts, a hostname, a username,
or a process owner. A receipt label and an evidence statement are the user's
own text: they appear in `data` and in human output, which report the user's
own record back to them, and never in a `message`, in `details`, or in any
refusal. Which input failed is answered by `input_index`, never by
a name. The original filename is stored as an attribute of the import event
record only.

## Planned ownership

Cases, submissions, receipts, and user-asserted associations are implemented
as described above. openPapir will also own the wider user workflow. The
storage technology and the on-disk layout are decided in
[local archive layout and storage design](archive-layout.md); the parts of it
not listed above are not implemented, and the record shapes it fixes for
unimplemented kinds are not a promised schema.

openKRX will own KRX container reading, creation, structural validation, and
safe extraction. openSzigno owns `.es3` dossier operations and their signature
verification. The workspace has no dependency on either sibling project.
Choose an integration boundary only after their needed contracts are available;
do not duplicate format implementations here.

Receipt states must remain independently expressible:

| State | Meaning |
| --- | --- |
| Imported | Bytes were accepted into the local archive. |
| Matched | Evidence associates the receipt with a submission. |
| Authenticity verified | A specified cryptographic check passed in context. |

The first two are implemented, and each asserts nothing beyond itself. The
integrity check re-digests stored bytes, which is a storage-layer identity
check and never the third state. An
association is the user's own statement: openPapir reads no artefact bytes, so
automatic matching and derived metadata remain design requirements with no
code behind them. Verification has no code behind it at all. An association
cannot imply authenticity, successful delivery, or legal effect. Delegated
verification must identify the attachment or receipt covered, the verifier, and
its trust context.

## Integration boundary

No government submission API is assumed. KRX creation alone establishes no
ability to submit a package to e-Papír. Automatic sending, authentication,
background services, and a web interface are outside the initial foundation.
An integration needs separate discovery of authorised access, actual contracts,
and recovery semantics before a scoped implementation proposal.
