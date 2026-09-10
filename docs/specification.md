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
claim, summarises what it holds and what is still worth looking for, copies
one case out of the archive and reads such a copy back in, narrows a restored
archive's permissions back to owner-only, deletes a case on request, writes
the agent skill document it carries, and generates its own shell completions
and man page. These invocations exist and nothing else:

| Invocation | Result |
| --- | --- |
| `openpapir --help` | Usage text from the argument parser. |
| `openpapir --version` | The crate version. |
| `openpapir capabilities [--json]` | The project, its stage, and the operations it reports as implemented. |
| `openpapir archive init <root> [--json]` | Creates an archive in an existing, empty directory: the marker first, then the owner-only layout. |
| `openpapir import --archive <root> <file>... [--json]` | Stores each file's original bytes in the content-addressed artefact store and records one import event per input. |
| `openpapir case create --archive <root> --title <t> [--notes <n>] [--tag <t>]... [--status open\|closed] [--json]` | Records one case, the user's own folder of related correspondence, with its status and its tags. |
| `openpapir case list --archive <root> [--status <s>] [--tag <t>]... [--query <text>] [--json]` | Lists the cases in the archive that match every filter given. `--query` is a case-insensitive substring of the title or notes, matched in one linear scan with no index, and is never echoed back. |
| `openpapir case show --archive <root> <case-id> [--json]` | Shows one case, with its status, tags, and update time, the submissions recorded against it and their artefact references, and the receipts a live association ties to one of those submissions. |
| `openpapir case update --archive <root> <case-id> [--title <t>] [--notes <n>\|--clear-notes] [--status open\|closed] [--tag <t>]... [--untag <t>]... [--json]` | Rewrites one case record in place, keeping its identifier and creation time, and reports the names of the fields it changed. It is the one invocation that rewrites a stored record. |
| `openpapir submission add --archive <root> --case <case-id> --description <d> [--date <yyyy-mm-dd>] [--artefact <digest>[:<role>]]... [--json]` | Records one submission the user states they sent, against a case. |
| `openpapir receipt add --archive <root> --artefact <digest> [--import-event <id>] [--label <l>] [--json]` | Records that the user believes one stored artefact to be a receipt. |
| `openpapir receipt list --archive <root> [--json]` | Lists every receipt in the archive. |
| `openpapir association create --archive <root> --receipt <receipt-id> --outcome <outcome> [--candidate <submission-id>:<confidence>:<statement>]... [--supersedes <association-id>] [--json]` | Records what the user asserts about one receipt, with one of the four outcomes. |
| `openpapir association list --archive <root> --receipt <receipt-id> [--json]` | Lists one receipt's whole association history, newest first. |
| `openpapir association retire --archive <root> <association-id> [--reason <text>] [--json]` | Withdraws one assertion by writing a record that supersedes it and claims nothing. Nothing is edited or removed. |
| `openpapir submission show --archive <root> <submission-id> [--json]` | Shows one submission as it is stored, with every association naming it: the live heads first and the superseded records after them. |
| `openpapir receipt show --archive <root> <receipt-id> [--json]` | Shows one receipt as it is stored, with its whole association history, newest first and superseded records included. |
| `openpapir association show --archive <root> <association-id> [--json]` | Shows one association as it is stored, whether it is the live head of its chain, and the chain it belongs to: what it supersedes and what supersedes it. |
| `openpapir archive check --archive <root> [--json]` | Re-digests every stored object and reports, in counts only, what disagrees with the records. It takes no lock and changes nothing. |
| `openpapir archive status --archive <root> [--as-of <yyyy-mm-dd>] [--json]` | Summarises what the archive holds and lists the submissions whose receipt is still worth looking for in the delivery storage, inside the 30-day window the operator describes. It takes no lock and changes nothing. |
| `openpapir case export --archive <root> --case <case-id> --to <dir> [--json]` | Copies one case's objects byte for byte, writes its records as JSON, and writes a manifest, into a destination outside the archive. It changes nothing in the archive. |
| `openpapir case import --archive <root> --from <dir> [--json]` | Reads a directory `case export` wrote back into an archive: the objects through the artefact store and the records under their original identifiers, checked against the manifest before anything is written. |
| `openpapir archive repair-permissions --archive <root> [--json]` | Narrows every path in the archive back to owner-only and reports the counts it changed. It only ever narrows. |
| `openpapir case delete --archive <root> --case <case-id> [--purge] [--json]` | Deletes one case and its submissions, with the receipts and association histories tied only to them. Objects go only with `--purge`, and only when nothing that remains references them. |
| `openpapir skill` | Writes the embedded agent skill document to stdout, byte for byte and with nothing added. It takes no file and no `--json`, touches no archive, and exits `0`. |
| `openpapir completions <bash\|zsh\|fish\|powershell\|elvish>` | Writes one shell's completion script to stdout, generated from the command definition the parser uses. It takes no file and no `--json`, touches no archive, and exits `0`. |
| `openpapir manpage` | Writes the man page for the whole command tree to stdout as one roff stream, the page for `openpapir` first and then one page for each subcommand. It takes no file, no directory, and no `--json`, touches no archive, and exits `0`. |

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
storage integrity rather than authenticity.

