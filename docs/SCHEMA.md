# Current source rules — v1

Every canonical record begins with YAML frontmatter containing `schema: wedl/v1`,
`kind`, `id` and `title`. A world has one world record under `story/`.
Canonical records accept only this schema, including explicit Git revision reads. Fixtures
are edited directly; no old-format reader or conversion command exists. The
schema label stays `wedl/v1` until the plan is complete, including when these
rules change incompatibly. V1 remains a work in progress.

Bodies remain opaque Markdown. The LF/CRLF envelope and original body bytes are
retained. Existing ID, timeline and record structures continue unless specified
below. See the example worlds for complete records.

Hypothesis records share the `story/` source tree and fingerprint but remain
noncanonical possibilities. They require nonblank `statement` and `context`,
nonempty `subjects` and `alternatives`, and a `placement` map. Their closed
vocabulary excludes state, effects, chronology, descriptions, thread membership
and operational audience/perspective fields. Status is `open`, `adopted` or
`rejected`; adoption references existing admissible records, while rejection
requires a note. Neither changes truth or grants knowledge. Exact nullable raw
source fields, resolution rules and the narrower explicit request grammar are
specified in [HYPOTHESES.md](HYPOTHESES.md).

Typed record requests now cover all twenty-eight non-world kinds and established
world declarations. Whole maps/lists replace or clear their authored field;
omission preserves it. Literal object affordances retain duplicates, order and
empty strings. Environment conditions preserve scalar types and nulls;
relationship transitions retain complete snapshots without synthesized inverse
updates or metric defaults. See [AUTHORING.md](AUTHORING.md) for each closed edit
vocabulary and complete candidate validation.

Private action reasoning uses a separate closed JSON document under
`author/actions/`, specified in [ACTIONS.md](ACTIONS.md). That tree is durable
author source but never a canonical record, search corpus, character evidence
source or contributor to the canonical fingerprint. No new source schema or
version label is introduced. Required `proposals` retain bounded immutable human
history, exact answer snapshots and explicit typed plans. Both drafts and each
proposal require `evidence` arrays using the existing closed historical citation
grammar; the pointers are inert syntax rather than authenticated evidence. Effective context
staleness is read metadata; it never rewrites the original base or history.
[Consequence previews](CONSEQUENCES.md) construct and validate complete candidate
records in memory. Neither private history nor a valid preview publishes them.

General retained edit buffers use separate closed JSON under `author/edits/`,
specified in [EDITS.md](EDITS.md). Opaque patch/body strings are not canonical
source until a complete saved candidate is explicitly reviewed and approved.
Historical private reads require the same current v1 grammar at the selected
commit. [Revision review](REVISION_REVIEW.md) compares independently admitted
original worlds without a merged schema, migration or source-format adapter.

## Private manuscript source

Private Markdown uses fixed `schema: wedl-author/v1`, separate from canonical
`wedl/v1`. Every envelope has exact `kind`, `id` and `title`; unknown or duplicate
fields fail. IDs have a 26-character uppercase Crockford suffix and must match
the filename and kind. See [MANUSCRIPT.md](MANUSCRIPT.md) and the executable
[manuscript walkthrough](../examples/manuscripts/README.md).

| Kind | Exact path | Required kind-specific fields | Optional fields |
| --- | --- | --- | --- |
| `manuscript-segment` | `author/segments/segment_<suffix>.md` | `source_links` | `placement`, `pov_character_id` |
| `work` | `author/works/work_<suffix>.md` | `chapters`, `duplicate_policy` | None |
| `reveal-plan` | `author/reveals/reveal_<suffix>.md` | `work`, `entries` | None |

Segment links are exact typed `record`, `section` or `transition` references to
canonical source. Placement is an explicit `{timeline,tick,order}` with native
integer coordinates; POV is an exact character ID. Each work chapter has exactly
`id`, `title` and a nonempty `segments` array of `{kind: author-record,id}`
references. Chapter IDs are unique within a work. `duplicate_policy` is `reject`
or `allow-explicit-reuse`; an empty work is valid. Chapter/segment array order
defines occurrences independently of placement.

Reveal plans require `work: {kind: author-record, id: work_<suffix>}`. Each entry
has exactly `milestone: {chapter_id, segment_index}` and `admissions`. Milestones
must resolve actual chapter-local occurrences and increase strictly in authored
work order; an index is an integer from 0 through 1,023. Admission shapes are:

| Kind | Exact source fields |
| --- | --- |
| `record` | `{kind: record, target: {kind: record, id}}` |
| `section` | `{kind: section, target: {kind: section, owner_id, id}}` |
| `text` | `{kind: text, id: revealtext_<suffix>, markdown}` |

Record/section targets are exact canonical references, including original
description members subsequently replaced. Admission identity is globally unique
by kind, owner when present and ID. Literal text remains exact, including an empty
string. Empty entry/admission arrays are valid; each has at most 1,024 elements
under the existing full source budgets. An empty plan may select an empty work.
The plan body is private notes. See [REVEAL_PLANS.md](REVEAL_PLANS.md).

Raw UTF-8 source, LF/CRLF envelopes and body whitespace remain exact. Manuscript
fingerprints cover these three private directories' paths and original bytes;
canonical counts, fingerprints, retrieval and character evidence stay separate.
Save requests pin current canonical source, the complete manuscript snapshot and
the exact prior file hash, and validate the complete candidate's references.
Historical resolution pairs canonical and private originals from one full commit
OID. Listing/detail inspection validates private structure without requiring
canonical reference validity. All complete source/expanded-YAML/result limits are
specified in [MANUSCRIPT.md](MANUSCRIPT.md).

Private source compilation supplies no reader authority. Segment/work retcon
analysis remains separate: reveal plans add no retcon dependency edges or semantic
witness entries. Writing can bind an exact private work and its source fingerprint
without importing private prose or granting reader disclosure; its frame contract
is specified below.

## Reader presentation and trusted policy

Reader projection requires an original committed canonical/private source pair,
independently supplied ordinary scope and an exact occurrence. It returns only
minimal record, section or text units, as specified in [REVEALS.md](REVEALS.md).
Private source resolution also supports coherent current author captures; that
native author option is not a reader edition. Character lenses continue to exclude
raw body prose, even when a section audience includes the character.

