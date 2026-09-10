# Local archive layout and storage design

## Status and scope

This document describes the on-disk layout and the storage technology of
openPapir's local archive as they are **implemented**, together with the
decisions and the rejected alternatives behind them. The archive, artefact
store, record, integrity, export, permission-repair, and deletion parts of it
have code, and the tree, the identifiers, the write procedure, the caps, and
the permission rules below are what that code writes.

Building it changed some of the decisions. Each change is recorded in the
section that decides the point, with the reason: the fourth reportable Windows
condition under Windows degradation, the digest-only export object names and
the dropped sidecar under Export and backup, and the association and
import-event survival rules under Open questions resolved or deferred. Where a
decision here reads as weaker than first proposed, that section says why.

[architecture](architecture.md) is the canonical description of the
implemented contract and the only place that lists the operations
`capabilities` reports; this document names none of them except where a
decision below is about one. Where architecture and this document disagree,
architecture is authoritative and this page is a defect.

Verification results, the rebuildable `cache/`
index, schema migration, and encrypted backup at
rest are **not implemented** and stay a design. `verified`
is `false` in every envelope ([architecture](architecture.md)).

It is follow-up 3 of
[receipt evidence and local case model decisions](receipt-discovery.md), the
only follow-up there with no external blocker. It turns that note's numbered
proposals into decisions with rationale and rejected alternatives, and resolves
or explicitly defers each of its open questions.

Out of scope, deliberately: the import and association JSON and exit-code
contract (follow-up 5), any receipt parsing (follow-up 7, blocked on evidence),
any dependency on a sibling project, and any government format, identifier, or
API. Container and `.es3` handling belong to openKRX and openSzigno
([AGENTS.md](../AGENTS.md)); this design neither describes nor assumes their
internals.

Wording rule for everything below: importing bytes, associating records, and
verifying authenticity are three separate records. No structure in this
document may be read as evidence of delivery, receipt by an authority,
authenticity, or legal effect.

## Requirements recap

