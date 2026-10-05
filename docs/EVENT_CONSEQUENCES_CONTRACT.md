# Event consequences and candidate verification v1

This is the shared contract for additive consequence authoring, event verification,
and preview comparison. It specifies the downstream implementation; its publication
alone does not advertise executable commands or endpoints. Discover actual support
through the installed changeset/authoring schemas and CLI help. Existing
`wedl-changeset/v1`, source versions, world capabilities, raw writes, and ordinary
query response versions remain unchanged.

Markdown and Git remain authoritative. A consequence is an expressly authored
effect, timed transition, or outcome link, never an inference from prose, presence,
participants, genealogy, thread membership, a trigger, or a search result. This
contract adds no alternate canon, identity system, migration, or source field.
It follows [ADR 0002](decisions/0002-shared-world-concurrent-narrative-threads.md),
[ADR 0003](decisions/0003-calendar-and-historical-chronology-semantics.md),
[ADR 0004](decisions/0004-spatial-domain-query-and-version-contract.md),
[ADR 0005](decisions/0005-first-class-generational-history.md), and
[ADR 0006](decisions/0006-preserve-object-affordances-in-v07.md).

## Exact values and schema notation

All objects defined here are closed: reject unknown members, duplicate JSON keys,
wrong types, nonfinite numbers, and missing required members. In the shape tables,
all members are required unless prefixed with `?`; `A | B` is a discriminated union,
`T[]` is an array of T, and literal strings are constants. No implicit inheritance,
defaults, coercion, or free-form extension exists in these transport objects.
Source frontmatter, genealogy assertions, and authored state values retain their
existing source schemas, including permitted opaque source extensions.

| Type | Exact definition |
| --- | --- |
| `Text` | Nonempty JSON string; whitespace-only input is invalid. |
| `Revision` | Lowercase full Git commit SHA, `^[0-9a-f]{40}$`; no `HEAD`, abbreviated SHA, branch, or tag. |
| `Hash` | Lowercase SHA-256 hex, `^[0-9a-f]{64}$`. |
| `Time` | `{timeline: Text, tick: Decimal, order: Decimal}` on a declared timeline. |
| `Decimal` | String matching `^(0\|-?[1-9][0-9]*)$`; tick in signed i64, order in signed i32. Reject `-0`, `+1`, leading zeroes, numeric JSON, and overflow. |
| `Ref` | Stable ID, exact title/alias, or existing title/alias slug; only explicitly typed reference positions resolve names. |
| `Temp` | A request-local declaration string beginning `tmp:` with a nonempty suffix. It is never a source ID. |
| `Value` | Finite JSON value, checked against the selected world's authored state-key type where used as state. |

`Time` is the exact global, timeline-scoped ordinal tuple. Horizons are inclusive;
compare numeric tick/order on the same timeline, never their decimal spelling.
No predecessor arithmetic, scene-cursor fallback, wallclock, civil-date conversion,
tick duration, or cross-timeline ordering supplies a missing horizon. Source
frontmatter stores integer tick/order. The new consequence transport normalizes
its `Time` leaves to integers once at the source boundary and serializes them back
as canonical decimal strings in every new report/citation. Existing raw changeset
operations retain their existing integer-time input format.

## Authenticated author scope

Verification v1 has exactly one mode: authenticated `author-as-of`. It rejects
character mode, all-time mode, and caller-supplied `mode`, `viewer`, `character`,
`audience`, or `perspective` fields. CLI and HTTP establish the same trusted local
author context; the request cannot establish or widen it. There is no new identity
framework. If the transport cannot establish an authenticated author scope, it
returns its existing authentication error before loading/resolving world data.

Apply the established scope in this order: authenticated identity, server-derived
audience/perspective, supported source capabilities, selected timeline and horizon,
record/section audience and perspective, time applicability, then resolution of
eligible references, derived folds, ordering, counts, limits, and serialization.
The authorization decision precedes resolution and filtering; resolution operates
only on that authorized view. A named hidden, absent, or future explicit target
has the same `unavailable`/`CONSEQUENCE-UNAVAILABLE-001` result without suggestions,
candidate IDs, hidden counts, paths, titles, or rejection details. An ambiguous
eligible name is `invalid` without listing candidates. Unsupported declared source
capabilities remain closed failures; never silently read part of a world.

Existing genealogy knowledge is still an assertion, not canonical genealogy. Its
learning time, assertion validity, disclosure, evidence admission, and capability
rules follow [GENERATIONAL_SOURCE_CONTRACT.md](GENERATIONAL_SOURCE_CONTRACT.md)
and [GENERATIONAL_QUERY.md](GENERATIONAL_QUERY.md). A consequence report does not
grant knowledge, expose withheld facts, or equate a belief with truth.

## Additive operations and discovery

New raw operation discriminators use `type`; the authoring wrapper uses `action`.
Do not revive the dormant `knowledge.transition` allocation branch as a public
operation or accept historical-effects append. The new raw union is exactly:

| Discriminator | Exact members besides `type` |
| --- | --- |
| `knowledge.create` | `temporaryId: Temp, value: {frontmatter: KnowledgeSource, bodyMarkdown: string}` |
| `relationship.create` | `temporaryId: Temp, value: {frontmatter: RelationshipSource, bodyMarkdown: string}` |
| `knowledge.transition.append` | `knowledge: Ref, transition: KnowledgeTransitionInput` |
| `relationship.transition.append` | `relationship: Ref, transition: RelationshipTransitionInput` |
| `story-point.transition.append` | `storyPoint: Ref, transition: PlotTransitionInput` |
| `outcome.link` | `event: Ref, storyPoints: Ref[], scenes: Ref[]` |
| `expectation.check` | `event: Ref, at: Time, policy: "required" | "advisory", items: Check[]` |

`KnowledgeSource` and `RelationshipSource` are actual source envelopes of the
matching kind at the world version: schema, kind, title, domain, status, tags,
aliases, and existing kind-specific members. `id` may be omitted for the declared
temporary ID; if supplied it must be a valid unused matching stable ID and the
temporary ID maps to that exact ID. Reject duplicate declarations, duplicate
stable IDs, cross-kind creates, upserts disguised as creates, and version/capability
changes. Preserve the Markdown body and all preexisting transition bytes/values.
Create knowledge through the existing knowledge/genealogy validation and evidence
admission path; `claim.genealogy` requires its already declared capability.

The new closed transition input shapes retain existing source member spellings:

| Type | Required members | Optional members |
| --- | --- | --- |
| `KnowledgeTransitionInput` | `time: Time`, `state` (accepted, suspected, rejected, uncertain, remembered, forgotten), `causing_event: Ref` | `id` (valid `kt_` ID or scoped Temp), `confidence` (finite number 0..1), `acquisition: Text`, `source_entity: Ref`, `note: string` |
| `RelationshipTransitionInput` | `time: Time`, `causing_event: Ref` | `id` (valid `rt_` ID or scoped Temp), `relationship_status: Text`, `metrics` (object of finite numeric values), `facets: Text[]`, `note: string` |
| `PlotTransitionInput` | `time: Time`, `state` (dormant, active, resolved, failed, cancelled), `causing_event: Ref` | `id` (valid `spt_` ID or scoped Temp), `note: string` |

