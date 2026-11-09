# Record authoring

The v1 implementation provides typed YAML requests for all twenty-eight non-world kinds:
`character`, `location`, `object`, `event`, `knowledge`, `environment`,
`relationship`, `story-point`, `hypothesis`, `scene`, `conversation`,
`route`, `map`, `anchor`, `portal`, `overlay`, `organization`, `parentage`,
`union`, `affiliation`, `legacy`, `tenure`, `claim`, `vital-history`, `lore-concept`,
`assertion`, `existence-episode` and `context-brief`.
The source schema is `wedl/v1`.
World records additionally support updates to `current_time`, `timelines`,
`default_timeline`, `capabilities`, `threads`, `relationship_metrics`, `chronology`,
`profiles`, `predicates`, `state_keys`, `state_profile_bindings` and common metadata;
world creation still uses `init`. Authoring remains part of the ongoing plan;
its contract can change within v1.
Requests do not include a schema or file path; the writer supplies them.

## Preview and save

```sh
wedl-rs --repo my-world status
wedl-rs --repo my-world record create --file character.yaml --expected-fingerprint BASE
wedl-rs --repo my-world record create --file character.yaml --expected-fingerprint BASE --apply --expected-after-sha256 CANDIDATE

wedl-rs --repo my-world record update "Ana Reed" --file update.yaml --expected-fingerprint BASE
wedl-rs --repo my-world record update "Ana Reed" --file update.yaml --expected-fingerprint BASE --apply --expected-after-sha256 CANDIDATE
```

`BASE` is the source fingerprint from `status`. `CANDIDATE` is `afterSha256`
from the corresponding preview. Inspect `validation` and `afterSource` before
applying. After a save, use the new fingerprint for the next operation. Inputs
are UTF-8 YAML, limited to 1 MiB; `--file -` reads standard input.
Before constructing expanded values, a streaming preflight limits expanded
UTF-8 strings (including mapping keys and every alias copy) to 1 MiB,
values/keys to 131,072 and nesting depth to 128. Bounded ordinary aliases remain
accepted. Retained [edit drafts](EDITS.md) keep their separate smaller limits.

A malformed request fails immediately. A structurally valid request whose
candidate world is invalid returns diagnostics with `canApply:false`; applying
it fails without changing source. A preview is always read-only. `--apply` and
`--expected-after-sha256` must be supplied together.

Explicit [consequence plans](CONSEQUENCES.md) reuse these exact create/update
YAML strings for a complete read-only candidate containing 1–64 operations.
Each operation constructs against the same captured base; final validation sees
the combined records, including ordinary forward references. Rich typed values
requiring a profile binding or target kind newly introduced elsewhere in the
plan may fail the existing construction boundary. Plans accept exact update IDs
and one write per record, with no deletes or temporary IDs. They expose exact
source/diffs and diagnostics. A standalone plan cannot apply itself; an exact
saved proposal can proceed through separate [outcome approval and publication](OUTCOMES.md).
Private proposal history and its fresh human review are specified in
[ACTIONS.md](ACTIONS.md).

## Request rules

Creation requires `kind`, `id`, and `title`, plus any kind-specific required
fields below. IDs must follow the record kind's existing ID rules and be unique.
New scenes default to `planned`, new conversations to `draft`, and new hypotheses
to `open`. The other twenty-five kinds use canonical status. Empty arrays/maps and an empty body are valid
where their field allows them. Omitted optional fields use minimal defaults.

Update requires `kind` matching the existing record. Select the record by ID,
title or alias in the command; stored references within the request use exact
IDs. Omitted fields remain unchanged. Supplied lists and maps replace the whole
field, including nested objects; they do not merge. Unknown fields and top-level
null values are refused except where the selected type explicitly permits them.
Examples include an assertion's whole `value: null`, environment condition values
and open `time.end`, relationship `inverse` and transition `causing_event`, and
hypothesis resolution or optional placement links. Chronology permits nullable data such as `epoch:null`
or open annotation bounds, and canonical/learned generational payloads permit declared null roles,
holders and parents; domain validation checks the exact positions.
Spatial history also permits null for an explicit root parent, while its omitted
interval endpoints remain distinct from invalid null endpoints.
Lore values permit null only where their selected profile declares a null type
or a matching enum alternative; profile enums may likewise contain literal null.
Predicate value types likewise permit declared nulls at any supported typed
position. The same declared nulls are accepted inside profile-bound initial values and
set effects. Unbound state keeps its ordinary value grammar.
`id`, `schema`, and source path cannot be edited. Status uses the source kind's
existing vocabulary: world `canonical`; events `canonical`, `draft`, `cancelled`,
`retconned`; scenes `planned`, `active`, `closed`, `abandoned`; conversations
`draft`, `active`, `closed`, `retired`; hypotheses `open`, `adopted`, `rejected`.
All eight generational kinds, lore concepts, assertions, existence episodes and
context briefs accept `canonical`, `draft`, `retired`. Remaining kinds accept
`canonical`, `retired`. Every status validates its complete contents.

| Kind | Editable fields in addition to common fields |
| --- | --- |
| All editable kinds, including world updates | `title`, `aliases`, `tags`, `body`, `status`, `domain`, `section_audiences` |
| All kinds except hypothesis | `threads`, `audience`, `perspectives`, `concept_links`, `descriptions` |
| World (update only) | `current_time`, `timelines`, `default_timeline`, `capabilities`, `threads` declarations, `relationship_metrics`, `chronology`, `profiles`, `predicates`, `state_keys`, `state_profile_bindings`; supplied arrays/maps replace whole fields |
| Twenty-seven non-world kinds other than hypothesis | `chronology` annotation array, replaced as a whole |
| Character | `initial_state`, `role`, `pronouns` |
| Location | `initial_state`, `parent_id`, `spatial`, `spatial_history` |
| Object | `initial_state`, `object_affordances` |
| Event | `time` (required on create), `location`, `participants`, `causes`, `effects`, `related_story_points` |
| Knowledge | `knower` and `claim` (required on create), `transitions`, optional `audience` and `perspectives` |
| Environment | `time` and `targets` (required on create); `conditions`, `sensory`, `initial_state` |
| Relationship | `from` and `to` (required on create); `relationship_kind`, `inverse`, `transitions` |
| Story point | `activation_policy` and `lifecycle` (required on create); `requires`, `when`, `priority`, `outcome_events` |
| Hypothesis | `statement`, `subjects`, `alternatives`, `context`, `placement` (all required on create); `resolution` |
| Context brief | `domain`, `scope`, `valid`, `required_refs`, `audience`, `perspectives` (all required on create); conditional `scope_ref`, `status`, `provenance` |
| Scene | `location` and `time` (required on create), `status`, `participants`, `objects`, `environments`, `conversations`, `story_points`, `outcome_events`, `observations` |
| Conversation | `location` and `time` (required on create), `status`, `scene`, `participants`, `turns`, `recollections` |
| Route | `from_location_id`, `to_location_id`, `direction`, `modes` (all required on create); `availability`, `uncertainty`, `route_distance`, `travel_cost`, `duration`, `spatial_history` |
| Map | `crs`, `axis_order`, `unit`, `bounds` (all required on create); `z_policy`, `origin`, `scale` |
| Anchor | `from`, `to` (both required on create); `conversion`, `status` |
| Portal | `from_location_id`, `to`, `modes` (all required on create); `status` |
| Overlay | `lifecycle`, `membership`, `audience`, `perspectives` (all required on create); `valid` (required for time-bounded lifecycle), `status` |
| Parentage | `child_id`, `parent_id`, `initialization` (all required on create); `transitions`, `status` |
| Union | `participant_ids`, `initialization` (both required on create); `transitions`, `status` |
| Organization | `organization_kind`, `initialization` (required on create); `parent_id`, `location_id`, `transitions`, `status` |
| Affiliation | `character_id`, `organization_id`, `initialization` (required on create); `transitions`, `status` |
| Legacy | `legacy_kind`, `initialization` (required on create); `organization_id`, `transitions`, `status` |
| Tenure | `legacy_id`, `initialization` (required on create); `predecessor_tenure_id`, `successor_tenure_id`, `transitions`, `status` |
| Claim | `legacy_id`, `claimant_id`, `initialization` (required on create); `transitions`, `status` |
| Vital history | `character_id`, `disclosure`, `initialization` (required on create); `transitions`, `status` |
| Lore concept | `domain`, `profile`, `values`, `valid`, `audience`, `perspectives` (all required on create); `provenance`, `status` |
| Assertion | `domain`, `subject`, `predicate`, `value`, `classification`, `valid`, `evidence`, `audience`, `perspectives` (all required on create); `attributed_to`, `replaces`, `provenance`, `status` |
| Existence episode | `domain`, `character_id`, `episode_kind`, `valid`, `start_evidence`, `audience`, `perspectives` (all required on create); `end_evidence`, `provenance`, `status` |

