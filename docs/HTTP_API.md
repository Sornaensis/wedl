# HTTP API

Chronology read and confirmed-replacement authoring APIs advertise capability
for v0.6 source. Schema migration is deliberately local-only (`wedl migrate`):
there is no HTTP upgrade, rollback, or cache-copy endpoint.

`wedl serve` exposes a loopback-only JSON API and generated OpenAPI document at
`/openapi.json` (interactive documentation: `/api/docs`). The CLI parser is
the parameter source of truth; the server generates its API bindings and
OpenAPI parameter descriptions from that parser contract.

`GET /` returns the local browser application. It is public like the read API;
use `/api/session` only when a client needs the local session token.

## Authentication

`POST /api/events/consequences` requires `X-Wedl-Token`. Send a closed
`wedl-event-consequences/v1` request with `revision` (full Git SHA), `event`
(ID, title or alias), `at` (timeline and canonical signed decimal tick/order
strings), and `limit` (1–1000). Optional `expectations: {policy, items}` uses
the shared closed predicate catalogue. Caller identity, scope and perspective
fields and duplicate JSON members at any depth are rejected. Authentication
precedes source loading and name lookup.

Verification loads and validates only the requested revision without compiling
or publishing caches. The report compares immediately before/after event T and
follows literal consequences through horizon H. Its flat `expectations` cite
that source revision; required non-pass checks return `ok` with
`applyAllowed: false`. Advisory checks permit application. This read performs
no application. Combined collection/check and serialized-byte limits replace
the whole report on overflow. Outcomes map to HTTP 200 (`ok`), 400 (`invalid`),
409 (`unavailable`), and 422 (`limit`). Generated OpenAPI supplies the request,
predicates and closed report components.

