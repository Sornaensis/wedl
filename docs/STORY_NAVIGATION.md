# Story and relationship navigation

These author reads use one source snapshot and exact `StoryTime` values. They
do not activate story points, infer event participation, teach a character, or
write canonical/private source. Times remain decimal strings in JSON.

## Story points and dated trails

```sh
wedl-rs --repo my-world story-points --tick 40 --order 2 --scene SCENE_ID
wedl-rs --repo my-world story-point STORY_POINT_ID --tick 40 --limit 1000
```

GET `/api/story-points` accepts `tick`, optional `order`, `timeline`, `scene`,
and `limit`. GET `/api/story-point` additionally requires the exact story-point
`id`. Native functions are `navigation::story::story_points` and
`navigation::story::story_point_trail`.

The list retains the canonical candidate order: descending priority, then
lowercased title and stable ID. The selected scene supplies predicate context;
its `story_points` list does not select or exclude candidates. Each item keeps
the stored lifecycle, eligibility, requirement/condition results and dated
outcome IDs. `plotTrail` adds:

- Every lifecycle transition at or before the horizon, including its exact
  member ID, time, state, optional note, and original array-index citation.
- Its named `causingEvent`, only when that independently admitted canonical
  event occurred at or before the transition. No nearby event is inferred.
- Declared `outcomeEvents`, sorted by exact time/title/ID, with named events,
  event times and citations to their original `outcome_events` positions.

The detail response returns one such item as `storyPoint`. Both reads include
the shared admitted-horizon `continuityAdvisories` described in
[TIMELINE_NAVIGATION.md](TIMELINE_NAVIGATION.md).

Owner labels, event horizons and target admission precede projection. A hidden
or future outcome is omitted; a withheld transition cause is null. A point with
a withheld condition endpoint or prerequisite is blocked with a generic
unavailable reason, without the withheld ID/title. The whole condition remains
unavailable through `not` and `any`; an inaccessible false leaf cannot be
inverted into affirmative eligibility.

An author-visible event that has not yet occurred remains a false occurrence
predicate, and a relationship with no transition yet has no current metric.
Their `not`/`any` semantics are unchanged. Future names are still omitted from
links and unmet-condition explanations.

State predicates use a separate admitted replay. Withheld events and effects
whose typed operands are unavailable are excluded before folding. Actual state
values and equality operands follow the declared entity/entity-set/profile
types; strings that resemble IDs remain literal strings. A predicate comparing
an unavailable typed value remains unavailable, including under `not`. Hidden
knowledge records do not enter knowledge conditions, and relationship
conditions require independently admitted endpoints. The internal canonical
evaluator retains its established source semantics for other consumers.
If omitting a hidden effect makes the remaining replay violate an exclusive
group or numeric bound, a successful canonical replay establishes that the
source remains valid; the affected state conditions are then unavailable. A
projection never reintroduces the hidden effect or conceals a canonical error.

### Explicit scene conditions

The existing `when` grammar additionally accepts this leaf:

```yaml
when:
  scene:
    location: loc_00000000000000000000000001
    participant: char_00000000000000000000000001
    object: obj_00000000000000000000000001
```

All three fields are optional, exact references of the named kinds. Supplied
checks are combined with AND. Omitted fields impose no check; unknown fields,
nulls and wrong kinds are refused by both source validation and typed authoring.
`scene: {}` requires an admitted selected scene but no additional membership.
Without a scene selection, a scene predicate is false.

An explicit selection must be an author-admitted active or closed scene at the
requested coordinate. Its start and end are inclusive; an open active scene
ends at its current cursor for this read. Planned, abandoned, withheld,
not-yet-started, ended or wrong-timeline selections are unavailable. The
participant check uses the authored inclusive arrival/departure window at that
coordinate. Location and object checks use only the explicit scene fields;
neither grants a hidden endpoint or infers possession or travel.

The result limit is 1–1,000. It independently bounds admitted candidate count,
the combined returned transition/outcome members, and continuity advisories.
Complete overflow is refused. Scoped state evaluation additionally allows at
most 1,000 state predicates across the admitted candidate set. No partial
candidate list or trail is returned. A request-local 8 MiB JSON budget charges
each admitted trail member across all returned points before accumulation,
then checks the complete response including metadata and advisories.

## Pair interactions

```sh
wedl-rs --repo my-world interactions FIRST_CHARACTER_ID SECOND_CHARACTER_ID --tick 40
```

GET `/api/interactions` accepts `first`, `second`, `tick`, optional
`order`/`timeline`, and `limit`. The native function is
`navigation::relations::interactions`.

The selectors must be two distinct, exact, admitted canonical/retired character
IDs. A returned event must be canonical, admitted at the requested horizon,
and explicitly name both characters in `participants`. Scene co-presence,
effect targets, prose, and directional relationships do not create an
interaction. Events sort by exact time and ID.

Each event retains its title/time and source citation. The `roles` object maps
each selected character ID to its authored role array, preserving repeated
roles and null for omitted roles. This avoids losing roles permitted by the
native event grammar. Unselected participants do not enter the response.
Location IDs/names are included only when independently admitted.

The limit is 1–1,000 returned events, with complete overflow refusal. Each row
is charged against one 8 MiB JSON budget before accumulation; a final complete
response check includes envelope and selected-character overhead.

## Selected thread membership

```sh
wedl-rs --repo my-world thread-memberships --record RECORD_ID --record ANOTHER_ID --thread THREAD_ID
```

GET `/api/thread-memberships` requires `records` and `threads` JSON string
arrays. `tick`, `order`, `timeline`, and `limit` are optional; an omitted tick
keeps all-time selection. The native function is
`navigation::relations::thread_memberships`.

Select 1–256 exact record IDs and 1–32 declared exact thread IDs. The native
arrays must be sorted and unique; CLI/HTTP normalize order and reject
duplicates. Admission precedes membership lookup. Unknown, hypothesis,
unsettled or author-withheld records are omitted. Settled means canonical or
retired for ordinary records, and active/closed/retired for scene/conversation
records; the record's domain admission rules still apply.

With a horizon, event time, scene/conversation/environment start, knowledge and
relationship transitions, generational initialization and applicable typed
validity are checked before membership. Identity-bearing timeless records and
initial story-point lifecycle remain available without inventing a start date.

Only records with a nonempty selected-thread intersection are returned as
`records: [{recordId, threadIds}]`, sorted by record ID and thread ID. Unselected
memberships and record titles are absent. The 1–1,000 result limit bounds
returned records; each has at most the 32 requested memberships. Overflow is an
error, not a truncated page. The complete JSON response also has an 8 MiB bound.
