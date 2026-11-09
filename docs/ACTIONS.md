# Private action drafts, proposals and literal requirements

Private actions hold authored intention, questions and reasoning. Their source is
separate from canonical `story/` records: saving an action does not create an
event, change state, teach a character, satisfy a story point or apply a
consequence. Literal requirement comparisons are separate read operations. They
never enter a draft save or supply an outcome approval.

The current `wedl-rust/v1` contract supports private readiness, append-only human
proposal history and complete previews of explicit consequence plans. Approved
and applied outcomes are specified separately in [OUTCOMES.md](OUTCOMES.md).
Action and proposal documents require `evidence` arrays of retained historical
citations. Saved outcome review verifies their original source before approval.
Private `hypothesisRefs` link existing possibilities without adopting them or
adding character knowledge. Exact historical list/detail reads are implemented;
general retained edits have a separate [edit contract](EDITS.md). Only explicit
human resolution executes. Unknown fields and unsupported operations are rejected.
Source remains `wedl/v1`; required `proposals`, `evidence` and `hypothesisRefs`
arrays have no legacy adapter or migration.

## Durable source and independent bindings

Each saved document is a closed UTF-8 JSON object at
`author/actions/<action-id>.json`. The canonical source loader reads only
`story/`, so private documents do not affect canonical record counts, the source
fingerprint, validation of story records, any search corpus or character context.
The private tree is durable author source; it is not a disposable index. Authors
can track it in their own Git repository. These operations do not create Git
commits, stage files or rebuild an index.

| Binding | Meaning |
| --- | --- |
| Draft `baseFingerprint` | Original canonical source when the new action was created. Immutable after creation. |
| Proposal `contextFingerprint` and plan `expectedFingerprint` | Equal fingerprints of the canonical source captured for that proposal. Retained unchanged with its original plan. |
| Review frame `fingerprint` and `reviewSha256` | The current exact source, draft and explicit context selections the human reviews. A review never replaces the original base. |
| Save `expectedFingerprint` and `expectedSha256` | Current canonical source plus exact prior private-file bytes. These guard the requested private write. |

`sourceChanged` compares the original base with current canonical source.
`effectivePhase` instead describes the current applicability of proposal
history: an authored `proposed` action reads as effectively `stale` if any
unsuperseded proposal has a different context fingerprint. A newer parallel
proposal does not retire an older one; explicit supersession does. Consequently,
`sourceChanged:true` and `effectivePhase:proposed` can coexist after an explicit
refresh that preserves the original base. Metadata also returns ordered
`activeProposalIds`, meaning unsuperseded alternatives; the authored phase still
governs whether they can be previewed.

These are read-only reports. They never rewrite the saved phase, refresh a plan
or establish sufficiency. A private save alone cannot change the canonical
fingerprint. Reads and previews do not create directories, lock files or caches.

Current store reads require a current, valid worktree. The live author server
captures current source for each canonical request; historical sessions retain
their selected snapshot.
Saved documents with stale actor, target or hypothesis pointers remain
structurally readable; saving or reviewing them requires every live pointer to
resolve. A hypothesis link may name any existing open, adopted or rejected
hypothesis, but it cannot name another record kind or a section.

`list` and `detail` also accept a selected historical world at a literal full
lowercase Git commit OID. They read the original private tree/blob at that same
commit, preserve exact private bytes, and return `revision`, `path` and `blobOid`.
Commit, intermediate tree and blob identities are verified; regular-file modes,
safe paths and complete store/source bounds are enforced. An absent historical
store lists empty; a missing requested detail document or malformed source is
refused. Neither case falls back to current files.
Historical `sourceChanged` and effective proposal phase compare against the
selected historical canonical snapshot, independently of live source changes.
An invalid or pending live worktree does not hide valid historical private
content. All save, readiness, saved-proposal, outcome and current receipt
operations still require the current worktree and reject historical captures.
The browser makes historical private input read-only.

