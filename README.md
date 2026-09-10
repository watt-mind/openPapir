# openPapir

A proposed local-first Rust library and CLI for organising Hungarian government
correspondence: cases, submissions, attachments, and receipts.

**Status: alpha**, which is the stage the tool itself reports. The
executable reports its capabilities, creates a local archive, imports files
into a content-addressed store that preserves the original bytes, organises
what it holds into cases and submissions, records receipts together with the
user's own assertions about whether a receipt relates to a submission, checks
a whole archive against what its records claim without changing anything,
copies one case, or a whole archive, out as plain files and reads such a copy
back in, keeps a case record current, narrows a restored archive's permissions back
to owner-only, deletes a case when asked, removing stored bytes only on an
explicit `--purge`, writes the agent skill document it carries, and generates
its own shell completions and man page. Automatic matching, derived metadata,
receipt parsing, KRX and `.es3` handling, editing of a stored record other
than the case record `case update` rewrites, deleting a single submission or
receipt, deleting an archive, signature verification, and government delivery
are not implemented. There is no published release.

openPapir is an independent open-source project. It is not the government's
e-Papír service, is not affiliated with its operators, and does not submit
correspondence to it.

## Try it

Build from this checkout with Rust 1.88 or newer:

```sh
cargo run --locked -p openpapir-cli -- --help
cargo run --locked -p openpapir-cli -- --version
cargo run --locked -p openpapir-cli -- capabilities --json
cargo run --locked -p openpapir-cli -- archive init ./my-archive --json
cargo run --locked -p openpapir-cli -- import --archive ./my-archive ./a-file --json
cargo run --locked -p openpapir-cli -- case create --archive ./my-archive --title "Tax matter" --json
cargo run --locked -p openpapir-cli -- case list --archive ./my-archive --status open --tag tax --json
cargo run --locked -p openpapir-cli -- case update --archive ./my-archive <case-id> --status closed --tag appeal --json
cargo run --locked -p openpapir-cli -- submission add --archive ./my-archive --case <case-id> --description "Posted the form." --json
cargo run --locked -p openpapir-cli -- case show --archive ./my-archive <case-id> --json
cargo run --locked -p openpapir-cli -- receipt add --archive ./my-archive --artefact sha256:<digest> --label "Envelope" --json
cargo run --locked -p openpapir-cli -- receipt list --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- association create --archive ./my-archive --receipt <receipt-id> --outcome candidate --candidate "<submission-id>:moderate:The reference matches." --json
cargo run --locked -p openpapir-cli -- association list --archive ./my-archive --receipt <receipt-id> --json
cargo run --locked -p openpapir-cli -- submission show --archive ./my-archive <submission-id> --json
cargo run --locked -p openpapir-cli -- receipt show --archive ./my-archive <receipt-id> --json
cargo run --locked -p openpapir-cli -- association show --archive ./my-archive <association-id> --json
cargo run --locked -p openpapir-cli -- archive check --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- archive status --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- case export --archive ./my-archive --case <case-id> --to ./my-export --json
cargo run --locked -p openpapir-cli -- case import --archive ./my-archive --from ./my-export --json
cargo run --locked -p openpapir-cli -- archive export --archive ./my-archive --to ./my-archive-export --json
cargo run --locked -p openpapir-cli -- archive import --archive ./my-archive --from ./my-archive-export --json
cargo run --locked -p openpapir-cli -- archive repair-permissions --archive ./my-archive --json
cargo run --locked -p openpapir-cli -- case delete --archive ./my-archive --case <case-id> --purge --json
cargo run --locked -p openpapir-cli -- skill
cargo run --locked -p openpapir-cli -- completions bash
cargo run --locked -p openpapir-cli -- manpage
```

