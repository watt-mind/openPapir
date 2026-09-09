# Documentation

Start at [README](../README.md) for what openPapir is and how to build it,
then [specification.md](specification.md) for the single index of scope,
implemented behaviour, decided designs, and deferred contracts.

The implementation is early: help, version, capabilities reporting, archive
creation, artefact import, and the case and submission records exist. Every
document below separates implemented behaviour from plans, and the remaining
correspondence workflows are not available yet.

Every file under `docs/` appears in the first table. The rules that keep these
documents correct, including which document each kind of change must update,
are in the Documentation section of [contributing](../CONTRIBUTING.md).

## Documents under `docs/`

| Document | Purpose |
| --- | --- |
| [specification.md](specification.md) | Top-level index of purpose, scope, non-goals, implemented behaviour, decided designs, deferred contracts, and the separated receipt states. |
| [architecture.md](architecture.md) | Canonical contract of what is implemented: crate responsibilities, supported invocations, the capabilities JSON envelope, planned ownership, and the integration boundary. |
| [roadmap.md](roadmap.md) | Milestone sequencing and the discovery gates that must close before implementation, plus how work is split into bounded issues. |
| [receipt-discovery.md](receipt-discovery.md) | Discovery note recording what public sources state about one candidate receipt type, a proposed local case model, and the questions that stay open. |
| [archive-layout.md](archive-layout.md) | Design for review of the local archive: storage technology, on-disk layout, record shapes, association states, and deletion semantics. |
| [error-contract.md](error-contract.md) | Specification for review of the import and association error model: the extended JSON envelope, the stable error-code catalogue, and the exit-code mapping. |
| [testing.md](testing.md) | Test layout, the coverage expectation, the synthetic public fixture policy, and the rules for any future private input. |
| [references.md](references.md) | Related independent projects, discovery entry points, and the evidence policy every future format decision must satisfy. |
| [releasing.md](releasing.md) | The current release position, which is that no release process exists, and the checklist a first release would need. |
| [factory.md](factory.md) | Runner setup for an explicitly launched orchestration run: tools, credentials, and checkout expectations. |
| [orchestrator.md](orchestrator.md) | Portable master orchestrator instructions, copied to an ignored local file before use. |
| [index.md](index.md) | This page: the purpose of every document and root policy file. |

## Root policy files

| File | Purpose |
| --- | --- |
| [README.md](../README.md) | Front door: what openPapir is, its honest status, how to build and run the scaffold, and its intended responsibilities. |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | How to propose and land a change: branch model, required checks, review expectations, and the documentation policy. |
| [SECURITY.md](../SECURITY.md) | How to report a vulnerability privately, the current security scope, and the requirements future features must meet. |
| [CHANGELOG.md](../CHANGELOG.md) | Keep a Changelog record of user-visible and contributor-visible change, with an entry added by every pull request. |
| [AGENTS.md](../AGENTS.md) | Instructions and hard boundaries for automated contributors, plus the repository layout and who may change each path. |
| [LICENSE](../LICENSE) | The MIT licence covering the source; fixtures are covered separately under CC0. |
| [CLAUDE.md](../CLAUDE.md) | Pointer file directing Claude to `AGENTS.md` and the orchestrator instructions. |
| [GEMINI.md](../GEMINI.md) | Pointer file directing Gemini to `AGENTS.md`. |
