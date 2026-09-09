# openPapir

A local-first correspondence toolkit scaffold. Read [README.md](README.md),
[architecture](docs/architecture.md), [roadmap](docs/roadmap.md),
[contributing](CONTRIBUTING.md), and [security](SECURITY.md) before changes.

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
