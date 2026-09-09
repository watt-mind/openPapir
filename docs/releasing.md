# Releasing

There is no release process yet, and this document says so on purpose rather
than describing one that does not exist. It records the current position and
the checklist a first release would have to satisfy.

## Current position

| Fact | Value |
| --- | --- |
| Published releases | None. No tag, no GitHub release, no artefact. |
| Crate publication | Disabled. Both workspace crates set `publish = false`. |
| Release pipeline | None. No workflow builds, signs, or uploads an artefact. |
| Stable branch | `master`, reserved for human-controlled releases. |
| Integration branch | `develop`. Every pull request targets it. |
| Version | The workspace version in `Cargo.toml`. It is not a release claim. |

Nothing in the repository produces a distributable binary for anyone else.
Users build from a checkout, as [README.md](../README.md) describes.

## Branch promotion

`develop` is where reviewed work lands. Promotion from `develop` to `master`
is a human decision, taken deliberately, not an automatic consequence of a
green pipeline. No automation opens, approves, or merges a promotion pull
request, and no agent may merge one.

The reason is that `master` is the branch a reader treats as the project's
stated position. While the project is unreleased, that position must not move
because a check went green; it moves when a maintainer decides the claim on
`master` is still accurate.

## What a first release would need

None of the following is set up. This is the checklist, not a procedure to
follow today.

1. **An honest scope statement.** A release describes what the binary does.
   The current answer is help, version, and capabilities reporting, which is
   not worth releasing. A first release waits for one complete offline
   workflow; see [roadmap and discovery gates](roadmap.md).
2. **A version bump.** Decide the version in `Cargo.toml`, and decide whether
   the JSON envelope's `schema_version` moves with it. The envelope is
   versioned separately from the crate; see
   [architecture and CLI contract](architecture.md).
3. **A changelog cut.** Move the accumulated entries from `Unreleased` into a
   dated version section in [CHANGELOG.md](../CHANGELOG.md), keeping the
   Keep a Changelog categories.
4. **A green baseline.** `./scripts/check.sh`, the release build, and the
   full CI matrix pass on the promotion pull request, not only on `develop`.
5. **A tag.** Tag the merge commit on `master`. Tags are not created on
   `develop` and are not created by automation.
6. **An artefact policy.** Undecided. Whether a release ships prebuilt
   binaries, a container image, published crates, or source only, and how
   those artefacts would be checksummed and attested, has not been chosen. A
   release that ships an artefact needs that decision documented here first,
   with its pinned workflow, before the workflow is added.
7. **A security statement.** [SECURITY.md](../SECURITY.md) currently says
   security fixes target `develop` and that there are no released versions.
   A first release has to state which versions receive fixes.

## What this document is not

It does not authorise a release, and it does not describe a pipeline that
exists elsewhere. If a release process is ever added, it is specified here in
the same pull request that adds it, and the "Current position" table above is
corrected at the same time. Until then, any statement that openPapir has a
release is wrong.
