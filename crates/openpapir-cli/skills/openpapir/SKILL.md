---
name: openpapir
description: >-
  Keep Hungarian government correspondence in one local, offline archive with
  the openpapir CLI: create the archive, import files with their bytes
  preserved, record cases, submissions, receipts and the user's own assertions
  about them, withdraw an assertion, look at one submission, receipt or
  association on its own, update a case's title, notes, status and
  tags, search and filter the case list, check the archive against its
  records, see what is in it and where to look for the submission receipts
  still outstanding, copy one case out, delete one case, and narrow a restored
  archive back to owner-only. Use whenever a
  task involves organising what was sent to an authority and what came back,
  without uploading anything.
license: MIT
compatibility: Requires the openpapir CLI, 0.1.0-dev.0 or later, on PATH.
metadata:
  author: watt-mind
  version: "1.0"
  source: https://github.com/watt-mind/openPapir
---

openpapir is a command-line tool for a local-first correspondence archive. It
creates an archive in a directory the user names, stores files in a
content-addressed object store that preserves their bytes exactly, and records
the user's own cases, submissions, receipts, and assertions about them as
plain JSON documents. It re-digests what it holds on request, copies one case
out as plain files, and narrows a restored archive's permissions back to
owner-only.

It is an independent project. It is not the government's e-Papir service, it
submits nothing, and it opens no socket at all.

## When to use it, and when not

Use it to build or inspect a local archive of correspondence: importing files,
recording what the user says they sent, recording an artefact the user
believes to be a receipt, recording the user's own link between the two,
withdrawing such a link when the user says it no longer stands, reading back
one submission, receipt, or association with what relates to it,
keeping a case's own title, notes, status, and tags current, finding a case
again by status, tag, or a substring of its title or notes, checking storage
integrity, summarising what the archive holds and what is still worth looking
for, exporting one case, importing such an export back into an archive, and
deleting one case when the user asks for that by name.

Do not use it to send anything, to decide whether a document is genuine, to
read what a receipt says, or to match a receipt to a submission
automatically. None of that is implemented, and nothing in this tool may be
reported as if it were.

## Rules that always apply

1. **Always pass `--json`** and read the envelope. The human text is for
   people; only the JSON is a contract. Exactly one compact object reaches
   stdout in the `--json` form, and stderr stays empty.
2. **Branch on the exit code first, then on `error.code`.** Never parse
   messages: codes are stable within a `schema_version`, wording is not.
3. **Every record is the user's own statement.** openpapir sends nothing and
   reads no artefact bytes to form an opinion, so a submission is what the
   user says they sent, a receipt is a file the user believes to be one, and
   an association is what the user asserts. Report them in those words.
4. **`verified` is `false` in every envelope this build emits.** A SHA-256
   digest here identifies bytes. It says nothing about authenticity, origin,
   delivery, or legal effect.
5. **Private material stays private.** Output carries no original filename,
   no user-supplied path, and no payload byte, and neither may your report.
   The one path any output carries is the export directory in human mode, the
   `--to` of an export or the `--from` of an import, because the user typed it
   in the same command.
6. **The archive root is always explicit.** openpapir never searches for an
   archive, never adopts a directory that has no marker, and never creates
   one as a side effect of another command.

## The boundary, stated once

Imported, matched, and authenticity-verified are three separate states, and
only the first two exist here.

- **Imported** means bytes were accepted into the local store. It says
  nothing about what they are.
- **Matched** means the user recorded an association between a receipt and a
  submission. It is their assertion, it changes nothing about the artefact,
  and it creates no verification result.
- **Authenticity verified** has no code behind it at all. Nothing openpapir
  prints may be reported as authenticity, successful delivery, receipt by an
  authority, or legal effect.

`archive check` re-digests stored bytes. A passing check is storage
integrity: the bytes are the bytes their paths name. It is never the third
state. `.es3` dossier verification belongs to openSzigno and KRX container
processing to openKRX; neither is part of this tool.

## Check the tool, and install this skill

Run `openpapir --version`. The binary carries this document, so installing
the skill needs no checkout.

```sh
mkdir -p .claude/skills/openpapir
openpapir skill > .claude/skills/openpapir/SKILL.md
```