`aliases` and `tags` are arrays of unique nonblank strings. `initial_state` is
a string-keyed map checked against the world's state declarations. Pronouns
contain `subject` and `object`, optionally `possessive`. Body strings are exact
replacement Markdown; use YAML block chomping (`|`, `|-`, `|+`) to control their
trailing newline.

Unknown existing metadata is retained when updating. An actual structured
change serializes a new YAML header, so header comments/formatting normalize.
An unchanged request keeps the original bytes, and a body-only change keeps the
exact original envelope. Omitted prose is preserved byte-for-byte.

Replacement applies to the entire supplied top-level value. For example,
replacing `time` without its optional `end` removes that nested endpoint. A
request containing only `time.end` is not a partial nested update: supply the
required start/current fields too. Removing an optional top-level field such as
`conversation.scene` still requires a direct source edit.

## Common metadata and world declarations

`domain` is a nonblank authored string. Ordinary `audience` and `perspectives`
arrays preserve literal strings, including order, duplicates and empty entries.
Knowledge retains its unique nonblank label rule. Overlay, lore concept,
assertion, existence episode and context brief labels retain their sorted unique
rule. These labels describe author filters; editing them creates no character
knowledge. Hypotheses have a separate closed vocabulary and refuse these fields,
`threads`, `chronology`, `concept_links` and `descriptions`.

`section_audiences` replaces a complete map from nonblank single-line headings
to unique audience entries. Headings are unique after trimming and case folding.
Entries are `public`, `author`, `self` or exact character IDs; `participants` is
also available for scenes and conversations. An empty map clears the declarations.
These are prose audiences, not acquisition times or learned facts.

World `timelines` is a nonempty array of `{id, label}` declarations with optional
`origin: {tick, label}`. IDs are unique, labels nonblank, and origin ticks are
exact signed i64 values. `default_timeline` must name a declared timeline.
Changing a default also changes the interpretation of source points that omit
their timeline. Complete validation still checks every explicit point and the
world cursor; removing a used timeline leaves an invalid candidate.

World `threads` is a sorted unique array of exactly `{id, label}` declarations.
IDs use `thread_` followed by 26 uppercase Crockford characters. Other supported
records use sorted unique arrays of declared thread IDs. `threads: []` clears
memberships or unused declarations. Removing a declaration while a record still
uses it fails complete-candidate validation. A consequence plan can update the
declaration and every affected record together.

`capabilities` uses the known registry order and must include each selected
capability's prerequisites. A typed world update can declare capabilities before
dependent records are created, or a consequence plan can add both together.
Removing a capability required by existing source produces an invalid candidate.
No record create request enables capabilities automatically.

`relationship_metrics` replaces a map of nonblank metric names to declarations.
The editable declaration members are finite numeric `min`, `max` and `default`,
all optional; `{}` is valid. Submitted relationship transition metrics must be finite
numbers whose names remain declared. Declaration values are retained as authored;
they do not fill, clamp or rewrite any transition. Removing a used metric fails
unless the complete candidate also removes or replaces its uses.

## Environments, relationships, story points and hypotheses

An environment requires `time: {start: POINT, end: POINT_OR_NULL}` and a nonempty
unique `targets` array of exact record IDs. Omitted or null `end` leaves the
interval open. Endpoints must share a timeline and end cannot precede start.
`conditions` is a flat map of nonblank keys to literal strings, booleans, finite
numbers or null; large integers retain their exact source values. `sensory` is an
array of unique nonblank strings. `conditions: {}` and `sensory: []` clear those
fields. Optional `initial_state` uses existing world state declarations.

A relationship is directed from one exact character ID to another. Its optional
`inverse` names a separately authored relationship; null clears that link.
`relationship_kind` is a nonblank authored label. New relationships default to
`transitions: []`. Each transition contains `id` and `time`, plus optional
`relationship_status`, `metrics`, `facets`, `causing_event` and `note`.
Transition IDs are unique local strings and times strictly increase on one
timeline. Status is an authored nonblank string. Metrics are complete maps of
declared names to finite numbers; facets are literal string arrays preserving
order, duplicates and empty entries. Notes are
literal strings. A nonnull cause must be a canonical event no later than the
transition on the same timeline. Replacing the transition array replaces the
entire history; `transitions: []` clears it. No reciprocal record, metric default,
or extra transition is generated.

Existing raw relationship null defaults and opaque transition/time extensions
remain intact when omitted. Unsupported raw metric payloads or declaration
policies keep their existing read errors or semantic `unsupported` results;
typed replacements still enforce the closed request shapes above.

Story points use the existing native `requires`/`when` grammar:

```yaml
kind: story-point
id: sp_00000000000000000000000001
title: Find a way through
activation_policy: suggest
lifecycle:
  initial_state: dormant
  transitions: []
requires: []
when: {all: []}
priority: 0
outcome_events: []
```

Policy is `manual` or `suggest`. Lifecycle states are `dormant`, `active`,
`resolved`, `failed`, `cancelled`; transitions contain unique local `id`, `state`
and `time`, optionally a canonical dated `causing_event` and nonblank `note`.
`requires` contains unique `{id: STORY_POINT_ID, states: [STATE, ...]}` entries
with nonempty allowed state sets, and the complete dependency graph must be
acyclic. `when` contains one operator per node: `all`, `any`, `not`, `event`,
`state`, `knowledge`, `relationship` or `scene`, using the closed source shapes in
[SCHEMA.md](SCHEMA.md). `priority` is signed i32. Outcome IDs name canonical
events. Eligibility reads never activate a point or manufacture events; an
explicit lifecycle edit records the author's choice. The optional closed
`scene` condition names `location`, `participant` and/or `object`; it evaluates
against an explicitly selected admitted scene and respects timed participant
presence. It does not narrow the candidate list. Legacy `trigger`,
`dependencies`, `on_activate` and `repeat_policy` are not request fields.

