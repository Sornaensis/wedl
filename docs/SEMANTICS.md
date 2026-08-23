# Narrative Semantics

This document defines the provisional meaning of the source records. It is intentionally stricter than a generic wiki because character-grounded retrieval depends on predictable distinctions between canon, belief, observation, and author machinery.

## 1. Shared concepts

### Entity

An entity is one versioned Markdown record with:

- A stable typed ID.
- Exactly one entity kind.
- A required schema version.
- A human title.
- One dotted organizational domain.
- Optional tags and aliases.
- Entity-specific frontmatter.
- A Markdown body.
- Tool-managed provenance.

Paths and titles may change. IDs do not.

### World revision

A world revision is the managed source tree at one Git commit. “Current” without a fictional-time qualifier means the current selected Git revision. Branches are alternate world histories or authoring experiments.

### Story time

Story time orders facts inside the fiction. It is a unitless ordinal tuple,
not a duration, clock, or calendar:

```yaml
timeline: main
tick: 120
order: 10
```

Only `(timeline, tick, order)` controls ordering. `tick` is a signed 64-bit
integer and may be negative; `order` is a signed 32-bit integer that
deterministically orders multiple facts at the same tick. Git timestamps and
file modification times never stand in for story time.

Each world declares a default timeline and one or more timeline labels. A
timeline may optionally declare an origin with a `tick` and display `label`.
That origin is descriptive, not a lower bound: a valid event may occur before
it. For example, an origin at `main:0:0` can coexist with an event at
`main:-20:0`:

```yaml
default_timeline: main
timelines:
  - id: main
    label: Campaign chronology
    origin:
      tick: 0
      label: Arrival at Harrowcross
```

Intervals with both endpoints—such as scene, participant, observation,
conversation, and environment intervals—are inclusive at both ends. A
participant present from `main:-20:0` through `main:5:0` is present at both
of those moments. WEDL defines no tick-to-duration or tick-to-date conversion;
keep calendar dates, uncertain chronology, and elapsed-time narration in
authored prose.

### Current scenes and the shared cursor

A scene is canonically active when its source status is `active`. A world may
have several active scenes on its one canonical timeline—for example, when a
party splits. Two or more active scenes require an explicit world cursor:

```yaml
current_time: {timeline: main, tick: 210, order: 0}
```

Every active scene's `time.current` must equal that cursor exactly. A character
may be present in at most one active scene at the cursor, and active scene
objects may not be listed by two fronts. A singleton active scene may omit
`current_time` for compatibility; its `time.current` remains the effective
cursor. If a world does declare `current_time`, its active scene must agree.

A generic query with several active scenes must select a scene explicitly. A
character query may infer the character's unique active scene; otherwise it
fails as ambiguous. It never silently uses the latest event.

### Canon and draft state

Most records have `status: draft | canonical | retired`. Some kinds refine this vocabulary. Draft records participate in author-mode queries but are excluded from character-mode context unless explicitly requested.

### Author hypotheses

A `hypothesis` is a non-canonical author record for an unresolved possibility.
It has an `open`, `adopted`, or `rejected` status; a statement, named subjects,
alternatives, and placement context. Placement may name a timeline, scene,
event, location, or prose context, but it does not assert a StoryTime,
ordering, duration, location, or state.

Hypotheses never participate in replay, canonical timeline/horizon views,
knowledge, causality, plot continuity, whereabouts, or active-scene checks.
They are excluded from ordinary entity discovery and search unless an author
explicitly opts in. `adopted` records preserved author reasoning only: adoption
does not create canon; its supporting record must already be settled (a
canonical/retired entity or an active/closed/retired scene or conversation),
or be authored separately through the usual confirmed changeset flow.

## 2. Character

A character record defines relatively stable identity and author-facing material:

- Names, aliases, pronouns, and descriptive traits.
- Narrative role and domain.
- Initial state, such as starting location or condition.
- Author notes, voice guidance, goals, and boundaries.

A character file does **not** contain everything the character knows. It also does not define another character’s opinion of them.

Current mutable state—location, health, possession, disguise, availability—is derived from the character’s initial state plus ordered event effects.

## 3. Character knowledge

### 3.1 Knowledge item

A knowledge item belongs to exactly one knower and describes exactly one proposition. It contains an append-only sequence of epistemic transitions.

A proposition has:

- A required human-readable statement.
- A stable claim key within the repository.
- Optional structured `subject`, `predicate`, and `object` fields.
- Optional author-only truth status: `true`, `false`, `unknown`, or `contested`.

The same proposition may appear in knowledge items for several characters. The compiler normalizes its structured form and can group equivalent claims, but v0.1 does not require a separately authored Fact entity.

### 3.2 Epistemic states

The active transition uses one state:

- `accepted` — the character acts as though the proposition is true.
- `suspected` — the character considers it plausible but unsettled.
- `rejected` — the character acts as though it is false.
- `uncertain` — the character is aware of the proposition but has no settled stance.
- `forgotten` — the proposition is not available to ordinary character-context retrieval.

