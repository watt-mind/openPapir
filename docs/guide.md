# End-to-end guide

This guide walks one person through one matter from beginning to end, using
openPapir to keep the local record of it. Every command below was run against
a real archive and every block of output was copied from that run, so what you
read here is what the binary printed.

Maintainer note: the blocks below come from one walk of this page, in the
order it presents them, against the release binary built from the `develop`
commit `2adfb14` on 2026-09-10, using the synthetic files step 2 creates.
When the output of a command changes, walk the page again and replace every
block from that one run, so the identifiers, digests, counts, and byte totals
stay consistent with each other.

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
Archive identifier: 6d37fd7b465baae1e9ff7e2b1422b8c1. Schema version: 1.
```

## 2. Import the files before you send them

The files in this walk are stand-ins, so make them first. They are the four
files the rest of the page uses: the two Dorottya sends here, and the two she
finds in the storage later. Use your own files instead and every digest below
will differ from the ones printed here.

```sh
mkdir -p ./inbox
printf '%%PDF-1.7\nSynthetic stand-in for the request form.\n' > ./inbox/request-form.pdf
printf '%%PDF-1.7\nSynthetic stand-in for the site plan.\n' > ./inbox/site-plan.pdf
printf '%%PDF-1.7\nSynthetic stand-in for a submission receipt.\n' > ./inbox/feladasi-igazolas.pdf
printf '%%PDF-1.7\nSynthetic stand-in for an unrelated notice.\n' > ./inbox/unknown-notice.pdf
```

Import stores each file's bytes unchanged and records one import event per
input. Do this before you submit, so the archive holds exactly the bytes you
sent rather than a later copy of them.

```console
$ openpapir import --archive ./archive ./inbox/request-form.pdf ./inbox/site-plan.pdf
Stored 2 artefact(s); 0 already present.
sha256:657902d781ed6fa9e6fa38426de2c8ac8e851895e92d514c4fcf6504d9d10046 (50 bytes), import event b74c2f6a058633cb3a299c3a62e4136d, stored.
sha256:4d4e682f06fc1b43ad7bc060b54488d8ce12a60afbfd2243a7d9a4c1ef034754 (47 bytes), import event d2fa1121000ecba6cfb5a4930adab5ca, stored.
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
Case 7c9a1ecc35a1eab7c576193be0678e0a, recorded 2026-09-10T13:27:46Z.
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
    --case 7c9a1ecc35a1eab7c576193be0678e0a \
    --description "Sent the roof permit request through the service." \
    --date 2026-09-01 \
    --artefact sha256:657902d781ed6fa9e6fa38426de2c8ac8e851895e92d514c4fcf6504d9d10046:primary \
    --artefact sha256:4d4e682f06fc1b43ad7bc060b54488d8ce12a60afbfd2243a7d9a4c1ef034754:attachment
Case 7c9a1ecc35a1eab7c576193be0678e0a.
Submission 7ce8adfd25639d96a550f5101793707c, recorded 2026-09-10T13:27:46Z.
Description: Sent the roof permit request through the service.
Date stated by the user: 2026-09-01. openPapir does not interpret it.
Artefacts referenced: 2.
sha256:657902d781ed6fa9e6fa38426de2c8ac8e851895e92d514c4fcf6504d9d10046 as primary
sha256:4d4e682f06fc1b43ad7bc060b54488d8ce12a60afbfd2243a7d9a4c1ef034754 as attachment
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

Or in one step: `--file <path>` or `--file <path>:<role>` imports the file and
references it in the same command, so a submission whose files are not in the
archive yet needs no separate `import` first. It takes the same caps and
records the same import event that `import` would, and the report gains a line
counting the files it imported and the ones it found already stored. The two
flags may be mixed, so an artefact already in the archive stays a `--artefact`.