The append operations' transition members use these Input types. These are the
additive API's accepted shapes, not a retroactive restriction on
all legacy source. Existing relationship fold defaults (`active`, `{}`, `[]`) are
retained when the optional members are absent. No inverse relationship is created.
Append to `transitions` or `lifecycle.transitions` without replacing/sorting history.
The new point must follow the last point strictly on the same timeline; IDs must
be unique in that history. Plot `eligible`/`blocked` are derived, never stored.
Every new noninitial typed append requires an explicit `causing_event`; the
record must already exist, or be created earlier in the operations array before
its append. Initial transitions in create frontmatter and legacy generic/raw/source
records retain their existing optional-cause rules. A supplied cause must be a canonical event
on the same timeline at or before the transition; equality is valid under
`_validate_typed_causes`. Event `causes` instead requires a strictly earlier event.
Genealogy evidence and learning restrictions still apply; the new operations
cannot bypass them or synthesize evidence from a causing-event reference.

`outcome.link` requires at least one expressly supplied target across both arrays.
Targets must be unique and correctly typed. It appends only the event to each
supplied story point's `outcome_events`, that story point to the event's
`related_story_points`, and the event to each supplied scene's `outcome_events`.
It repairs a missing supplied reciprocal half but never adds other targets,
transitions, effects, causes, participants, or a fictional scene membership.
An already linked half is a no-op. Preserve existing order; append story-point
outcomes only when their event coordinate strictly follows the existing tail.
Scene outcomes must meet the existing canonical-event and inclusive scene-window
validation. Reject invalid order/intervals instead of silently reordering history.

The exact authoring request is:

```text
ConsequenceBatch = {
  action: "consequence.batch", expectedHead: Revision,
  idempotencyKey: Text, operations: (ExistingOperation | NewOperation)[],
  ?summary: Text, ?consequenceRequest: DeltaRequest
}
```

`operations` has 1..1000 members. The wrapper compiles to the same
`wedl-changeset/v1` envelope with a deterministic summary when omitted. No inferred
HEAD/key, unsafe confirmation bypass, or implicit current-time advance is allowed.
The optional `consequenceRequest` is retained as an additive top-level member in
that changeset, so its horizon/limit are part of hashing, confirmation, and replay.
Raw changeset previews accept the same optional member; its omission preserves
the existing preview shape only when no `expectation.check` operation is present.
It requests one delta, not a new persistence protocol. Check operations always
produce the independent `expectationChecks` field defined below, with or without
`consequenceRequest`.
`ExistingOperation` is the installed existing raw changeset operation union,
including generic entity updates used for explicit scene/current-time edits.
Those operations retain their existing shape and integer-time codec; only the
wrapper's event.create and new typed append/check/report time leaves use Time
strings. Their inclusion changes no generic operation semantics and supplies no
implicit cursor advancement.
At `expectedHead`, establish scope, resolve existing names through the ordinary
resolver, allocate all declared temporary IDs deterministically, and resolve
forward temporary references before validating the complete candidate. Names of
new records are not aliases for temporary IDs. Reject undeclared/colliding temporary
references and wrong-kind references. Substitute only declared reference leaves:
operation targets, source knower/from/to, typed claim/evidence references, transition
source/cause references, event location/participants/causes/related story points,
typed entity state values, outcome targets, and predicate references. Never replace
body prose, statements, notes, arbitrary strings, metric keys, thread labels, or
opaque extensions. Raw legacy operations retain their existing behavior.

### Auxiliary allocation and input/source separation

New typed inputs may omit auxiliary `id`, supply a valid unused stable ID of the
matching prefix, or declare a scoped Temp in that `id` leaf. This applies to
knowledge/relationship/plot append IDs, event effect IDs, and IDs of transitions
inside typed create frontmatter. Persisted knowledge/relationship transitions and
effects always have valid `kt_`/`rt_`/`effect_` IDs. Persisted plot append IDs are
allocated as `spt_`; preexisting plot source lacking IDs remains unchanged.
Report transition/effect IDs are resolved stable IDs, never Temp labels. Reports
of legacy knowledge/relationship transitions retain mandatory stable IDs and
optional causes; legacy plot IDs/causes remain optional. Thus the reported
`KnowledgeTransition`, `RelationshipTransition`, and `PlotTransition` are source
shapes with Time strings, distinct from the mandatory-cause typed Input shapes.

First normalize the complete request's known transport Time leaves (event time,
typed append time, check `at`, consequence request `at`) to canonical source
integers, while preserving omitted/scoped ID input leaves. Source create
frontmatter already uses source integers; reject wrong codecs there. Compute
requestHash once from that complete unallocated normalized changeset. Never hash
allocated IDs back into their own seed. Allocate using existing `id_from_seed`
with seed `requestHash + ":" + operationIndex + ":" + collection + ":" + elementIndex`;
indexes are zero-based canonical decimal integers. Collection is exactly `effects`
for event effects, `transitions` for knowledge/relationship transitions (including
create frontmatter), or `lifecycle.transitions` for plot transitions. An append's
elementIndex is 0. Matching kind prefixes select effect, knowledge-transition,
relationship-transition, or story-point-transition. Scoped label spelling does
not change the seed format, but remains in requestHash. Entity temporary ID
allocation retains the existing request-hash/operation-position entity allocator.

Reject duplicate Temp declarations across entity/auxiliary namespaces, stable ID
collisions in the applicable record history/event effects, cross-kind IDs, and
undeclared or wrong-kind Temp references. Supplied stable IDs are preserved,
not reallocated. Scoped auxiliary declarations may be referenced forward only
at schema-declared auxiliary identifier leaves (for example `transitionId` and
typed genealogy evidence `transition_id`); the owner record must also match.
An omitted ID has no request label: reference it only after discovery of its
generated ID in preview, with a new request/confirmation if the request changes.
Publish scoped label mappings in existing `generatedIds` alongside entity mappings;
omitted IDs are visible in normalized candidate/report/diff. Entity reference
leaves never resolve an auxiliary label as an entity. Preserve source-frontmatter
time/reference leaves and schema-specific claim/evidence validity/learning times;
do not recurse into arbitrary extension objects looking for times or identifiers.

Discovery must advertise accepted and executable variants together, including full
closed JSON Schemas (required/optional members, unions, reference kinds, bounds,
time codecs, examples), not only field-name hints. Contextual discovery has the
exact optional input and response extension below, at one authorized exact revision:

```text
SchemaContextRequest = {revision: Revision}
SchemaContext = {
  revision: Revision, sourceSchema: Text, capabilities: Text[],
  stateKeys: StateKeyDefinition[], relationshipMetrics: MetricDefinition[]
}
StateKeyDefinition = {
  entityKind: Text, key: Text, valueType: "string" | "entity" | "boolean" | "number" | "array" | "object" | "null",
  entityKindConstraint: Nullable<Text>, nullable: boolean,
  itemDefinition: Nullable<Value>, objectDefinition: Nullable<Value>,
  exclusiveGroup: Nullable<Text>, allowedValues: Nullable<Value[]>
}
MetricDefinition = {name: Text, minimum: Nullable<number>, maximum: Nullable<number>}
```

