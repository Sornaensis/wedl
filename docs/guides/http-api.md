# Local HTTP usage

Start the server against your world repository:

```text
wedl serve --repo PATH
```

Open the printed loopback URL. Use `/api/docs` for interactive requests and
`/openapi.json` for the current parameters, accepted bodies and response examples.
`GET /api/session` provides the repository session token and HEAD. Add that
token as `X-Wedl-Token` for routes marked yes below; contextual changeset
schema discovery also requires it. Use the OpenAPI examples rather than
guessing a body from a CLI file path.

## Route lookup

| Method | Route | Session token | Purpose |
| --- | --- | --- | --- |
| GET | `/` | no | Local browser application. |
| GET | `/api/session` | no | Discover the repository session token and current HEAD. |
| GET | `/api/status` | no | Repository and compiled-cache status. |
| GET | `/api/validate` | no | Source validation report. |
| POST | `/api/compile` | yes | Compile using CLI-equivalent query options. |
| GET | `/api/entities` | no | List entities. |
| GET | `/api/entities/{entity_id}` | no | Show one entity, including complete public chronology annotations. |
| GET | `/api/entities/{entity_id}/state` | no | Resolve state at required `tick`. |
| GET | `/api/entities/{character_id}/knowledge` | no | Resolve character knowledge at required `tick`. |
| GET | `/api/interactions` | no | Interactions for required `first` and `second`. |
| GET | `/api/story-points` | no | Story points at a scene or time. |
| GET | `/api/timeline` | no | Complete ordinal chronology for one declared timeline. |
| GET | `/api/chronology` | no | Public calendar chronology catalogue and capability. |
| POST | `/api/chronology/format` | no | Format an unwrapped `wedl-chronology/v1` date request. |
| POST | `/api/chronology/convert` | no | Convert an unwrapped `wedl-chronology/v1` date request. |
| POST | `/api/chronology/search` | no | Search an unwrapped `wedl-chronology/v1` annotation request. |
| POST | `/api/chronology/story-times` | no | Map an unwrapped `wedl-chronology/v1` date request through anchors. |
| POST | `/api/spatial/containment` | no | Read authored spatial containment from an unwrapped `wedl-spatial/v1` request. |
| POST | `/api/spatial/children` | no | List authored child locations from an unwrapped `wedl-spatial/v1` request. |
| POST | `/api/spatial/bbox` | no | Query compiled authored geometry bounds. |
| POST | `/api/spatial/nearby` | no | Query same-map authored geometry candidates. |
| POST | `/api/spatial/adjacency` | no | Read authored directed route and portal edges. |
| POST | `/api/spatial/reachability` | no | Traverse bounded authored directed route and portal edges. |
| POST | `/api/spatial/path` | no | Read one authored metric path without inferring travel. |
| POST | `/api/spatial/overlay-as-of` | no | Read authorized overlays at an exact StoryTime horizon. |
| GET | `/api/spatial/explorer/catalog` | no | Bootstrap or page compiled map descriptors. |
| POST | `/api/spatial/explorer/places` | no | Page bounded hierarchy, title search, or exact-ID place cards. |
| POST | `/api/spatial/explorer/viewport` | no | Page native same-map authored geometry. |
| POST | `/api/spatial/explorer/layers` | no | Page authorized as-of overlay membership in one viewport. |
| POST | `/api/spatial/explorer/routes` | no | Page authored incoming or outgoing route and portal cards. |
| POST | `/api/generational/{operation}` | yes | Read named, cited generational history; operations: parents, ancestors, descendants, relatives, union, organization, legacy, vital, search, context, discover, labels, character-unions, organization-legacies. |
| GET | `/api/generational/bootstrap` | yes | Discover compiled revision, capabilities and generational read-session metadata. |
| POST | `/api/generational/scaffold` | yes | Current-HEAD generational organization starter intent. |
| GET | `/api/generational/schema` | yes | Closed generational intent variant catalogue. |
| GET | `/api/threads` | no | Declared optional narrative grouping labels only. |
| GET | `/api/thread-memberships` | no | Selected grouping membership projection for supplied records. |
| GET | `/api/whereabouts` | no | Horizon-bounded character locations and explicit journeys. |
| GET | `/api/hypotheses` | no | Explicitly non-canonical author possibilities. |
| GET | `/api/causal/{event_id}` | no | Explicit event-cause trail at an author horizon. |
| GET | `/api/search` | no | Perspective- and time-bounded search. |
| GET | `/api/context` | no | Bounded writing-context packet. |
| GET | `/api/conversations/{conversation_id}` | no | Time- and perspective-bounded transcript. |
| GET | `/api/changesets/schema` | with `revision` | Concrete operation schemas; optional exact-revision world declarations. |
| POST | `/api/changesets/scaffold` | yes | Current-HEAD starter changeset. |
| POST | `/api/changesets/preview` | yes | Validate and preview a changeset. |
| POST | `/api/changesets/apply` | yes | Confirmed, atomic changeset apply. |
| POST | `/api/authoring/preview` | yes | Resolve a semantic, name-oriented authoring intent and preview its compiled changeset. |
| POST | `/api/authoring/apply` | yes | Confirmed atomic apply of a semantic authoring intent. |

