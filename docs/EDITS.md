# Retained edits of existing records

The native `wedl-rust/v1` edit workflow retains an unfinished buffer, reviews one
existing canonical target, and publishes an explicitly approved change together
with its private receipt. Source remains `wedl/v1`. The executable
[walkthrough](../examples/edits/README.md) starts with a fresh disposable world.

## Private document and source pins

Each edit is a closed UTF-8 JSON document at `author/edits/<id>.json`. Its required
fields are:

| Field | Contract |
| --- | --- |
| `id` | `edit_` plus 26 uppercase Crockford characters; immutable. |
| `title` | Nonblank private title, at most 1,024 UTF-8 bytes. |
| `worldId`, `targetId` | Exact canonical world and existing target IDs; immutable. |
| `baseFingerprint` | Complete canonical source fingerprint retained for this edit. |
| `baseSourceSha256`, `baseSourcePath` | Exact target bytes and safe repository-relative `story/` path ending in `.md` at that base; the path is at most 4,096 bytes. |
| `patch` | Opaque YAML text, at most 131,072 UTF-8 bytes; empty or unfinished text is saveable. |
| `bodyMode` | Exactly `preserve` or `replace`. |
| `bodyMarkdown` | Exact retained prose, at most 131,072 UTF-8 bytes; empty is allowed. |
| `phase` | `draft`, `cancelled` or `applied`. |

`appliedReceipt` exists exactly in applied history. It is omitted otherwise;
explicit `null` is invalid. Duplicate and unknown JSON keys are rejected at every
typed level. Saves produce compact JSON plus one LF while preserving string
content, including CRLF, Unicode and trailing spaces. The complete private file
must fit 262,144 bytes. Private files remain outside canonical fingerprints,
record counts, search corpora and character context.

Three independent pins serve different purposes:

| Pin | Meaning |
| --- | --- |
| Draft base triple | The retained whole-source fingerprint, target SHA-256 and target path against which the author was editing. |
| Save `expectedFingerprint` | The current complete canonical snapshot admitting this private save. |
| Save `expectedSha256` | The exact prior private-file bytes, including formatting and final newline; explicit `null` means create a new private file. |

A save request has exactly `{expectedFingerprint,expectedSha256,draft}`. All
three members are required, including `expectedSha256`. Hashes are lowercase
64-character hexadecimal strings. `edit-drafts preview --file REQUEST` previews
the private write; `edit-drafts save --file REQUEST` performs it. Neither parses
the unfinished patch. An identical nonterminal save performs no write.

A new draft requires the exact current base triple. An existing draft may keep
an older triple while its text changes, provided the save pins match the current
valid worktree and private file. To reconcile, inspect the current target and
explicitly replace the base triple with that same target's current fingerprint,
byte hash and path, then save and review again. Any changed base pin requires the
entire triple to match current source. Reconciliation cannot retarget an edit.

Ordinary saves permit `draft` to remain `draft` or become `cancelled`; a new
cancelled buffer is also allowed. Cancelled and applied edits are terminal,
including for identical ordinary saves. Only approved application can create
applied history or its receipt.

## Candidate review and explicit approval

Review uses exactly `{expectedFingerprint,editId,expectedSha256,maxCharacters}`.
`edit-drafts review --file REQUEST` reads the exact saved nonterminal buffer and
requires its base triple to be current. `maxCharacters` is 256–1,000,000 Unicode
scalar values for the complete compact JSON report; the report also has an
8 MiB byte ceiling. An over-bound report is refused whole.

The patch uses the native [record-update grammar](AUTHORING.md): it requires the
target's matching `kind`, retains omitted fields, and replaces supplied arrays
or maps as whole values. Identity, kind and path cannot change. Unknown fields
and unsupported null values fail review. Raw comments and numeric tokens remain
in the private buffer even though evaluation normalizes the parsed YAML.

Prose has one explicit authority. `preserve` keeps the current target body and
retains `bodyMarkdown` only as private work. `replace` publishes that exact
string, including an empty string. A top-level `body` patch member fails review
in both modes. Expanded YAML is checked before value allocation: at most
131,072 nodes, depth 128 and 262,144 aggregate scalar bytes. Serialized YAML and
the complete derived single-update consequence plan must fit the native plan
bound of 262,144 bytes. Small ordinary YAML aliases remain supported.

Malformed or unconstructible patches return `canApply:false`, diagnostics and
`consequences:null`; constructible invalid candidates retain their complete
consequence report and validation diagnostics. A valid complete review returns
`canApply:true`, `approvalRequired:true` and `writesPerformed:false`. Its nested
consequence preview remains read-only and has its own `canApply:false`.
Validation retains its native `complete` flag; `valid:true` means the implemented
checks passed.

