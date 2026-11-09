# Explicit semantic consequence checks

An optional `analysis` in a consequence plan compares the complete base and
candidate at **the same exact story horizon** and checks explicit expectations
against the candidate. It is a local-author read. It creates no effects,
learning, lifecycle transitions or approval.

```json
{
  "at": {"timeline":"main","tick":"12","order":"1"},
  "policy": "required",
  "expectations": [
    {"id":"reply-is-held","predicate":{
      "kind":"knowledge.state",
      "knowledgeId":"know_00000000000000000000000050",
      "state":"accepted"
    }}
  ]
}
```

Put this object in the plan's optional `analysis` field. Omit the field to request
only the existing exact source preview; `null` is invalid. An empty expectations
array requests the semantic delta without asserting a desired result. Timeline,
tick and order are explicit; coordinates are canonical decimal strings within
the native signed i64/i32 ranges. Unknown/duplicate fields and duplicate check
IDs are refused. References are exact IDs, never titles or aliases.

## Seven predicates

Every expectation is exactly `{id,predicate}`. The predicate contains `kind` and
the following fields:

| Kind | Fields | Meaning |
| --- | --- | --- |
| `state.equals` | `targetId`, `key`, `expectedValueRaw` | Current typed state equals the authored JSON literal. |
| `state.absent` | `targetId`, `key` | A declared state key is absent on an available owner. |
| `knowledge.state` | `knowledgeId`, `state` | Latest authored belief state, including forgotten, at the horizon. |
| `relationship.matches` | `relationshipId`, `values` | Selected current status, metrics and/or facets match. |
| `story-point.state` | `storyPointId`, `state` | Stored lifecycle state matches, independently of derived eligibility. |
| `outcome.linked` | `eventId`, `targetId` | Explicit scene/event or reciprocal story-point/event outcome linkage exists. |
| `transition.caused` | `recordId`, `transitionId`, `eventId` | A selected knowledge, relationship or story-point transition explicitly names this cause. |

Knowledge states are accepted, suspected, rejected, uncertain, remembered and
forgotten. Lifecycle states are dormant, active, resolved, failed and cancelled.
Checking a belief state does not determine whether its claim is true. Checking a
cause inspects the retained pointer; it does not infer causality. A transition
is unavailable before its own time, even when its causing event has happened.
Duplicate matching transition IDs refuse the selected cause check, including
duplicates whose second occurrence is in the future.

`expectedValueRaw` is a **string containing one JSON literal**, for example
`"9007199254740993"` for an integer, or `"\"ready\""` for a string. It preserves
numeric tokens through browser import/export. The literal is checked against
the selected native state definition, including bound lore profiles. Equality
uses that definition: sets compare by membership; arrays inside structured
profiles remain ordered; booleans, strings, integers and floating values do not
coerce into one another. Absent and explicitly present null remain different.
Numeric tokens must round-trip through the native representation without changing
their decimal value; a precision-losing operand is refused. The existing native
finite-number semantics still apply; this adds no arbitrary-precision state
arithmetic.

Relationship `values` selects one or more of `status`, `metrics` and `facets`:

```json
{"status":"active","metrics":{"trust":"0.75"},"facets":["colleague"]}
```

Metric operands are decimal JSON-number tokens **inside strings**. Comparison
normalizes their decimal values without rounding integer operands through a
browser number. Selected declarations use native `min`, `max` and optional
`default` fields with finite numeric values and ordered bounds. Unsupported or
malformed definitions are reported explicitly. A default never manufactures a
missing current metric. Facets compare in their authored array order.

## Results and application policy

Each result is `pass`, `fail`, `unknown` or `unsupported`, with its predicate,
observed value and applicable source citations. Missing or temporally excluded
targets cannot prove absence. A missing declared state value can satisfy
`state.absent`; a missing owner or undeclared key cannot. Wrong existing record
kinds and malformed or incompatible operands are request errors.

`expectationsSatisfied` means every selected expectation passed. `applyAllowed`
additionally respects policy: required non-passes block application; advisory
results remain informational. Neither field supplies human approval. Empty
checks pass vacuously and still require a separate outcome approval.

The complete analysis is retained in immutable proposal history and covered by
plan, preview and outcome report hashes. Changing its horizon, policy or operands
requires a new proposal/review. Outcome application recomputes the complete
report under the shared writer lock and requires valid candidate source plus
passing required expectations. Invalid candidates retain source diagnostics but
are not semantically folded. Standalone previews keep `canApply:false`.

## Same-horizon delta

The report compares typed state, current knowledge, directed relationships,
story-point lifecycle/eligibility and explicit outcome links at H against those
same families at H in the candidate. It retains before/after values and separate
source citations in deterministic subject order. It does not compare H with H+1
or attribute a change to a particular event.

Per-family admission follows the established native reads: event writers,
knowledge and story-points use canonical status; state owners and relationship
histories retain their existing native status behavior. Timed owners become
available at their authored start; this is not an active-presence query. Latest
transitions use exact inclusive time and native tie ordering. Forgotten beliefs
remain represented. Character/reader admission is a separate contract.

