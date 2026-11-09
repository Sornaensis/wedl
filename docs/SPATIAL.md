# Spatial geometry, topology and history

These v1 queries read canonical authored declarations in the selected source/Git
snapshot. Default geometry, containment, paths, reachability, maps and anchors
read static author source. History members, route inspection and geometry can
select an exact story horizon; overlay membership always requires one.
Responses retain author snapshot metadata, and record details cite their source.
Neither spatial facts nor overlay presentation labels grant character knowledge.

## Queries

| CLI | GET route |
| --- | --- |
| `spatial location REFERENCE [--mode MODE] [--limit N]` | `/api/spatial/location?id=REFERENCE&mode=MODE&limit=N` |
| `spatial history REFERENCE [--tick T] [--order O] [--timeline NAME] [--limit N]` | `/api/spatial/history?record=REFERENCE&tick=T&order=O&timeline=NAME&limit=N` |
| `spatial route REFERENCE [--tick T] [--order O] [--timeline NAME]` | `/api/spatial/route?id=REFERENCE&tick=T&order=O&timeline=NAME` |
| `spatial path FROM TO [--mode MODE] [--metric METRIC] [--unit UNIT] [--limit N]` | `/api/spatial/path?from=FROM&to=TO&mode=MODE&metric=METRIC&unit=UNIT&limit=N` |
| `spatial reachable FROM [--mode MODE] [--limit N]` | `/api/spatial/reachable?from=FROM&mode=MODE&limit=N` |
| `spatial maps [--limit N]` | `/api/spatial/maps?limit=N` |
| `spatial anchors [--from-map MAP] [--to-map MAP] [--limit N]` | `/api/spatial/anchors?from_map=MAP&to_map=MAP&limit=N` |
| `spatial bbox MAP --min-x X --min-y Y --max-x X --max-y Y [--min-z Z --max-z Z] [--relation RELATION] [--tick T] [--order O] [--timeline NAME] [--limit N]` | `/api/spatial/bbox?map=MAP&min_x=X&min_y=Y&max_x=X&max_y=Y&min_z=Z&max_z=Z&relation=RELATION&tick=T&order=O&timeline=NAME&limit=N` |
| `spatial nearby MAP --x X --y Y [--z Z] --radius R [--tick T] [--order O] [--timeline NAME] [--limit N]` | `/api/spatial/nearby?map=MAP&x=X&y=Y&z=Z&radius=R&tick=T&order=O&timeline=NAME&limit=N` |
| `spatial overlays LOCATION --tick T [--order O] [--timeline NAME] [--audience LABEL] [--view LABEL] [--overlay ID] [--limit N]` | `/api/spatial/overlays?location=LOCATION&tick=T&order=O&timeline=NAME&audience=LABEL&view=LABEL&overlay=ID&limit=N` |

Optional HTTP parameters may be omitted. Author record/location/map selectors accept
IDs or unambiguous titles/aliases; `overlay` requires an exact overlay ID.
Parameters are closed. `history`, `route`, `bbox` and `nearby` accept optional
time; `order` or `timeline` without `tick` is an error. With a tick, order defaults
to `2147483647` (end of tick) and timeline to the world's default. Supply an explicit
order for exact endpoint checks. The other static operations reject time parameters.
No spatial endpoint accepts character or perspective parameters. Overlay `view`
selects an authored presentation label while the response stays `perspective:author`.
The local browser exposes the same graph, geometry, history, anchor and overlay controls;
map results are structured data, without a map renderer.

## Maps and location geometry

Map declarations use these exact coordinate conventions:

| `crs` | `axis_order` | `unit` |
| --- | --- | --- |
| `local-planar:NAME`, with a nonblank name | `[east, north]` | Nonblank authored unit |
| `EPSG:4326` | `[longitude, latitude]` | `degree` |

`bounds` contains exactly `min` and `max`, with matching two- or three-coordinate
arrays and a strict minimum below the maximum on every axis. EPSG:4326 longitude
bounds stay within `[-180,180]` and latitude bounds within `[-90,90]`. The
two-entry `axis_order` describes the first two coordinates even on a 3D map.

