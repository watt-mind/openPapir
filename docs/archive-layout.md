# Local archive layout and storage design

## Status and scope

This document is a **design for review**, and much of it now has code behind
it. It decides the on-disk layout and the storage technology of openPapir's
local archive so that implementation issues can be written against something
concrete, and the archive, artefact store, record, integrity, export,
permission-repair, and deletion parts of it were then implemented.
[architecture](architecture.md) is the canonical description of the
implemented contract: where it and this document disagree, architecture is
authoritative and this page is a defect. Derived-metadata records,
verification results, the rebuildable `cache/` index, import from an export,
schema migration, and encrypted backup at rest are **not implemented** and
stay a design. `verified` is `false` in every envelope
([architecture](architecture.md)).

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
        derived/<id>.json          designed, not created
        verifications/<id>.json    designed, not created
    cache/                  disposable, rebuildable, never authoritative
```

`records/derived/` and `records/verifications/` are part of this design and
are not created by any build: derived metadata and verification results have
no code behind them. Everything else in the tree is created by
`archive init`.

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
user-supplied title, optional notes, creation timestamp.

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

**Derived metadata** (designed, **not implemented**): anything computed from
an artefact: detected type,
extracted text, parsed fields. Fields: identifier, artefact digest, extractor
name and version, computation timestamp, and the derived payload. Derived
records are disposable by definition: deleting all of them and recomputing must
never alter an original or a user-entered record.

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
no file is ever modified in place. Staging files are removed only by a process
that already holds the writer lock, after acquiring it, never on startup by any
process that happens to open the archive: a reader must not delete a file the
current writer is still filling. The lock file records the holder's process
identifier, host, and start time. A lock whose holder is provably gone is taken
over only by an explicit user action that says what it found; it is never
broken silently, and never on a timeout. The exact recovery flow, including
what counts as proof, belongs to the artefact-import issue.

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
belong to the error-contract issue (follow-up 5) and are deliberately not fixed
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
`confidence`, and `evidence` list. `candidates` holds one entry for
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
calibration data exists and a number would imply one. Records are append-only:
a change writes a new record superseding the previous one, so history is
inspectable. An association never implies delivery, receipt by an authority,
authenticity, or legal effect ([architecture](architecture.md)).

## Export and backup

Export is implemented as `case export`, and
[architecture](architecture.md) is authoritative for its contract. What
follows is the design, reconciled with what was built.

Export writes a directory holding the original bytes of each exported
artefact, copied **byte for byte**, plus readable JSON records and a
manifest. The implemented layout is:

| Path | Content |
| --- | --- |
| `objects/<digest>` | One copied object, named by its lowercase hexadecimal digest and nothing else. |
| `records/<kind>/<id>.json` | One record document, exactly as the archive stores it, with `<kind>` one of `case`, `submission`, `receipt`, `association`, or `import_event`. |
| `manifest.json` | One JSON document with sorted keys listing every copied object with its digest and byte length and every record with its kind and identifier. |

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

Exporting a whole archive, and importing an export back into an archive, are
not implemented.

A backup is a copy of the whole archive root taken while no openPapir process
holds the lock; `cache/` may be omitted. Ordinary copy tooling routinely widens
permissions on restore, which the owner-only rule would then refuse. The
documented remedy is an explicit repair action that narrows an archive's
permissions back to owner-only and reports every path it changed. It is a
repair, not an escape hatch: there is no flag that makes openPapir accept wide
permissions, and the repair only narrows, never widens. Encryption at rest is
not designed here; see the deferred question below.

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
- Every association whose named submissions are all going, provided it names
  at least one. An association naming no submission stays, and one a surviving
  association supersedes is kept, because removing it would leave the newer
  record naming a record the archive no longer holds.
- Every receipt an association tied to a departing submission, unless a
  remaining association still names it.
- **Import events are history and are kept.** The one exception is an import
  event naming an object the purge actually removed: it goes with that object,
  because an event describing content that is gone describes nothing. An
  object the purge could not unlink keeps its import event, so it stays a
  referenced object rather than becoming an orphan.

An association naming submissions in **two cases** is refused rather than
resolved. It references a submission that remains, so it cannot go, and one
that is going, so keeping it whole would leave a dangling reference. openPapir
edits no stored record, so the deletion is refused in the scan before anything
is unlinked, and the refusal is symmetric: until the user resolves the
association themselves, neither case can be deleted.

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
4. **Recomputing derived metadata on extractor upgrade: resolved.** Never
   automatic. Derived records carry the extractor name and version; one from an
   older extractor is reported as stale and recomputed only on explicit
   request. That keeps listings reproducible and avoids background work.
5. **Retention of import events and association history: resolved.** History
   is deletable, and openPapir keeps no immutable log of the user's own
   correspondence. As implemented, deleting a case removes the associations
   tied only to it, and removes an import event only together with an object
   the purge actually unlinked; every other import event is kept as history.
   Whether a separate "delete history, keep artefacts" operation is worth
   offering stays deferred.
6. **Windows parity: resolved as documented degradation.** Owner-only access
   is required with no override, so filesystems that cannot express it are
   refused rather than silently accepted; the two weakened guarantees above are
   named, reportable conditions whose wire representation belongs to the error
   contract.
7. **Encrypted backup at rest: deferred.** It needs a threat model and a
   key-handling decision under [SECURITY.md](../SECURITY.md), not a layout
   decision. Nothing above precludes it; whole-tree and per-object encryption
   both remain open. Until it is decided the archive relies on operating-system
   disk encryption and owner-only permissions, and users must be told so
   plainly.

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
  `association create`, and `association list`, using the user's own evidence
  only and no receipt parsing.
- **Whole-archive integrity check** (new): **implemented** as `archive check`.
- **Case export, backup, and the permission-repair action** (new):
  **implemented** as `case export` and `archive repair-permissions`.
- **Case deletion with an explicit purge** (new): **implemented** as
  `case delete`.
- **Derived-metadata staleness and recompute-on-request** (new): **not
  implemented**, and the only follow-up here with no code behind it. Depends
  on nothing further in this document.

Unchanged blockers: receipt parsing still needs the format gap closed
(follow-up 1), and the delegated verification boundary still needs published,
versioned contracts from openKRX and openSzigno (follow-up 8).

## Limits of this design

It is a layout, not an implementation. No capability follows from the parts
still awaiting code, and [architecture](architecture.md) rather than this page
is the contract for the parts that have it. The caps are proposals chosen for
safety rather than measurement, and no performance work has been done. It
assumes one user and one writing process on one local filesystem; network
filesystems and multi-user archives are not designed for. It fixes no response
schema, exit code, command name, or field of any government artefact, and
assumes nothing about what a receipt contains,
because nothing is yet established about that
([receipt-discovery](receipt-discovery.md)). Treat every number and name above
as reviewable.
