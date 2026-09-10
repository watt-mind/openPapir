# Roadmap

The project is at the alpha stage. The local archive is implemented as far
as the operations the tool reports, and every milestone below still
has work left in it. The [specification index](specification.md) records what
is implemented, what is decided but not built, and what is blocked, and
[architecture and CLI contract](architecture.md) is authoritative for
implemented behaviour; this page records the order in which the remaining work
is meant to happen.

## 1. Discover supported inputs

Depend on openKRX's profile and reader discovery before specifying package
import. Identify a concrete supported KRX profile using authoritative sources
and redistributable synthetic examples. Record unsupported variants explicitly.

Separately identify receipt types and their documented identifiers, relationship
rules, and signature formats. The discovery deliverable is an evidence matrix,
unknowns, a synthetic fixture plan, and bounded implementation issues. Stop
short of claiming a universal e-Papír format or inventing a submission API.
The first receipt-side result is recorded in
[receipt evidence and local case model decisions](receipt-discovery.md), whose
unresolved findings gate any receipt parsing work.

## 2. Design local cases

Specify case/submission/receipt identities, preserved originals, derived
metadata, duplicate imports, storage limits, permissions, atomic writes, schema
migrations, export, backup, and deletion. Decide storage technology with those
requirements in view. Define import and association errors and their JSON/exit
contracts. Review privacy and failure recovery before implementing persistence.
The proposed model and its open questions are drafted in
[receipt evidence and local case model decisions](receipt-discovery.md); they
are proposals, not decisions. Its follow-up 3 is answered by
[local archive layout and storage design](archive-layout.md), which decides the
storage technology, the on-disk layout, and the record shapes for review, and
records which of that note's questions stay open. Follow-up 5 of that same
note is answered by the
[import and association error, JSON, and exit-code contract](error-contract.md),
which specifies the JSON envelope, the error-code catalogue, and the
exit-code mapping for review. The storage, record, error, and deletion parts
of both documents are now implemented and their contract is in
[architecture and CLI contract](architecture.md); derived metadata,
verification results, migration, and encrypted backup at rest are not.

## 3. Implement one offline workflow

Deliver a small vertical slice: import a file, preserve its original bytes,
assign it to a case, and list or export that case. The local half of this is
done: `import`, `case create`, `case list`, `case show`, `submission add`,
`archive check`, `case export`, `archive repair-permissions`, and
`case delete` exist, with resource limits, interrupted writes, duplicate
input, and cross-platform paths covered, and `case import` and
`archive import` read an export back into an archive.
What remains is importing a supported synthetic KRX
package through a versioned openKRX contract rather than a duplicated
parser.

## 4. Associate one supported receipt type

Import receipts and offer evidence-backed matching with explicit unresolved
and ambiguous outcomes. The user-asserted half is done: `receipt add`,
`receipt list`, `association create`, `association list`, and
`association retire` record, list, and withdraw the four outcomes with the
user's own evidence. What remains is receipt
parsing and derived evidence, which stay blocked on the format gap, so every
association this build writes carries `created_by` `user`. Keep imported,
matched, and cryptographically verified states separate. Signature
verification is a separate integration milestone; no matching result asserts
authenticity, legal effect, or delivery by itself.

## 5. The local organiser

Make openPapir the reference local organiser for e-Papír correspondence
inside the evidence rules of the milestones above: no invented format, no
integration with the service, and no claim about delivery, authenticity, or
legal effect. The items below are in the order they are meant to happen, and
each names the gate that has to hold before it starts and its state as of this
page's last update.

- Restoring from an export (`case import`) reads a directory written by
  `case export` back into an archive, preserving the original bytes and
  reporting a duplicate rather than overwriting it, gated on the export layout
  in [local archive layout and storage design](archive-layout.md) staying the
  written contract: implemented.
- Case lifecycle and search (`case update`, the `case list` filters, and
  `search`) let a case change the fields its record already carries, let the
  list narrow by them, and let one word find any record the user typed it
  into, gated on those fields and their write stages being the ones
  [architecture and CLI contract](architecture.md) documents: implemented.
  `search` reads the user's own record text only, and no index is stored.
- The receipt-retrieval reminder (`archive status`) summarises what the
  archive holds and lists the submissions that carry a user-supplied date and
  that no live association names as associated or a candidate, each with the
  date by which the receipt would have to be retrieved from the delivery
  storage, so that the user can decide for themselves where to look;
  submissions carrying no usable date are counted rather than listed. It is
  gated on the
  operator's own descriptive statement that the storage retains incoming
  documents for 30 days ([receipt evidence and local case model
  decisions](receipt-discovery.md), source E1, retrieved 2026-09-09), which
  the tool restates as that operator's description while reading no
  mailbox and asserting no deadline of its own: implemented.
- The entangled-deletion remedy (`association retire`) marks an association
  withdrawn instead of removing it, so that separating a receipt from a case
  leaves the earlier record readable, gated on the record and deletion
  contracts in [architecture and CLI contract](architecture.md): implemented.
- Generative testing drives the archive operations with generated inputs to
  reach the orderings and limits the example-based tests in
  [testing and fixture policy](testing.md) do not, gated on nothing outside
  the workspace: implemented.
- A release pipeline and a Windows-target lint publish a checked build and
  keep the cross-platform path and permission rules honest on the target that
  differs most, gated on [releasing](releasing.md) recording the release
  position that this work changes: planned.
- Shell completions and man pages ship the command surface in the forms a
  shell and a terminal already read, gated on the command set being settled
  enough that the generated files do not contradict the binary: planned.
- An end-to-end user guide walks one archive from the first import through
  association and export, gated on every operation it walks through being
  implemented: planned.
- A whole-archive export (`archive export`) writes every case, submission,
  receipt, association, and import event in one pass, with the same preserved
  originals as `case export` and the archive marker beside the manifest, and
  `archive import` reads it back as one set, gated on `case import` existing
  so that the result could be read back at all: implemented.
- Derived metadata on explicit request records the file type and the size of a
  stored object and nothing else, never parsing a receipt, gated on the
  derived-metadata design in
  [local archive layout and storage design](archive-layout.md) and on the
  request being explicit rather than implied by an import: implemented as
  `archive derive`.
- Encrypted backup at rest protects a copy of the archive kept outside it, and
  never the live archive. Its key handling is now decided in
  [local archive layout and storage design](archive-layout.md): a standard AEAD
  container over a tarball of the export shape, a key derived from a passphrase
  the user holds with a memory-hard KDF, and no key stored by openPapir. It is
  gated on the one thing that design leaves open, a dependency review that
  admits a container crate under the licence and audit constraints recorded
  there: planned.

## Later, subject to evidence

Receipt parsing and the derived evidence built on it, KRX package import,
delegated `.es3` verification, and any integration with the e-Papír service
stay behind their blockers: the format gap recorded in
[receipt evidence and local case model decisions](receipt-discovery.md), the
openKRX profile and reader discovery of milestone 1, and the absence of a
documented verification or submission contract. Additional receipt types, a
user interface, authentication, and background processing each require
independent discovery and acceptance criteria and may follow demonstrated
needs. None is implied by these milestones.

## Work orchestration

Turn each discovery result into one bounded issue per change. Include source
provenance, supported subset, non-goals, acceptance criteria, tests, and
dependency issues. Mark unresolved external contracts as blockers rather than
filling gaps
with assumptions. Contributors work on one issue per branch; review and CI are
required before merging to `develop`. The configured issue tracker owns live
work state; this roadmap records design sequencing, not queue status.
