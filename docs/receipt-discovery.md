# Receipt evidence and local case model decisions

Discovery note for [roadmap](roadmap.md) milestones 1 and 2. It records what
authoritative public sources actually state about one candidate receipt type,
proposes the smallest useful local case model, and lists what remains unknown.

Nothing in this note is implemented, beyond the retention window the
`archive status` reminders read this note for. The tool exposes help, version,
`capabilities`, and the operations `capabilities` reports, none of
which parses a receipt; see [architecture](architecture.md). This note does
not claim a universal e-Papír receipt format, does not describe a submission
API, and does not assert conformance with any government service.

Retrievals below were performed on 2026-09-09 and re-attempted on 2026-09-10 by
following the entry points in [references](references.md). Every attempt made
on 2026-09-10 is logged under "Retrieval attempts", with its route and its
result. Sources are linked, never copied into this repository; no third-party
schema or document content is redistributed here.

## Scope and method

- Candidate receipt type in focus: the **e-Papír "Feladási igazolás"**
  (submission receipt) that the service states a submitter receives in their
  personal delivery storage after a successful submission.
- Secondary terms first seen in unverified search snippets and since read on
  the issuing page itself (see F4), recorded but not adopted as a target:
  *Letöltési igazolás*, *Átvételi lehetőség értesítő*, *Át nem vett
  dokumentumról értesítő*, *Meghiúsulási igazolás*.
- Each claim below is labelled **normative** (a legal or operator requirement),
  **descriptive** (an operator's own account of behaviour), or **unknown**.
- Where a source could not be retrieved, that is recorded as a finding. No gap
  is filled by inference, and no identifier, field name, or format is invented.

## Retrieval attempts

Every attempt made on 2026-09-10 against the three sources that the 2026-09-09
pass left open, in the order tried. Routes are recorded so that a later reader
can tell a content gap from an access failure.

| Date | Source | Route | Result |
| --- | --- | --- | --- |
| 2026-09-10 | E5 | Direct fetch of the page URL | Failed, TLS chain incomplete |
| 2026-09-10 | E5 | Public web archive, availability and index queries | No snapshot of the page exists |
| 2026-09-10 | E5 | Direct fetch with the issuing intermediate certificate named by the server's own authority information access extension supplied to the client | Retrieved |
| 2026-09-10 | E6 | Direct fetch of the page URL | Failed, HTTP 403 |
| 2026-09-10 | E6 | Public web archive, availability query | No snapshot of the page exists |
| 2026-09-10 | E6 | Direct fetch with an ordinary desktop browser user agent | Retrieved |
| 2026-09-10 | E2 | Consolidated text at `njt.jog.gov.hu` | Retrieved in full, § 1 to § 158 with the annexes and the amendment footnotes |
| 2026-09-10 | E2 | Consolidated text at the shorter `njt.hu` host | Failed, no response |

Two observations from the log itself, both about access rather than content:

- The E5 failure was a server-side chain configuration, not a revoked or
  invalid certificate; the certificate validated once the intermediate that
  the server itself names was supplied. This is an observation about one
  retrieval, not an assessment of any operator.
- The E6 failure depended only on the request's user agent. Nothing on the
  page is access controlled.

## Evidence matrix

One subsection per source, each recording the same fields: the issuing
organisation, the source title, its version or date, the URL and section, the
retrieval date, the redistribution terms, the claim the source supports, and
the kind of that claim.

### E1: e-Papír Súgó, "Általános tájékoztató"

- Issuing organisation: Operator of the e-Papír service (imprint names
  IdomSoft Zrt. as the service provider).
- Source title: e-Papír Súgó, "Általános tájékoztató" section.
- Version / date: Page footer shows `verzió: 1.1.6 (2025-11-04)`.
- URL and section: <https://epapir.gov.hu/sugo> (accordion section
  "Általános tájékoztató"; "Impresszum" for the operator statement).
- Retrieved: 2026-09-09, retrieved.
- Redistribution terms: Not established.
- Claim supported: A submitter receives a *Feladási igazolás* in their
  personal delivery storage after a successful submission; the storage retains
  incoming documents, *igazolások* and *nyugták* for 30 days, with an option
  to move items to permanent storage; a letter may carry attachments up to
  25 MB; the accepted attachment format list includes `.pdf`.
- Kind: Descriptive.

### E2: 451/2016. (XII. 19.) Korm. rendelet