Source hash, path, title and note changes alone are not semantic changes.
Story-point transition IDs and times are provenance when stored lifecycle state
and derived eligibility remain unchanged; current knowledge and relationship
occurrences retain their native transition identity and time. Replacing
an earlier effect that is overwritten before H can therefore leave the semantic
delta empty while the exact source diff remains nonempty. Derived eligibility
changes stay visible without inventing a cause. A complete history with no
transition at/before H has no current belief/relationship. Other unavailable
boundaries are marked explicitly and never presented as a fabricated deletion.

## Temporal entity operands

The temporal rule applies only to event effects on **unbound scalar `entity`
state keys**. When the effect's `{entity: ID}` operand refers to a record outside
existing native semantic admission at H, the complete effect is skipped before
replay. An earlier admitted contribution remains effective, including an earlier
clear. Retained effects keep their original source ordinal and ID.

Endpoint admission uses the same native family rules as semantic owners: events
must be canonical and at/before H; scenes, conversations and environments become
available at their inclusive authored start and remain available after their end;
knowledge and relationships need an effective history member, with canonical
status additionally required for knowledge; story-points must be canonical.
Different timelines do not satisfy these time checks. Other source kinds retain
their existing local-author availability; no new validity or narrative-scope gate
is inferred for them.

Initial values, entity sets and set-member operations, bound profiles, and
literal strings retain their existing native behavior. This includes initial
values and nested typed profile values that mention future records. Expected
operands remain exact typed literals: comparison with an absent value fails, and
known absence can satisfy `state.absent`. This change adds no new unknown state
or predicate outcome. Independent owner-unavailability rules still apply.

Semantic story-point eligibility uses the same filtered replay for projected
state owners, so a skipped scalar effect cannot affect derived eligibility.
Ordinary runtime/story queries keep their existing fold. The original source,
raw operands and profile projections are never rewritten.

Revision and event comparisons report `futureEntityTargetPruning: true` together
with `stateReferences: native-values-with-scalar-entity-effect-pruning` to identify
this narrow scope. The separate authored event-effect list remains exact source
evidence, distinct from an effect's semantic contribution. This establishes no
character knowledge or reader entitlement.

The pinned reference is
[`event_consequences.py::Projection._filter_links`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/event_consequences.py):
its scalar entity effect check runs before replay and retains original ordinals.
Its [`semantics.py::resolve_state`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/semantics.py)
retains initial values and ordinary set operations. The separate nested lore
projection is not substituted for semantic replay.

## Check current source and an applied receipt

`consequences check --file request.json` takes exactly
`{expectedFingerprint,analysis,maxCharacters}`. It evaluates the selected captured
source and can read a valid historical Git capture. GET
`/api/consequences/check?request=...` accepts that same closed JSON request within
the existing URL bound. Both return complete results and perform no writes.

The Actions browser can select analysis for an unsaved plan, import/export exact
operands, check current source and inspect candidate analysis in saved previews
and outcome reviews. Required failures disable approval. Input/selection changes
and navigation discard prior results and pending responses. Request downloads
support checks too large for a URL; native validation remains authoritative.

For an applied proposal with retained analysis, `action-drafts verify-outcome`
also returns `semanticVerification` on the **current** valid source at the saved
analysis horizon. Exact source/file matching still determines its existing
`verified` field. Later unrelated source drift can leave the semantic checks
passing; later state changes can make them fail. Unsupported selected checks
remain explicit. Schema or horizon errors that prevent evaluation report the
semantic analysis as unavailable. Verification
never rewrites the terminal receipt or its original approved history.

## Complete bounds and remaining scope

Analysis allows at most 100 expectations and 256 KiB encoded input, still inside
the complete consequence plan/private document bounds. State literal operands
are at most 64 KiB. Standalone requests are at most 512 KiB; reports must fit
256–1,000,000 requested Unicode scalar values and 8 MiB. HTTP queries remain
limited to 16 KiB encoded. No response is silently truncated.

Projection additionally bounds source records (4,096), raw source (32 MiB), YAML
nodes (100,000), nesting (64), estimated fold work (2,000,000), semantic subjects
(1,024 across the comparison) and serialized projection/delta bytes (8 MiB).
Exhaustion refuses the complete analysis. These limits do not establish a global
bound for all other application loading/validation paths.

[Event-local exclusion and event-grouped source changes](EVENT_CONSEQUENCES.md)
now use this native projection and candidate engine. Exact event time, separate
selected horizon, delayed history and mixed source attribution remain explicit.
Unbound scalar entity effects now use the narrow temporal admission described
above; broader semantic domains remain outside this comparison. [Typed retcon impact](RETCON_IMPACT.md)
and [saved reviews](RETCON_REVIEWS.md) track source dependencies and admitted-input
currentness independently of semantic expectation results. Retained general edits
and historical private reads are implemented in [EDITS.md](EDITS.md) and
[ACTIONS.md](ACTIONS.md); [REVISION_REVIEW.md](REVISION_REVIEW.md) specifies the
separate three-original comparison. Historical citation arrays remain authored
syntax and do not resolve or admit evidence. The [proposal](../examples/actions/PROPOSALS.md) and
[outcome](../examples/actions/OUTCOMES.md) walkthroughs exercise this implemented
slice. See [CONSEQUENCES.md](CONSEQUENCES.md), [OUTCOMES.md](OUTCOMES.md) and
[PARITY.md](PARITY.md) for the surrounding contracts and remaining plan.