Use `.codex/skills/openpapir/` for Codex, or `~/.claude/skills/openpapir/` to
install it for every project instead of one. `openpapir skill` takes no file
and no `--json` and writes the document and nothing else.

The binary also generates its own shell completions and its own man page,
both to stdout and neither touching an archive:
`openpapir completions bash|zsh|fish|powershell|elvish` writes one shell's
completion script, and `openpapir manpage` writes the man page for the whole
command tree as one roff stream. A shell outside that set is a usage refusal.
Offer them when the user is setting the tool up; nothing else needs them.

Check the exit code of that redirection. It is `0` when the document was
written, and `0` too when a reader such as `head` closed the pipe, which is
not a failure. It is `4`, the `write` bucket's code, when the destination
could not take the bytes, a full disk most often, with one line on stderr
and no path in it. A `4` here means the file you just redirected into is
truncated or empty: do not install it.

## The envelope and the exit codes

```json
{ "schema_version": 1, "ok": true, "command": "case.create",
  "data": { "...": "command specific" },
  "verified": false }
```

| Key | Meaning |
| --- | --- |
| `schema_version` | The envelope's version, currently `1`. |
| `ok` | `true` only when the command completed its stated work. |
| `command` | The stable command name, for example `association.create`. |
| `data` | The result. `{}` when `ok` is `false`, except `archive.check` and `case.delete`, whose counts are the work they completed. |
| `verified` | Always `false` in this build. |
| `error` | Present exactly when `ok` is `false`: `code`, `message`, `details`. |
| `warnings` | Present only when a platform guarantee was weaker than designed. |

`error.details` always carries `bucket`, and the exit code carries that
bucket and nothing else.

| Exit | Bucket | Meaning |
| --- | --- | --- |
| `0` | Success, a duplicate import and a warning included. | Read `data`. |
| `2` | `usage` | The command line is wrong. Fix it. |
| `3` | `input`, `path` | A cap was exceeded, or a path was a link or would be overwritten. |
| `4` | `archive`, `lock`, `write`, `record`, `integrity`, `export`, `delete` | The archive or a record refused the work. |
| `5` | `platform` | The filesystem cannot host an archive. |
| `6` | `internal` | An unexpected failure. Report it. |

`1` is never emitted. An invocation the argument parser rejects is the same
contract: with `--json` it is one `usage.arguments` envelope on stdout, and
without it the parser's own usage text on stderr. Both exit `2`. `--help` and
`--version` exit `0`.

`warnings` never changes `ok` and never changes the exit code. The codes are
`platform.no_directory_fsync`, `platform.owner_only_via_acl`, and
`platform.no_follow_after_open`; the last two are reported on Windows, where
owner-only access is an access-control list and a no-follow open opens the
link itself.

## The archive lifecycle

### 1. Create the archive

```sh
openpapir archive init ./archive --json
```

The directory must already exist and be empty. `data` holds `archive_id` and
`archive_schema_version`. Creation narrows the root to owner-only; nothing
here ever widens a permission. A directory that is not empty is
`archive.adopt_refused`, and one that already holds a marker is
`path.overwrite`.

### 2. Import files

```sh
openpapir import --archive ./archive ./letter.pdf ./receipt.es3 --json
```

Each file's bytes are stored unchanged under their digest, and one import
event is recorded per input. `data` holds `imported`, `duplicates`, and
`artefacts[]`, each with `digest` (`sha256:<64 hex>`), `byte_length`,
`import_event`, and `created_object`. Re-importing the same bytes is not an
error: the entry adds `previous_import_count` and `first_imported_at`, the
object is left untouched, and the run still exits `0`. An import-event count
is history, not an anomaly.

Read `artefacts[].digest`: it is the handle every later command uses. The
original filename is stored as an attribute of the import event and appears
in no output.

Prefer the one-step form when the user is recording something they sent. It
imports and records in one command, under one writer lock, so no digest has
to be carried between two calls:

```sh
openpapir import --archive ./archive ./form.pdf ./annex.pdf \
  --case <case-id> --description "Posted the completed form." \
  --date 2026-01-13 --json
```