The design is judged against requirements already recorded in this repository
([SECURITY.md](../SECURITY.md), [AGENTS.md](../AGENTS.md), and the discovery
note's numbered proposals):

- Preserve originals unmodified and write-once; record derived data separately.
- Atomic writes: an interrupted import leaves the whole artefact or nothing (5).
- Owner-only permissions, with refusal to widen them (6).
- Bounded input, refused before allocation (7).
- Prevent path traversal, symlink escape, and unintended overwrites of stored
  files.
- A recorded schema version, refusal on newer versions, forward migrations that
  never rewrite originals (8).
- Export as a copy restorable without openPapir; reviewed backup and deletion
  semantics (9, 10).
- Cross-platform behaviour including Windows, and no network, telemetry,
  upload, or background service anywhere.
- All imported bytes are untrusted, including their names and reported sizes.

## Storage technology decision

**Decided: plain files are the system of record; any index is a rebuildable
cache that is never authoritative.**

Every durable fact lives in a file under the archive root: original bytes in a
content-addressed object store, and every record as one small JSON document.
An embedded database may later be added under `cache/` purely to accelerate
listing and search, and it must be safe to delete and rebuild from the files at
any time. The initial implementation ships no such index.

Rationale. Each requirement above is satisfied by a single-file rename, which
is the cheapest correct primitive available on every target platform. A backup
is a directory copy; a restore is a directory copy back; an export is already
almost the storage format, so "restorable without openPapir" costs nothing. A
corrupt or truncated file damages one record instead of the archive. Forward
migration rewrites record files and never touches the object store. Inspection
uses ordinary tools, which matters for a project whose users hold sensitive
personal correspondence and deserve to see what is stored about them.

Rejected: **an embedded database as the system of record.** Holding original
bytes in a database contradicts write-once preservation, makes byte-for-byte
export a conversion rather than a copy, concentrates corruption risk in one
file, and makes an archive unreadable without openPapir. Its transactional
convenience is not needed at personal-archive scale.

Rejected: **plain files with no index, ever.** Search is a roadmap goal, and
forbidding a cache now would force a scan-everything implementation or a later
reversal. Reserving the directory and the disposability rule costs nothing.

Rejected: **a database for records plus files for blobs, both authoritative.**
Two authorities need a consistency protocol between them, and every migration
and backup must keep them in step. Making the database strictly derived removes
that class of bug.

## On-disk layout

The archive root is supplied explicitly by the user. openPapir never searches
for an archive, never adopts a directory that has no archive marker, and never
creates one implicitly as a side effect of another operation.

```text
<archive-root>/
    papir-archive.json      schema version marker, written first
    lock                    single-writer advisory lock
    objects/
        sha256/
            ab/cd/abcd...   original bytes, write-once, read-only
        incoming/           staging for object writes
    records/
        cases/<id>.json
        submissions/<id>.json
        receipts/<id>.json
        imports/<id>.json
        associations/<id>.json
        derived/<digest>.json      derived metadata, on request only
        verifications/<id>.json    designed, not created
    cache/                  disposable, rebuildable, never authoritative
```

`records/verifications/` is part of this design and is not created by any
build: verification results have no code behind them. Everything else in the
tree, `records/derived/` included, is created by `archive init`, and only
`archive derive` ever puts a file in the derived directory.

`papir-archive.json` is the marker and the first file written when an archive
is created. It records the archive schema version, the archive's own opaque
identifier, and the creating version of openPapir. openPapir never adopts a
directory that has no marker; an existing empty directory may be initialised
explicitly.

Identifiers are 128-bit values from a cryptographically secure random source,
rendered as 32 lowercase hex characters, and used as the record filename.
Random rather than time-ordered: a sortable identifier leaks when correspondence
was imported to anyone who sees a filename listing or a backup. Government
identifiers, if any are ever parsed, are stored as attributes with a recorded
source, never as primary keys (discovery note, decision 1).

The whole archive root must be one filesystem. This is checked at creation and
before each write, because the atomic write procedure depends on it.

## Artefact store

**Digest: SHA-256**, lowercase hex. The object path is derived from the digest
as `objects/sha256/<first two hex chars>/<next two hex chars>/<full digest>`,
which keeps directory fan-out bounded on filesystems that degrade with very
large directories. The algorithm appears in the path so a second algorithm can
be added later without moving existing objects.

Rationale: SHA-256 is collision-resistant for this purpose, available without
exotic dependencies, and reproducible by users with ordinary command-line
tools, which matters for checking a backup outside openPapir. Rejected: BLAKE3,
faster but less universally reproducible by a user's own tooling, for no
benefit at these sizes; SHA-1 and MD5, collision-broken; SHA-512, longer paths
for no gain.

**The digest is a storage-layer identity only.** It says two files have the same
bytes. It says nothing about authenticity, origin, integrity of a signature, or
whether the file is a receipt at all. No user-facing wording may present it as
verification.

Objects are write-once. Once an object exists at its path it is never modified,
truncated, or replaced, and its permissions are set to owner read-only. Deleting
an object is a separate, explicit operation described under Deletion.

## Records

Every record is one JSON document, UTF-8, LF-terminated, with sorted keys so
that diffs and backups are stable. Every record carries its own identifier, its
record kind, the archive schema version it was written under, and a creation
timestamp. Records reference each other by identifier only.

**Case**: a user-created folder of related correspondence. Purely local; it
corresponds to nothing any government service issues. Fields: identifier,
user-supplied title, optional notes, a status of `open` or `closed`,
user-supplied tags stored sorted and deduplicated, creation timestamp, and an
update timestamp once there has been an update. A record written before the
status and tags existed reads as `open` with no tag, so an archive an earlier
build wrote needs no migration.

**The case record is the one kind that may be rewritten in place.**
`case update` writes the whole record again through the atomic write
procedure, keeping the identifier and the creation timestamp and setting the
update timestamp. Every other kind stays append-only, and a change to one of
those writes a new record that supersedes the earlier one. The reason is what
each record is for. A submission, a receipt, and an association are the user's
evidence of what they recorded at the time, and evidence that can be edited is
no longer evidence: their whole value is that the history is inspectable. A
case is the user's own folder label, carries no evidence of anything, and is
named by the same identifier for the life of the archive, so writing a second
record for a retitling would leave every reference the user already holds,
and every submission that names the case, pointing at a document that is no
longer current. The rewrite is the same atomic write procedure with a rename
in place of the link the never-overwrite publish uses, so a concurrent reader
sees the whole old document or the whole new one and never a partial file; the
writer lock is held for it exactly as for a first write.

**Submission**: something the user states they sent, recorded from what the
user has locally. openPapir sends nothing, so a submission is always imported
or user-asserted. Fields: identifier, owning case, user-supplied description,
optional user-supplied date, and zero or more artefact references with the role
the user gave them. It never asserts that anything was received anywhere.

**Receipt**: an imported artefact the user believes to be a receipt. Fields:
identifier, artefact digest, the import event that introduced it, optional
user-supplied label. It references its artefact and never rewrites it, and
asserts nothing about the file's type or authenticity; it records what the user
said when importing.

**Import event**: one record per import attempt that stored or re-encountered
bytes. Fields: identifier, artefact digest, byte length, timestamp, the
original filename as supplied by the user's filesystem (an attribute only,
never used to derive a path), and whether this import created the object or
found it already present.

**Derived metadata**: anything openPapir computed from an artefact rather
than read from the user. The design covers detected type, extracted text, and
parsed fields; what is **implemented** is the first of those and only the
first, as `archive derive`. The rest stays a design, because extracting text
or parsing fields needs the format gap closed.

One record per stored object, at `records/derived/<digest>.json`. It is keyed
by the artefact digest rather than by a minted identifier, unlike every other
record here, and deliberately: there is exactly one derived record per object,
recomputing must replace the one that is there rather than leave a second
beside it, and a digest is a name openPapir derived from the bytes rather than
one the user supplied. Fields: the archive schema version, the artefact
digest, the byte length, the computation timestamp, the extractor name and
version, and the derived payload, which today is `media_type` alone.

`media_type` is one value of a **closed table**, decided from the leading
bytes by a hand-written signature list and no dependency:

| Value | What the leading bytes are |
| --- | --- |
| `pdf` | `%PDF-` |
| `png` | The PNG signature. |
| `jpeg` | The JPEG start-of-image marker. |
| `zip` | A zip local-file header, empty-archive record, or spanned marker. |
| `xml` | An XML declaration, after an optional byte-order mark. |
| `text` | UTF-8 with no control character but tab, line feed, and carriage return. |
| `unknown` | Anything else, an empty object included. |

The table is closed so that a listing cannot grow a vocabulary a consumer
never agreed to, and it stops at `zip` on purpose. A container such as an
`.asice` or a `.krx` package is zip-family bytes; openPapir records `zip` for
it and refines the value no further. Reading a container to say which
zip-family format it is belongs to openKRX and openSzigno, and deciding it
from the original filename's extension instead would put a user-supplied name
into a stored record and would still be a guess. So the value is refined only
where the bytes already say `zip`, and openPapir does not do that refining.