The capabilities command reports the current implementation honestly:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "alpha",
    "operations": [
      "archive.init",
      "import",
      "case.create",
      "case.list",
      "case.show",
      "submission.add",
      "receipt.add",
      "receipt.list",
      "association.create",
      "association.list",
      "association.retire",
      "archive.check",
      "archive.status",
      "case.export",
      "case.import",
      "archive.repair_permissions",
      "case.delete",
      "skill",
      "case.update",
      "submission.show",
      "receipt.show",
      "association.show",
      "completions",
      "manpage"
      "archive.export",
      "archive.import"
    ]
  },
  "verified": false
}
```

The operation list names exactly what is implemented today, one sentence
each. All but the last three can process input:

- `archive init` creates an archive in an existing, empty directory, writing
  the marker first and refusing to adopt anything else.
- `import` stores each file's bytes unchanged, records one import event per
  input, and reports a re-import of the same bytes as a duplicate rather than
  an error.
- `case create` records one case, the user's own folder of related
  correspondence, with the status and the tags the user gave it.
- `case list` lists the cases that match every filter given: `--status`,
  every `--tag`, and a `--query` matched as a case-insensitive substring of
  the title or notes in one linear scan, with no index. The query is never
  echoed back.
- `case show` shows one case, with its status, tags, and update time, the
  submissions recorded against it and their artefact references, and the
  receipts a live association ties to one of those submissions.
- `case update` rewrites one case record in place, keeping its identifier and
  creation time and reporting the names of the fields it changed. It is the
  one invocation that rewrites a stored record.
- `submission add` records one submission the user states they sent, against
  a case.
- `receipt add` records that the user believes one stored artefact to be a
  receipt.
- `receipt list` lists every receipt in the archive.
- `association create` records what the user asserts about one receipt, with
  one of the outcomes `unassociated`, `candidate`, `associated`, and
  `contradictory`.
- `association list` lists one receipt's whole association history, newest
  first, superseded records included.
- `association retire` withdraws one assertion by writing a record that
  supersedes it and claims nothing; both records stay, and nothing is edited
  or removed.
- `submission show` shows one submission as it is stored, with every
  association naming it: the live heads first and the superseded records after
  them.
- `receipt show` shows one receipt as it is stored, with its whole association
  history, newest first and superseded records included.
- `association show` shows one association as it is stored, whether it is the
  live head of its chain, and the chain it belongs to: what it supersedes and
  what supersedes it.
- `archive check` re-digests what the store holds and reports counts only; it
  takes no lock, changes nothing, and a passing check is storage integrity
  rather than authenticity.
- `archive status` summarises what the archive holds and lists the
  submissions whose receipt is still worth looking for in the delivery
  storage, inside the 30-day window the operator's help page describes. It
  takes no lock, changes nothing, and states no delivery, no receipt by an
  authority, and no legal effect.
- `case export` copies one case out as plain files, the original bytes named
  by their digest plus readable JSON records and a manifest, without changing
  the archive, and re-digests every copy.
- `case import` reads such a copy back into an archive, checked against the
  export's manifest before anything is written; a record keeps the identifier
  it had, an object or a record already there is not an error, and importing
  one export twice leaves the same archive.
- `archive export` copies the whole archive out in the same shape, every
  object and every record of every kind, with the archive marker beside the
  manifest so the schema version travels with the copy.
- `archive import` reads such a copy back, restoring every case in it as one
  set: all of it or none of it. Each import reads its own kind of export and
  refuses the other's.
- `archive repair-permissions` narrows a restored archive back to owner-only;
  it never widens anything.
- `case delete` is the one destructive command: it removes a case and its
  submissions, and the receipts and associations tied only to them, but it
  removes no stored bytes unless `--purge` is given, and even then only bytes
  nothing that remains references. It reports counts and record kinds, never a
  digest or a filename, records no deletion anywhere, and does not erase data
  from the storage medium.
- `skill` writes the embedded agent skill document to stdout byte for byte and
  adds nothing. It takes no file and no `--json`, touches no archive, and
  processes no input.
- `completions` writes one shell's completion script to stdout, for `bash`,
  `zsh`, `fish`, `powershell`, or `elvish`. A value outside those five is a
  usage refusal, exit `2`, printed as the argument parser's own usage text.
- `manpage` writes the man page for the whole command tree to stdout as one
  roff stream: the page for `openpapir` first, then one page for each
  subcommand. Both are generated from the same command definition the parser
  uses, take no file and no `--json`, and touch no archive.

Every record is the user's own local record: openPapir sends nothing and reads
no artefact bytes, so a submission is what the user states they sent, a
receipt is an artefact the user believes to be one, an association is what the
user asserts about it, and a date they supply is stored verbatim and never
read as a delivery or receipt date. `verified: false` means no cryptographic
verification was performed: a digest identifies bytes, and says nothing about
authenticity or delivery. The full contract, including the error codes and
exit codes, is in [architecture and CLI contract](docs/architecture.md).

## Intended responsibilities

| Responsibility | State |
| --- | --- |
| Preserve original submission and receipt bytes in a local case archive. | Implemented by `import` and the write-once artefact store. |
| Associate submissions, attachments, and receipts with explicit provenance. | Implemented for the user's own assertions; automatic matching, derived metadata, and receipt parsing are not implemented. |
| Expose case information through a CLI and structured JSON. | Implemented by the list, show, and export commands; search is not implemented. |
| Delegate KRX container processing to [openKRX](https://github.com/watt-mind/openKRX) and `.es3` processing to [openSzigno](https://github.com/watt-mind/openSzigno). | Not implemented. Neither sibling project is a build dependency of this project, and neither parser is copied into it. |
| Delegated authenticity verification, reported with its exact scope and trust context. | Not implemented. No cryptographic check of any kind exists here. |
| Government submission and delivery. | Not implemented and out of scope for now. |

Importing a receipt, matching it to a submission, and verifying its
cryptographic authenticity remain separate records: an association changes
nothing about the artefact and creates no verification result. Matching alone
must never assert legal effect or successful delivery.

## Agents and automation

Every command has two modes, and they are the same contract seen twice. With
`--json` exactly one object reaches stdout and nothing reaches stderr, so a
program reads a result without parsing prose. Branch on the exit code first,
which carries the error's bucket and nothing else, then on `error.code`, which
is stable within a `schema_version`; `message` wording is not. `verified` is
`false` in every envelope this build emits.

```console
$ openpapir receipt add --archive ./archive --artefact sha256:5e0d7d51... \
    --label "Envelope from the post" --json
{"schema_version":1,"ok":true,"command":"receipt.add","data":{"receipt":{"archive_schema_version":1,"artefact_digest":"sha256:5e0d7d511a1df60c20dc2d589f7f1d13b87019603a724a938c71e81a8d7e1dac","created_at":"2026-09-09T16:10:57Z","id":"71df7d0ed619cc29d4478e0db8d014b6","import_event_id":"db11ed295d6b8d7c3edebf4ab251de78","label":"Envelope from the post","record_kind":"receipt"}},"verified":false}
```

The binary carries an agent skill document describing when to reach for
openPapir, every command's exact invocation, the envelope, the exit codes, the
privacy rule, and the boundary between imported, matched, and
authenticity-verified. `openpapir skill` writes it to stdout byte for byte and
nothing else, so installing it needs no checkout:

```sh
mkdir -p .claude/skills/openpapir
openpapir skill > .claude/skills/openpapir/SKILL.md
```

Use `.codex/skills/openpapir/` for Codex, or `~/.claude/skills/openpapir/` to
install it for every project instead of one. The same bytes are committed as
[the agent skill](crates/openpapir-cli/skills/openpapir/SKILL.md).

Both modes are pinned byte for byte by the captured output under
[tests/golden](tests/golden/README.md), so a change to what a caller parses is
a reviewed change rather than an accident.

## Shell completions and the man page

The binary generates both from the same command definition its parser uses, so
neither can fall behind the commands it has. Both write to stdout, so the
destination is your own redirection:

```sh
openpapir completions bash > ~/.local/share/bash-completion/completions/openpapir
openpapir completions zsh > ~/.local/share/zsh/site-functions/_openpapir
openpapir completions fish > ~/.config/fish/completions/openpapir.fish
openpapir manpage > ~/.local/share/man/man1/openpapir.1
```

`powershell` and `elvish` are offered as well; a shell outside the five is a
usage refusal. Start a new shell after installing the script. The man stream
holds the page for `openpapir` and one page for each subcommand, so
`man openpapir` shows the whole tool, and `openpapir manpage | man -l -` reads
it without installing anything. No release archive carries either file today;
see [releasing](docs/releasing.md).

## People

Without `--json` the same command prints lines meant to be read, with the
result on stdout and any warning or error on stderr, so diagnostic text never
shares stdout with a result. The lines carry the user's own titles, labels, and
statements, the identifiers and digests openPapir minted, and no path,
filename, or payload byte.

```console
$ openpapir receipt add --archive ./archive --artefact sha256:5e0d7d51... \
    --label "Envelope from the post"