Most read routes are public to the local server. Generational reads require
the repository session token because their author scope can include private
history. Session-protected routes require
`X-Wedl-Token`, whose value is returned by `GET /api/session` and stored for
the local repository in `.wedl/session.json`. The OpenAPI token field is
optional so an omitted token reaches WEDL's structured `401
authentication_required` response; it is required for a successful protected
call.

## Routes

`GET /api/changesets/schema` without query input remains public static discovery.
Add `?revision=<exact lowercase 40-character SHA>` and `X-Wedl-Token` for the
selected world's source version, ordered capabilities, state-key definitions and
relationship metric bounds. Authentication precedes source loading. Contextual
discovery validates that exact revision and writes no compiled cache; it accepts
no audience, viewer, identity or scope fields.

`consequence.batch` uses the existing `/api/authoring/preview` and
`/api/authoring/apply` routes with an unwrapped intent body. Concrete operation
schemas and a batch example appear in OpenAPI. Operations expressly author
effects, transitions and links; `expectation.check` evaluates the final candidate
at its explicit horizon without creating source facts. Apply uses the original
intent's exact `X-Wedl-Confirmation`, including for automation. Edited aliases,
policies, checks or times require a new preview. Session authentication and
idempotent recorded-result replay retain the ordinary authoring behavior.

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
| POST | `/api/generational/{operation}` | yes | Read named, cited generational history; operations: parents, ancestors, descendants, relatives, union, organization, legacy, vital, search, context. |
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

Query names preserve the established aliases where applicable: `q`,
`requireCompiled`, `includeText`, `includeHypotheses`, `allTime`, `maxCharacters`, and `maxItems`.
Use `/openapi.json` for the full current parameter list, parser defaults,
enums, numeric bounds, and descriptions.

Generational reads accept an unwrapped, closed `wedl-generational/v1` body
with the selected revision, canonical capabilities, author mode, timeline,
and named selector. The same body is accepted by `wedl generational ACTION
FILE`. The server derives author audience and perspective; caller-supplied
viewer fields are rejected. The local token is repository-scoped and cannot
authenticate a character, so character mode remains closed `unknown`.
Author as-of is the default read scope; all-time is explicit and forbids `at`.
StoryTime ticks and orders are canonical signed decimal strings. Semantic
`available` and `unknown` use 200, `invalid` uses 400, `unavailable` uses 409,
and `limit` uses 422. The generic WEDL error envelope remains in use for
authentication, parse, and repository errors. See [Generational query](GENERATIONAL_QUERY.md)
and [Generational authoring](GENERATIONAL_AUTHORING.md).

## Spatial reads

### Spatial explorer adjunct

The five `/api/spatial/explorer/*` reads use `wedl-spatial-explorer/v1` and
are HTTP-only. They consume the compiled projection and do not add CLI
commands. Call `GET /api/spatial/explorer/catalog?limit=20` first. It returns
the exact `revision`, ordered `capabilities`, source schema, map descriptors
(`id`, label, CRS, axes, unit, bounds), `spatialAvailable`, and `nextCursor`.
Legacy and coordinate-free worlds return an empty map list. For later catalog
pages, send that revision and each capability as a repeated `capabilities`
query parameter, with the cursor and unchanged limit. The first page takes no
revision or capability guess.

The other four routes accept one raw, closed JSON request with `protocol`,
catalog `revision`, ordered `capabilities`, `limit` (1–100), and nullable
`cursor`. Examples:

```json
{"protocol":"wedl-spatial-explorer/v1","revision":"0000000000000000000000000000000000000000","capabilities":["spatial-core-v1","geometry-v1","route-v1","overlay-v1"],"limit":20,"cursor":null,"mode":"children","parentId":"location:gate"}
```

`places` modes are `roots`, `children` (`parentId`), `search` (nonblank `query`),
and `select` (1–100 exact `ids`). Roots and children page by stable ID using
the compiled parent index. Place cards contain stable ID, label,
authored parent, map ID, geometry availability, and basis. Search inspects at
most 2,000 compiled title-document candidates before the place join. A
broader title match returns `limit` even if its documents are not places.
It does not send all records to the browser.

| Mode | Request fields in addition to the shared envelope | Response `basis` |
| --- | --- | --- |
| `roots` | `"mode":"roots"` | `authored-parent-id` |
| `children` | `"mode":"children","parentId":"location_parent"` | `authored-parent-id` |
| `search` | `"mode":"search","query":"Example place"` | `compiled-title-search` |
| `select` | `"mode":"select","ids":["location_X"]` | `authored-location-id` |

The OpenAPI document includes a request and a bounded result example for
each mode.

`routes` inspects authored directed edges for one `locationId`. Set
`direction` to `incoming` or `outgoing` and optionally provide `modes` as an
array of authored mode names; an empty or omitted array accepts every mode.
It requires `route-v1` in the selected capabilities. For example:

```json
{"protocol":"wedl-spatial-explorer/v1","revision":"0000000000000000000000000000000000000000","capabilities":["spatial-core-v1","geometry-v1","route-v1","overlay-v1"],"limit":20,"cursor":null,"locationId":"location:gate","direction":"outgoing","modes":["foot"]}
```

Results order directed route edges by the other endpoint ID, route ID, and
explicit reverse-edge flag, then portals by portal ID. A two-way route has
an explicit reverse card; a one-way route has only its authored direction.
Self-loops follow the same rule. Closed routes remain visible as inspection
cards with their authored availability, while traversal rules remain as
before. Each route card has directed endpoint IDs, authored direction, modes,
availability, uncertainty, and separate nullable `routeDistance`,
`travelCost`, and `duration` metrics with value and unit when known. Each
portal card has its authored source and either a target location ID or an
authored target position with native map ID, coordinates, CRS, axes, and unit.
Position-target portals occur only on their source's outgoing page. No card
infers reverse access, geometry distance, travel time, or map conversion.
Coordinate-free locations remain valid. A missing place returns `unavailable`.
The cursor is bound to revision, location, direction, normalized modes, and
limit; invalid or fabricated keys return `invalid`. Reads seek the compiled
edge indexes and cap mode-filter inspection at 2,000 candidates per page,
returning `limit` if that budget is exceeded.

`viewport` adds `mapId`, finite same-dimensional `bounds` (`min`/`max`), and
`relation` (`within` or `intersects`). It returns only that map's authored
features in native CRS and unit. `layers` adds the same viewport, canonical
decimal-string `asOf` StoryTime, and explicit `audience` and `perspective`.
An optional `overlayId` selects an authorized overlay. Hidden and nonexistent
IDs receive the same forbidden outcome. Authorization precedes layer counts,
pagination, geometry joins, and cursor generation. This selected lens is a
local author presentation filter, not a trusted character identity.

All cursors bind the selected revision, operation, filters, page limit, map
scope, and selected lens. A changed request receives `invalid`. Semantic
states are `ok` 200, `invalid` 400, `unavailable` 409, `forbidden` 403, and
`limit` 422; malformed requests and compile failures use the ordinary WEDL
error envelope. A feature exceeding the 10,000-vertex budget returns a
closed `limit` outcome. The normalized OpenAPI document gives complete
request, response, and status schemas for every route.
Explorer reads use compiled parent and title indexes. Viewports use a
map-scoped RTree for bounded geometry candidate probes, then apply exact
same-map bounds tests before sorting and paging. A compiled cache without
that optional RTree returns `unavailable` for an explorer viewport; the
compiler fingerprint rebuilds older caches with the new RTree dimensions and
location-first audience/perspective membership index. Layer queries probe at
most 10,000 public viewport geometries, then seek the selected lens at each
location. Static memberships use a partial index; changing memberships use
a compiled temporal RTree keyed by location/lens/timeline scope and four
exact 24-bit digits of signed StoryTime. Each interval uses at most 15
disjoint boxes; exact StoryTime checks still precede the 2,000 visible
membership budget.
Markdown and Git source remain authoritative.

The eight `/api/spatial/*` endpoints accept the raw, closed `wedl-spatial/v1`
JSON body; a CLI file path is never part of the HTTP body. Every request names
the exact compiled revision and canonical capability list, has `limit` from 1
through 100, and supplies `cursor` as null or an opaque response cursor.
Responses use the same envelope with `state` `ok`, `invalid`, `unavailable`,
`forbidden`, or `limit`; those states map to HTTP 200, 400, 409, 403, and 422
respectively. Cache metadata contains readiness and revision facts only, never
repository, source, or database paths. StoryTime `tick` and `order` are
canonical signed decimal strings. Overlay authorization occurs before result
selection or pagination.

Each spatial answer is bound to one verified open compiled projection: its
result rows, response revision, and cache metadata are read from the same
SQLite file. If a concurrent compile replaces that file before the read binds,
the server retries rather than mixing revisions. `compile_required` remains an
ordinary WEDL error (HTTP 400 / CLI exit 2), but its spatial transport details
contain only the rebuild hint and never raw cache paths.

| Operation | Required operation members | `ok` result |
| --- | --- | --- |
| containment | `locationId` | authored containment `ids` and `basis`; cursor must be null |
| children | `locationId` | catalogue `ids`, `basis`, `filters`, `nextCursor` |
| bbox | `mapId`, ordered `bounds`, `relation` | geometry catalogue with `units` and `nextCursor` |
| nearby | `position`, nonnegative `radius` | same-map catalogue with `units` and `nextCursor` |
| adjacency | `locationId`, optional `modes` | authored route/portal edges; cursor must be null |
| reachability | `fromLocationId`, optional `modes` | bounded directed `ids` and `expansions`; cursor must be null |
| path | `fromLocationId`, `toLocationId`, `metric`, optional exact `unit`/`modes` | route IDs and labelled `metric.computedTotal`; cursor must be null |
| overlay-as-of | exact `queryScope`, `locationId`, audience, perspective, `asOf` | authorized overlay IDs and `nextCursor` |

`overlay-as-of` is a discriminated union: `queryScope: "location"` forbids
`overlayId`; `queryScope: "overlay"` requires it. This happens before the
store call. An explicit hidden or nonexistent overlay ID is the same closed
`forbidden` result (`SPATIAL-OVERLAY-001`, HTTP 403, CLI exit 2). A catalogue
read without an explicit ID remains an `ok` empty result when no overlay is
visible. Non-StoryTime numeric operands are finite
and limited to JSON's exact integer range; source/SQLite-sized integers are not
accepted as lossy JSON numbers.

## Entity-detail chronology annotations

Every `GET /api/entities/{entity_id}` response includes
`chronologyAnnotations`, even for legacy or empty records (`[]`). It is a
complete, public `chronology.replace` annotation array: IDs, role, display,
provenance, every annotation value kind, and allowed nested `x-*` data are
preserved. Core chronology coordinates use canonical decimal strings; opaque
`x-*` payloads retain their original JSON values. A qualitative approximate
claim has `bounds:{lower:null,upper:null}` and omits `calendarId`; any
approximation with a bound includes the shared `calendarId`. This additive
field does not change `frontmatter` or the existing entity-detail fields. The
closed entity response enumerates those established fields and rejects unknown
members. On every chronology value, `tagExtensions` is the separate opaque
`x-*` channel for source extensions beside the discriminator tag; direct
`x-*` members remain payload extensions, so same-name values in both channels
round-trip independently.

## Narrative thread catalog

`GET /api/threads` (and `wedl threads`) returns the `wedl-threads/v1` catalog
of declared optional narrative grouping labels. A `wedl/v0.3` world returns an
empty catalog with `groupingAvailable: false`; it does not invent a default
thread. A `wedl/v0.5` world returns only `{id, label}` declarations, ordered by
case-folded label and then ID. The catalog never includes memberships, record
counts, state, time, status, search ranks, corpus text, or vector data.

`GET /api/search` accepts repeated `threadId` values (CLI: repeated
`--thread-id`); `GET /api/context` accepts repeated `recallThreadId` values
(CLI: repeated `--recall-thread-id`). IDs must be declared, unique, and sorted
in a validated `wedl/v0.5` world. Search retains the stable filtered
subsequence after ranking and deduplication without backfilling. Context uses
the selector only to remove already-selected useful recall after fitting; it
does not alter other packet sections, authorization, chronology, state, or
retrieval ranking. Omitting either parameter preserves the existing response
byte-for-byte.

`GET /api/thread-memberships` (CLI: `wedl thread-memberships`) accepts required
repeated `recordId` (one to 256) and `threadId` (one to 32) values. Both lists
must be stable IDs, unique, and lexicographically sorted. The selected thread
IDs use the same validated v0.5 selector as search and context, with ANY
semantics. Its `wedl-thread-memberships/v1` response contains only the
requested public canonical records that belong to at least one selected group;
each entry returns `{recordId, threadIds}` where `threadIds` is only the sorted
selected intersection. Unknown, unavailable, noncanonical, and hypothesis
candidates are omitted, so it cannot reveal membership outside the caller's
already visible candidate set. It does not expose state, chronology, ranking,
corpus, or vector data.

## Whereabouts

`GET /api/whereabouts` (and `wedl whereabouts`) is an author-only bulk read
for stories with concurrent fronts. It reports every canonical and retired
character independently, grouped by their recorded location, with offstage
and unlocated people called out separately. Use optional `character`, `tick`,
`timeline`, and `order` parameters to select one named character and/or an
author horizon. Without a tick it uses the shared current cursor when one is
authored.

`activeScenes` and each character's `activeScene` are evaluated at that
selected horizon. A scene closed in the present can therefore appear when the
requested moment falls inside its authored interval.

The `wedl-whereabouts/v2` response retains the v1 location, journey, front,
and aggregate fields and adds display-ready `role`, a per-character
`importance` breakdown, and top-level `importancePolicy`. Prominence is a
calculated, disposable, noncanonical navigation aid: it is never authored,
serialized into Markdown/frontmatter, or available to changesets.

The score is calculated at the selected horizon over the complete canonical
and retired cohort before any `character` filter. Raw scene appearances,
point-of-view scene appearances, canonical event involvement (participant or
effect target, once per event), and distinct reciprocal relationship neighbors
are respectively normalized as `log1p(raw) / log1p(cohort maximum)` and
weighted 40/25/20/15. A zero maximum contributes zero; contributions and the
final score are rounded deterministically. It deliberately excludes prose,
tags, knowledge, inferred travel, co-presence, and manual overrides.

Location fields still contain only initial character locations and canonical
`location` set/clear effects. Journey entries distinguish an initial placement,
move, reaffirmation, and clear, and carry exact decimal story coordinates plus
named event references. They never infer travel from scene participation,
location links, routes, group labels, or character knowledge.

## Possibilities

`GET /api/hypotheses` (and `wedl hypotheses`) lists author hypotheses, or
reads one by a title, alias, slug, or ID. A hypothesis has an `open`,
`adopted`, or `rejected` lifecycle plus statement, subjects, alternatives,
and placement context. It is deliberately non-canonical: placement never
asserts story time, order, duration, or state.
Its author-facing projection carries names and kinds rather than opaque source
IDs. An adoption may cite only already-settled records and never promotes or
changes them.

The ordinary entity list and search exclude hypotheses by default. Authors may
pass `includeHypotheses=true` (or `wedl search --include-hypotheses`) to search
them; character search and context never permit this opt-in. Adoption records
which already-settled records address a possibility and does not canonize or
create a new fact. Hypotheses are deliberately outside the shared compiled FTS
and vector corpus; their explicit author search uses an isolated lexical read,
so a possibility cannot reorder or rescore canonical retrieval.

## Errors and client discovery

Every operation in `/openapi.json` carries a stable success schema, compact
examples, and structured WEDL error examples. Errors use the JSON envelope
`{"code", "message", "details"}`. Request-shape failures are WEDL `400`
responses (not FastAPI `422`). The complete semantic error inventory is:

| HTTP status | Codes |
| --- | --- |
| 400 | `usage_error`, `validation_failed`, `confirmation_required`, `confirmation_mismatch`, `not_found`, `compile_required`, `repository_error`, `parse_error`, `protocol_error`, `v04_superseded`, `upgrade_required` |
| 401 | `authentication_required` |
| 409 | `conflict`, `stale_revision`, `dirty_managed_tree` |

A programmatic client can begin with `GET /api/session`, then read the
generated `/openapi.json` document for current route metadata and examples.
Use public reads directly; attach the returned `X-Wedl-Token` only for
compile/changeset workflow calls. This lets a client discover the API without
copying parser defaults or guessing response fields.

## Changeset workflow

Static `schema` discovery is public. Supplying an exact revision for contextual
discovery requires `X-Wedl-Token`. `scaffold`, `preview`, and `apply` also require
`X-Wedl-Token`. `scaffold` has no request body. `preview` and `apply` accept
the raw `wedl-changeset/v1` JSON object itself, not an object wrapped in
`payload`, `changeset`, or a CLI file field.

1. `POST /api/changesets/scaffold` to obtain a current-HEAD starter.
2. Edit the object, then `POST /api/changesets/preview` with that object.
   Preview is safe to review: invalid candidates return `200` with
   `"valid": false` and diagnostics; it never returns private `_changes`.
3. Send the unchanged object to `POST /api/changesets/apply`, with both
   `X-Wedl-Token` and the preview's `confirmationToken` in
   `X-Wedl-Confirmation`.

The confirmation token is a deterministic proof over canonical JSON (including
`requestId`), the request hash, and resolved HEAD. JSON member order is not
significant; changing the canonical payload is. The token is not authorization
and is never accepted from the request body. HTTP always injects current-head
planning; it does not expose the CLI's `--yes` or `--use-current-head` escape
hatches. Review the canonical payload represented by preview before apply.

An exact retry with the same idempotency key, request hash, full payload, and
confirmation token returns the saved result with `idempotentReplay: true`; it
does not write, compile, or broadcast another revision. Reusing the key for a
different full payload is a `409 conflict`.

Thread grouping changes use the existing `entity.update` operation and only
its `frontmatterPatch`; there is no separate route or operation type. It is
optional narrative grouping in the one shared world, not a second canon,
calendar, mutable state, or search corpus. A world record replaces its complete
declaration list with canonical source `threads`:

```json
{"entity":"world_...","frontmatterPatch":{"threads":[{"id":"thread_...","label":"Archive"}]}}
```

An ordinary non-hypothesis record replaces its complete membership list with
public `threadIds`; it is translated internally and serialized as canonical
Markdown frontmatter `threads`, never `threadIds`. Use `[]` to clear
membership:

```json
{"entity":"character_...","frontmatterPatch":{"threadIds":[]}}
```

Lists are full replacements, never merged, sorted, deduplicated, or inferred.
A changeset may replace declarations and memberships together, and validation
runs against that final candidate atomically: removing a still-used declaration
fails without writing any part of the changeset. Hypotheses cannot use either
grouping key. The raw request is not rewritten, so confirmation and
idempotency remain bound to precisely the submitted JSON.

## Authoring intents

`POST /api/authoring/preview` and `/api/authoring/apply` accept a compact
authoring intent rather than raw IDs. Each is resolved against the current
world and compiled to an ordinary `wedl-changeset/v1`; preview returns the
normal changeset preview and apply requires that preview token in
`X-Wedl-Confirmation`. Supported `action` values are `current-time.set`,
`scene.create`, `scene.advance`, `scene.close`, `character.move`,
`conversation.create`, `conversation.append`, `hypothesis.create`,
`hypothesis.adopt`, `hypothesis.reject`, `chronology.replace`,
`spatial.map.create`, `spatial.map.update`, `spatial.location.update`,
`spatial.route.create`, `spatial.route.update`, `spatial.overlay.create`, and
`spatial.overlay.update`, and `consequence.batch`.
`chronology.replace` requires the exact audited `expectedHead`, replaces each
supplied catalogue or record annotation array completely, previews before any
write, and applies through the ordinary confirmed changeset workflow. A hypothesis create takes a
name, statement, named subjects, alternatives, and placement context. Adoption
accepts only records that are already canonical and explicitly reports that
canon is unchanged. A conversation creation takes a
title, an optional active-scene name (required when several fronts are active),
and an optional independently present character selection; otherwise it uses
the scene's location/current time and all present characters. The generated OpenAPI schema documents the action
variants and request examples show representative payloads. There is no party/group action: `characters` is only a convenient list
of independent character references, expanded to individual effects.

Spatial actions have closed ID-only payloads and require a 40-character
`expectedHead`, a nonblank `idempotencyKey`, and an exact preview confirmation.
Spatial request and intent identifiers, labels, modes, and other text members
are limited to 256 characters; authoring summaries are limited to 1024.
They cannot use the CLI `--yes` bypass. Maps use `crs`, `axisOrder`, `unit`,
and ordered `bounds`; location updates patch `parentId` and/or `spatial`
(`null` clears only that authored field); routes use endpoint IDs, direction,
modes and authored metrics; overlays use `membership.locationIds`, audience,
perspectives and `valid.start/end`. Public overlay StoryTime ticks/orders are
canonical decimal strings and become bounded source integers only inside the
confirmed authoring transaction.
Create requires a title for maps, routes, and overlays. Updates preserve the
existing title, body, provenance, extensions, and unrelated frontmatter;
`title` is not an update field or a selector.

Ergonomic `conversation.append` targets active conversations only. Historical
or closed transcripts are deliberate source-history edits and must use a raw
changeset; the helper does not extend their time bounds or participant presence.

Authoring preview and apply responses additionally include `authorImpact`, a
stable `{summary, items}` helper intended for people rather than automation.
Its items use names only—conversation, scene, location, characters, and story
time—and can report `conversation-created`, `conversation-turn-appended`, and
`author-horizon-advanced`. The ordinary raw changeset and apply result fields
remain unchanged. An append whose order is automatically allocated after the
current cursor advances every active front atomically; an explicit order never
does so implicitly.

## Semantic preview and explicit consequence batches

Raw and authoring previews accept optional `consequenceRequest` with an explicit
canonical signed-string horizon. Their existing protocols, source diff, files,
diagnostics, generated IDs and confirmation flow remain unchanged. The nested
`semanticDelta` uses `wedl-event-consequence-delta/v1`: base and final candidate
are compared at the same H; literal event groups and unattributed record changes
preserve their different attribution. Event-local before/after at T is a separate
event report comparison. Closed invalid/unavailable/limit results never assert a
partial success.

`consequence.batch` records explicit effects, caused transitions and outcome links,
with optional required/advisory expectation checks. `expectationChecks` is reported
independently of the optional delta. Apply binds the complete original intent and
requires the exact author preview token; no scope or confirmation body fields are
accepted. A check-only no-op returns a checked receipt without advancing HEAD.
See the [executable rescue example](guides/event-consequence-preview.md) for custody,
belief, directed trust, delayed plot resolution, scene outcome and unattributed
prose, plus generated-reference and candidate-provenance handling.

## Explicit exclusions

The API does not expose local process/file concerns. This is the complete
local-only inventory; use the CLI for each of these instead.

| Local-only item | Reason |
| --- | --- |
| `completion` | Emits shell code on the local machine. |
| `init` | Creates a repository and may invoke Git locally. |
| `serve` | Owns local host, port, and browser lifecycle. |
| `migrate` | Performs a confirmed local Git source migration/recovery and cache rebuild; it has no HTTP equivalent. |
| `--repo` | Selects a local filesystem repository. |
| `--compact` | Controls CLI output formatting only. |
| `--version` | Reports the local CLI/application version rather than a repository operation. |
| `--output` | Writes a local output file. |
| `FILE` | Is the CLI positional local changeset file path; HTTP sends raw JSON instead. |
| `--yes` | Is an unsafe CLI-only confirmation bypass. |
| `--use-current-head` | Is a CLI-only expected-HEAD escape hatch. |
| `--expected-head` | Binds every local optimistic-concurrency authoring or migration preview/apply to its audited source head. |
| `--source-snapshot-hash`, `--rollback-backup-ref` | Bind the local-only migration preview/apply and forward rollback flow. |
| `--host`, `--port`, `--open` | Configure local server binding/browser launch. |

The API also does not accept a confirmation token in a JSON body. It uses only
`X-Wedl-Confirmation` for the reviewed preview token.
