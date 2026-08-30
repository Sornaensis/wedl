# ADR 0004: Spatial domain, queries, and schema-version contract

- **Status:** Accepted
- **Owner:** WEDL maintainers
- **Approved:** 2026-08-30 by Project owner (user)
- **Decision examples:** [spatial-schema-v07.yaml](examples/spatial-schema-v07.yaml) and [spatial-query-v1.yaml](examples/spatial-query-v1.yaml)
- **Depends on:** the shared-world and global-StoryTime decisions in [ADR 0002](0002-shared-world-concurrent-narrative-threads.md) and [ADR 0003](0003-calendar-and-historical-chronology-semantics.md)

## Context and boundary

WEDL needs an authored spatial vocabulary for large shared worlds while retaining
coordinate-free locations.  This is a ratified contract and test vector, not a runtime, source-parser, transport, renderer, migration command, provider
choice, or UI implementation.  Markdown and Git remain authoritative; a future
SQLite index is disposable and must be fingerprint-rebuilt from normalized
source.  No existing world gains coordinates or inferred routes by this ADR.

The contract reserves the `wedl/v0.7` capability envelope.  It is deliberately
capability-based: readers negotiate features and reject unsupported declared
features rather than treating the version string as permission to guess.
Implementing the envelope, upgrading source, or accepting v0.7 is separately
owned work.

## Decision

### Hybrid source model and identity

The future source shape is hybrid: ordinary `location` records retain their
stable WEDL IDs and optional typed `spatial` references; separately identified
`map`, `overlay`, and `route` declarations describe only authored facts.  IDs
are opaque, globally unique, lowercase stable paths such as `map:riverward`,
`location:archive`, `overlay:ward`, and `route:archive-gate`.  A reference is
always an ID path, never title matching or proximity inference.

A map declares `id`, `crs`, `axis_order`, coordinate `unit`, optional `origin`,
`scale`, `z_policy`, and finite `bounds`.  Local planar CRS values are first
class; optional geodetic CRS values must declare their axis order and units.
Coordinates carry their map ID and use that map's declared CRS and unit.  Maps
with unequal map IDs, CRS, axis order, or units have no comparable metric unless
an authored anchor or portal supplies a stated conversion.  A location may have
no geometry and remains fully valid.

Geometry is restricted to validated `point`, `line`, and `polygon` values with
finite signed numeric ordinates and declared map membership.  It denotes a
shape, not containment, a route, movement, distance, access, or visibility.
Structural `parent_id` is a single authored acyclic location path.  It differs
from zero-or-more overlays, which may overlap and are not a parent tree.

Authored anchors explicitly link a local coordinate on two maps; a portal explicitly
links two locations or map positions.  Neither creates a general common metric.
A route has a stable ID, authored ordered endpoints, direction (`one-way` or
`two-way`), explicit modes, optional route distance, optional cost, optional
duration, availability/status, uncertainty, and stable source order.  A reverse
route exists only when authored or when the same route explicitly says
`two-way`; nearby points never create edges, reverse edges, or movement.

### Time, overlays, and non-inference

`StoryTime` stays the one global `(timeline,tick,order)` ordering from ADR 0002.
Canonical source frontmatter uses signed integer `tick` and `order` values.
Public CLI/HTTP transport uses canonical signed decimal strings for `tick` and
`order` (for example `"12"` and `"0"`): numeric JSON values, `+` prefixes, and
noncanonical leading zeroes are rejected.  The public form preserves the exact
tuple; it is not a presentation-only approximation.  Ticks do not imply a
route duration, civil date, elapsed time, or calendar conversion.  Geometric
distance, route distance, travel cost, duration, and StoryTime are distinct
typed values and must never be substituted or calculated from one another.

An overlay lifecycle is either `static` (no time predicate) or `time-bounded`
with an explicit inclusive StoryTime interval.  It is considered only after
exact timeline, horizon/as-of, audience, and perspective filters.  Audience and
perspective are separately authored opaque IDs; both must match exactly.
Restricted overlays are filtered before counts, ordering, pagination, and
serialization.  A location-catalogue `overlay-as-of` query returns successful
visible-only results: an otherwise identical location with no overlays and a
location with only hidden overlays are observationally identical.  `forbidden`
is reserved for an explicit overlay-ID query-scope authorization failure, where
the caller has already named that ID; it is never returned by a catalogue query.
Membership is only an authored reference or geometry predicate declared by the
overlay; geometry alone never invents political, geographic, or structural
membership.

Faction, institution, and calendar boundaries are not v0.7 overlay semantics.
They cannot be encoded as audience or perspective aliases, inferred from a
label, or used as a hidden filter.  Their lifecycle/visibility semantics are
deferred to the **Scope-governance maintainer** as a follow-up decision before a
future capability advertises them.  Until then, source that declares one is
`SPATIAL-REQUEST-001` invalid rather than a best-effort authorization result.

The system must not infer coordinates, map conversions, containment, topology,
route access, reverse travel, availability, duration, calendar meaning,
audience access, or an answer from titles, prose, nearest neighbors, or prior
results.  An unavailable or incompatible value is a closed diagnostic, not an
empty success or best effort.