The separate public reader wire protocol is `wedl-reader-presentation/v1`. A page
request has exactly `protocol`, `publicationId`, `revision`, `occurrence`, `limit`
and required nullable `cursor`; search additionally requires `query`. The author
simulation request instead has `protocol`, `revision`, `workId`, `planId`,
`operation`, `at`, `textPolicy`, `occurrence`, `limit`, required nullable `cursor`,
and `query` only for search. `at` is exactly `{timeline,tick,order}` with canonical
signed decimal strings for i64 ticks and i32 order. Original revisions are full
lowercase 40- or 64-character commit OIDs, occurrences are 0–1,023 and limits are
1–100. Native cursors are opaque authenticated 72-character lowercase hexadecimal
strings. Request JSON is closed, duplicate-safe and bounded to 16 KiB.

Trusted operator files have schema `wedl-reader-policy/v1`, a 1 MiB limit and
closed required fields. They are loaded explicitly by CLI/server startup; reader
requests cannot construct a policy or provide its secret.

| Policy | Required fields |
| --- | --- |
| Shared | `schema`, `kind`, `revision`, `workId`, `planId`, `canonicalScope`, `generation`, `maxOccurrence`, `cursorSecret` |
| `kind: public` | Shared fields plus `publicationId`, `at`, `textPolicy` |
| `kind: author-simulation` | Shared fields plus `principalId`, `authorPermissions` |

`canonicalScope` and `authorPermissions` are exactly
`{worldId,recordIds,sections}`. Record IDs and section paths are unique; `sections`
must be explicitly null or an owner-to-path-array map. A null map permits fields
on selected records; a present map grants only listed paths, with parent paths
permitting descendants. Simulation intersects both scopes. `maxOccurrence` is
0–1,023; `cursorSecret` encodes exactly 32 bytes as 64 lowercase hexadecimal
characters. `textPolicy` is `none` or `all-at-pinned-plan`. Repository identity is
captured from the explicitly selected Git root, not supplied in configuration.
Native callers can replace/revoke installations; emitted results recheck that
installation. No HTTP installation or configuration discovery exists.

The original-source, complete-result, browser and route boundaries are in
[READER.md](READER.md), with an executable
[reader walkthrough](../examples/readers/README.md).

## Typed state

Declare fields in the world record:

```yaml
state_keys:
  character:
    condition: {type: string}
    ready: {type: boolean}
    experience: {type: integer}
    temperature: {type: number}
    location: {type: entity, kind: location, exclusive_group: placement}
    equipment: {type: entity-set, kind: object}
    labels: {type: string-set}
```

| Type | Source value |
| --- | --- |
| `string` | YAML string |
| `boolean` | YAML boolean |
| `integer` | Exact signed i64 integer, not a decimal, boolean or numeric string |
| `number` | Finite YAML number |
| `entity` | Exactly `{entity: RECORD_ID}` referencing an existing record |
| `string-set` | Array of unique strings |
| `entity-set` | Array of unique entity references |

Declarations contain only `type`, optional `kind` for entity types, and optional
`exclusive_group`. Old `entity_kind` and `exclusive` fields are rejected. Two
present fields in the same exclusive group are invalid, even if one contains
an empty value. A missing key differs from false, an empty string, or an empty
set. Set equality ignores array order; authored array order is retained.

`set` replaces the whole declared value. `clear` removes a key and forbids a
`value` member. `add-to-set`/`remove-from-set` accept one correctly typed item
and are valid only for set fields. Boolean/integer/decimal values are not
coerced into each other. JSON responses emit `integer` state as decimal strings;
`number` state remains a JSON number. Conditions compare raw typed values.

An explicit world `state_profile_bindings` mapping may replace an existing
state declaration's leaf contract with an object-root lore profile. The ordinary
declaration must remain valid and its `exclusive_group` remains in force.
Bound values accept only complete `set` and value-free `clear` effects; set-item
operations are refused. Their integers use the lore decimal-string codec.
Ordinary author state/context omit bound keys and their citations; the separate
exact-horizon `lore state` query projects those values through lore visibility.

## Structured lore

The explicit `lore-core-v1` capability enables world `profiles` and
`state_profile_bindings`, `lore-concept` records and common `concept_links`.
Each profile contains exactly `id`, `label`
and an object-root `fields` schema; a concept's `profile` is that exact ID.
Definitions are edited directly, with all dependent values revalidated in the
complete candidate world.

The eight closed profile types are `string`, `boolean`, `number`, `null`,
`entity`, `enum`, `array` and `object`. Objects declare disjoint `required` and
`optional` maps and reject undeclared values. Missing fields remain absent;
null requires an explicit null type or enum alternative. YAML uses
`type: 'null'` for the type name. Arrays require an item descriptor and exact
`max_items` in 0–1024. No defaults or coercions are applied. Bounded data has
at most 16 levels, 16,384 expanded nodes including keys, 256 keys per map and
1024 items per array. The whole world profile registry shares those budgets.

Entity values contain exactly `{kind: record, id: ID}` and must name an existing
record of an allowed actual kind; draft and retired targets still validate.
Only declared entity leaves create references. ID-shaped strings remain literal.
Enums distinguish scalar categories, while numeric `1`/`1.0` and `0`/`-0.0`
compare equal. Large source integers remain exact and use decimal strings in
JSON. Concept `valueTypes` describes only returned values, with `scalarType` for
enum leaves. Explicit profile queries return full schemas with `enumValueTypes`
to distinguish numeric alternatives from literal strings in JSON.

Concepts require a dotted `domain`, `profile`, typed `values`, `valid`, and
sorted nonempty unique `audience` and `perspectives` label arrays. Status is
canonical, draft or retired; only canonical concepts enter lore queries.
Validity is closed `{kind: timeless}` or an interval on a declared timeline
with optional inclusive start/end points. Every supplied point contains exact
timeline, i64 tick and i32 order. Link members contain exactly `relation`, an
existing `concept_id` and `valid`; their authored order is retained.

Author queries filter concept status, labels and time before lookup, ambiguity,
counts, references and limits. Without a horizon, intervals across all timelines
participate. Ordinary non-concept targets and link owners retain author-source
visibility regardless of status or time, except for separately gated existence
episodes and context briefs; hypotheses are excluded. Briefs require canonical
status, their own labels and applicable validity. Episodes additionally require
their character endpoint to be admitted. Hidden concept
entity leaves are omitted from projected values, while literal IDs remain.
Links confer no traits or character evidence. Separate character lore readers
derive known records from scoped acquisitions and prune typed endpoints before
projection; links do not acquire their targets. Generic character context/search
admits dated sections and learned assertion words without automatically importing
typed canonical lore values. See [NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md).

