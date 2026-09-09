# Roadmap

Only the scaffold is implemented: Rust workspace, help/version/capabilities,
and quality and documentation foundations. The following milestones are plans.

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
records which of that note's questions stay open. Follow-up 5 of the archive
layout document is answered by
[import error, JSON, and exit-code contract](error-contract.md), which
specifies the JSON envelope, the error-code catalogue, and the exit-code
mapping for review. No part of any of these documents is implemented.

## 3. Implement one offline workflow

Deliver a small vertical slice: import a supported synthetic package, preserve
its original bytes, assign it to a case, and list/export that case. Integrate a
versioned openKRX contract rather than duplicating its parser. Cover resource
limits, interrupted writes, duplicate input, and cross-platform paths.

## 4. Associate one supported receipt type

Import receipts and offer evidence-backed matching with explicit unresolved
and ambiguous outcomes. Keep imported, matched, and cryptographically verified
states separate. Signature verification is a separate integration milestone;
no matching result asserts authenticity, legal effect, or delivery by itself.

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
