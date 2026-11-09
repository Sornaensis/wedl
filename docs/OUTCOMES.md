# Reviewed outcome application

One saved human proposal can now publish its complete explicit candidate and
mark its private action `applied`. This is a separate decision from readiness
or proposal sufficiency. The original action base, complete proposal history,
answers, intention and Markdown remain intact.

Only the current worktree is writable. There is no standalone plan-apply path,
implicit consequence, inferred learning or Git publication.
All contracts remain v1. See [ACTIONS.md](ACTIONS.md) for private reasoning and
[CONSEQUENCES.md](CONSEQUENCES.md) for the existing plan/candidate grammar.

## Review and approval

The review request uses the saved-proposal selectors:

```json
{
  "expectedFingerprint": "<current canonical fingerprint>",
  "actionId": "action_00000000000000000000000001",
  "expectedSha256": "<exact saved private-file SHA-256>",
  "proposalId": "proposal_00000000000000000000000001",
  "maxCharacters": 100000
}
```

`action-drafts review-outcome --file request.json` reloads that exact saved
document. It requires authored and effective phase `proposed`, an active current
human proposal, every required answer, and an exact ordered match between the
proposal's answer snapshot and all current answer-present questions.

The complete report includes the retained draft, verified original evidence,
original/proposal/current source bindings, candidate source, deterministic diffs, validation and a private
transition summary identifying the existing file and resulting `applied` phase.
It supplies a new `reviewSha256`. The saved-preview digest and earlier readiness
digest cannot replace it. The review checks that the eventual complete applied
document fits private document/store quotas, including its receipt.

A constructible invalid candidate has `canApply:false`. A valid complete review
whose required expectations all pass has `canApply:true` and
`approvalRequired:true`; it still performs no writes. Required non-passes keep
`canApply:false`, while advisory results do not block application.
The nested consequence preview retains `canApply:false` under its own read-only
contract. The hashes bind content and selected budgets, not authenticated human
identity or evidence that someone read a report.

An application request contains the five review fields above plus exactly:

```json
{"reviewSha256":"<outcome review digest>","approved":true}
```

These are **two additional fields in the same object**, not a nested review.
`action-drafts apply-outcome --file approved.json` recomputes the complete
review under the shared writer lock. Any changed source, private bytes,
selection, plan, question snapshot, budget or review binding refuses application.
Unavailable or mismatched original evidence also refuses the first application.
An invalid candidate, non-passing required semantic expectation or absent/false
approval also refuses it. Optional analysis is retained in the selected plan,
covered by the complete report digest and recomputed under the same lock.
Advisory checks never supply approval. Every request
decoder rejects missing, duplicate and unknown fields.

## Original evidence

Outcome review resolves the draft's `evidence` followed by the selected proposal's
`evidence`, preserving order and repeated occurrences across those two arrays.
Unselected proposal citations stay inert. Each citation must resolve in its
literal original Git commit to the exact canonical record, description section
or transition member, with the recorded owner path and blob OID. Existing native
member rules also cover declared world sections and lifecycle transitions.

The original commit, trees and regular-file blobs are authenticated by their
object identities. Each captured historical world must pass native validation.
Replacement refs, current files, renamed current targets and live-only members
cannot satisfy an original citation. Missing objects, invalid originals or any
owner/member/path/blob mismatch refuse the complete review. Empty evidence
requires no Git repository.

The report adds ordered `evidence` rows of exactly `{citation,sourceSha256}`.
`sourceSha256` hashes the complete original owner's raw bytes, including comments,
line endings and final-newline state. `evidenceSha256` hashes the complete ordered
JSON array, with sorted object keys, under the `wedl-action-outcome-evidence\0`
domain. It is covered by the report and review bindings and retained in the
applied receipt. The browser checks the ordered citations and digest before
enabling approval, and checks the submitted proof in the write acknowledgement.
These proofs do not copy historical prose into readiness context or grant facts
or character knowledge.

One review supports at most 64 distinct original revisions. Captures share the
existing complete ceilings of 4,096 records, 32 MiB raw source, 32 MiB expanded
YAML strings and 1,000,000 YAML nodes across all selected revisions. Existing
per-file, tree, depth and final-report limits also apply. Exceeding a bound
refuses the whole review without a usable approval.

## Applied state and exact receipts

Application publishes every ordered canonical operation, including unchanged
entries in its receipt, and one replacement private document. Its only private
content changes are `phase:"applied"` and `appliedReceipt`. Ordinary draft save
cannot introduce a receipt or enter `applied`; applied actions are terminal.
Exact source line endings and final-newline state are retained.

The receipt records the chosen proposal, proposal/plan/review/request hashes,
the required original `evidenceSha256`,
the complete report digest, source-before and candidate fingerprints, original private-file and
normalized draft hashes, selected budget, and ordered record IDs, paths and
before/after file hashes. A separate review binding includes the complete report
digest and file manifest; receipt decoding recomputes that binding, so isolated
changes to a before hash or no-op classification are rejected. It remains part
of private author source, outside the
canonical fingerprint, search corpus and character evidence. Direct editing of
local source is outside the authenticated-approval model: these are local
integrity bindings, not signed attestations.

An explicit no-op plan still changes the private action to `applied`, while
retaining the canonical fingerprint and reporting `canonicalNoOp:true`.
Replaying the **identical approved request** succeeds with `alreadyApplied:true`
and `writesPerformed:false` only while the terminal document is exact and the
complete current canonical source still matches the receipt. It does not replay
the operations. Altered requests, reformatted terminal bytes or later canonical
changes are conflicts. Use verification to inspect later drift.
Terminal retries and receipt verification retain the approval-time evidence
digest without reopening original Git objects. They inspect the already applied
publication; later removal of historical objects does not cause another execution
or invalidate an otherwise exact publication receipt.