`--case` needs `--description`; without it the run is `usage.arguments` and
exits `2`. Every imported file is referenced with the role `attachment`, and
`data` holds everything plain import reports plus `submission`. Use
`submission add --file` instead when a file needs a role of its own. Use
plain `import` only when there is no case to file the bytes under yet.

### 3. Record a case

```sh
openpapir case create --archive ./archive --title "Tax matter" \
  --notes "First contact." --tag tax --status open --json
openpapir case list --archive ./archive --status open --tag tax \
  --query "office" --json
openpapir case show --archive ./archive <case-id> --json
```

A case is the user's own folder. It corresponds to nothing any authority
issues. `case create` returns `data.case` with `id`, `title`, optional
`notes`, `status`, `tags`, `created_at`, `record_kind`, and
`archive_schema_version`. `--status` is `open` or `closed` and is `open` when
it is not given; `--tag` is repeatable, and the tags are stored sorted and
deduplicated.

`case list` returns `cases[]` ordered by identifier and `count`, which is how
many the listing holds and so, under a filter, how many matched; an empty
archive and a filter that matched nothing are both `count` `0` and exit `0`.
`--status` keeps one status, `--tag` is repeatable and every tag given must be
on the case, and `--query` keeps cases whose title or notes contain the text,
compared without regard to case. There is no index: `--query` is a substring
match in one linear scan, so its cost grows with the number of cases. The
query is the user's own text and appears in no output, so never quote it back
from a result.

`case show` returns `case`, `submissions[]` ordered by identifier,
`submission_count`, and `receipts[]`. An identifier that names no case is
`record.not_found`.

`receipts[]` is every receipt whose live association names a submission of the
case, each entry holding `receipt`, `association_id`, `outcome`, and
`submission_ids`. Only the live head of a supersession chain counts, so
retiring an assertion takes the receipt out of the section while both records
stay stored. It is the user's own assertion read back: never report an entry
there as a delivery, a receipt by an authority, or a verified match.

A case record written by an earlier build reads as `open` with no tag, and
`updated_at` is absent until an update sets it.

### 3a. Update a case

```sh
openpapir case update --archive ./archive <case-id> --title "Tax appeal" \
  --status closed --tag appeal --untag tax --json
openpapir case update --archive ./archive <case-id> --clear-notes --json
```

This is the one invocation that rewrites a stored record, and it rewrites only
the case record. The record keeps its `id` and its `created_at` and gains an
`updated_at`. What may change is the user's own filing: `--title`, `--notes`
or `--clear-notes` (never both), `--status`, and `--tag` and `--untag`, both
repeatable. Removing a tag the case does not carry changes nothing and is not
an error, and a `--untag` value is never held to the tag caps, because a value
no case could carry is simply not on this one. A tag both added and removed in
one invocation stays on the case: the removal is applied first and the
additions after it, so the add is the request that wins.

`data` holds `case`, the whole record as it now stands, and `changed`, the
sorted names of the fields that changed and nothing more: report the names,
and read the new values from `case` rather than describing what they were.

An update that would leave the record exactly as it is, whether it named
nothing at all or only values the record already holds, is `usage.arguments`
with `argument` `update` and exit `2`; nothing is written.

### 4. Record a submission

```sh
openpapir submission add --archive ./archive --case <case-id> \
  --description "Posted the completed form." --date 2026-01-13 \
  --file './form.pdf:cover letter' --file ./annex.pdf --json
```

Prefer `--file`: it imports the file and references it in the same record,
under one writer lock, so no digest has to be carried between two calls. It
is repeatable, takes `<path>` or `<path>:<role>`, and gives a file with no
role of its own the role `attachment`. The role is the text after the last
colon, so a path holding a colon keeps it as long as what follows carries a
path separator. `data.imported` then holds what plain import reports for the
files this call stored, and is absent when no `--file` was given.

Use `--artefact` for bytes the archive already holds:

```sh
openpapir submission add --archive ./archive --case <case-id> \
  --description "Posted the completed form." --date 2026-01-13 \
  --artefact 'sha256:<digest>:cover letter' --json
```

