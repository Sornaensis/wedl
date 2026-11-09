# Timeline and causal navigation

These author reads use one immutable source world. They retain `wedl/v1` source
and `wedl-rust/v1` JSON, exact signed i64 ticks and i32 orders, and decimal-string
time coordinates. They do not change lifecycle, state, learning or source files.

## Authored timeline

```sh
wedl-rs --repo my-world timeline --timeline main
wedl-rs --repo my-world timeline --timeline main --tick 40 --order 2 --limit 1000
```

The native function is
`navigation::timeline::timeline(world, timeline_id, at, limit)`.
GET `/api/timeline` accepts `timeline`, optional `tick`/`order`, and `limit`.
An omitted tick selects all authored time on that timeline. Order requires a
tick; the timeline and any supplied horizon must agree.

The result keeps the existing `timeline` string and canonical `events` lane.
It adds `timelineDefinition` with the authored label and optional exact origin,
`points`, `spans`, and explicit temporal semantics: ordinal spacing, no duration
interpretation, and inclusive interval endpoints. Tick differences are not
elapsed time, and an as-of read never invents an interpolated scene cursor.

| Row | Contents |
| --- | --- |
| Event point | Authored status, exact time, named event/location/participants, literal participant roles, explicit causes and related story points |
| Story-point transition | Authored owner status, member ID, dated lifecycle state/note, admitted causing event, dated outcome references and native `requires` states |
| Scene span | Start, current/end when admitted, authored status, location, participant windows/roles/POV, explicit conversation/story-point links |
| Conversation span | Start/end when admitted, authored status, location, participant windows/roles and explicit scene link |

All authored event, story-point, scene and conversation statuses can appear in
the timeline. Draft, cancelled and retconned events are chronology rows; they do not enter
the canonical `events` lane or replay as facts. Rows sort by exact coordinate,
kind, Unicode-folded title and stable identity. The canonical event list retains
its coordinate/ID ordering.

Owner and link-target admission use the existing complete local-author label
scope. Missing or withheld links are omitted, or null for a single reference;
their IDs and titles are not returned as placeholders. At a horizon, events
must have occurred, spans must have started, and each lifecycle transition is
selected by its own coordinate. Future linked events/conversations and future
participant arrivals are omitted before their names are copied. A transition's
causing event must also be at or before that transition, even if the request's
horizon is later.

Future scene cursors, span ends and participant ends are null until their
coordinates are admitted. Native `until` intervals are inclusive. An open active
scene supplies presence only through its authored current coordinate; an open
conversation supplies no implicit end. Completed scene/conversation summaries
are unavailable before their authored end. A transition's own dated note is
independent of the owner's body summary. The timeline never imports opaque
frontmatter or uses prose to infer a link.

The result limit is 1–1,000 combined points and spans. Overflow refuses the
complete result; there is no partial page. Additional complete-response bounds
are 16,384 named link occurrences and 8 MiB of JSON. Withheld rows and references
do not consume those result counts.

## Causal directions and continuity advisories

```sh
wedl-rs --repo my-world causal EVENT_ID --tick 40 --direction upstream
wedl-rs --repo my-world causal EVENT_ID --tick 40 --direction downstream
wedl-rs --repo my-world causal EVENT_ID --tick 40 --direction both
```

The native function is
`navigation::causal::causal(world, event, at, direction, limit)`.
GET `/api/causal` accepts `event`, required `tick`, optional `order`/`timeline`,
`direction` and `limit`. CLI and HTTP default to `upstream`.

Only canonical events admitted by author labels and the exact horizon enter
the graph. The focus must be one such event. Title/alias resolution uses that
admitted event cohort; withheld aliases cannot introduce ambiguity or reveal
hidden IDs. Exact IDs remain authoritative. Edges come exclusively from an
event's `causes` array and point from an earlier same-timeline cause to its
effect. Hidden, future and other-timeline endpoints cannot enter an edge.
Same-coordinate or reverse-time admitted edges are refused; ordinary Service
source validation also checks the complete authored DAG.

`upstream` follows ancestors; `downstream` follows consequences. `both` traverses
the connected component in both directions at every visited node. It therefore
includes a sibling reached through a shared cause. Shared cast, location,
scene or story-point membership alone does not create an edge.

Nodes retain `eventId`, `title`, `time` and citation, and add a named `event`
reference and canonical status. Edges retain `causeId`/`eventId`, add named
`cause`/`effect` references, and cite their exact source array member. The
response includes `direction`, `targetEventId` and named `focusEvent`.

`continuityAdvisories` covers the admitted horizon, including relevant authored
gaps outside the selected causal component:

| Kind | Exact condition |
| --- | --- |
| `plot-link-asymmetry` | An admitted canonical story point names an admitted outcome event, but the event does not reciprocally list that story point |
| `cross-location-causality` | An admitted explicit causal edge has two independently admitted, different location endpoints |

These are reading aids, not validation failures or inferred outcomes. They do
not create reciprocal links or suggest that equal locations prove continuity.
Story-point navigation reuses
`navigation::causal::continuity_advisories(world, at, limit)`.

The limit is 1–1,000 and bounds nodes, selected edges and the complete advisory
list separately. Graph construction additionally refuses more than 16,384
admitted canonical events or 65,536 admitted edges before traversal. Timeline
declarations are read once for this inventory; it does not repeatedly scan the
world for every event. Complete JSON remains bounded to 8 MiB.

## Evidence

The callable reference is the pinned
[`query.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/query.py)
timeline and causality implementation and its
[`test_causality.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_causality.py)
cases. Native requirements, presence windows, author labels, complete bounds and
exact source citations remain the Rust contracts.

The focused tests cover all-status points, lifecycle and named links, inclusive
windows, exact extreme coordinates, future/withheld mutation resistance, both
causal directions and connected-component traversal, advisory conditions,
complete-bound refusals, malformed visible edges and unchanged source.