Hypotheses require nonblank `statement` and `context`, nonempty unique exact
record `subjects`, and a nonempty array of nonblank `alternatives`; repeated
alternative wording is retained. `placement` is a nonempty map containing only
`context`, `timeline`, `scene`, `event`, `location`. Timeline names must be
declared, links must have their exact kinds, and placement is contextual rather
than operational story time. Null optional scene/event/location links are valid.
New hypotheses are open and have no resolution. Adopted hypotheses require
nonempty unique `resolution.canonical_entities` naming existing settled records;
rejected hypotheses require a nonblank `resolution.note`. Resolution fields are
validated against status, including when both change in one request. To reopen
explicitly, replace status with `open` and resolution with null. These records
remain author possibilities and do not generate canonical facts or knowledge.

Object `object_affordances` is an object-only array of literal strings. Order,
duplicates, empty strings and whitespace are retained exactly. Omission preserves
absence or the existing value; `[]` records an explicit empty array. Labels do
not create action templates or imply that an action occurred.

## Event fields

```yaml
kind: event
id: event_00000000000000000000000001
title: Bell Rung
time: {timeline: main, tick: 1, order: 0}
location: loc_00000000000000000000000001
participants:
  - character: char_00000000000000000000000001
    role: bell ringer
causes: []
effects: []
```

Time requires exact signed i64 `tick`; optional `order` is signed i32 and defaults
to zero. An omitted timeline uses the world's default. Location references a
location; each participant references a character and may include a role.
Causes reference strictly earlier events on the same timeline. Canonical events
require canonical causes.

Effects contain `id`, `target`, `key`, `operation`, and a typed `value` except
for `clear`, which forbids a value. Operations are `set`, `clear`, `add-to-set`
and `remove-from-set`. Effect IDs are unique within the event and use the
`effect_` prefix with 26 Crockford characters. State declarations, references,
exclusivity and replay conflicts are validated against the entire candidate world.
An explicitly profile-bound key instead accepts a complete profile object on
`set`, or `clear` without a value. Item add/remove operations are refused.

## Knowledge fields

```yaml
kind: knowledge
id: know_00000000000000000000000001
title: The Bell Has Rung
knower: char_00000000000000000000000001
claim:
  key: harbor.bell-rung
  statement: The harbor bell has rung.
  truth: 'true'
transitions:
  - id: first-observation
    time: {timeline: main, tick: 1, order: 0}
    state: accepted
    acquisition: observed
    confidence: 1.0
    causing_event: event_00000000000000000000000001
```

The knower references a character. A plain claim requires nonblank `key` and
`statement`; optional fields are `subject` (record ID), `predicate` (string),
`object` (exactly `{text: STRING}` or `{entity: ID}`), and `truth`. Truth is a
string: `true`, `false`, `unknown`, or `contested`. Quote true/false in YAML.
The former `author_truth_status` field is rejected.

An assertion-linked claim instead contains exactly `key`, `statement` and
`assertion_id`. It requires an existing exact assertion and the declared
`assertions-v1`/`lore-core-v1` capabilities. It cannot mix ordinary claim fields
or genealogy. Its learned words remain independent of the assertion's current
values, validity, labels and corrections. Initial rejection is valid source but
does not admit learned words; an earlier affirmative transition is required.
Later rejection remains held, while latest forgetting withholds the claim.
See [LEARNED_ASSERTIONS.md](LEARNED_ASSERTIONS.md).

All knowledge forms accept optional `audience` and `perspectives` arrays of
unique nonblank strings; order is not significant. Omitted arrays impose no
extra constraint, and an empty array withholds the record. Character learning
matches audience `public` or the viewer ID, and perspective `ordinary` or the
viewer ID. Submitted arrays replace completely; omitted fields remain intact.
These scopes filter plain, generational and linked character evidence, and any
knowledge used as explicit generational learning proof must also be admitted.

For shared narrative reads, an admitted ordinary `claim.object.entity` acquires
that exact record. A subject, literal mention, assertion link or genealogy
payload does not acquire canonical targets. Acquisition does not bypass the
target's own scope or applicability. See [NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md).

Each transition requires a nonblank unique `id`, `time`, `state`, `acquisition`
and finite `confidence` between zero and one. States are `accepted`, `suspected`,
`rejected`, `uncertain`, `remembered`, and `forgotten`. Acquisition is `authored`,
`experienced`, `heard`, `inferred`, `observed`, `read`, `remembered`, `tested`, or
`told`. Optional fields are `source_entity`, `causing_event`, and `note`.
Transitions strictly increase on one timeline. A cause must be canonical and
at or before the transition on that timeline.

A typed generational claim instead contains exactly `key`, `statement` and
`genealogy`; it cannot also carry plain claim truth/subject/object fields. The
world must explicitly declare `generational-core-v1` and
`generational-knowledge-v1`. The same create/update operation validates its
closed literal payload, inclusive validity interval, learned labels, affirmative
learning and exact admitted evidence. Nullable organization parents, tenure
holders and affiliation roles are domain values. Empty transitions, missing
capabilities or invalid references produce reviewable `canApply:false`
candidates; apply refuses them. No canonical history changes when a belief is
created or rejected. Requests 36–44 demonstrate all seven kinds and a rejection
followed by a new replacement claim. Keep the older record and its transitions
to preserve earlier beliefs. [The learned contract](GENERATIONAL_KNOWLEDGE.md)
defines the source forms and query behavior.

## Scene and conversation fields

Both kinds require an existing location and a complete time mapping when created.
A scene's time requires `start` and `current`, with optional `end`. A conversation
requires `start`, with optional `end`. The same exact tick/order rules apply.
Omitted collection fields become empty arrays on create; updates retain omitted
fields.

```yaml
kind: scene
id: scene_00000000000000000000000001
title: Harbor Watch
location: loc_00000000000000000000000001
time:
  start: {tick: 1, order: 0}
  current: {tick: 2, order: 0}
  end: {tick: 2, order: 0}
participants:
  - character: char_00000000000000000000000001
    role: harbor keeper
    point_of_view: true
```

This creates a planned scene. To make it contribute dated evidence, explicitly
set a supported status in a create request or later update:

```yaml
kind: scene
status: closed
```

Scene statuses are `planned`, `active`, `closed`, `abandoned`; conversation
statuses are `draft`, `active`, `closed`, `retired`. Active/closed records can
contribute evidence. A status edit is an authored correction, without a separate
transition engine or automatic event, knowledge, memory or cursor changes.

All statuses still validate their contents. Closed records require an end.
An active scene must have `time.current` equal to `world.current_time` when that
world cursor is present; multiple active scenes require a world cursor. `init`
sets it to tick 0/order 0. A later-time active scene therefore fails validation
until source declares a matching world cursor. An explicit
[current-time preview](CURRENT_TIME.md) plans the world and every active scene
cursor together through the existing reviewed consequence workflow.
The supplied later-time examples use closed scenes and conversations.

The separate [scene controls](SCENE_CONTROL.md) coordinate scene creation,
advancement, closure and character movement. They export one complete Plan with
inclusive presence handoffs, carried-object membership, shared clocks and explicit
movement events. These operations also reconcile active placement evidence;
ordinary single-record status edits retain their correction-only behavior.
[Conversation controls](CONVERSATION_CONTROL.md) additionally create an active
conversation and reciprocal scene link in one Plan, or append a speech/action
beat while preserving earlier rows and recollections. Explicit full times leave
clocks unchanged. These controls infer no knowledge or events.

