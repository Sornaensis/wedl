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

Read routes are public to the local server. Session-protected routes require
`X-Wedl-Token`, whose value is returned by `GET /api/session` and stored for
the local repository in `.wedl/session.json`. The OpenAPI token field is
optional so an omitted token reaches WEDL's structured `401
authentication_required` response; it is required for a successful protected
call.

## Routes

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
| GET | `/api/threads` | no | Declared optional narrative grouping labels only. |
| GET | `/api/thread-memberships` | no | Selected grouping membership projection for supplied records. |
| GET | `/api/whereabouts` | no | Horizon-bounded character locations and explicit journeys. |
| GET | `/api/hypotheses` | no | Explicitly non-canonical author possibilities. |
| GET | `/api/causal/{event_id}` | no | Explicit event-cause trail at an author horizon. |
| GET | `/api/search` | no | Perspective- and time-bounded search. |
| GET | `/api/context` | no | Bounded writing-context packet. |
| GET | `/api/conversations/{conversation_id}` | no | Time- and perspective-bounded transcript. |
| GET | `/api/changesets/schema` | no | Changeset envelope and operation vocabulary. |
| POST | `/api/changesets/scaffold` | yes | Current-HEAD starter changeset. |
| POST | `/api/changesets/preview` | yes | Validate and preview a changeset. |
| POST | `/api/changesets/apply` | yes | Confirmed, atomic changeset apply. |
| POST | `/api/authoring/preview` | yes | Resolve a semantic, name-oriented authoring intent and preview its compiled changeset. |
| POST | `/api/authoring/apply` | yes | Confirmed atomic apply of a semantic authoring intent. |

Query names preserve the established aliases where applicable: `q`,
`requireCompiled`, `includeText`, `includeHypotheses`, `allTime`, `maxCharacters`, and `maxItems`.
Use `/openapi.json` for the full current parameter list, parser defaults,
enums, numeric bounds, and descriptions.

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

`schema` is public. `scaffold`, `preview`, and `apply` require
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
`hypothesis.adopt`, `hypothesis.reject`, and `chronology.replace`.
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
