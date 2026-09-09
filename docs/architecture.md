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
| `openpapir-core` | The local archive: marker, artefact store, atomic writes, single-writer lock, input caps, path safety, and import-event records. |
| `openpapir-cli` | Argument parsing, the response envelope, and the exit-code mapping. |

Only these invocations are supported:

```sh
openpapir --help
openpapir --version
openpapir capabilities [--json]
openpapir archive init <root> [--json]
openpapir import --archive <root> <file>... [--json]
```

Two operations are implemented, `archive.init` and `import`, and those are the
two names `capabilities` reports. Everything else in
[local archive layout and storage design](archive-layout.md) and
[import error, JSON, and exit-code contract](error-contract.md) remains a
design: no cases, submissions, receipts, associations, derived metadata, or
verification records; no export, deletion, integrity check, matching, receipt
parsing, or migration.

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
| `data` | The command's result. `{}` when `ok` is `false`. |
| `verified` | Always `false`. No cryptographic check is implemented. |
| `error` | Present exactly when `ok` is `false`: `code`, `message`, `details`. |
| `warnings` | Present only when a platform degradation was observed. |

`code` is the only field a caller may branch on. `message` wording may change
within a `schema_version`. `details` carries at most 16 keys, whose values are
strings, integers, booleans, or arrays of at most 16 such scalars, and always
carries `bucket`. Changes within `schema_version` are additive only.

The capabilities response is unchanged in shape and now lists the two
implemented operations:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "scaffold",
    "operations": ["archive.init", "import"]
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

## Storage guarantees

| Guarantee | How it is kept |
| --- | --- |
| Atomic write | The content is written to a staging file inside the archive root, flushed, linked into place, and the destination directory is flushed. The staging name is then removed. |
| Never overwrite | The publish step is a hard link, which fails rather than replacing an existing file, so a destination openPapir did not create is refused as `path.overwrite`. |
| Interrupted write | A leftover staging file is never adopted, so the archive holds the complete file or nothing. |
| One filesystem | The root and its layout directories must share one device. A cross-device publish is refused as `path.cross_device`. |
| Owner-only | Directories are created `0o700`, files `0o600`, and stored objects become `0o400`. An archive whose permissions are wider is refused as `archive.permissions_wide`, with no override flag. |
| Path safety | Input files are opened with the platform's no-follow flag, symbolic links inside the archive are refused, and a user-supplied filename is never joined into a path. |
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

## Implemented codes and exit codes

The exit code carries the error's bucket and nothing else. `1` is never
emitted.

| Exit | Buckets | Implemented codes |
| --- | --- | --- |
| `0` | Success, including a duplicate import and a warning | |
| `2` | `usage` | `usage.arguments`, `usage.archive_root_missing` |
| `3` | `input`, `path` | the five caps above, `path.symlink`, `path.overwrite`, `path.cross_device` |
| `4` | `archive`, `lock`, `write`, `integrity` | `archive.marker_missing`, `archive.marker_malformed`, `archive.adopt_refused`, `archive.schema_newer`, `archive.schema_older`, `archive.permissions_wide`, `archive.multiple_filesystems`, `lock.held`, `write.interrupted`, `integrity.length_mismatch` |
| `5` | `platform` | None. The named degradations are warnings, and `platform.filesystem_unsupported` is not detected yet. |
| `6` | `internal` | `internal.unexpected` |

An invocation the argument parser rejects exits `2` with the parser's usage
text on stderr and no envelope, as it did before.

Every other code in [error-contract](error-contract.md) is unimplemented,
including all `export`, `delete`, and `record` codes, `lock.stale`,
`path.traversal`, `write.incomplete`, `record.malformed`,
`integrity.digest_mismatch`, and `integrity.orphan_object`.

### Decisions this implementation had to make

The error contract deferred three conditions to the implementing change. They
are decided as follows, and no other reserved code became reachable:

1. A marker that cannot be read is `archive.marker_malformed`. It is reported,
   never repaired, and every operation on that archive is refused.
2. Initialising a directory that already holds a marker is `path.overwrite`,
   because the marker would have to be replaced. A non-empty directory with no
   marker stays `archive.adopt_refused`.
3. A lock whose holder is gone is still `lock.held`. No takeover exists, silent
   or explicit, so `lock.stale` stays reserved.

An import-event record that cannot be parsed is skipped when counting a
duplicate's history rather than reported, because `record.malformed` is
reserved. The reported `previous_import_count` is therefore a count of the
readable events.

## Platform degradation

Where a guarantee weakens, the condition is reported as a warning in the
envelope and never silently accepted. `warnings` never changes `ok` and never
changes the exit code.

| Code | Condition |
| --- | --- |
| `platform.no_directory_fsync` | The directory entry a publish created may not be durable, although the file content was flushed. Emitted on platforms with no directory flush, with `stage`. |
| `platform.owner_only_via_acl` | Owner-only access is an access-control list rather than a permission bit, so it depends on the filesystem. Emitted on Windows. |

On Windows the no-follow flag has no portable equivalent, so a symbolic link is
detected by a preceding check rather than by the open itself, which is the
weaker guarantee the design records rather than a faked Unix semantic.

## Privacy of output

`message`, `details`, `data`, human-readable output, and stderr may carry
counts, byte lengths, cap values, input positions, bucket names, error codes,
archive-relative paths, digests of stored artefacts, identifiers openPapir
minted, and timestamps openPapir recorded.

They never carry an original filename or any form of one, a user-supplied path
including the archive root, payload bytes or excerpts, a hostname, a username,
or a process owner. Which input failed is answered by `input_index`, never by
a name. The original filename is stored as an attribute of the import event
record only.

## Planned ownership

openPapir will own persistent cases, submission relationships, receipt
associations, and the user workflow around them. The storage technology and
the on-disk layout are decided in
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

Only the first is implemented, and it asserts nothing beyond storage. Matching
and verification are design requirements with no code behind them. Association
cannot imply authenticity, successful delivery, or legal effect. Delegated
verification must identify the attachment or receipt covered, the verifier, and
its trust context.

## Integration boundary

No government submission API is assumed. KRX creation alone establishes no
ability to submit a package to e-Papír. Automatic sending, authentication,
background services, and a web interface are outside the initial foundation.
An integration needs separate discovery of authorised access, actual contracts,
and recovery semantics before a scoped implementation proposal.