Participants use exact character IDs, optional inclusive `from`/`until`, role,
and point_of_view. The old `to` spelling is rejected. Observations, speech/action
beats, audiences and recollections use the closed mappings in
[SCHEMA.md](SCHEMA.md). Missing audiences admit no recipients; authored speakers
and actors still perceive their own beats. Typed turn replacements retain the
already-valid optional `audience: null` spelling and speech `delivery: null` or
`addressee: null`; these supply no audience or target. Other typed optional fields
use omission, and tagged YAML remains rejected. Speech may contain `interrupts`,
an exact earlier local speech ID at a strictly earlier time; null is invalid.
Character transcripts expose that reference only when both beats are admitted.

Complete-candidate validation checks reference existence, interval ordering,
attendance, overlap, the world cursor and local recollection references. For
example, replacing `turns` while leaving a recollection that names a removed turn
returns `canApply:false`. Replace both arrays in one request to make a consistent
single-record edit. A status change that creates overlapping scene attendance
also fails without writing source.

Scene and conversation attendance/location remain independent. Their grouping
links need not be reciprocal: create the scene, create a conversation referencing
it, then optionally update the scene's conversation list. Each step must validate;
the ordinary record command writes one file and creates no automatic backlink.
Use a reviewed saved outcome for explicitly authored interdependent operations.
`topics`, `author_constraints` and `provenance` are not scene/conversation request fields;
existing values remain intact when omitted.

## Location parents and routes

Location `parent_id` names an existing location. Omission retains it on update;
removing it still requires a direct source edit. Complete-candidate validation
rejects self-parenting, cycles and a canonical child with a retired parent. The
legacy `parent` spelling is not accepted.

Routes require distinct location endpoints, `direction: one-way` or `two-way`,
and a nonempty array of unique modes. New records default to canonical status,
`availability: open` and `uncertainty: exact`. Availability also accepts `closed`,
`restricted` and `unknown`; uncertainty accepts `estimated` and `unknown`.
Only open canonical routes enter the source graph. Route status accepts
`canonical` or `retired` through the common status field.

Optional distance, cost and duration metrics contain exactly `value` and `unit`.
Values are finite nonnegative numbers; units are nonblank strings. Path queries
can minimize hops or sum one explicitly selected metric within a single exact
unit. Missing metrics supply no weighted edge. Map coordinates do not generate
route measurements; no units are converted.

Route creation requires the world's existing capability declaration to include
`spatial-core-v1` and `route-v1` in the established order. Declare these in an
explicit world update before the request, or include both in a consequence plan.
Missing capabilities give a reviewable invalid candidate. Examples 11–14 show
a child location, route creation, closure and one-way reopening. See
[SPATIAL.md](SPATIAL.md) for source rules, queries and limits.

## Maps and geometry placements

Map creation requires `crs`, `axis_order`, `unit` and closed
`bounds: {min: [...], max: [...]}` with matching dimensions and strict `min < max`
on each axis. Use `local-planar:NAME` with `[east, north]` and a nonblank unit, or
`EPSG:4326` with `[longitude, latitude]` and `unit: degree`. The geographic bounds
must keep longitude within [-180,180] and latitude within [-90,90]. Coordinates
are finite numeric values with absolute value at most 9007199254740991; geometry
calculations use f64.

Optional `z_policy` defaults to `forbidden`, requiring 2D bounds. `required`
requires 3D bounds. `optional-level` allows either dimensionality, fixed by the
map's authored bounds. Every placement and map position must use that same
dimensionality; a map does not mix 2D and 3D positions.

Optional `origin` contains exactly `label` and `coordinates`, with a nonblank
label and the same dimensionality as the map. Optional `scale` contains exactly
a finite nonnegative `value` and nonblank `unit`. These fields retain authored
metadata and do not alter coordinate calculations. For example:

```yaml
kind: map
id: map:harbor/levels
title: Harbor Levels
crs: local-planar:harbor-levels
axis_order: [east, north]
unit: m
z_policy: required
bounds: {min: [-10, -10, 0], max: [10, 10, 5]}
origin: {label: Dock level, coordinates: [0, 0, 0]}
scale: {value: 1, unit: m}
```

Location `spatial` contains exactly `map_id` and `geometry`; geometry contains
exactly `kind` and `coordinates`. A point takes one position, a line takes
2–10,000 vertices, and a polygon takes 4–10,000 vertices with the first repeated
as the last. All vertices have one dimensionality. This polygon is valid for a
2D map whose bounds contain the shown coordinates:

```yaml
spatial:
  map_id: map:harbor
  geometry:
    kind: polygon
    coordinates: [[0, 0], [10, 0], [10, 10], [0, 0]]
```

Whole-candidate validation checks exact map references, dimensions and inclusive
bounds for every vertex. A canonical location requires a canonical map. Map
updates also recheck existing placements, anchor endpoints and portal positions.
An invalid candidate remains previewable and cannot publish.

Map/placement declarations require explicit world capabilities `spatial-core-v1`
and `geometry-v1`; declare them through an explicit world update. With
routes, use `[spatial-core-v1, geometry-v1, route-v1]` in that order. Examples
15–18 create a map, place two locations and expand bounds; 45–50 add line/polygon,
3D and geographic examples. Placement omission preserves it; supplied placement
replaces the complete value. Removing it requires a direct source edit. Map
status accepts `canonical` or `retired` through the common status field.

Bbox queries compare geometry bounding envelopes, and nearby queries use
coordinate-space Euclidean distance. Geographic values remain degrees. These
operations do not derive geodesic distances, travel edges or route weights.

## Literal map anchors

Anchor creation requires `from` and `to`, each a closed
`{map_id: MAP_ID, coordinates: [...]}` position. The two exact map IDs must differ.
Each position uses its map's dimensions and lies within its inclusive bounds;
the endpoint maps may have different dimensions. Optional `conversion` is a
literal untagged string, including an empty string. For example:

```yaml
kind: anchor
id: anchor:harbor/survey
title: Harbor Survey Correspondence
from: {map_id: map:harbor, coordinates: [0, 0]}
to: {map_id: map:harbor/coast, coordinates: [12.5, 55.7]}
conversion: These two authored survey positions identify the harbor.
```

Anchors default to canonical status and accept `status: canonical` or `retired`
on creation and update. Canonical anchors require canonical endpoint maps.
Explicit `spatial-core-v1` and `geometry-v1` capabilities are required. Queries
return the authored pair and prose in its declared direction; they do not
execute a coordinate transform, interpolate, create an inverse or add travel
edges. Request 51 supplies the complete example after the maps exist.

## Directed portals

Portal creation requires an exact `from_location_id`, `to` and nonempty sorted
unique `modes`. The target is either a distinct exact location ID or a closed
`{map_id: MAP_ID, coordinates: [...]}` position. All modes are nonblank strings.
A canonical portal requires canonical source/target locations or a canonical
target map; position targets also obey map dimensions and inclusive bounds.

```yaml
kind: portal
id: portal:harbor/watchtower
title: Watchtower Entrance
from_location_id: loc_00000000000000000000000001
to: location:harbor/watchtower
modes: [foot]
```