State bindings have the shape
`state_profile_bindings: {character: {species_notes: character-description-state}}`.
They are world-only, map existing state keys to exact current profile IDs, and
require capability opt-in even when explicitly empty. Known non-world kinds
except hypotheses may have bindings. Every initial value and every event's
effect value validates against its bound profile, including past, future and
draft events. Canonical events alone replay; clear leaves the key absent.
Profile, declaration and binding edits validate all these dependents before
publication. Typed refs are extracted only from declared entity leaves; literal
`chronology` fields inside values remain data. Bound state does not infer
movement, holders, containers, species membership or character knowledge.

The optional author [common/individual description reader](SPECIES.md) uses the
same source, without a new kind or capability. Callers explicitly select profiles,
root typed link fields and independent display fields. Common statements and
individual variation stay separate; cycles and exhausted bounds refuse a partial
composition. See [LORE.md](LORE.md) for profiles, bindings and query projection.
The [magic reader](MAGIC.md) similarly assigns explicit tradition, practice,
realm and artifact roles over current profiles and concepts. Restrictions and
exceptions stay separate; only selected typed fields create traversal links.
This adds no new source kind or capability. The [social](SOCIAL.md) and
[environmental](ENVIRONMENT.md) author readers likewise compose current profile
records, explicit assertions and dated sections. Their fixed role profile IDs
are conventions supplied by the two JSON packs under `examples/profiles/`, not
new record kinds. They require the existing lore, description and assertion
capabilities. Shared character lore/section admission and linked learned
assertion reads are implemented separately. Character species/social/environment
composition uses independently admitted concepts; character magic selects only
explicit learned accounts. Belief targets never classify reports through current
canonical subjects or predicates. Species/magic author reads attach native
sections/assertions for composed owners when those capabilities are declared.
Optional family selections use the exact writing frame and its scoped scene
grants. Reviewed explicit outcome publication is specified in
[OUTCOMES.md](OUTCOMES.md). Explicit expectation checks and same-horizon deltas
are specified in [SEMANTIC_CONSEQUENCES.md](SEMANTIC_CONSEQUENCES.md).
Event-local exclusion and grouped causal attribution are specified in
[EVENT_CONSEQUENCES.md](EVENT_CONSEQUENCES.md); original revision analysis and
typed private dependencies are specified in [REVISION_REVIEW.md](REVISION_REVIEW.md)
and [RETCON_IMPACT.md](RETCON_IMPACT.md). Private manuscript/reveal assembly and
minimal reader disclosure use the source and policy models above. Work-bound
Writing selections use those same private sources; structured reader domain
composition remains unfinished.

## Dated narrative source

An optional root `descriptions` array stores explicit Markdown sections
independently of the record body. It requires `descriptions-v1` and is allowed
on all known canonical kinds, including world and assertion, but not hypotheses.
Members have local IDs, exact validity, separate audience/perspective labels and
optional earlier same-owner corrections. Corrections preserve exact validity and
labels; dated reads suppress admitted predecessors while all-time reads retain
history. Unrelated overlapping accounts remain separate. See
[DESCRIPTIONS.md](DESCRIPTIONS.md) for the closed grammar and bounds.

The world may declare current `predicates` under `lore-core-v1`. Each has an
exact token ID, allowed subject kinds and any finite `value_type`. The
`assertion` kind additionally requires `assertions-v1` and stores a typed subject,
predicate value, `author-fact`/`attributed-claim`/`unresolved` classification,
validity, scopes and historical evidence syntax. Attribution is required exactly
for attributed claims. Replacements preserve subject, predicate and validity,
remain acyclic and do not select a winning branch. See
[ASSERTIONS.md](ASSERTIONS.md) for source and projection rules.

## Author navigation

[Navigation](NAVIGATION.md) composes dated author reads from existing source.
Timeline points include every authored event status; only canonical events
contribute state, interactions and causal edges. Spans preserve inclusive exact
coordinates without inventing elapsed duration. Story-point `when` also accepts
`{scene: {location: LOCATION_ID, participant: CHARACTER_ID, object: OBJECT_ID}}`;
each predicate field is optional and all supplied fields must match the explicitly
selected admitted scene. Timed participant presence is respected. No selected
scene means the scene condition is unsatisfied. Scene selection never restricts
the story-point candidate list and never creates lifecycle changes.

## Selected author retrieval

An explicit author search horizon selects a request-local prose corpus without
changing source. Ordinary record titles remain timeless identity; body prose
uses authored event times, completed scene/conversation ends, inclusive
environment intervals or the earliest timed transition. Applicable description
sections contribute separately cited heading/Markdown candidates after owner and
member scope checks and correction suppression. Excluded text contributes no
retrieval statistics, training material or input-budget bytes.

The dated generic corpus excludes raw typed lore, assertion, generational and
existence payloads, assertion-linked knowledge and required context briefs.
Applicable dated descriptions on typed owners retain their independent admission;
this does not supply complete structured typed-search parity. Hypotheses and
private manuscript/action/edit source remain outside the corpus. Omitting author
horizon and thread selections preserves the established all-time search behavior.

Optional thread grouping selects 1–32 sorted unique exact IDs declared in the
world's `threads`. Membership is ANY across selected IDs, applied after complete
ranking and before the output limit, without changing training, scores or lane
ranks. Thread declarations and memberships retain their existing source grammar;
they confer no character knowledge or reader access. Explicit selected searches
use no persistent FTS cache or model. See [RETRIEVAL.md](RETRIEVAL.md) for request
shapes, source exclusions and complete-input bounds.

## Authored context briefs and writing selections

`context-brief` requires `briefs-v1` without implicitly enabling lore. Its ID
uses `brief_` plus 26 Crockford characters. It stores a dotted domain, canonical/
draft/retired status, explicit `valid`, nonempty sorted audience/perspective
labels, scope and an ordered `required_refs` array. Body prose is bounded to
1 MiB. Rust common chronology, descriptions, concept links and preserved opaque
metadata retain their normal source contracts.

Scope is `world`, `region` or `era`. World forbids `scope_ref`; region requires
a location/overlay record reference; era requires the exact world's chronology
era section. References are exactly `{kind: record, id}` or
`{kind: section|transition, owner_id, id}`. Sections resolve descriptions or world
chronology members; transitions resolve existing ordinary or generational
members. Titles, body headings and approximate names never resolve pins.
`required_refs: []` is valid; omission, null, duplicate exact references and
nonexistent/wrong-owner members are invalid. Source retains at most 1024 pins
per brief. See [BRIEFS.md](BRIEFS.md).