Use optional `--revision SHA` with `--repo` for CLI context and optional exact
`revision` query input to schema HTTP discovery; no context input retains the
current context-free response. The existing `wedl-changeset-schema/v1` response
gains only optional `context:SchemaContext` plus complete `$defs`/operation schemas.
Context `capabilities` uses the world's exact registry order (empty for legacy
worlds); sourceSchema is the selected homogeneous source version. Unsupported
declared capability fails before contextual fields are emitted. State-key type,
entity-kind constraints, exclusivity groups, allowed values, array/item/object
rules, and inclusive metric bounds come from the same validated world definitions
used by typed validators; null means an absent constraint, never a guessed default.
Reject an unrepresentable contextual type explicitly rather than advertising
unchecked support. Sort state keys by entityKind/key and metrics by name. Nullable
item/object definitions contain only the admitted declarative JSON type schema,
never executable expressions or arbitrary source paths. Context request/output
and concrete operation schemas share these definitions and bound checks; no
parallel documentation-only validator or inferred schema is allowed.

Complete the existing
`event.create` schema with its actual integer-time raw shape: required `type`,
`temporaryId`, `title`, `time`; optional `id`, `domain`, `status`, `tags`, `aliases`,
`section_audiences`, `location`, `participants`, `causes`, `relatedStoryPoints`,
`effects`, `bodyMarkdown`. Document the existing `related_story_points` compatibility
spelling separately; reject conflicting dual spellings in the wrapper. Participant
and source field validation stays with the current source validator. The effects
persisted/report union is `{id,target,key,operation:"clear"}` or
`{id,target,key,operation:"set"|"add-to-set"|"remove-from-set",value}`. New typed
effect input uses that union with optional `id` (stable or scoped Temp), allocated
as above; raw legacy effect ID requirements remain unchanged. `id` in source/report
is an `effect_` ID, target is a typed reference, key is an authored state key, and value
has that key's type. The wrapper accepts `Time` strings for event/transition time;
the raw existing event operation still accepts integer coordinates. A clear omits
value. No operation appends effects to an existing event.

## Candidate projection and transaction parity

Build one pure candidate world from immutable base records plus the complete
normalized operations. Reuse deterministic allocation, reference expansion,
canonical source serialization/validation, and the existing semantic folds.
Use a fresh candidate cache; never share a populated fold cache with the base.
Verification/semantic projection writes no Markdown, Git objects/refs, receipts,
SQLite cache, or current time. This is an end-to-end guarantee for new semantic
preview and consequence.batch compilation/preview, not just the projector:
load exact base source with `cache_write=False`, preserve the final in-memory
candidate, and never call compilation, require_database, recovery, parser-cache
publication, source/index/receipt writers, or reload HEAD as a substitute revision.
The public revision-pinned consequence read also loads with `cache_write=False`,
including deferred-compile worlds. Apply retains its existing transaction/recovery
boundary; preview cannot repair or recover a pending transaction. Existing
check-free legacy preview behavior outside the new path is unchanged.

Canonical request hashing uses the existing `canonical_json`: UTF-8 JSON, sorted
object keys, compact separators, `ensure_ascii=False`, array order preserved.
Hash the normalized complete changeset (excluding only legacy `requestId` as in
the existing writer). The semantic expectation operations remain in that payload,
hash, confirmation binding, and receipt; they produce no source record. The
authoring intent hash also retains the existing complete-intent binding for replay.
Candidate identity is exactly `{baseRevision: Revision, requestHash: Hash}`,
never a synthetic commit SHA or a claim that the base SHA contains candidate data.

Evaluate every request-local check against the complete validated candidate, after
all operations, in request order. Required non-pass blocks apply. A malformed
predicate invalidates preview. The evaluator creates no source record or automatic
repair; an advisory non-pass permits otherwise valid accompanying writes.
Preview and apply must agree on generated IDs,
candidate identity, diagnostics, checks, and confirmation. Recheck expected HEAD,
request hash, source freshness, scope, validation, and required expectations at apply.
The exact out-of-band preview token is mandatory for `consequence.batch`; retain
existing authorization, atomic single-commit source/index handling and receipts.
Same key and identical intent replays the existing receipt; same key with changed
operations/checks/horizon is an idempotency conflict. A blocked check writes
nothing, creates no receipt, and consumes no key. Raw existing writes stay valid
and do not acquire mandatory expectations by omission.

## Event-local inclusion and common folds

Let the canonical focus event E occur at T and the request horizon be H on the
same timeline, with T <= H. Use the validated, authorized world at the requested
revision. The event-local views both evaluate at **T**, with the same initial
state and all ordinary applicable evidence through T:

- `before`: exclude E's effects, and exclude knowledge, relationship, and plot
  transitions whose literal `causing_event` is E **and** whose point equals T.
- `after`: include those effects/transitions. Include other disjoint events at T
  and their transitions in both views. Uncaused transitions at T remain in both.

Perform exclusion on temporary projection records/input streams before running
the same folds; never change canon or subtract one from tick/order. This works
at the minimum signed coordinate and does not assume an event predecessor.
Two different events writing one state cell at the same coordinate are an
existing `WDL-STATE-020` source conflict; fail validation rather than pick an ID
winner. Within E, preserve effect array order, including multiple writes to the
same cell. State folds use initial state then set/clear/add/remove; set membership
and removal follow current fold behavior. Knowledge and relationship select their
latest applicable transition by numeric point then source ordinal. Knowledge
inspection includes forgotten/rejected states; ordinary omission of forgotten
knowledge from display is not evidence of an absent transition. Plot stored state
and derived eligibility/dependency/trigger values reuse `story_point_state` and
`evaluate_story_point`, including any upstream state/knowledge/relationship changes.
An event report does not activate an eligible story point.

Enumerate explicitly E-caused transitions separately through H, at their own
coordinates, marking `atEventTime` true only at T. Later transitions are absent
from both T views, even if E caused them. Never pull future transitions beyond H
into items, counts, citations, or diagnostics. Direct event `causes`, typed
`causing_event`, and outcome links remain distinct authored relations; no transitive
causal closure or inferred knowledge/kinship/plot result is introduced.

## Read and delta shapes

The exact request schema is:

```text
EventRequest = {
  protocol: "wedl-event-consequences/v1", revision: Revision,
  event: Ref, at: Time, limit: integer(1..1000),
  ?expectations: {policy: "required" | "advisory", items: Check[]}
}
DeltaRequest = {
  protocol: "wedl-event-consequence-delta/v1", at: Time, limit: integer(1..1000)
}
```

The event must be canonical and eligible through the explicit horizon. No cursor,
all-time option, scene fallback, or caller identity exists. A report with no
authored consequences is a complete `ok` with empty collections, not invented
consequences. It returns source-authored effect values and separately derived
before/after values, so a no-op effect remains visible in `effects`.

The following shapes freeze the full semantic response. `Reference` is exactly
`{id: Text, kind: Text, title: string}` with eligible safe source title. References
are labels plus provenance, never name guesses. `Nullable<T>` means T or JSON null.