Portals default to canonical status and accept canonical/retired status edits.
They require explicit `spatial-core-v1` and `route-v1` capabilities. The record
is directed; author a separate reverse portal when needed. Portal requests do
not accept route fields such as `direction`, `to_location_id`, `availability`
or metrics. Hop paths and reachability follow location-target portals. Position
targets are terminal authored positions and never snap to nearby locations.
Weighted paths continue to use only routes carrying the selected explicit
metric. Requests 52–53 create both target forms; 56 retires a portal.

## Scoped overlay membership

Overlay creation requires `lifecycle`, closed
`membership: {location_ids: [...]}`, `audience` and `perspectives`. Membership is
a sorted unique array of exact existing location IDs. Audience and perspectives
are sorted unique arrays of nonblank author-defined presentation labels. Empty
arrays are allowed. Canonical overlays require canonical member locations.

```yaml
kind: overlay
id: overlay:harbor/flood-warning
title: Harbor Flood Warning
lifecycle: time-bounded
membership:
  location_ids: [loc_00000000000000000000000001, location:harbor/landing]
audience: [author]
perspectives: [author]
valid:
  start: {timeline: main, tick: 10, order: 0}
  end: {timeline: main, tick: 12, order: 0}
```

A static overlay must omit `valid`. A time-bounded overlay requires both closed
`start` and `end` positions, explicitly containing `timeline`, signed i64 integer
`tick` and signed i32 integer `order`. Bounds are inclusive, ordered and on one
declared timeline. The typed request does not accept shorthand or null endpoints.
Overlay status defaults to canonical and accepts canonical/retired edits.
Explicit `spatial-core-v1` and `overlay-v1` capabilities are required. A world
using these static spatial features declares
`[spatial-core-v1, geometry-v1, route-v1, overlay-v1]` in that order, retaining
other capabilities in their established positions.

Overlay queries match the requested audience and perspective labels before
membership, time and result bounds. These filters select author presentation;
they confer no character knowledge or entitlement. Adding valid maps,
placements, anchors, portals and overlays leaves complete generic character
context and search responses unchanged.

Requests 54–55 create static and time-bounded overlays; 57 replaces the dated
overlay's full validity window. Updating `membership` or `valid` replaces its
whole mapping. To change a bounded overlay to static, remove `valid` with a
direct source edit; supplying null or changing lifecycle alone does not remove
the old field. Top-level chronology annotations remain separate from explicit
spatial history. See [SPATIAL.md](SPATIAL.md) for all query boundaries.

## Spatial history

Location and route creation/updates accept a complete `spatial_history` array.
Omission preserves the field; a supplied array replaces it, and `[]` clears it.
Top-level null is refused. The existing preview/hash/apply flow validates the
complete candidate and preserves omitted static fields, metadata and prose.

Declare `spatial-core-v1` and `spatial-history-v1` explicitly. Placement/geometry
members additionally require `geometry-v1`; route members require `route-v1`.
For a world using all spatial features, retain the registry order:
`[spatial-core-v1, geometry-v1, route-v1, overlay-v1, spatial-history-v1]`.

Each of at most 1024 ordered members has exactly `id`, `aspect`, `valid` and
`value`, with optional `replaces`. IDs use `spatialhistory_` plus 26 uppercase
Crockford characters, unique within the owner. A location can author `placement`
or `geometry` with a complete existing map/geometry value, or `parentage` with
exactly `parent_id` (an existing location ID or null for root). A route can author
`route-availability` or `route-metric`; a metric member names exactly one of
`route_distance`, `travel_cost` and `duration`, retaining its explicit value/unit.

```yaml
kind: location
spatial_history:
  - id: spatialhistory_00000000000000000000000001
    aspect: placement
    valid:
      kind: interval
      timeline: main
      start: {timeline: main, tick: 20, order: 0}
      end: {timeline: main, tick: 29, order: 0}
    value: {map_id: map:harbor, geometry: {kind: point, coordinates: [8, 9]}}
```

`valid` is either exactly `{kind: timeless}` or an interval on a declared
timeline. Interval bounds are inclusive; omit either or both bounds to leave
them open. A supplied bound must contain exactly `timeline`, signed i64 `tick`
and signed i32 `order`, with the same timeline as the interval. Null bounds and
reversed intervals are invalid.

To correct a member, retain it and append another with `replaces` naming that
earlier local ID. The new member must keep the exact aspect, literal validity
declaration and metric target. Overlapping changes to the same effective field
must form one correction chain; competing branches are refused. Placement and
geometry share the same field. Candidate validation also checks containment
cycles when history starts or ends, including fallback immediately after an
inclusive endpoint. Invalid references, corrections, overlap or cycles cannot
publish.

Static source is the fallback outside applicable history; absent metrics remain
absent. Dated history, route inspection and geometry queries use the explicit
horizon. Ordinary paths, reachability and containment inspection stay static.
These author histories do not admit character evidence. Requests 58–62 retain
placement and route corrections; request 63 explicitly clears the landing's
history while retaining its static placement and parent.

## Canonical generational histories

Parentage requests use `kind: parentage`, a `kinship_` ID and exact canonical
character `child_id`/`parent_id` endpoints. Union requests use `kind: union`, a
`union_` ID and sorted unique `participant_ids`. Both require an initialization
object and default to canonical status with `transitions: []`. Explicit statuses
are `canonical`, `draft` and `retired`; all validate, while only canonical source
supplies author family facts.

Organizations, affiliations, legacies, tenures, claims and vital histories use
the same initialization/transition/status contract. Their required fields are
listed above. Optional references are exact canonical IDs of the declared kind;
their top-level removal remains a direct source edit. Organization and legacy
history payloads carry historical titles/aliases independently of the current
source header. Editing the header title does not rewrite history.

Initialization/transitions use the closed instant, payload and lifecycle rules
in [GENERATIONAL.md](GENERATIONAL.md). Every point names its timeline, tick and
order. Confirmation/reconciliation, endings, strict earlier causes and explicit
same-instant corrections are checked against the complete candidate world.
Cycles, duplicate canonical edges, invalid rosters and orphaned references give
reviewable `canApply:false` candidates and cannot publish.

Affiliation overlap is checked for the same character and organization,
including shared start/end points. Tenure links require the same legacy and
strict initialization order; transfers require an eligible existing target and
earlier cause. Claims remain separate from holders and competing claim evidence
must be reciprocal at the same applicability. Unknown vital source cannot retain
affirmative transitions; withheld source can retain them while typed reads
conceal their values. These rules apply during ordinary candidate validation.

Supply a complete `transitions` array to append or correct history; the writer
does not append automatically or merge nested values. Omission preserves the
existing history, and `transitions: []` removes its later entries while retaining
initialization. Other fields, body, chronology and existing metadata remain
unchanged when omitted. Chronology belongs on the record, not inside the history.

Declare `generational-core-v1` through a world update before creating either
kind. After requests 11–18, adding it gives the ordered capabilities
`[generational-core-v1, spatial-core-v1, geometry-v1, route-v1]`; retain any
additional declarations in their established order. Refresh the fingerprint
after this edit. Requests 21–26 demonstrate creation and endings;
neither a union nor its edits imply another parent or a learned belief.