Receipt 8bbfed851cf9dedefc80442ca4f80a0d, recorded 2026-09-09T16:10:57Z.
Artefact: sha256:5e0d7d511a1df60c20dc2d589f7f1d13b87019603a724a938c71e81a8d7e1dac
Import event: db11ed295d6b8d7c3edebf4ab251de78
Label: Envelope from the post
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The closing line is not decoration. A receipt is a file the user believes to
be one, an association is what the user asserts about it, and neither is a
verification: openPapir reads no artefact bytes and opens no socket.

These lines are English only, because their careful wording and the argument
parser's own text would each need a native reviewer before every release. The
JSON form remains the scripting contract for anyone who reads a result
programmatically.
[Human output language](docs/architecture.md#human-output-language) records
that decision, together with the compiled-in message table a demand for
Hungarian output would go through.

## Documentation

The [end-to-end guide](docs/guide.md) walks one matter from `archive init` to
`case delete` on a synthetic example, with the real output of every command.
The [specification index](docs/specification.md) is the single entry point to
the project's scope, what is implemented today, which designs are decided but
not built, and which contracts are still blocked. The
[documentation index](docs/index.md) gives every document and root policy file
a one-line purpose, including
[the agent skill](crates/openpapir-cli/skills/openpapir/SKILL.md) and the
[golden output contract](tests/golden/README.md).

## Development

The workspace contains `openpapir-core` and `openpapir-cli`. Both crates are
unpublished, use Rust edition 2024, and are licensed under [MIT](LICENSE).

```sh
./scripts/check.sh
```

Pull requests target `develop`; `master` is reserved for stable releases.
See [contributing](CONTRIBUTING.md), whose Documentation section states how
the documents are kept correct, and [security](SECURITY.md). The
[roadmap](docs/roadmap.md) records the bounded discovery work each format
decision waits on, and its fifth milestone, the local organiser, records the
order of the remaining local work: restoring from an export, case lifecycle
and search, the receipt-retrieval reminder, the entangled-deletion remedy,
generative testing, a whole-archive export, a release pipeline, completions
and man pages, a user guide, derived metadata on explicit request, and
encrypted backup at rest.
