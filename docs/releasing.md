# Releasing

Nothing has been released. This document records the current position, the
artefact policy a release follows, and the checklist a first release still has
to satisfy. The pipeline exists; the decision to use it does not follow from
the pipeline being green.

## Current position

| Fact | Value |
| --- | --- |
| Published releases | None. No tag, no GitHub release, no artefact. |
| Crate publication | Disabled. Both workspace crates set `publish = false`. |
| Release pipeline | `.github/workflows/release.yml`. It has never run on a tag. |
| Artefact policy | Decided; see [artefact policy](#artefact-policy) below. |
| Stable branch | `master`, reserved for human-controlled releases. |
| Integration branch | `develop`. Every pull request targets it. |
| Version | The workspace version in `Cargo.toml`. It is not a release claim. |

No distributable binary has been published. Until one is, users build from a
checkout, as [README.md](../README.md) describes.

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
   Keep a Changelog categories. From the first published crate version,
   changes to `openpapir-core`'s public Rust API are logged as well, which the
   changelog rules in [contributing](../CONTRIBUTING.md) waive while no
   version is published.
4. **A green baseline.** `./scripts/check.sh`, the release build, and the
   full CI matrix pass on the promotion pull request, not only on `develop`.
5. **A tag.** Tag the merge commit on `master`. Tags are not created on
   `develop` and are not created by automation.
6. **An artefact policy.** Decided, and recorded in
   [artefact policy](#artefact-policy) below.
7. **A security statement.** [SECURITY.md](../SECURITY.md) says security
   fixes target `develop` while nothing is released. A first release has to
   replace that with the versions that receive fixes.

## Artefact policy

A release ships prebuilt command-line binaries and nothing else. No container
image is published, and no crate is published: both workspace crates keep
`publish = false`, so `openpapir-core` has no Rust API stability promise and
the changelog rule for that API stays waived until a crate is published.

### The artefacts

| Target | Runner | Archive |
| --- | --- | --- |
| `x86_64-unknown-linux-musl` | `ubuntu-latest` | `.tar.gz` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | `.tar.gz` |
| `aarch64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `x86_64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `.zip` |

Both Linux targets are musl, so the binary is statically linked and does not
depend on the host's C library. Each Linux and each Windows artefact is built
on a runner of its own architecture, so no cross-compilation tooling and no
emulation is involved. The one artefact built for another architecture is
x86_64 macOS, which the Apple silicon runner produces natively from the same
toolchain; it is therefore the one artefact the workflow cannot smoke test,
and every other artefact runs `openpapir capabilities --json` on the runner
that built it.

Each archive holds the binary, `LICENSE`, `README.md`, and `CHANGELOG.md`
under a single directory named `openpapir-<version>-<target>`. Beside each
archive is a `<archive>.sha256` file in the format `sha256sum --check` reads.
The checksum is written and verified on the runner that built the archive and
verified again from the collected artefacts before a draft release is created.

### Provenance

The repository is public and builds on GitHub-hosted runners, so
`actions/attest-build-provenance` is available to it. The release job attests
every archive and every checksum file, and a consumer can check an artefact
with `gh attestation verify <file> --repo watt-mind/openPapir`. Attestation is
not a signature by a maintainer and is not authenticity verification of any
document the tool handles; it states which workflow, at which commit, produced
the bytes. A dry run attests nothing, because a dry run distributes nothing.

### The workflow

`.github/workflows/release.yml` is the only thing that builds an artefact. It
pins every action to a commit SHA with its version comment, as
`.github/workflows/ci.yml` does, keeps `contents: read` at the top level, and
grants `contents: write`, `id-token: write`, and `attestations: write` to the
release job alone. Every build runs `cargo build --release --locked`.

It has three triggers.

- A push of a `v*` tag. The workflow refuses a tag whose commit is not on
  `master`, takes the version from the tag, and creates a **draft** GitHub
  release whose notes are the [CHANGELOG.md](../CHANGELOG.md) section for that
  version, with every archive and checksum file attached. A version with no
  changelog section fails the run. Publishing the draft is a separate human
  act.
- A manual `workflow_dispatch` with the `dry_run` input, which defaults to
  true. A dry run takes the version from `Cargo.toml`, builds and checks the
  same artefacts, uploads them to the workflow run, and creates no release and
  no tag. Run one with:

```sh
gh workflow run release.yml --ref <branch> -f dry_run=true
```

- A pull request that changes `.github/workflows/release.yml` itself. GitHub
  registers a `workflow_dispatch` trigger only from the default branch, so a
  change to this workflow could otherwise not be exercised before it is
  merged. The trigger is filtered to that one path, so it is a dry run on the
  pull requests that change the pipeline and nothing at all on the rest.

The workflow never creates a tag, never pushes, and never publishes a draft.

## What this document is not

It does not authorise a release. A pipeline that can build artefacts is not
permission to cut a release, and a green dry run is not a release. Any change
to the release process is specified here in the same pull request that makes
it, and the "Current position" table above is corrected at the same time.
Until a tag exists on `master` and a draft is published, any statement that
openPapir has a release is wrong.