`--artefact` is repeatable and is `sha256:<64 hex>` or
`sha256:<64 hex>:<role>`, and the two flags may be combined: the artefacts
are referenced first, then the files. `--date` is `YYYY-MM-DD`, is stored
verbatim, and is never compared, interpreted, or read as a delivery or
receipt date. `data.submission` holds `case_id`, `description`, optional
`stated_date`, and `artefacts[]` of `{digest, role?}`. A digest that names no
stored object is `record.not_found` with `record_kind` `artefact`. A file
that cannot be read is `usage.arguments`, and no submission record is written
when an import is refused.

```sh
openpapir submission show --archive ./archive <submission-id> --json
```

`submission show` returns `submission`, `associations[]`, and
`association_count`. The associations are the ones naming this submission, as
a candidate or as the `submission_id` an `associated` outcome confirms, live
heads first and superseded records after them. An identifier that names no
submission is `record.not_found`.

### 5. Record a receipt

```sh
openpapir receipt add --archive ./archive --artefact sha256:<digest> \
  --label "Envelope from the post" --json
openpapir receipt list --archive ./archive --json
openpapir receipt show --archive ./archive <receipt-id> --json
```

This records that the user believes one stored artefact to be a receipt. The
bytes are never opened and never parsed. `data.receipt` holds
`artefact_digest`, `import_event_id`, and an optional `label`. With no
`--import-event` the earliest import event for that digest is recorded; one
that records another artefact is `record.inconsistent` with rule
`import_event_digest_mismatch`.

`receipt show` returns `receipt`, `associations[]`, and `association_count`.
The history is the one `association list` returns for the same receipt, in the
same order. An identifier that names no receipt is `record.not_found`.

### 6. Record what the user asserts

```sh
openpapir association create --archive ./archive --receipt <receipt-id> \
  --outcome candidate \
  --candidate '<submission-id>:moderate:The reference matches.' --json
openpapir association list --archive ./archive --receipt <receipt-id> --json
openpapir association show --archive ./archive <association-id> --json
openpapir association retire --archive ./archive <association-id> \
  [--reason 'The user withdrew it.'] --json
```

`--outcome` is `unassociated`, `candidate`, `associated`, or
`contradictory`. All four are results, none is an error, and all four exit
`0`; `contradictory` least of all, because keeping conflicting evidence is
the designed behaviour. `--candidate` is repeatable and is
`<submission-id>:<confidence>:<statement>`, split on the first two colons
only. `confidence` is the ordinal set `weak`, `moderate`, `strong` and is
never a number, because no calibration data exists.

| Rule broken (`record.inconsistent`) | What it means |
| --- | --- |
| `unassociated_has_candidates` | `unassociated` was given a candidate. |
| `candidate_requires_candidates` | `candidate` was given none. |
| `associated_requires_one_candidate` | `associated` needs exactly one. |
| `contradictory_requires_two_candidates` | `contradictory` needs at least two. |
| `duplicate_candidate_submission` | A submission was named twice. |
| `supersedes_other_receipt` | The superseded record is another receipt's. |
| `already_superseded` | The record a retirement names is superseded already. Retire the newest record of the history instead. |

`data.association` holds `outcome`, `candidates[]` with their `evidence[]`,
`created_by` (always `user`), `submission_id` (the confirmed submission, set
only for `associated`, otherwise `null`), and `supersedes`. Records are
append-only, as every record kind but the case record is: `--supersedes` names
an earlier association for the same
receipt, and the superseded record is never modified or removed.
`association list` returns the whole history newest first, superseded records
included, with `associations[]`, `count`, and `receipt_id`.

`association show` returns one record with the chain it belongs to:
`association`, `live`, `chain[]`, and `chain_length`. `chain[]` is what the
record supersedes and what supersedes it, newest first, and `live` is `true`
only when no stored record supersedes it. Read `live` before reporting what
the user asserts today, because a superseded record is history.

`association retire` withdraws an assertion the user no longer stands behind.
It writes a new record for the same receipt with outcome `unassociated`, no
candidate, and `supersedes` naming the record the user retired, so both
records stay and nothing is edited or removed. `--reason` is the user's own
text, at most 512 bytes; it is stored on the new record as `statement` and is
never repeated in a message. Retire an association when a `case delete` was
refused with `delete.record_entangled`, and only after the user has said they
want the assertion withdrawn.