Query names include `q`, `requireCompiled`, `includeText`, `includeHypotheses`,
`allTime`, `maxCharacters`, and `maxItems`. Consult the generated document for
each route's options, numeric bounds and examples.

## Preview and apply

1. Send `POST /api/changesets/scaffold` with `X-Wedl-Token` for a starter.
2. Edit it, then send the JSON object directly to `/api/changesets/preview`.
   Inspect `valid`, diagnostics, the diff and the returned confirmation token.
3. Send the unchanged object to `/api/changesets/apply` with `X-Wedl-Token`
   and the exact preview token as `X-Wedl-Confirmation`.

For semantic intents, use `/api/authoring/preview` and `/api/authoring/apply`
with the same header sequence. Supported actions include current-time/scene,
move/conversation/hypothesis, chronology replacement, spatial map/route/overlay
and location update, `consequence.batch`, generational create/append/correct/batch
and generational knowledge opt-in/create/state/replace. Use generated variants
for the exact action body. Edit a previewed request only after making a new
preview. HTTP confirmation belongs in the header, not in the JSON object.

## Domain reads

For spatial explorer navigation, start with
`GET /api/spatial/explorer/catalog?limit=20`. Copy its revision and ordered
capabilities into the OpenAPI request example for places, viewport, layers or
routes. Copy response cursors verbatim and keep the selected filters unchanged.
For direct spatial reads, send the corresponding raw JSON request to the
operation route; a local request filename is not an HTTP member.

For generational reads, call protected `/api/generational/bootstrap` first.
Copy the returned revision and capabilities into the selected OpenAPI example
and supply explicit `"mode":"author-as-of"`, `"author-all-time"`, or
`"character"`. As-of uses the example's explicit time; all-time omits `at`.
For character mode pass `?viewpoint=CHARACTER`, selected by the local author.
This repository token does not authenticate that character. There is no public
`knowledge-history` operation. See [generational usage](generational.md).

Chronology capability/catalogue and replacement authoring advertise v0.6;
v0.7 compiled format/convert/search/story-times reads are also implemented,
while v0.7 catalogue definitions are empty and replacement remains v0.6-only.
Schema migration is local: there is no HTTP upgrade, rollback or cache-copy
endpoint. Use [migration and recovery](migration-and-recovery.md).

Thread catalogue/filter reads currently admit v0.5/v0.6. For selected grouping,
repeat `threadId` on search or `recallThreadId` on context with sorted unique
declared IDs. Public grouping reads currently do not admit v0.7, although its
canonical source retains declarations and memberships. Use `recordId` and
`threadId` on `/api/thread-memberships` for an explicit selected projection.

For consequence requests and batches, use the [preview walkthrough](event-consequence-preview.md).

## Contract lookup

From the WEDL development/source checkout, read the transport contract with
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M494NTSZ76DE2X6NM4TBCB0C --json`. Spatial and chronology
domain contracts remain A01M48ZSKSZ3EKW7EZX5HDTSRET and
A01M48XZJ6V1X1QSM4QEZJZS81M; generational query and authoring are
A01M491YZG40BC65JPVT4JMYR7D and A01M491ZXP5DQ61KMG2YC66G8MM.