- Issuing organisation: Nemzeti Jogszabálytár (national consolidated law
  database).
- Source title: 451/2016. (XII. 19.) Korm. rendelet az elektronikus
  ügyintézés részletszabályairól.
- Version / date: Consolidated text dated 2024-07-01.
- URL and section: <https://njt.jog.gov.hu/jogszabaly/2016-451-20-22>
  (§ 6(10); § 30(3); § 12(1); § 133(1) to (4)).
- Retrieved: 2026-09-09 partially; 2026-09-10 in full.
- Redistribution terms: Established and restrictive. The database's own footer
  reserves all rights in the texts it publishes to its publisher. No
  redistribution permission is granted, so this repository links only.
- Claim supported: § 6(10) states that the secure delivery service provider
  sends the sender authentic confirmations (`hiteles igazolásokat`) about
  delivery; § 30(3) requires certain confirmations to be transmitted via the
  secure delivery service. § 133 is the e-Papír section: § 133(2) states that
  the attachment formats and the size limit are set by the service provider
  rather than by the decree, and § 133(4) requires the provider to make
  available to the submitter, or to deliver by the secure delivery rules, a
  confirmation evidencing at least the lodging of the submission, its point in
  time, and its content. § 133(4) also names the event that confirmation is
  issued about: the delivery of the submission or of the reply to it, or the
  failure of that delivery. § 12(1) lists the alternative conditions under which
  an electronic document counts as authentic, one of which is an advanced
  electronic signature or seal plus, where prescribed, a time stamp.
- Kind: Normative.

### E3: magyarorszag.hu tárhely szolgáltatások

- Issuing organisation: Operator of the magyarorszag.hu SZÜF portal.
- Source title: "Tárhely szolgáltatások: Személyes tárhely, Cégkapu,
  Hivatali kapu" service description.
- Version / date: No version or date shown on the page.
- URL and section: <https://magyarorszag.hu/szuf_ugyleiras?id=2b96c3a5-d636-4a82-8a4e-ee4a13fbdc5b>
- Retrieved: 2026-09-09, retrieved.
- Redistribution terms: Not established.
- Claim supported: A single "Hiteles Elektronikus Postafiók" surface provides
  storage for citizens, Hivatali kapu and Cégkapu holders, i.e. the receipt
  destination is a storage mailbox, not a per-application inbox.
- Kind: Descriptive.

### E4: e-Papír Felhasználói Kézikönyv

- Issuing organisation: e-Papír content management group; copy published by
  Emberi Erőforrás Támogatáskezelő (EMET).
- Source title: e-Papír Felhasználói Kézikönyv.
- Version / date: `v1.0`, `2018.02.21.` (read from the document's own title
  page text).
- URL and section: <https://emet.gov.hu/app/uploads/sites/2/2020/11/epapir_felhasznaloi_kezikonyv.pdf>
- Retrieved: 2026-09-09, retrieved but not usable.
- Redistribution terms: Not established.
- Claim supported: Establishes only that a manual with this title, version and
  date exists; see the finding in F2.
- Kind: Unknown.

### E5: NOVA.PACK information page

- Issuing organisation: Police (Országos Rendőr-főkapitányság)
  e-administration site.
- Source title: "NOVA.PACK szolgáltatás" information page on the inNOVA portal.
- Version / date: The page carries no version or date of its own. The only
  dated item in its form table is a single form, marked `v1.0` and
  `2021.12.01`.
- URL and section: <https://ugyintezes.police.hu/novapack> (whole page; the
  sections headed "A megoldás célja" and "A megoldás részletes leírása").
- Retrieved: 2026-09-10, retrieved (see the retrieval log and F3).
- Redistribution terms: Not established. The site's legal notice covers
  cookies only and grants no redistribution permission.
- Claim supported: NOVA.PACK is described as a machine-interface service for
  organisations that assembles a submission out of a form XML file, a PDF
  rendered from that XML by the portal's own form runner, and the supplied
  attachments, packs it into a KRX container, and encrypts it with the
  recipient's key obtained through the secure delivery service. The page
  states that sending the resulting package is a separate process that this
  service does not cover, and it describes no receipt, confirmation, or
  notification of any kind.
- Kind: Descriptive.

### E6: "Az elektronikus beadványok beküldését követő értesítések"

- Issuing organisation: Országos Bírósági Hivatal / Magyarország Bíróságai.
- Source title: "Az elektronikus beadványok beküldését követő
  értesítések".