### 7. Check the archive

```sh
openpapir archive check --archive ./archive --json
```

Read-only in the strongest sense: no lock is taken, every file is opened with
the platform's no-follow flag, and nothing is created, renamed, removed, or
repaired. `data` holds `bytes_digested`, `objects_checked`,
`objects_unchecked`, `orphan_objects`, `records_checked`, `records_unchecked`,
`staging_files`, and `problems[]`, one entry per code with a `count`,
including the codes it did not see, ordered by code.

A clean archive exits `0`. When something is found, `ok` is `false`, the
report stays in `data`, and `error` names the first problem in this fixed
precedence: `path.symlink`, `record.malformed`, `integrity.digest_mismatch`,
`integrity.length_mismatch`, `integrity.dangling_reference`,
`integrity.orphan_object`. The report carries counts only: never the path,
the name, or the digest of a damaged object. Report the counts and the code,
and never invent which file it was.

### 8. See what is in the archive, and what is left to look for

```sh
openpapir archive status --archive ./archive --json
openpapir archive status --archive ./archive --as-of 2026-02-10 --json
```

Run this at the start of a session to orient yourself, and whenever the user
asks what is outstanding. It is read-only in the same sense as
`archive check`: no lock, nothing written, and no artefact byte read.

`data` holds `as_of`, `cases`, `cases_by_status[]`, `submissions`,
`receipts`, `associations`, `stored_objects`, `undated_submissions`,
`retention_window_days`, and `receipts_to_retrieve[]`, each entry with
`case_id`, `submission_id`, `submission_date`, `retrieve_by`, and `days_left`,
ordered by `retrieve_by`. `cases_by_status[]` holds one `{count, status}` per
status in the closed set, including a status no case holds, and sums to
`cases`. A case's status changes no reminder: a submission in a closed case is
listed exactly as one in an open case.

A submission is listed when it carries a user-supplied date, no live
association with outcome `associated` or `candidate` names it, and its date
plus the window is on or after the as-of date. Live means the head of the
supersession chain: a record another record supersedes is still in the history
and is still returned by `association list`, but it no longer says what the
user asserts today, so `association retire` on an `associated` record
withdraws the assertion and the reminder comes back.
`days_left` `0` is the last open day. Submissions with no usable date are
counted in `undated_submissions` and never listed. `--as-of` is `YYYY-MM-DD`,
defaults to today, and accepts a past or future date as a what-if; an unusable
one is `usage.arguments` with `argument` `as_of`.

The 30-day window is the operator's own published description of the personal
delivery storage, retrieved 2026-09-09, and it is descriptive rather than a
rule openpapir applies. Report an entry as a reminder to go and look in the
delivery storage for a submission receipt while the operator says one would
still be there, never as a statement that one is there. Never report it as a
deadline openpapir enforces, as proof that a receipt exists, as delivery, as
receipt by an authority, or as legal effect,
and never claim openpapir looked in any mailbox or checked any service. It
read the local archive and did arithmetic on the date the user typed.

### 9. Export one case

```sh
openpapir case export --archive ./archive --case <case-id> --to ./out --json
```

A plain copy outward: the objects are the original bytes named by their
digest, the records are the archive's own JSON, and `manifest.json` lists
both. Nothing is converted, compressed, or encrypted, no hard link is made,
and the archive is not changed. The destination must be an empty directory or
one the export creates, and it is never inside the archive root. `data` holds
`case_id`, `object_count`, `record_count`, `bytes_copied`, and `records[]`
per kind; it never holds the destination. An export that fails removes
exactly what it created.

`export.destination_conflict` means the destination already held something,
and `export.copy_mismatch` means a copy re-digested to something else and was
removed.

### 9a. Import one export

```sh
openpapir case import --archive ./archive --from ./out --json
```

