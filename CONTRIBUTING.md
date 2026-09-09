# Contributing

openPapir is at the scaffold stage: the local archive exists as far as the
operations `capabilities` reports, and everything else is a plan.
Start with the [specification index](docs/specification.md) for what is
implemented and the [roadmap](docs/roadmap.md) for what comes next, and agree
a bounded issue before implementing a new workflow. Describe the problem,
scope, acceptance criteria, synthetic evidence, and required checks. Record
unresolved format or service contracts explicitly.

Use one issue per `codex/` branch and open pull requests against `develop`.
`master` is the stable branch. Keep changes focused and link the public issue
when one exists. Public descriptions must not disclose private tracker content,
maintainer-local paths, credentials, or real correspondence.

## Setup and checks

Use Rust 1.88 or newer and the checked-in lockfile. The workspace uses edition
2024 and `publish = false`; no package publication is part of the scaffold.

```sh
cargo build --workspace --locked
./scripts/check.sh
```

The local script checks formatting, Clippy, documentation, tests, and repository
policies. CI also exercises Linux, macOS, Windows, MSRV, dependency policy,
coverage, and security checks. Resolve failures before review; do not disable a
check to land a change. Install tools requested by the script or CI workflows
when running their additional checks locally.

Use Conventional Commits, for example `docs: clarify receipt discovery scope`.
The repository supplies `scripts/commit-msg.sh` for subject validation.

### Every check and the command that runs it

| Check | Command | Where |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | Local script, CI, pre-commit hook |
| Lints | `cargo clippy --workspace --all-targets --locked -- -D warnings` | Local script, CI, pre-commit hook |
| Tests | `cargo test --workspace --locked` | Local script, CI on Linux, macOS, and Windows |
| Rustdoc | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | Local script, CI |
| Unused dependencies | `cargo machete` | Local script, CI |
| Licences and advisories | `cargo deny --all-features check` | Local script; CI runs the same arguments through the pinned `cargo-deny` action |
| Relative Markdown links and anchors | `python3 scripts/check-doc-links.py` | Local script, CI |
| Source file length | `python3 scripts/check-file-length.py` | Local script, CI |
| Prose style, no em-dashes | `python3 scripts/check-prose.py` | Local script, CI |
| Markdown lint | `npx --yes markdownlint-cli2@0.18.1 "**/*.md" "#target" "#samples" "#refs" "#tmp" "#node_modules"` | Local script, CI |
| Workflow lint | `actionlint` | Local script, CI |
| Release build and CLI smoke | `cargo build --release --locked`, then `--help`, `--version`, and `capabilities --json` | CI on Linux, macOS, and Windows, and the baseline below |
| Minimum supported Rust | `cargo check --workspace --all-targets --locked` on Rust 1.88.0 | CI |
| Coverage threshold | `cargo llvm-cov --workspace --locked --fail-under-lines 90` | CI |
| Commit subject shape | `scripts/commit-msg.sh` | CI on every pull request commit, commit-msg hook |
| Secret scan, advisories, CodeQL | The `Security` workflow | CI |

The full local baseline is:

```sh
./scripts/check.sh
cargo build --release --locked
cargo run --locked -p openpapir-cli -- capabilities --json
```

## Documentation

Documentation is part of the product. People and agents read it before they
run anything, so a document that describes behaviour the code does not have is
a defect of the same weight as a wrong result.

Every change to behaviour, a contract, code, a flag, a field, or an exit
status updates the affected document in the same pull request. Concretely,
such a pull request updates all of:

1. [architecture](docs/architecture.md), the canonical implemented contract.
2. [specification](docs/specification.md), the index, when scope, a decided
   design, or a deferred contract moves.
3. [documentation index](docs/index.md), when a file is added or removed.
4. [CHANGELOG.md](CHANGELOG.md), with an entry under Unreleased.

### Where things live

| Content | Home |
| --- | --- |
| What the project is, its status, how to build it | `README.md`, the front door; keep it short |
| Scope, non-goals, what is implemented, what is deferred | `docs/specification.md`, an index only |
| The canonical contract of implemented behaviour | `docs/architecture.md` |
| A decided design that is not implemented yet | Its own document under `docs/`, linked from the specification index |
| Plans, milestones, and discovery gates | `docs/roadmap.md` |
| Test layout, fixtures, coverage | `docs/testing.md` |
| Sources, evidence rules, related projects | `docs/references.md` |
| Release position and its checklist | `docs/releasing.md` |
| The purpose of every document and root policy file | `docs/index.md` |
| Runner setup and orchestration procedure for an explicitly launched run | `docs/factory.md`, `docs/orchestrator.md` |
| Every user-visible or contributor-visible change | `CHANGELOG.md`, under Unreleased |

Every file under `docs/` is listed in [docs/index.md](docs/index.md) with a
one-line purpose. Adding a document without adding its row is an incomplete
change.

### Style

- Present tense, plain and precise English. Sentence-case headings.
- Wrap prose at 80 columns. Tables, code blocks, and URLs may run longer.
- No em-dashes. Use a period, colon, comma, or parentheses instead.
  `scripts/check-prose.py` fails on U+2014 outside a fenced code block.
- Use fenced blocks for every command, path, and JSON example, and tables for
  parallel facts.
- Prefer captured output to invented output. Run the binary and paste what it
  printed, then keep it current.
- Never describe a planned operation as implemented, and never imply a
  government integration, delivery, legal effect, or a verification result
  that no code produces. Say what is decided, and say that it is not built.

### Changelog entries

`CHANGELOG.md` follows [Keep a Changelog][keepachangelog] 1.1.0. Add the entry
under `## Unreleased` in the right category, `Added`, `Changed`, `Fixed`,
`Removed`, `Deprecated`, or `Security`. Describe the observable difference to
a user or a contributor, not the diff. Reference a pull request as `(#12)`.
Never reference a private tracker identifier.

[keepachangelog]: https://keepachangelog.com/en/1.1.0/

## Review expectations

Explain the resulting behaviour, its boundary, and how it was tested. Separate
implemented behaviour from plans. Extend tests for meaningful failure modes
and observable contracts, rather than mirroring implementation details.

Do not attach real government correspondence to an issue or pull request.
Provide a synthetic reproducer; see [testing](docs/testing.md).
Report vulnerabilities privately using [SECURITY.md](SECURITY.md).

## Local hooks

Install Lefthook, then run `lefthook install` once in your checkout. Hooks
check formatting, Clippy, and Conventional Commit subjects. CI repeats these
checks, so hooks are optional convenience rather than the enforcement boundary.
