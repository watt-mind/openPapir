# Architecture and CLI contract

## Documentation map

This document is the canonical contract of what openPapir actually does. For
the wider picture, purpose, scope, non-goals, which designs are decided but
not built, and which contracts are still blocked, start at the
[specification index](specification.md). Every document has a one-line purpose
in the [documentation index](index.md). Where this document and
[local archive layout and storage design](archive-layout.md) or
[import and association error, JSON, and exit-code contract](error-contract.md)
disagree, this document is authoritative for implemented behaviour and those
two are the design and the wire specification behind it.

## Current implementation

The Rust edition 2024 workspace has an MSRV of 1.88 and two unpublished crates:

| Crate | Current responsibility |
| --- | --- |
| `openpapir-core` | The local archive: marker, artefact store, atomic writes, single-writer lock, input caps, path safety, import-event records, the case, submission, receipt, and association records, the read-only whole-archive integrity check, the export of one case, the permission repair, and the deletion of one case with its explicit purge. |
| `openpapir-cli` | Argument parsing, the response envelope, and the exit-code mapping. |

Only these invocations are supported:

```sh
openpapir --help
openpapir --version
openpapir capabilities [--json]
openpapir archive init <root> [--json]
openpapir import --archive <root> <file>... [--json]
openpapir case create --archive <root> --title <t> [--notes <n>] [--json]
openpapir case list --archive <root> [--json]
openpapir case show --archive <root> <case-id> [--json]
openpapir submission add --archive <root> --case <case-id> --description <d> [--date <yyyy-mm-dd>] [--artefact <digest>[:<role>]]... [--json]
openpapir receipt add --archive <root> --artefact <digest> [--import-event <id>] [--label <l>] [--json]
openpapir receipt list --archive <root> [--json]
openpapir association create --archive <root> --receipt <receipt-id> --outcome <outcome> [--candidate <submission-id>:<confidence>:<statement>]... [--supersedes <association-id>] [--json]
openpapir association list --archive <root> --receipt <receipt-id> [--json]
openpapir archive check --archive <root> [--json]
openpapir case export --archive <root> --case <case-id> --to <dir> [--json]
openpapir archive repair-permissions --archive <root> [--json]
openpapir case delete --archive <root> --case <case-id> [--purge] [--json]
openpapir skill
```

The sections below take each implemented command in that order, and each
states its invocation, the shape of its `data`, the codes particular to it,
and how the privacy rule binds its output.