Composition admits the whole required closure before complete-result bounds.
Nested brief cycles terminate through exact reference deduplication, preserving
authored traversal order. One unavailable required item or scope reference gives
a closed unknown result with no partial content. A selected brief grants no
knowledge. Character assertion pins use independently learned words, ordinary
record pins exclude untimed bodies, and brief-owned descriptions retain their
owner's canonical status, scope and applicability.

A writing frame is a separate closed operational JSON object with six required
fields: `{fingerprint, work, sceneId, povId, at: {timeline, tick, order}, lens}`.
Its fingerprint must match the exact captured canonical source. `work` is null or
exactly `{workId, authorFingerprint}`, binding one exact work ID and the complete
private manuscript source hash. Both hashes are 64 lowercase hexadecimal
characters. Canonical and private snapshots must have the same revision;
historical capture uses originals from the same full commit, and current capture
rechecks both sources before returning the complete result. The selected work's
complete segment closure validates even when a segment's placement is later than
the frame horizon.

Scene and POV are exact IDs or null. Time coordinates are required canonical
decimal strings. The lens is author, character or dramatic-irony; non-author
lenses require a POV. Context additionally requires an explicit scene and POV for
every lens. The selected scene's scope and actual presence gate character scene
grants. Work selection supplies no prose, scene, POV, time, learning or reader
disclosure, and the frame has no occurrence selector. Author choices alone offer
minimal work IDs/titles and the private catalogue pin; character/dramatic choices
return an empty work catalogue. Unbound context reads do not capture private
source. No shared cursor or source record is created or changed.
[WRITING_FRAMES.md](WRITING_FRAMES.md) defines selection and packet bounds.

`existence-episode` requires `existence-v1` and `generational-core-v1`, without
requiring lore. It stores an exact character ID, one of `embodied`, `disembodied`,
`constructed`, `other`, `unknown`, and an explicit native interval. Affirmative
episodes require start evidence; bounded ends require end evidence and unbounded
ends forbid it. An unknown episode has a wholly unbounded interval, empty start
evidence and no end evidence. All statuses validate; only scoped canonical source
is read. Historical pointers share the closed assertion evidence grammar, remain
unverified, and need not identify current records. Common source metadata is
allowed; operational validity/evidence objects cannot own chronology annotations.
See [EXISTENCE.md](EXISTENCE.md). These episodes do not update vital histories.

Governance uses ordinary profile-backed concepts under `collective-parties-v1`,
which requires both generational and lore core. A query binds six named roles to
unique exact fields or null. Known typed null is distinct from a missing/unbound
field. Every selected typed reference must be admitted before the whole policy
is returned. Jurisdiction follows exact location equality or direct admitted
overlay membership, with a horizon required for timed overlays. Native assertions
link by exact subject or root typed entity value; literal objects do not create
joins. Public Party projections use character/organization/concept tags while
stored entity references and legacy endpoint restrictions stay unchanged.
Supersedes is only a cited pointer. See [GOVERNANCE.md](GOVERNANCE.md).

These author domains preserve typed nulls, exact integers and original source
citations. Character reads use the separate acquired-record scope: canonical
assertion and existence payloads are withheld, linked assertions preserve learned
words, and known-owner dated sections can enter generic character context/search.
Untimed bodies and author metadata remain excluded. Character governance leaves
jurisdiction/basis/parties unassigned. See [NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md).

## Story points

```yaml
activation_policy: suggest
lifecycle:
  initial_state: dormant
  transitions: []
requires:
  - id: sp_00000000000000000000000001
    states: [resolved]
when:
  all:
    - event: event_00000000000000000000000001
    - state:
        entity: char_00000000000000000000000001
        key: ready
        equals: true
```

The IDs above illustrate the format; a real source must reference actual records.
Lifecycle states are `dormant`, `active`, `resolved`, `failed`, and `cancelled`.
Transitions carry `id`, `state`, `time`, and optional `causing_event`/`note`.
Requirements use an exact story-point ID and a nonempty list of accepted stored
states. Requirements must be acyclic. Omitted requirements mean none.

`when` has exactly one operator. `all` and `any` take arrays of conditions;
`not` takes one condition. Empty `all` is true; empty `any` is false. Omitted
`when` is true. Leaf conditions are:

| Operator | Value |
| --- | --- |
| `event` | A canonical event ID; true when it has happened at the horizon |
| `scene` | A closed mapping with optional exact `location`, `participant` and `object` IDs; every supplied predicate must match the selected admitted scene |
| `state` | `{entity: ID, key: FIELD, equals: TYPED_VALUE}` |
| `knowledge` | `{character: ID, claim: CLAIM_KEY, states: [...], minimum_confidence: 0.6}`; confidence is optional |
| `relationship` | `{id: RELATIONSHIP_ID, metric: METRIC, min: NUMBER, max: NUMBER}`; at least one bound |

An eligible point must be dormant and satisfy both its requirements and its
condition. `manual`/`suggest` is returned as the author's activation policy;
queries perform no activation. Stored lifecycle is never replaced by derived
eligibility. Conditions are bounded to depth 32 and 1000 nodes.

Old `trigger`, `dependencies`, `on_activate`, and `repeat_policy` fields are
rejected. Optional `outcome_events` references authored events; query output
includes only outcomes at or before the chosen horizon.

## Causes and environments

Events use an explicit `causes` array of event IDs strictly earlier on the same
timeline. Canonical events may reference only canonical causes. Causal queries
traverse those links; they do not infer
causes from adjacency, prose, or effects. Results are bounded to 1000 nodes and
1000 edges and refuse overflow instead of returning an unexplained partial graph.

An environment has explicit `targets`, `time.start`, optional `time.end`, and
optional authored `conditions`/`sensory`. Conditions are a flat string-keyed map
of strings, booleans, nulls or finite numbers; sensory is an array of strings.
Condition integers use decimal strings in JSON. Both interval ends are inclusive.
Environment queries match exactly the requested target. They do not infer that
a condition follows a character to its location or propagates to contained places.

## Core event and knowledge records

Events validate the shapes and reference kinds of time, location, participants,
causes and effects. Participant entries contain a character ID and optional role;
a character may have more than one authored role. Effects must have unique valid
IDs and use only their declared fields.

Knowledge requires a character knower and a claim with nonblank key/statement.
For a plain claim, the single optional truth field is `claim.truth`, with string values `true`,
`false`, `unknown` or `contested`. `author_truth_status` is rejected. Optional
subject/object entity references must exist; an object contains exactly one
`text` or `entity` field.