Requests 27–35 add the remaining six kinds, then rename the organization, vacate
a tenure over an inclusive interval and end an affiliation. Null payload values
have specific meanings: `role:null` is an unspecified initial role,
`holder_id:null` is a permitted vacancy, and `parent_id:null` in a reparent
transition explicitly detaches the organization. Other null positions remain
invalid. A vacancy expires after its last exact point without inferring a new
holder or a persisted expiry transition.

The writer still saves one record at a time. A reciprocal claim pair cannot be
published as two invalid intermediate edits; either preview is refused until
its complete source is consistent. Author both operations in one complete
saved proposal, then use [reviewed outcome publication](OUTCOMES.md).

## Structured lore profiles, concepts and links

Declare `lore-core-v1` explicitly through a world update before adding profiles,
concepts or links, then refresh the source fingerprint. The writer does not
enable capabilities automatically. Use `kind: world` in a `record update`
request to replace the complete `profiles` array. Each entry contains exactly
`id`, `label` and `fields`, where `fields` is a closed object schema:

```yaml
kind: world
profiles:
  - id: harbor-note
    label: Harbor note
    fields:
      type: object
      required:
        description: {type: string}
      optional:
        origin: {type: 'null'}
        marker: {type: enum, values: [1, '1', null]}
        place: {type: entity, kinds: [location]}
```

The eight types are string, boolean, number, null, entity, enum, array and
object. Object schemas always include both `required` and `optional` maps.
Arrays declare `items` and `max_items`. Types do not coerce data: the enum above
distinguishes numeric `1`, string `'1'` and null. Quote `type: 'null'` so YAML
retains the type name as a string. Actual null values remain null and are legal
only where declared by the selected type. Tagged YAML, undeclared fields and
nonfinite numbers are refused. [LORE.md](LORE.md) defines the exact grammar and
bounded nesting rules.

Concept creation requires a `concept_` ID with 26 uppercase Crockford characters,
an exact declared profile ID, complete `values`, a dotted `domain`, sorted unique
`audience` and `perspectives` labels, and explicit validity. For example, after
declaring the profile above:

```yaml
kind: lore-concept
id: concept_00000000000000000000000001
title: Harbor Note
domain: harbor.custom
profile: harbor-note
values:
  description: A literal authored harbor custom.
  origin: null
  marker: '1'
valid: {kind: timeless}
audience: [author]
perspectives: [author]
```

Typed entity leaves use exactly `{kind: record, id: RECORD_ID}` and must name
an existing record of an allowed kind. Literal strings that resemble IDs do not
become references. Hypotheses are excluded from this reference domain. Ordinary
record targets can have any valid status; concept targets must be admitted by
the selected author filters to appear in typed query output. `provenance` is an
optional literal string. Status defaults to canonical; draft and retired
concepts retain valid source but are unavailable to semantic lore reads.

Every supplied profile array or concept `values` mapping replaces the whole
field. Omission preserves it. Preview validates the complete candidate world,
including every concept that depends on a changed profile, for all statuses.
Removing a required profile or changing a type incompatibly returns an invalid
candidate that cannot be saved. `profiles: []` is valid only when no concepts
or state bindings still require those definitions. Adding an optional field permits later values
but generates no defaults and changes no existing concept bytes. Profile IDs
name current definitions: there are no profile versions, hashes, migrations or
automatic dependent rewrites.

All twenty-seven non-world kinds other than hypothesis, and world updates, accept a complete
`concept_links` array. Its closed members contain `relation`, `concept_id` and
`valid`, for example:

```yaml
kind: character
concept_links:
  - relation: associated-with
    concept_id: concept_00000000000000000000000001
    valid:
      kind: interval
      timeline: main
      start: {timeline: main, tick: 12, order: 0}
      end: {timeline: main, tick: 16, order: 0}
```

Relations are lowercase tokens and targets are exact existing lore concept IDs.
Concept and link validity is either `{kind: timeless}` or an interval on one
declared timeline. Each supplied endpoint includes timeline, signed i64 tick
and signed i32 order, with inclusive same-timeline bounds. Omitted endpoints
are open; null endpoints are invalid. Omission retains the entire link array;
`concept_links: []` explicitly clears it. A link remains authored source when
its target is retired, but that target is omitted from scoped lore output.
Hypotheses cannot own links.

Requests 64–71 declare two profiles, create two concepts, attach Ana's dated
and timeless links, replace a concept's values, extend a profile with an optional
field, retire a symbol and clear Ana's links. They reuse the same reviewed
single-record writer. Lore reads remain author presentation operations; these
edits grant no character knowledge, traits, abilities, membership or authority.
Restart the snapshot server after saves to use the Lore tab.

## Profile-bound state and species descriptions

World updates can replace the complete `state_keys` and `state_profile_bindings`
maps, independently or in the same request as `profiles`. Bindings require
`lore-core-v1`, even when the supplied map is empty. Each binding names a known
non-world, non-hypothesis record kind, an existing state key and an exact current
profile ID:

```yaml
kind: world
state_keys:
  character:
    mood: {type: string}
    species_notes: {type: string}
state_profile_bindings:
  character:
    species_notes: character-description-state
```

The referenced profile must already exist or be supplied in the same world
request. The profile's object type replaces the ordinary leaf type for that
key; the key declaration and any `exclusive_group` remain required. Unbound
keys retain their previous rules. No new placeholder state type is introduced.
These maps replace complete fields, so retain any other declarations or bindings
that the world still needs.

Character/location/object `initial_state` edits and event `set` effects accept
the bound profile's entire typed object. Nested arrays, declared nulls and exact
integer values retain their type. `clear` removes the state key at its event
coordinate; it takes no value and does not delete source history. Clearing a
current value does not make its initial or historical values exempt from later
profile validation. All event statuses validate, including draft, cancelled and retconned source.

World definition edits check every existing initial value, set effect, state
predicate and binding. Incompatible removal or replacement produces a candidate
that cannot publish. Compatible optional additions do not rewrite any character,
event or concept. Bound values support whole-object set/clear only; array updates
require a complete replacement object.

`lore state RECORD --tick T` reads this state explicitly, with exact order and
timeline options plus author audience/view filters. Ordinary `state` and author
context omit bound values and their citations. The lore state read retains the
last clear citation at `effects[N]` without inventing a value. Raw author source
remains inspectable; generic character responses remain unchanged.

Species/common/individual descriptions use ordinary lore profiles and concept
edits. Authors choose the common and individual profiles, reference fields and
display fields explicitly. Common edits change subsequent composition reads but
never rewrite individual values or bound state. No source kind, automatic
inheritance or new publication workflow is introduced. Requests 72–79 demonstrate
the world definitions, linked common/individual concepts, initial state, a set
event, a common-description edit and a clear event. The complete sequence has
79 requests and 47 source records at this checkpoint. See [LORE.md](LORE.md) and
[SPECIES.md](SPECIES.md) for the query and projection rules.

## Authored magic composition

Magic roles use ordinary current profiles and lore-concept creates/updates, with
the same `lore-core-v1` capability and reviewed writer. Request 80 replaces the
complete profile array, retaining all five earlier definitions and adding four
magic profiles. Omitted chronology, state declarations and bindings remain
unchanged. Requests 81–83 create a tradition, realm and artifact; 84–86 create
three separately dated practices. Request 87 replaces the tradition's values
without rewriting the practice source, bound state or character evidence.

