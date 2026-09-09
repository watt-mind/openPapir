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

The executable creates a local archive, imports files into it, organises what
it holds into cases and submissions, records receipts and the user's own
assertions about them, checks the whole archive against what its records
claim, copies one case out of the archive, and narrows a restored archive's
permissions back to owner-only. These invocations exist and nothing else:

| Invocation | Result |
| --- | --- |
| `openpapir --help` | Usage text from the argument parser. |
| `openpapir --version` | The crate version. |
| `openpapir capabilities [--json]` | The project, its stage, and the thirteen implemented operations. |
| `openpapir archive init <root> [--json]` | Creates an archive in an existing, empty directory: the marker first, then the owner-only layout. |
| `openpapir import --archive <root> <file>... [--json]` | Stores each file's original bytes in the content-addressed artefact store and records one import event per input. |
| `openpapir case create --archive <root> --title <t> [--notes <n>] [--json]` | Records one case, the user's own folder of related correspondence. |
| `openpapir case list --archive <root> [--json]` | Lists every case in the archive. |
| `openpapir case show --archive <root> <case-id> [--json]` | Shows one case with the submissions recorded against it and their artefact references. |
| `openpapir submission add --archive <root> --case <case-id> --description <d> [--date <yyyy-mm-dd>] [--artefact <digest>[:<role>]]... [--json]` | Records one submission the user states they sent, against a case. |
| `openpapir receipt add --archive <root> --artefact <digest> [--import-event <id>] [--label <l>] [--json]` | Records that the user believes one stored artefact to be a receipt. |
| `openpapir receipt list --archive <root> [--json]` | Lists every receipt in the archive. |
| `openpapir association create --archive <root> --receipt <receipt-id> --outcome <outcome> [--candidate <submission-id>:<confidence>:<statement>]... [--supersedes <association-id>] [--json]` | Records what the user asserts about one receipt, with one of the four outcomes. |
| `openpapir association list --archive <root> --receipt <receipt-id> [--json]` | Lists one receipt's whole association history, newest first. |
| `openpapir archive check --archive <root> [--json]` | Re-digests every stored object and reports, in counts only, what disagrees with the records. It takes no lock and changes nothing. |
| `openpapir case export --archive <root> --case <case-id> --to <dir> [--json]` | Copies one case's objects byte for byte, writes its records as JSON, and writes a manifest, into a destination outside the archive. It changes nothing in the archive. |
| `openpapir archive repair-permissions --archive <root> [--json]` | Narrows every path in the archive back to owner-only and reports the counts it changed. It only ever narrows. |

The exact envelope, the storage guarantees, the input caps, the implemented
error codes, the exit-code mapping, and the privacy rule that binds all output
are specified in [architecture and CLI contract](architecture.md), which is the
canonical description of implemented behaviour. `verified` is `false` in every
response, because no cryptographic check is implemented: a digest is a
storage-layer identity only.

Every record is the user's own local organisation: openPapir sends nothing, so
a submission is always user-asserted, and a user-supplied date is stored
verbatim and never interpreted as a delivery or receipt date. A receipt
records that the user believes a stored artefact to be a receipt, and an
association records what the user asserts about whether that receipt relates
to a submission, with one of the four outcomes `unassociated`, `candidate`,
`associated`, and `contradictory`, an ordinal confidence, and append-only
supersession.

Automatic matching and derived metadata remain unimplemented, and so does
every extractor: openPapir reads artefact bytes only to re-digest a stored
object during the integrity check and never to form an opinion of its own, so
every association carries `created_by` `user`. The integrity check is
read-only: it repairs nothing, removes nothing, and a passing check is
storage integrity rather than authenticity. No derived-metadata or
verification record exists, and there is no deletion, editing of a stored
record, migration, receipt parsing, signature verification, or government
delivery. An export is a plain copy outward: it converts nothing, and
importing an export back into an archive is not implemented. A backup stays a
plain copy of the archive root, and the permission repair is the documented
way to make a restored copy usable again.

## Decided designs, awaiting implementation

Each document below decides something for review. None of them is implemented,
and none of them changes the capabilities output.

| Document | What it decides |
| --- | --- |
| [receipt evidence and local case model decisions](receipt-discovery.md) | What authoritative public sources actually state about one candidate receipt type, the smallest useful local case model, and which questions stay open. |
| [local archive layout and storage design](archive-layout.md) | The storage technology, the on-disk layout, the record shapes, and the deletion, permission, and atomic-write semantics of the local archive. |
| [import and association error, JSON, and exit-code contract](error-contract.md) | How a command extends the JSON envelope with an error object and warnings, the stable error-code catalogue, and the exit-code mapping. |

Archive creation, artefact import, the case, submission, receipt, and
user-asserted association records, the whole-archive integrity check, case
export, and the permission repair are the parts of those two documents that
are now implemented, and their contract has moved to
[architecture and CLI contract](architecture.md). The rest of both documents,
including derived metadata, verification results, automatic association,
import from an export, deletion, and migration, is still only decided. A record
shape or code named there is a proposal, not a promised schema. It becomes a contract
only when the implementing pull request adds it to
[architecture and CLI contract](architecture.md).

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
verified. Imported and matched are implemented as separate records: recording
an association creates no verification result, and nothing about a match is
written into the receipt or its artefact. A delegated verification result must
retain the exact scope of the check, the verifier, and the supplied trust
context.

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