```text
EventOk = {
  protocol: "wedl-event-consequences/v1", outcome: "ok", revision: Revision,
  event: Reference, eventTime: Time, at: Time,
  timeScope: {mode: "author-as-of", at: Time},
  effects: EffectEntry[], changes: Change[], causedTransitions: CausedTransition[],
  outcomes: OutcomeEntry[], causalSuccessors: CausalSuccessor[],
  currentAtHorizon: CurrentAtHorizon[], advisories: LinkAdvisory[],
  expectations: CheckResult[], applyAllowed: boolean
}
DeltaOk = {
  protocol: "wedl-event-consequence-delta/v1", outcome: "ok",
  baseRevision: Revision, candidate: {baseRevision: Revision, requestHash: Hash},
  at: Time, timeScope: {mode: "author-as-of", at: Time},
  focusEvents: Reference[], eventGroups: EventDeltaGroup[],
  unattributedChanges: Change[], unattributedRecordChanges: RecordDelta[],
  expectations: CheckResult[],
  applyAllowed: boolean
}
Failure = {
  protocol: "wedl-event-consequences/v1" | "wedl-event-consequence-delta/v1",
  outcome: "invalid" | "unavailable" | "limit", code: Text, message: string
}
Subject =
  {kind: "state", recordId: Text, key: Text} |
  {kind: "knowledge" | "relationship" | "story-point", recordId: Text} |
  {kind: "outcome", recordId: Text, targetId: Text, targetKind: "story-point" | "scene"}
Snapshot =
  {presence: "absent", payload: null, citations: Citation[]} |
  {presence: "present", payload: Payload, citations: Citation[]}
Change = {subject: Subject, before: Snapshot, after: Snapshot}
EventDeltaGroup = {event: Reference, eventTime: Time, changes: Change[], recordChanges: RecordDelta[]}
RecordDelta = {
  recordId: Text, kind: Text, change: "created" | "deleted" | "updated" | "noop",
  before: Nullable<Reference>, after: Nullable<Reference>, sections: SectionDelta[],
  operationIndexes: integer(0..)[], citations: Citation[]
}
SectionDelta = {section: Text, before: ObservedValue, after: ObservedValue}
ObservedValue = {presence: "absent"} | {presence: "present", value: Value}
EffectEntry = {effect: Effect, citation: Citation}
CausedTransition = {
  kind: "knowledge" | "relationship" | "story-point", record: Reference,
  transitionId: Nullable<Text>, sourceOrdinal: integer(0..), time: Time,
  atEventTime: boolean, transition: KnowledgeTransition | RelationshipTransition | PlotTransition,
  citation: Citation
}
OutcomeEntry = {event: Reference, target: Reference, reciprocal: boolean, citations: Citation[]}
CausalSuccessor = {event: Reference, time: Time, citation: Citation}
CurrentAtHorizon = {
  subject: Subject, at: Time, snapshot: Snapshot,
  supersession: "unchanged" | "superseded" | "unknown", supersedingCitations: Citation[]
}
LinkAdvisory = {
  code: "CONSEQUENCE-LINK-001", event: Reference, target: Reference, citations: Citation[]
}
Citation = {
  recordId: Text, sourcePath: Text, section: Text, sourceOrdinal: Nullable<integer(0..)>,
  memberId: Nullable<Text>, time: Nullable<Time>, provenance: Provenance
}
Provenance =
  {kind: "source", revision: Revision, blobOid: Revision} |
  {kind: "candidate", baseRevision: Revision, requestHash: Hash}
```

For event `EffectEntry`, `Effect` is the closed discovery union above with resolved
stable targets. `Payload` is selected by `Subject.kind`, never an untyped report:

| Subject kind | Exact present payload |
| --- | --- |
| state | `{value: Value}` |
| knowledge | `{state: Text, confidence: number|null, transitionId: Text, time: Time}` |
| relationship | `{status: Text, metrics: object<string,number>, facets: string[], transitionId: Text, time: Time}` |
| story-point | `{storedState: Text, derivedState: Text, eligible: boolean, dependenciesSatisfied: boolean, triggerSatisfied: boolean}` |
| outcome | `{eventLinked: boolean, targetLinked: boolean}` |

Knowledge state uses the six source states above. Plot stored states use the five
source states above; derived state additionally admits `eligible` and `blocked`.
Relationship status preserves authored text. Absent state and present null differ:
`{presence:"absent",payload:null,...}` versus
`{presence:"present",payload:{value:null},...}`. Absent knowledge/relationship
means no eligible applicable transition; it is not a negative fact about the world.
A scene outcome has `eventLinked:false` because events have no reciprocal scene
field; `targetLinked` says whether the scene names the event. A story-point outcome
uses both actual reciprocal fields. No title/note/body accompanies a transition
unless explicitly part of its admitted authored payload.

Source citations bind exact record/path/blob at revision. Candidate citations bind
baseRevision/requestHash for changed/new content; unchanged evidence may retain
its source citation. Never label modified content with the base blob. Paths are
repository-relative slash paths, no absolute repository/cache paths. Sections are
exact `initial_state.<key>`, `effects`, `transitions`, `lifecycle.initial_state`,
`lifecycle.transitions`, `trigger`, `dependencies`, `related_story_points`,
`outcome_events`, `causes`, `bodyMarkdown`, or `frontmatter.<field>` for an admitted
top-level record-delta field. Ordinals are zero-based source array positions, null for scalar
sections. Existing absent plot transition IDs remain null with ordinal citation;
never invent a historical ID. A cleared cell may be absent with a clear-effect
citation; an originally absent cell has no citation. Derived plot citations include
the admitted dependency/trigger evidence used by its fold.

`causalSuccessors` enumerates only admitted canonical events whose literal `causes`
contains the focus event, strictly after T and through H; each citation names the
successor's `causes` section/ordinal. It never traverses descendants or infers a
causal edge. `currentAtHorizon` contains one snapshot through H for each unique
state/knowledge/relationship/plot/outcome subject expressly affected by the focus
event's admitted effects, caused transitions, or links. This is separately labeled
from both T views and the historical `causedTransitions` collection. Supersession
is `superseded` when a later admitted writer/transition replaces the latest
focus-authored contribution for that subject; cite that latest superseding chain,
even if its net value is equal. It is `unchanged` when the focus contribution
remains current, and `unknown` when no admitted applicable contribution supports
a comparison. An E-caused accepted belief at T followed by another event's
rejection before H therefore remains a cited historical acceptance while its
current snapshot reports rejected plus the rejecting citation. A later E-caused
transition itself is occurrence history, not another event's supersession. A clear
is a cited absent state, not unknown. Exclude every future contribution beyond H
before snapshots, supersession, successor lists, diagnostics, and counts.
`advisories` reports only eligible explicit reciprocal story-point/event link
asymmetries with existing-half citations; scene single-half links are normal.
Empty lists assert recorded coverage only, never narrative completeness.