| `z_policy` | Required bounds and placement dimension |
| --- | --- |
| Omitted or `forbidden` | 2D |
| `required` | 3D |
| `optional-level` | 2D or 3D, fixed by that map's bounds |

Every placement, query coordinate and anchor/portal position must match the
selected map's dimension. `optional-level` does not permit mixed dimensions on
one map. Optional `origin` contains exactly a nonblank `label` and matching
`coordinates`; the origin may lie outside the map bounds. Optional `scale`
contains exactly a finite nonnegative `value` and nonblank `unit`. Both are
retained authored metadata and do not alter query coordinates or distances.

```yaml
crs: local-planar:harbor-levels
axis_order: [east, north]
unit: m
z_policy: required
bounds: {min: [-10, -10, 0], max: [10, 10, 5]}
origin: {label: Dock level, coordinates: [0, 0, 0]}
scale: {value: 1, unit: m}
```

A location may have one explicit `spatial` placement. The placement contains
exactly `map_id` and `geometry`; geometry contains exactly `kind` and
`coordinates`:

```yaml
spatial:
  map_id: map:harbor
  geometry:
    kind: polygon
    coordinates: [[-2, -2], [2, -2], [2, 2], [-2, 2], [-2, -2]]
```

`point` uses one coordinate array. `line` uses 2–10000 vertices; `polygon` uses
4–10000 vertices, including an exactly repeated first/last vertex. Polygons have
one closed ring. All vertices must share the map's dimension and lie within its
inclusive bounds. Absent or untagged-null `spatial` means unplaced. Map references
are exact existing IDs; canonical locations require canonical maps. Retired
declarations still validate but do not enter results.

Source and query coordinates are finite numbers with absolute value at most
9007199254740991. This prevents unsafe integer coercion into f64. Decimal
coordinates and distance calculations use f64 precision; authored integer
coordinates, bounds and metadata retain decimal-string JSON output.

Map listing returns canonical declarations in ID order, including dimensions,
CRS, axes, unit, z policy and any origin/scale. Geometry results identify each
location's `geometryKind` and retain its authored coordinate shape.

`bbox` compares each geometry's per-axis bounding envelope with the query box.
The default `intersects` includes any envelope overlap; `within` requires the
whole envelope inside the query box. Both include boundaries. This is an
envelope test, so a line or polygon may match an intersecting box without its
actual segments or filled area touching that box. Query boxes may extend beyond
the map and may have zero extent on an axis. Supply both `min_z` and `max_z` for
a 3D map; supplying only one is an error.

`nearby` includes geometry at Euclidean coordinate distance at most a finite
nonnegative radius. Radius and reported distance use the map's authored unit.
It measures to a point, the nearest line segment, or a filled polygon. For 3D
polygons, the first noncollinear plane supplies the containment
projection; an exterior projection or collinear ring uses boundary distance.
All supplied ordinates participate. Geographic queries use the authored degree
coordinates directly: there are no geodesics, longitude wrapping or inferred
altitude/unit conversions. `distance` is geometric and does not supply route
cost. Bbox and nearby examine only the selected map and return stable location
ID order, rather than distance ranking.

With an explicit horizon, geometry selects every location's effective authored
placement before filtering map membership, bounds, proximity or limits. This
discovers placements moved into the selected map, including locations without a
static placement. Dated results retain the same geometry rules and add exact
`at`, scope `author-as-of-spatial-history`, and selected-field citations. Static
map and anchor declarations remain unchanged.

## Literal map anchors

An `anchor` pairs two explicit positions on different maps:

```yaml
from: {map_id: map:harbor, coordinates: [0, 0]}
to: {map_id: map:harbor/coast, coordinates: [12.5, 55.7]}
conversion: These two survey positions identify the harbor.
```

Each position contains exactly `map_id` and `coordinates` and must satisfy its
map's dimension and inclusive bounds. A canonical anchor requires canonical
maps. Optional `conversion` is literal string prose. `spatial anchors` lists
canonical anchors in ID order; `from_map` and `to_map` filters preserve the
authored direction and do not match the reverse pairing.