**A derived record is disposable and never authoritative.** Deleting all of
them and computing them again alters no original and no user-entered record.
Nothing in the archive references one, no command refuses because one is
missing, and no conclusion about a receipt, a submission, or an association
follows from a media type: it is a statement about leading bytes and never
authenticity, delivery, or legal effect. `case show` and `receipt list` show
the media type and the byte length of an artefact that has a record and say
nothing at all about one that does not. A document in the derived directory
that cannot be read as a derived record is therefore not `record.malformed`:
it is a disposable computation that failed to read, so it holds no facts for a
reader and the next derivation writes it again.

**Recomputation is explicit and nothing else.** `archive derive` is the only
thing that writes, replaces, or removes a derived record. It takes the writer
lock, opens each object once with the no-follow flag, takes the byte length
from that handle, and reads at most 4 KiB of it to decide the media type, so
its cost grows with the number of objects and never with their size. An object
over the single-file cap is left without a record rather than read, exactly as
the integrity check leaves it undigested. A record naming an object the
archive no longer holds is removed, because the record describes the store as
it is now. A purge takes the derived record of every object it removes with
it, in its own all-or-nothing record pass, so the ordinary purge leaves none
behind; a purge that stopped between its record pass and its object pass
leaves one, and the next derivation discards it.

**`archive check` counts derived records and judges none of them.** The count
is of the files the derived directory holds under a digest name: the check
does not parse one, because a disposable computation decides nothing there and
the next derivation rewrites the file whether it still parses or not. A
missing record is nothing at all rather than a problem, and an unreadable one
is counted like any other and is not damage either. `derived_orphans` counts
the records naming an object the store no longer holds, read from the same
names against the objects the store pass found, and it is a count rather than
a problem for the same reason: nothing references such a record, and the next
derivation discards it. Neither figure changes the exit code, because nothing
in the archive depends on a derived record existing.

An export carries no derived record. An export is the case's evidence, and a
disposable computation is not evidence; an archive that imports one runs
`archive derive` if it wants the facts locally.

**Association record**: described in its own section below.

**Verification result** (designed, **not implemented**): the outcome of a
specified cryptographic check
performed by a named verifier. Fields: identifier, the exact artefact covered,
the verifier's identity and version, the trust context supplied by the user,
the precise scope of what was checked, and the outcome. Delegated `.es3` and
container verification belong to openSzigno and openKRX; this record only
stores what such a verifier reported, with its scope intact.

### Separation of states

Imported, matched, and authenticity-verified are separate records, not values
of one status field, exactly as
[receipt-discovery](receipt-discovery.md) requires. An import event may exist
with no association and no verification result; a verification result may exist
with no association. Creating an association must never create a verification
result, and creating a verification result must never create an association.
No record kind, field name, or output may be phrased as "delivered",
"accepted", "official", or "legally effective".

## Write procedure and platform behaviour

Every write of every file follows the same procedure:

1. Create the temporary file in the destination directory, named so that it
   cannot collide with a record filename.
2. Write the full content, then `fsync` the file.
3. Rename the temporary file onto the final path.
4. `fsync` the destination directory.

Object writes stage in `objects/incoming/` because the digest, and so the
destination path, is known only once the bytes have been read. That directory
is inside the archive root, so the rename stays on one filesystem; a
cross-device error abandons the write and the archive is refused as
misconfigured. Both directories are `fsync`ed after the rename. A leftover
staging file is never adopted, so an interrupted import leaves the complete
object or nothing.

A single advisory `lock` file admits one writer at a time; a second writer
refuses rather than waiting indefinitely. Concurrent readers are safe because
every write puts a whole file in place: a reader sees one complete document or
another and never a partial one, including the one rewrite there is, the case
record's. Staging files are removed only by a process
that already holds the writer lock, after acquiring it, never on startup by any
process that happens to open the archive: a reader must not delete a file the
current writer is still filling. The lock file records the holder's process
identifier, host, and start time. A lock whose holder is provably gone is taken
over only by an explicit user action that says what it found; it is never
broken silently, and never on a timeout. Taking over a stale lock is **not
implemented**: a held lock is reported and the operation stops, and the exact
recovery flow, including what counts as proof, stays undecided.

### Path safety

No path inside the archive root may be a symbolic link: not the root, not a
directory within it, not an object, not a record. The rule is enforced when the
path is opened or renamed, using the platform's no-follow flag, rather than by
a stat call beforehand. A pre-check is a time-of-check-to-time-of-use bug, not
a defence. The single-filesystem check likewise does not follow links. Every
path openPapir uses is derived from the archive root plus its own fixed
directory names plus a digest or an identifier; a filename supplied by the user
is stored as an attribute and never joined into a path, so traversal segments
in an imported name cannot escape the root. A path that violates any of this is
refused and reported; it is never repaired, resolved, or followed.

Export applies the same rules outward: openPapir never follows a symbolic link
in the export destination, never overwrites an existing file there, and refuses
and reports instead of replacing anything it did not create.

### Windows degradation

Three guarantees weaken and must be reported rather than assumed:

- Directory `fsync` has no portable equivalent, so the durability of the rename
  itself after a power loss is weaker than on POSIX. The file content is still
  flushed; the directory entry may not be.
- Replacing or removing an existing file can fail when another process holds it
  open.
- Owner-only access is expressed as an access-control list rather than a
  permission bit, so it depends on the underlying filesystem supporting them.

Each weakening is a named condition the implementation must report at the point
of the write and in any archive health output. Its wire name and JSON shape
belong to [error-contract](error-contract.md) and are deliberately not fixed
here. What is fixed: degradation is reported, never silently accepted and never
described as equivalent. The implementation added a fourth condition, the
no-follow open that refuses the handle rather than the open, and reports it the
same way ([architecture](architecture.md)).