```sh
openpapir submission add --archive ./archive \
    --case 7c9a1ecc35a1eab7c576193be0678e0a \
    --description "Sent the roof permit request through the service." \
    --date 2026-09-01 \
    --file ./inbox/request-form.pdf:primary \
    --file ./inbox/site-plan.pdf:attachment
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
case 7c9a1ecc35a1eab7c576193be0678e0a, submission 7ce8adfd25639d96a550f5101793707c, stated 2026-09-01, look by 2026-10-01, 21 day(s) left.
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
sha256:a9774311b28300fae8f780e42c37c8694a4d8f9b044cbf70b841735767d34941 (54 bytes), import event aceac5ae6e46718f38593dcc1f528490, stored.
A digest identifies bytes only. Nothing here is verified, matched, or delivered.
```

```console
$ openpapir receipt add --archive ./archive \
    --artefact sha256:a9774311b28300fae8f780e42c37c8694a4d8f9b044cbf70b841735767d34941 \
    --label "Feladasi igazolas taken from the storage"
Receipt dd2643aa9706f70e4a5f505cd713a42f, recorded 2026-09-10T13:27:46Z.
Artefact: sha256:a9774311b28300fae8f780e42c37c8694a4d8f9b044cbf70b841735767d34941
Import event: aceac5ae6e46718f38593dcc1f528490
Label: Feladasi igazolas taken from the storage
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The label is yours. Nothing in the file was read, and no signature on it was
checked.

The record names one import event for the bytes, and `--import-event` picks
which one when the same bytes were imported more than once, for example once
from the storage and once from a copy someone sent you. Without the flag the
earliest import event of those bytes is recorded.

## 7. Say what you believe about the receipt

An association records what you assert about one receipt, with one of the
outcomes `unassociated`, `candidate`, `associated`, and `contradictory`. Start
where the evidence actually is. On the day it arrives you may only have a date
that fits, which is a candidate.

```console
$ openpapir association create --archive ./archive \
    --receipt dd2643aa9706f70e4a5f505cd713a42f --outcome candidate \
    --candidate "7ce8adfd25639d96a550f5101793707c:moderate:The date on the file matches the day the request was sent."
Association fd348d516fa11332924d566bc2808a3f, recorded 2026-09-10T13:27:46Z by user.
Receipt: dd2643aa9706f70e4a5f505cd713a42f
Outcome: candidate
Confirmed submission: none
Supersedes: nothing
Candidates: 1.
7ce8adfd25639d96a550f5101793707c confidence moderate
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
    --receipt dd2643aa9706f70e4a5f505cd713a42f --outcome associated \
    --candidate "7ce8adfd25639d96a550f5101793707c:strong:The reference number on the file is the one the request carries." \
    --supersedes fd348d516fa11332924d566bc2808a3f
Association e3a1ebcdea1fb655a99c5701a8d56f00, recorded 2026-09-10T13:27:46Z by user.
Receipt: dd2643aa9706f70e4a5f505cd713a42f
Outcome: associated
Confirmed submission: 7ce8adfd25639d96a550f5101793707c
Supersedes: fd348d516fa11332924d566bc2808a3f
Candidates: 1.
7ce8adfd25639d96a550f5101793707c confidence strong
user_assertion from user: The reference number on the file is the one the request carries.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The statement stored above is hers, in her words, and openPapir repeats it back
without having read the file. Both records stay. `association list` shows the
whole history, newest first, superseded records included, so the reason she
changed her mind stays legible.

```console
$ openpapir association list --archive ./archive --receipt dd2643aa9706f70e4a5f505cd713a42f
2 association(s) for receipt dd2643aa9706f70e4a5f505cd713a42f, newest first.
Association e3a1ebcdea1fb655a99c5701a8d56f00, recorded 2026-09-10T13:27:46Z by user.
Receipt: dd2643aa9706f70e4a5f505cd713a42f
Outcome: associated
Confirmed submission: 7ce8adfd25639d96a550f5101793707c
Supersedes: fd348d516fa11332924d566bc2808a3f
Candidates: 1.
7ce8adfd25639d96a550f5101793707c confidence strong
user_assertion from user: The reference number on the file is the one the request carries.
Association fd348d516fa11332924d566bc2808a3f, recorded 2026-09-10T13:27:46Z by user.
Receipt: dd2643aa9706f70e4a5f505cd713a42f
Outcome: candidate
Confirmed submission: none
Supersedes: nothing
Candidates: 1.
7ce8adfd25639d96a550f5101793707c confidence moderate
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
sha256:4a23be0e674c10dd761d5c2124a78cf4f09697fc1ae6911be3e0a98c91fe6412 (53 bytes), import event 15ed428bfe65d3fd60cef8ac6f69f5eb, stored.
A digest identifies bytes only. Nothing here is verified, matched, or delivered.
```