Inspect the exact before/after source, diff, validation, retained draft and
`privateTransition`. To approve, add exactly `reviewSha256` from this report and
`approved:true` to the four review-request fields, then run
`edit-drafts apply --file REQUEST`. Missing or false approval is refused. The
writer reloads and recomputes the complete review under the shared writer lock;
changed source, private bytes, budget or review binding requires a fresh review.
These hashes bind local content; they do not authenticate a human identity.

## Terminal receipt, retry and recovery

Application publishes the one canonical target and private terminal document
together. The only private content changes are `phase:"applied"` and the added
receipt. The receipt requires `planSha256`, `reviewSha256`, `reportSha256`,
`requestSha256`, `baseFingerprint`, `candidateFingerprint`, `beforeEditSha256`,
`beforeDraftSha256`, `maxCharacters` and `file`. The file member is exactly
`{entityId,sourcePath,beforeSha256,afterSha256}`. Raw prior private bytes and the
normalized retained draft are bound independently.

The result reports the candidate `fingerprint`, terminal `sourceSha256`,
`receipt`, `changedFiles` (0 or 1), `canonicalNoOp`, `alreadyApplied`,
`writesPerformed` and `verificationScope:"exact-source-publication"`. The CLI and
HTTP outer fingerprint describes the loaded input snapshot; use the result's
candidate fingerprint or a fresh read after publication. A canonical no-op
still terminalizes the private edit once. Repeating the **identical approved
request** returns `alreadyApplied:true` and `writesPerformed:false` while exact
terminal bytes and the complete applied source still match. Reformatted
terminal bytes, changed requests or later canonical drift refuse replay.

The shared publisher journals the pair before changing live files. Pending
publication fences current native source and private reads; a generation check
also detects a completed publication during a read. Inspect with
`consequences publication-status`. If `pending:true`, review that report and use
`consequences recover --expected-journal-sha256 HASH` with its exact
`journalSha256`. These commands take the usual `--repo` and run before canonical
source loading. Recovery validates the retained edit pair and reconstructed
worlds, then restores both before images. Unrecognized external edits or damaged
journals refuse recovery; a retained journal requires explicit rollback even
when all after images are present. External editors must coordinate with the
advisory writer lock. See [publication recovery](OUTCOMES.md#interrupted-publication-and-recovery)
for the shared durability limits.

## Discovery, history and transports

`edit-drafts list` returns metadata; `edit-drafts detail ID [--sha256 HASH]`
additionally returns the parsed `draft` and exact raw `source`. Current reads
require a valid, unchanged captured worktree. With `--revision COMMIT`, these
two operations read original private Git blobs in the selected valid canonical
snapshot, returning its full `revision`, original `path`, `blobOid` and
`sourceSha256`. Missing historical documents never fall back to live files.

Historical edits compare their retained base with one separately captured live
worktree. `comparedFingerprint` identifies that comparison. `currentness.state`
is `current`, `stale` or `unavailable`; reasons are `unchanged`,
`repository-changed`, `target-changed`, `canonical-changed` or `unavailable`.
An invalid or unavailable current comparison leaves readable historical bytes
intact. Currentness describes **base freshness**, including for applied history;
it does not replace the application receipt. Historical saves, save previews,
candidate reviews and applications are refused.

The store permits 1,024 documents, 16 MiB aggregate bytes and at most 2,048
directory entries. Bounded regular `.wedl-edit-` temporary files count toward
the byte allowance. Save requests are at most 512 KiB; review/apply requests are
at most 16 KiB. Paths and private parents must be safe regular files/directories.
These checks also run under the writer lock.

The service exposes the same native operations. HTTP offers GET
`/api/edit-drafts/list`, `/detail?id=…&sha256=…`, `/preview?request=…`, and
`/review?expectedFingerprint=…&id=…&sha256=…&maxCharacters=…`. Encode query values;
the optional detail hash selects exact private bytes. Current local-author
sessions also accept POST `/api/edit-drafts/save` and `/api/edit-drafts/apply`
through the same native save and reviewed application contracts. The browser
keeps request downloads and requires separate approval for a reviewed apply.
Read requests exceeding its 16 KiB encoded URL bound use exported files with
the CLI. See [browser publication](BROWSER_PUBLICATION.md) for the write boundary.

Live canonical requests capture current source without restarting the server;
historical sessions remain fixed and GET-only. Unsaved input retains its exact
pins until an explicit capture or reload. These operations create no Git
commits and rebuild no search cache.