An optional confidence is a number from `0.0` to `1.0`. Confidence refines a state; it does not replace it.

“Accepted” is subjective. It does not assert canonical truth.

### 3.3 Acquisition

Every non-initial transition identifies how the character reached the new state:

- `observed`
- `told`
- `read`
- `inferred`
- `remembered`
- `dreamed`
- `fabricated`
- `other`

It should reference a causing event and may reference a source character, object, or document. Participation in the event is not sufficient; the transition is explicit.

### 3.4 Temporal resolution

For a query at story time `T`, the active epistemic state is the last transition whose time is not after `T`. Transitions must be strictly ordered within a knowledge item. A later transition may revise, reject, or forget an earlier belief.

Conflicting knowledge items may coexist. The engine does not reason them away. A context bundle may flag contradictions but must preserve the character’s authored state.

## 4. Event

An event represents an important canonical or proposed occurrence at one story-time key.

It may contain:

- Participants and their event roles.
- A location.
- Causes and consequences by entity ID.
- A Markdown account of what happened.
- Typed state effects.
- References to related scenes and story points.

Event participation is descriptive, not epistemic. Knowledge and relationship changes are stored in their own records and reference the event that caused them.

### Event lifecycle

- `draft` — planned or proposed; not part of character-visible canon.
- `canonical` — occurred in the selected world revision.
- `retconned` — retained for audit and links but excluded from current canon.
- `cancelled` — never occurred.

A canonical event should not be rewritten casually. A substantial correction creates a replacement event and marks the original `retconned`, preserving explicit fictional history in addition to Git history.

### Event effects

An event can apply schema-checked effects:

```yaml
- target: obj_...
  key: holder
  operation: set
  value:
    entity: char_...
```

Allowed keys depend on the target kind. v0.1 supports `set`, `clear`, `add-to-set`, and `remove-from-set`. Arbitrary JSON paths and arbitrary code are forbidden.

Effects at the same story-time position that assign incompatible values to the same target/key are validation errors unless one explicitly supersedes the other.

## 5. Object

An object is a persistent item or artifact with:

- Identity and description.
- Object type and tags.
- Optional initial holder, location, containment, condition, or availability.
- Optional capabilities and author-only notes.

Its current state is derived from initial state plus event effects. An object may be held by a character, placed at a location, or contained by another object, but not in more than one exclusive containment relation at the same story time.

Objects do not automatically reveal their contents or history to a nearby character. Such awareness is scene observation or character knowledge.

## 6. Location

A location is a stable spatial or conceptual place:

- It may have a parent location. A parent must be another location and cannot
  point back to itself.
- It may define directional exits or links to other locations. Each route
  targets one distinct location; its `description`, `label`, and `summary`
  remain authored directional prose and do not imply a reciprocal route.
- It may have a type, scale, coordinates, or map reference.
- Its body describes relatively stable features.

Locations form a containment graph. Parent cycles are invalid. Current occupants are derived from character/object state, not manually duplicated into the location file.

## 7. Environment

An environment is a time-bounded condition overlay applied to one or more locations or scenes. Examples include weather, lighting, noise, crowd mood, hazards, or magical conditions.

An environment is distinct from a location:

- The location answers **where**.
- The environment answers **under what current conditions**.

It defines a start and optional end story time, explicit targets, sensory details, and structured condition keys. A scene can also add temporary shared or private observations without creating a reusable environment entity.

## 8. Character relationship

All relationship records are directional: `from` character to `to` character. This avoids pretending that trust, fear, affection, obligation, or hostility are symmetric.

A relationship contains:

- A user-defined relationship kind.
- An append-only list of temporal transitions.
- A status such as active, ended, or unknown.
- Configured scalar metrics and qualitative facets.
- A causing event for each non-initial transition.
- An optional inverse relationship ID.

A helper command may create paired inverse records for a socially symmetric link, such as siblings or spouses. Subjective metrics remain independent.

Relationship metrics are defined in `story/world.md`, including minimum, maximum, and default values. Trigger expressions may compare only declared metrics.

A relationship is author model, not automatically character knowledge. Whether either participant knows or understands the relationship is represented through knowledge items.

## 9. Character interaction

An interaction is not a separate source kind in v0.1. It is an event with two or more character participants.

Interaction queries are derived by:

1. Finding canonical events containing the requested characters.
2. Ordering them by story time.
3. Joining relationship and knowledge transitions caused by those events.
4. Returning only perspective-safe summaries in character mode.

A later version may introduce a specialized conversation or interaction entity if event granularity proves insufficient.

## 10. Story point

A story point is a potential narrative beat with declarative prerequisites and a lifecycle. It is author machinery and is excluded from ordinary character-perspective context.

### 10.1 Lifecycle

```text
dormant -> eligible -> active -> resolved
                    \-> failed
                    \-> cancelled
```