## Limits and permissions

The archive root and every directory inside it are created owner-only; files
are created owner-read-write and objects become owner-read-only once stored.
openPapir never widens permissions on an existing archive, and it refuses to
operate on one whose permissions are already wider, reporting what it found. It
offers no flag to override this; the sole exception is the explicit
permission-repair action described under export and backup, which only
narrows. The consequence, accepted deliberately, is that archives on
filesystems that cannot express owner-only access are unsupported.

Caps are **initial proposals, adjustable by review**. They exist to bound
resource use, and are never relaxed to make one particular input succeed
([AGENTS.md](../AGENTS.md)):

- Single file: 64 MiB. The one observed operator statement about attachment
  size is 25 MB (discovery note, E1, descriptive only); this leaves headroom
  without being unbounded.
- Total bytes per import operation: 512 MiB.
- Files per import operation: 1000.
- Single record document: 1 MiB, which bounds derived metadata as well.
- Original filename: 255 bytes, stored as an attribute only.

Every cap is checked before allocation, from the size the filesystem reports,
and enforced again while streaming, because a file may grow or the reported
size may be wrong. Exceeding a cap mid-stream aborts the write and removes the
staging file. Container expansion, XML complexity, and extraction safety are
not implemented here and stay with openKRX and openSzigno.

## Duplicate import

Re-importing bytes already present is **not an error**. The object is left
untouched and a second import event is recorded against the existing artefact,
so the user is told the file is already present and when it was imported
before. The number of import events for an artefact is therefore meaningful
history, not an anomaly.

Before recording a duplicate the stored object's byte length is compared with
the incoming length; a mismatch means the store is damaged and is reported as
such rather than overwritten. A matching length is not evidence that the bytes
are identical, only that nothing obvious is wrong; the stored object is not
re-read on every import, for cost reasons, so the whole-archive integrity
check, implemented as `archive check` and specified in
[architecture](architecture.md), is the real answer to a damaged store: it
re-digests every object.

## Association records

An association is a separate record with its own evidence, never a foreign key
implying certainty. The record carries `id`, `receipt_id`, `submission_id`
(null unless the outcome is `associated`), `outcome`, `created_by` (`user` or
`automatic`, and only `user` is written today, because no automatic matching
exists), `created_at`, `supersedes` (null unless the record replaces an
earlier one), and `candidates`. Evidence and confidence are not record-level
fields: each entry in `candidates` carries its own `submission_id`,
`confidence`, and `evidence` list. One record-level field is optional and only
a retirement carries it: `statement`, the user's own reason for withdrawing an
assertion, written only when they gave one. `candidates` holds one entry for
`associated`, one or more for `candidate`, two or more for `contradictory`,
and none for `unassociated`. [error-contract](error-contract.md) is
authoritative for wire shapes.

Outcomes are exactly:

- `unassociated`: no evidence links the receipt to any submission.
- `candidate`: one or more possible submissions, each with its own evidence.
  A candidate set is never collapsed to a single best guess automatically.
- `associated`: the user confirmed a candidate, or the evidence is
  unambiguous by a rule recorded in the evidence itself.
- `contradictory`: evidence of associating strength points at more than one
  submission. All of it is retained; nothing is discarded to resolve it.

Each evidence entry records its kind, the derived record and extractor version
it came from or that the user asserted it, and a readable statement of what was
observed. A candidate's confidence is an ordinal label from a closed set
(`weak`, `moderate`, `strong`) and explicitly not a probability, because no
calibration data exists and a number would imply one. Association records are
append-only, as every record kind but the case record is: a change writes a new
record superseding the previous one, so history is inspectable. An association
never implies delivery, receipt by an authority,
authenticity, or legal effect ([architecture](architecture.md)).

Withdrawing an assertion is that same supersession rather than an edit or a
removal, implemented as `association retire`. It writes a record for the same
receipt with outcome `unassociated`, no candidate, and `supersedes` naming the
record the user retired, so the history reads as what they asserted and then
that they withdrew it. The retired record is untouched, and a record something
already supersedes cannot be retired again: the newest record of a history is
the one a further statement supersedes. That is the chosen remedy for a
deletion refused because an association spans two cases, decided over the two
alternatives considered, an edit that drops the departing candidate and a
`case delete` flag that removes the association outright. Both were rejected:
openPapir edits no stored record, and no deletion of one case may silently
discard the user's own assertion about another. Retiring is the user's own
statement, and the deletion below then treats what it withdrew as history.

## Export and backup

Export is implemented at two scopes: `case export` for one case and
`archive export` for a whole archive, with `case import` and `archive import`
reading each back. [architecture](architecture.md) is authoritative for all
four contracts. What follows is the design, reconciled with what was built.

Export writes a directory holding the original bytes of each exported
artefact, copied **byte for byte**, plus readable JSON records and a
manifest. The implemented layout is:

| Path | Content |
| --- | --- |
| `objects/<digest>` | One copied object, named by its lowercase hexadecimal digest and nothing else. |
| `records/<kind>/<id>.json` | One record document, exactly as the archive stores it, with `<kind>` one of `case`, `submission`, `receipt`, `association`, or `import_event`. |
| `manifest.json` | One JSON document with sorted keys listing every copied object with its digest and byte length and every record with its kind and identifier, and naming its own `export_scope`, `case` or `archive`. |
| `papir-archive.json` | The archive marker, in a whole-archive export only, copied byte for byte so the schema version travels with the copy. |

