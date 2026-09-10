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
| `openpapir-core` | The local archive: marker, artefact store, atomic writes, single-writer lock, input caps, path safety, import-event records, the case, submission, receipt, and association records, the read-only whole-archive integrity check, the read-only summary and its receipt-retrieval reminders, the export of one case, the import of one export back into an archive, the permission repair, and the deletion of one case with its explicit purge. |
| `openpapir-cli` | Argument parsing, the response envelope, and the exit-code mapping. |

Only these invocations are supported. `openpapir --help`,
`openpapir --version`, and `openpapir capabilities [--json]` stand outside the
operation list. The table below is the authoritative enumeration of the
operations `capabilities` reports: `crates/openpapir-cli/tests/contract.rs`
parses its operation names and fails when they and the binary's list disagree,
so an operation added later changes this table and that test. Prose elsewhere
in this repository defers to the list `capabilities` reports instead of
naming a count that would go stale.

| Operation | Invocation |
| --- | --- |
| `archive.init` | `openpapir archive init <root> [--json]` |
| `import` | `openpapir import --archive <root> <file>... [--json]` |
| `case.create` | `openpapir case create --archive <root> --title <t> [--notes <n>] [--tag <t>]... [--status open\|closed] [--json]` |
| `case.list` | `openpapir case list --archive <root> [--status <s>] [--tag <t>]... [--query <text>] [--json]` |
| `case.show` | `openpapir case show --archive <root> <case-id> [--json]` |
| `case.update` | `openpapir case update --archive <root> <case-id> [--title <t>] [--notes <n>\|--clear-notes] [--status open\|closed] [--tag <t>]... [--untag <t>]... [--json]` |
| `submission.add` | `openpapir submission add --archive <root> --case <case-id> --description <d> [--date <yyyy-mm-dd>] [--artefact <digest>[:<role>]]... [--json]` |
| `receipt.add` | `openpapir receipt add --archive <root> --artefact <digest> [--import-event <id>] [--label <l>] [--json]` |
| `receipt.list` | `openpapir receipt list --archive <root> [--json]` |
| `association.create` | `openpapir association create --archive <root> --receipt <receipt-id> --outcome <outcome> [--candidate <submission-id>:<confidence>:<statement>]... [--supersedes <association-id>] [--json]` |
| `association.list` | `openpapir association list --archive <root> --receipt <receipt-id> [--json]` |
| `association.retire` | `openpapir association retire --archive <root> <association-id> [--reason <text>] [--json]` |
| `submission.show` | `openpapir submission show --archive <root> <submission-id> [--json]` |
| `receipt.show` | `openpapir receipt show --archive <root> <receipt-id> [--json]` |
| `association.show` | `openpapir association show --archive <root> <association-id> [--json]` |
| `archive.check` | `openpapir archive check --archive <root> [--json]` |
| `archive.status` | `openpapir archive status --archive <root> [--as-of <yyyy-mm-dd>] [--json]` |
| `case.export` | `openpapir case export --archive <root> --case <case-id> --to <dir> [--json]` |
| `case.import` | `openpapir case import --archive <root> --from <dir> [--json]` |
| `archive.repair_permissions` | `openpapir archive repair-permissions --archive <root> [--json]` |
| `case.delete` | `openpapir case delete --archive <root> --case <case-id> [--purge] [--json]` |
| `skill` | `openpapir skill` |
| `completions` | `openpapir completions <bash\|zsh\|fish\|powershell\|elvish>` |
| `manpage` | `openpapir manpage` |

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
`archive check`, `archive status`, and `case export` open the archive
read-only, which creates no layout directory and flushes nothing, but runs the
same checks.
Every command that writes adds `lock.held`, `path.overwrite`,
`path.cross_device`, `write.interrupted`, `input.cap.record_size`, and
`platform.filesystem_unsupported`. Every command that reads a stored document
can report `record.malformed`. The per-command tables and lists below name
only what is particular to that command, and the whole set with its exit codes
is in [Implemented codes and exit codes](#implemented-codes-and-exit-codes).
`skill`, `completions`, and `manpage` are the exceptions: they open no
archive, read no input, and can refuse only with `usage.arguments` or
`internal.unexpected`.

The operations `capabilities` reports are the ones enumerated in
[Current implementation](#current-implementation) above, which is the
authoritative list. Everything else in
[local archive layout and storage design](archive-layout.md) and
[import error, JSON, and exit-code contract](error-contract.md) remains a
design: no derived-metadata or verification records; no automatic matching, no
receipt parsing, no export of a whole archive, and no editing of a stored
record other than the case record `case.update` rewrites, no
deletion of a single submission or receipt, deletion of an archive, or
migration. openPapir reads artefact bytes only to re-digest a stored object
during the integrity check, to copy one out during an export, and to store one
back during an import, and never to form an opinion about what an artefact
says, so an association is only ever the user's own assertion.

## The response envelope

Every command prints exactly one JSON object on stdout in its `--json` form,
on one line, and nothing else. Human-readable output goes to stdout only in the
non-`--json` form; warnings and errors go to stderr there, so diagnostic text
never shares stdout with the JSON object.

| Key | Meaning |
| --- | --- |
| `schema_version` | The envelope's version, currently `1`, independent of `archive_schema_version`. |
| `ok` | `true` only when the command completed its stated work. |
| `command` | The invoked command's stable name: `capabilities`, or one of the operation names `capabilities` reports. An invocation the argument parser rejected before it recognised a subcommand carries `openpapir` instead. |
| `data` | The command's result. `{}` when `ok` is `false`, except `archive check`, whose report is the result the user asked for and stays in `data` beside the error. |
| `verified` | Always `false`. No cryptographic check is implemented. |
| `error` | Present exactly when `ok` is `false`: `code`, `message`, `details`. |
| `warnings` | Present only when a platform degradation was observed. |

`code` is the only field a caller may branch on. `message` wording may change
within a `schema_version`. `details` carries at most 16 keys, whose values are
strings, integers, booleans, or arrays of at most 16 such scalars, and always
carries `bucket`. Changes within `schema_version` are additive only.

`stage` is a plain string that names how far the implementation has come, and
it is one of a small closed set: `scaffold`, `alpha`, `beta`, `stable`. It is
not a version, not a support promise, and never a verification verdict. The
value is `alpha` today, because the operations `capabilities` reports are
implemented against a local archive whose on-disk layout may still change. A
move to another value is a release decision, recorded in `CHANGELOG.md` in
the pull request that makes it; the set itself grows or shrinks the same way. A caller
that branches on `stage` must treat an unknown value as "at least as far as
the last value it knows", and a caller that needs to know what the binary can
do reads `operations`, not `stage`. Changing the value is not a
`schema_version` change: the field's name, type, and meaning are unchanged.

The capabilities response is unchanged in shape and lists the implemented
operations:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "alpha",
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
      "association.retire",
      "archive.check",
      "archive.status",
      "case.export",
      "case.import",
      "archive.repair_permissions",
      "case.delete",
      "skill",
      "case.update",
      "submission.show",
      "receipt.show",
      "association.show",
      "completions",
      "manpage"
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
into owner-only directories. The case record is the one kind that may be
rewritten in place, by `case update`, keeping its `id` and its `created_at`
and gaining an `updated_at`; every other kind stays append-only. The reason is
what each record is for. A submission, a receipt, and an association are the
user's evidence of what they recorded at the time, and evidence that can be
edited is no longer evidence, so a change there writes a new record that
supersedes the earlier one. A case is the user's own folder label, carries no
evidence, and is named by the same identifier for the life of the archive, so
writing a second record for a retitling would leave every reference the user
already holds pointing at a document that is no longer current. The same
decision, with the same reasoning, is recorded in
[local archive layout and storage design](archive-layout.md). The rewrite goes
through the atomic write procedure like every other write, with a rename in
place of the link the never-overwrite publish uses, so a concurrent reader
sees the whole old document or the whole new one and never a partial file.

Opening an archive for writing creates any layout
directory this build expects and an earlier one did not, so an archive created
by an earlier build gains the record directories it lacks; nothing else
changes. Opening an archive read-only creates nothing, and an absent layout
directory reads as empty there.

| Record | Path | Fields |
| --- | --- | --- |
| Case | `records/cases/<id>.json` | `title`, optional `notes`, `status`, `tags`, optional `updated_at`, plus the four common fields. |
| Submission | `records/submissions/<id>.json` | `case_id`, `description`, optional `stated_date`, `artefacts`, plus the four common fields. |
| Receipt | `records/receipts/<id>.json` | `artefact_digest`, `import_event_id`, optional `label`, plus the four common fields. |
| Association | `records/associations/<id>.json` | `receipt_id`, `outcome`, `candidates`, `created_by`, `submission_id`, `supersedes`, an optional `statement` a retirement carries, plus the four common fields. |
| Import event | `records/imports/<id>.json` | `digest`, `byte_length`, `imported_at`, `created_object`, `original_filename`, `id`, `record_kind`, `archive_schema_version`. |

Each entry of `artefacts` is an object with `digest`, the algorithm-qualified
digest of an object already stored in this archive, and an optional `role`, the
short label the user gave that artefact. `role` is omitted when the user
supplied none, so an entry is `{"digest": "sha256:..."}` or
`{"digest": "sha256:...", "role": "cover letter"}`.

`status` is `open` or `closed` and means only what the user's own filing
means: a closed case is one the user stopped working on. It says nothing about
delivery, receipt by an authority, authenticity, or legal effect, and openPapir
never sets it on its own. `tags` are the user's own words, stored sorted and
deduplicated. A case record written before these fields existed reads as
`open` with no tag, so an archive an earlier build wrote needs no migration.
`updated_at` is absent until `case update` changes something, so the field
states what happened rather than repeating `created_at`.

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
| `tag` | 64 bytes each, 32 distinct per case | No | `input.cap.tag_length`, `input.cap.tag_count` |

An empty required field, a forbidden control character, an unusable artefact
reference, and a date that is not a calendar date are `usage.arguments`,
naming the argument and never its value.

## `case create`

`openpapir case create --archive <root> --title <t> [--notes <n>]
[--tag <t>]... [--status open|closed]` writes one
case record. It takes the writer lock and runs the archive's permission checks
first. An empty or oversized `--title` or `--notes` is refused before the
archive is touched, as `usage.arguments` or `input.cap.field_length`, and so
is a tag over its cap or past the per-case count, as `input.cap.tag_length` or
`input.cap.tag_count`. `--tag` may be repeated; the tags are stored sorted and
deduplicated, so a tag the user repeated never spends part of the count cap.
`--status` takes `open` or `closed` and is `open` when it is not given; any
other value is `usage.arguments` naming the argument and never the value.
`data` holds `case`, the document just written, so it carries the user's own
title, notes, status, and tags and nothing else.

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
      "status": "open",
      "tags": [],
      "title": "Tax matter"
    }
  },
  "verified": false
}
```

## `case list`

`openpapir case list --archive <root> [--status <s>] [--tag <t>]...
[--query <text>]` reads every case record and keeps the ones that match every
filter it was given. It takes no lock: a reader sees one whole document or
another and never a partial one. `data` holds `cases`, ordered by identifier,
and `count`, which is how many cases the listing holds and so, under a filter,
how many matched. The order is the identifier order whether a filter was given
or not.

`--status` keeps only cases with that status. `--tag` may be repeated and
every tag given must be on the case. `--query` keeps only cases whose title or
notes contain the text, compared without regard to case. There is **no index**:
the query is a substring match applied in one linear scan over the records the
listing already read, so its cost grows with the number of cases the archive
holds. The query text is the user's own and is never echoed back, in either
output form, whether it matched anything or not.

An archive with no case, and a filter that matches none, are both not errors:
`cases` is empty, `count` is `0`, and the exit code is `0`. A `--status` that
is not `open` or `closed` is `usage.arguments`. Beyond that and the shared
refusals it emits nothing of its own. `data` is the user's own records read
back to them, so it carries the titles, notes, statuses, and tags they typed,
and no path.

## `case show`

`openpapir case show --archive <root> <case-id>` reads one case and the
submissions that name it. `data` holds `case`, `submissions` ordered by
identifier, `submission_count`, and `receipts`. The human form prints the
case's status
and tags, and its `updated_at` once there is one. An identifier that names no
case, and one
that is not 32 lowercase hexadecimal characters, are both `record.not_found`,
naming the kind and how it was referenced and never the value the user
supplied. `data` is the user's own records read back to them, and carries no
path.

`receipts` is every receipt whose live association names a submission of this
case, ordered by receipt identifier and then by association identifier. Each
entry holds `receipt`, the record as it is stored, `association_id`,
`outcome`, and `submission_ids`, the submissions of this case that one
association names. Only the live head of each supersession chain is read, as
[`archive status`](#archive-status) reads one: a superseded record stays
stored and [`association list`](#association-list) still shows it, but it is
what the user asserted then rather than now, so withdrawing an assertion takes
the receipt out of this section without removing anything. A case with no
submission, and one no live association names, both hold an empty array rather
than an absent key. The entry is the user's own assertion read back: it states
no delivery, receipt by an authority, authenticity, or legal effect, and
openPapir matched nothing to build it.

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
      "status": "open",
      "tags": [],
      "title": "Tax matter"
    },
    "receipts": [],
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

## `case update`

`openpapir case update --archive <root> <case-id> [--title <t>]
[--notes <n> | --clear-notes] [--status open|closed] [--tag <t>]...
[--untag <t>]...` rewrites one case record in place. It is the one invocation
that rewrites a stored record, and it rewrites only the case record; the rule
and the reason are under [Records](#records) above.

The record keeps its `id` and its `created_at` and gains an `updated_at`. What
may change is the user's own filing: the title, the notes, the status, and the
tags. `--tag` adds and `--untag` removes; both may be repeated, and the result
is stored sorted and deduplicated. Removing a tag the case does not carry is
not an error and changes nothing. `--notes` and `--clear-notes` may not be
given together.

The removal is applied first and the additions after it, so a tag both added
and removed in one invocation **stays on the case**: the user named it as
something the case should carry, and that is the more specific of the two
requests.

A `--untag` value is checked against the record rather than against the caps.
A value that breaks the tag length cap or its shape cannot be on a case at
all, so removing it is a no-op rather than a refusal; only a `--tag` value is
held to `input.cap.tag_length`, `input.cap.tag_count`, and the no-control-
character rule. An invocation whose every `--untag` names a tag the case does
not carry therefore changes nothing, and is the `usage.arguments` refusal
above rather than a cap refusal.

Every other supplied field is checked before the archive is opened, so a field
over its cap is refused before the writer lock is even asked for. The command
then takes the lock and reads the record.

An update that would leave the record exactly as it is, either because the
invocation named nothing at all or because it named only values the record
already holds, is `usage.arguments` with `argument` `update`. Nothing is
written and no `updated_at` moves. An identifier that names no case, and one
that is not 32 lowercase hexadecimal characters, are both `record.not_found`.

`data` holds `case`, the whole record as it now stands, and `changed`, the
sorted names of the fields the update changed. `changed` carries **field names
only**: what a value was before is the user's own text that they have just
replaced, and the record beside it already carries what each value is now.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "case.update",
  "data": {
    "case": {
      "archive_schema_version": 1,
      "created_at": "2026-01-14T09:12:33Z",
      "id": "6b73d041fb6bed26be75545fabfd45bc",
      "record_kind": "case",
      "status": "closed",
      "tags": [
        "appeal"
      ],
      "title": "Tax appeal",
      "updated_at": "2026-02-01T08:00:00Z"
    },
    "changed": [
      "status",
      "tags",
      "title"
    ]
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

Association records are append-only. `--supersedes` names an earlier
association for the
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
| `already_superseded` | The record `association retire` names is superseded already. |
| `import_event_digest_mismatch` | A named import event records another artefact (`receipt.add`). |

One further rule of the same code, `supersedes_cycle`, is enforced by
[`case delete`](#case-delete) rather than by a write. It names stored
association records that supersede each other in a cycle, which no command
here can produce, so it belongs to the records an archive already holds rather
than to the fields a user supplies.

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

## `association retire`

`openpapir association retire --archive <root> <association-id>
[--reason <text>]` withdraws an assertion the user no longer stands behind. It
writes a new record for the same receipt, with outcome `unassociated`, an
empty `candidates` list, and `supersedes` naming the record the user retired.
Nothing is edited and nothing is removed: the retired record stays exactly as
it was written, and `association list` shows both, so the history reads as
what the user asserted and then that they withdrew it. `data` is one
`association`, the same shape `association create` returns.

`--reason` is the user's own single line, at most 512 bytes. It is stored on
the new record as `statement`, the one field only a retirement carries, and it
is the user's own text: no message, warning, or count repeats it, and the
human form does not print it back. An oversized or multi-line reason is
`input.cap.field_length` or `usage.arguments`, naming the field and never the
value.

An identifier that names no association is `record.not_found`. A record
another association supersedes already is `record.inconsistent` with rule
`already_superseded`, because a further statement has to supersede the newest
record of the history rather than one behind it, and two records claiming to
replace the same one would leave the history ambiguous. Retiring holds the
writer lock, like every other write.

Retiring is what unblocks a `case delete` refused with
`delete.record_entangled`; see [`case delete`](#case-delete).

## `submission show`

`openpapir submission show --archive <root> <submission-id>` reads one
submission and every association naming it. It takes no lock, exactly as
`case show` takes none. `data` holds `submission`, the record as it is
stored, `associations`, and `association_count`.

An association names a submission when the submission is one of its
`candidates` or is the `submission_id` an `associated` outcome confirms.
Every such record is reported whatever its outcome and whether or not another
record supersedes it. The order is the live heads first and the superseded
records after them, each group newest first as `association list` orders one,
so what the user asserts today reads before what they asserted before it.
Nothing is collapsed and nothing is filtered.

An identifier that names no submission, and one that is not 32 lowercase
hexadecimal characters, are both `record.not_found`, naming the kind and how
it was referenced and never the value the user supplied. Beyond that and the
shared refusals it emits nothing of its own. `data` carries the user's own
records and openPapir's own identifiers, and no path.

## `receipt show`

`openpapir receipt show --archive <root> <receipt-id>` reads one receipt and
its whole association history. It takes no lock. `data` holds `receipt`, the
record as it is stored, `associations`, and `association_count`.

The history is the one [`association list`](#association-list) reports for
the same receipt, in the same order: newest first, superseded records
included, each entry carrying its own `supersedes`. Nothing is collapsed,
filtered, or presented as a single best guess.

An identifier that names no receipt is `record.not_found`, naming the kind and
how it was referenced and never the value the user supplied. Beyond that and
the shared refusals it emits nothing of its own. `data` carries the user's own
records and openPapir's own identifiers, and no path.

## `association show`

`openpapir association show --archive <root> <association-id>` reads one
association and the supersession chain it belongs to. It takes no lock. `data`
holds `association`, the record as it is stored, `live`, `chain`, and
`chain_length`.

`chain` is every record reachable from this one along `supersedes`, in both
directions: what it supersedes, transitively, and what supersedes it, with the
record itself among them. The order is the one `association list` uses, newest
first. `chain_length` is how many records the chain holds, and `live` is
`true` exactly when no stored record supersedes this one, which is what makes
it the assertion that stands today rather than history behind a newer record.
The walk keeps a visited set, so a hand-edited archive holding a cycle yields
a finite chain instead of looping.

An identifier that names no association is `record.not_found`, naming the kind
and how it was referenced and never the value the user supplied. Beyond that
and the shared refusals it emits nothing of its own. `data` carries the user's
own statements and openPapir's own identifiers, and no path.

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
      { "code": "record.inconsistent", "count": 0 },
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

`path.symlink`, `record.malformed`, `record.inconsistent`,
`integrity.digest_mismatch`, `integrity.length_mismatch`,
`integrity.dangling_reference`, `integrity.orphan_object`.

The order runs from what stopped the check reading something, through what it
read and disbelieved, to what is merely unreferenced, and it is fixed so that
one archive always reports one code. The exit code is the highest of the
buckets' groups, as the contract requires of any command that reports several
conditions: `4` whenever a `record` or `integrity` condition was found, and
`3` for an archive whose only complaint is a link inside the store.

`record.inconsistent` counts the `supersedes` cycles the association records
form, archive-wide. An association supersedes at most one record, so the
supersession graph has out-degree one and every walk along it ends, leaves the
archive at a reference nothing stored answers, or closes on itself; a closed
walk is a cycle. The count is of cycles rather than of the records in them,
and the record pass keeps two fixed-size identifier keys per edge and nothing
else, so the graph costs the check what a pair of identifiers costs. The
`error` carries `record_kind` and rule `supersedes_cycle` and never an
identifier, exactly as the refusal `case delete` raises for a chain holding
one.

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

## `archive status`

`openpapir archive status --archive <root> [--as-of <yyyy-mm-dd>]` summarises
what the archive holds and reminds the user where to look in their delivery
storage for a submission receipt while the operator says one would still be
there. It is read-only exactly as
`archive check` is: no writer lock is taken, so a held lock never stops it, no
missing layout directory is created, no directory entry is flushed, and
nothing inside the root is written, renamed, or removed. It reads no artefact
bytes at all, because it counts objects rather than judging them; judging them
is `archive check`'s work.

### The retention window and where it comes from

The window is one constant, 30 days. The operator's help page states that the
personal delivery storage retains incoming documents, *igazolások* and
*nyugták* for 30 days unless they are moved to permanent storage. It was
retrieved on 2026-09-09, and the claim is **descriptive**, not normative
([receipt-discovery](receipt-discovery.md), E1). openPapir does not enforce
the window, does not read any mailbox, and does not check any service: a
reminder here is arithmetic over the date the user typed themselves and the
operator's own published description of their storage, and nothing else.

A reminder therefore says one thing: go and look in the delivery storage
while the operator says a submission receipt would still be there. It does not
presuppose that one is there, and it never states that anything was delivered,
that a receipt exists, that one was received by an authority, or that any
legal effect followed. The human wording is bound by that rule as tightly as
the JSON is.

### What is listed

A submission is listed in `receipts_to_retrieve` when all three hold:

1. It carries a date the user supplied, which is a `YYYY-MM-DD` calendar date.
2. No **live** association with outcome `associated` or `candidate` names it,
   either as its confirmed submission or as one of its candidates.
   `unassociated` and `contradictory` do not take a submission off the list:
   neither ties a receipt to it. Only the live head of each supersession chain
   is read, that is, every record no other record supersedes. A superseded
   record is history: it is never modified, never removed, and
   `association list` still shows the whole chain, but it no longer says what
   the user asserts today. So a user who withdraws an `associated` assertion,
   with [`association retire`](#association-retire) or by superseding it
   themselves, is reminded of that submission again, which is the point of
   being able to withdraw one.
3. Its date plus the window is on or after the as-of date. The last day of the
   window still counts as open, so `days_left` is then `0`.

Each entry carries the case identifier, the submission identifier, the date
the user stated, the computed `retrieve_by`, and `days_left`, and the list is
ordered by `retrieve_by` and then by submission identifier, so one archive
always reports one order. A submission carrying no date, or a stored date
openPapir cannot read as a calendar date, is counted in `undated_submissions`
and never listed: without a date there is nothing to remind anybody of, and a
stored date is the user's own text, which openPapir never repairs or guesses
at.

`--as-of` is validated exactly as `submission add --date` is, and a date in
the past or the future is accepted rather than refused, because asking what
the archive looked like, or will look like, on another day is the point of the
flag. Without it the date is the process clock's own UTC date. A value that is
not a calendar date is `usage.arguments` with `argument` `as_of`, refused
before the archive is opened and never echoing the value.

`cases` is the total, and `cases_by_status` breaks it down, one entry per
status in the closed set `open`, `closed`, including a status no case holds,
so a caller reads a count rather than testing for a key's presence. The counts
sum to `cases`.

A case's status changes no reminder. Closing a case is the user's own filing,
which says the user stopped working on the matter and nothing else, so
openPapir does not read it as saying that no receipt is wanted: a submission
in a closed case is listed exactly as one in an open case. Filtering the list
would be openPapir deciding something about the user's correspondence, which
it does not do anywhere else either.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "archive.status",
  "data": {
    "as_of": "2026-02-10",
    "associations": 2,
    "cases": 2,
    "cases_by_status": [
      { "count": 1, "status": "open" },
      { "count": 1, "status": "closed" }
    ],
    "receipts": 1,
    "receipts_to_retrieve": [
      {
        "case_id": "<id>",
        "days_left": 9,
        "retrieve_by": "2026-02-19",
        "submission_date": "2026-01-20",
        "submission_id": "<id>"
      }
    ],
    "retention_window_days": 30,
    "stored_objects": 3,
    "submissions": 4,
    "undated_submissions": 1
  },
  "verified": false
}
```