```console
$ openpapir receipt add --archive ./archive \
    --artefact sha256:4a23be0e674c10dd761d5c2124a78cf4f09697fc1ae6911be3e0a98c91fe6412 \
    --label "Notice found in the storage the same week"
Receipt 08077dad82be6725dfa2f3463659a6ae, recorded 2026-09-10T13:27:47Z.
Artefact: sha256:4a23be0e674c10dd761d5c2124a78cf4f09697fc1ae6911be3e0a98c91fe6412
Import event: 15ed428bfe65d3fd60cef8ac6f69f5eb
Label: Notice found in the storage the same week
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

She guessed weakly that it belonged to this matter.

```console
$ openpapir association create --archive ./archive \
    --receipt 08077dad82be6725dfa2f3463659a6ae --outcome candidate \
    --candidate "7ce8adfd25639d96a550f5101793707c:weak:It arrived in the same week as the request."
Association d910074c7e48a3ebe06bca9f585cc686, recorded 2026-09-10T13:27:47Z by user.
Receipt: 08077dad82be6725dfa2f3463659a6ae
Outcome: candidate
Confirmed submission: none
Supersedes: nothing
Candidates: 1.
7ce8adfd25639d96a550f5101793707c confidence weak
user_assertion from user: It arrived in the same week as the request.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

It belongs to a different matter. Retiring the assertion writes a record that
supersedes it and claims nothing. It does not delete the guess and does not
assert the opposite of it.

```console
$ openpapir association retire --archive ./archive d910074c7e48a3ebe06bca9f585cc686 \
    --reason "It belongs to a different matter."
The assertion is withdrawn. Both records stay, and nothing was edited or removed.
Association 0f3a6a9ed408b7d79ebe8985faf79f92, recorded 2026-09-10T13:27:47Z by user.
Receipt: 08077dad82be6725dfa2f3463659a6ae
Outcome: unassociated
Confirmed submission: none
Supersedes: d910074c7e48a3ebe06bca9f585cc686
Candidates: 0.
These are the user's own assertions. openPapir checked nothing about the file and reports no delivery, authenticity, or legal effect.
```

The reason is stored and is never repeated in a message.

## 9. Keep the filing current

`case update` is the one command that rewrites a stored record. It keeps the
case's identifier and creation time and reports which fields it changed.

```console
$ openpapir case update --archive ./archive 7c9a1ecc35a1eab7c576193be0678e0a \
    --status closed --tag granted
Case 7c9a1ecc35a1eab7c576193be0678e0a, recorded 2026-09-10T13:27:46Z.
Title: Workshop roof permit, Kovacs Dorottya
Notes: Municipal permit request for the workshop roof.
Status: closed
Tags: granted, municipal, permit
Updated: 2026-09-10T13:27:47Z
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
7c9a1ecc35a1eab7c576193be0678e0a 2026-09-10T13:27:46Z closed Workshop roof permit, Kovacs Dorottya
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
case 7c9a1ecc35a1eab7c576193be0678e0a notes
case 7c9a1ecc35a1eab7c576193be0678e0a title
submission 7ce8adfd25639d96a550f5101793707c description (case 7c9a1ecc35a1eab7c576193be0678e0a)
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
receipt 08077dad82be6725dfa2f3463659a6ae label
receipt dd2643aa9706f70e4a5f505cd713a42f label
2 hit(s) in the record kind(s) read: receipt.
Search reads the user's own record text and the identifiers openPapir minted, never a stored object, an original filename, or anything derived.
```