Role assignment, typed link fields and independent restriction/exception fields
are query selections, not extra source metadata. Profile edits still validate
every dependent record; a change that turns a literal Ref-shaped object into an
unresolved entity reference cannot publish. Missing fields, declared nulls and
empty arrays remain distinct and supply no inferred precedence or outcome.

The first **87 requests leave 53 source records**. Their three practice
intervals are main ticks 0–9, 10–19 and 20–29 with inclusive order-zero endpoints.
The common tradition edit affects current composition at each horizon while
preserving all three practice files. This is an explicit source revision, not a
new dated event. See [MAGIC.md](MAGIC.md) and the
[worked sequence](../examples/authoring/README.md) for exact selections and reads.

## Dated descriptions and classified assertions

`descriptions` is an optional common array on every editable kind and the world.
Declare `descriptions-v1` before supplying it, including an empty array. This
capability works independently of lore. Each closed member contains a stable
owner-local `description_…` ID, heading, exact Markdown, validity, audience and
perspectives. Optional `replaces` names an earlier member with exactly the same
validity and labels. A supplied array replaces the whole array; omission preserves
it and `[]` clears it. The record body stays separate and unchanged unless the
request also supplies `body`.

World `predicates` uses the same complete-field update. Each declaration has
exactly `id`, `subject_kinds` and `value_type`; it requires `lore-core-v1` and uses
the existing finite type grammar with any root type. Assertions additionally
require `assertions-v1`. Declaration edits validate every dependent assertion,
including draft, retired and historical source. Compatible optional additions
do not create default values or rewrite records.

Create an assertion with an exact typed `subject: {kind: record, id: …}`, a
declared predicate and typed `value`. Classification is exactly `author-fact`,
`attributed-claim` or `unresolved`. Only an attributed claim supplies
`attributed_to`, which resolves to a character, organization or lore concept.
An unresolved assertion still supplies a value of its declared type, including
an explicit null when that predicate allows it.

`replaces` names an existing assertion with exactly the same subject, predicate
and validity. Replacement cycles fail validation; independent corrections of
one predecessor remain alternatives. Changing a source claim does not teach a
character, revise other records or create an event. Historical `evidence`
citations are closed syntax and need not resolve in the current source. Their
revision and blob IDs are not authenticated by a save or read.

Requests 88–97 declare predicates, add and correct description sections, create
all three classifications, add two alternative assertion corrections, attach a
description and chronology annotation, and extend a predicate. That checkpoint
leaves 58 source records. See
[DESCRIPTIONS.md](DESCRIPTIONS.md), [ASSERTIONS.md](ASSERTIONS.md) and the worked
examples. Character section admission and linked `knowledge.claim.assertion_id`
are implemented in the later worked sequence. Private manuscript source and work
order are covered in [MANUSCRIPT.md](MANUSCRIPT.md); reveal/reader workflows and
work-bound Writing context remain planned.

## Social and environmental context source

Requests 98–125 preserve the previous nine profiles and three predicates while
adding fifteen current family profiles and three predicate definitions. The two
JSON packs under `examples/profiles/` are source examples: copying one does not
install capabilities or replace existing declarations automatically. For an
existing world, retain its current declarations when preparing a complete world
update. Publish referenced concepts before their dependent records.

Ordinary concept creation authors languages, scripts, senses, registers, dated
spellings, explicit memberships and all seven environment roles. Descriptions
and assertions continue to use their existing source fields and reviewed writer.
A doctrine correction leaves the earlier assertion file intact. Context queries
are reads; their projected results and edit hooks are not replacement source or
publication instructions. Shared source edits preserve neighboring records and
character learning.

The **125-request checkpoint leaves 85 source records**, with 24 profiles
and six predicates. See [SOCIAL.md](SOCIAL.md), [ENVIRONMENT.md](ENVIRONMENT.md)
and the [worked sequence](../examples/authoring/README.md).

## Existence episodes and governance source

`existence-episode` creation requires `existence-v1` and `generational-core-v1`.
The exact `character_id` must identify a current character. `episode_kind` is
`embodied`, `disembodied`, `constructed`, `other` or `unknown`. Validity must be
an interval with an explicit timeline and optional exact native endpoints;
`timeless` and null endpoints are invalid. The `unknown` kind must be wholly
unbounded with empty start evidence and no end evidence. Every affirmative kind
requires nonempty `start_evidence`, even with an unbounded start. A bounded end
requires nonempty `end_evidence`; an unbounded end forbids that field.

Evidence arrays use the same closed historical pointer grammar as assertions:
revision, safe relative path, blob OID and record/section/transition target. These pointers
are syntax-checked and remain unverified historical source. Missing current
historical targets do not invalidate them. They never become authenticated
citations or alter character learning, vital histories or computed age.

Omitted fields preserve source, supplied arrays/maps replace completely, and
common descriptions, concept links and chronology retain their existing rules.
All canonical/draft/retired episodes validate, while only admitted canonical
episodes appear in reads. Changing a bounded end to an unbounded end also
requires removing `end_evidence`; top-level optional removal remains a direct
source edit, so neither omission, `[]` nor null performs that removal.

Governance uses ordinary lore concepts under a selected current profile, plus
optional native assertions. No separate policy record kind or writer is added.
`collective-parties-v1` requires both lore and generational core capabilities.
Source entity fields keep ordinary `{kind: record, id: ...}` references. The
public Party tags (`character`, `organization`, `concept`) describe projections;
they are not accepted as replacement entity values, organization parents, tenure
holders, claim claimants or interpersonal endpoints. The six-role query binding
does not rewrite the profile or supply missing fields.

Requests 126–142 preserve the previous declarations, add the `harbor-policy`
profile and three predicates, then create six independent episodes, four policy
accounts and five assertions. A common episode update adds a dated description
and calendar annotation. The **142-request checkpoint leaves 100 source
records**, with **25 profiles and nine predicates**. See [EXISTENCE.md](EXISTENCE.md),
[GOVERNANCE.md](GOVERNANCE.md) and the worked sequence for exact read contracts.

Requests 143–158 add two scoped public concepts, dated descriptions and a
correction, independent linked learning with rejection/forgetting, private and
unlearned alternatives, explicit record/map acquisitions and spatial history.
They exercise existing reviewed source fields and the new closed linked claim;
the **158-request checkpoint leaves 112 source records**, still with
**25 profiles, nine predicates and 23 editable non-world kinds**.

### Context briefs and writing frames

`context-brief` was added as the 24th editable non-world kind and requires `briefs-v1`,
independently of lore. Creation requires explicit domain, scope, validity,
audience, perspectives and `required_refs`; an empty pin array is valid, while
omission or null is not. Scope is world, region or era. World forbids `scope_ref`,
region names an exact location/overlay record, and era names an exact era section
in the world's chronology declaration. See [BRIEFS.md](BRIEFS.md).

Pins are closed record or owner/member references. Every selected owner and
member must exist in the complete candidate, even if its source is draft or
outside the current horizon. Deleting a pinned member through another record
edit therefore refuses publication. Required arrays replace as a whole; omission
preserves them. Duplicate references within one array are invalid. Nested briefs
may form cycles because composition deduplicates exact identities.

Brief bodies are explicitly authored, scoped material with their own validity.
No generated summary, implicit learning or truth reconciliation is stored.
The ordinary preview/fingerprint/hash/save workflow preserves other fields and
neighboring records. Switching scope in a way that removes `scope_ref` remains
a direct source edit because generic updates do not remove optional fields.