The same plain copy inward. The manifest is authoritative: every object it
lists is re-digested and every record it lists is read before the archive is
written to at all, and the record set is then published all at once or not at
all. A record keeps the identifier it had, an object or a record the archive
already holds is not an error and is not written again, and each object the
import stores gets one new import event with `source` `export`. Importing one
export twice therefore leaves the same archive. `data` holds `case_id`,
`object_count`, `objects_stored`, `objects_present`, `bytes_stored`,
`records_written`, `records_present`, `records[]` per kind, and
`events_recorded`; it never holds the source. A case whose objects come to
more than 512 MiB in total meets `input.cap.import_bytes`: the per-operation
cap binds a restore as it binds an import.

`export.manifest_missing` and `export.manifest_malformed` mean the directory
is not an export this build can read, `export.object_mismatch` that a copy is
absent or is not the object the manifest describes, `export.record_missing`
that a record the manifest names is not there, and `export.record_conflict`
that the archive holds a different record under one of the identifiers. Every
one of them leaves the archive exactly as it was.

### 9b. Export and import a whole archive

```sh
openpapir archive export --archive ./archive --to ./whole --json
openpapir archive import --archive ./archive --from ./whole --json
```

The same plain copy at the other scope: every object the store holds, every
record of every kind, one manifest, and a copy of the archive marker as
`papir-archive.json`, so the schema version travels with the copy. The
objects come from the store rather than from what the records reference, so
an object no record names is copied too. `data` holds `case_count` where a
case export holds `case_id`, and never the destination or the source.

`archive import` restores the whole export as one set: all of it or none of
it. A record identifier a different record already holds refuses the import
before anything is written, exactly as it does for one case. Each import
reads its own kind of export and refuses the other's with
`export.manifest_malformed`, so hand `case import` a `case export` directory
and `archive import` an `archive export` one.

An archive whose objects come to more than 512 MiB in total meets
`input.cap.import_bytes` on the way back in. For an archive that large, a
plain copy of the archive root is the backup, followed by
`archive repair-permissions`.

`integrity.digest_mismatch` from `archive export` means an entry under
`objects/` is not an object filed under its own digest; run `archive check`
to see the whole picture before doing anything else.

### 10. Delete one case

```sh
openpapir case delete --archive ./archive --case <case-id> --json
openpapir case delete --archive ./archive --case <case-id> --purge --json
```

This is the only destructive invocation openpapir has, and the only one that
can remove an object, which it does only when `--purge` says so in as many
words. Never add `--purge` on your own initiative: without it no object is
touched at all, and the objects that would become unreferenced are counted as
retained instead. Confirm with the user before either form, and say plainly
that nothing here can bring an object's bytes back.

What goes with the case: every submission recorded against it; every
supersession chain of associations that names a departing submission and
whose newest record names no submission that remains, the withdrawn history
of a retired assertion included, counted under `records_removed` as
`association`; every receipt an association tied to a departing submission
that no remaining association still names. An
import event is history and is kept, unless `--purge` removed the object it
describes. The whole archive is read first, under the writer lock, and the
removal set is decided before a single file is unlinked.

`data` holds `purge`, `records_removed[]` per kind with
`records_removed_total`, `objects_removed`, `objects_retained[]` per reason
with `objects_retained_total`, and `records_retained`. The reasons are
`purge_not_requested`, `referenced_elsewhere`, `records_retained`, and
`unremovable`. No digest, path, or filename appears anywhere: recording the
fingerprint of content the user asked to purge would defeat the purge, which
is why no deletion record is written and no audit log is kept.

| Code | Meaning |
| --- | --- |
| `delete.record_entangled` | A live association names submissions in this case and in another. Nothing was touched, and `retained_count` counts the live records in the way, naming none of them. Ask the user whether to withdraw the assertion with `association retire`; until one of them does, neither case can be deleted. |
| `record.inconsistent` with rule `supersedes_cycle` | The stored association records supersede each other in a cycle, so the history has no live record and the deletion cannot tell what the user asserts. Nothing was touched. Only a hand-edited archive holds one; report it and do not guess. |
| `delete.records_retained` | A record unlink was refused, so the object pass never ran. `data` keeps the counts, `ok` is `false`, and the exit code is `4`. |
| `delete.objects_retained` | Only the purge fell short. The same shape. |

Deletion unlinks files. It does not erase data from the storage medium, and a
backup already taken is outside openpapir's reach. Say both when you report a
deletion.