Anchors do not transform coordinates or create travel edges. The
[pinned upstream query kernel](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/spatial_query.py)
has no executable cross-map transform engine; the delivered contract is literal
correspondence inspection, with no inferred interpolation, inverse or projection.

## Containment, routes and directed portals

A location's `parent_id` is an exact existing location ID. Omission or untagged
null means a root. The old `parent` spelling is rejected. Parent links must be
acyclic, cannot point to the same location, and require a canonical parent for a
canonical child. Containment, `links`, geometry and anchors do not imply travel.

A route uses these fields:

```yaml
from_location_id: loc_00000000000000000000000001
to_location_id: location:harbor/landing
direction: two-way
modes: [foot]
availability: open
uncertainty: exact
route_distance: {value: 30, unit: m}
duration: {value: 45, unit: seconds}
```

Endpoints must be exact existing, distinct locations. Direction is `one-way` or
`two-way`; two-way contributes an edge in each direction. Modes are nonempty,
unique, nonblank strings. Availability is `open` (default), `closed`, `restricted`
or `unknown`; only open routes are traversable. Uncertainty is `exact` (default),
`estimated` or `unknown`, and is returned without changing topology. Canonical
routes require canonical endpoints. Distinct IDs may represent parallel routes.

Optional `route_distance`, `travel_cost` and `duration` each contain exactly
`value` and `unit`. Values are finite nonnegative numbers, and units are nonblank
authored strings. Integer values are returned as decimal strings without losing
precision. The selected weighted metric is summed only across compatible units;
other metrics remain per-edge values. No unit conversion or missing-value
substitution is performed.

A `portal` declares `from_location_id`, `to` and nonempty, sorted, unique `modes`:

```yaml
from_location_id: loc_00000000000000000000000001
to: location:harbor/watchtower
modes: [foot]
```

The target may instead be a closed map position such as
`to: {map_id: map:harbor/levels, coordinates: [3, 4, 2]}`. Location targets must
exist and differ from the source; map targets satisfy the usual position rules.
Canonical portals require canonical endpoints/maps. Every portal is directed;
author a separate portal for reverse travel. Portal source forbids route-only
direction, availability, uncertainty and metric fields. A map-position target
appears in adjacency output but does not become a location edge, even if a
location has exactly those coordinates.

Location output includes the immediate parent, ancestors nearest-parent-first,
direct children, `outgoingRoutes` and separate `outgoingPortals`. Children and
outgoing records have stable ID ordering. The optional mode matches an exact
authored mode; without it, any mode qualifies.

The default path metric is `hops`: breadth-first traversal finds the fewest
open-route/location-portal edges, with stable edge-ID ties. Results include
`locationIds`, separate `routeIds` and `portalIds`, mixed ordered `edgeIds`, and
authored edge details. `reachable` returns all reachable location IDs in stable
ID order, including the origin. It follows the same directed edges and mode
filter, with no inferred transfers, permissions or map-position snapping.

### Weighted paths and exact units

Weighted metrics are `route_distance`, `travel_cost` and `duration`; they minimize
the sum of that authored field over routes only. Portals supply no weighted edge.
Equal totals prefer fewer hops, then lexicographic route-ID sequences. Zero-cost
cycles and parallel routes do not make selection nondeterministic.

Weights are comparable only within the same exact unit label. `--unit` selects
that unit; without it, exactly one unit must reach the endpoint. If multiple units
reach it, the query asks for an explicit unit instead of comparing them. Missing
metrics do not supply weighted edges; a query never substitutes hops, geometric
distance or another metric. Supplying a unit with `hops` is an error.

Integer weights retain exact precision, and accumulated totals can exceed the
u64 range. Decimal addition and ordering use the canonical decimal spelling of
each parsed YAML number; floating-point source literals still follow the YAML
parser's precision. For example, parsed 0.1 plus 0.2 is 0.3 and ranks below
0.30000000000000004. All weighted `total.value` results are decimal strings;
`total.unit` is the selected unit. Hops-only paths have `total:null`.