## Publication verification

`action-drafts verify-outcome --file request.json` accepts exactly
`expectedFingerprint`, `actionId`, `expectedSha256` and `maxCharacters`.
The fingerprint and private hash select the **current** valid worktree and saved
applied document, not the earlier proposal's source. It performs no writes.

| Result | Meaning |
| --- | --- |
| `sourceMatches` | Complete current canonical fingerprint equals the applied candidate. |
| `filesMatch` | Every receipt record still occupies its recorded path with exact after bytes. |
| `verified` | Both checks pass, within the declared exact-source-publication scope. |
| Per-file `status` | `current`, `changed`, `moved` or `missing`. |

An unrelated later source edit can produce `filesMatch:true` with
`sourceMatches:false`. This does not rewrite the receipt or approve another
application. If the retained proposal has analysis, `semanticVerification`
separately reevaluates its explicit expectations on the current source at their
saved horizon. Its results do not replace the exact-source `verified` field or
rewrite the receipt. Unsupported selected checks remain explicit. Schema or
horizon errors that prevent evaluation report the semantic analysis as unavailable.
See [SEMANTIC_CONSEQUENCES.md](SEMANTIC_CONSEQUENCES.md) for current checks and
candidate same-horizon deltas. [Three-way revision review](REVISION_REVIEW.md)
compares original commits separately. [Retcon impact](RETCON_IMPACT.md),
[saved review decisions](RETCON_REVIEWS.md) and [event/grouped reports](EVENT_CONSEQUENCES.md)
are independent informational reads; none supplies this outcome approval.
Applying an outcome may stale a saved retcon review immediately through its
live admitted-input witness, even without a Git commit.

## Shared retained-edit publication

[Retained edits](EDITS.md) reuse this writer lock, journal, generation fence and
rollback path. Their publication contains one exact target record and one
terminal `author/edits/<id>.json` receipt. Recovery reconstructs the complete
before/candidate worlds and verifies the private transition against the exact
reviewed target. It rejects unrelated canonical changes or forged private
history. An approved canonical no-op still records its terminal private receipt.
This adds no second publisher, journal format or Git commit operation.

## Interrupted publication and recovery

Several filesystem renames are not one atomic filesystem operation. The native
publisher instead retains a bounded durable journal before changing live files,
checks every before image, publishes exact after images, then checks the full
candidate and private bytes before clearing the journal. Files are synced, and
Unix builds also sync affected directory entries. The non-Unix directory-sync
helper is currently a no-op. Windows/macOS execution and durability remain
unverified; the validation evidence for this iteration comes from Linux.

Current native source/private reads refuse a pending journal. A generation token
checked around a complete read also detects a publication or rollback that
starts and finishes during that read. Historical Git captures remain independent.
Already captured immutable Worlds can still be inspected as snapshots. External
editors must coordinate with the advisory writer lock; direct filesystem readers
are not covered by the native consistency guarantee.

If application reports interruption, inspect before retrying:

```sh
wedl-rs --repo WORLD consequences publication-status
wedl-rs --repo WORLD consequences recover --expected-journal-sha256 JOURNAL_SHA
```

These commands run before canonical source loading, so they work when a partial
publication cannot load normally. Both reject `--revision`. Status is read-only.
Recovery requires the exact journal hash returned by status and restores the
complete before state, including the proposed private action. It checks every
affected file and reconstructs the complete before/after worlds before its first
write. Current bytes must equal an exact recorded before or after image.
Unrecognized external edits, damaged journals, invalid paths, symlinks, special
files, read-only targets or incompatible surrounding source refuse recovery.

Recovery never overwrites an unrecognized external edit. Failure retains the
journal; partial rollback can be resumed with the same inspected journal if
all remaining checks still pass. A leftover journal always requires explicit
rollback, even if all after images are visible. If publication removed its
journal but reported a final durability error, inspect the saved action and use
the exact retry path. Do not assume the operation failed before publishing.

`.wedl-rust/pending-outcome.json` and `publication-generation` are publication
state. **Do not delete them as cache cleanup.** Only the search index is
disposable. The journal includes private reasoning; preserve its private file
permissions. No cache rebuild or Git commit is part of publication or recovery.

## Limits and interfaces

Requests are limited to 16 KiB. Complete review/apply/verification reports must
fit the requested 256–1,000,000 Unicode scalar values and the existing 8 MiB
report cap. Existing 64-operation, plan, exact-source/diff and private document
limits still apply. Publication admits at most 65 file entries (64 canonical
operations and one private action or edit), 16 MiB of combined before/after source and
a 64 MiB encoded journal. Bounds fail without truncation.

GET `/api/action-drafts/review-outcome` uses the same compact selectors as
`preview-proposal`: `expectedFingerprint`, `id`, `sha256`, `proposalId` and
optional `maxCharacters`. GET `/api/action-drafts/verify-outcome` omits
`proposalId`. Both default to 100,000 characters and reject unknown/duplicate
query fields. Current local-author sessions accept the same bound apply request
at POST `/api/action-drafts/apply-outcome`; recovery remains a CLI/native
operation. See [browser publication](BROWSER_PUBLICATION.md) for the write boundary.

The Actions browser reviews exact saved proposals, requires a separate approval
checkbox, and can publish or download the bound application request. Edits, navigation,
selections and source capture discard approval and pending results. Applied
history is read-only. Live canonical requests capture fresh source after
publication; historical sessions remain fixed and GET-only. Unsaved input
retains its original pins until explicitly recaptured or reloaded. The complete continuation is
in [the outcome walkthrough](../examples/actions/OUTCOMES.md).