An exported object is named by its **digest alone**. The earlier design here
named an object by a sanitised form of the recorded original filename and
carried a `.metadata.json` sidecar beside it; that was rejected during
implementation, because sanitising a filename into a path publishes the name
the privacy rule protects and reintroduces the collision handling the digest
removes. The original filename stays where it has always been, an attribute
inside the exported import-event record. There is no sidecar: the records are
whole documents in their own directories, so nothing has to be reassembled.

The manifest is authoritative for what the export contains. Export re-digests
each copy as it writes it and fails if it differs; it never converts,
re-encodes, normalises, compresses, or encrypts an original. The result is
restorable without openPapir: the files are the files, the records are
readable JSON. Re-digesting is a storage-layer identity check and never a
cryptographic verification.

Importing an export back into an archive is implemented as `case import`. It
is the same plain copy inward: the manifest is authoritative, every object it
lists is re-digested from the export's own bytes and every record it lists is
read and parsed before the archive is written to at all, and the record set is
then published under the writer lock all at once or not at all. Every record
keeps the identifier it had, so a restored case is the case that was exported;
an identifier a different record already holds is a refusal, because openPapir
edits no stored record. An object or a record the archive already holds is not
an error and is not written again, so importing one export twice leaves the
same archive. Each object the import stores gets one new import event with
`source` `export`, and the exported import event is restored beside it, so the
history of the user's own import survives the round trip.

Exporting a whole archive is implemented as `archive export`. It is the same
plain copy outward in the same layout, taken over everything the archive holds
rather than over one case: every record of every kind, filtered by nothing,
and every object the store holds. The objects come from the store itself
rather than from what the records reference, which is the one difference from
a case export: an object no record names is still the user's own bytes, and a
copy that left it behind would be a smaller archive rather than the same one.
The two sets are still compared, so a record naming an object the store does
not hold refuses the export, and an entry under `objects/` that is not an
object filed under its own digest refuses it too rather than being passed
over. The archive marker is copied beside the manifest, so the export says
which schema version it was taken under without openPapir being run at all.

Reading a whole-archive export back is `archive import`, and it is a separate
command rather than `case import` accepting both. The reason is the
record-conflict rule. Whether a record may be written is a question about one
record identifier and the archive, so a whole-archive import restores the
whole export as one set, all of it or none of it: an identifier a different
record holds refuses the import before anything is written, exactly as it does
for one case, and openPapir edits neither record. Restoring case by case would
have had to answer what a conflict inside one case means for the receipts,
associations, and import events it shares with another, and a whole-archive
export holds records that belong to no case at all, which no per-case result
could report. Each command reads its own `export_scope` only and refuses the
other's directory, so the shape of what was handed over is never in doubt.

The practical limit of `archive import` is its own ceiling,
`input.cap.restore_bytes`, over the sum of the object bytes the manifest names:
a restore replays inputs the archive already accepted one command at a time, so
the per-operation import cap is not the bound that serves it. The value and the
reasoning for it are in [architecture](architecture.md). An archive whose
objects come to more than that ceiling still cannot be restored by this build,
which is why a backup remains the other option below rather than being replaced
by it.

A backup is a copy of the whole archive root taken while no openPapir process
holds the lock; `cache/` may be omitted. Ordinary copy tooling routinely widens
permissions on restore, which the owner-only rule would then refuse. The
documented remedy is an explicit repair action that narrows an archive's
permissions back to owner-only and reports every path it changed. It is a
repair, not an escape hatch: there is no flag that makes openPapir accept wide
permissions, and the repair only narrows, never widens. A backup taken this way
is plaintext on the medium it lands on; the encrypted backup below is the
decided alternative, and it is not implemented.

## Encrypted backup at rest

This section decides the design of an encrypted backup. Nothing here is
implemented, no dependency is added by the decision, and `capabilities` reports
no such operation. It closes the deferred question below by making the threat
model and the key handling explicit, so that an implementation issue can be
written without reopening the choices.

### Scope of the encryption

The subject is **the backup artefact, not the live archive**. The archive root
stays exactly as the rest of this document describes it: plain files, plain
records, owner-only permissions, readable by ordinary tools while openPapir is
not running. Encrypting the live tree would break the read-only integrity
check, the atomic write procedure, and the promise that original bytes sit on
disk unconverted, and it would move a key into every command that touches the
archive. The archive continues to rely on operating-system disk encryption and
owner-only permissions, and users must be told so plainly.

An encrypted backup is therefore one file produced from, and restored to, a
directory. It is a copy taken outward. Restoring it produces a directory, not
an archive. `case import` reads such a directory back into an archive, and
this decision does not change that.

### Container format

The plaintext is a **tarball of the export shape**: the `objects/`,
`records/<kind>/`, and `manifest.json` tree described under Export and backup,
written whole into an uncompressed tar stream, in sorted order, with the
manifest last. The export shape is already restorable without openPapir and
already excludes the rebuildable `cache/` index, so the encrypted backup adds
no second layout to maintain. Tar entries carry no owner name, no group name,
and no path outside the export shape.

The ciphertext is a **standard AEAD container over that stream**, not an
openPapir invention. The decided container is the age format version 1
([age-encryption.org/v1][agespec], published as `age.md` in the C2SP
repository), used in its passphrase mode. Reading that specification, the
properties this design depends on are:

| Property | What the specification states |
| --- | --- |
| Payload AEAD | The payload is split into chunks of 64 KiB and each of them is encrypted with ChaCha20-Poly1305, in a STREAM variant that binds chunk order and marks the final chunk. |
| Passphrase mode | An `scrypt` recipient stanza carries a base64-encoded 16-byte salt from a CSPRNG and the base-two logarithm of the scrypt work factor. |
| Header integrity | The header is covered by HMAC-SHA-256 under a key derived from the file key with HKDF-SHA-256. |