Same-location paths have zero hops and, when weighted, total value `"0"` with the
requested unit or null if omitted. With no qualifying path, output has
`found:false`, null hops and total, and empty path arrays. This does not imply
that no unweighted path exists when a metric or unit was requested. Every
traversed edge must match the optional mode filter.

## Explicit spatial history

Only locations and routes may own `spatial_history`, an ordered array of at most
1024 members. Each member contains exactly `id`, `aspect`, `valid` and `value`,
plus optional `replaces`. IDs are local to their owner: `spatialhistory_` followed
by 26 uppercase Crockford characters. They must be unique within that array.

| Owner | Aspect | Exact value |
| --- | --- | --- |
| Location | `placement` or `geometry` | Complete `{map_id: ..., geometry: ...}` using the existing shape, dimensions and bounds rules |
| Location | `parentage` | `{parent_id: LOCATION_ID}` or `{parent_id: null}` for a root |
| Route | `route-availability` | `{availability: open\|closed\|restricted\|unknown}` |
| Route | `route-metric` | Exactly one of `route_distance`, `travel_cost` or `duration`, containing the existing `{value, unit}` metric |

References are exact existing IDs. Canonical owners require canonical referenced
maps or parents; a parent cannot be the owner itself. Retired histories still
validate, though only canonical owners appear in query results.

Validity is either `{kind: timeless}`, applying on every declared timeline, or:

```yaml
kind: interval
timeline: main
start: {timeline: main, tick: 20, order: 0}
end: {timeline: main, tick: 29, order: 0}
```

An interval may omit either or both bounds. A supplied bound must contain exactly
`timeline`, signed i64 integer `tick` and signed i32 integer `order`, with the same
declared timeline as the interval. Explicit null bounds are refused. Bounds are
ordered and inclusive, including same-tick order changes; an omitted bound is
open to the corresponding end of the time axis. Public coordinates are decimal
strings, preserving values beyond JavaScript's exact integer range.

Members targeting the same effective field cannot overlap unless they form an
explicit correction chain. Placement and geometry both target `spatial`; route
metrics conflict only when they name the same metric field. `replaces` must name
an earlier member of the same owner, retaining its exact aspect, literal validity
mapping and metric target. Forward references, forks, changed applicability and
overlapping independent claims are refused. Both original and correction remain
in source. Parentage is checked across all locations at every relevant interval
boundary and immediately after inclusive ends, including i32/i64 rollover, so
temporary or fallback containment cycles cannot publish.

Effective reads select applicable members and suppress replaced predecessors.
Each selected field overrides only that field; outside applicability, the
original static value is used. Static absence remains absent, and an expired
metric never carries forward into a later availability interval. These operations
borrow values from the actual source record without rewriting its frontmatter,
raw bytes, revision or fingerprint.

`spatial history REFERENCE` returns all authored members in source order,
including corrections, with scope `author-all-time-spatial-history`. Supplying
a horizon returns only applicable unsuperseded members. Every item retains its
literal value, validity and correction-chain citations, oldest to selected.
History reads do not manufacture fallback members when no history applies.

`spatial route REFERENCE` inspects one canonical route, including a closed route.
Without time it returns static fields; with time it resolves availability and
each metric independently. Missing metrics are null in this inspection response.
`fieldCitations` identifies historical chains or authored static fields; a missing
field has no invented citation. Dated geometry similarly returns a selected
`citation` and the full `citations` chain; static fallback cites `spatial`.

Location adjacency, paths and reachability continue to use static source and
reject time parameters. Historical parentage is available through the history
read. Character admission of spatial history awaits the shared dated narrative
evidence rules; canonical histories and local author filters do not grant that
knowledge. Valid history changes leave complete generic character context/search
responses unchanged.

## Static and time-bounded overlay membership

An `overlay` explicitly names its member locations and presentation scope:

```yaml
lifecycle: time-bounded
membership:
  location_ids: [loc_00000000000000000000000001, location:harbor/landing]
audience: [author]
perspectives: [author]
valid:
  start: {timeline: main, tick: 10, order: 0}
  end: {timeline: main, tick: 12, order: 0}
```

`membership` contains exactly `location_ids`. Membership IDs, `audience` and
`perspectives` are sorted, unique arrays; empty arrays are permitted. Members
must be exact existing location IDs, canonical when the overlay is canonical.
Audience/view values are nonblank literal labels. They are presentation filters,
not character identities, entitlement or evidence. `faction`, `institution` and
`calendar` do not define membership and are rejected as operational overlay fields.

`lifecycle: static` must omit `valid`. `time-bounded` requires exactly `start` and
`end`; each endpoint contains exactly `timeline`, signed i64 `tick`, and signed
i32 `order`. Endpoints must be ordered on one declared timeline. Membership is
applicable at both endpoints inclusively and never crosses timelines.

Queries require `tick`; `order` defaults to `2147483647` (end of tick), and an
omitted timeline uses the world's default. Supply explicit order for exact
endpoint checks: the example applies at tick 12/order 0, but not order 1.
`audience` and `view` both default to `author` and must both appear in the
overlay's corresponding arrays. A static overlay applies independently of the
queried time, while the query horizon must still name a declared timeline.

Scope is checked before membership, time applicability or result budgets. An
exact overlay selector returns the same unavailable error for an absent, retired
or differently scoped record. A scoped record outside its time window or without
the requested location yields no match. Only matching items consume the limit;
results use stable overlay-ID order and retain `perspective:author` with scope
`author-as-of-overlay-membership`.

## Capabilities, authoring and limits

| Declaration | Required world capabilities |
| --- | --- |
| Maps, placements, anchors | `spatial-core-v1`, `geometry-v1` |
| Routes, portals | `spatial-core-v1`, `route-v1` |
| Overlays | `spatial-core-v1`, `overlay-v1` |
| Any `spatial_history`, including an empty array | `spatial-core-v1`, `spatial-history-v1` |
| Historical placement/geometry | The history capabilities plus `geometry-v1` |
| Route history | The history capabilities plus `route-v1` |

A portal's map target also relies on a valid declared map. To use all delivered
features, retain other world capabilities and add these in registry order:

```yaml
capabilities: [spatial-core-v1, geometry-v1, route-v1, overlay-v1, spatial-history-v1]
```

World capability declarations still use explicit source edits. Typed creation
and queries never enable features automatically. Empty catalogues do not require
capabilities when no corresponding source declarations exist. Declaring a
capability does not certify its still-unimplemented parts.
Explicit history reads and dated geometry/route inspection require the history
capabilities even when the selected owner has no history and uses static fallback.

All spatial records use canonical or retired status; all source shapes validate,
while only canonical records participate in results. Tagged values and malformed
typed fields fail validation. The shared `record create`/`record update` workflow
supports these spatial fields alongside each kind's common authoring fields:

| Kind | Spatial typed fields and creation requirements |
| --- | --- |
| `location` | Optional `parent_id`, `spatial`, `spatial_history` |
| `map` | Required `crs`, `axis_order`, `unit`, `bounds`; optional `z_policy`, `origin`, `scale` |
| `route` | Required `from_location_id`, `to_location_id`, `direction`, `modes`; optional `availability`, `uncertainty`, `route_distance`, `travel_cost`, `duration`, `spatial_history` |
| `anchor` | Required `from`, `to`; optional `conversion`, `status` |
| `portal` | Required `from_location_id`, `to`, `modes`; optional `status` |
| `overlay` | Required `lifecycle`, `membership`, `audience`, `perspectives`; `valid` required for time-bounded lifecycle; optional `status` |

Creates default to canonical status; route creation also defaults to open
availability and exact uncertainty. Map and route status remain outside their
typed request contracts; anchor, portal and overlay status can be set or updated
to canonical/retired. Namespaced spatial IDs use safe `<kind>-SHA256.md` filenames.

