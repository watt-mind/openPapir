# Architecture and CLI contract

## Current implementation

The Rust edition 2024 workspace has an MSRV of 1.88 and two unpublished crates:

| Crate | Current responsibility |
| --- | --- |
| `openpapir-core` | Describe the scaffold's capabilities. |
| `openpapir-cli` | Expose help, version, and capabilities output. |

Only these invocations are supported:

```sh
openpapir --help
openpapir --version
openpapir capabilities
openpapir capabilities --json
```

The JSON capabilities response is one object on stdout:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "capabilities",
  "data": {
    "project": "openPapir",
    "stage": "scaffold",
    "operations": []
  },
  "verified": false
}
```

This envelope describes capabilities only. It is not a promised response schema
for future commands. `operations` is empty because no correspondence operation
is implemented. `verified` is false because no cryptographic check took place.

## Planned ownership

openPapir will own persistent cases, submission relationships, receipt
associations, original-byte preservation, and the user workflow around them.
Storage technology and the case schema remain undecided.

openKRX will own KRX container reading, creation, structural validation, and
safe extraction. openSzigno owns `.es3` dossier operations and their signature
verification. The scaffold has no dependency on either sibling project.
Choose an integration boundary only after their needed contracts are available;
do not duplicate format implementations here.

Receipt states must remain independently expressible:

| State | Meaning |
| --- | --- |
| Imported | Bytes were accepted into the local archive. |
| Matched | Evidence associates the receipt with a submission. |
| Authenticity verified | A specified cryptographic check passed in context. |

These are design requirements, not implemented state transitions. A receipt
may be imported without being matched or verified. Association cannot imply
authenticity, successful delivery, or legal effect. Delegated verification must
identify the attachment or receipt covered, the verifier, and its trust context.

## Integration boundary

No government submission API is assumed. KRX creation alone establishes no
ability to submit a package to e-Papír. Automatic sending, authentication,
background services, and a web interface are outside the initial foundation.
An integration needs separate discovery of authorised access, actual contracts,
and recovery semantics before a scoped implementation proposal.