An assertion-linked claim instead contains exactly `key`, `statement` and
`assertion_id`, with an existing exact assertion and assertions/lore capabilities.
It uses ordinary knowledge transitions; initial rejection does not create
learning, rejection after an affirmative state remains held, and latest
forgetting withholds the learned words. Canonical assertion edits, replacement
and expiry do not change those words. See [LEARNED_ASSERTIONS.md](LEARNED_ASSERTIONS.md).

All knowledge forms may declare unique nonblank `audience`/`perspectives` string
arrays. Omission adds no constraint; an empty array admits nothing. Character
learning uses fixed public/viewer and ordinary/viewer label sets. An admitted
ordinary `claim.object.entity` acquires an exact record for shared narrative
reads; subjects, typed assertion/genealogy pointers and literal mentions do not.

Knowledge transitions require unique nonblank IDs, time, a supported state,
acquisition and finite confidence in [0, 1]. A causing event must be canonical,
on the same timeline and no later than the transition. An optional cause/source
may be absent or null in canonical source. Typed authoring requests use omission
and reject null. See [AUTHORING.md](AUTHORING.md) for complete request shapes.

## Scenes and conversations

Both kinds declare a location ID, a time interval and participant rows. Scene
statuses are `planned`, `active`, `closed`, `abandoned`; conversation statuses
are `draft`, `active`, `closed`, `retired`. Only `active` and `closed` records
confer presence or character evidence. Planned/draft contents still validate.

```yaml
time:
  start: {tick: 10, order: 0}
  current: {tick: 12, order: 0} # Scenes only; required.
  end: {tick: 12, order: 0}     # Required when closed.
participants:
  - character: char_00000000000000000000000001
    from: {tick: 10, order: 0}
    until: {tick: 12, order: 0}
    role: witness
    point_of_view: true
```

Every coordinate uses a declared timeline. Start, current and end must satisfy
`start <= current <= end` on one timeline. Conversations have start and optional
end, without a current cursor. A closed record requires an end. An active scene
without an end has known presence only through its current cursor; an open
conversation remains open and every beat supplies its own exact time.

When the world declares `current_time`, each active scene's `time.current` must
equal that cursor. Multiple active scenes require a world cursor. Status edits
do not change it; `init` starts the world at tick 0/order 0.

The explicit [current-time preview](CURRENT_TIME.md) accepts exactly
`{expectedFingerprint, at: {timeline, tick, order}}`, with canonical decimal
strings for the request's i64 tick and i32 order. It constructs one native
consequence Plan updating the world and every active scene together, then
validates the complete candidate. World `current_time` typed updates require all
three coordinates and numeric source scalars. The preview itself writes nothing;
application uses the existing separately reviewed outcome publisher. Existing
interval and timeline invariants still apply, and no events, knowledge,
attendance or scene status changes are inferred.

Participant bounds are **inclusive**. Omitted or null `from`/`until` inherit the
parent bounds; explicit bounds must fit inside them. Only `from` and `until`
are accepted; `to` is an error. Each character has at most one participant row
per record. Role is optional prose, and point_of_view is an optional boolean.
Active/closed scene memberships for one character on one timeline cannot
overlap. A shared endpoint overlaps; use a distinct order for a handoff.

A conversation's optional `scene` is a grouping reference. Its attendance and
location remain independently authored: a remote caller can participate in a
conversation without being present in the linked scene. Reciprocal links are
not required. Scene collections `conversations`, `outcome_events`, `objects`,
`environments` and `story_points` contain unique existing IDs of the named kinds.

### Dated evidence and audiences

An observation contains `id`, `at` and nonblank `text`; optional fields are
`until`, `audience`, and finite nonnegative `salience`. Its origin must lie in
the scene interval. An explicit until cannot precede its origin or switch
timelines; it can extend beyond scene end. Without until, it remains a dated
record rather than an inferred current belief. Salience is not a probability.

A speech beat contains `id`, `at`, `speaker` and nonblank `text`, with optional
`kind: speech`, `audience`, `delivery` prose, `addressee` and `interrupts`.
Omitted kind means speech. An interruption must name an exact earlier local
speech in source order at a strictly earlier point on the same timeline;
missing, null, self, forward and action targets are invalid. Author transcripts
retain the field only when authored; character transcripts retain it only when
the target speech independently passes the same time and audibility gates. An action beat contains `kind: action`, `id`, `at`, nonempty unique
`actors`, and `text`, with optional audience. Explicit times must lie inside
the conversation. Speakers and actors must be present at that exact time.
An addressee is `participants`, a present character ID, or an array of unique
present character IDs. Addressees and delivery prose do not widen audibility.

Audience arrays accept `public`, `participants`, `author`, and exact bare
character IDs. Duplicate/unknown tokens and `char:ID` spellings are rejected.
Public/participants admit only present members. Explicit recipients must also
be present at the evidence origin. Missing, null or empty audiences admit no
recipients. A present speaker or actor always perceives their own beat. An
observation's audience is evaluated at its origin, with its optional inclusive
until checked against the query horizon.

### Recollections

A recollection contains unique `id`, `character`, `at`, `state`, `confidence`
and nonblank `summary`. Its character must occur in the conversation's
participant list. State is `remembered` or `forgotten`; confidence is finite in
[0, 1]. Optional fields are `exact_turns`, `remembered_quotes`, `interpretation`
and `emotional_impression`. Recollections cannot precede conversation start,
but can follow its end. One character cannot have two recollections at the same
exact coordinate in one conversation.

Exact turn references are unique existing local beat IDs no later than the
recollection. Each remembered quote contains `source_turn`, an existing
character `speaker`, authored `text`, and `fidelity: exact` or `approximate`.
The referenced beat must exist and cannot be in the memory's future. Authored
memory may reference a beat that was inaudible; projection filters that
reference and preserves the authored memory text. Validation does not turn
memory into canonical transcript or infer the speaker's identity.

These typed nested mappings reject unknown fields and tagged YAML values.
Optional arrays may be omitted or null, meaning empty. Optional prose and
participant booleans may be omitted or null. Required fields cannot be null.
Ordinary Markdown remains opaque and excluded from character evidence.

### Section declarations

`section_audiences`, when present, maps nonblank single-line section names to
audience arrays. Names must be unique after trimming and lowercasing. Entries
must be unique `public`, `author`, `self`, or exact existing character IDs;
`participants` is also accepted for scenes/conversations. Empty arrays are
allowed, while null or malformed declarations are errors.

These declarations do not specify acquisition time. They do not admit ordinary
Markdown to character views, including sections marked public or self. See
[CHARACTER_VIEWS.md](CHARACTER_VIEWS.md) for the returned evidence contract.