Deletion is the one destructive operation, and it is explicit twice over: it
names one case, and it removes an object only when `--purge` says so. It
unlinks files openPapir created, reports counts and record kinds, persists no
summary, and does not erase data from the storage medium. No derived-metadata
or verification record exists, and there is no editing of a stored record
other than the case record `case update` rewrites, no
deletion of a single submission or receipt, no deletion of an archive, and no
migration, receipt parsing, signature verification, or government delivery. An
export is a plain copy outward and an import the same copy back inward: both
convert nothing, and an import is checked against the export's manifest before
it writes anything. A backup stays a plain copy of the archive root, and the
permission repair is the documented way to make a restored copy usable again.

## Decided designs, awaiting implementation

Each document below decides something for review. Each is partly implemented
at most, and the implemented part has moved to
[architecture and CLI contract](architecture.md), which is authoritative for
it. Nothing that is still only decided changes the capabilities output.

| Document | What it decides | What of it is still only decided |
| --- | --- | --- |
| [receipt evidence and local case model decisions](receipt-discovery.md) | What authoritative public sources actually state about one candidate receipt type, the smallest useful local case model, and which questions stay open. | All of it. No receipt is parsed and no finding of that note has code behind it. |
| [local archive layout and storage design](archive-layout.md) | The storage technology, the on-disk layout, the record shapes, the deletion, permission, and atomic-write semantics of the local archive, and the encrypted backup: the backup artefact only, a standard AEAD container over a tarball of the export shape, a passphrase-derived key with a memory-hard KDF, and no key stored by openPapir. | Derived-metadata and verification records, the rebuildable `cache/` index, export of a whole archive, schema migration, and the encrypted backup, whose remaining open point is the dependency review that admits a container crate. |
| [import and association error, JSON, and exit-code contract](error-contract.md) | How a command extends the JSON envelope with an error object and warnings, the stable error-code catalogue, and the exit-code mapping. | The reserved codes `lock.stale`, `path.traversal`, and `write.incomplete`, and every automatic or derived evidence shape. |

The archive, record, error, integrity, export, import, permission-repair, and
deletion operations listed above are the parts of the last two documents
that are now implemented. A record shape or a code named there and not in
[architecture and CLI contract](architecture.md) is a proposal, not a promised
schema. It becomes a contract only when an implementing pull request adds it
to that document.

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
workflow, associate one supported receipt type, then build the local
organiser. That fifth milestone sequences the work that turns the implemented
archive into a usable local organiser: restoring from an export, case
lifecycle and search, the receipt-retrieval reminder, the entangled-deletion
remedy, generative testing, a release pipeline and a Windows-target lint,
shell completions and man pages, an end-to-end user guide, a whole-archive
export, derived metadata on explicit request, and encrypted backup at rest.
Each item there is marked implemented or planned, and those markers are the
current state of that milestone; "Implemented today" above is what the
executable does now. Receipt parsing, KRX package import, delegated
`.es3` verification, and any integration with the e-Papír service stay behind
their blockers. The roadmap records design sequencing, not queue status.

[Releasing](releasing.md) records the current release position: the artefact
policy and the release workflow exist, and no version is published.

## Every document

[Documentation index](index.md) lists every file under `docs/` and every root
policy file with a one-line purpose. The rules for keeping these documents
correct, including which document a given kind of change must update, are in
the Documentation section of [contributing](../CONTRIBUTING.md).

The remaining documents under `docs/`, which the sections above do not link,
are:

| Document | Purpose |
| --- | --- |
| [testing and fixture policy](testing.md) | The test layout, the coverage expectation, and the synthetic public fixture policy. |
| [references and evidence policy](references.md) | Related independent projects, discovery entry points, and the evidence rules every future format decision must satisfy. |
| [runner setup](factory.md) | Tools, credentials, and checkout expectations for an explicitly launched orchestration run. |
| [master orchestrator instructions](orchestrator.md) | The portable orchestration procedure, copied to an ignored local file before use. |

Neither `factory.md` nor `orchestrator.md` describes product behaviour. They
are contributor tooling, and nothing in them changes what the binary does.