Delta is a **base/candidate comparison at one identical explicit H**, not the
event-local T comparison. Each subject's `before` folds the full base through H;
`after` folds the full candidate through H. Preserve authored operations whose net
effect at H is unchanged in ordinary preview diff; each event group's `changes`
and `unattributedChanges` contain only unequal semantic payload/presence. Compare
every eligible state cell, knowledge,
relationship, stored/derived plot state, and explicit outcome relation in the two
worlds, including indirect derived plot changes, without making them authored
consequences. Candidate focus is the eligible union of events directly created or
changed, explicit causes on changed transitions, supplied outcome events, and
expectation-check events. Include old and new explicit references for replacements
in raw preview, not a guessed causal ancestor. Filter that union through H/scope.
A preview supplies H through `consequenceRequest` to obtain a delta; it never
derives H from operation times. The delta is the optional `semanticDelta` field
in the existing preview response, containing exactly `DeltaOk | Failure`; the
existing preview files/diff/diagnostics/confirmation fields keep their versions.
Verification and preview both use this report grammar and expectation evaluator.
Invalid candidates retain ordinary structural/source diagnostics and emit semantic
`unavailable` (`CONSEQUENCE-UNAVAILABLE-001`) without running any semantic fold.

Group each changed semantic subject only by literal event effect ownership,
`causing_event`, or supplied outcome event links; event.create itself belongs to
its declared event. No-link state/knowledge/relationship/plot changes, initial-state
edits, static/prose edits, and indirectly derived plot changes lacking their own
literal attribution go in the unattributed collections. Old/new explicit event
references may place a changed subject in more than one event group; that is
reference coverage, not inferred cause. Never attach an unrelated edit to the
nearest event or to a batch's first event. Include one event group per admitted
focus event, even with no net semantic changes.

Record deltas keep source changes which semantic folds cannot show. Enumerate each
eligible operation-target record with its final created/deleted/updated/noop status:
created/deleted reflects base/candidate existence; updated means admitted content
differs; noop means targeted operations leave admitted content identical. Before
and after references are null only when the record is absent in that world.
`sections` contains differing admitted top-level source fields and/or the literal
`bodyMarkdown` section, represented as present values or absent leaves. Static
title/body/link/initial-state changes therefore never disappear. Include every
applicable operation index for that record in ascending order. Noop has empty
sections and still cites the targeted record; creation/deletion cites every
admitted differing section. Use source/candidate citations matching each side.
Temporal arrays are projected through H before section comparison, including
canonical string serialization of known time leaves. Filter restricted body
sections and record members before comparison; never return excluded field names,
hashes, excluded item counts, or future transition content. Preserve the ordinary
source diff as the authoritative full-operation diff under its existing policy.
Unattributed static/initial-state/prose changes remain unattributed even when the
same record also has explicitly attributed transitions; partition differing
sections by their literal link/effect ownership without duplicating attribution.
For a differing top-level array section, inspect only members changed between
the two admitted, horizon-sliced arrays. Assign the whole section to one event
only when every changed member has that same explicit owner. Multiple owners,
an old/new cause replacement, or any uncaused changed member place the complete
section once in `unattributedRecordChanges`. Unchanged historical members do not
add owners. Preserve the literal array; never manufacture per-event slices.
Semantic subjects may still cover both old/new explicit event groups separately.
Record deltas may accompany a semantic delta of the same subject: the former
describes authored content, the latter describes its fold at H.

Event-local `changes` also contains only unequal semantic payload/presence, evaluated
by the T inclusion rule. Inspect all eligible subjects for indirect derived plot
changes; static outcome links are enumerated separately in `outcomes`. An authored
effect/transition remains in its authored collection even when its net local
change is empty. Outcome `reciprocal` is true only for a story-point link with
both halves present; a scene link reports false and its single source citation.

Order focus events by numeric point then stable ID; effects by source ordinal;
caused transitions by numeric point, kind, record ID, source ordinal, member ID
(null first); outcomes by target kind then stable target ID. Order changes by
subject kind in `[state,knowledge,relationship,story-point,outcome]`, record ID,
key, target kind, target ID (inapplicable fields compare as empty strings).
Event groups use focus-event order; record deltas use kind then recordId; sections
use source field spelling with bodyMarkdown last. Current-at-H snapshots use the
same subject order as changes. Successors use numeric time then event ID;
advisories use target kind/ID. Noop records sort with other record deltas.
Citations deduplicate by full value and sort by record ID, section, ordinal
(null first), member ID (null first), time (null first), provenance kind, then
canonical JSON as a total tie breaker. Strings compare by Unicode code
point without locale/case normalization; times compare numerically. Check results
preserve request order. Authored array values, including facets and sets, preserve
fold order; report ordering never rewrites source.

## Request-local expectations

Checks are exact `{id: Text, predicate: Predicate}` objects with unique IDs per
items array. The seven predicate variants are:

| `kind` | Required members besides kind |
| --- | --- |
| `state.equals` | `target: Ref, key: Text, value: Value` |
| `state.absent` | `target: Ref, key: Text` |
| `knowledge.state` | `knowledge: Ref, state` (one of the six knowledge states) |
| `relationship.matches` | `relationship: Ref, values: { ?status: Text, ?metrics: object<string,number>, ?facets: string[] }` |
| `story-point.state` | `storyPoint: Ref, state` (one of the five stored plot states) |
| `outcome.linked` | `event: Ref, target: Ref` (target must resolve to story-point or scene) |
| `transition.caused` | `record: Ref, transitionId: Text, event: Ref` (record must resolve to knowledge, relationship, or story-point) |

`relationship.matches.values` must contain at least one member. Supplied status and
facets compare exactly; supplied metric entries compare by key and value (other
metrics need not match). This adds no thresholds or inferred inverse relation.
`state.equals` compares full typed values: boolean differs from number, null
differs from absence, strings do not coerce to IDs/numbers, object key order is
irrelevant and array order matters. Finite JSON numbers compare mathematically
without string coercion. Entity-typed values resolve only their `entity` reference
leaf. Validate the value against the world state-key declaration first.
`state.absent` passes only for an eligible record and a declared state key which
is absent after the fold. Missing target, unknown key, or inaccessible evidence
cannot prove absence. Knowledge states inspect the named record, including
forgotten, not display-list membership. Plot predicates compare stored state.
`outcome.linked` requires both actual story-point reciprocal halves, or the actual
scene outcome half; it does not assert that a plot was resolved. `transition.caused`
requires the named eligible transition at/before H to have that literal event
cause; an absent historical plot ID cannot be substituted with a made-up ID.

```text
CheckResult = {
  id: Text, policy: "required" | "advisory",
  outcome: "pass" | "fail" | "unknown" | "unsupported",
  code: "EXPECTATION-PASS" | "EXPECTATION-FAIL" | "EXPECTATION-UNKNOWN" | "EXPECTATION-UNSUPPORTED",
  comparison: ComparisonIdentity, actual: Actual,
  citations: Citation[]
}
ComparisonIdentity = {
  world: {kind: "source", revision: Revision} |
         {kind: "candidate", baseRevision: Revision, requestHash: Hash},
  at: Time, predicate: Predicate
}
Actual =
  {kind: "state", observed: ObservedValue} |
  {kind: "knowledge", state: Text, transitionId: Text, time: Time} |
  {kind: "relationship", values: { ?status: Text, ?metrics: object<string,ObservedValue>, ?facets: string[] }} |
  {kind: "story-point", state: Text} |
  {kind: "outcome", eventLinked: boolean, targetLinked: boolean} |
  {kind: "transition", causingEvent: ObservedValue, transitionId: Text, time: Time} |
  {kind: "redacted"}
```