Rejected: a container framed by this project over a raw AEAD crate. It would
need its own chunking, nonce discipline, final-chunk marker, and header
authentication, which is the part of such a format that is easiest to get
wrong and that this repository cannot review at the cost it pays for the rest.
Also rejected: compressing before encrypting, because compression ratios leak
content, and per-object encryption, because it exposes the object count and the
size of every object by construction.

[agespec]: https://age-encryption.org/v1

### Candidate crates and their audit state

No dependency is added by this decision. The candidates below are named so that
the dependency review an implementation issue must pass starts from a list.
Each claim is what the crate's own repository or its crates.io page states,
read without running anything; anything those sources do not state is recorded
as not verified.

| Crate | Role | Latest stable | Licence | Audit state as stated by the project | RustSec |
| --- | --- | --- | --- | --- | --- |
| `age` | The age v1 container, encrypt and decrypt, passphrase mode | 0.12.1 | MIT OR Apache-2.0 | Not verified. Neither the `rage` repository README nor the crates.io page read for this section states a security audit of the library. | RUSTSEC-2024-0433, 2024-12-18, "Malicious plugin names, recipients, or identities can cause arbitrary binary execution", affecting several 0.6 to 0.11 lines and patched in 0.6.1, 0.7.2, 0.8.2, 0.9.3, 0.10.1, and 0.11.1. The advisory scopes it to the plugin APIs with the `plugin` feature enabled. |
| `chacha20poly1305` | The AEAD, if the container were built from parts instead | 0.11.0 | Apache-2.0 OR MIT | Its README states: "This crate has received one security audit by NCC Group, with no significant findings." | No advisory directory for this crate in the RustSec database at the time of writing. |
| `aes-gcm` | The alternative AEAD, if a caller ever requires AES | 0.11.1 | Apache-2.0 OR MIT | Its README states: "This crate has received one security audit by NCC Group, with no significant findings." | RUSTSEC-2023-0096, 2023-11-22, plaintext exposed by `decrypt_in_place_detached` even when tag verification fails, affecting >= 0.10.0, < 0.10.3 and patched in 0.10.3. |
| `argon2` | The memory-hard KDF, if the container were built from parts | 0.6.0 | MIT OR Apache-2.0 | Not verified. The README read for this section states no audit. | No advisory directory for this crate in the RustSec database at the time of writing. |

Constraints any of them must satisfy before it is admitted: the licence must be
one `deny.toml` already allows, the crate must build on the minimum supported
Rust version this repository pins, and the `age` plugin feature must stay off,
because the plugin path is what that crate's one advisory is about and this
project runs no external binary. The three RustCrypto crates state a minimum
supported Rust version of 1.85 in their READMEs, and the crates.io metadata for
`age` 0.12.1 publishes `rust_version` 1.74. All four therefore clear the 1.88
minimum this repository pins, so none of them would move that floor. Every row
above is a reading of a public repository or crates.io page taken while this
section was written, not a standing guarantee: the review that admits a crate
re-reads it.

### Key handling

The key is **derived from a passphrase the user holds**, with the memory-hard
KDF the container format specifies: scrypt in age v1, or Argon2id if the
rejected build-from-parts route is ever revisited. The work factor is written
into the container by the format, so a backup taken today stays openable after
the default is raised.

openPapir **stores no key**. It writes no key file, no keyring entry, no
recovery copy, no passphrase hint, and no environment variable of its own. The
passphrase is read from an interactive prompt with echo off, or from a file
descriptor the user names, never from a command-line argument, which would land
in the shell history and the process list. It is zeroised once the key is
derived. There is no recovery path: a lost passphrase is a lost backup, and any
future prompt says so before it takes a passphrase for a new container.

### What is and is not protected

| Protected | Not protected |
| --- | --- |
| The bytes of every artefact in the backup. | The existence of the backup file, and that it is an openPapir backup, if its name or its location says so. |
| The record contents: case, submission, receipt, association, and import-event documents, and with them the original filenames, titles, notes, and digests they carry. | The total size of the container, which bounds the total size of what it holds. |
| The manifest, and with it the number of objects and the length of each one. | The file's timestamps, and that it changed between two backups. |
| The tar entry names and sizes, because the tar stream sits inside the encrypted payload. | Anything in the live archive, on the medium that held it, or in any plaintext copy taken before. |

The chunk order and the final-chunk marker are **authenticated, not hidden**.
The STREAM construction binds them so that a reordered or truncated container
is refused, which is a recovery property and not a confidentiality one. The
chunk size is fixed at 64 KiB by the format, so the container's length reveals
the payload's length up to that granularity. No padding is designed, and none
is claimed.

The floor this design had to meet was that contents are protected while the
existence, the size, and the count of files are not. The container's own
existence and size are indeed unprotected, as that floor allows. On the rest
the design goes further than the floor rather than contradicting it: putting
the tar stream inside the encrypted payload hides the number of files and the
size of each one, which the rejected per-object encryption would have exposed.
The tables above are what this design promises. A host compromised while the
passphrase is being typed, or while a restore is running, is outside all of it.

### Recovery semantics

- A **wrong passphrase is a clean refusal**. The passphrase either opens the
  container's header or it does not, and no payload chunk is decrypted before
  it does. The refusal says that the passphrase did not open this container,
  names no path, no digest, and no record, and exits through an ordinary
  refusal bucket of the [error contract](error-contract.md), never through a
  crash.
- A **truncated or altered container is detected before any decryption output
  is written**. A restore writes into a staging directory beside the target,
  the way records and deletions already stage their writes, and publishes it
  only after the last chunk authenticates and the format's final-chunk marker
  is present. A container that ends early therefore fails with nothing
  published, and the staging directory is removed.
