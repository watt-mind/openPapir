# Documentation

Start at [README](../README.md) for what openPapir is and how to build it,
then [specification.md](specification.md) for the single index of scope,
implemented behaviour, decided designs, and deferred contracts.

The implementation is early: help, version, and the operations
`capabilities` reports exist, which are archive creation, artefact import, the
case, submission, receipt, and user-asserted association records, the update
of one case record, the withdrawal of one assertion, the read-only
whole-archive integrity check, the read-only summary and its
receipt-retrieval reminders, the export of one case and the import of such an
export back into an archive, the permission repair, deleting a case with an
explicit purge, and writing the embedded agent skill document. The one
enumeration of them is the table in
[architecture and CLI contract](architecture.md).
Every document below separates implemented behaviour from plans, and the
remaining correspondence workflows, including automatic matching, derived
metadata, any receipt parsing, and any verification, are not available yet.

Every file under `docs/` appears in the first table. The rules that keep these
documents correct, including which document each kind of change must update,
are in the Documentation section of [contributing](../CONTRIBUTING.md).

## Documents under `docs/`

| Document | Purpose |
| --- | --- |
| [specification.md](specification.md) | Top-level index of purpose, scope, non-goals, implemented behaviour, decided designs, deferred contracts, and the separated receipt states. |
| [architecture.md](architecture.md) | Canonical contract of what is implemented: crate responsibilities, one section per implemented command, the response envelope, the storage guarantees, the input caps, the measured cost of the linear scans, the implemented codes and exit codes, the privacy rule, planned ownership, and the integration boundary. |
| [guide.md](guide.md) | End-to-end walkthrough of one matter on a synthetic example, with the real command output at every step, the receipt-retrieval window and the attributed sources behind it, and a short section for agents. |
| [roadmap.md](roadmap.md) | Milestone sequencing and the discovery gates that must close before implementation, plus how work is split into bounded issues. |
| [receipt-discovery.md](receipt-discovery.md) | Discovery note recording what public sources state about one candidate receipt type, a proposed local case model, and the questions that stay open. |
| [archive-layout.md](archive-layout.md) | Design for review of the local archive: storage technology, on-disk layout, record shapes, association states, and export, backup, and deletion semantics. Partly implemented; architecture.md is authoritative for the implemented part. |
| [error-contract.md](error-contract.md) | Specification for review of the import and association error model: the extended JSON envelope, the stable error-code catalogue, and the exit-code mapping. Partly implemented; architecture.md is authoritative for the implemented part. |
| [testing.md](testing.md) | Test layout, the coverage expectation, the scan benchmark and how to run it, the synthetic public fixture policy, and the rules for any future private input. |
| [references.md](references.md) | Related independent projects, discovery entry points, and the evidence policy every future format decision must satisfy. |
| [releasing.md](releasing.md) | The current release position, which is that nothing is published, the artefact policy the release workflow follows, and the checklist a first release still needs. |
| [factory.md](factory.md) | Runner setup for an explicitly launched orchestration run: tools, credentials, and checkout expectations. |
| [orchestrator.md](orchestrator.md) | Portable master orchestrator instructions, copied to an ignored local file before use. |
| [index.md](index.md) | This page: the purpose of every document and root policy file. |

## Root policy files

| File | Purpose |
| --- | --- |
| [README.md](../README.md) | Front door: what openPapir is, its honest status, how to build and run it, and its intended responsibilities. |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | How to propose and land a change: branch model, required checks, review expectations, and the documentation policy. |
| [SECURITY.md](../SECURITY.md) | How to report a vulnerability privately, the current security scope, and the requirements future features must meet. |
| [CHANGELOG.md](../CHANGELOG.md) | Keep a Changelog record of user-visible and contributor-visible change, with an entry added by every pull request. |
| [AGENTS.md](../AGENTS.md) | Instructions and hard boundaries for automated contributors, plus the repository layout and who may change each path. |
| [LICENSE](../LICENSE) | The MIT licence covering the source; fixtures are covered separately under CC0. |
| [CLAUDE.md](../CLAUDE.md) | Pointer file directing Claude to `AGENTS.md` and the orchestrator instructions. |
| [GEMINI.md](../GEMINI.md) | Pointer file directing Gemini to `AGENTS.md`. |

## Documents outside `docs/`

Two documents describe the contract from the outside and are kept correct by
the same rules.

| File | Purpose |
| --- | --- |
| [Agent skill](../crates/openpapir-cli/skills/openpapir/SKILL.md) | The document `openpapir skill` writes byte for byte: how an AI agent should drive the CLI, the envelope, the exit codes, the privacy rule, and the boundary between imported, matched, and authenticity-verified. |
| [Golden output contract](../tests/golden/README.md) | The pinned CLI output under `tests/golden/`, the placeholders it normalises, and how to regenerate it deliberately. |
