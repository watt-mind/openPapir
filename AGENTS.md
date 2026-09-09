# openPapir

A local-first correspondence toolkit scaffold. Read [README.md](README.md),
the [specification index](docs/specification.md),
[architecture](docs/architecture.md), [roadmap](docs/roadmap.md),
[contributing](CONTRIBUTING.md), and [security](SECURITY.md) before changes.

## Repository layout

| Path | What lives there | Who may change it |
| --- | --- | --- |
| `crates/openpapir-core/` | The library: the capabilities value and, later, the local case model. | Any contributor, with tests. |
| `crates/openpapir-cli/` | The `openpapir` binary: argument parsing and output. | Any contributor, with tests. |
| `crates/*/tests/` | Contract tests over observable behaviour. | Any contributor. |
| `tests/fixtures/` | Synthetic, CC0 fixtures with recorded provenance. | Any contributor; never derived from real correspondence. |
| `docs/` | Specification index, implemented contract, designs, plans, policies. | Any contributor, in the same pull request as the change described. |
| `scripts/` | The local baseline checks and commit-subject validation. | Any contributor; never weakened to land a change. |
| `.github/workflows/` | CI and security workflows, with actions pinned to a commit SHA. | A maintainer; every pin keeps its `# vX.Y.Z` comment. |
| `.github/ISSUE_TEMPLATE/`, `.github/pull_request_template.md` | Issue and pull request forms. | A maintainer. |
| `Cargo.toml`, `Cargo.lock`, `deny.toml`, `clippy.toml`, `rust-toolchain.toml` | Workspace, dependency policy, lint, and toolchain configuration. | A maintainer; a dependency change is its own pull request. |
| `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` | Agent instructions and the pointer files to them. | A maintainer. |
| `tmp/` | Ignored local orchestrator state. | Never committed. |

## Documentation

Documentation is part of the product. A change to behaviour, a contract, a
code, a flag, a field, or an exit status updates the affected documents,
including `CHANGELOG.md`, in the same pull request. The rules, the fixed home
of each kind of content, and the style are in the Documentation section of
[contributing](CONTRIBUTING.md).

## Boundaries

- Only help, version, and `capabilities [--json]` exist today. Never document a
  planned operation as implemented or imply a government integration exists.
- Keep KRX processing in openKRX and `.es3` processing in openSzigno. Do not add
  unpublished sibling dependencies or copy their parsers into this project.
- Receipt import, association, and authenticity verification are distinct.
  Matching proves neither delivery nor legal effect. A delegated verification
  result must retain its exact scope and supplied trust context.
- Preserve original bytes when import is implemented. Storage, identifiers,
  migrations, deletion, and backup need a reviewed design before coding.
- Real correspondence is private. Never enumerate, print, hash, log, commit,
  or disclose private filenames, paths, metadata, payloads, signer information,
  or receipt identifiers. Private-corpus reporting is aggregate counts and
  stable error-code buckets only. Never derive public fixtures from it.
- Synthetic fixtures only, with provenance and a redistributable licence.
  Never commit keys, complete private-key PEM armour lines, secrets, or `.env`
  files. Generate any required test keys at runtime. Never allowlist a secret
  scanner rule to admit a key.
- Future input handling must bound archive expansion, XML complexity, and
  storage growth. Extraction must prevent traversal, symlink escape, and
  overwrites. Do not relax limits to accept additional input.

## Work ownership

Use one specified issue per `codex/` branch, with pull requests to `develop`.
`master` is the stable branch. Give concurrent contributors explicit file or
module ownership, preserve others' changes, and keep unrelated follow-ups in
separate issues. Public issues and PRs must contain no private project-tracker
content, local maintainer paths, or real documents. Vulnerabilities follow
[SECURITY.md](SECURITY.md), not public issues.

## Checks

```sh
./scripts/check.sh
cargo build --release --locked
cargo run --locked -p openpapir-cli -- capabilities --json
```

The check script is the local baseline. CI adds platform/MSRV checks,
coverage, dependency policy, and security scans. Changes to behaviour require
meaningful tests; document observable contracts and limitations. Keep files
small enough for the repository's file-length check and use Conventional
Commits. Read [testing](docs/testing.md) before adding fixtures or tests.

## Factory orchestration

For an explicitly requested orchestration run, read
[the master orchestrator instructions](docs/orchestrator.md) and
[runner setup](docs/factory.md). Claim and re-read the private ticket before
editing, use one isolated worktree per ticket, and obtain independent review
before serial merges to `develop`. Only green post-merge CI permits `Done`.
The repository privacy rules override generic Factory examples that expose
private tracker identifiers in public branches, commits or PRs.
