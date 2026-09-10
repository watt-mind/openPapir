# End-to-end guide

This guide walks one person through one matter from beginning to end, using
openPapir to keep the local record of it. Every command below was run against
a real archive and every block of output was copied from that run, so what you
read here is what the binary printed.

Read this alongside [architecture and CLI contract](architecture.md), which is
the authoritative description of each command. This page is the narrative; that
page is the contract.

## What openPapir does and does not do here

openPapir is an independent local tool. It is not the e-Papír service and is
not affiliated with its operators.

- openPapir sends nothing. It opens no socket, reaches no mailbox, and submits
  nothing to any authority. You submit through the service yourself, by
  whatever route the service offers you, and openPapir records what you state
  you did.
- openPapir verifies nothing cryptographically. It checks no signature, no
  seal, and no time stamp. A digest identifies bytes and says nothing about
  authenticity, delivery, or legal effect. Every envelope this build emits
  carries `verified: false`.
- openPapir reads no artefact bytes to form an opinion. It re-digests stored
  objects during the integrity check and never parses a receipt, so a receipt
  is a file you believe to be one and an association is what you assert about
  it.

Every statement about the e-Papír service in this guide is numbered and
attributed to a source in [receipt evidence and local case model
decisions](receipt-discovery.md), labelled descriptive (an operator's own
account of behaviour) or normative (a legal or operator requirement), and given
with the date that source was retrieved. The statements are collected in [What
the sources state](#what-the-sources-state) at the end.

## The example

The names, the files, and the matter are invented. Dorottya Kovács is applying
to a municipal office for a permit to rebuild a workshop roof. She has two
files ready to submit, `request-form.pdf` and `site-plan.pdf`, and she sent
them through the service on 2026-09-01.

The commands are written as `openpapir`. From a checkout, either build the
release binary with `cargo build --release --locked` and run
`./target/release/openpapir`, or put `cargo run --locked -p openpapir-cli --`
in front of every invocation.

## 1. Create the archive

The root must already exist and must be empty. openPapir writes its marker
first and refuses to adopt a directory holding anything else.

```console
$ mkdir -p ./archive
$ openpapir archive init ./archive
Archive created at the supplied root.
Archive identifier: 21b7b5db3b5d642729243845405ae6dd. Schema version: 1.
```

## 2. Import the files before you send them

Import stores each file's bytes unchanged and records one import event per
input. Do this before you submit, so the archive holds exactly the bytes you
sent rather than a later copy of them.

```console
$ openpapir import --archive ./archive ./inbox/request-form.pdf ./inbox/site-plan.pdf
Stored 2 artefact(s); 0 already present.
sha256:1e48579f2023edd5119906d743c45810572950c8e477cea3d6fc1ce62d825529 (53 bytes), import event 9f214f0a5ccaa7e642615dde546aa034, stored.
sha256:833b43e6edae9901243f234f2752467ea291d75f851c207dc694727d85a83925 (29 bytes), import event 67b986d7126c0b48b0688cacf85f0703, stored.
A digest identifies bytes only. Nothing here is verified, matched, or delivered.
```

Importing the same bytes again is a duplicate rather than an error, so re-runs
are safe. Note the digests: every later command refers to a stored file by its
digest and never by its filename.

The service publishes its own limits on what may be attached to a submission.
[S5](#what-the-sources-state) covers where those limits come from. openPapir
imposes none of them and checks none of them.

## 3. Record the case

A case is your own folder of related correspondence. It carries a title, notes,
a status, and tags, all of them yours.

```console
$ openpapir case create --archive ./archive \
    --title "Workshop roof permit, Kovacs Dorottya" \
    --notes "Municipal permit request for the workshop roof." \
    --tag permit --tag municipal
Case fb3a8eeb2968c64b22aab6c4d1223011, recorded 2026-09-10T08:29:41Z.
Title: Workshop roof permit, Kovacs Dorottya
Notes: Municipal permit request for the workshop roof.
Status: open
Tags: municipal, permit
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

## 4. Send it yourself, then record the submission

Submit through the service yourself. openPapir has no part in that step, and
nothing about how you do it reaches the archive. When it is done, record what
you sent, against the case, naming the stored artefacts and their roles and
stating the date in your own words.

```console
$ openpapir submission add --archive ./archive \
    --case fb3a8eeb2968c64b22aab6c4d1223011 \
    --description "Sent the roof permit request through the service." \
    --date 2026-09-01 \
    --artefact sha256:1e48579f2023edd5119906d743c45810572950c8e477cea3d6fc1ce62d825529:primary \
    --artefact sha256:833b43e6edae9901243f234f2752467ea291d75f851c207dc694727d85a83925:attachment
Case fb3a8eeb2968c64b22aab6c4d1223011.
Submission e3f278b2b94d7c36b782a4efb33d283f, recorded 2026-09-10T08:29:41Z.
Description: Sent the roof permit request through the service.
Date stated by the user: 2026-09-01. openPapir does not interpret it.
Artefacts referenced: 2.
sha256:1e48579f2023edd5119906d743c45810572950c8e477cea3d6fc1ce62d825529 as primary
sha256:833b43e6edae9901243f234f2752467ea291d75f851c207dc694727d85a83925 as attachment
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

`--date` is stored verbatim. It is your statement of when you sent the thing,
and openPapir never reads it as a delivery date or a receipt date. It is the
one input the receipt-retrieval reminder in the next step measures from.

## 5. Look for the receipt while the window is open

`archive status` summarises what the archive holds and lists the submissions
whose receipt is still worth looking for in your delivery storage.

```console
$ openpapir archive status --archive ./archive --as-of 2026-09-10
As of 2026-09-10. Case(s): 1. Submission(s): 1. Receipt(s): 0. Association(s): 0. Stored object(s): 2.
Case(s) by status: open 1, closed 0.
Submission(s) with no usable date: 0.
Reminder(s) to look for a submission receipt in the delivery storage while the 30-day window the operator describes is open: 1.
case fb3a8eeb2968c64b22aab6c4d1223011, submission e3f278b2b94d7c36b782a4efb33d283f, stated 2026-09-01, look by 2026-10-01, 21 day(s) left.
The window is the operator's own published description of their storage, not a rule openPapir applies or checks. Nothing was read from any mailbox or service, and nothing here states that a submission was delivered, that a receipt exists, that one was received by an authority, or that any legal effect followed.
The summary read the archive and changed nothing.
```

The reminder counts 30 days from the date you stated, because the operator's
help page describes the delivery storage as retaining incoming documents,
igazolások and nyugták for 30 days, with an option to move an item to
permanent storage ([S2](#what-the-sources-state), E1, descriptive, retrieved
2026-09-09). That is the operator's own account of their storage, not a rule
openPapir applies. openPapir reads no mailbox, so it cannot know whether a
receipt is waiting for you, whether one was ever issued, or whether the
operator's window still says 30 days today. Going and looking is your job, and
the deadline that matters is the operator's, not this reminder's.

Where to look, and for what, is covered by [S1](#what-the-sources-state) and
[S3](#what-the-sources-state). Bear [S6](#what-the-sources-state) in mind: at
least one authority's channel names several distinct notification artefacts
around a single submission rather than one, so what you find may need more than
one receipt record.

A reminder leaves the list once the submission has a receipt associated with
it, and `--as-of` accepts a past or future date so you can ask what the list
looked like, or will look like, on another day.

## 6. Record the receipt

Download whatever the storage holds for you, import the file, and record that
you believe it to be a receipt.

```console
$ openpapir import --archive ./archive ./inbox/feladasi-igazolas.pdf
Stored 1 artefact(s); 0 already present.
sha256:362d93e8f17fc55691799951f2b2da7ec3b7d3acdff1045c539299d3f8529180 (57 bytes), import event a2304c6ac539b1b27efc56c190f950ee, stored.
A digest identifies bytes only. Nothing here is verified, matched, or delivered.
```

```console
$ openpapir receipt add --archive ./archive \
    --artefact sha256:362d93e8f17fc55691799951f2b2da7ec3b7d3acdff1045c539299d3f8529180 \
    --label "Feladasi igazolas taken from the storage"
Receipt a70c8badfbd952836ba6012dcb0feb49, recorded 2026-09-10T08:29:41Z.
Artefact: sha256:362d93e8f17fc55691799951f2b2da7ec3b7d3acdff1045c539299d3f8529180
Import event: a2304c6ac539b1b27efc56c190f950ee
Label: Feladasi igazolas taken from the storage
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The label is yours. Nothing in the file was read, and no signature on it was
checked.

## 7. Say what you believe about the receipt

An association records what you assert about one receipt, with one of the
outcomes `unassociated`, `candidate`, `associated`, and `contradictory`. Start
where the evidence actually is. On the day it arrives you may only have a date
that fits, which is a candidate.

```console
$ openpapir association create --archive ./archive \
    --receipt a70c8badfbd952836ba6012dcb0feb49 --outcome candidate \
    --candidate "e3f278b2b94d7c36b782a4efb33d283f:moderate:The date on the file matches the day the request was sent."
Association 78151bafd0a94250957fcfc5fc9fcc67, recorded 2026-09-10T08:29:41Z by user.
Receipt: a70c8badfbd952836ba6012dcb0feb49
Outcome: candidate
Confirmed submission: none
Supersedes: nothing
Candidates: 1.
e3f278b2b94d7c36b782a4efb33d283f confidence moderate
user_assertion from user: The date on the file matches the day the request was sent.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

Later she opens the file, reads on it what she takes to be the reference number
her request carries, and is willing to say so. That is her reading of the file
and nothing more: no consulted source establishes what such a file contains or
how it is laid out, and openPapir checked none of it. Do not edit the first
record. Write a new one that supersedes it.

```console
$ openpapir association create --archive ./archive \
    --receipt a70c8badfbd952836ba6012dcb0feb49 --outcome associated \
    --candidate "e3f278b2b94d7c36b782a4efb33d283f:strong:The reference number on the file is the one the request carries." \
    --supersedes 78151bafd0a94250957fcfc5fc9fcc67
Association ba3fec13b4149371bbfc0d839703ec12, recorded 2026-09-10T08:29:41Z by user.
Receipt: a70c8badfbd952836ba6012dcb0feb49
Outcome: associated
Confirmed submission: e3f278b2b94d7c36b782a4efb33d283f
Supersedes: 78151bafd0a94250957fcfc5fc9fcc67
Candidates: 1.
e3f278b2b94d7c36b782a4efb33d283f confidence strong
user_assertion from user: The reference number on the file is the one the request carries.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The statement stored above is hers, in her words, and openPapir repeats it back
without having read the file. Both records stay. `association list` shows the
whole history, newest first, superseded records included, so the reason she
changed her mind stays legible.

```console
$ openpapir association list --archive ./archive --receipt a70c8badfbd952836ba6012dcb0feb49
2 association(s) for receipt a70c8badfbd952836ba6012dcb0feb49, newest first.
Association ba3fec13b4149371bbfc0d839703ec12, recorded 2026-09-10T08:29:41Z by user.
Receipt: a70c8badfbd952836ba6012dcb0feb49
Outcome: associated
Confirmed submission: e3f278b2b94d7c36b782a4efb33d283f
Supersedes: 78151bafd0a94250957fcfc5fc9fcc67
Candidates: 1.
e3f278b2b94d7c36b782a4efb33d283f confidence strong
user_assertion from user: The reference number on the file is the one the request carries.
Association 78151bafd0a94250957fcfc5fc9fcc67, recorded 2026-09-10T08:29:41Z by user.
Receipt: a70c8badfbd952836ba6012dcb0feb49
Outcome: candidate
Confirmed submission: none
Supersedes: nothing
Candidates: 1.
e3f278b2b94d7c36b782a4efb33d283f confidence moderate
user_assertion from user: The date on the file matches the day the request was sent.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

`associated` is your own filing decision and nothing more. It does not state
that the submission was delivered, that an authority received it, or that any
legal effect followed.

## 8. Withdraw an assertion you no longer stand behind

A second file turned up in the storage the same week. Import it exactly as
before, then record it as a receipt.

```console
$ openpapir import --archive ./archive ./inbox/unknown-notice.pdf
Stored 1 artefact(s); 0 already present.
sha256:562f95dc47d2a4f9ef789066b68238d00b4c7c051b82155f9b571d1aedc92131 (61 bytes), import event efd7036c0e115a25f30973d49735067a, stored.
A digest identifies bytes only. Nothing here is verified, matched, or delivered.
```

```console
$ openpapir receipt add --archive ./archive \
    --artefact sha256:562f95dc47d2a4f9ef789066b68238d00b4c7c051b82155f9b571d1aedc92131 \
    --label "Notice found in the storage the same week"
Receipt 7099e121dd744f3f2eeb855cc8480725, recorded 2026-09-10T08:29:41Z.
Artefact: sha256:562f95dc47d2a4f9ef789066b68238d00b4c7c051b82155f9b571d1aedc92131
Import event: efd7036c0e115a25f30973d49735067a
Label: Notice found in the storage the same week
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

She guessed weakly that it belonged to this matter.

```console
$ openpapir association create --archive ./archive \
    --receipt 7099e121dd744f3f2eeb855cc8480725 --outcome candidate \
    --candidate "e3f278b2b94d7c36b782a4efb33d283f:weak:It arrived in the same week as the request."
Association ade6c901c30f81c2e94dc917c7e40903, recorded 2026-09-10T08:29:41Z by user.
Receipt: 7099e121dd744f3f2eeb855cc8480725
Outcome: candidate
Confirmed submission: none
Supersedes: nothing
Candidates: 1.
e3f278b2b94d7c36b782a4efb33d283f confidence weak
user_assertion from user: It arrived in the same week as the request.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

It belongs to a different matter. Retiring the assertion writes a record that
supersedes it and claims nothing. It does not delete the guess and does not
assert the opposite of it.

```console
$ openpapir association retire --archive ./archive ade6c901c30f81c2e94dc917c7e40903 \
    --reason "It belongs to a different matter."
The assertion is withdrawn. Both records stay, and nothing was edited or removed.
Association 7e5fdf00ca4b528daaf06ac9be6adbab, recorded 2026-09-10T08:29:41Z by user.
Receipt: 7099e121dd744f3f2eeb855cc8480725
Outcome: unassociated
Confirmed submission: none
Supersedes: ade6c901c30f81c2e94dc917c7e40903
Candidates: 0.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The reason is stored and is never repeated in a message.

## 9. Keep the filing current

`case update` is the one command that rewrites a stored record. It keeps the
case's identifier and creation time and reports which fields it changed.

```console
$ openpapir case update --archive ./archive fb3a8eeb2968c64b22aab6c4d1223011 \
    --status closed --tag granted
Case fb3a8eeb2968c64b22aab6c4d1223011, recorded 2026-09-10T08:29:41Z.
Title: Workshop roof permit, Kovacs Dorottya
Notes: Municipal permit request for the workshop roof.
Status: closed
Tags: granted, municipal, permit
Updated: 2026-09-10T08:29:41Z
Changed: status, tags.
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

`case list` keeps only the cases matching every filter given. `--status` takes
`open` or `closed`, `--tag` may be repeated and all of them must match, and
`--query` matches a case-insensitive substring of the title or the notes. The
query text is never echoed back.

```console
$ openpapir case list --archive ./archive --status closed --tag permit
1 case(s) listed.
fb3a8eeb2968c64b22aab6c4d1223011 2026-09-10T08:29:41Z closed Workshop roof permit, Kovacs Dorottya
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

```console
$ openpapir case list --archive ./archive --status open
0 case(s) listed.
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

## 10. Find a record by a word you wrote

`search` looks for text in your own records: case titles, notes, and tags,
submission descriptions, receipt labels, and the reason on a withdrawn
assertion, together with the identifiers openPapir minted. It reads no stored
file, no imported filename, and nothing openPapir derived, so it finds only
what you typed. It takes no lock and changes nothing.

```console
$ openpapir search --archive ./archive roof
case fb3a8eeb2968c64b22aab6c4d1223011 notes
case fb3a8eeb2968c64b22aab6c4d1223011 title
submission e3f278b2b94d7c36b782a4efb33d283f description (case fb3a8eeb2968c64b22aab6c4d1223011)
3 hit(s) in the record kind(s) read: association, case, receipt, submission.
Search reads the user's own record text and the identifiers openPapir minted, never a stored object, an original filename, or anything derived.
```

A line names the record kind, the record's identifier, the case it belongs to
when it belongs to one, and the field that matched. It never repeats the text
that matched, so open the record itself with `case show`, `submission show`,
`receipt show`, or `association show` to read it. One record that matched in
two fields is two lines, as the case above is.

`--kind` narrows what is read and may be repeated:

```console
$ openpapir search --archive ./archive storage --kind receipt
receipt 7099e121dd744f3f2eeb855cc8480725 label
receipt a70c8badfbd952836ba6012dcb0feb49 label
2 hit(s) in the record kind(s) read: receipt.
Search reads the user's own record text and the identifiers openPapir minted, never a stored object, an original filename, or anything derived.
```

The reason you gave when you withdrew an assertion is your own text too:

```console
$ openpapir search --archive ./archive different
association 7e5fdf00ca4b528daaf06ac9be6adbab statement
1 hit(s) in the record kind(s) read: association, case, receipt, submission.
Search reads the user's own record text and the identifiers openPapir minted, never a stored object, an original filename, or anything derived.
```

The match ignores case on both sides, exactly as `case list --query` does, and
the query is never echoed back. A query that matches nothing is a success with
a count of `0` rather than a refusal. There is no index: the search reads every
record of every kind it was asked for, so its cost grows with what the archive
holds.

## 11. Check the archive

`archive check` re-digests what the store holds and compares it with what the
records claim. It takes no lock and changes nothing.

```console
$ openpapir archive check --archive ./archive
Checked 4 object(s) and 12 record(s); 200 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0 incoming, 0 in record directories.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

A passing check means the bytes on disk are the bytes the records name. It is
storage integrity, never authenticity.

## 12. Export one case

An export is a plain copy outward: the original bytes named by their digest,
readable JSON records, and a manifest. The archive is not changed, and every
copy is re-digested as it is written.

```console
$ openpapir case export --archive ./archive --case fb3a8eeb2968c64b22aab6c4d1223011 --to ./export
Exported case fb3a8eeb2968c64b22aab6c4d1223011 to ./export.
Copied 4 object(s), 200 byte(s), and wrote 11 record(s).
case 1
submission 1
receipt 2
association 3
import_event 4
The archive was not changed. Every copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

```console
$ ls ./export
manifest.json
objects
records
```

An export is readable without openPapir, which is the point of it.

## 13. Import the export into a second archive

`case import` reads such a copy back in. The manifest is authoritative: every
object is re-digested and every record is parsed before the archive is written
to at all, and the whole case then lands at once or not at all.

```console
$ mkdir -p ./second-archive
$ openpapir archive init ./second-archive
Archive created at the supplied root.
Archive identifier: 1a1c1f7017ac4c55a006125bab44eda3. Schema version: 1.
```

```console
$ openpapir case import --archive ./second-archive --from ./export
Imported case fb3a8eeb2968c64b22aab6c4d1223011 from ./export.
Stored 4 object(s), 200 byte(s); 0 already present.
Wrote 11 record(s); 0 already present.
case 1
submission 1
receipt 2
association 3
import_event 4
Recorded 4 import event(s) with source export.
Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

Every record keeps the identifier it had, so the case is the same case in both
archives. A record or an object the archive already holds is not an error and
is not written again, so importing one export twice leaves the same archive.
Check the result before trusting it.

```console
$ openpapir archive check --archive ./second-archive
Checked 4 object(s) and 15 record(s); 200 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0 incoming, 0 in record directories.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

## 14. Back up the archive, then repair permissions

A backup is a plain copy of the archive root, taken with whatever copy tool you
already trust. openPapir has no backup command, and no encrypted backup exists
in this build, so treat the copy as plaintext and put it somewhere you would be
willing to put the originals.

```sh
cp -a ./archive ./archive-backup
```

Ordinary copy and restore tooling widens permissions. After restoring a copy,
narrow it back to owner-only. The command only ever narrows; it widens nothing
and reads no content.

```console
$ openpapir archive repair-permissions --archive ./second-archive
Narrowed 0 of 39 archive path(s) to owner-only.
cache 0
directory 0
marker 0
object 0
record 0
root 0
Permissions are only ever narrowed here; nothing was widened and no content was read or changed.
```

Nothing needed narrowing in this run, which is what a healthy archive looks
like.

## 15. Delete a case when it is finished

Deletion is the one destructive command, and it is explicit twice over: it
names one case, and it removes stored bytes only when `--purge` says so.
Without `--purge` the records go and the objects stay.

```console
$ openpapir case delete --archive ./second-archive --case fb3a8eeb2968c64b22aab6c4d1223011
Removed 7 record(s): association 3, case 1, import_event 0, receipt 2, submission 1.
Removed 0 object(s); 4 retained: purge_not_requested 4, records_retained 0, referenced_elsewhere 0, unremovable 0.
No purge was requested, so no object was removed.
Deletion unlinked files in this archive. It does not erase data from the storage medium, and any backup already taken is outside openPapir's reach.
```

The case is gone from the second archive now, so bring it back from the export
before deleting it again, this time with the bytes.

```console
$ openpapir case import --archive ./second-archive --from ./export
Imported case fb3a8eeb2968c64b22aab6c4d1223011 from ./export.
Stored 0 object(s), 0 byte(s); 4 already present.
Wrote 7 record(s); 4 already present.
case 1
submission 1
receipt 2
association 3
import_event 4
Recorded 0 import event(s) with source export.
Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

With `--purge`, an object is unlinked as well, and then only when no remaining
import event, receipt, or submission references it.

```console
$ openpapir case delete --archive ./second-archive --case fb3a8eeb2968c64b22aab6c4d1223011 --purge
Removed 15 record(s): association 3, case 1, import_event 8, receipt 2, submission 1.
Removed 4 object(s); 0 retained: purge_not_requested 0, records_retained 0, referenced_elsewhere 0, unremovable 0.
A purge was requested: an object is unlinked only when no remaining import event, receipt, or submission references it.
Deletion unlinked files in this archive. It does not erase data from the storage medium, and any backup already taken is outside openPapir's reach.
```

Read the closing line literally: deletion unlinks files, it does not erase data
from the storage medium, and the backup taken in step 14 is entirely outside
openPapir's reach. Export first if you want the case to survive the deletion.

```console
$ openpapir archive check --archive ./second-archive
Checked 0 object(s) and 0 record(s); 0 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0 incoming, 0 in record directories.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

## Reading the JSON envelope

Add `--json` to any command and exactly one object reaches stdout, with nothing
on stderr. The shape is one envelope for every command: `schema_version`, `ok`,
`command`, `data`, `verified`, and an `error` object when `ok` is `false`.

```console
$ openpapir archive status --archive ./archive --as-of 2026-09-10 --json
{"schema_version":1,"ok":true,"command":"archive.status","data":{"as_of":"2026-09-10","associations":0,"cases":1,"cases_by_status":[{"count":1,"status":"open"},{"count":0,"status":"closed"}],"receipts":0,"receipts_to_retrieve":[{"case_id":"fb3a8eeb2968c64b22aab6c4d1223011","days_left":21,"retrieve_by":"2026-10-01","submission_date":"2026-09-01","submission_id":"e3f278b2b94d7c36b782a4efb33d283f"}],"retention_window_days":30,"stored_objects":2,"submissions":1,"undated_submissions":0},"verified":false}
```

That one was taken at the point step 5 reached, which is why its reminder list
is not empty. The next was taken at the end of step 9.

```console
$ openpapir case list --archive ./archive --status closed --json
{"schema_version":1,"ok":true,"command":"case.list","data":{"cases":[{"archive_schema_version":1,"created_at":"2026-09-10T08:29:41Z","id":"fb3a8eeb2968c64b22aab6c4d1223011","notes":"Municipal permit request for the workshop roof.","record_kind":"case","status":"closed","tags":["granted","municipal","permit"],"title":"Workshop roof permit, Kovacs Dorottya","updated_at":"2026-09-10T08:29:41Z"}],"count":1},"verified":false}
```

Branch on the exit code first, which carries the error's bucket and nothing
else, then on `error.code`, which is stable within a `schema_version`. The
`message` wording is not a contract. `verified` is `false` in every envelope
this build emits.

The envelope, the field lists per command, the error codes, and the exit codes
are specified in [the response
envelope](architecture.md#the-response-envelope) and the sections after it, and
the code catalogue is in [import and association error, JSON, and exit-code
contract](error-contract.md). Both modes are pinned byte for byte by the
captured output under [the golden output contract](../tests/golden/README.md).

## For agents

The binary carries an agent skill document describing when to reach for
openPapir, every command's exact invocation, the envelope, the exit codes, the
privacy rule, and the boundary between imported, matched, and
authenticity-verified. `openpapir skill` writes it to stdout byte for byte and
adds nothing, so installing it needs no checkout.

```sh
mkdir -p .claude/skills/openpapir
openpapir skill > .claude/skills/openpapir/SKILL.md
```

Use `.codex/skills/openpapir/` for Codex. The same bytes are committed as [the
agent skill](../crates/openpapir-cli/skills/openpapir/SKILL.md). Drive every
command with `--json` and read the envelope described above rather than the
human lines, whose wording is not a contract.

## What the sources state

Each statement is numbered, attributed to a source in the [evidence
matrix](receipt-discovery.md#evidence-matrix), labelled descriptive or
normative, and given with the date that source was retrieved. Nothing here is
openPapir's own claim about the service, and none of it is checked by any code
in this repository.

- **S1**: a submitter receives a *Feladási igazolás* in their personal
  delivery storage after a successful submission (E1, descriptive, retrieved
  2026-09-09).
- **S2**: the delivery storage retains incoming documents, *igazolások* and
  *nyugták* for 30 days, with an option to move an item to permanent storage
  (E1, descriptive, retrieved 2026-09-09). This is the window the reminder in
  step 5 counts, and it is the operator's own published description, not a rule
  openPapir applies or verifies.
- **S3**: the receipt destination is a single storage mailbox surface serving
  citizens and organisation gateways, not a per-application inbox (E3,
  descriptive, retrieved 2026-09-09).
- **S4**: the service provider must make available to the submitter, or
  deliver by the secure delivery rules, a confirmation evidencing at least the
  lodging of the submission, its point in time, and its content, and the decree
  names the event that confirmation is issued about: the delivery of the
  submission or of the reply to it, or the failure of that delivery (E2
  § 133(4), normative, retrieved in full 2026-09-10). Both halves are
  statements about what the decree requires of the service provider. Neither is
  a claim about any artefact openPapir stores, and the normative text names no
  file format, no field list, and no identifier syntax for such a confirmation.
- **S5**: the accepted attachment formats and the size limit are set by the
  service provider rather than by the decree (E2 § 133(2), normative, retrieved
  in full 2026-09-10); the operator's help page describes attachments up to
  25 MB and an accepted format list including `.pdf` (E1, descriptive,
  retrieved 2026-09-09).
- **S6**: for one authority's own channel, a family of distinct notification
  artefacts is named around a single submission rather than one, including a
  *Feladási igazolás*, a *Letöltési igazolás*, and an *Automatikus válasz*
  (E6, descriptive, for that authority's channel only, retrieved 2026-09-10
  after a first attempt returned HTTP 403).

Nothing above establishes what a receipt file looks like on disk. That question
is open, and the open questions and their bounded follow-ups are recorded in
[receipt evidence and local case model
decisions](receipt-discovery.md). Until they close, openPapir parses no
receipt, and the guide above is a filing workflow rather than a verification
workflow.