Omission preserves fields; supplied maps/lists replace whole values. Requests
reject nulls, so removing a top-level parent, placement, metric or optional
metadata still requires a direct source edit. Invalid references, cycles or map
changes that invalidate placements/endpoints produce a reviewable candidate
with `canApply:false`; apply refuses publication. Preview, fingerprints, exact
after-source hashes and atomic save behavior are shared with other typed kinds.
For location/route requests, a supplied `spatial_history` replaces the complete
array. Omission preserves it, `[]` clears its members, and null is refused.
Historical `{parent_id: null}` remains valid domain data. The existing twenty-four
editable non-world kinds and their common chronology annotations are unchanged.

Limit is 1–1000, default 100. Location applies it to each complete list; path to
the number of result locations, including endpoints; reachable to its complete
location set, including the origin. Maps, geometry, anchors, overlays and history apply
it to their complete matching item lists. Overflow fails instead of truncating.
History source can contain 1024 members, while the public complete-result limit
is at most 1000; a larger selected result is refused. Route inspection has no list
limit because it returns exactly one route.
Path/reachability traversal also refuses more than 10000 expansions; weighted
traversal distinguishes location/unit states. Geometry retains the 10000-vertex
source cap and the existing source/draft size limits.

## Executable examples and remaining scope

[Authoring examples](../examples/authoring/README.md) 11–18 establish containment,
routes, a local map and point placements. The extension continues with:

| Examples | Operation |
| --- | --- |
| 45–46 | Create Harbor Boardwalk line and Harbor Courtyard polygon |
| 47–48 | Create Harbor Levels 3D map with origin/scale and Harbor Watchtower point |
| 49–50 | Create EPSG:4326 Harbor Coast Survey map and Harbor Survey Mark point |
| 51 | Create literal Harbor Survey Correspondence anchor |
| 52–53 | Create Watchtower Entrance location portal and Ladder Exit Position portal |
| 54–55 | Create static Maintenance Area and time-bounded Harbor Flood Warning overlays |
| 56 | Update `portal:harbor/watchtower` to retired |
| 57 | Update `overlay:harbor/flood-warning` through tick 14/order 0 |
| 58–60 | Add Landing placement history, correct its coordinates, then add historical root parentage |
| 61–62 | Add independent route metrics/availability and correct the historical travel cost |
| 63 | Clear the Landing history array, retaining its static placement and the route's history |

Process requests in numbered order with the declared capabilities and the usual
preview/hash/apply workflow. Example 53 deliberately targets the same coordinates
as the watchtower without inferring a location edge. Example 56 removes the
explicit location portal from current snapshot traversal; it does not create a
historical retirement boundary. Example 57 replaces the authored validity window.
Examples 58–63 use the same reviewed writer and add no source records; all
63 requests leave a valid 40-record world. The final clear removes the Landing's
historical overrides while preserving its ordinary source fields.

## Character history admission

`character spatial-history VIEWER OWNER --tick T` and GET
`/api/character/spatial-history?character=VIEWER&owner=OWNER&tick=T` use the
shared [character gate](NARRATIVE_ADMISSION.md). The owner must be an exact,
independently known, scoped canonical location or route. The horizon is required;
optional timeline/order retain exact story coordinates and limits are 1–1000.
Missing, wrong-kind and inaccessible owners share a generic unavailable error.

Members must apply at that horizon. A placement's map and a non-null historical
parent must also be independently known and admitted before the member can
participate in correction suppression. Unknown endpoint references do not teach
the map or parent. Only admitted corrections suppress predecessors. Character
items retain their explicit historical payload and safe identities, without
author source citations or hidden replacement IDs.

The character history list supplies no static placement fallback, geometry
inference or route traversal. Examples 150–152 independently acquire Harbor and
its map: the corrected placement appears at tick 15 and disappears when map
knowledge is forgotten at tick 18. An overlay's validity window remains separate
from location/route history. Richer narrative context and browser editing remain
in [PARITY.md](PARITY.md).