A historical citation is exactly `{revision,path,blob_oid,target}`, with a full
40- or 64-character lowercase Git OID, safe `story/*.md` path and closed record,
section or transition target. Member targets use `owner_id`, preserving the
existing [assertion citation grammar](ASSERTIONS.md#historical-citation-syntax).
Each required `evidence` array contains at most 1,024 unique items; `[]` is valid.
Historical IDs need only be nonblank: these are authored pointers, not live
reference or blob-resolution requests. Syntax validation grants no character
knowledge, access, readiness or outcome. No public citation resolver is added.
Private save, readiness review and ordinary saved-proposal preview retain this
syntax-only boundary. [Outcome review](OUTCOMES.md#original-evidence) resolves
the draft's citations followed by the selected proposal's citations against
exact original Git worlds and binds each complete owner byte digest. Historical
IDs and members are resolved there; current names or files cannot repair them.

## Closed draft document

Every draft has exactly these required fields:

| Field | Contract |
| --- | --- |
| `id` | `action_` followed by 26 uppercase Crockford characters; immutable. |
| `title` | Nonblank text, at most 1,024 UTF-8 bytes. |
| `worldId` | Actual canonical world ID; immutable. |
| `baseFingerprint` | Exact lowercase 64-character source hash; immutable. |
| `horizon` | Exactly `{timeline,tick,order}`. Timeline is explicit; tick/order are canonical decimal strings for signed i64/i32. |
| `intent` | Nonblank private prose, at most 16,384 UTF-8 bytes. |
| `actorRefs` | Nonempty, ordered, unique record references to actual characters or organizations; at most 1,024. |
| `targetRefs` | Ordered, unique record or section references; at most 1,024. No learning or visibility grant. |
| `hypothesisRefs` | Required ordered, unique record references to actual hypotheses; at most 1,024. Private reasoning links only. |
| `evidence` | Required array of closed, unique historical citations; at most 1,024. Syntax-checked on private save and resolved during outcome review. |
| `questions` | At most 128 ordered questions with unique `question_` IDs. |
| `proposals` | Required ordered array, initially empty; at most 64 closed human proposals. |
| `phase` | One of the seven phases below. |
| `bodyMarkdown` | Exact private Markdown string, at most 131,072 UTF-8 bytes; empty is allowed. |

A record pointer is exactly `{"kind":"record","id":"…"}`. A section pointer
is exactly `{"kind":"section","ownerId":"…","id":"…"}` and must name one
actual member of that exact owner. It reuses the brief reference resolver,
including declared world chronology sections. Actor/target arrays do not accept
transition pointers or hypotheses. The separate `hypothesisRefs` array accepts
only record pointers to hypotheses. Its contents enter the private draft and
review hash, while admitted context, canonical state and retrieval remain
independently selected.

A question is exactly `{id,text,required}` plus optional `answer`. Question text
is nonblank and at most 4,096 bytes; an answer is at most 16,384 bytes. Omitting
`answer` means unanswered. An explicit `""` is an answer. `null`, booleans and
numbers are invalid answers. Required-answer presence is a mechanical check,
independent of whether a human considers the answer informative or sufficient.

### Immutable proposal history

Each proposal has exactly the required fields below, plus optional `supersedes`:

| Field | Contract |
| --- | --- |
| `id` | Unique `proposal_` ID with 26 uppercase Crockford characters. |
| `summary` | Nonblank human-authored text, at most 16,384 UTF-8 bytes. |
| `contextFingerprint` | Exact lowercase source fingerprint captured for this proposal. |
| `questionAnswers` | Ordered array of at most 128 exact `{questionId,answer}` snapshots. Answers are strings, including explicit empty strings. |
| `resolver` | Exactly the string `"human"`. |
| `evidence` | Required historical citation array; retained in immutable proposal history. |
| `consequences` | A closed native [consequence plan](CONSEQUENCES.md), whose `expectedFingerprint` equals `contextFingerprint`. |
| `supersedes` | If present, an exact earlier proposal ID; null, self-reference and forward references are invalid. |

Previously saved proposals form an immutable prefix. Updates cannot remove,
reorder, edit or rebind them. Every historical answer ID must remain in the
draft's questions, in the same relative question order. Current question text
and answers can change through permitted stale edits while earlier answer
snapshots remain unchanged. Optional questions never named by a historical
snapshot can still be removed.

Each new proposal snapshots every answer-present question in current order,
including optional answers and explicit empty answers. Its context and plan
fingerprints must equal the current save fingerprint. Original base, earlier
proposal contexts and earlier plans are preserved. A proposal does not embed
the full-draft review hash; the save request supplies that fresh confirmation
separately, avoiding a circular digest.

Plans require `{expectedFingerprint,operations}` and optionally `analysis`, with 1–64 explicit
operations. Supported operation shapes are
`{type:"record.create",draft:"<native YAML>"}` and
`{type:"record.update",entityId:"<exact ID>",patch:"<native YAML>"}`.
The `draft` and `patch` fields are raw strings using the existing record-create
and record-update grammars. History preserves their numeric tokens, comments,
Markdown and line endings without browser number coercion. Creates supply
permanent IDs; there is no temporary-ID allocator, delete or inferred operation.

Historical plans receive structural validation without resolving their old
targets against current source. A new append must construct and preview its
whole candidate within the complete preview bounds. Malformed or unsupported
source inputs, duplicate writes and construction failures refuse the append.
A successfully constructed candidate with `validation.valid:false` may remain
an inert private proposal despite its diagnostic errors. The plan is retained;
diagnostics are computed by preview rather than stored as approval or proposal
fields. Saving it neither makes the candidate valid nor approves or applies it.
Construction uses the captured
base codec: new profile or target-kind bindings that another operation would
introduce remain unsupported. See [CONSEQUENCES.md](CONSEQUENCES.md) for the
complete candidate and bound contracts.

All JSON inputs reject duplicate and unknown keys, including nested keys.
Canonical decimal strings reject numeric JSON, leading zeroes, `+1`, `-0`,
fractions and out-of-range coordinates. Saved source is deterministic compact
JSON followed by one LF; strings preserve Markdown content, CRLF and trailing
spaces. The stored file SHA-256 hashes exact bytes, including its final LF.
External formatting changes therefore invalidate a prior file hash.

## Lifecycle and human readiness

A new action must start in `draft` with empty `proposals` and the current source
as its original base. Updates retain ID, world ID and original base. The table
gives every supported transition; the proposal and review conditions below
also apply.

| Existing phase | Allowed next phases |
| --- | --- |
| `draft` | `draft`, `context-needed`, `resolution-needed`, `stale`, `cancelled` |
| `context-needed` | `context-needed`, `resolution-needed`, `stale`, `cancelled` |
| `resolution-needed` | `resolution-needed`, `proposed`, `stale`, `cancelled` |
| `proposed` | Unchanged `proposed`, phase-only `resolution-needed` or `cancelled`, or `stale` for content edits |
| `stale` | `stale`, `context-needed`, `proposed`, `cancelled` |
| `cancelled` | None, including an unchanged save. |
| `applied` | None. Only the dedicated approved outcome operation can enter this phase. |

Every `resolution-needed` save and every newly appended proposal require all
required questions to have answers, a current complete review of the exact
completed draft, and explicit human sufficiency confirmation. This includes
same-phase resolution edits and phase-only rejection back to resolution.
Entering `proposed` requires a new append from `resolution-needed` or `stale`;
changing only the phase cannot revive old proposal provenance.

Editing proposed title, intent, questions, answers, body or other content
requires `stale`. Phase-only rejection or cancellation preserves all content.
An unchanged proposed save is allowed with `review:null`; cancellation is
terminal even for an unchanged save. Every save outside resolution readiness or
a new proposal append requires `review:null`. Review without answers remains
useful, but cannot establish readiness. No ordinary private save grants outcome approval or
canonical effects. Applied documents additionally contain `appliedReceipt`;
that optional field is required only in `applied` and forbidden otherwise.
A draft save cannot introduce or modify it. See [OUTCOMES.md](OUTCOMES.md).

## Exact review context

A review request is exactly:

```json
{"draft":{},"frame":{},"briefs":[],"families":{},"maxCharacters":100000}
```

Replace the empty draft and frame with complete objects; an empty family
selection and no briefs are valid. The six-field frame grammar is specified in
[WRITING_FRAMES.md](WRITING_FRAMES.md). The frame supplies the exact current
source, scene, POV and lens. **The draft's saved horizon supplies the review
clock**, replacing the frame's clock. The selected scene and POV must be valid
exact scene and character IDs. Character and dramatic review require the POV to
be actually present at that saved horizon. Author review retains the operational
author view and makes no claim of POV presence or scene applicability. The input
frame is not mutated.

The result contains `frame`, `draftSha256`, `reviewSha256`, `requiredAnswered`,
`unansweredRequired`, `context`, `maxCharacters` and the existing `protocol`.
The normalized frame discloses the clock actually used. Character prompts use
only independently admitted evidence; dramatic review keeps the same character
prompt and a separate author margin. Intent, body, question answers and proposal
plans are not inserted into either context component as facts. Optional briefs
and family selections obey their existing all-or-nothing admission rules.

`draftSha256` hashes deterministic draft source. `reviewSha256` binds the full
normalized review, exact draft hash, explicit briefs and family selection. It is
an integrity check, not a signed authorization or proof that someone read the
packet. The complete review must fit its Unicode-scalar budget, from 256 to
100,000 characters. It never truncates a required component.

The save confirmation is exactly
`{frame,briefs,families,maxCharacters,reviewSha256,sufficient}` with
`sufficient:true`. The save consumer recomputes the review. Changes to draft
content, questions, phase, horizon, context selections, lens, budget or canonical
source invalidate the old binding. Appending a proposal therefore requires a
new review of the full proposed draft, including its history and raw plan; an
earlier resolution review cannot be reused. The explicit sufficiency decision
remains a human judgment. Confirmation is not stored as reusable approval
authority; outcome publication uses the separate complete review and explicit
approval contract in [OUTCOMES.md](OUTCOMES.md).

## Guarded private writes

A save/preview request is exactly
`{expectedFingerprint,expectedSha256,draft,review}`. Creation requires
`expectedSha256:null` and no existing file. Update requires the exact stored
SHA-256. The caller supplies an ID, never a destination path. Preview returns
the complete before/after source and hashes without writing. Save uses the same
validation and preserves all unrelated private and canonical files.

The writer checks current source and prior draft bytes, takes the shared native
writer lock, repeats admission, writes a same-directory temporary file, syncs
its contents, rechecks source/prior bytes, then publishes atomically. Creation
does not overwrite an existing file. Parents, targets and locks must be real
directories/regular files; symlinks and special files are rejected. Unix creates
private directories with mode 0700 and files with mode 0600; updates preserve
existing file permissions. Advisory locks coordinate native writers, not
arbitrary external editors. No filesystem compare-and-swap, receipt replay,
journal recovery or power-loss durability is claimed.

An ordinary valid unchanged save reports no write. A changed save makes an exact
retry with the old prior hash stale: inspect the current saved bytes before
preparing a new request. Private cancellation preserves the document; no delete
command is implemented. Listing reads each document's exact bytes and returns
its own hash; it is not a transactional snapshot of the entire private tree.

| Complete input bound | Maximum |
| --- | --- |
| Draft document, including canonical LF | 262,144 bytes |
| Proposal history | 64 members, still within the complete document bound |
| Save or review request | 524,288 bytes |
| Parsed request nesting / structural nodes | 32 / 16,384 |
| Saved action documents | 1,024 |
| Complete committed private store bytes | 16 MiB |
| Ignored publication temporary-file bytes | A separate 16 MiB aggregate; 262,144 bytes per file |
| Entries scanned in the private action directory | 2,048 |

Unexpected entries in the dedicated action directory fail closed. Only bounded
regular files with the writer's `.wedl-action-` temporary prefix are omitted
from discovery. Their byte allowance is separate from the committed-document
allowance; both kinds count toward the 2,048-entry scan limit. Reads never clean
up a temporary file or silently drop an invalid committed document. Updates
account for the complete projected committed store before publication, releasing
the replaced document's old byte charge.

## Preview an exact saved proposal

`action_store::proposal_preview` reads one saved, active human proposal and
delegates to the pure consequence preview. Its closed request has exactly:

```json
{
  "expectedFingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "actionId": "action_00000000000000000000000001",
  "expectedSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "proposalId": "proposal_00000000000000000000000001",
  "maxCharacters": 100000
}
```

The sample hashes illustrate shape; use the current canonical fingerprint and
exact saved private-file hash. The request is at most 16 KiB and carries no
replacement draft, plan, frame or approval. Both the saved phase and effective
phase must be `proposed`; the selected proposal must be unsuperseded and its
context/plan pins must match current source. A stale unsuperseded parallel
alternative blocks this preview even if the selected alternative is current.
A historical capture, stale source/private pins and cancelled or rejected
proposals are refused for this current-only candidate operation.

The result retains the exact action/proposal IDs, private `sourceSha256`,
original `baseFingerprint`, current `contextFingerprint`, `proposalSha256`,
wrapper `previewSha256`, phase and effective phase. `consequences` contains the
complete native candidate report: exact before/after source, hashes and diff,
candidate fingerprint and validation diagnostics. The whole result, including
wrapper metadata and digest, must fit `maxCharacters` from 256 to 1,000,000.
No component is truncated. Both `writesPerformed` and `canApply` are always
false, including for a valid candidate. Hashes bind reviewable content; they
are not reusable outcome approval capabilities.

The CLI reads the request above with
`action-drafts preview-proposal --file request.json`. Its closed GET counterpart
is `/api/action-drafts/preview-proposal`, requiring `expectedFingerprint`,
`id`, `sha256` and `proposalId`; optional `maxCharacters` defaults to 100,000.
The GET names `id` and `sha256` correspond to request fields `actionId` and
`expectedSha256`. A standalone unsaved plan can be inspected with
`consequences preview --file plan.json --max-characters 100000`.

## Separate captured magic requirements

`action-requirements capture` admits an exact selected magic reading. The
separate family selection must contain only `magic`; other family members are
rejected. Author captures use exact-horizon native role composition and require
`linkageComplete:true`. Ordinary family context can still disclose incomplete
linkage; that is not actionable capture readiness. Captures retain source
citations and fail if records, evidence or complete output exceed their bounds.

Character and dramatic requirement captures use only explicit learned targets.
Dramatic capture deliberately follows the learned-only supplement contract,
while ordinary dramatic writing review still has its separate author margin.
Canonical role fields are never looked up to evaluate a character requirement.
An unknown learned target may yield a complete empty report, without learning
anything. The supplied author selector is inert in these learned-only reads.

A literal requirement has exactly three fields:

```json
{"targetId":"concept_…","selector":{"field":"restrictions"},"expectedValueRaw":"null"}
```

The selector contains exactly one root `field` or `assertionId`. `targetId` is
an exact concept ID. `expectedValueRaw` is JSON text, not an already parsed
JavaScript number. Compare recaptures the source from the exact frame and
selection; it never trusts imported capture contents or hashes.

| Result | Meaning |
| --- | --- |
| `matches` | One available author operand equals the explicitly supplied literal. |
| `differs` | One available author operand differs from that literal. |
| `unknown` | Learned-only lens, absent/withheld/unsupported operand, ambiguity, or disputed assertion. |

A field must be explicitly selected as a restriction or exception in the
captured native composition. An assertion must be the selected admitted
`author-fact` for the target. Any other admitted assertion on the same exact
subject/predicate keeps the comparison unknown; no predicate declares a winning
value. Eligible corrections are already applied by native admission. Attributed
claims and unresolved accounts do not become author facts through comparison.

Equality is type-sensitive: strings differ from numbers, null differs from an
empty array, object-key order is irrelevant, and array order matters. Decimal
numeric tokens are normalized as coefficient/exponent strings, without binary64
conversion. Thus `1`, `1.0` and `1e0` match, while `9007199254740993` differs from
`9007199254740992`. Actual operands are reconstructed from the native projected
value **and its used type**; projected integer strings remain integers and
Ref-shaped literal objects remain literal objects. Native YAML floating-point
source has its existing finite-number precision; comparison cannot recover
digits already rounded by that source codec.

Expected literals are limited to 16,384 bytes, nesting 32, numeric token length
256, and decimal exponent magnitude 1,024 before trailing-zero normalization.
Duplicate keys and invalid/over-budget expected literals fail. Unsupported
actual operands yield unknown. Author captures admit at most 64 records, 256
assertions and 256 sections; learned captures admit at most 256 reports. Complete
capture and comparison output must fit 262,144 bytes and the requested character
budget. Public service/CLI/GET budgets are 256–100,000 characters; the native
capture function additionally supports 1–1,000,000, still under the byte cap.

## CLI and browser

```sh
wedl-rs --repo WORLD action-drafts list
wedl-rs --repo WORLD action-drafts detail ACTION_ID --sha256 STORED_SHA
wedl-rs --repo WORLD action-drafts preview --file save.json
wedl-rs --repo WORLD action-drafts save --file save.json
wedl-rs --repo WORLD action-drafts review --file review.json
wedl-rs --repo WORLD action-drafts preview-proposal --file proposal-preview.json
wedl-rs --repo WORLD consequences preview --file plan.json --max-characters 100000
wedl-rs --repo WORLD action-requirements capture --file frame.json --families '{"magic":{...}}'
wedl-rs --repo WORLD action-requirements compare --file frame.json --families '{"magic":{...}}' --requirement '{...}'
```

Closed GET routes expose `/api/action-drafts/list`, `/detail?id=…&sha256=…`,
`/review?request=…`, the saved `/preview-proposal` grammar above, and
`/api/action-requirements/capture` or `/compare`.
Requirement routes use explicit `frame`, `families`, optional `maxCharacters`,
and `requirement` for comparison. Query parameters and duplicate keys are
closed. The existing 16 KiB raw URL-query cap remains in force. Large requests
for those reads use the CLI. Current local-author sessions also accept the exact
save request at POST `/api/action-drafts/save`; reviewed outcome application has
its separate [outcome contract](OUTCOMES.md). See
[browser publication](BROWSER_PUBLICATION.md) for the write boundary.

The browser **Actions** view keeps unsaved input in this tab, supports exact
import, editable answer presence, saved draft loading, deliberate context
selection, review and separate sufficiency confirmation. Its proposal editor
retains raw YAML operation strings, appends human alternatives locally and
displays immutable history and currentness. After an explicit browser or CLI
save, load the exact saved proposed draft to preview a selected active alternative.
Unsaved draft or proposal-input edits invalidate that saved preview; it cannot
stand in for the edited work. Downloads remain request files to inspect and save
with the CLI. Downloading does not persist an action.
Navigation, source/selection changes and edits invalidate pending or displayed
review while retaining private input. Late replies cannot replace newer input.
Unsaved input retains its exact pins until an explicit capture or reload.
Imported Markdown keeps CRLF until its body field is explicitly edited. Local
discard affects only this tab. Requirement capture/comparison remains visibly
separate from the save request.

## Worked requests and pinned evidence

[The worked request guide](../examples/actions/README.md) exercises the complete
207-request authoring world without adding canonical records. The existing
canonical sequence remains 144 records, 25 profiles, nine predicates and 24
editable non-world kinds. A private create/update/cancel sequence changes only
its separate action document.

[The proposal walkthrough](../examples/actions/PROPOSALS.md) adds explicit human
alternatives, diagnostic candidate previews, stale-source handling and
supersession without adding canonical records. The standalone plan contract is
documented in [CONSEQUENCES.md](CONSEQUENCES.md).

The functional reference is the fixed
[WEDL commit](https://github.com/Sornaensis/wedl/tree/f0e885ccebc279eff65a5db2984d01ebc6647b2b).
Inspected primary sources include
[action_drafts.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/action_drafts.py),
[action_resolution.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/action_resolution.py),
[the action guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/action-drafts.md),
[magic_context.mjs](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/static/magic_context.mjs),
and the action source, persistence, transport, review and browser tests. Native
JSON, fingerprints, file hashes and atomic private saves are deliberate current
Rust contract choices; exact upstream source/wire/commit machinery is not the
target. Human proposals and diagnostic consequence previews are implemented;
outcome approval/publication and applied receipts are now implemented through
the separate [outcome contract](OUTCOMES.md). Optional immutable plan analysis
adds [explicit semantic checks and same-horizon deltas](SEMANTIC_CONSEQUENCES.md);
standalone [three-way revision review](REVISION_REVIEW.md) is implemented.
Retcon impact/review and event-local/grouped analysis are implemented through
their separate informational contracts.


## Independent retcon review

An explicit consequence plan can also be inspected with
[typed retcon impact](RETCON_IMPACT.md) or
[grouped consequences](EVENT_CONSEQUENCES.md). These reads share candidate
validation and required expectation gates. [Saved retcon decisions](RETCON_REVIEWS.md)
retain their own immutable analysis and currentness witness in a separate
private store. They do not mutate an action/proposal, answer its required
questions, satisfy human readiness or replace explicit outcome approval.
The browser copies an Actions plan only through an explicit copy control;
subsequent plans and private input remain local to their respective tabs.