`data` carries counts, dates, and the identifiers openPapir minted, and
nothing else: no title, no description, no notes, no statement, no digest, and
no path. Human output prints the same figures in the same order and no path.

```text
As of 2026-02-10. Case(s): 2. Submission(s): 4. Receipt(s): 1. Association(s): 2. Stored object(s): 3.
Case(s) by status: open 1, closed 1.
Submission(s) with no usable date: 1.
Reminder(s) to look for a submission receipt in the delivery storage while the 30-day window the operator describes is open: 1.
case <id>, submission <id>, stated 2026-01-20, look by 2026-02-19, 9 day(s) left.
```

The summary always completes its stated work, so it exits `0` with `ok`
`true` whatever it counted; an empty archive is a summary of zeroes rather
than a refusal. The refusals are those of opening an archive, plus
`record.malformed` for a stored document that cannot be read as a record of
its kind, plus `usage.arguments` for an unusable `--as-of`.

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

An interrupted write in the destination reports the stage it was in, from the
one table of [write stages](#write-stages): the copies and the directory they
go in are an object write, a record document and its directory a record write,
and the destination itself and its `manifest.json` a marker write, because
they describe the export rather than any one record.

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

Reading an export back into an archive is `case import`, below. Exporting a
whole archive is not implemented: a backup is a plain copy of the archive root
taken while no openPapir process holds the lock, and
`archive repair-permissions` is what makes a restored copy usable again.

## `case import`

`openpapir case import --archive <root> --from <dir>` reads a directory
`case export` wrote and puts the case back into an archive. It is the same
plain copy in the other direction: the objects are the exported bytes, the
records are the exported documents, and every record keeps the identifier it
had, so a restored case is the case that was exported rather than a copy of
it.

The manifest is authoritative for what the export contains
([archive-layout](archive-layout.md)). Nothing outside it is read, so a file
dropped into the directory afterwards is neither stored nor reported, and a
manifest that names something the directory does not hold is a refusal rather
than a smaller import.

Everything is checked before anything is written. The manifest is read and
every claim it makes about itself is checked, every record it names is read
and parsed as a record of its own kind, and every object it names is
re-digested from the export's own bytes. Only then is the writer lock taken,
the record set probed against the archive, the objects stored, and the records
published. A refusal in any of those passes leaves the archive exactly as
`archive check` found it.

The source must be an existing directory that is not a symbolic link and does
not lie inside the archive root. Both paths are resolved before they are
compared, and a path that cannot be resolved at all is refused rather than let
through.

Inside the source the rules the archive applies inward are applied outward,
and one rule covers every path there. A symbolic link is refused as
`path.symlink` carrying `scope` `export_source`, wherever it is: the manifest,
a record, and an object are all refused the same way rather than each
reporting the condition of its own reader. Every path is opened with the
platform's non-blocking no-follow open, so a named pipe planted in a
user-supplied directory cannot hold the open call open, and the file kind is
then taken from the opened handle rather than from a second look at the path.
A record document that is not a bounded regular file is `record.malformed`,
the condition of the document, and an object path that is not a regular file
is `export.object_mismatch` with `reason` `unusable`, because it holds no
copy at all.

An export written under another archive schema version is refused by the
schema rules that refuse such an archive, `archive.schema_newer` or
`archive.schema_older`. The manifest's own format version is separate and a
manifest of another one is `export.manifest_malformed`.

### What an import writes

| What | Rule |
| --- | --- |
| An object the archive does not hold | Stored through the content-addressed store, exactly as `import` stores one. |
| An object the archive already holds | Left untouched. A duplicate is not an error, and no second copy is made. |
| A record the archive does not hold | Written under its original identifier. |
| A record the archive already holds, byte for byte | Not written again, and not an error. |
| A record identifier a different record holds | `export.record_conflict`, before anything is written. openPapir edits no stored record, so neither is changed. |
| An import event for each stored object | Written as a new record with `source` `export`. |

The record pass is all or nothing. Every document is staged in the directory
it will be published into, and only then is the set published, so an import
that cannot be finished removes the records it had already published and the
objects it had just created. The object pass is undone the same way: an
object placed before a later one is refused is removed again, so a refusal
part way through storing never leaves an object no record names. An
interrupted import therefore leaves the whole case or nothing of it, and the
archive stays clean.

A restored import event is a record like any other, so the history of the
user's own import survives the round trip. The event openPapir writes for a
stored object is new and additive: it carries `source` `export`, an absent
field being the user's own import of a local file, and it carries no original
filename, because an export names its objects by digest alone and holds none.
The filename the user's own import recorded stays where it has always been, an
attribute inside the restored import-event record.

An object the archive already held gains no event, so importing one export
twice writes nothing at all the second time. That is what makes an import
idempotent: the same export applied twice leaves the same archive.

The input caps are the caps of `import`, enforced while the copies are read
and again while they are stored.

| Condition | Code |
| --- | --- |
| The source is not a usable directory, or lies inside the archive root. | `usage.arguments` |
| The source, or a path in it, is a symbolic link. | `path.symlink` |
| The source holds no manifest. | `export.manifest_missing` |
| The manifest cannot be read as a manifest of this format. | `export.manifest_malformed` |
| The export was written under another archive schema version. | `archive.schema_newer`, `archive.schema_older` |
| A copy is absent, is not a regular file, or its bytes disagree with the manifest. | `export.object_mismatch` |
| The manifest names a record document the export does not hold. | `export.record_missing` |
| A document the export holds cannot be read as a record of its kind. | `record.malformed` |
| A record identifier is held in the archive by a different record. | `export.record_conflict` |
| A copy exceeds the single-file cap, or the import exceeds the total cap. | `input.cap.file_size`, `input.cap.import_bytes` |
| A copy could not be read, or a record could not be published. | `write.interrupted` |

`data` reports counts and the case, and never the source.

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "case.import",
  "data": {
    "bytes_stored": 27,
    "case_id": "e19fb6367693c26aadf609565ec6b8d8",
    "events_recorded": 1,
    "object_count": 2,
    "objects_present": 1,
    "objects_stored": 1,
    "records": [
      { "count": 1, "kind": "case" },
      { "count": 1, "kind": "submission" },
      { "count": 1, "kind": "receipt" },
      { "count": 1, "kind": "association" },
      { "count": 2, "kind": "import_event" }
    ],
    "records_present": 2,
    "records_written": 4
  },
  "verified": false
}
```

`records` describes the case the manifest holds, one count per kind including
the empty ones. `records_written` and `records_present` split those between
what this import wrote and what the archive already held, and
`events_recorded` counts the import events openPapir wrote of its own, which
are not part of the export.

Human output adds one thing the JSON does not carry, the source the user
supplied, because the line repeats the argument they just typed.

```text
Imported case e19fb6367693c26aadf609565ec6b8d8 from /tmp/example-export.
Stored 1 object(s), 27 byte(s); 1 already present.
Wrote 4 record(s); 2 already present.
case 1
submission 1
receipt 1
association 1
import_event 2
Recorded 1 import event(s) with source export.
Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

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
`stage` is the kind of path the repair was inspecting, read from the one table
of [write stages](#write-stages). The kind is a closed set, so a path the
repair walks never reaches a stage by falling through a default.

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
| Associations | The unit is the supersession chain, because a record that supersedes another cannot go without it: removing the older record alone would leave the newer one naming a record the archive no longer holds. A chain goes when one of its records names a submission that is going and its live record, the one no other record supersedes, names none that remains. That is the ordinary case, where every candidate the chain ever named belongs to this case, and the retired one, where the live record asserts nothing at all, so the withdrawn history goes with the case it was about. Every record of a departing chain is counted under `records_removed` as `association`. A chain naming no departing submission is no business of this deletion and stays, and one whose live record still names submissions in this case **and** in another is refused rather than resolved; see below. |
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
tell which object that record meant, so a purge unlinks none of them: every
object `--purge` would otherwise have removed is retained with the reason
`referenced_elsewhere` instead. The records the deletion planned to remove
still go, `ok` stays `true`, and the same reference on a record that is
**going** changes nothing, because that record and its claim both leave.

Such references are reported as a `record.malformed` warning carrying
`stage`, `malformed_count`, and `withheld_count`, and **only when the
reference held an object of this deletion back**: `--purge` was given and this
case had a candidate the purge would otherwise have removed. `malformed_count`
is how many such references the scan read anywhere in the archive.
`withheld_count` is how many candidate objects of this deletion they held
back, which is the number the message's `for this case` describes. They count
different things, so either may be the larger: one such reference, wherever it
sits, already holds back every candidate of this deletion, and several may
hold back a single one. Both are above zero wherever the warning is emitted at
all, and neither is derived from the other. The scan reads the whole archive,
so one hand-edited document anywhere would otherwise attach the
warning to every later deletion, including ones with no candidate and ones
that asked for no purge, and describe the archive rather than the command the
user ran. The warning's text says `for this case` for the same reason.
Finding such a document wherever it sits is `archive check`'s work, not
`case delete`'s. The counts are the whole of the warning: the record that
holds the reference is not named, and neither is the value.

Without `--purge` nothing was going to be unlinked, so the unresolvable
reference decided nothing: a candidate no remaining record names keeps
`purge_not_requested`, the reason that actually held it, exactly as in an
archive with no such document. `referenced_elsewhere` is reserved for a
candidate a remaining record names outright, and, for the unresolvable
reference, for one a purge would otherwise have removed. Nothing is more
removable for either rule: with `--purge` the retained set is unchanged, and
without it no object is ever unlinked.

An association may name submissions in more than one case. While the user
still asserts it, one of them going and another remaining leaves the record
naming a submission the archive no longer holds. openPapir edits no stored
record, so it can neither drop the departing candidate nor invent a shorter
record, and removing the assertion would delete the user's own statement
about a case they did not ask to delete. The deletion is refused instead,
with `delete.record_entangled`, before anything is touched. The refusal is
symmetric: until the assertion is withdrawn, neither case can be deleted. It
is the only one of the three possible outcomes that loses nothing and can be
undone.

The remedy is `association retire`, and it is the user's decision rather than
openPapir's. Retiring writes a record superseding the assertion, so nothing is
edited and nothing is removed, and the chain's live record then names no
submission that remains. Deleting either case then takes the withdrawn history
with it, the record naming the other case's submission included, because the
user has said the assertion no longer stands. `retained_count` counts the live
records standing in the way, which are the ones a retirement can name, and
names none of them.

A **superseded** record naming another case's submission is the other side of
the same rule, and it goes rather than refusing. The chain's live record is
what the user asserts today; when it names no submission that remains, nothing
live objects to the deletion, so the whole chain goes with the case, the
superseded record about the other case included. That case keeps its own
submission and its own record. Refusing instead would let history the user
already replaced block a deletion, and removing the superseded record alone
would leave the newer one naming a record the archive no longer holds. The
chain is therefore the unit in both directions
([archive-layout](archive-layout.md)).

A chain holding a `supersedes` cycle anywhere in it is refused, and the unit of
the refusal is the chain rather than the cycle alone. No openPapir command
writes a cycle: `association retire` refuses a record something already
supersedes, and `association create` refuses a `--supersedes` outside the
receipt, so a cycle reaches an archive only by hand. Inside a cycle every
record is superseded by another, so nothing in it says what the user asserts
today. A chain that is only a cycle has no live record for the two rules above
to read, so one naming a departing submission and a remaining one would
otherwise be read as history nobody asserts and removed whole; a chain whose
live record supersedes a cycle behind it is the same anomaly one layer up,
where the head's claim about that history cannot be checked because the
history cannot be read in order. A deletion that would otherwise have removed
such a chain is refused with `record.inconsistent` and rule `supersedes_cycle`,
before anything is touched, carrying the kind and the rule and never an
identifier or a count. A cycle this deletion would not have touched is left
alone: the scan reads the whole archive, so refusing on a cycle anywhere would
describe the archive rather than the command the user ran, and finding one
wherever it sits is `archive check`'s work.

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
| `purge_not_requested` | The object would have become unreferenced, and `--purge` was not given. A document elsewhere in the archive whose reference is not a digest does not change that, because no purge was going to unlink anything. |
| `referenced_elsewhere` | A submission or receipt that remains still references the object; or `--purge` was given and a record that remains references an artefact by something that is not a digest, which may be any of them. The second half is reserved for a purge: it is the reason only where the object would otherwise have been removed. |
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

`skill`, `completions`, and `manpage` are the operations `capabilities`
reports that touch no archive. They are reported there so that a machine
caller learns of them from the same list as every other operation.

```console
$ openpapir skill | head -3
---
name: openpapir
description: >-
```

## `completions`

`openpapir completions <shell>` writes one shell's completion script to
stdout, and nothing else. The shell is one of `bash`, `zsh`, `fish`,
`powershell`, or `elvish`, and a value outside that set is `usage.arguments`,
exit `2`. There is no `--archive` and no `--json`: a completion script is not
a result to report in two forms.

Because the command defines no `--json`, the refusal is the argument parser's
own usage text on stderr, which names `<SHELL>`, lists the five values, and
repeats the value typed. That is the parser's rendering of an invocation it
rejected, not openPapir's output about an archive, and the privacy rule binds
the second: no envelope is printed here at all, so no `details.argument` key
carries the argument's name. `skill` is the same case, and
`openpapir skill --json` is refused for the flag it does not define with an
envelope that carries no `argument` either.

The script is generated by `clap_complete` from the same command definition
the argument parser uses, so it cannot describe a command this build does not
have and cannot fall behind one it does. Nothing is stored in the repository
and nothing is pinned byte for byte: the exact script moves with the
generator's version, and pinning it would turn a dependency bump into a
contract change. What is pinned instead is what a user relies on, that every
subcommand this build defines is named by every shell's script.

The documented install path is a redirection into the shell's own completion
directory, so a destination that cannot take the bytes is a failure of the
command, reported exactly as `skill` reports one: a reader that closed the
pipe still exits `0`, and every other write or flush failure exits `4`, the
`write` bucket's code, with one line on stderr that names no path.

```console
$ openpapir completions bash | head -1
_openpapir() {
```

## `manpage`

`openpapir manpage` writes the man page for the whole command tree to stdout,
as one roff stream, and nothing else. It takes no file, no directory, and no
`--json`. The stream holds the page for `openpapir` first and then one page
per subcommand at any depth, each with its own title line and filed under the
dashed name a manual page for that command carries, `openpapir-archive-init`
for one. Every title line carries the version the binary reports, so a page
split out of the stream still names the build it describes. A reader pages
through the whole stream with `openpapir manpage | man -l -`, and an
installer that wants one file per
command splits the stream on its title lines. Writing the pages as files is
deliberately not an option: openPapir writes into a directory only when the
user named one for an export, and a generated document is handed to the
caller's own redirection instead.

The pages are generated by `clap_mangen` from the same command definition the
argument parser uses, with the same consequence as for `completions`: nothing
is stored, nothing is pinned byte for byte, and what is pinned is that every
command in the tree has a page. The stdout rule is the one `skill` and
`completions` follow.

```console
$ openpapir manpage | grep -m1 archive-init
openpapir\-archive\-init(1)
```

## Storage guarantees

| Guarantee | How it is kept |
| --- | --- |
| Atomic write | The content is written to a staging file inside the archive root, flushed, linked into place, and the destination directory is flushed. The staging name is then removed. |
| Never overwrite | The publish step is a hard link, which fails rather than replacing an existing file, so a destination openPapir did not create is refused as `path.overwrite`. A filesystem that reports it cannot create a hard link at all cannot host an archive and is refused as `platform.filesystem_unsupported` rather than as the retryable `write.interrupted`. A link the system merely refused, which on Unix is the `EPERM` a FAT32 or exFAT volume and an immutable file report alike, is `write.interrupted` carrying `capability` `hard_link` and `condition` `link_refused`: it names the condition observed rather than a cause openPapir cannot prove ([error-contract](error-contract.md)). |
| Interrupted write | A leftover staging file is never adopted, so the archive holds the complete file or nothing. |
| One filesystem | The root and its layout directories must share one device. A cross-device publish is refused as `path.cross_device`. |
| Owner-only | Directories are created `0o700`, files `0o600`, and stored objects become `0o400`. The root, the marker, the lock file, every layout directory, and each stored object and fan-out directory the operation touches are checked before anything is published; a wider one is refused as `archive.permissions_wide`, naming the archive-relative path. There is no override flag, and nothing is ever narrowed implicitly: an existing path is refused, not repaired. `archive repair-permissions` is the one explicit action that narrows an existing archive, and it never widens. |
| Copies outward | An export writes only into a destination outside the archive root, creates every file there with create-new semantics, follows no symbolic link, replaces nothing, and re-digests every copy before it is published. A destination the export itself created is removed again when the export fails. |
| Copies inward | An import reads only what the export's manifest lists, re-digests every copy before the archive is written to at all, and publishes the whole record set or none of it. An import that cannot finish removes the records it had published and the objects it had created, so `archive check` is clean either way. |
| Path safety | Input files are opened with the platform's no-follow flag, `O_NOFOLLOW` on Unix and `FILE_FLAG_OPEN_REPARSE_POINT` on Windows, and no path is stat-ed before it is opened. Symbolic links inside the archive are refused, on Windows together with NTFS junctions and every other reparse point, and a user-supplied filename is never joined into a path. |
| Single writer | A `lock` file recording the holder's process identifier, host, and start time admits one writer. A second writer refuses with `lock.held` rather than waiting. |

### Write stages

A `write.interrupted` refusal carries a `stage`, the kind of path that was
being written, never the module that reported it. The three stages and the
paths each one names are the same set the
[error contract](error-contract.md) lists, and a test parses both tables and
holds them to it.

| Stage | What it names |
| --- | --- |
| `object_write` | A stored object or an exported copy of one, the directory a copy is created in, a fan-out directory the repair cannot list, and a leftover staging file inside the object store. |
| `record_write` | A record document, the directory one is written into, a cached file, a layout directory, a fan-out directory the repair cannot narrow, and the archive root. |
| `marker_write` | The archive marker, and outside the archive the export destination itself and its `manifest.json`. |

The kinds of path the repair walks are a closed set in the implementation, and
each one names its stage: no kind falls through to `record_write` by default,
so a kind added without a decided stage does not compile.

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
| Case tag | 64 bytes | `input.cap.tag_length` |
| Case tags per record | 32 distinct | `input.cap.tag_count` |

The caps bind `case import` as they bind `import`, and the per-operation cap
is the one that binds a restore: an import reads every object the manifest
lists in one operation, so a case whose objects come to more than 512 MiB in
total is refused with `input.cap.import_bytes` and cannot be restored by this
build, even though several smaller imports were able to build it. The cap is
deliberately not scoped per object to make that case succeed: a cap is never
relaxed for one particular input ([AGENTS.md](../AGENTS.md)), and raising the
ceiling is a decision about the cap itself rather than about the command that
met it.

`input.cap.record_size` bounds a whole document and reports one cap, the
record cap. A per-field cap has to say which field it refused and which of the
six bounds applied, which that code cannot carry, so the field caps use the
additive `input.cap.field_length` instead. A tag has its own two codes rather
than a seventh field bound: one of them counts tags rather than bytes, and the
length one reports the tag's position among the tags supplied, which is how a
repeated flag says which value it refused without echoing the value. All of
them are `input` refusals and all exit `3`.

## Performance

There is no index. Every listing, the integrity check, the export, and the
deletion plan read the records they could report, in one linear scan, so their
cost grows with what the archive holds. The numbers below say what that costs
at a size a user could reach. They are indicative: they are one run on one
machine on one date, not a guarantee and not a benchmark result to compare
builds by.

Measured on 2026-09-10 on a 13th Gen Intel Core i9-13900 with an NVMe
solid-state disk and an ext4 filesystem, with the release binary, on a
synthetic archive of 10000 cases, 10000 submissions, 10000 receipts, 10000
associations, and 20000 imported objects of 256 bytes each.

| Invocation | Wall time | Ceiling |
| --- | --- | --- |
| `case list` | 0.06 s | 5 s |
| `case list --query` | 0.06 s | 5 s |
| `archive check` | 1.41 s | 10 s |
| `archive status` | 0.24 s | 5 s |
| `case export` of one case | 0.22 s | 5 s |
| `case delete --purge` of one case | 0.31 s | 10 s |

The ceiling is what `crates/openpapir-cli/tests/bench.rs` asserts. It is loose
on purpose: the same assertion has to hold on an unoptimised build, on a
slower disk, and on a busy machine, so crossing one means the cost changed in
kind rather than drifted. [Testing](testing.md) says how to run the
measurement and how to change its size.

One cost is not in the table, because it is not a scan a user asks for.
Building that archive took 11 minutes, and almost all of it was `import`:
recording an import event reads the import events already stored, so the cost
of importing a file grows with the number of imports the archive has ever
seen. In a separate run on the same machine and date, importing 1000 files
into an empty archive took 1.6 s, while importing the twentieth batch of 1000,
with 19000 import events already stored, took 46 s. Resolving a receipt's
artefact to its earliest import event reads the same records, which took
0.12 s over 20000 of them; naming `--import-event` reads one record instead.

## Implemented codes and exit codes

The exit code carries the error's bucket and nothing else. `1` is never
emitted.

| Exit | Buckets | Implemented codes |
| --- | --- | --- |
| `0` | Success, including a duplicate import and a warning | |
| `2` | `usage` | `usage.arguments`, `usage.archive_root_missing` |
| `3` | `input`, `path` | the eight cap codes above, `path.symlink`, `path.overwrite`, `path.cross_device` |
| `4` | `archive`, `lock`, `write`, `record`, `integrity`, `export`, `delete` | `record.not_found`, `record.malformed`, `record.inconsistent`, `archive.marker_missing`, `archive.marker_malformed`, `archive.adopt_refused`, `archive.schema_newer`, `archive.schema_older`, `archive.permissions_wide`, `archive.multiple_filesystems`, `lock.held`, `write.interrupted`, `integrity.digest_mismatch`, `integrity.length_mismatch`, `integrity.dangling_reference`, `integrity.orphan_object`, `export.destination_conflict`, `export.copy_mismatch`, `export.manifest_missing`, `export.manifest_malformed`, `export.object_mismatch`, `export.record_missing`, `export.record_conflict`, `delete.objects_retained`, `delete.records_retained`, `delete.record_entangled` |
| `5` | `platform` | `platform.filesystem_unsupported`, for a filesystem that reports it cannot create the hard link the publish step needs. A link refused without saying so is `write.interrupted` at `4` instead. The named degradations are warnings, and the owner-only condition of the same code is not detected yet. |
| `6` | `internal` | `internal.unexpected` |

An invocation the argument parser rejects exits `2` as `usage.arguments`.
With `--json` it is one envelope on stdout and nothing on stderr, so a machine
caller reads the same shape it reads for every other refusal; without `--json`
it is the parser's own usage text on stderr and no envelope. `--json` is found
in the raw arguments, because the parse that would have reported the flag is
the one that failed, and a token after `--` is a positional value rather than
the flag. `details.argument` names the flag or value name the parser
complained about, and only when the recognised command or one of its parents
defines it: an invented token, and a flag only another subcommand defines, are
text the user typed and are never echoed. The envelope's `command` is the
subcommand path that was recognised, or `openpapir` when none was. The
recognition walks the raw arguments as the parser would and consumes each
flag's value with the flag, so `case create --title list` is `case.create` and
not `case.list`.
`--help` and `--version` are not refusals and still exit `0`.

Every other code in [error-contract](error-contract.md) is unimplemented,
including `lock.stale`, `path.traversal`, and `write.incomplete`.

### Decisions this implementation had to make

The error contract deferred four of its reserved conditions to an implementing
change, and the commands below decided them and added fourteen further codes
and rules additively under the contract's compatibility rule. All eighteen are
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

The import back decided five more, all in the `export` bucket, because they
are conditions of a directory outside the archive rather than of the archive:

1. A source that holds no `manifest.json` is `export.manifest_missing`, and
   one whose manifest this build cannot read as a manifest of its format is
   `export.manifest_malformed`. Both carry `scope` `export_source` and the
   additive `export_path`, which is the manifest's fixed name. A manifest
   written under another archive schema version is neither: it is refused by
   the schema rules, because it is a document this build has no business
   reading rather than a broken one.
2. `export.object_mismatch` covers an exported copy the source does not hold
   as well as one whose bytes disagree with the manifest, because the manifest
   is authoritative and both say the export is not what it claims to be. Its
   `details` carry `scope`, the expected `digest`, a `conflict_count`, and the
   additive `reason`, whose closed set is `absent`, `digest`, `length`, and
   `unusable`, the last for a path that is there and is not a regular file.
   The digest is the manifest's own name for the object, which the privacy
   rule already permits; nothing about the bytes actually found is reported.
3. `export.record_missing` names a record document the manifest lists and the
   source does not hold, with `scope`, `record_kind`, and `path_count`. A
   document that is there and cannot be read as a record of its kind is
   `record.malformed` instead, exactly as it would be inside an archive.
4. `export.record_conflict` is a record identifier the target archive holds
   for a different record. Its `details` carry `record_kind` and
   `conflict_count` and never the identifier, which would name a record of the
   archive rather than of the export. A byte-identical record already there is
   not a conflict: it is the same record, so the import writes nothing for it.
   That rule, and the content-addressed store's own treatment of a duplicate,
   are what make a second import of one export change nothing.
5. An import event openPapir writes for a restored object carries the additive
   `source` `export`. The field is absent on every event `import` writes, so
   an archive written by an earlier build reads unchanged and an event without
   it is the user's own import of a local file. The event carries an empty
   original filename, because an export names its objects by digest alone and
   holds none; the filename the user's own import recorded is restored with
   the exported import-event record that carries it.

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
| `record.malformed` | A record `case delete` keeps names an artefact by something that is not a digest, so the archive cannot say which object it means. Emitted by `case delete`, with `stage`, `malformed_count`, the archive-wide number of such references the scan read, and `withheld_count`, the number of this case's candidate objects they held back, and only where it held an object of that deletion back: `--purge` was given and this case had a candidate the purge would otherwise have removed. Those objects are retained as `referenced_elsewhere`, and the records the deletion planned to remove still go. Without `--purge` a candidate keeps `purge_not_requested` and no warning is emitted. |
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
something it did not create. The refusal the handle raises is a sentinel type
of openPapir's own, so it is told apart from any other error of the same kind
rather than by its message.

openPapir builds for Unix and Windows targets only. A target with neither
no-follow rule fails to compile with a stated reason, because the archive has
no weaker mode to fall back to and a pre-open check of the path would be a
time-of-check-to-time-of-use gap rather than a defence. Among the Unix
targets, Linux and macOS report a refused `O_NOFOLLOW` open as `ELOOP` while
FreeBSD and DragonFly report it as `EMLINK`; both are accepted, although
neither BSD is a target this project builds or tests for today.

## Privacy of output

`message`, `details`, `data`, human-readable output, and stderr may carry
counts, byte lengths, cap values, input positions, bucket names, error codes,
archive-relative paths, digests of stored artefacts, identifiers openPapir
minted, and timestamps openPapir recorded.

They never carry an original filename or any form of one, a user-supplied path
including the archive root, payload bytes or excerpts, a hostname, a username,
or a process owner. The single exception is the export directory in human
output: `case export` prints the `--to` argument and `case import` the
`--from` argument the user typed in the same invocation, back to them, so that
they can see where their copy went or came from. Neither reaches `data`,
`message`, `details`, or stderr, and no other user-supplied path is echoed
anywhere. A receipt label and an evidence statement are the user's
own text: they appear in `data` and in human output, which report the user's
own record back to them, and never in a `message`, in `details`, or in any
refusal. The `statement` an `association retire` stores from `--reason` is the
user's own text too, and it appears in `data` alone: the human form of a
retirement does not print it back, and no message, warning, or count repeats
it. Which input failed is answered by `input_index`, never by
a name. The original filename is stored as an attribute of the import event
record only.

### Human output language

Human output is English only, in every command and on both streams. That is a
decision rather than an omission, and it is recorded here so that the next
command family does not have to reopen it.

The surface it applies to is larger than a single command's output suggests.
`crates/openpapir-cli/src/report.rs` holds 61 message templates, of which 57
carry English words and 4 are pure column layout. Outside it, 57 distinct
diagnostic sentences reach `message` from 60 construction sites in the two
crates, and 74 `clap` attributes carry the help text of the commands, the
subcommands, and their flags. The 24 golden cases pin 48 human-form files,
`human.txt` and `human.stderr.txt` for each. Roughly 190 English strings are
in scope today, and the count grows with every command family added.

Five reasons decide it that way.

1. The parser's own text is not ours to translate. `clap` renders the usage
   block, the argument-parser refusals, and the `--help` and `--version` output
   from strings it owns, and it offers no message table to replace them. The
   goldens already pin two of those lines under `usage.arguments`. A locale
   switch would therefore leave the first text a user meets when they get an
   invocation wrong in English, or force a hand-written renderer for help and
   for every parser refusal, which is a larger commitment than translating the
   result lines.
2. The wording carries the disclaimer load. Lines such as the closing sentence
   of every record command exist to keep the tool from being read as a claim of
   delivery, receipt by an authority, authenticity, or legal effect. A
   Hungarian rendering of those sentences has to be as careful in Hungarian
   administrative vocabulary as the English is in English, and a rendering
   that is merely fluent could imply exactly what the sentence exists to deny.
   That review is not a spell check; it needs a reader who knows the domain.
3. Every added language doubles the machine-checked human surface. A second
   language means 48 more golden files, a language axis in the harness, and a
   reviewer who reads that language for every diff those files show. A golden
   nobody in the review can read is a golden that is regenerated rather than
   read, which is the failure the golden directory exists to prevent.
4. The review has no owner. The release rule for a translated build would be a
   native reader signing off before each release. There is no published
   release and no reviewer who has committed to that recurring work, so
   adopting the rule now would mean adopting a rule that cannot be kept.
5. No integrator is affected. The JSON form is the scripting contract: field
   names, `command`, `error.code`, and the exit codes are stable identifiers
   and stay English whatever the human form does. A caller branches on the
   code and the exit code, never on `message` wording, so nothing that reads
   openPapir programmatically depends on this decision at all.

Only the fourth reason is about effort. The first three would still apply to a
funded translation, and they are why the answer is not simply "later, when
there is time".

The path a demand for Hungarian output should take is a compiled-in message
table, and naming it here is the point of recording the decision. It would be
a module in `openpapir-cli` with one function per message, each taking typed
arguments and returning the rendered line, so the format strings stay in the
source and no format string ever comes from data. It needs no new dependency:
the selected language is one enum value threaded from the command layer, from
`--lang hu|en` or `OPENPAPIR_LANG`, and the functions match on it. Under such
a table the rules are that every human line goes through it with no literal
left at a call site, that goldens exist for each language, and that a native
reader reviews the Hungarian before each release. Adopting it is a ticket per
command family so that no single change has to move all 61 templates at once.
The trigger is a stated need from a user who reads Hungarian together with a
named reviewer who accepts the per-release review; until both exist, the
answer stays English only.

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