The reason you gave when you withdrew an assertion is your own text too:

```console
$ openpapir search --archive ./archive different
association 0f3a6a9ed408b7d79ebe8985faf79f92 statement
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
Checked 4 object(s) and 12 record(s); 204 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0 incoming, 0 in record directories.
Derived-metadata record(s): 0, of which 0 name(s) an object the store no longer holds. A missing one is not a problem, and neither is one of those: the next derivation discards it.
Cache file(s): 1. A cache is rebuildable and never a problem.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

A passing check means the bytes on disk are the bytes the records name. It is
storage integrity, never authenticity.

## 12. Export one case

An export is a plain copy outward: the original bytes named by their digest,
readable JSON records, and a manifest. The archive is not changed, and every
copy is re-digested as it is written.

```console
$ openpapir case export --archive ./archive --case 7c9a1ecc35a1eab7c576193be0678e0a --to ./export
Exported case 7c9a1ecc35a1eab7c576193be0678e0a to ./export.
Copied 4 object(s), 204 byte(s), and wrote 11 record(s).
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
Archive identifier: 64dd797fdec3298031928b5766e1369d. Schema version: 1.
```

```console
$ openpapir case import --archive ./second-archive --from ./export
Imported case 7c9a1ecc35a1eab7c576193be0678e0a from ./export.
Stored 4 object(s), 204 byte(s); 0 already present.
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
Checked 4 object(s) and 15 record(s); 204 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0 incoming, 0 in record directories.
Derived-metadata record(s): 0, of which 0 name(s) an object the store no longer holds. A missing one is not a problem, and neither is one of those: the next derivation discards it.
Cache file(s): 0. A cache is rebuildable and never a problem.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

## 14. Copy the whole archive out and back, then repair permissions

No encrypted backup exists in this build. What does exist is openPapir's own
copy-out and restore for the whole archive: `archive export` writes every
record and every stored object into a directory, and `archive import` reads
such a directory back into an archive. Step 12 did that for one case; these
two do it for all of it.

```console
$ openpapir archive export --archive ./archive --to ./archive-copy
Exported 1 case(s) to ./archive-copy.
Copied 4 object(s), 204 byte(s), and wrote 12 record(s).
case 1
submission 1
receipt 2
association 4
import_event 4
The archive marker travelled with the copy, so the schema version is in the export.
The archive was not changed. Every copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

The copy holds what a case export holds and the archive marker as well, so the
schema version it was taken under is readable without running openPapir.

```console
$ ls ./archive-copy
manifest.json
objects
papir-archive.json
records
```

Reading it back is the other direction. The destination is an archive like any
other, so create it first.

```console
$ mkdir -p ./restored
$ openpapir archive init ./restored
Archive created at the supplied root.
Archive identifier: 0e7b5c7b57f4a1a661574f8f9078127e. Schema version: 1.
```

```console
$ openpapir archive import --archive ./restored --from ./archive-copy
Imported 1 case(s) from ./archive-copy.
Stored 4 object(s), 204 byte(s); 0 already present.
Wrote 12 record(s); 0 already present.
case 1
submission 1
receipt 2
association 4
import_event 4
Recorded 4 import event(s) with source export.
Every restored copy was re-digested: a digest identifies bytes only, never authenticity, delivery, or legal effect.
```

```console
$ openpapir archive check --archive ./restored
Checked 4 object(s) and 16 record(s); 204 byte(s) digested.
No problem found.
Orphan object(s): 0. Object(s) not digested: 0. Record directory(ies) not read: 0. Leftover staging file(s): 0 incoming, 0 in record directories.
Derived-metadata record(s): 0, of which 0 name(s) an object the store no longer holds. A missing one is not a problem, and neither is one of those: the next derivation discards it.
Cache file(s): 0. A cache is rebuildable and never a problem.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

The records keep the identifiers they had, and the archive keeps the
identifier `archive init` gave it, so the restored archive holds the same
records in a new archive rather than a second set of them.