Some refusals belong to no one command. Any invocation the argument parser
rejects, and any flag value this build cannot use, is `usage.arguments`, and
any violated invariant is `internal.unexpected`. Every command that opens an
existing archive can refuse with `usage.archive_root_missing`,
`archive.marker_missing`, `archive.marker_malformed`, `archive.schema_newer`,
`archive.schema_older`, `archive.multiple_filesystems`,
`archive.permissions_wide`, or `path.symlink` for a linked layout directory;
`archive check` and `case export` open the archive read-only, which creates no
layout directory and flushes nothing, but runs the same checks.
Every command that writes adds `lock.held`, `path.overwrite`,
`path.cross_device`, `write.interrupted`, `input.cap.record_size`, and
`platform.filesystem_unsupported`. Every command that reads a stored document
can report `record.malformed`. The per-command tables and lists below name
only what is particular to that command, and the whole set with its exit codes
is in [Implemented codes and exit codes](#implemented-codes-and-exit-codes).
`skill` is the exception: it opens no archive, reads no input, and can refuse
only with `usage.arguments` or `internal.unexpected`.

Fifteen operations are implemented, `archive.init`, `import`, `case.create`,
`case.list`, `case.show`, `submission.add`, `receipt.add`, `receipt.list`,
`association.create`, `association.list`, `archive.check`, `case.export`,
`archive.repair_permissions`, `case.delete`, and `skill`, and those are the
fifteen names `capabilities` reports. Everything else in
[local archive layout and storage design](archive-layout.md) and
[import error, JSON, and exit-code contract](error-contract.md) remains a
design: no derived-metadata or verification records; no automatic matching, no
receipt parsing, no import from an export, and no editing of a stored record,
deletion of a single submission or receipt, deletion of an archive, or
migration. openPapir reads artefact bytes only to re-digest a stored object
during the integrity check and to copy one out during an export, and never to
form an opinion about what an artefact says, so an association is only ever
the user's own assertion.

## The response envelope

Every command prints exactly one JSON object on stdout in its `--json` form,
on one line, and nothing else. Human-readable output goes to stdout only in the
non-`--json` form; warnings and errors go to stderr there, so diagnostic text
never shares stdout with the JSON object.

| Key | Meaning |
| --- | --- |
| `schema_version` | The envelope's version, currently `1`, independent of `archive_schema_version`. |
| `ok` | `true` only when the command completed its stated work. |
| `command` | The invoked command's stable name: `capabilities`, or one of the fifteen operation names `capabilities` reports. An invocation the argument parser rejected before it recognised a subcommand carries `openpapir` instead. |
| `data` | The command's result. `{}` when `ok` is `false`, except `archive check`, whose report is the result the user asked for and stays in `data` beside the error. |
| `verified` | Always `false`. No cryptographic check is implemented. |
| `error` | Present exactly when `ok` is `false`: `code`, `message`, `details`. |
| `warnings` | Present only when a platform degradation was observed. |

`code` is the only field a caller may branch on. `message` wording may change
within a `schema_version`. `details` carries at most 16 keys, whose values are
strings, integers, booleans, or arrays of at most 16 such scalars, and always
carries `bucket`. Changes within `schema_version` are additive only.

The capabilities response is unchanged in shape and lists the fifteen
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
      "archive.check",
      "case.export",
      "archive.repair_permissions",
      "case.delete",
      "skill"
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

A directory that is not empty and holds no marker is `archive.adopt_refused`,
and one that already holds a marker is `path.overwrite`, because the marker
would have to be replaced. `data` carries the archive's own minted identifier
and its schema version, and never the root the user supplied.

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

Beyond the shared refusals, import can emit `input.cap.file_size`,
`input.cap.import_bytes`, `input.cap.import_files`,
`input.cap.filename_length`, `path.symlink` with `scope` `input` for an input
that is a symbolic link, and `integrity.length_mismatch`. `data` carries
digests, byte lengths, and identifiers openPapir minted, and never an input
path or an original filename; which input failed is answered by
`input_index`.

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
into owner-only directories. Opening an archive for writing creates any layout
directory this build expects and an earlier one did not, so an archive created
by an earlier build gains the record directories it lacks; nothing else
changes. Opening an archive read-only creates nothing, and an absent layout
directory reads as empty there.

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
first. An empty or oversized `--title` or `--notes` is refused before the
archive is touched, as `usage.arguments` or `input.cap.field_length`. `data`
holds `case`, the document just written, so it carries the user's own title
and notes and nothing else.

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
`cases` is empty, `count` is `0`, and the exit code is `0`. Beyond the shared
refusals it emits nothing of its own. `data` is the user's own records read
back to them, so it carries the titles and notes they typed, and no path.

## `case show`

`openpapir case show --archive <root> <case-id>` reads one case and the
submissions that name it. `data` holds `case`, `submissions` ordered by
identifier, and `submission_count`. An identifier that names no case, and one
that is not 32 lowercase hexadecimal characters, are both `record.not_found`,
naming the kind and how it was referenced and never the value the user
supplied. `data` is the user's own records read back to them, and carries no
path.

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

`data` holds `submission`, whose shape is the submission entry shown under
[`case show`](#case-show). It carries the user's own description, stated date,
and artefact roles, plus the digests of stored objects, and never a path. An
empty or oversized `--description`, `--artefact` role, or `--date` that is not
a calendar date is `usage.arguments` or `input.cap.field_length`, naming the
argument and never its value.

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
and the exit code is `0`. Beyond the shared refusals it emits nothing of its
own, and `data` carries the user's own labels and the digests of stored
objects, never a path or an original filename.

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

Beyond `record.not_found` and the shared refusals it emits nothing of its own.
`data` carries the user's own statements and openPapir's own identifiers, and
never a path.

Newest first means ordered by `created_at`, then by how many records a record
supersedes, then by identifier, all descending. The middle key matters because
openPapir records whole seconds: two records written in the same second would
otherwise order arbitrarily, and a record that supersedes another is by
construction the later of the two.

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
    "records_staging_files": 0,
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
`staging_files` counts what `objects/incoming/` still holds, and
`records_staging_files` counts the leftover staging files the `records/<kind>`
directories hold together. A staging file is openPapir's own transient
artefact from an interrupted write: it is never a record and never a malformed
one, no code is reported for it, it does not affect the exit code, and the
check leaves it exactly where it is, because removing it is a write.

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

## `case export`

`openpapir case export --archive <root> --case <case-id> --to <dir>` copies
one case out of the archive. It is a plain copy: the objects are the original
bytes, the records are readable JSON, and the result is usable without
openPapir ([archive-layout](archive-layout.md)). Nothing is converted,
re-encoded, normalised, compressed, or encrypted, and no hard link is made, so
the destination may be on any filesystem.

The archive is opened read-only, exactly as `archive check` opens it. No lock
is taken, no layout directory is created, and nothing inside the root is
written, renamed, or removed. Reading a case therefore never modifies it.

The destination holds:

| Path | Content |
| --- | --- |
| `objects/<digest>` | One copied object, named by its lowercase hexadecimal digest and nothing else. |
| `records/<kind>/<id>.json` | One record document, exactly as the archive stores it. `<kind>` is `case`, `submission`, `receipt`, `association`, or `import_event`. |
| `manifest.json` | One JSON document with sorted keys listing every copied object and every written record. |

What belongs to the case is fixed: the case record; every submission recorded
against it; every association naming one of those submissions, as the
confirmed submission or as a candidate, superseded records included; every
receipt those associations name; and every import event that introduced one of
the artefacts those records reference. Objects are the artefacts the
submissions and receipts reference. An association is the user's own
statement, so a receipt reaches an export only because the user tied it to the
case themselves.

The destination must be an existing empty directory or one the export creates,
and it is never inside the archive root. Both paths are resolved before they
are compared, and a path that cannot be resolved at all is refused rather than
let through, because the question the check answers is whether the export is
about to write inside the archive. The path rules apply outward: a symbolic
link in the destination is refused rather than followed, and a file already at
a target path is refused rather than replaced.

The outward no-follow rule is weaker than the archive-side one, and the
difference is stated rather than hidden. Inside the archive every path is
reached through the platform's no-follow open, so nothing is stat-ed before it
is opened and no link can be substituted between the check and the open.
Outside it the rule has two halves. Each leaf file is created with create-new
semantics, which the system call itself refuses on an existing path, a
symbolic link included, so the leaf needs no separate check. Each directory
component openPapir would make, `objects` and `records/<kind>`, is tested with
a no-follow stat first and refused when it is a link, which is a check
followed by a use rather than one no-follow call.

The residual difference is a substitution between that check and the create.
It is accepted for three reasons. The destination is outside the archive, so
nothing an attacker gains there reaches stored bytes. The destination is
required to be empty or to be created by the export, so a link that appears in
it appeared after the user pointed the export at it. And the leaf create still
refuses to replace anything, so the worst a race achieves is a new file under
a directory the user's own filesystem redirected, never an overwrite. An
openat-style walk that holds a directory handle for every component would
close that gap, and is not implemented; it would need a per-platform
implementation for a destination the archive does not own.

An interrupted write in the destination reports the stage it was in:
`object_write` while copying a stored object or making the directory the
copies go in, `record_write` while writing a record document or its
directory, and `marker_write` for the destination itself and for
`manifest.json`, which describe the export rather than any one record
([error contract](error-contract.md)).

The archive's own publish step, a hard link into place, is deliberately not
used outside the root. A destination may be a filesystem that cannot create a
hard link at all, and the design requires an export to work on any of them, so
each file is created at its final path with create-new semantics instead,
which refuses an existing path just as firmly. The cost is that an interrupted
export could leave a partial file, so an export that fails removes exactly
what it created, files before the directories that hold them, and never a path
that was already there. A destination the export itself created is removed
again; one the user made is left in place and empty. A retry therefore meets
the destination the first attempt met. Removal is best effort: the export is
already reporting a refusal of its own, and a destination that cannot be
tidied is not a second one. Directory entries in the destination are not
flushed, and the archive's durability guarantee does not extend outside the
root.

Every copy is streamed in 64 KiB chunks and digested as it is written, then
compared with the digest its source path names. A copy that differs is
`export.copy_mismatch` and its partial file is removed, so the destination
never holds a file whose name does not describe its content. The comparison is
a storage-layer identity check and never a cryptographic verification, so
`verified` stays `false`.

| Condition | Code |
| --- | --- |
| The case identifier names no case, or a record names an object the archive does not hold. | `record.not_found` |
| A stored document cannot be read as a record of its kind. | `record.malformed` |
| The destination lies inside the archive root, or is not a usable directory. | `usage.arguments` |
| The destination, or a path in it, is a symbolic link. | `path.symlink` |
| The destination is not empty, or a target path is already there. | `export.destination_conflict` |
| A copy re-digested to something other than its source. | `export.copy_mismatch` |
| A stored object exceeds the single-file cap. | `input.cap.file_size` |
| A copy could not be read or written. | `write.interrupted` |
| The archive root could not be resolved, so the destination could not be checked against it. | `usage.arguments` |

The manifest is authoritative for what the export contains. Its keys are
sorted, it records the archive's schema version, its own format version, the
case, and the time openPapir wrote it, and it lists nothing the destination
does not hold.

```json
{
  "archive_schema_version": 1,
  "case_id": "e19fb6367693c26aadf609565ec6b8d8",
  "exported_at": "2026-09-09T15:15:49Z",
  "objects": [
    {
      "algorithm": "sha256",
      "byte_length": 14,
      "digest": "aa8c28cf0c0bbf1af78fc8613d8e052dd42475fe937af1016f2f6067f127b37f"
    }
  ],
  "records": [
    { "id": "e19fb6367693c26aadf609565ec6b8d8", "kind": "case" }
  ],
  "schema_version": 1
}
```

An exported object is named by its digest alone. No original filename is a
file name, a directory name, or a manifest field: it stays where it has always
been, an attribute inside the exported import-event record that the user's own
import wrote.

`data` reports counts and the case, and never the destination.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "case.export",
  "data": {
    "bytes_copied": 34,
    "case_id": "e19fb6367693c26aadf609565ec6b8d8",
    "object_count": 3,
    "record_count": 8,
    "records": [
      { "count": 1, "kind": "case" },
      { "count": 2, "kind": "submission" },
      { "count": 1, "kind": "receipt" },
      { "count": 1, "kind": "association" },
      { "count": 3, "kind": "import_event" }
    ]
  },
  "verified": false
}
```

Human output adds one thing the JSON does not carry, the destination the user
supplied, because the line repeats the argument they just typed.

```text
Exported case e19fb6367693c26aadf609565ec6b8d8 to /tmp/example-export.
Copied 3 object(s), 34 byte(s), and wrote 8 record(s).
case 1
submission 2
receipt 1
association 1
import_event 3
The archive was not changed. Every copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

Importing an export back into an archive is not implemented, and neither is
exporting a whole archive: a backup is a plain copy of the archive root taken
while no openPapir process holds the lock, and `archive repair-permissions` is
what makes a restored copy usable again.

## `archive repair-permissions`

`openpapir archive repair-permissions --archive <root>` narrows every path in
the archive back to the owner-only modes of the design. It is the only action
besides `archive init` that changes permissions, and it exists because
ordinary copy tooling widens them when a backup or an export is restored,
which the owner-only rule would then refuse
([archive-layout](archive-layout.md)).

It only ever narrows. The new mode is the old mode with every group bit, every
other bit, and every set-user, set-group, and sticky bit cleared, and for a
stored object with owner write cleared as well, so a path can lose access and
never gain it. A path that is already narrower than the design's mode, an
unreadable file the user closed deliberately for instance, is left exactly as
it is rather than raised to `0o600`.

| Path | Mode it is narrowed to |
| --- | --- |
| The root, every layout directory, and every object fan-out directory | `0o700` |
| The marker and every record document | `0o600` |
| Every stored object and every leftover staging file | `0o400` for a stored object, `0o600` for a staging file |

The lock file is deliberately not inspected. The repair holds the writer lock
while it runs, so the only lock file that can exist while it walks is the one
it created itself, owner-only by construction, and a lock file another writer
left refuses the repair with `lock.held` before the walk begins. A `lock`
count would always be zero, so the report does not carry one: it names only
what the repair actually looked at.

The repair refuses a root with no marker, so it never adopts a directory, and
it refuses an archive whose schema version this build does not support. It
takes the writer lock, because it writes modes, and refuses with `lock.held`
when another writer holds it. It deliberately does not run the archive's
permission check first: the wide permissions that check refuses are exactly
what it repairs. It reads no file content, and it changes no byte of any
record or object.

A symbolic link inside the archive is refused as `path.symlink` rather than
narrowed. Changing a link's permissions changes its target's, and a link out
of the archive would be a file the archive does not own.

A path the repair cannot read or cannot narrow is `write.interrupted`, and its
`stage` is the kind of path the repair was inspecting: `object_write` for a
stored object, a fan-out directory it cannot list, or a leftover staging
file, `marker_write` for the marker, and `record_write` for a record
document, a cached file, a layout directory, a fan-out directory it cannot
narrow, or the root ([error contract](error-contract.md)).

`data` reports counts by kind, in a fixed order, including the kinds nothing
changed for, so a caller reads a count rather than testing for a key.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "archive.repair_permissions",
  "data": {
    "changed": [
      { "count": 0, "kind": "cache" },
      { "count": 0, "kind": "directory" },
      { "count": 0, "kind": "marker" },
      { "count": 0, "kind": "object" },
      { "count": 0, "kind": "record" },
      { "count": 0, "kind": "root" }
    ],
    "paths_changed": 0,
    "paths_checked": 16
  },
  "verified": false
}
```

Human output prints the same counts and no path.

```text
Narrowed 0 of 16 archive path(s) to owner-only.
cache 0
directory 0
marker 0
object 0
record 0
root 0
Permissions are only ever narrowed here; nothing was widened and no content was read or changed.
```

On a platform without permission bits nothing is changed and every count is
`0`. Owner-only access there is an access-control list, which the envelope
reports as the `platform.owner_only_via_acl` warning, exactly as every other
command reports it.

## `case delete`

`openpapir case delete --archive <root> --case <case-id> [--purge] [--json]`
removes one case. It is the only destructive invocation openPapir has, and it
is the only one that can remove an object, which it does only when `--purge`
says so in as many words. Without `--purge` no object is touched at all, and
the objects that would become unreferenced are counted and reported as
retained instead.

What goes, and why:

| Record | Rule |
| --- | --- |
| The case | The named case, always. |
| Submissions | Every submission recorded against that case. |
| Associations | An association goes when every submission it names is going and it names at least one. One naming no submission at all stays. An association a remaining association supersedes is kept, because removing it would leave the newer record naming a record the archive no longer holds. An association naming submissions in this case **and** in another is refused rather than resolved; see below. |
| Receipts | A receipt is its own record. It goes when an association tied it to a submission that is going and no remaining association still names it. A receipt no association names is not tied to this case and stays. |
| Import events | History, and kept. The one exception is an import event naming an object `--purge` removed: it goes with that object, because an event describing content that is gone describes nothing. An object the purge could not unlink keeps its import event, so it stays a referenced object rather than becoming an orphan. |
| Objects | Only with `--purge`, and only an object no remaining import event, receipt, or submission references. |

The whole archive is read first, under the writer lock, and the removal set is
decided before a single file is unlinked. A record document that cannot be
read as a record of its kind therefore aborts the deletion with
`record.malformed` while the archive is still exactly as it was. An unknown
case identifier is `record.not_found`, and one that is not 32 lowercase
hexadecimal characters is refused with the same code and never joined into a
path.

A record the deletion **keeps** may itself name an artefact by something that
is not a digest at all. openPapir validates a digest on every write, so no
command writes such a record, but a document edited outside openPapir can
hold one, and the plan must read it as a reference to an object it cannot
identify rather than as a reference to nothing. The deletion therefore cannot
tell which object that record meant, so it purges none of them: every object
the purge had considered is retained with the reason `referenced_elsewhere`,
with or without `--purge`, and the number of such references is reported as a
`record.malformed` warning carrying `stage` and `malformed_count`. The
records the deletion planned to remove still go, `ok` stays `true`, and the
same reference on a record that is **going** changes nothing, because that
record and its claim both leave. The count is the whole of the warning: the
record that holds the reference is not named, and neither is the value.

An association may name submissions in more than one case. When one of them is
going and another remains, the association has to stay, because it still
references a submission this deletion leaves behind, and it would then name a
submission the archive no longer holds. openPapir edits no stored record, so
it can neither drop the departing candidate nor invent a shorter record, and
removing the association would delete the user's own assertion about a case
they did not ask to delete. The deletion is refused instead, with
`delete.record_entangled`, before anything is touched. The refusal is
symmetric: until the user resolves the association themselves, neither case
can be deleted. It is the only one of the three possible outcomes that loses
nothing and can be undone.

The record pass is **all or nothing per case**. Before a single document is
unlinked, every record directory the deletion would remove an entry from is
probed: it is opened without following a link, and the mode of the opened
handle must allow the owner to write and to search it. Each planned document
is then looked at in turn and must be either already absent or a regular
file. The probe attempts nothing destructive: nothing is created, moved, or
removed, and no permission is changed. If any part of it says a document will
not go, the deletion is refused with `delete.records_retained` before the
first unlink, and the archive is exactly as it was. The directories probed
are the ones the plan touches: associations, receipts, submissions, the case,
and, when a purge would remove them, the import events naming the objects
going with it.

The rule exists because a record pass that stops part way through cannot be
resumed. The documents it did remove are gone, so the next run plans a
smaller deletion, and an object whose last referencing record went in the
interrupted pass is no longer a purge candidate at all: it stays in the store
with the import event that describes it, `archive check` calls the archive
clean, and nothing tells the user that a purge they asked for did not happen.
Refusing before anything goes is the only outcome the user can undo by
clearing the cause and running the same command again.

The probe is a check that precedes a use, so the window between them is a
time-of-check-to-time-of-use gap, and openPapir says so rather than claiming
more than it can. A mode changed, a filesystem remounted read only, or a
quota reached after the probe and before the unlink still refuses. The probe
reads permission bits rather than asking the kernel whether this process may
write, so a refusal expressed as an access-control list, an immutable flag,
or a mandatory lock is not seen by it either; on Windows, where the
permission is an access-control list rather than a mode, the probe confirms
only that each directory is a directory openPapir created rather than a
reparse point. The writer lock is held across both the probe and the pass, so
no other openPapir writer moves in between; nothing outside openPapir is
under its control. The probe narrows the window that stranded a purge
candidate. It does not close it.

What still holds when the probe is wrong is the older rule: the record pass
stops at the first unlink the filesystem refuses, and takes the object pass
with it. The kinds go in the order of the references between them,
associations, receipts, submissions, and then the case, so each kind goes only
once everything that could name it has gone; carrying on past a refusal would
remove a record something still there names, and purging afterwards would
remove bytes a surviving record still names. Both are dangling references, so
neither is attempted. The deletion keeps what it had already removed, touches
no object at all, and reports `delete.records_retained` with the number of
documents it planned to remove and did not, the ones it never reached
included. The import events that would have gone with the purged objects are
among them: they are documents the deletion planned to remove, and a pass
that never reached the object stage removed none of them. The count is the
same after a probe refusal, where that is every document the deletion planned
to remove.

Every removal is the unlink of one file openPapir created: a record document
under `records/`, or an object at its own fan-out path under
`objects/sha256/`. No directory is removed, nothing is removed recursively,
and nothing outside those two trees is touched. Records go first, so an object
is only ever unlinked once nothing in the archive names it. The object's own
mode is not changed: unlinking needs the permission of the directory holding
it, which is owner-only and enough. Nothing is ever widened.

`data` carries counts, record kinds, and the reason an object stayed, and
never a digest, a path, or a filename: recording the fingerprint of content
the user asked to purge would defeat the purge, which is also why no deletion
record is written and no audit log is kept
([archive-layout](archive-layout.md)).

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "case.delete",
  "data": {
    "objects_removed": 1,
    "objects_retained": [
      { "reason": "purge_not_requested", "count": 0 },
      { "reason": "records_retained", "count": 0 },
      { "reason": "referenced_elsewhere", "count": 1 },
      { "reason": "unremovable", "count": 0 }
    ],
    "objects_retained_total": 1,
    "purge": true,
    "records_removed": [
      { "kind": "association", "count": 0 },
      { "kind": "case", "count": 1 },
      { "kind": "import_event", "count": 1 },
      { "kind": "receipt", "count": 0 },
      { "kind": "submission", "count": 2 }
    ],
    "records_removed_total": 4,
    "records_retained": 0
  },
  "verified": false
}
```