- The manifest inside is checked after publication the way an export's is:
  every object re-digested against the name it is filed under. That is the
  storage-layer identity check described above, and it is not a cryptographic
  verification of anything.
- A partial restore is not offered. The unit is the whole container.

### Wording rule for an encrypted backup

An encrypted backup asserts **confidentiality of the copy, and nothing about
the authenticity of the originals**. A container that opens shows that whoever
wrote it held the passphrase and that the bytes were not altered afterwards. It
shows nothing about where the artefacts inside came from, whether any of them
is genuine, or whether anything was delivered, received by an authority, or has
legal effect. No message, field, or document may describe a successful
decryption as verification, validation, authentication of a document, or proof
of anything beyond the container having opened. `verified` stays `false`.

## Deletion

Deletion is implemented as `case delete`, and
[architecture](architecture.md) is authoritative for its contract. What
follows is the design, reconciled with what was built.

Deleting a case deletes its records and does **not** delete objects by
default. Objects go only by an explicit purge, and only when no remaining
import event, receipt, or submission references them. Every object left behind
is counted with the reason it stayed, which is one of `purge_not_requested`,
`referenced_elsewhere`, `records_retained`, and `unremovable`; a purge never
leaves an unreported orphan, and an archive the integrity check found clean
stays clean after a deletion.

The reason is a **reason, not a reference**. The earlier design here reported
each retained object with the record still referencing it; that was rejected
during implementation, because naming a surviving record would say which
record still points at content the user asked to purge.

What goes with the case is fixed:

- Every submission recorded against the case.
- Every association of a departing supersession chain. The chain is the unit,
  because removing a record another one supersedes would leave the newer
  record naming a record the archive no longer holds. A chain goes when one of
  its records names a departing submission and its live record, the one no
  other record supersedes, names none that remains: that is the ordinary case,
  where every candidate the chain ever named belongs to this case, and the
  retired one, where the live record asserts nothing at all, so the withdrawn
  history goes with the case it was about. A chain naming no departing
  submission stays.
- Every receipt an association tied to a departing submission, unless a
  remaining association still names it.
- **Import events are history and are kept.** The one exception is an import
  event naming an object the purge actually removed: it goes with that object,
  because an event describing content that is gone describes nothing. An
  object the purge could not unlink keeps its import event, so it stays a
  referenced object rather than becoming an orphan.
- **The derived-metadata record of a purged object goes with the purge**,
  counted under `records_removed` as `derived_metadata`. It is openPapir's own
  disposable computation about those bytes, so once they are gone it describes
  nothing; nothing in the archive references one, and no user statement is
  lost with it. It goes in the record pass rather than the object pass,
  because it is a record: it is probed with the others, so one that will not
  go refuses the deletion before the first unlink, and a deletion that removes
  no object removes none of them. A purge that stops between the two passes
  therefore leaves a record about bytes that are gone; `archive check` counts
  one under `derived_orphans` and the next `archive derive` discards it.

An association naming submissions in **two cases** is refused rather than
resolved while the user still asserts it. It references a submission that
remains, so it cannot go, and one that is going, so keeping it whole would
leave a dangling reference. openPapir edits no stored record, so the deletion
is refused in the scan before anything is unlinked, and the refusal is
symmetric: until the assertion is withdrawn, neither case can be deleted. The
remedy is the user's own `association retire` above, after which either case
can go and takes the withdrawn history with it.

A **superseded** record naming another case's submission is the second case,
and it goes rather than refusing. Suppose the user asserted that a receipt
answered a submission of case B, then superseded that record with one naming a
submission of case A, and then deleted case A. The chain's live record names
only case A's submission, so nothing the user asserts today survives the
deletion, the chain is doomed, and the earlier record about case B goes with
it. Case B keeps its own submission and its own case record; what goes is
association history the user had already replaced. The alternative is worse in
both directions: keeping the older record alone would leave the newer one
naming a record the archive no longer holds, and refusing would let history
the user superseded long ago block a deletion nothing live objects to.
Superseded history is therefore held by the case its live record names, and
`association list` shows the whole chain, so the user can read what a deletion
would take before running it.

A chain holding a **`supersedes` cycle** anywhere in it is refused with
`record.inconsistent` and rule `supersedes_cycle`, and the unit of the refusal
is the whole chain rather than the cycle alone. No openPapir command writes a
cycle: a retirement of a record something already supersedes is refused, and a
superseded record outside the receipt is refused, so a cycle reaches an archive
only by hand.

Inside a cycle every record is superseded by another, so nothing in it says
what the user asserts today. A chain that is nothing but a cycle has no live
record at all, and both rules above read exactly that record, so a cycle naming
a departing submission and a remaining one would otherwise be read as history
nobody asserts and removed whole. A chain whose live record supersedes a cycle
behind it is the same anomaly one layer up: reading the head alone would say
the history is withdrawn and take the cycle with it, but the cycle is what
makes that history unreadable in order, so what the head withdraws cannot be
told either. A deletion that cannot read the history it would remove must not
guess, so the anomaly is reported as what it is and nothing is touched.

Only a cycle this deletion would otherwise have removed is refused, because the
scan reads the whole archive and refusing on a cycle anywhere would describe
the archive rather than the command the user ran. Finding one wherever it sits
is `archive check`'s work: the check counts the cycles the association records
form, archive-wide and as a count alone, and reports them under the same code
and rule.