- Version / date: The page carries no version or date. It states its own scope
  instead: matters started on or after 2018-01-01 that fall to the
  administrative competence of the court or of the office's president.
- URL and section: <https://birosag.hu/ugyfeleknek/elektronikus-ugyintezes/elektronikus-kapcsolattartas-birosagokkal/e-per/e-kapcsolattartas-az-egyes-ugytipusokban/az-elektronikus-beadvanyok-bekuldeset-koveto-ertesitesek>
  (the whole page; the two lists of notifications sent by the central system).
- Retrieved: 2026-09-10, retrieved (see the retrieval log and F4).
- Redistribution terms: Not established. The page carries no licence statement.
- Claim supported: The page names a family of distinct notification artefacts
  around one submission, not a single one: a *Feladási igazolás* issued
  automatically by the form submission support service once the submission
  meets the informatics requirements, a *Letöltési igazolás* when the office
  fetches the item from its storage, an *Automatikus válasz* carrying the
  reason when the submission cannot be processed, and, on the incoming side,
  an *Átvételi lehetőség értesítő*, an *Át nem vett dokumentumról értesítő*
  and a *Meghiúsulási igazolás*. The page further states that the
  *Feladási igazolás* names a point in time, that the central system signs the
  igazolás, that every notification the central system sends carries as its
  reference number the KR number belonging to the originally submitted item,
  and that the data an igazolás shows may differ between storage kinds. At
  this stage only the integrity of the KR envelope data is checked, the item
  itself being encrypted to the operator.
- Kind: Descriptive, for one authority's own account of its own channel; the
  KR envelope and the central system it describes are shared infrastructure,
  but this page is not a general e-Papír statement.

### Findings on evidence quality

- **F1: the decisive detail is absent from the normative text, and now that
  the whole text has been read the absence is a fact about the decree rather
  than about the retrieval.** E2 was retrieved in full on 2026-09-10, § 1 to
  § 158 with the annexes. It contains no occurrence of the term *Feladási
  igazolás*, no file format, no field list, no identifier syntax, and no
  encoding rule for any confirmation about a submission. What it does add is
  § 133(4), a minimum information content: a confirmation about a submission
  lodged through the e-Papír service evidences at least the lodging, its point
  in time, and its content. § 133(2) leaves attachment formats and the size
  limit to the service provider, so the format list in E1 is operator policy
  and can change without the decree changing. The receipt's byte-level
  structure therefore stays **unknown**, and the reason is now known: no
  consulted normative source fixes it.
- **F2: E4 could not be read.** The PDF uses subset fonts whose text could not
  be extracted reliably with the tooling available during this discovery, so no
  sentence from it can be quoted or relied on. It is also from 2018 and may not
  describe the service as it behaves now. No claim rests on it.
- **F3: E5 is reachable and says nothing about receipts.** The 2026-09-09
  TLS chain failure was an access failure; the page was retrieved on
  2026-09-10 through the route in the log. Read, it describes the outbound
  side only: how a machine-interface client has a submission assembled and
  packed into an encrypted KRX container, with sending stated to be a separate
  process the service does not cover. It names no confirmation and no
  notification. So E5 removes an unknown of the "unreachable" kind and adds
  none of the "receipt format" kind. It remains police-specific and
  machine-interface specific, and must not be read as general e-Papír access
  ([references](references.md) already says so).
- **F4: E6 is reachable, and it converts the earlier hearsay into an
  attributed source.** The 2026-09-09 HTTP 403 depended on the request's user
  agent; the page was retrieved on 2026-09-10 through the route in the log.
  It confirms that "receipt" is a family of artefacts rather than one type,
  and it is the first consulted source to name a correlation identifier: it
  states that every notification the central system sends carries the KR
  number of the originally submitted item as its reference number. It also
  states that the central system signs the igazolás and that the data an
  igazolás shows may differ between storage kinds. Its scope is one
  authority's own channel, so none of this is established for e-Papír
  generally, and it still gives no format, no field list, and no identifier
  syntax.
- **F5: redistribution terms are established for one source and restrictive;
  elsewhere they stay unestablished.** E2's publisher reserves all rights in
  the texts it publishes, so E2 may not be reproduced here. E1, E3, E4, E5 and
  E6 state no licence terms at all; E5's legal notice covers cookies only.
  No consulted source permits redistribution of its text, schemas, or sample
  documents. Link only; vendor nothing.