For an authorized reader body projection, exact heading or Unicode-casefolded
ASCII heading keys select explicit audience mappings. Recognized private headings
and explicit restrictions remain effective through the entire descendant tree,
until a nearer explicit mapping overrides them. This conservative native ancestor
rule is documented in [REVEALS.md](REVEALS.md); it does not make body prose dated
character evidence or admit a record without ordinary permission.

## Explicit spatial topology

Locations use optional `parent_id` references, with absent/null meaning root.
Parents must exist, use location IDs, and form an acyclic graph. The old `parent`
field is rejected. Canonical children require canonical parents.

Route records require distinct `from_location_id`/`to_location_id`, direction
(`one-way` or `two-way`) and nonempty unique modes. Optional availability defaults
to open; closed/restricted/unknown routes are not traversed. Uncertainty defaults
to exact and can also be estimated/unknown. Optional `route_distance`,
`travel_cost` and `duration` contain exactly a finite nonnegative value and
nonblank unit. Integer metric values use decimal strings in query responses.
Canonical routes require canonical location endpoints. Retired records still
validate, but do not contribute to queries.

Portals require `from_location_id`, `to` and a nonempty sorted unique array of
nonblank `modes`. The source is an exact existing location ID. The target is
either a distinct exact location ID or a closed map position:
`{map_id: MAP_ID, coordinates: [X,Y]}` (three coordinates for a 3D map).
A canonical portal requires canonical source/target locations or a canonical
target map. Its position must match that map's dimensions and inclusive bounds.
Portal status is canonical or retired; omission means canonical. Portals are
directed and reject route-only fields `direction`, `to_location_id`,
`availability`, `uncertainty`, `route_distance`, `travel_cost` and `duration`.

Routes and portals require explicit world capabilities `spatial-core-v1` and
`route-v1`. Their queries read static author source. Hop paths and reachability
follow open canonical routes and canonical location-target portals. A portal to
a map position is an explicit terminal destination; it does not snap to a
location or add a location edge. Containment supplies no travel edge.
Weighted paths use routes with one selected metric, exact decimal accumulation
and same-unit edges. Missing weights supply no edge, portals supply no weight,
and incompatible units are never compared or converted. Weighted totals are
decimal strings.

### Maps and placements

Maps require `crs`, `axis_order`, `unit` and closed
`bounds: {min: [...], max: [...]}`. The supported CRS combinations are:

| CRS | Axis order | Unit |
| --- | --- | --- |
| `local-planar:NAME`, with a nonblank name | `[east, north]` | Nonblank authored string |
| `EPSG:4326` | `[longitude, latitude]` | `degree` |

Bounds have matching two- or three-coordinate arrays and strict `min < max` on
every axis. Optional `z_policy` defaults to `forbidden`:

| Policy | Bound dimensionality |
| --- | --- |
| `forbidden` | Exactly 2D |
| `optional-level` | Either 2D or 3D, fixed by the authored bounds |
| `required` | Exactly 3D |

Placements, map positions and query coordinates must match the map's bound
dimensionality; `optional-level` does not permit mixed dimensions on one map.
All coordinates are finite numeric values with absolute value at most
9007199254740991. Geographic bounds additionally keep longitude within
[-180,180] and latitude within [-90,90]. The optional third coordinate is an
authored level; no vertical datum or physical conversion is inferred.

Optional `origin` contains exactly a nonblank `label` and numeric `coordinates`
matching the bounds' dimensionality. Optional `scale` contains exactly a finite
nonnegative `value` and nonblank `unit`. Both are retained as authored metadata;
they do not transform or rescale coordinates.

A location's optional `spatial` contains exactly `map_id` and `geometry`.
Geometry contains exactly `kind` and `coordinates`:

| Kind | Coordinates |
| --- | --- |
| `point` | One 2D or 3D position |
| `line` | 2–10,000 positions of one dimensionality |
| `polygon` | 4–10,000 positions of one dimensionality, repeating the first as the last |

Absent/null spatial means unplaced. Otherwise `map_id` must be an exact existing
map ID, every vertex must match its dimensions and inclusive bounds, and a
canonical location requires a canonical map. Maps and placed locations use
canonical/retired status; all declarations validate and only canonical ones
participate in queries.

Maps and placements require explicit `spatial-core-v1` and `geometry-v1`
capabilities. Bbox queries compare inclusive geometry bounding envelopes;
nearby queries measure Euclidean distance to the authored point, line or polygon
using f64 geometry. Geographic queries use the same coordinate-space rules in
degrees. Geometry supplies neither geodesic distances nor route weights or
travel edges.

### Literal map anchors

An anchor requires closed `from` and `to` positions, each containing exactly
`map_id` and `coordinates`. They must name different existing maps. Each position
matches its own map's dimensions and inclusive bounds, so a 2D map can be paired
with a 3D map. Anchor status is canonical or retired, defaulting to canonical;
canonical anchors require canonical endpoint maps. Optional `conversion` is a
literal untagged string. Anchor queries retain the authored direction and prose;
they do not execute a transform, interpolate coordinates or create a reverse
anchor. Anchors require explicit `spatial-core-v1` and `geometry-v1` capabilities
and supply no travel edges.

### Overlay membership

An overlay requires `lifecycle`, `membership`, `audience` and `perspectives`.
Membership is closed: `{location_ids: [...]}` contains sorted unique exact
existing location IDs. Audience and perspectives are sorted unique arrays of
nonblank authored presentation labels. All three arrays may be empty. Overlay
status is canonical or retired, defaulting to canonical; canonical overlays
require canonical member locations. The old `faction`, `institution` and
`calendar` fields are rejected.

`lifecycle: static` requires `valid` to be absent, including no explicit null.
`lifecycle: time-bounded` requires closed `valid: {start: ..., end: ...}`. Both
endpoints explicitly contain only `timeline`, signed i64 integer `tick` and
signed i32 integer `order`. The timeline must be declared and the inclusive
bounds ordered on the same timeline. Static membership applies on any declared
query timeline; bounded membership applies only within its authored window.

Overlays require explicit `spatial-core-v1` and `overlay-v1` capabilities. Their
queries filter exact audience and perspective labels before membership, time
and result bounds. These are author presentation filters: they confer no
character evidence or entitlement. Spatial topology alone does not enter
generic character context/search. Explicit dated sections on independently
known spatial owners use the separate shared character admission rules.