### 11. Repair permissions after a restore

```sh
openpapir archive repair-permissions --archive ./archive --json
```

Ordinary copy tooling widens permissions when a backup or an export is
restored, and the owner-only rule then refuses the archive. This narrows the
root, the marker, every layout directory, every record, and every object back
to owner-only and reports `paths_checked`, `paths_changed`, and `changed[]`
per kind. It only ever narrows, it reads no file content, and it refuses a
symbolic link inside the archive rather than narrowing it.

## Hard limits

No flag, environment variable, or configuration relaxes any of these.

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

One writer at a time: a second writer refuses with `lock.held` rather than
waiting, and there is no takeover. Symbolic links inside the archive are
refused as `path.symlink`, on Windows together with every other reparse
point. An archive lives on one filesystem, and a filesystem that cannot
create a hard link cannot host one (`platform.filesystem_unsupported`).

## What this tool never does

- It opens no socket. There is no network access and no background work.
- It submits nothing and delivers nothing. No government integration exists.
- It verifies no signature and asserts no authenticity or legal effect.
- It parses no receipt, derives no metadata, and matches nothing on its own.
- It edits no stored record but the case record, which `case update` rewrites
  in place; it migrates nothing and never deletes a single submission,
  receipt, or archive. `case delete` is the one
  removal it performs, and only when asked for by name.

## Reporting to the user

Say, in this order: what the archive now holds (counts, identifiers, digests);
which of those facts are the user's own statements rather than findings; what
a check or an export actually proved, which is that bytes are the bytes their
paths name; and any refusal by its code and bucket, with the count the report
gave and no invented detail. Never call a receipt genuine, a submission
delivered, or a match a verification.

## Quick reference

```sh
openpapir capabilities --json
openpapir archive init ROOT --json
openpapir archive check --archive ROOT --json
openpapir archive status --archive ROOT [--as-of YYYY-MM-DD] --json
openpapir archive repair-permissions --archive ROOT --json
openpapir import --archive ROOT FILE... [--case CASE_ID --description D \
  [--date YYYY-MM-DD]] --json
openpapir case create --archive ROOT --title T [--notes N] [--tag TAG]... \
  [--status open|closed] --json
openpapir case list --archive ROOT [--status open|closed] [--tag TAG]... \
  [--query TEXT] --json
openpapir case show --archive ROOT CASE_ID --json
openpapir case update --archive ROOT CASE_ID [--title T] \
  [--notes N | --clear-notes] [--status open|closed] [--tag TAG]... \
  [--untag TAG]... --json
openpapir case export --archive ROOT --case CASE_ID --to DIR --json
openpapir case import --archive ROOT --from DIR --json
openpapir archive export --archive ROOT --to DIR --json
openpapir archive import --archive ROOT --from DIR --json
openpapir case delete --archive ROOT --case CASE_ID [--purge] --json
openpapir submission add --archive ROOT --case CASE_ID --description D \
  [--date YYYY-MM-DD] [--artefact 'sha256:HEX[:ROLE]']... \
  [--file 'PATH[:ROLE]']... --json
openpapir submission show --archive ROOT SUBMISSION_ID --json
openpapir receipt add --archive ROOT --artefact sha256:HEX \
  [--import-event ID] [--label L] --json
openpapir receipt list --archive ROOT --json
openpapir receipt show --archive ROOT RECEIPT_ID --json
openpapir association create --archive ROOT --receipt RECEIPT_ID \
  --outcome unassociated|candidate|associated|contradictory \
  [--candidate 'SUBMISSION_ID:weak|moderate|strong:STATEMENT']... \
  [--supersedes ASSOCIATION_ID] --json
openpapir association list --archive ROOT --receipt RECEIPT_ID --json
openpapir association show --archive ROOT ASSOCIATION_ID --json
openpapir association retire --archive ROOT ASSOCIATION_ID [--reason TEXT] \
  --json
openpapir skill
openpapir completions bash|zsh|fish|powershell|elvish
openpapir manpage
```

The full contract, including every error code and the privacy rule, is
`docs/architecture.md` in the openPapir repository.