Outcome and code must correspond. Pass/fail requires admitted evidence sufficient
to evaluate and expose the typed `actual` for that predicate. Relationship actuals
include only the requested comparison members/metrics, with absent metric leaves
distinct from present null (which is invalid for numeric metrics). State actual
uses explicit presence: absent has no value member; present null has value:null.
Transition actual includes its literal admitted cause or absent cause; it cannot
claim missing historical evidence is a negative world fact. Comparison predicate
retains the request's exact typed shape and resolves only permitted reference
leaves to stable IDs; never add a hidden title or target discovered from a failed
resolution. Unknown/unsupported results always have `actual:{kind:"redacted"}`
and empty citations. Their comparison identity retains the caller-supplied
predicate with only already admitted references resolved, the same world identity,
and the exact horizon, without disclosing a hidden resolution. Malformed input
has no fabricated check result. All actual/citation/comparison bytes count against
semantic bounds and receipts preserve them exactly.

Pass/fail requires admitted evidence sufficient
to evaluate the predicate. Missing/hidden/future target or applicable transition
is `unknown` with no secret-revealing detail. A well-formed predicate whose required
capability or comparison type is unsupported is `unsupported`; an unknown variant,
extra/missing field, bad reference kind/time/value shape is request `invalid`.
Unknown state keys are unsupported comparisons, not absent facts. A required
fail/unknown/unsupported makes `applyAllowed:false`; advisory non-pass does not.
With no checks, `expectations:[]` and `applyAllowed:true` on a valid report.
Standalone verification evaluates at request H; batch checks evaluate at their
own explicit `at`, on the candidate. Both require the named focus event through
that horizon. Invalid candidate validation independently blocks apply; a semantic
failure report cannot override ordinary preview `valid:false`.

### Check-only preview, apply, and replay

Whenever a changeset contains at least one `expectation.check`, its normal
`wedl-preview/v1` response contains the required additive `expectationChecks`
member, independently of the optional `semanticDelta` delta member. Each check
operation has 1..100 items; the complete request still has at most 100 checks.
The exact closed nested report union is:

```text
CheckReportOk = {
  outcome: "ok", baseRevision: Revision,
  candidate: {baseRevision: Revision, requestHash: Hash},
  groups: CheckGroup[], applyAllowed: boolean
}
CheckGroup = {
  operationIndex: integer(0..), event: Reference, at: Time,
  policy: "required" | "advisory", results: CheckResult[]
}
CheckReportFailure = {
  outcome: "invalid" | "unavailable" | "limit", code: Text, message: string
}
CheckReport = CheckReportOk | CheckReportFailure
```

`operationIndex` is the zero-based index in the complete operations array, not
the index among check operations. Groups follow operation order; each group's
results follow its items order. Candidate identity is the same normalized
changeset identity used by preview and any requested delta. Group event references
are resolved through their explicit `at` and authenticated scope. A named focus
event unavailable through that horizon yields `CheckReportFailure` unavailable;
individual predicate evidence gaps yield the defined unknown check result.
The failure code/message pairs are exactly the consequence failure table below;
failure reports have no groups, partial results, counts, or candidate identity.

Preview keeps all existing fields and deterministic confirmation-token generation.
With a valid candidate and complete check evaluation, `expectationChecks.outcome`
is `ok`, including required or advisory non-pass results. Its `applyAllowed` is
true exactly when all required results pass; advisory results never change that
gate. Required non-pass makes enclosing preview `valid:false` and appends an
error diagnostic `CONSEQUENCE-EXPECTATION-001`; advisory non-pass alone leaves
`valid:true`. A check-report invalid/unavailable/limit outcome also makes
`valid:false` with the matching consequence error diagnostic. Existing source
validation errors always keep preview invalid; they produce check-report invalid
`CONSEQUENCE-SOURCE-001`, since evaluation requires a validated candidate. Any
requested semanticDelta is unavailable without folding that invalid candidate.
The existing diagnostic shape is retained (code, message, severity, and existing
optional field/entity context); do not insert raw private predicate evidence.
When both checks and delta are requested, their check results must agree. The
delta's `applyAllowed` uses the same gate, and no delta success may accompany a
check-report failure. Independently triggered semantic limits likewise replace
all requested semantic reports with their closed limit failures.

Apply re-evaluates the same checks before writes and confirmation acceptance as
part of ordinary candidate validation. If preview is invalid for check reasons,
apply returns the existing `validation_failed` error envelope:
`{code:"validation_failed",message:"Consequence checks block apply.",details:{diagnostics:Diagnostic[],expectationChecks:CheckReport}}`.
This includes complete required non-pass results, or the closed report failure,
and no source/index/receipt write occurs. Authentication, HEAD, request grammar,
confirmation, and idempotency errors that precede semantic evaluation retain
their existing envelopes and need not include an unevaluated check report.

Successful apply retains the normal `wedl-command-result/v1` success members and
adds `expectationChecks:CheckReportOk`, with `applyAllowed:true`. This is required
even when no delta was requested; ordinary check-free requests omit it. Receipt
publication retains that exact bounded report in the existing result, alongside
the complete request/full-payload/authoring-intent hashes and original confirmation
token. Identical confirmed replay returns the stored report with
`idempotentReplay:true`, rather than reevaluating it against a later HEAD. Its
candidate remains bound to the original base revision/request hash. Changed check
policy/items/event/time, operations, or consequence request invalidates that retry.

A successful check-only request, or a check-bearing batch whose source operations
are all no-ops, has empty preview `files`/`diff`. Confirmed apply publishes the
bounded receipt without advancing HEAD, creating a source commit, changing
source/index files, or rebuilding the compiled cache. Its exact success variant is:

```text
CheckOnlyApplyResult = {
  protocol: "wedl-command-result/v1", status: "checked",
  previousHead: Revision, newHead: Revision,
  generatedIds: object<string,Text>, touchedEntityIds: Text[],
  compile: {status: "not-required", revision: Revision},
  idempotentReplay: boolean, expectationChecks: CheckReportOk
}
```

`previousHead`, `newHead`, and `compile.revision` all equal the checked base SHA;
`touchedEntityIds:[]` because there is no source diff. `generatedIds` preserves any
actual request allocation map (normally `{}` for pure checks), even if generic
create/delete operations cancel out in the final candidate.
The new checked variant applies only when check operations are present and the
validated candidate has no source diff. Legacy check-free generic no-ops retain
their existing writer behavior. Receipt publication uses the existing atomic
receipt/recovery discipline and expected-HEAD recheck, with no branch-ref update.
Checks themselves add no source record. Replay returns this stored result with
`idempotentReplay:true` and writes nothing, including when HEAD has since advanced;
`newHead` remains the originally checked SHA. A required non-pass or report failure
creates neither a source commit nor a receipt and does not consume the idempotency
key. Standalone verification remains a pure read without receipt publication.

## Bounds, failures, and transport parity