`records_removed` lists every record kind and `objects_retained` every reason,
including the ones with nothing to report, ordered by name, so a caller reads
a count rather than testing for a key. The four reasons are fixed:

| Reason | Meaning |
| --- | --- |
| `purge_not_requested` | The object would have become unreferenced, and `--purge` was not given. |
| `referenced_elsewhere` | A submission or receipt that remains still references the object, or references an artefact by something that is not a digest, which may be any of them. |
| `records_retained` | A record document would not go, so the object pass never ran and none of these objects was attempted. The record pass is all or nothing, so this normally means nothing at all was unlinked. |
| `unremovable` | `--purge` was given and the unlink did not succeed. |

A `records_retained` or `unremovable` count above zero makes `case.delete` the
second command whose `data` survives a failure: the deletion did the rest of
its stated work, so the counts stay in `data`, `ok` is `false`, and the exit
code is `4`. `error` is `delete.records_retained` when a record document would
not go, which is reported first because it is the reason nothing was purged,
and `delete.objects_retained` when only the purge fell short. Every other
refusal, `delete.record_entangled` included, empties `data` as usual.

Human output prints the same counts and no path.

```text
Removed 4 record(s): association 0, case 1, import_event 1, receipt 0, submission 2.
Removed 1 object(s); 1 retained: purge_not_requested 0, records_retained 0, referenced_elsewhere 1, unremovable 0.
A purge was requested: an object is unlinked only when no remaining import event, receipt, or submission references it.
Deletion unlinked files in this archive. It does not erase data from the storage medium, and any backup already taken is outside openPapir's reach.
```