### Query and transport contract

The `wedl-spatial/v1` request/response vectors define five reads: `containment`,
`bbox`, `nearby`, `path`, and `overlay-as-of`.  Every concrete request vector
is self-contained and explicitly includes `protocol`, `revision`,
`capabilities`, a `limit` (bounded 1..100 for valid requests), and `cursor`
(`null` where no next page is requested).  The sole out-of-range limit vector
is an intentionally invalid diagnostic case.  The shared YAML `common` block is constraints and
documentation only, never inherited request data.  Results have deterministic
order: `(source_ordinal, stable_id)` for catalogues, `(cost, source_ordinal,
stable_id)` for paths only after a declared requested cost metric, and no
implicit distance ranking.  Cursor input binds the normalized request and
revision.  A changed revision or request is `SPATIAL-CURSOR-001`.

Closed outcomes are `ok`, `invalid`, `unavailable`, `forbidden`, and `limit`.
The vectors define exact codes for malformed input, absent geometry,
incompatible metrics, no authored path, unauthorised overlay, and budgets.
HTTP and CLI must preserve the same code, JSON shape, StoryTime tuple, order,
cursor semantics, and exit classification; they may not turn an unavailable
spatial answer into a 200 empty result.  Future compiled reads must agree with
normalized source on the same revision and return the source ID/provenance.

### Offline, renderer, and asset trust boundary

List and structural-tree views work offline and without geometry.  All remote
tiles, geocoders, routing providers, and external assets are disabled by
default and opt in only through an explicit trust/privacy policy.  No remote
request may contain world source, hidden overlays, titles, coordinates, or
audience identity unless that policy allows each field.  Renderers consume only
validated local geometry and allowlisted assets; they never execute SVG/script
content, fetch a provider by default, or decide authorization.  Renderer
failure leaves an accessible textual list/tree and closed diagnostics.

### Version, migration, and recovery

`wedl/v0.3`, `wedl/v0.5`, and `wedl/v0.6` remain readable legacy source
versions and represent no spatial capability.  `wedl/v0.7` is the future
spatial envelope.  Mixed worlds are rejected until a future migration creates
one coherent target source.  That migration must be lossless, previewable,
atomic, idempotent, expected-HEAD protected, and Git-reversible; it must retain
legacy location IDs and links, never fabricate spatial declarations, and rebuild
SQLite from source.  Rollback is `git revert` (or restoring the expected source
revision) followed by index disposal/rebuild; no database rollback is
authoritative.

Spatial migration explicitly waits for the future generational-history ADR.
This ADR does not claim completion of that work; a future migration
must use the generational contract if historical place identity or lineage needs
to be represented.

| Source version | Read | Spatial capability | Mixed-world policy | Migration state |
| --- | --- | --- | --- | --- |
| v0.3 | legacy | none | reject with v0.7 | no conversion implied |
| v0.5 | legacy thread grouping | none | reject with v0.7 | no conversion implied |
| v0.6 | chronology-enabled | none | reject with v0.7 | no conversion implied |
| v0.7 | capability-negotiated | declared features only | one version per world | future explicit migration |

### Performance envelope and rollout

Before implementation, CI vectors must cover 100,000 locations, depth 128,
10,000 siblings, 32 maps/CRSs, signed large coordinates, 250,000 route edges,
100 portals, 10,000 changing overlays, and hostile text/assets.  A request may
inspect at most 100 returned records, 1,000 route expansions, 2,000 overlay
candidates after authorization, and 10,000 geometry vertices; exceeding a cap
returns `SPATIAL-LIMIT-001` with no partial answer.  These are contract budgets,
not authorization for a full-world browser scan.

Rollout order is: schema validator and source round trip; normalized compiler
and source/index parity; bounded CLI and HTTP; accessible offline UI; then the
explicit migration.  Each stage must keep legacy coordinate-free worlds valid.
Each stage rolls back by removing its capability advertisement and reverting
source changes; the next stage is blocked on the preceding evidence.

## Risks and open decisions

| Open decision | Owner | Blocking boundary |
| --- | --- | --- |
| Exact parser grammar and diagnostic locations | Schema maintainer | v0.7 source acceptance |
| Index storage/query algorithm and benchmark harness | Compiler maintainer | compiled spatial reads |
| CLI/HTTP status mapping and cursor encoding | API maintainer | public transport |
| Accessible map interaction and local asset policy | UI maintainer | visual renderer |
| Faction, institution, and calendar visibility semantics | Scope-governance maintainer | any future overlay capability for those boundaries |
| Generational location identity/history semantics | Generational-history ADR owner | spatial migration |
| External-provider allowlist and privacy defaults | Security owner | any remote integration |

## Consequences

Future implementation has a bounded, testable contract without changing the
current runtime.  Coordinate-free worlds, the shared global StoryTime, and
offline use remain first-class.  Schema-dependent work remains blocked until
the listed owners implement and independently review its stage.
