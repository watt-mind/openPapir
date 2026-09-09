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
input, and cross-platform paths covered. What remains is importing a supported
synthetic KRX package through a versioned openKRX contract rather than a
duplicated parser, and importing an export back into an archive.

## 4. Associate one supported receipt type

Import receipts and offer evidence-backed matching with explicit unresolved
and ambiguous outcomes. The user-asserted half is done: `receipt add`,
`receipt list`, `association create`, and `association list` record and list
the four outcomes with the user's own evidence. What remains is receipt
parsing and derived evidence, which stay blocked on the format gap, so every
association this build writes carries `created_by` `user`. Keep imported,
matched, and cryptographically verified states separate. Signature
verification is a separate integration milestone; no matching result asserts
authenticity, legal effect, or delivery by itself.

## Later, subject to evidence

Search, additional receipt types, delegated `.es3` verification, and a user
interface may follow demonstrated needs. Government submission, authentication,
and background processing each require independent discovery and acceptance
criteria. None is implied by these milestones.

## Work orchestration

Turn each discovery result into one bounded issue per change. Include source
provenance, supported subset, non-goals, acceptance criteria, tests, and
dependency issues. Mark unresolved external contracts as blockers rather than
filling gaps
with assumptions. Contributors work on one issue per branch; review and CI are
required before merging to `develop`. The configured issue tracker owns live
work state; this roadmap records design sequencing, not queue status.