An archive `archive check` found clean stays clean after a deletion, with or
without a purge, and in every refusal above. Nothing that remains names a
record or object that went: the entangled association is refused rather than
orphaned, a refused record unlink stops the purge before it starts, and an
object that could not be unlinked keeps its import event. A purge leaves no
orphan behind.

## `skill`

`openpapir skill` writes the agent skill document the binary carries to
stdout, byte for byte, and nothing else. It takes no file, no `--archive`, and
no `--json`: the document is the whole output, so there is no envelope to
render and no result to report in two forms.

A stdout a pager or `head` closed is not a failure of the command: only
`BrokenPipe` is swallowed, and the run still exits `0`, exactly as a reader
that stopped reading intended. Every other write or flush failure is reported.
The documented install path is a redirection, `openpapir skill > SKILL.md`, so
a destination that cannot take the bytes, a full disk above all, would
otherwise leave a truncated document behind and still exit `0`. It exits `4`
instead, the `write` bucket's code, with one line on stderr saying the
document could not be written. That line names no path and repeats no
argument, because the destination is the caller's own redirection and
openPapir never echoes one back
([privacy of output](#privacy-of-output)). There is no envelope in either
case: `skill` is outside it by construction.

The document is embedded with `include_str!` from
`crates/openpapir-cli/skills/openpapir/SKILL.md`, so the bytes the binary
writes and the bytes this repository holds are the same bytes, and installing
the skill needs no checkout. It is the agent-facing description of everything
above: when to reach for openPapir, each command's exact invocation and the
`data` fields to read, the envelope, the exit codes by bucket, the privacy
rule, the caps, and the separation of imported, matched, and
authenticity-verified. It states no behaviour this document does not state as
implemented.

`skill` is the one operation `capabilities` reports that touches no archive.
It is reported there so that a machine caller learns of it from the same list
as every other operation.

```console
$ openpapir skill | head -3
---
name: openpapir
description: >-
```

## Storage guarantees

| Guarantee | How it is kept |
| --- | --- |
| Atomic write | The content is written to a staging file inside the archive root, flushed, linked into place, and the destination directory is flushed. The staging name is then removed. |
| Never overwrite | The publish step is a hard link, which fails rather than replacing an existing file, so a destination openPapir did not create is refused as `path.overwrite`. A filesystem that cannot create a hard link at all, FAT32 and exFAT among them, cannot host an archive and is refused as `platform.filesystem_unsupported` rather than as the retryable `write.interrupted`. |
| Interrupted write | A leftover staging file is never adopted, so the archive holds the complete file or nothing. |
| One filesystem | The root and its layout directories must share one device. A cross-device publish is refused as `path.cross_device`. |
| Owner-only | Directories are created `0o700`, files `0o600`, and stored objects become `0o400`. The root, the marker, the lock file, every layout directory, and each stored object and fan-out directory the operation touches are checked before anything is published; a wider one is refused as `archive.permissions_wide`, naming the archive-relative path. There is no override flag, and nothing is ever narrowed implicitly: an existing path is refused, not repaired. `archive repair-permissions` is the one explicit action that narrows an existing archive, and it never widens. |
| Copies outward | An export writes only into a destination outside the archive root, creates every file there with create-new semantics, follows no symbolic link, replaces nothing, and re-digests every copy before it is published. A destination the export itself created is removed again when the export fails. |
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
| `4` | `archive`, `lock`, `write`, `record`, `integrity`, `export`, `delete` | `record.not_found`, `record.malformed`, `record.inconsistent`, `archive.marker_missing`, `archive.marker_malformed`, `archive.adopt_refused`, `archive.schema_newer`, `archive.schema_older`, `archive.permissions_wide`, `archive.multiple_filesystems`, `lock.held`, `write.interrupted`, `integrity.digest_mismatch`, `integrity.length_mismatch`, `integrity.dangling_reference`, `integrity.orphan_object`, `export.destination_conflict`, `export.copy_mismatch`, `delete.objects_retained`, `delete.records_retained`, `delete.record_entangled` |
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
including `lock.stale`, `path.traversal`, and `write.incomplete`.

### Decisions this implementation had to make

The error contract deferred four of its reserved conditions to an implementing
change, and the commands below decided them and added nine further codes and
rules additively under the contract's compatibility rule. All thirteen are
decided as follows; `lock.stale`, `path.traversal`, and `write.incomplete`
stay reserved and unreachable:

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

11. A purge that could not unlink an object reports
    `delete.objects_retained`, a `delete` refusal that exits `4` and is never
    retryable. Its `details` carry `retained_count` and the additive `reason`,
    which is `unremovable`, and nothing else. The contract's
    `referencing_record_ids` is deliberately not emitted: the objects this
    code names are the ones the filesystem would not release, not ones a
    record still points at, and an object a remaining record references is
    reported in `data` as `referenced_elsewhere` rather than as a refusal.
    `platform.replace_while_open`, which the contract describes as an error,
    is emitted by `case delete` as a warning, because a deferred unlink stops
    that one object rather than the operation, which completes and reports
    what it could not remove. Its `details` carry the **additive**
    `read_only_restored` only where the read-only attribute was actually
    cleared: where the platform needs it cleared before an unlink, it is put
    back when the unlink still fails, and the flag says whether putting it
    back succeeded. Where reading the permissions or clearing the attribute
    failed, the file is exactly as it was and there is nothing to put back,
    so the flag is **absent** rather than `true`. A caller reads its absence
    as "the attribute was never cleared, and nothing was widened", which is a
    different statement from `true` and must not be confused with it. A
    repeated warning is reported once and carries the worst outcome any
    object saw, rather than the first, so one object left writable is never
    hidden by another that was restored, and a warning carrying no flag at
    all never displaces one that reports `false`.
12. A record document the filesystem refuses to unlink is the additive
    `delete.records_retained`, a `delete` refusal that exits `4` and is never
    retryable. Its `details` carry `retained_count` and nothing else. It
    stops the object pass entirely rather than purging around the record that
    stayed, because a record that is still there still names its artefacts,
    and the record pass is probed first so that the refusal normally comes
    before any document is unlinked at all; see [`case delete`](#case-delete)
    for the probe and for the time-of-check-to-time-of-use gap it leaves.
13. A record that must survive a deletion and names a record the deletion
    would remove is the additive `delete.record_entangled`, a `delete`
    refusal that exits `4` and is never retryable, raised in the scan before
    anything is unlinked. Its `details` carry `record_kind` and
    `retained_count` and never an identifier. Only an association can reach
    it today, by naming submissions in two cases.

Permissions are never repaired as a side effect. `archive init` narrows the
supplied root once, deliberately, as part of creating the archive; after that
every wider path is refused. The explicit repair action the design describes
is `archive repair-permissions`, which is asked for by name, only narrows, and
reports every path it changed.

The export decided three more conditions the contract left open:

1. A destination that lies inside the archive root, and one that is not a
    usable directory, are `usage.arguments` naming the `destination`
   argument, because the invocation itself is wrong rather than the archive.
   A destination that already holds anything, including a file at a target
   path, is `export.destination_conflict`, whose `details` carry `scope`,
   `conflict_count`, and the additive `export_path`, the path relative to the
   destination. That path is built from openPapir's own fixed directory names
   plus a digest or an identifier, so it discloses nothing the privacy rule
   protects.
2. `export.copy_mismatch` reports the expected digest and a
   `conflict_count`, and the partial copy is removed. A stored object whose
   bytes no longer match its path is found by `archive check`; the export
   refuses rather than writing a file whose name does not describe its
   content, and repairs nothing.
3. The exported directory for a record kind is the record kind's own name,
   so `records/<kind>/<id>.json` is derivable from the manifest entry alone.
   The archive's own directory names, `records/cases` and the rest, are not
   reused, because a manifest entry names a kind rather than a directory.

An input that is a symbolic link is refused with `path.symlink` carrying
`scope` `input`. The path is outside the archive, so it has no
archive-relative form and no `archive_path` is reported; the path itself is
user-supplied and is never echoed. The refusal comes from the no-follow open
itself, not from a check that precedes it, so nothing is stat-ed first. A link
in an export destination carries `scope` `export_destination` and the additive
`export_path`, the path relative to the destination, and no `archive_path`.

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
| `record.malformed` | A record `case delete` keeps names an artefact by something that is not a digest, so the archive cannot say which object it means. Emitted by `case delete`, with `stage` and `malformed_count`. Every object the purge had considered is retained as `referenced_elsewhere`, and the records the deletion planned to remove still go. |
| `platform.replace_while_open` | A purge could not unlink an object now because another process holds it open, so the removal is deferred to the user closing it. Emitted by `case delete` on platforms that defer an unlink, with `stage`, and with `read_only_restored` only where the read-only attribute was actually cleared; its absence says it never was. The object is counted as `unremovable` and the command still reports what it did remove. Reported once however many objects deferred, carrying the worst outcome any of them saw. |

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
or a process owner. The single exception is the export destination in human
output: `case export` prints the `--to` argument the user typed in the same
invocation, back to them, so that they can see where their copy went. It never
reaches `data`, `message`, `details`, or stderr, and no other user-supplied
path is echoed anywhere. A receipt label and an evidence statement are the user's
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
