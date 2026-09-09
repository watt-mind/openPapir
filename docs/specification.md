# openPapir specification index

This page is the single entry point to what openPapir is, what it does today,
what has been decided for review, and what is still blocked. It indexes the
other documents; it does not restate them. Where this page and a linked
document disagree, the linked document is authoritative and this page is a
defect.

## Purpose

openPapir is a local-first Rust library and command-line tool for organising
Hungarian government correspondence: cases, submissions, attachments, and
receipts. It is an independent open-source project. It is not the government's
e-Papír service, is not affiliated with its operators, and does not submit
correspondence to it.

The intended value is that a person or an office can keep what they sent and
what came back in one local archive, with the original bytes preserved and the
provenance of every association recorded, without uploading anything.

## Scope

| In scope | Out of scope |
| --- | --- |
| A local archive of cases, submissions, attachments, and receipts. | Any government submission, delivery, or authentication path. |
| Preserving imported original bytes unchanged. | KRX container parsing and creation, which belong to openKRX. |
| Recording associations with explicit evidence and confidence. | `.es3` dossier handling and signature verification, which belong to openSzigno. |
| A CLI and a structured JSON output for scripted use. | A web interface, a background service, telemetry, or any network call. |
| Delegating cryptographic verification and reporting its exact scope. | Asserting legal effect, delivery, or authenticity from an association. |

## Non-goals

- No claim of a universal e-Papír format. Support is defined by evidence for
  named, documented variants, and unsupported variants are recorded as such.
- No invented submission API. Government integration requires independent
  discovery of authorised access and real service contracts first.
- No duplication of a sibling project's parser inside this repository.
- No processing of real correspondence in this repository's tests, fixtures,
  issues, or logs. Public fixtures are synthetic; see
  [testing and fixture policy](testing.md).

## Implemented today

The executable is a scaffold. These invocations exist and nothing else:

| Invocation | Result |
| --- | --- |
| `openpapir --help` | Usage text from the argument parser. |
| `openpapir --version` | The crate version. |
| `openpapir capabilities` | Two human-readable lines naming the project, its stage, and that nothing is implemented or verified. |
| `openpapir capabilities --json` | One JSON object on stdout: the capabilities envelope. |

The exact envelope, its fields, and the meaning of the empty `operations`
list and of `verified: false` are specified in
[architecture and CLI contract](architecture.md), which is the canonical
description of implemented behaviour. No case storage, package import, receipt
matching, signature verification, or government delivery exists.

## Decided designs, awaiting implementation

Each document below decides something for review. None of them is implemented,
and none of them changes the capabilities output.

| Document | What it decides |
| --- | --- |
| [receipt evidence and local case model decisions](receipt-discovery.md) | What authoritative public sources actually state about one candidate receipt type, the smallest useful local case model, and which questions stay open. |
| [local archive layout and storage design](archive-layout.md) | The storage technology, the on-disk layout, the record shapes, and the deletion, permission, and atomic-write semantics of the local archive. |
| [import and association error, JSON, and exit-code contract](error-contract.md) | How a future command extends the JSON envelope with an error object and warnings, the stable error-code catalogue, and the exit-code mapping. |

A record shape or code named in those documents is a proposal, not a promised
schema. It becomes a contract only when the implementing pull request adds it
to [architecture and CLI contract](architecture.md).

## Deferred and blocked contracts

| Contract | State | Blocker |
| --- | --- | --- |
| Receipt format support | Deferred | No authoritative, redistributable specification of a receipt's structure and identifiers has been obtained. The unresolved findings in the receipt discovery note gate any parsing work. |
| KRX package import | Deferred | Depends on openKRX publishing a versioned reader contract and a named supported profile. This repository must not duplicate that parser. |
| Delegated authenticity verification | Deferred | Depends on openSzigno exposing a stable `.es3` verification interface that reports its scope and trust context. No cryptographic check is performed here. |
| Government submission and delivery | Out of scope for now | No authorised access, documented service contract, or recovery semantics exist. KRX creation alone establishes no ability to submit. |

## Separated receipt states

The three states below are independent, and the design must keep them
expressible on their own. They are requirements, not implemented transitions.

| State | Meaning | What it does not mean |
| --- | --- | --- |
| Imported | Bytes were accepted into the local archive. | That the artefact is genuine or relates to anything. |
| Matched | Evidence associates the receipt with a submission. | Delivery, legal effect, or authenticity. |
| Authenticity verified | A specified cryptographic check passed, in a stated trust context. | That any other check was performed, or that the document has legal effect. |

A receipt may be imported without being matched, and matched without being
verified. A delegated verification result must retain the exact scope of the
check, the verifier, and the supplied trust context.

## Plans and sequencing

[Roadmap and discovery gates](roadmap.md) records the milestone order:
discover supported inputs, design local cases, implement one offline import
workflow, then associate one supported receipt type. The roadmap records design
sequencing, not queue status.

[Releasing](releasing.md) records the current release position: there is no
release pipeline and no published version.

## Every document

[Documentation index](index.md) lists every file under `docs/` and every root
policy file with a one-line purpose. The rules for keeping these documents
correct, including which document a given kind of change must update, are in
the Documentation section of [contributing](../CONTRIBUTING.md).