Spatial references use exact IDs of the named kind. Locations, maps, anchors,
routes, portals and overlays accept their portable prefixed IDs or namespaced
IDs such as `location:harbor/landing`, `map:harbor` and `portal:harbor/ladder`.

### Spatial history

Only location and route records may own `spatial_history`. It is an ordered
untagged array of at most 1024 closed members, each with `id`, `aspect`, `valid`
and `value`, plus optional `replaces`. Local IDs use `spatialhistory_` followed
by 26 uppercase Crockford characters and are unique within the owner.

| Owner | Aspect | Closed value |
| --- | --- | --- |
| Location | `placement`, `geometry` | `{map_id: MAP_ID, geometry: SHAPE}` using the existing complete placement codec |
| Location | `parentage` | `{parent_id: LOCATION_ID}` or `{parent_id: null}` |
| Route | `route-availability` | `{availability: open\|closed\|restricted\|unknown}` |
| Route | `route-metric` | One named `route_distance`, `travel_cost` or `duration` field with exactly `{value, unit}` |

`valid: {kind: timeless}` applies across declared timelines. An interval contains
exactly `kind: interval`, a declared `timeline`, and optional `start`/`end`.
Each supplied endpoint has exactly `timeline`, signed i64 integer `tick` and
signed i32 integer `order`. Its timeline must equal the interval's. Omitted
bounds are open; explicit null bounds are invalid. Ordered bounds are inclusive,
including exact order within a tick. Public time coordinates use decimal strings.

Parent references and map IDs must be exact existing records. Historical shape
dimensions, coordinates, map bounds and metrics use their existing validators.
Canonical owners require canonical referenced maps/parents; self-parentage is
invalid. Retired owners also validate; only canonical owners supply results.

Overlapping members targeting the same effective field require one explicit
correction chain. Placement and geometry share the `spatial` target, while each
route metric has its own target. A correction names an earlier same-owner member
and retains its exact aspect, literal validity and metric target. Forks, forward
references and changed applicability are refused. Effective containment must
remain acyclic at all relevant interval boundaries and immediately after an
inclusive end, including the successor across maximum order values.

Applicable unsuperseded members override only their declared fields. Outside
their windows, static values apply; static absence remains absent. All-time
history reads retain correction members; dated reads select the applicable chain
ends. Geometry discovers effective placements before map/bounds/result limits,
and route inspection resolves availability and each metric independently. These
readers borrow source values and retain exact field/correction citations without
mutating source records. Ordinary paths, adjacency and reachability remain static.

Any history field, including `[]`, requires `spatial-core-v1` and
`spatial-history-v1`; placement/geometry adds `geometry-v1`, and route history
adds `route-v1`. Location and route create/update requests replace a supplied
history array as a whole. Omission preserves it, `[]` clears it, and top-level
null is refused. All twenty-seven non-world kinds other than hypothesis retain
their top-level chronology annotation support.

The author reads retain exact source provenance. Separate character
spatial-history reads require an independently known canonical owner and admit
each referenced map/parent endpoint before correction suppression. Their
results omit source-array indexes, owner titles and citations. Spatial-history
members do not automatically enter generic character context/search; explicit
dated sections follow their own admission. See
[NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md) for character scope and
[SPATIAL.md](SPATIAL.md) for CLI/HTTP queries, exact author provenance and
complete-or-error limits. Authoring examples 58–63 cover history, corrections
and clearing.

## Canonical generational histories

Both kinds require the world capability `generational-core-v1`. Parentage uses a
`kinship_` ID, exact canonical-character `child_id`/`parent_id`, initialization and
transitions. Unions use a `union_` ID, a sorted unique 2–500 character header
roster, initialization and transitions. Status is canonical, draft or retired;
all validate, and only canonical histories supply queried family facts.

Each history entry has a globally unique `transition_` ID, supported kind,
instant applicability and closed payload. Its point requires a declared
timeline, exact signed i64 tick and i32 order. Parentage initializes as asserted,
may confirm biological/adoptive basis, and may end. A union initializes as
declared, may form, reconcile its roster while formed, and end or annul. Terminal
states do not reopen. The latest visible union payload supplies membership;
header and historical rosters never substitute for it.

Later transitions may name a strictly earlier canonical cause event or replace
an earlier same-record transition at the identical instant. Corrections validate
against that target's prior lifecycle and retain superseded source citations.
Histories have at most 1,000 entries including initialization and increase on
one timeline; equal points require a replacement. Canonical parentage must be
acyclic even across ended histories and different timelines. Duplicate checking
uses the exact child, parent, initial basis and initialization point.

Nested chronology is forbidden on history entries, applicability, points and
payloads; top-level annotations and opaque metadata remain valid. Author queries
use exact inclusive horizons and complete-or-error collection/traversal limits.
Unions supply no parentage edges or learned knowledge. See
[GENERATIONAL.md](GENERATIONAL.md) for the full transition, citation, query and
limit contract.

Organizations require `organization_kind` (house/dynasty/clan/institution/other)
and allow canonical organization-parent/location references. Affiliations name
an exact character and organization; overlapping canonical histories for that
pair are invalid even when their roles differ. Organizational histories support
rename, reparent-to-ID/null, dormancy and dissolution. Header containment cycles
are invalid; cycles created by reparenting fail queries at the affected horizon.

Legacies require `legacy_kind` (office/estate/title/other), optional organization,
and dated rename/dormancy/dissolution. Tenures reference a legacy and explicitly
initialize vacant or holding with legal/de-facto basis. Designation, holding,
inclusive vacancy, caused transfer and ending obey their closed lifecycle.
Vacancy becomes query-derived expired after its inclusive last point, without
reopening. A transfer clears the source holder and requires an already eligible
target; it never generates the target's hold. Optional predecessor/successor
links require the same legacy and strict initial time order.

Claims name a legacy and claimant, with proposed/disputed/recognized/withdrawn/
rejected lifecycle. Competing claims require reciprocal effective evidence at
the same applicability. Claims never appoint holders. Vital histories name a
character and disclosure (known/unknown/withheld), then follow birth/death or
existence-start/end. Unknown source allows initialization only; withheld source
may retain affirmative history but typed queries conceal it. Conflicting known
states remain explicit ambiguity, never an inferred winner, age or death.

