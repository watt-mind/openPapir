# Contributing

openPapir is at the scaffold stage. Start with the
[roadmap](docs/roadmap.md) and agree a bounded issue before implementing a new
workflow. Describe the problem, scope, acceptance criteria, synthetic evidence,
and required checks. Record unresolved format or service contracts explicitly.

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