The hard ceilings are 100 eligible focus events per preview, 1000 total report
items, 100 total checks per request/preview (including all check operations), and
262144 bytes of canonical UTF-8 semantic output. Count items as effects + changes
+ causedTransitions + outcomes + causalSuccessors + currentAtHorizon + advisories
+ focusEvents + semantic changes + record deltas + section deltas + check results
across the complete
semantic report collection; repeated source citations do not count as items but
do count as bytes. Each request `limit` is an additional cap on this total, never
a page size. A preview's delta request supplies the same explicit `at` and
`limit`; checks at other explicit horizons still share global bounds. A wrapper
does not silently add per-focus full reports to the delta output.
Count eventGroups once each as focus events, not as additional items. A semantic
subject/record section present in multiple explicit groups counts once by its
full typed identity/value toward the item ceiling, while every serialized copy
counts toward the byte ceiling. Noop/created/deleted record markers each count as
a record delta. Current-at-H snapshots, successors, actuals and static record
sections are bounded with the same collection, never emitted as unbounded extras.
Without a delta request, `expectationChecks` uses the hard 100-focus/1000-item/
100-check/262144-byte ceilings with no inferred per-request limit or horizon;
each group retains its operation's explicit `at`. With both report fields present,
count each logical check once toward the check/item ceilings, but include both
serialized copies in the combined semantic byte ceiling. Check group metadata
does not count as another check; its resolved unique event contributes to the
focus-event ceiling. No check-only limit failure may return partial success.

Authorization/filtering precedes all counts. Enforce bounds before emitting
success and during collection/serialization; never truncate arrays, emit partial
success, infer from a partial graph, or disclose a hidden excluded count. A limit
failure has only `Failure`, no partial items/cursor/candidate/check results.

| Outcome | Code | Meaning |
| --- | --- | --- |
| invalid | `CONSEQUENCE-REQUEST-001` | Closed grammar, time, reference-kind, ambiguity, or expectation shape failure. |
| invalid | `CONSEQUENCE-SOURCE-001` | Complete selected source/candidate violates existing validation. |
| unavailable | `CONSEQUENCE-UNAVAILABLE-001` | Named target/revision/support cannot be admitted; generic safe detail. |
| limit | `CONSEQUENCE-LIMIT-001` | An authorized item/focus/check/byte bound would be exceeded. |

Failure messages are fixed respectively: `Invalid consequence request.`,
`Invalid consequence source.`, `Consequence unavailable.`, and
`Consequence limit exceeded.` Transport syntax/authentication, stale expected HEAD,
confirmation, idempotency, and transaction failures retain their existing WEDL
error envelopes outside these semantic outcomes. Invalid limit values are request
errors, not limit outcomes. Syntactically malformed revision is invalid; a valid
unavailable revision is unavailable. Required check non-pass is a complete `ok`
report with `applyAllowed:false`, not a claim of transaction success.

CLI and HTTP serialize identical semantic JSON, decimal times, ordering, bounds,
and codes. Semantic `ok` exits 0/HTTP 200; `invalid` exits 2/HTTP 400;
`unavailable` exits 2/HTTP 409; `limit` exits 2/HTTP 422. Preview still uses its
existing transport envelope and validity classification, embedding this semantic
outcome. Apply follows the existing mutation failure classification for blocked
required checks and preserves the report in safe structured details. Discovery,
CLI help, HTTP/OpenAPI, and the executable unions must describe the same actual
support; no advertised operation may dispatch to an absent implementation.

## Compatibility and literal examples

The following are schema examples, not commands claimed to exist already.
This complete bounded read names a revision, event, explicit horizon, limit, and
one advisory check:

```json
{
  "protocol": "wedl-event-consequences/v1",
  "revision": "0123456789abcdef0123456789abcdef01234567",
  "event": "Open the archive",
  "at": {"timeline": "main", "tick": "12", "order": "0"},
  "limit": 100,
  "expectations": {"policy": "advisory", "items": [
    {"id": "door-open", "predicate": {"kind": "state.equals", "target": "Archive door", "key": "condition", "value": "open"}}
  ]}
}
```

This complete wrapper creates one event and checks its candidate state. It
explicitly authors an effect; participants alone would change no state. It omits
`consequenceRequest`, so preview and successful apply carry `expectationChecks`
without a `semanticDelta` delta:

```json
{
  "action": "consequence.batch",
  "expectedHead": "0123456789abcdef0123456789abcdef01234567",
  "idempotencyKey": "archive-open-12",
  "summary": "Open the archive door",
  "operations": [
    {"type": "event.create", "temporaryId": "tmp:open", "title": "Open the archive",
     "time": {"timeline": "main", "tick": "12", "order": "0"},
     "effects": [{"id": "effect_00000000000000000000000001", "target": "Archive door", "key": "condition", "operation": "set", "value": "open"}]},
    {"type": "expectation.check", "event": "tmp:open",
     "at": {"timeline": "main", "tick": "12", "order": "0"},
     "policy": "required", "items": [
       {"id": "door-open", "predicate": {"kind": "state.equals", "target": "Archive door", "key": "condition", "value": "open"}}
     ]}
  ]
}
```

Its complete check-only report shape is illustrated below; the SHA/hash values
are schema examples, not a claim that these are the preceding request's actual
allocation or digest. In preview it appears as `expectationChecks`; successful
apply and its receipt retain the identical report:

```json
{
  "outcome": "ok", "baseRevision": "0123456789abcdef0123456789abcdef01234567",
  "candidate": {"baseRevision": "0123456789abcdef0123456789abcdef01234567", "requestHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
  "groups": [{
    "operationIndex": 1,
    "event": {"id": "event_00000000000000000000000001", "kind": "event", "title": "Open the archive"},
    "at": {"timeline": "main", "tick": "12", "order": "0"},
    "policy": "required",
    "results": [{"id": "door-open", "policy": "required", "outcome": "pass", "code": "EXPECTATION-PASS",
      "comparison": {"world": {"kind": "candidate", "baseRevision": "0123456789abcdef0123456789abcdef01234567", "requestHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}, "at": {"timeline": "main", "tick": "12", "order": "0"}, "predicate": {"kind": "state.equals", "target": "obj_00000000000000000000000001", "key": "condition", "value": "open"}},
      "actual": {"kind": "state", "observed": {"presence": "present", "value": "open"}},
      "citations": [{
      "recordId": "event_00000000000000000000000001", "sourcePath": "story/events/main/open-the-archive--event_00000000000000000000000001.md",
      "section": "effects", "sourceOrdinal": 0, "memberId": "effect_00000000000000000000000001",
      "time": {"timeline": "main", "tick": "12", "order": "0"},
      "provenance": {"kind": "candidate", "baseRevision": "0123456789abcdef0123456789abcdef01234567", "requestHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
    }]}]
  }],
  "applyAllowed": true
}
```

A check-only limit outcome is exactly this `expectationChecks` value; preview is
invalid and apply returns it inside the defined validation failure details:

```json
{"outcome":"limit","code":"CONSEQUENCE-LIMIT-001","message":"Consequence limit exceeded."}
```

The additive transition and reciprocal-link operations are literal closed variants:

This complete composite rescue also requests a semantic delta, uses omitted/scoped
auxiliary IDs, creates records before appending to them, and leaves an unrelated
prose edit unattributed. The example world is already homogeneous wedl/v0.6 and
declares the custody state key, trust metric, and named records/window used here:

```json
{
  "action": "consequence.batch",
  "expectedHead": "0123456789abcdef0123456789abcdef01234567",
  "idempotencyKey": "rescue-ledger-12",
  "summary": "Rescue the prisoner and recover the ledger",
  "consequenceRequest": {"protocol": "wedl-event-consequence-delta/v1", "at": {"timeline": "main", "tick": "14", "order": "0"}, "limit": 100},
  "operations": [
    {"type": "event.create", "temporaryId": "tmp:rescue", "title": "Ledger rescue", "time": {"timeline": "main", "tick": "12", "order": "0"}, "effects": [{"target": "Prisoner", "key": "custody", "operation": "set", "value": {"entity": "Mara"}}]},
    {"type": "knowledge.create", "temporaryId": "tmp:belief", "value": {"frontmatter": {"schema": "wedl/v0.6", "kind": "knowledge", "title": "Mara believes the ledger safe", "domain": "story.knowledge", "status": "canonical", "tags": [], "aliases": [], "knower": "Mara", "claim": {"key": "ledger-safe", "statement": "The ledger is safe."}, "transitions": []}, "bodyMarkdown": "An authored belief, not a guarantee of truth.\n"}},
    {"type": "knowledge.transition.append", "knowledge": "tmp:belief", "transition": {"id": "tmp:learned", "time": {"timeline": "main", "tick": "12", "order": "0"}, "state": "accepted", "causing_event": "tmp:rescue"}},
    {"type": "relationship.create", "temporaryId": "tmp:trust", "value": {"frontmatter": {"schema": "wedl/v0.6", "kind": "relationship", "title": "Mara trusts Oren after rescue", "domain": "story.relationship", "status": "canonical", "tags": [], "aliases": [], "from": "Mara", "to": "Oren", "relationship_kind": "trust", "transitions": []}, "bodyMarkdown": "An expressly directed relationship.\n"}},
    {"type": "relationship.transition.append", "relationship": "tmp:trust", "transition": {"time": {"timeline": "main", "tick": "12", "order": "0"}, "metrics": {"trust": 0.8}, "causing_event": "tmp:rescue"}},
    {"type": "story-point.transition.append", "storyPoint": "Recover the ledger", "transition": {"time": {"timeline": "main", "tick": "13", "order": "0"}, "state": "resolved", "causing_event": "tmp:rescue"}},
    {"type": "outcome.link", "event": "tmp:rescue", "storyPoints": ["Recover the ledger"], "scenes": ["Archive rescue"]},
    {"type": "entity.update", "entity": "Prisoner", "bodyMarkdown": "The prisoner wears a blue coat.\n"}
  ]
}
```

The corresponding semanticDelta must contain an explicit rescue eventGroup for
its custody/belief/directional relationship/plot/outcome changes, while the coat
prose section belongs to unattributedRecordChanges. A complete independent literal
delta illustrating the static-edit branch is:

```json
{
  "protocol": "wedl-event-consequence-delta/v1", "outcome": "ok",
  "baseRevision": "0123456789abcdef0123456789abcdef01234567",
  "candidate": {"baseRevision": "0123456789abcdef0123456789abcdef01234567", "requestHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
  "at": {"timeline": "main", "tick": "14", "order": "0"},
  "timeScope": {"mode": "author-as-of", "at": {"timeline": "main", "tick": "14", "order": "0"}},
  "focusEvents": [], "eventGroups": [], "unattributedChanges": [],
  "unattributedRecordChanges": [{
    "recordId": "char_00000000000000000000000001", "kind": "character", "change": "updated",
    "before": {"id": "char_00000000000000000000000001", "kind": "character", "title": "Prisoner"},
    "after": {"id": "char_00000000000000000000000001", "kind": "character", "title": "Prisoner"},
    "sections": [{"section": "bodyMarkdown", "before": {"presence": "present", "value": "A grey coat.\n"}, "after": {"presence": "present", "value": "A blue coat.\n"}}],
    "operationIndexes": [0],
    "citations": [
      {"recordId": "char_00000000000000000000000001", "sourcePath": "story/characters/prisoner.md", "section": "bodyMarkdown", "sourceOrdinal": null, "memberId": null, "time": null, "provenance": {"kind": "source", "revision": "0123456789abcdef0123456789abcdef01234567", "blobOid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}},
      {"recordId": "char_00000000000000000000000001", "sourcePath": "story/characters/prisoner.md", "section": "bodyMarkdown", "sourceOrdinal": null, "memberId": null, "time": null, "provenance": {"kind": "candidate", "baseRevision": "0123456789abcdef0123456789abcdef01234567", "requestHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}
    ]
  }],
  "expectations": [], "applyAllowed": true
}
```

```json
[
  {"type": "knowledge.transition.append", "knowledge": "Mara knows the route", "transition": {"id": "kt_00000000000000000000000001", "time": {"timeline": "main", "tick": "12", "order": "0"}, "state": "accepted", "causing_event": "Open the archive"}},
  {"type": "relationship.transition.append", "relationship": "Mara trusts Oren", "transition": {"id": "rt_00000000000000000000000001", "time": {"timeline": "main", "tick": "12", "order": "0"}, "relationship_status": "active", "metrics": {"trust": 0.8}, "facets": [], "causing_event": "Open the archive"}},
  {"type": "story-point.transition.append", "storyPoint": "Recover the ledger", "transition": {"time": {"timeline": "main", "tick": "13", "order": "0"}, "state": "resolved", "causing_event": "Open the archive"}},
  {"type": "outcome.link", "event": "Open the archive", "storyPoints": ["Recover the ledger"], "scenes": ["Archive entry"]}
]
```

For an event at `(main,12,0)`, those first two transitions enter event-local after;
the plot transition at `(main,13,0)` appears only as a later caused transition when
H includes 13. A disjoint event at `(main,12,0)` writing another cell stays in both
local views. At H `(main,14,0)`, if an already-authored later event closes the door,
base/candidate delta sees the door closed in both worlds and omits its unchanged
state cell, even though the new event-local view shows the door opening at 12.

A complete empty event success (the focus exists but has no authored consequences)
and a complete failure have no implicit fields:

```json
{
  "protocol": "wedl-event-consequences/v1", "outcome": "ok",
  "revision": "0123456789abcdef0123456789abcdef01234567",
  "event": {"id": "event_00000000000000000000000001", "kind": "event", "title": "Quiet arrival"},
  "eventTime": {"timeline": "main", "tick": "12", "order": "0"},
  "at": {"timeline": "main", "tick": "14", "order": "0"},
  "timeScope": {"mode": "author-as-of", "at": {"timeline": "main", "tick": "14", "order": "0"}},
  "effects": [], "changes": [], "causedTransitions": [], "outcomes": [],
  "causalSuccessors": [], "currentAtHorizon": [], "advisories": [],
  "expectations": [], "applyAllowed": true
}
```

```json
{"protocol":"wedl-event-consequences/v1","outcome":"limit","code":"CONSEQUENCE-LIMIT-001","message":"Consequence limit exceeded."}
```

Existing integer-time raw `event.create` and `entity.update` full-history replacement
remain valid; the new append operations provide safer additive authoring without
changing their compatibility behavior. Existing ordinary read protocols keep their
current time encoding. Legacy sources need no empty consequence records, capability
upgrade, or conversion to read authored effects/transitions. Only already-declared
genealogy support admits typed genealogy claims. Breaking discriminator/field/time
or inclusion changes require a new report version; adding executable operation
variants to discovery does not change `wedl-changeset/v1` or the source envelope.
