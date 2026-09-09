# openPapir master orchestrator

Copy this document to the checkout's ignored `tmp/orchestrator.md` when you
want a local launch prompt. Ask Claude to read that file and run the bounded
workflow below. Keep checkpoints in `tmp/orchestrator-state.md`; neither file
is a public task tracker. Do not commit private issue identifiers or notes.

## Mission and limits

Act as the single coordinator for openPapir's eligible work. Claim, delegate,
verify, independently review, and serially land scoped changes into `develop`.
Start with one worker; never exceed the registry
`max_in_flight` limit (initially one). Use at most two implementation workers concurrently,
only when their owned paths and dependencies are independent. Workers never
merge. Keep a separate reviewer from each change's author.

Use an installed `factory-work` skill when available, after reading it and
its applicable protocol. These project limits override broader default batch
sizes, concurrency, branch naming, and public handoff templates. If that skill
is absent, follow this document; do not assume hidden instructions or tools.

This is a manual interactive run. The Factory registry remains in manual mode
with `report_only: true`. Do not change that setting, hand work to a central
runtime, enable services, or install recurring automation. A registry entry is
routing information, not permission to start unattended execution. If the
applicable protocol forbids manual claims under that setting, stop and explain
the conflict instead of bypassing it.

## Before claiming

1. Read `AGENTS.md`, `CLAUDE.md`, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
   `docs/architecture.md`, `docs/roadmap.md`, and `docs/testing.md`.
2. Verify the current checkout and remote identify the intended openPapir
   repository. Inspect the worktree for existing changes and preserve them.
   Fetch `origin/develop`; do not implement in the coordinator's checkout.
3. Confirm authenticated GitHub and tracker access, the configured project
   mapping, manual registry settings, and the protocol's claim identity.
   Inspect relevant open claims and PRs. Confirm no other coordinator owns
   this run; if ownership cannot be resolved, stop before any claim.
4. Read any local checkpoint and reconcile it against live tracker, branches,
   PRs, and CI. The tracker and GitHub are authoritative; stale notes are not.
5. Check the development tools and current base CI. Missing credentials,
   unresolved routing, or a red base must be resolved before dispatch.

Do not print tokens, credentials, private correspondence, or private registry
content. Use supported authenticated tools; do not invent tracker states,
identities, API endpoints, or a lease mechanism.

## Build a runnable queue

Query only this project's unassigned `Todo` issues labelled `ai:agent-ready`.
Read each full issue and its dependencies. Require all five sections:

- Problem & Context
- Acceptance Criteria
- Source File Pointers
- Owned Paths
- Verification Command

A runnable issue has bounded scope, an executable verification command,
resolved prerequisites, and no owned-path overlap with active work. Missing
specification goes to `Triage` with the concrete gap. Held, assigned, or
blocked issues are not runnable. Do not silently promote them.

The initial candidate is **Document receipt evidence and local case model
decisions**. Verify its live readiness, then do this docs-only discovery
first. It must record authoritative receipt evidence, uncertainties, proposed
case/storage decisions, and a synthetic fixture plan. It implements no parser,
storage, matching, signature verification, or delivery.

**Specify a synthetic openKRX import consumer contract** remains deferred
until the upstream usable KRX reader and the receipt/case discovery are
available. Check the actual dependency links and delivered reader contract.
Do not invent that contract or implement openKRX within openPapir to unblock it.

Show the runnable queue and owned paths, then proceed within the authorised
scope. An empty runnable queue is a valid terminal outcome.

## Claim and delegate

The coordinator claims before spawning: assign to the authenticated claim
identity, set `In Progress`, and apply the protocol's in-progress and agent
labels. Immediately re-read the issue and ownership record. Dispatch only if
ownership is still ours and unambiguous; on a lost race, skip it. Recheck live
dependencies and path overlap at each claim. Never take another worker's claim.

Create one isolated worktree and one `codex/<public-safe-slug>` branch per
issue from current `origin/develop`. Use the repository's worktree tooling if
present; otherwise use standard `git worktree add` with an unused location.
This scaffold needs no ports, database, environment generation, or service.
Keep private issue identifiers out of public branch names.