- **F6: no source consulted describes a machine-readable receipt schema or a
  receipt identifier syntax, and the one correlation identifier now named is
  outside the e-Papír scope.** E6 names the KR number as the reference number
  carried by the notifications of one authority's channel, which makes a
  documented submission-to-notification correlation field a thing that exists
  somewhere rather than a hypothesis. Nothing consulted states its syntax, its
  stability, its uniqueness, whether it is user-visible, or whether e-Papír
  confirmations carry it. The relationship between a submission and its
  receipt is therefore **narrowed but still not evidence-backed** for the
  candidate type in focus.

### What is and is not established

Established (with the labels above):

- A distinct artefact called *Feladási igazolás* exists and is delivered to the
  submitter's storage after a successful e-Papír submission (E1, descriptive).
- Authentic delivery confirmations are a regulated obligation of the secure
  delivery service provider, not an e-Papír convenience (E2 § 6(10),
  normative).
- The mailbox retains such items for 30 days unless moved to permanent storage
  (E1, descriptive), so a local archive is the durable copy, not the mailbox.
  This is the one place the 30-day figure comes from. `archive status`
  implements it as a single constant and states the same provenance in
  [architecture](architecture.md): the operator's help page states 30 days,
  retrieved 2026-09-09, descriptive rather than normative. openPapir does not
  enforce the window, reads no mailbox, and checks no service, so a reminder
  it prints is arithmetic over the user's own stated date and this published
  description, and asserts no delivery, no receipt by an authority, and no
  legal effect.
- A confirmation about an e-Papír submission carries at least the fact of the
  lodging, its point in time, and its content (E2 § 133(4), normative). This
  is an information minimum, not a format: it says what the confirmation must
  evidence and nothing about how the bytes carry it.
- The decree names the event that same confirmation is issued about: the
  delivery of the submission or of the reply to it, or the failure of that
  delivery (E2 § 133(4), normative). This is recorded as a statement about what
  the decree requires of the service provider, and it is explicitly not a claim
  about any artefact openPapir stores. openPapir reads no mailbox, contacts no
  service, and asserts nothing about whether a file it holds was delivered, was
  received by an authority, or carries any legal effect.
- Attachment formats and the size limit for an e-Papír submission are set by
  the service provider, not by the decree (E2 § 133(2), normative), so E1's
  format list and its 25 MB figure are operator policy and may change without
  notice to this repository.