This pair beats a plain copy for the same two reasons the case export and
import do. Every object is re-digested on the way out, so a copy that cannot
be read as the records name it is a refusal rather than a directory that looks
finished; and on the way back nothing is written until the manifest is read,
every record it names parses, and every object it names re-digests, so the
restore lands whole or not at all. A plain copy reproduces whatever is on disk,
damage included, and an interrupted one leaves a half-archive that says so
nowhere.

A plain copy is still the answer above the restore ceiling. A restore is
bounded by the sum of the object bytes the manifest names, 16 GiB by default,
and refuses above it before it opens a single copy, so an archive larger than
that is copied with whatever copy tool you already trust, taken while no
openPapir process holds the lock. Nothing in the copy is encrypted, so treat it
as plaintext and put it somewhere you would be willing to put the originals.

```sh
cp -a ./archive ./archive-backup
```

Ordinary copy and restore tooling widens permissions. After restoring a copy,
narrow it back to owner-only. The command only ever narrows; it widens nothing
and reads no content.

```console
$ openpapir archive repair-permissions --archive ./second-archive
Narrowed 0 of 40 archive path(s) to owner-only.
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
$ openpapir case delete --archive ./second-archive --case 7c9a1ecc35a1eab7c576193be0678e0a
Removed 7 record(s): association 3, case 1, derived_metadata 0, import_event 0, receipt 2, submission 1.
Removed 0 object(s); 4 retained: purge_not_requested 4, records_retained 0, referenced_elsewhere 0, unremovable 0.
No purge was requested, so no object was removed.
Deletion unlinked files in this archive. It does not erase data from the storage medium, and any backup already taken is outside openPapir's reach.
```

The case is gone from the second archive now, so bring it back from the export
before deleting it again, this time with the bytes.

```console
$ openpapir case import --archive ./second-archive --from ./export
Imported case 7c9a1ecc35a1eab7c576193be0678e0a from ./export.
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
$ openpapir case delete --archive ./second-archive --case 7c9a1ecc35a1eab7c576193be0678e0a --purge
Removed 15 record(s): association 3, case 1, derived_metadata 0, import_event 8, receipt 2, submission 1.
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
Derived-metadata record(s): 0, of which 0 name(s) an object the store no longer holds. A missing one is not a problem, and neither is one of those: the next derivation discards it.
Cache file(s): 0. A cache is rebuildable and never a problem.
The check read the archive and changed nothing. A digest identifies bytes only: a passing check is storage integrity, never authenticity, delivery, or legal effect.
```

## Other commands

These commands sit outside the walk above.

`archive derive` computes the media type and the byte length of every stored
object and writes one derived record per object. Nothing else computes them,
so until it is run, `case show`, `receipt list`, and `receipt show` report no
derived metadata at all. It reads the first bytes of an object to name a type
and forms no opinion beyond that.

```console
$ openpapir archive derive --archive ./archive
Examined 4 stored object(s); 0 not read; 204 byte(s) read to name a type.
Derived record(s) written: 4. Stale record(s) removed: 0.
By media type: jpeg 0, pdf 4, png 0, text 0, unknown 0, xml 0, zip 0.
A media type names what the first bytes look like. It is openPapir's own disposable computation, and it reports no authenticity, no delivery, and no legal effect.
```

Having run it, `case show` reports the derived lines as well, at the end of
the case it already showed.