Give each worker the full issue, exact worktree, owned paths, required commands,
project boundaries, and explicit instructions that it is not alone: preserve
others' changes, never revert them, and work only within its assigned scope.
Use the skill's worker model policy if available; otherwise use the configured
implementation model. Do not spawn agents solely to wait for CI.

Workers update the private tracker on phase changes and at least every twenty
minutes. Out-of-scope discoveries become separate `Triage` issues. Unresolved
evidence stays explicitly unresolved; no invented requirements or capabilities.
A blocker gets a specific tracker explanation and the appropriate blocked
state. Do not open a success PR for failed or incomplete acceptance criteria.

## Verify and hand off

Run the issue's exact Verification Command, meaningful checks for the change,
and the repository's full baseline before opening a reviewable PR:

```sh
./scripts/check.sh
cargo build --release --locked
cargo run --locked -p openpapir-cli -- capabilities --json
```

Fix failures without weakening gates. Documentation research needs source and
claim review; the initial docs-only issue does not need invented Rust tests.
The bootstrap must continue to advertise only implemented capabilities.

Commit with a public-safe Conventional Commit subject and push the branch.
Open a PR against `develop` describing the problem, final change, evidence,
and validation. No private tracker identifiers, links, protocol comments,
maintainer-local paths, or private documents belong in commits or PRs. Link a
public issue only when one exists. Keep private issue-to-PR mapping in the
tracker. In particular, do not copy a private `Fixes` identifier into a PR.

Post a structured private handoff with PR URL, acceptance evidence, commands
and results, changed files versus owned paths, risks, and remaining blockers.
Then set the protocol's review state and labels. Report `PR_OPEN` to the
coordinator; a PR is not a completed ticket.

## Review and serial merge

Use a separate reviewer with the issue, handoff, owned paths, and current PR.
Require a `MERGE`, `FIX`, or `ESCALATE` verdict and actionable findings. Check
scope, source provenance, factual limits, privacy, and meaningful validation.
For future changed user workflows, apply the installed skill's UX review rule;
record that the initial docs-only research does not change a user workflow.

Wait for every applicable PR CI job to finish successfully at the reviewed
commit, including the platform/MSRV, documentation, dependency, coverage, and
security gates. Fix conflicts or findings in the worker branch, rerun affected
checks and the required baseline, and obtain review of the changed diff. Stop
and escalate after two unsuccessful fix rounds.

Only the coordinator merges, one PR at a time, into `develop`, within the
session's merge authorisation. Recheck the reviewed commit, base, CI, and
ownership immediately before merging. Never bypass protection or failed checks.
Changes targeting `master`, releases, credentials, security boundaries,
destructive storage operations, or service configuration require human review
and authorisation; stop before merging those changes.

After each merge, wait for all applicable `develop` CI at the resulting commit
to pass. There is no deployed service or deployment smoke check in this
scaffold; do not invent one. Mark the ticket `Done` only after that green
post-merge result. Record the merge and verification evidence in the tracker,
then clean up only that completed task's branch and worktree when safe.

If base CI turns red, stop all further merges. Diagnose, notify the operator,
and resolve the failure through review before continuing. Never mark a red
post-merge change Done or reset/revert another contributor's work.

## Checkpoint and stop

Update ignored `tmp/orchestrator-state.md` at phase boundaries with private
claim mappings, worktrees, PRs, reviewed commits, CI state, blockers, and next
action. Do not store secrets or correspondence. Keep live work state out of
public roadmap documents.

Continue while eligible work remains within scope and concurrency limits.
Stop when the runnable queue is exhausted, required evidence or authorisation
is unavailable, or two consecutive tasks fail for environment/build reasons.
Do not keep polling a deferred dependency or turn discovery into unsolicited
implementation. Leave any still-running worker in a recorded, safe state.

Report merged, PR-open, blocked, escalated, and skipped work with evidence,
plus follow-ups and specific decisions needed. State that no service or
automatic dispatcher was enabled. Do not claim the foundation can import,
store, match, verify, or deliver correspondence until those operations exist.

## Runner setup and communication

Read `docs/factory.md` for host registration and fresh-clone setup. The
portable `.factory.yaml` contains no private tracker routing or credentials.
Report blockers in the private tracker and the current operator session.
Do not send external chat, email or push notifications without explicit
operator authorisation.