- Around a submission there is a family of distinct notification artefacts,
  not one (E6, descriptive, for one authority's channel), and in that channel
  each of them carries the same reference number back to the submitted item
  (E6, descriptive).
- E5 describes only the outbound assembly and packing path and names no
  confirmation (E5, descriptive). This is established for E5 alone. It says
  nothing about what any other source describes, and nothing about the
  candidate receipt type beyond that one source.

Not established, and must not be assumed:

- The byte-level container of a *Feladási igazolás* (PDF, XML, `.es3`, KRX, or
  otherwise), its signature or seal profile, and whether a timestamp is
  present. E6 states that a central system signs the igazolás in one
  authority's channel, which is not a profile, not a scope, and not a
  statement about e-Papír.
- The syntax of any field that identifies the originating submission, and
  whether such a field is stable, unique, or user-visible. E6 names the KR
  number as one authority's reference number; nothing consulted states that an
  e-Papír confirmation carries it, nor how it is written.
- Whether the same artefact shape is produced for every case type, every
  authority, and every mailbox kind (personal, Cégkapu, Hivatali kapu). E6
  states the opposite for what is displayed in its own channel, so uniformity
  must not be assumed even as a working guess.
- Whether a receipt is ever reissued, superseded, or re-downloadable, and what
  that would mean for duplicate detection.
- Any submission API, polling interface, or automated retrieval path.

## Proposed local case model

Everything in this section is a **proposal** or an **open question**. No
storage technology is chosen, no schema is frozen, and no code follows from
this note. Terms below are internal to openPapir and are not claimed to match
any government field name.

### Shape

Proposed minimum, three entities plus one artefact table:

- **Case**: a user-created folder of related correspondence. Purely local; it
  corresponds to nothing the government issues.
- **Submission**: something the user sent, recorded from what the user has
  locally. openPapir does not send anything, so a submission is always an
  imported or user-asserted record.
- **Receipt**: an imported artefact the user believes to be a receipt.
- **Artefact (blob)**: the preserved original bytes of any imported file.

Proposed rules:

- Every imported file becomes exactly one artefact holding **unmodified
  original bytes**, stored write-once. Derived metadata (parsed fields,
  detected type, extracted text) lives in a separate record that references the
  artefact and records which extractor version produced it, so it can be
  discarded and recomputed without touching originals.
- A receipt references its artefact. A receipt never rewrites it.
- The association between a receipt and a submission is a separate record with
  its own evidence and confidence, never a foreign key implying certainty.

### Decisions proposed

1. **Identity is local and opaque.** Proposed: openPapir mints its own
   identifiers for cases, submissions, receipts, and associations. Government
   identifiers, if any are ever parsed, are stored as *attributes with a
   source*, never as primary keys. Rationale: F6. No identifier is known to be
   stable or unique.
2. **Content addressing for artefacts.** Proposed: address stored originals by
   a cryptographic digest of their bytes, so identical bytes are stored once.
   This is a storage-layer fact only and carries no authenticity meaning.
3. **Duplicate import is not an error.** Proposed: re-importing identical bytes
   records a second *import event* against the existing artefact rather than
   failing or silently ignoring the input. The user sees "already present,
   imported again on <date>".
4. **Ambiguity is a first-class outcome.** Proposed: association results are
   `unassociated`, `candidate` (one or more, each with evidence), `associated`
   (user-confirmed or unambiguous evidence), and `contradictory`. A candidate
   set is never collapsed to a single best guess automatically.
5. **Atomic writes.** Proposed: write to a temporary file in the same
   directory, fsync, rename into place, then fsync the directory; never mutate
   a stored original. An interrupted import must leave either the complete
   artefact or nothing.
6. **Restrictive permissions by default.** Proposed: the archive directory and
   its contents are created owner-only, and openPapir refuses to widen them.
7. **Bounded input.** Proposed: explicit, documented caps on single-file size,
   total import size, and per-import file count, refused before allocation.
   Container expansion, XML complexity, and extraction safety stay with
   [openKRX](https://github.com/watt-mind/openKRX) and
   [openSzigno](https://github.com/watt-mind/openSzigno) per
   [AGENTS.md](../AGENTS.md); openPapir does not implement them.
8. **Versioned on-disk layout.** Proposed: a schema version recorded in the
   archive, refusal to operate on a newer version, and forward migrations that
   never rewrite preserved originals.
9. **Export is a copy, not a conversion.** Proposed: export reproduces original
   bytes plus a sidecar of derived metadata, so a backup is restorable without
   openPapir.
10. **Deletion is explicit and complete.** Proposed: deleting a case offers to
    delete its artefacts; anything retained is reported. Deletion never leaves
    an orphan original on disk unreported.

### Open questions

- Which storage technology (plain files plus an index, an embedded database, or
  both) satisfies atomic writes, migration, and backup best? Undecided.
- Does the digest of a receipt's bytes actually distinguish two genuinely
  different receipts, or can the same submission yield byte-different receipts?
  Unanswerable until F1 and F6 are closed.
- What is the correct behaviour when the same artefact is plausibly a receipt
  for two different submissions? Proposed `contradictory`, but the user-facing
  resolution is undesigned.
- Should derived metadata be recomputed automatically on extractor upgrade, or
  only on explicit request? Undecided; it affects reproducibility of listings.
- What is the retention story for import events and association history: is
  history itself deletable? Undecided, and privacy-relevant.
- Are permissions and atomic-rename semantics achievable identically on
  Windows, and what is the documented degradation if not? Unresolved.
- Does a backup need to be encrypted at rest by default? Out of scope here;
  [SECURITY.md](../SECURITY.md) requires this to be reviewed before persistence
  exists.

### Contracts deliberately not chosen

No service contract is adopted. No dependency on a sibling project is proposed.
No JSON response schema for import, match, or verify commands is fixed here;
`capabilities` remains the only stable output and its envelope is not a promise
for other commands ([architecture](architecture.md)).

## Separation of states

The three states in [architecture](architecture.md) are preserved and
strengthened by the model above:

| State | Established by | Never implies |
| --- | --- | --- |
| Imported | Original bytes were stored, with an import event | That the file is a receipt, or is authentic |
| Matched | An association record with recorded evidence and confidence | Delivery, receipt by an authority, authenticity, or legal effect |
| Authenticity verified | A specified cryptographic check performed by a named verifier, with its exact scope and supplied trust context | That the associated submission was delivered or had legal effect |

Consequences for the model:

- The three states are separate records, not values of one status field, so a
  receipt can be imported and verified but unmatched, or matched but unverified.
- An association record must not be created as a side effect of verification,
  and a verification result must not be created as a side effect of matching.
- A verification result stores the verifier's identity and version, the exact
  artefact covered, and the trust context supplied by the user. Delegated
  `.es3` verification belongs to openSzigno and KRX handling to openKRX; their
  internals are out of scope for this repository.
- No output of any of the three states may be phrased as "delivered",
  "accepted", "official", or "legally effective".

## Synthetic fixture plan

This section describes fixtures; it adds none. When fixtures are added they go
under `tests/fixtures/` and follow [testing](testing.md), including that
directory's CC0 policy and its README provenance requirement.

Provenance rule: every fixture is generated by a committed generator in this
repository from constants written for the purpose. Nothing is derived from real
correspondence, even after redaction, and no third-party sample is copied in.

Proposed fixture families, all wholly synthetic:

- **Opaque artefact bytes**: small files of fixed, meaningless content used to
  exercise import, digesting, duplicate detection, and preservation. These make
  no claim to resemble a real receipt and are the only family that can be
  written before F1 is closed.
- **Import hazards**: an empty file, a file at and just over the configured
  size cap, a name with path separators and traversal segments, a name with
  non-ASCII and reserved Windows characters, and a symlink. Expected outcome is
  a stable refusal error code, not acceptance.
- **Interruption**: a fixture harness that aborts between write and rename to
  assert that the archive contains either the whole artefact or nothing.
- **Association scenarios**: hand-written local records (no government format
  involved) producing each of `unassociated`, `candidate` with one match,
  `candidate` with several, `associated`, and `contradictory`.
- **Receipt-shaped fixtures are deliberately deferred.** Modelling a plausible
  *Feladási igazolás* now would encode guesses as test expectations. This
  family is blocked on F1.

Cryptographic material, if ever needed, is generated at runtime inside the test
and never committed, per [testing](testing.md) and [SECURITY.md](../SECURITY.md).

## Bounded follow-ups

Each item below is a candidate implementation issue with its blocker. The
configured tracker owns live work state; this list records sequencing only.

1. **Close the receipt-format gap.** The retrieval part of this item is done:
   E2 has been read in full and its redistribution terms are established as
   all rights reserved. The gap is not closed, because the decree fixes an
   information minimum and no format. What remains is to find a source that
   states the encoding of one concrete confirmation, most plausibly an
   operator or delivery service technical description rather than a legal
   text. *Blocker:* F1. No consulted source fixes a format.
2. **Re-attempt the unreachable sources.** Done. E5 and E6 were both retrieved
   on 2026-09-10; see the retrieval log, F3 and F4. Neither closes the format
   gap; E6 narrows the correlation question for one authority's channel.
3. **Specify the local archive layout and storage technology.** Turn the
   proposals above into a decided layout with a schema version.
   *Blocker:* the open questions in this note; not blocked on external
   evidence, so this can proceed independently.
4. **Implement artefact import with byte preservation.** Content-addressed
   store, atomic writes, permissions, size caps, and duplicate import events.
   *Blocker:* follow-up 3.
5. **Define import and association error codes with their JSON and exit-code
   contract.** *Blocker:* follow-up 3.
6. **Implement association records with candidate and contradictory outcomes.**
   Using local evidence only, with no receipt parsing. *Blocker:* follow-up 4.
7. **Specify receipt parsing for one concrete receipt type.**
   *Blocker:* follow-up 1. No format is known.
8. **Specify the delegated verification boundary.** *Blocker:* published,
   versioned contracts from openKRX and openSzigno, which do not exist yet.

## Limits of this note

The evidence base is broader than it was on 2026-09-09: the normative source
has been read in full and the two unreachable sources were retrieved. It is
not deeper on the question that matters. Nothing retrieved on 2026-09-10
closes the receipt-format gap. E2 § 133(4) narrows it from one side by fixing
what a confirmation must evidence, and E6 narrows the correlation question
from another by naming a reference number, but that is one authority's own
channel and one authority's own account of it, and neither source states an
encoding. Conclusions about receipt structure are therefore still absent by
design rather than summarised, and receipt-shaped fixtures stay deferred. Do
not treat any proposal in the local case model section as a decision until it
is restated in a bounded issue with acceptance criteria.
