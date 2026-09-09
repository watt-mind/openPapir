# Factory and runner setup

openPapir supports an explicitly launched Claude master orchestrator.
The repository includes portable instructions and Factory configuration;
each runner still needs its own checkout, tools, credentials and private
registry entry. A local setup does not provision another machine.

## Fresh checkout

Use the `develop` branch of `watt-mind/openPapir`. From its root:

```sh
python3 scripts/prepare-orchestrator.py
```

Then ask Claude: "Read ./tmp/orchestrator.md and begin orchestrating."
The script refuses to replace an existing file, preserving operator notes.
`tmp/` is ignored. The tracked source is [orchestrator.md](orchestrator.md),
which Claude can also read directly. Copying only Git history does not carry
private local handoff notes or Factory registration to another runner.

## Host registration

Locate the installed Factory checkout using its CLI and set `FACTORY_ROOT`
to that verified location for this shell. Read its onboarding instructions
and registry schema before editing its ignored `config/repos.yaml`.
Preserve all existing entries. Register the following values using the
runner's actual absolute checkout path and maintainer-confirmed tracker team:

```yaml
repos:
  - name: openpapir
    path: /absolute/runner/checkout/openPapir
    github: watt-mind/openPapir
    team: REPLACE_WITH_PRIVATE_TEAM
    project: openPapir
    control_plane: linear
    base: develop
    deploy_branch: master
    report_only: true
    max_in_flight: 1
    verify: bash scripts/check.sh && cargo build --release --locked
```

This is a template, not a second registry to commit. Replace placeholders
before use. The root `.factory.yaml` supplies portable branch, toolchain and
merge-check settings. Keep host identity, tracker mapping, credentials,
verification command and concurrency in the host registry. Do not write
tokens into either configuration file.

`report_only: true` keeps automatic dispatch disabled. The human-launched
Claude workflow performs explicit claims and delegates using its installed
agent tools. Do not run a second coordinator for the same project or hand
the same queue to the central event runtime. This repository has no
event-runtime worktree lifecycle scripts; unattended dispatch has not been
configured. Plain Git worktrees suffice for the manual stateless CLI flow.

## Tools and instruction delivery

Install authenticated Claude, GitHub CLI and tracker access through the
runner's normal credential setup. Never copy credential files into the repo.
Install Rust stable and 1.88, Git, Python 3, Node/npm, cargo-machete,
cargo-deny and actionlint. Coverage needs cargo-llvm-cov plus LLVM tools;
local security scans need Gitleaks. See [testing.md](testing.md) and current
CI for the actual versions and checks. Factory additionally requires Bun.

The portable prompt works without repository-local slash commands. If the
runner uses `/factory-*`, link only this checkout's ignored
`.claude/commands/factory-*.md` to the installed Factory command bundle.
Do not blindly run a global emitter over unrelated repositories. Recreate
local links for any worktree whose harness needs them; Git does not copy
ignored symlinks. Pass the full ticket instructions to implementation agents.

## Readiness checks

After registration, run these read-only checks:

```sh
factory doctor --repo openpapir
factory queue --repo openpapir
bash scripts/check.sh
cargo build --release --locked
```

Inspect every refusal or warning. The queue should identify this project's
eligible work while reporting automatic dispatch disabled. Missing worktree
lifecycle scripts are expected for manual mode; missing authentication,
toolchain, tracker routing or failing verification must be resolved before
claiming. Do not call `factory next --apply`, start services, or change
dispatch mode as a readiness test.

The orchestrator owns claims and serial merges to `develop`; each worker
owns one ticket and worktree. A cold reviewer and every applicable CI check
must pass before merge, and post-merge CI must pass before `Done`.
`master` and releases remain human-controlled.
