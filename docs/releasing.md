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

This is the checklist, not a procedure to follow today. Each item says where
it stands; a release happens when a maintainer decides every one of them is
satisfied, not when the last of them turns green.

1. **An honest scope statement.** A release describes what the binary does.
   The current answer is the set of operations `capabilities` reports, which
   covers the offline workflow end to end: create an archive, import files
   with their original bytes preserved, organise them into cases and
   submissions, record receipts and the user's own assertions about them,
   check an archive, copy a case or a whole archive out and read such a copy
   back, and delete a case. The scope statement also has to say what is
   absent, because receipt parsing, KRX and `.es3` handling, verification
   results, and any integration with the service stay blocked, and no output
   claims delivery, receipt by an authority, authenticity, or legal effect;
   see [roadmap and discovery gates](roadmap.md). Writing that statement is
   still to do.
2. **A version bump.** Decide the version in `Cargo.toml`, and decide whether
   the JSON envelope's `schema_version` moves with it. The envelope is
   versioned separately from the crate; see
   [architecture and CLI contract](architecture.md).
3. **A changelog cut.** Move the accumulated entries from `Unreleased` into a
   dated version section in [CHANGELOG.md](../CHANGELOG.md), keeping the
   Keep a Changelog categories. From the first published crate version,
   changes to `openpapir-core`'s public Rust API are logged as well, which the
   changelog rules in [contributing](../CONTRIBUTING.md) waive while no
   version is published. `Unreleased` is left empty by the cut, which is
   expected: the checks tolerate an empty `Unreleased` (see
   [the release notes](#the-release-notes)) and the next pull request fills
   it again.
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

Each archive holds the following under a single directory named
`openpapir-<version>-<target>`.

| Path in the archive | What it is |
| --- | --- |
| `openpapir`, or `openpapir.exe` on Windows | The binary for that target. |
| `LICENSE`, `THIRD-PARTY-NOTICES.md`, `README.md`, `CHANGELOG.md` | Copied from the built tree. |
| `completions/openpapir.bash` | The `bash` completion script. |
| `completions/_openpapir` | The `zsh` completion script. |
| `completions/openpapir.fish` | The `fish` completion script. |
| `completions/_openpapir.ps1` | The `powershell` completion script. |
| `completions/openpapir.elv` | The `elvish` completion script. |
| `man/man1/openpapir.1` | The man page for the whole command tree. |

The completion scripts and the man page are not committed anywhere. Each is
written by the built binary of that target, through `openpapir completions
<shell>` and `openpapir manpage`, on the runner that built it and in the step
that stages the archive, so a shipped script cannot describe a command the
archived binary does not have. The file names are the ones each shell looks
for, so installing one is a copy to the shell's own directory, which
[README.md](../README.md) documents alongside the manual route of running the
two commands.

The staging is `scripts/package-release.sh`, which copies the binary and the
four documents, generates the five scripts and the page, and then checks the
staged directory: every file above has to exist, be a regular file rather than
a symbolic link, and hold something, and the man page has to carry a
`.TH openpapir 1` header among its first lines, which is where the roff header
sits behind the two-line quote-escaping preamble the generator emits.

One table inside the script names the shells and their file names. The staging
loop writes one completion script per row, `--describe` names the shells from
the same rows, and `--describe --paths` prints the archive contents as one
relative path per line from them, so adding or removing a shell changes the
staging, the sentence a release prints, and the listing together. The
staged-layout check reads that same listing, so there is nothing to keep in
step by hand.

Before it writes anything, the script refuses a staging path that is a
symbolic link and one that is a directory already holding files, so a staging
run never writes through a link and never mixes into an earlier archive.

The CI test job calls the same script on the release binary it already builds,
on each of Linux, macOS, and Windows, so the layout is exercised on every pull
request without a tag and without an archive. That job then diffs the staged
tree against `--describe --paths`, so a file staged but not described, or
described but not staged, fails the pull request. It stages into `staging/`
under the checkout root, which `.gitignore` lists, so running the same step
locally leaves no untracked tree. The draft release and the dry-run summary
print what `--describe` says, so a release cannot name a layout other than the
one that was staged.

The build job keeps its uploaded artefact for one day, because the same run's
verify and draft release jobs are its only consumers, and the layout a pull
request dry run produces is already evidenced by the CI packaging step and by
`--describe --paths`.

Beside each archive is a `<archive>.sha256` file in the format
`sha256sum --check` reads. The checksum is written and verified on the runner
that built the archive and verified again from the collected artefacts before
a draft release is created.

### The third-party notice

`THIRD-PARTY-NOTICES.md` is the attribution the dependency licences require.
Every licence in the `deny.toml` allow list is permissive, and every one of
them asks for attribution: the file names each dependency a shipped binary
links, with the version resolved from `Cargo.lock` and the licence expression
the crate declares. It is not a claim about openPapir's own terms, which stay
the MIT licence in `LICENSE`.

It is committed at the repository root and copied into an archive like the
other three documents, so nothing is generated at release time. It is written
by `scripts/third-party-notices.py`, which reads `cargo metadata` and needs
cargo and Python 3, both of which the existing checks already require: no
build-time dependency is added to the workspace, and no extra tool has to be
installed. `cargo-about` and similar tools would embed every licence text and
produce a fuller notice; they stay optional, and the committed path works
without them.

```sh
python3 scripts/third-party-notices.py          # rewrite the notice
python3 scripts/third-party-notices.py --check  # fail if it is stale
```

The generator walks the resolved graph from both workspace crates over normal
and build edges only, with every feature on and no platform filter, so one
committed file is a superset of what any single release target links and no
target needs a notice of its own. Development-only dependencies are left out,
because no shipped binary holds them.

`--check` regenerates the notice and compares it with the committed file. It
runs in `./scripts/check.sh` and in the CI lint job, so a dependency added,
removed, or moved to another version fails the pull request that did it until
the notice is regenerated and committed. Because `package-release.sh` stages
the committed file, that check is what keeps a release archive's attribution
current.

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
`.github/workflows/ci.yml` does, and keeps `contents: read` at the top level.
Building the artefacts, verifying the collected checksums, and summarising a
dry run all happen under that read-only scope. `contents: write`,
`id-token: write`, and `attestations: write` are granted to the one job that
creates the release, and that job is skipped on every dry run, so no run that
creates nothing ever holds a write scope. Every build runs
`cargo build --release --locked`.

It has three triggers.

- A push of a `v*` tag. The workflow refuses a tag whose commit is not on
  `master`, takes the version from the tag, and creates a **draft** GitHub
  release whose notes are the [CHANGELOG.md](../CHANGELOG.md) section for that
  version and the artefact listing below it, with every archive and checksum
  file attached. A version with no
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
  pull requests that change the pipeline and nothing at all on the rest. The
  one part of the pipeline every pull request does exercise is the archive
  staging, because CI runs `scripts/package-release.sh` in its test job on all
  three platforms; a change to the archive layout is therefore caught without
  this trigger firing.

The workflow never creates a tag, never pushes, and never publishes a draft.

### The release notes

The notes of a draft release are one [CHANGELOG.md](../CHANGELOG.md) section,
copied verbatim, followed by one `The artefacts` section that names what an
archive holds. `scripts/release-notes.py` is the only thing that reads a
changelog section, and both the job that creates the release and the dry run
call it. The trailing section is not read from anywhere: it is printed by
`scripts/package-release.sh --describe`, the script that staged the archives,
so the notes describe the layout that was actually staged and cannot be
edited out of step with it. Nothing else is added to the notes.

A section runs from its `##` heading to the next `##` heading **outside a
code fence**. A line that looks like a heading inside a fenced block, opened
by three backticks or three tildes, is content: it neither starts nor ends a
section. Fenced headings are
therefore allowed in the changelog rather than forbidden by a gate, because
the changelog holds captured output and a captured Markdown example may
legitimately contain one. The fence rules are the ones
`scripts/check-prose.py` applies, so both checks read the file the same way.
A heading matches a version when it is the version, optionally in brackets,
optionally followed by a date or other trailing text, so `## 1.2.0`,
`## [1.2.0]`, and `## [1.2.0] - 2026-01-01` all name version `1.2.0`, while
`## 1.2.0-rc.1` does not.

A tagged run fails when the version has no section or the section is empty: a
release nobody described is not published with an empty note.

A dry run has no released version to look for, so it reads the `Unreleased`
section, which is what a release is cut from, and prints the result into the
run summary. The extraction runs in the verify job, under `contents: read`,
alongside the extraction's own cases (`--self-test`). A changelog the
extraction cannot read therefore fails a dry run rather than surfacing on the
one run that matters.

The dry run passes `--allow-empty`, and only for `Unreleased`. Between a
changelog cut and the next entry that section legitimately holds nothing, and
an empty `Unreleased` is not a broken changelog. The flag never reaches a
tagged run, so a version section that is missing or empty still fails.

`./scripts/check.sh` runs the same two checks locally:

```sh
python3 scripts/release-notes.py --self-test
python3 scripts/release-notes.py --allow-empty Unreleased
```

## What this document is not

It does not authorise a release. A pipeline that can build artefacts is not
permission to cut a release, and a green dry run is not a release. Any change
to the release process is specified here in the same pull request that makes
it, and the "Current position" table above is corrected at the same time.
Until a tag exists on `master` and a draft is published, any statement that
openPapir has a release is wrong.