- `dormant` — dependencies or trigger are not satisfied.
- `eligible` — its pure trigger is currently true.
- `active` — explicitly activated, or automatically activated by an opt-in policy.
- `resolved` — completed, with one or more outcome events.
- `failed` — no longer achievable in the intended form.
- `cancelled` — deliberately removed from consideration.

Eligibility is derived. Lifecycle transitions are authored and committed.

### 10.2 Dependencies

Hard dependencies refer to other story-point statuses. Cycles in hard dependencies are invalid. Soft references may be cyclic because they do not participate in eligibility.

### 10.3 Trigger language

The v0.1 trigger AST supports deterministic predicates over:

- Story-point state.
- Existence or status of an event.
- Active knowledge state and confidence.
- Relationship status or metric.
- Current entity state.
- Scene location, participant, point of view, or status.
- Active environment.
- Story-time comparisons.
- `all`, `any`, and `not`.

It does not support SQL fragments, Haskell expressions, regular process execution, model calls, randomness, or mutation.

### 10.4 Activation policy

The default policy is `suggest`: the engine reports eligibility but does not change source files. `manual` ignores eligibility for activation purposes but still displays it. `automatic` may create a normal activation transition through the command layer, with a commit and audit trail.

Activation never silently records the promised event as having occurred. It may optionally create a draft scene or event stub. Resolution must reference the actual canonical outcome events.

## 11. Scene

A scene is the working context for interactive writing. It contains:

- Status: `planned`, `active`, `closed`, or `abandoned`.
- Start story time and optional end story time.
- Location and optional active environments.
- Participants and point-of-view characters.
- Present objects.
- Linked active or eligible story points.
- Shared observations.
- Private observations keyed by character.
- Author-only notes and constraints.

Shared and private observations are immediate scene context. They do not become durable character knowledge until a knowledge transition records them, usually while closing or recording the outcome of the scene.

Several scenes may be `active` only as concurrent fronts sharing the world
cursor. They are not alternate timelines. An active scene's `time.current`
must equal `world.current_time`; a character cannot be physically present in
two fronts at that coordinate. Concurrent canonical state writes to the same
entity/key/coordinate are validation errors rather than an implicit ID-order
tie-break.

## 12. Perspective modes

### Author

May retrieve all canonical and draft data requested, including truth annotations, hidden story points, private notes, and other characters’ knowledge.

### Character

May retrieve only the selected character’s active knowledge and explicit scene perceptions. Author notes, hidden trigger logic, other characters’ internal states, and unknown canonical facts are excluded.

### Dramatic irony

Returns two separately labeled sections: character-accessible context and author-only context. The two sections must never be merged into an unlabeled block.

## 13. State resolution

Current state at time `T` is computed from:

1. The entity’s declared initial state.
2. Canonical event effects on the same timeline not after `T`.
3. Effect ordering by `(tick, order, event ID)`.
4. Explicit supersession where required.

The compiler materializes the latest state at `HEAD` and retains indexed transitions for arbitrary scene-time queries.

## 14. Validation invariants

The initial validator must enforce at least:

- Unique entity IDs and one entity per file.
- Valid kind-specific schema.
- All references resolve to compatible kinds.
- No location-parent or hard story-point dependency cycles.
- Transition order is monotonic within knowledge and relationship records.
- Event effects use declared keys and compatible values.
- Exclusive location/holder/containment state is not contradictory.
- Concurrent active scenes share an exact world cursor; no character or active
  object is double-booked at that cursor.
- Scene-presence intervals are checked across known history as well as at the
  live cursor. Endpoints are inclusive, so a character cannot leave one place
  and arrive at another at the same coordinate; the receiving presence must
  begin at a later order.
- Canonical event participation likewise cannot place one character at two
  different locations at the same coordinate. Multiple event records may
  describe the same character at one shared location.
- Character-private scene context names a participant or explicitly allowed observer.
- Character-context search cannot include author-only search documents.
- Story-point trigger references are resolvable and type correct.
- A `resolved` story point references at least one canonical outcome event.
- A knowledge transition’s causing event is not later than the transition.
- A mutation does not overwrite a newer `HEAD`.

## 15. Open semantic questions

These decisions should be revisited after the first end-to-end toy story:

1. Whether propositions deserve first-class source files rather than being embedded in knowledge records.
2. Whether canonical truth should be an explicit Fact entity instead of an optional proposition annotation.
3. Whether event duration and overlapping events are needed in v0.1.
4. Whether concurrent fronts need an optional author-named strand layer beyond
   their shared cursor, place, and cast projections.
5. Whether story-point automatic activation belongs in the first release.
6. How rich the relationship metric schema should be.
7. Whether draft records should be branch-local conventions or explicit statuses.
8. Whether retcons should use replacement events, event revisions, or both.
9. Whether scene observations should be independently addressable records.
10. How to represent uncertain or partially ordered fictional dates beyond numeric timeline ticks.