All eight kinds share the source/history caps, corrections and author-only query
boundary in `GENERATIONAL.md`. Folded values and visible history payloads omit
references to generational targets unavailable at the selected horizon.
Canonical history alone supplies no character evidence. Separately learned
assertions use `knowledge.claim.genealogy` with the additional explicit
`generational-knowledge-v1` capability. Typed claims contain only key, statement
and genealogy. The latter is a closed kind/payload/valid object, optionally
carrying endpoint-only learned labels and exact evidence references. Seven kinds
cover parentage, unions, organizations, affiliations, tenures, claims and vital
states. Explicit learning controls admission independently of inclusive claim
validity; rejection/forgetting excludes current evidence. Labels survive validity
expiry only while the assertion remains held. Existing transition IDs, time
shorthands and acquisition vocabulary remain supported. See
[GENERATIONAL_KNOWLEDGE.md](GENERATIONAL_KNOWLEDGE.md) for all payload fields,
limits, evidence rules and author/character query boundaries.

## Calendar chronology

An optional world `chronology` mapping declares all three arrays: `calendars`,
`eras` and `anchors`. Cycle/table calendars support exact civil days, signed
years and explicit shared-axis epochs. Era definitions provide display-year
offsets, optional zero skipping and inclusive bounds. Anchors map exact axis
days to explicit timeline/tick/order positions; they never imply elapsed time
between ticks. Different timelines are independent.

Declarations use closed typed mappings with opaque `x-*` extension metadata.
Source integers are exact signed i64 values where applicable; public date/axis
coordinates are decimal strings. Calendar rules and source bounds are detailed
in [CHRONOLOGY.md](CHRONOLOGY.md). No new capability is required.

Non-world `chronology` is an annotation array. Entries have an owner-scoped ID,
nonempty provenance, one tagged value and optional role/display strings. Civil
and era values support exact days or partial years/months. Range/approximation
bounds are inclusive civil values or null; conflicts retain separate claims.
Relative references name existing records and durations remain display-only.
Unsupported shapes, missing calendars/eras/references, invalid dates, reversed
bounds and overflowing normalization fail validation. Declarations and
annotations validate together, so calendar edits cannot orphan stored dates.

All seven tags, resource bounds, comparison/date-search rules and typed query
operands are specified in `CHRONOLOGY.md`. Queries are author-only and never alter
story replay or character evidence. Chronology belongs on a source record:
nested declarations inside story positions, effects, transitions, spatial
geometry, map bounds/origin/scale, anchor endpoints, portal positions, overlay
membership/validity and other declared operational objects fail with an exact
source-field diagnostic.
Arbitrary state keys, typed lore fields, metric names, scalar environment
conditions and opaque `x-*` metadata may still be named `chronology`. New domain
schemas must preserve this structural boundary. Validation remains partial for
other domains.

## Editing

`preview-body` constructs and validates one replacement while preserving its
YAML envelope. `record create` and `record update` preview typed requests for
characters, locations, objects, events, knowledge, environments, relationships,
story points, hypotheses, scenes, conversations, routes,
maps, anchors, portals, overlays, lore concepts, assertions, existence episodes, context briefs,
organizations, parentage, unions,
affiliations, legacies, tenures, claims and vital histories. `record update` also
accepts world chronology declarations, complete `profiles`, `predicates`, `state_keys`,
`state_profile_bindings`, `timelines`, `capabilities`, `threads` and
`relationship_metrics` replacements, explicit `current_time`, `default_timeline`,
and common metadata. All 27 non-world kinds other than hypothesis accept annotation arrays;
an empty array clears them. Those 27 kinds and the world accept complete `concept_links` and
`descriptions` arrays.
Concept `values` also replace the whole field. Every edit validates the complete
candidate, so profile changes cannot orphan or invalidate dependent concepts,
initial state or event effects. Predicate changes similarly check every retained
assertion. Examples 64–71 cover the lore foundation; 72–79 add bound state and
common/individual descriptions; 80–87 add magic composition; 88–97 add dated
sections and classified assertions; 98–125 add social and environmental
contexts; 126–142 add authored existence and governance; 143–158 exercise shared
character admission and linked learning; 159–175 add required briefs and exact
writing examples; 176–203 add explicit family admission and dated accounts.
Requests 204–207 add a public retrieval prior and independently learned evidence.
All 207 requests leave 144 records with 25 profiles and nine
predicates.
Typed edits save only with an explicit `--apply` and the matching candidate hash.
`apply-body` requires the preview's base fingerprint and full
candidate SHA-256. It rejects stale source, changed requests, invalid candidates,
read-only targets and historical snapshots. It writes a temporary file beside
the target and atomically replaces the target after rechecking the source.

A process-held OS lock serializes WEDL writers and releases on exit. It does not
coordinate arbitrary external editors; avoid simultaneous external writes during
apply. This is optimistic single-file editing, not a filesystem compare-and-swap
or a power-loss-durable multi-file transaction. No Git index/ref/commit or cache
is published by apply; the next search rebuilds a stale cache normally.

The separate saved-outcome operation publishes an explicit multi-record plan
and terminal private receipt through a bounded recoverable journal. Native
current captures refuse pending publication and detect generation changes.
This is separate from the single-file contract above; see
[OUTCOMES.md](OUTCOMES.md) for approval, read fencing, rollback and exact-source
verification. Publication state in `.wedl-rust/` must not be discarded as cache.


## Private retcon review source

`author/retcon-reviews/review_<26-character-id>.json` is separate private JSON;
it is excluded from canonical `story/` counts, validation, fingerprints and
search. Its closed fields are `id`, `title`, `analysis`, `decisions` and
`bodyMarkdown`. The exact original analysis includes the committed base, explicit
plan/target/horizon, report and item digests, and an admitted-input witness.
Updates may change only title, decisions and commentary. Decisions form a unique
ordered subsequence of original impact items; omission means pending.

Save requests require both current `expectedFingerprint` and explicit
`expectedSha256` (`null` only for creation). A decision grants no canonical
approval and no save publishes a plan. Witness source/declaration values are
canonical JSON literal strings in `valueRaw`, retaining exact numeric tokens
through JSON transports. The source schema remains `wedl/v1`; witness encoding
has its own `wedl-retcon-witness/v1` identifier. See
[RETCON_REVIEWS.md](RETCON_REVIEWS.md) for reconstruction, currentness and bounds.

An original analysis can include exact `author-record` segment/work targets.
Segment source links and POV references yield `manuscript-source` and
`manuscript-pov` edges; each admitted work occurrence yields a `work-segment`
edge. Original capture requires committed canonical and manuscript source to
match their current snapshots. Later currentness follows admitted inputs and
new incoming references, including private ones; unrelated private documents do
not stale a review merely by changing the whole manuscript fingerprint. Private
source stays unchanged in a canonical consequence candidate.