```console
$ openpapir case show --archive ./archive 7c9a1ecc35a1eab7c576193be0678e0a
Case 7c9a1ecc35a1eab7c576193be0678e0a, recorded 2026-09-10T13:27:46Z.
Title: Workshop roof permit, Kovacs Dorottya
Notes: Municipal permit request for the workshop roof.
Status: closed
Tags: granted, municipal, permit
Updated: 2026-09-10T13:27:47Z
Submissions recorded: 1.
Submission 7ce8adfd25639d96a550f5101793707c, recorded 2026-09-10T13:27:46Z.
Description: Sent the roof permit request through the service.
Date stated by the user: 2026-09-01. openPapir does not interpret it.
Artefacts referenced: 2.
sha256:657902d781ed6fa9e6fa38426de2c8ac8e851895e92d514c4fcf6504d9d10046 as primary
sha256:4d4e682f06fc1b43ad7bc060b54488d8ce12a60afbfd2243a7d9a4c1ef034754 as attachment
Receipts a live association names: 1.
Receipt dd2643aa9706f70e4a5f505cd713a42f outcome associated by association e3a1ebcdea1fb655a99c5701a8d56f00
names submission 7ce8adfd25639d96a550f5101793707c
Derived metadata for 2 artefact(s), computed by openPapir and asserting nothing about any file:
sha256:4d4e682f06fc1b43ad7bc060b54488d8ce12a60afbfd2243a7d9a4c1ef034754 pdf 47 byte(s)
sha256:657902d781ed6fa9e6fa38426de2c8ac8e851895e92d514c4fcf6504d9d10046 pdf 50 byte(s)
Cases and submissions are the user's own local records. Nothing here is verified, matched, or delivered.
```

A derived record is openPapir's own disposable computation. Deleting it costs
nothing, and the next run of `archive derive` writes it again and discards the
ones naming objects the store no longer holds.

`capabilities` reports the development status and the operations this build
implements, and it needs no archive.

```console
$ openpapir capabilities
openPapir: alpha
Implemented operations: archive.init, import, case.create, case.list, case.show, submission.add, receipt.add, receipt.list, association.create, association.list, association.retire, archive.check, archive.status, case.export, case.import, archive.repair_permissions, case.delete, skill, case.update, submission.show, receipt.show, association.show, completions, manpage, archive.export, archive.import, archive.derive, search. Nothing is verified.
```

`completions <shell>` writes one shell's completion script to stdout, and
`manpage` writes the man page for the whole command tree. Both write the bytes
and nothing else, so redirect them where your shell and your manual reader
look.

```sh
openpapir completions bash > ~/.local/share/bash-completion/completions/openpapir
openpapir manpage > ~/.local/share/man/man1/openpapir.1
```

## Reading the JSON envelope

Add `--json` to any command that reports a result and exactly one object
reaches stdout, with nothing on stderr. The shape is one envelope for every
such command: `schema_version`, `ok`, `command`, `data`, `verified`, and an
`error` object when `ok` is `false`.

`skill`, `manpage`, and `completions` are the exception. They write the bytes
of a document, not an envelope, and they take no `--json`: passing it is a
usage refusal that exits with `2`. Redirect their stdout to a file, as the
recipes above and in the next section do.

```console
$ openpapir archive status --archive ./archive --as-of 2026-09-10 --json
{"schema_version":1,"ok":true,"command":"archive.status","data":{"as_of":"2026-09-10","associations":0,"cases":1,"cases_by_status":[{"count":1,"status":"open"},{"count":0,"status":"closed"}],"receipts":0,"receipts_to_retrieve":[{"case_id":"7c9a1ecc35a1eab7c576193be0678e0a","days_left":21,"retrieve_by":"2026-10-01","submission_date":"2026-09-01","submission_id":"7ce8adfd25639d96a550f5101793707c"}],"retention_window_days":30,"stored_objects":2,"submissions":1,"undated_submissions":0},"verified":false}
```

That one was taken at the point step 5 reached, which is why its reminder list
is not empty. The next was taken at the end of step 9.

```console
$ openpapir case list --archive ./archive --status closed --json
{"schema_version":1,"ok":true,"command":"case.list","data":{"cases":[{"archive_schema_version":1,"created_at":"2026-09-10T13:27:46Z","id":"7c9a1ecc35a1eab7c576193be0678e0a","notes":"Municipal permit request for the workshop roof.","record_kind":"case","status":"closed","tags":["granted","municipal","permit"],"title":"Workshop roof permit, Kovacs Dorottya","updated_at":"2026-09-10T13:27:47Z"}],"count":1},"verified":false}
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
command that reports a result with `--json` and read the envelope described
above rather than the human lines, whose wording is not a contract. `skill`
itself is one of the three that take no `--json`, which is why the recipe
above redirects its bytes instead.

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