Requests **159–175** add six briefs, six acquisitions and one scene, plus four
updates for object description, pins and brief prose. The **175-request checkpoint leaves
125 records**, with the same **25 profiles and nine predicates**. Exact writing
frames are operational JSON selections, never source records. A frame does not
authorize publication or move a world/scene clock; an edited snapshot requires
explicit recapture. See [WRITING_FRAMES.md](WRITING_FRAMES.md).

### Family evidence and capture

Requests **176–203** use existing concept scopes, dated sections, assertion
corrections and ordinary/linked knowledge. They add 14 learning records and two
canonical assertions, plus 12 reviewed concept updates. The **203-request checkpoint leaves
141 records**, with **25 profiles, nine predicates and 24 editable non-world
kinds**. No new source kind, field, profile or capability is needed.

Publication and acquisition remain separate operations: widening a concept's
labels does not teach it, and an acquisition does not bypass the target's labels
or validity. Examples preserve a later-learned common description, later-learned
script and livelihood, forgotten endpoints and three literal mistaken reports.
Native family reads attach only admitted dated evidence. Canonical correction
of a magic account leaves the linked character report unchanged.

Writing `families` is an optional closed operational selection, separate from
the six-field frame and authored Markdown. Its belief targets select explicit
learned streams, without joining canonical assertion subjects or classifications.
The portable [family selection](../examples/writing/family-context.json) works
with the completed sequence and an explicitly captured main 12/0 frame.
Reading it performs no source edit or cursor change. See [SPECIES.md](SPECIES.md),
[MAGIC.md](MAGIC.md), [SOCIAL.md](SOCIAL.md), [ENVIRONMENT.md](ENVIRONMENT.md) and
[WRITING_FRAMES.md](WRITING_FRAMES.md).

### Retrieval prior and independent recall

Requests **204–207** replace the world's description array and author body, then
create one linked learned report, one object with dated sections and one
independent acquisition. The complete **207-request sequence leaves 144 valid
records**, retaining **25 profiles, nine predicates and 24 editable non-world
kinds**. They use the existing single-record preview/hash/apply flow.

The public, ordinary, timeless world sections supply a fixed LSA training prior.
Its private and interval sections do not train. Training does not teach the
world record or produce a hit; the character still needs independently admitted
recall targets. The linked report starts at 20/0, remains held after rejection
at 22/0 and is forgotten at 26/0. The notice is separately acquired at 21/0 and
forgotten at 24/0, independently of its section's 20–29 validity. Earlier
character context remains unchanged. Canonical edits never rewrite the report.

Search modes are operational selectors, not authored source fields or new
capabilities. Changing a selected public prior may change semantic relevance;
private/timed source changes cannot retrain it. Results preserve source words
and expose relevance scores separately from authored confidence. See
[RETRIEVAL.md](RETRIEVAL.md) and the
[worked commands](../examples/authoring/README.md#local-semantic-and-hybrid-retrieval).

## World chronology

Use `kind: world` in a `record update` request to replace its complete
`chronology: {calendars: [...], eras: [...], anchors: [...]}` declaration. All
three arrays are required. The existing source path, other world metadata and
body are preserved. An unchanged declaration retains exact source bytes,
including explicit null epochs and extension metadata.

The normal preview validates the candidate calendars, era bounds, epochs and
anchor consistency together. Invalid declarations return `canApply:false` and
cannot publish. No separate writer, capability or inferred cursor change is
introduced. To clear the catalogue, supply all three empty arrays; null is not
a field-deletion operation. World `current_time` also has a typed update and a
coordinated [shared cursor plan](CURRENT_TIME.md). The common/world declarations
listed above use this same typed update path.

See [CHRONOLOGY.md](CHRONOLOGY.md) for the source/query contract and request 19 in
the examples for a complete calendar conversion and anchor workflow. Source
annotations validate together with the declarations they reference; a calendar
edit that would invalidate a saved date cannot publish.

## Record chronology annotations

All twenty-seven non-world kinds other than hypothesis accept a `chronology` array on creation and
update. Each entry requires `id`, `provenance` and exactly one tagged `value`:
civil, era, range, approx, conflict, relative or duration. Optional `role` and
`display` describe the claim. Stored calendar/era/record references use exact IDs.
Partial dates and uncertainty retain their authored meaning; neither an edit nor
a query creates a story event, belief or new anchor.

Omit the array to retain it, replace the complete array to edit it, or supply
`chronology: []` to clear it. `chronology:null` is not a deletion operation.
Nulls inside range/approximation bounds are allowed data. Invalid values,
references, bounds and normalization are reviewable `canApply:false` failures.
Unchanged requests retain exact original bytes, including supported extension
metadata. Submitted tagged YAML is still refused by the typed request boundary.
Chronology annotations belong on the source record, including maps, anchors,
portals and overlays. Nested operational objects such as effects, turns,
transitions, map bounds/origin/scale, geometry, anchor endpoints, portal
positions, overlay membership/validity, and existence validity/evidence mappings
cannot own an annotation array; candidate validation rejects them. A declared
state key, metric name or opaque
metadata key named `chronology` remains ordinary author data. Lore profile field
names and typed `values` may also use `chronology` literally; that data follows
the profile schema and does not become a record annotation.

Request 20 demonstrates all seven forms and opens an inclusive range. It follows
the declarations in request 19 and updates Ana Reed. Its executable test checks
preview/save byte equality, relation results and preservation of other source.

## Save boundary

The shared writer validates a complete candidate world, checks the expected
source fingerprint and candidate hash, acquires the WEDL OS writer lock, and
creates or replaces one file. Creates cannot overwrite an occupied path.
Updates preserve file permissions. No-op saves perform no writes.

New records use the kind's directory under `story/`: `characters`, `locations`,
`objects`, `events`, `knowledge`, `environments`, `relationships`, `story-points`,
`hypotheses`, `scenes`, `conversations`, `routes`, `maps`,
`anchors`, `portals`, `overlays`, `concepts`, `assertions`, `organizations`, `kinships`, `unions`,
`affiliations`, `legacies`, `tenures`, `claims`, `vitals`, `briefs` or `existence`. Portable prefixed IDs
use `ID.md`. Namespaced location, route, map, anchor, portal and overlay IDs use
`<kind>-<full-SHA256-of-ID>.md` in the corresponding directory to avoid unsafe
filename characters. Updates retain their existing paths. Directories needed
by a new record are created during apply. A failed filesystem operation can
leave empty directories or the lock file; it does not publish a partial record.

Git staging/commits remain explicit. Search caches rebuild on demand. Current
author sessions capture fresh source; use **Refresh source** after external edits
and explicitly capture new pins when needed. Historical snapshots are read-only.
See [BROWSER_PUBLICATION.md](BROWSER_PUBLICATION.md) for reviewed action/edit
publication through the existing writer.
External editors must coordinate with the writer; the optimistic checks are
not a filesystem compare-and-swap. This single-file path is distinct from the
bounded [saved-outcome publication and recovery contract](OUTCOMES.md).

Unlisted world policies, top-level optional-field removal, record deletion and
unlisted record fields remain direct source edits. See the 207 requests under
`examples/authoring/` for a complete small workflow starting from `init`.