Deletion is real: the record files are removed. The deletion summary (counts
and record kinds only, no filenames, digests, or titles) is reported to the
user and **not persisted**, because a digest is a fingerprint of the deleted
content and storing one would defeat the purge. There is therefore no
`records/deletions/` directory. openPapir keeps no immutable audit log of a
user's own correspondence; here privacy outweighs auditability. Deletion does
not erase data from the storage medium and must not claim to; backups already
taken are outside openPapir's reach.

## Schema versioning and migration

`papir-archive.json` records `archive_schema_version`, an integer whose first
value is 1. The rules:

- A version **newer** than the running openPapir supports: refuse every
  operation, including read-only ones, with a distinct error. No best-effort
  read, no partial listing, no repair attempt.
- A version **older** than supported: refuse writes and report that a migration
  is required. Migration is an explicit user action and never runs
  automatically as a side effect of another command.
- Migration is **forward-only**. There is no downgrade. The tool states that a
  backup should be taken first.
- Migration may add and rewrite record files and may rebuild `cache/`. It
  **never** rewrites, moves, or re-digests anything under `objects/`. Original
  bytes survive every migration untouched.
- The new version marker is written last, by the same atomic procedure, after
  the migrated records are durable. An interrupted migration therefore leaves
  the old version marker and is safely re-runnable.

## Open questions resolved or deferred

The discovery note left seven open questions
([receipt-discovery](receipt-discovery.md)). Their status here:

1. **Storage technology: resolved.** Plain files as the system of record with
   a disposable, rebuildable index, as decided above.
2. **Whether one submission can yield byte-different receipts: deferred.**
   Blocked on the note's findings F1 and F6; it closes only on a retrieved
   normative or operator statement about whether a receipt is reissued,
   superseded, or re-downloadable and whether reissues are byte-identical. The
   layout already tolerates either answer, because artefact identity is a
   storage fact and receipts are separate records, so several receipts may
   reference one submission.
3. **One artefact plausibly a receipt for two submissions: partly resolved.**
   The record shape is decided: outcome `contradictory`, all evidence retained,
   no automatic collapse. The user-facing resolution flow is deferred to the
   association implementation issue; it is a workflow question, not a storage
   one.
4. **Recomputing derived metadata on extractor upgrade: resolved, and
   implemented.** Never automatic. Derived records carry the extractor name
   and version; recomputation is `archive derive` and nothing else. That keeps
   listings reproducible and avoids background work. As implemented, a
   derivation recomputes every record it walks rather than only the ones an
   older extractor wrote, which costs one bounded read per object and spares
   the archive a staleness rule it would have to keep true.
5. **Retention of import events and association history: resolved.** History
   is deletable, and openPapir keeps no immutable log of the user's own
   correspondence. As implemented, deleting a case removes the association
   chains tied only to it and the ones the user retired, and removes an import
   event only together with an object
   the purge actually unlinked; every other import event is kept as history.
   Whether a separate "delete history, keep artefacts" operation is worth
   offering stays deferred.
6. **Windows parity: resolved as documented degradation.** Owner-only access
   is required with no override, so filesystems that cannot express it are
   refused rather than silently accepted; the three weakened guarantees above,
   and the fourth the implementation added, are named, reportable conditions
   whose wire representation belongs to the error contract.
7. **Encrypted backup at rest: resolved as a design, not implemented.** The
   threat model and the key-handling decision it needed are in Encrypted
   backup at rest above: the backup artefact only, a standard AEAD container
   over a tarball of the export shape, a passphrase-derived key with a
   memory-hard KDF, and no key stored by openPapir. Whole-tree and per-object
   encryption of the live archive are both rejected there. Until the design is
   built the archive relies on operating-system disk encryption and owner-only
   permissions, and users must be told so plainly.

## Unblocked follow-ups

This design unblocks the following bounded implementation issues, which the
tracker owns; the sequencing only is recorded here.

- **Artefact import with byte preservation** (discovery note follow-up 4):
  **implemented** as `archive init` and `import`.
- **Import and association error codes with their JSON and exit-code contract**
  (follow-up 5): **agreed** as
  [error-contract](error-contract.md), and emitted by every implemented
  command.
- **Association records with candidate and contradictory outcomes**
  (follow-up 6): **implemented** as `receipt add`, `receipt list`,
  `association create`, `association list`, and `association retire`, using
  the user's own evidence only and no receipt parsing.
- **Whole-archive integrity check** (new): **implemented** as `archive check`.
- **Case export, backup, and the permission-repair action** (new):
  **implemented** as `case export`, `case import`, `archive export`,
  `archive import`, and `archive repair-permissions`.
- **Case deletion with an explicit purge** (new): **implemented** as
  `case delete`.
- **Derived-metadata staleness and recompute-on-request** (new): **not
  implemented**. Depends on nothing further in this document.
- **Encrypted backup at rest** (new): **not implemented**, decided above.
  Depends on a dependency review admitting one container crate, which is the
  only part of it this document leaves open.

Unchanged blockers: receipt parsing still needs the format gap closed
(follow-up 1), and the delegated verification boundary still needs published,
versioned contracts from openKRX and openSzigno (follow-up 8).

## Limits of this design

It is a layout and the reasoning behind it, not a contract:
[architecture](architecture.md) rather than this page is the contract for the
parts that have code, and no capability follows from the parts still awaiting
it. The caps are proposals chosen for
safety rather than measurement, and no performance work has been done. It
assumes one user and one writing process on one local filesystem; network
filesystems and multi-user archives are not designed for. It fixes no response
schema, exit code, command name, or field of any government artefact, and
assumes nothing about what a receipt contains,
because nothing is yet established about that
([receipt-discovery](receipt-discovery.md)). Treat every number and name above
as reviewable.
