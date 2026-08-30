# Chronology API contract

`wedl-chronology/v1` exposes active `wedl/v0.6` calendar chronology separately from the unchanged
ordinal `wedl-timeline/v1` contract. Chronology coordinates are canonical
decimal strings; JSON numbers are rejected for those coordinates. Reads are available at
`GET /api/chronology` and unwrapped POST requests to `/api/chronology/format`,
`/convert`, `/search`, and `/story-times`.

Successful semantic responses use `outcome: ok|invalid|unavailable`; invalid
date semantics are HTTP 200 outcomes, while malformed request objects are
structured HTTP 400 errors. Story-time mappings use explicit anchors only and
report `none`, `unique`, or `ambiguous`; no tick-to-duration conversion exists.

`wedl author chronology replace FILE --expected-head HEAD` previews a complete
catalogue and/or complete per-record annotation replacement through the normal
confirmed changeset workflow. Legacy v0.3/v0.5 sources require the active,
local-only `upgrade-v06` preview/confirmation before chronology authoring;
there is no HTTP or browser migration operation.

## Operations and client migration

`GET /api/chronology` and each POST accept `requireCompiled=true` for a strict
no-rebuild read. Without it, the normal disposable compiled cache may rebuild;
an invalid source returns the normal validation error rather than stale data.
POST bodies are unwrapped and closed: format and story-times take `value`,
convert takes `value` plus exactly one nonempty `calendarId` or `eraId` target,
and search takes a predicate/value, optional era filter, and `upper` only for
`between`. Malformed bodies are HTTP 400; a well-formed but unknown calendar or
era is a HTTP 200 chronology `invalid` outcome.

Format, convert, search, and story-times responses are closed `ok`, `invalid`,
or `unavailable` outcomes. `ok` includes the operation result; `invalid`
includes a definition/date/range/era/anchor/overflow reason; `unavailable`
includes the precise no-epoch/table/axis/uncertainty reason. Search advisories
are deterministic. Story-time mappings explicitly report `none`, `unique`, or
`ambiguous` and never choose an anchor implicitly.

Clients migrating from source-only chronology reads should use this protocol
rather than reconstructing a calendar projection. Coordinates are canonical
decimal strings, including anchor and StoryTime coordinates. `limit`, advisory
counts, and source ordinals are JSON numbers as specified. See
`examples/chronology-api-v1.yaml` for positive and negative request vectors.

## Operation shapes

All chronology read POST bodies include `protocol: "wedl-chronology/v1"` and reject unknown
members. `format` and `story-times` require `{protocol,value}`. `convert`
requires `{protocol,value,target}`, where target is exactly one of
`{calendarId:string}` or `{eraId:string}`. `search` requires
`{protocol,predicate,value}` and accepts `eraFilter:{eraId,mode}` and `limit`;
`upper` is required only when `predicate` is `between` and forbidden otherwise.
Every identifier is nonblank (whitespace-only values are malformed). The date
union is closed: civil, era, range, approximate, and conflict. `format` and
`convert` deliberately accept the non-conflict subset only; `search` and
`story-times` also accept conflicts. A day always requires a month. A conflict
can contain 2..64 claims and nest through at most 64 conflict objects;
over-depth values are structured HTTP 400 usage errors, never a server error.
`relative` and `duration` are valid only in chronology-replace annotation
authoring, never in a read operand.

## Entity-detail annotation handoff

`GET /api/entities/{entity_id}` always adds `chronologyAnnotations`: an empty
array for legacy or unannotated records, otherwise the complete public
authoring-shaped annotation list. It carries annotation IDs, role, display,
provenance, all seven source value kinds (civil, era, range, approximate,
conflict, relative, and duration), and recursive opaque `x-*` members. Core
coordinates are canonical decimal strings, including values outside JavaScript
safe-integer range. Clients can place the array unchanged in a
`chronology.replace` record replacement without reconstructing source
frontmatter. Qualitative approximate bounds omit `calendarId` exactly when
both endpoints are `null`; every bounded or one-sided approximation requires
the shared `calendarId`.

Every public value variant may also carry an optional `tagExtensions` object.
It is closed except for opaque `x-*` members and reversibly represents only
extensions that sat beside the source discriminator tag. Direct public `x-*`
members remain extensions of that tag's payload. The two locations are
independent—even identical extension names retain both values—and neither
extension payload is renamed, traversed, or coerced.

Catalog returns closed public calendar, era, and anchor definitions. Calendar
rules include cycle/table mechanics, months, and epoch; era definitions include
aliases, display epoch, provenance, and optional exact bounds. Public integer
coordinates in those definitions are strings as well. Source-compatible
`x-*` members are retained in catalog definitions, chronology-replace
declarations, and authored hit values, including nested date values; they are
opaque and never coerced as chronology coordinates. Arbitrary unrecognised
public fields are rejected.

The common semantic envelope always contains protocol, operation, revision,
outcome, and ordered advisories. `ok` has a typed result. `invalid` has a
definition/date/range/era/anchor/overflow/invalid_request reason, and
`unavailable` has a no-epoch, table-gap, disconnected-table, no-shared-axis,
approximate-only, conflicting-claims, no-chronology, or conversion-exactness
reason. Search advisories are limited to
`approximate-overlap-included`, `approximate-relation-excluded`,
`noncomparable-excluded`, and `result-limit`.

## Authoring migration

`chronology.replace` is a full replacement intent, not chronology CRUD. Its
closed request is `{action:"chronology.replace",expectedHead,change}` with
optional summary and idempotency key. `change` must contain a complete catalog,
one or more complete record annotation arrays, or both. Each replacement entry
has exactly one `id` or scoped `temporaryId`; `$calendar.*`, `$era.*`, and
`$chronology.*` references are resolved only in chronology identifier fields.
Preview and apply retain the ordinary expected-head, confirmation, atomic
compile, and receipt/idempotency rules. Older ordinal-only repositories report
`upgrade_required` / `WDL-MIG-V06-004` before a changeset is made.
Absent optional `summary` and `idempotencyKey` fields are omitted by the CLI;
an explicit JSON `null` is malformed so preview echoes always satisfy the
published non-nullable authoring schema.
