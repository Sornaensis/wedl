+++
schema = "adrai/decision/v1"
adr = "A01M48ZRCS8CP9EC5H5YH2MRVQ4"
record = "R01M48ZRDP569SZ6G10H1R6822T"
title = "Spatial coordinated v0.7 source contract"
summary = "Migrated SPATIAL_SOURCE_CONTRACT.md; preserved exact spatial semantics and current delivery boundaries."
domains = ["spatial"]
+++

## Authority and provenance

Migrated from `docs/SPATIAL_SOURCE_CONTRACT.md` at Git revision `aac36745258e6f93487481564dc1c40daeb327ef`. Existing approval, aliases, historical editions and semantic authority remain intact; this records current implementation boundaries without inventing ratification or redesigning the product.

# Coordinated spatial v0.7 source contract

Spatial source uses the single coordinated `wedl/v0.7` envelope.  Capabilities
are declared only by the world record in this exact order:
`generational-core-v1`, `spatial-core-v1`, `geometry-v1`, `route-v1`, `overlay-v1`, and optional `generational-knowledge-v1`. Geometry, routes, and overlays require `spatial-core-v1`; `generational-knowledge-v1` is last and requires `generational-core-v1`. A component never infers an undeclared capability.

The coordinated declaration is never empty and includes at least one of `generational-core-v1` or `spatial-core-v1`. A generational-only envelope can contain coordinate-free ordinary location records; operative spatial fields and spatial kinds still require `spatial-core-v1`.

Every authored map, placement, anchor, portal, route, or overlay requires
`spatial-core-v1`; geometry and coordinate anchors additionally require
`geometry-v1`; portals and routes require `route-v1`; overlays require
`overlay-v1`.  Retained `loc_…` location IDs remain legal endpoints alongside
new `location:` IDs. The new colon IDs use lower-case opaque slug paths;
every segment is portable (including on Windows, where reserved device names
are forbidden) and maps to `story/maps/`, `locations/`, `anchors/`,
`portals/`, `routes/`, and `overlays/` without placing a colon in a filename.

The component vocabulary is opaque `map:`, `location:`, `anchor:`, `portal:`,
`route:`, and `overlay:` references.  Maps declare CRS, axis order, unit, and
finite bounds. Locations may remain coordinate-free. Geometry is authored
point/line/polygon data. Location containment uses canonical `parent_id`, and
directional prose links retain the ordinary `links` grammar; they are authored
facts, not route declarations. `parent` is accepted only as an explicit legacy
compatibility input and serializes as `parent_id`. Parents are structural and acyclic, and anchors or
portals do not create a general coordinate conversion.  Routes are directed
and typed.  Overlays are static or inclusive, same-timeline bounded intervals.
Ticks are signed integers, not duration or calendar conversion.

Only `local-planar:<name>` maps with `[east, north]` axes and an authored unit,
or `EPSG:4326` maps with `[longitude, latitude]` axes and `degree` units, are
canonical. Coordinates are finite, dimensionality-checked, and within their
authored bounds. Unknown operative fields are rejected; `x-*` fields are
preserved as inert extension data. Canonical v0.7 serialization places
provenance last and preserves the Markdown body byte-for-byte after UTF-8
decoding. Legacy schemas retain their established body normalization and
therefore their historical canonical bytes.

`z_policy: forbidden` requires two ordinates everywhere on that map;
`optional-level` permits either two or three, but the map bounds, origin, and
every geometry/position on that map must use the one authored dimensionality;
`required` requires three. Routes and portals have a non-empty, sorted,
duplicate-free `modes` list. Detailed location links keep the established
`description`, `label`, and `summary` rule: when present, each is non-empty
text.

The v0.7 world envelope is homogeneous and world-only: it has the exact
ordered capability list and closed `timelines` declarations of
`{id,label[,origin:{tick,label}]}`, a listed `default_timeline`, and closed
`chronology: {calendars,eras,anchors}` when chronology is declared. The
component runs the complete v0.6
closed chronology grammar, including calendar/era/anchor IDs, ordering,
bounds, and references. It inherits chronology semantics; it
does not reinterpret calendar labels as spatial time or route duration.

The local-only `upgrade-v07` migration activates the coordinated v0.7 source envelope and compiled-cache projection. Migration itself does not create an HTTP migration route. Existing public spatial queries, explorer/UI, and confirmed changeset intents are separate implemented surfaces described below.

The inherited chronology schema is A01M48XWQD0809VQ3RD9NBYPSKB; the existing v0.7 compiled versus v0.6 advertised catalogue/replacement distinction is recorded in A01M48XZJ6V1X1QSM4QEZJZS81M. Spatial capabilities do not override it.

## Current delivery and authority boundary

Homogeneous `wedl/v0.7` is accepted by the generic source loader, validator, and compiler. The current SQLite schema is `wedl-sqlite/v15` and the spatial compiler fingerprint is `wedl-spatial-index/v5`. Markdown/Git remains canonical and all spatial SQLite structures are disposable projections.

The eight-operation `wedl-spatial/v1` codec is implemented in `src/wedl/spatial_api.py`: containment, children, bbox, nearby, adjacency, reachability, path, and overlay-as-of. The separate `wedl-spatial-explorer/v1` adapter implements catalog, places, viewport, layers, and routes. Browser explorer/editor and ID-only `spatial.<kind>.create|update` authoring intents are also present. Those intents compile to ordinary confirmed changesets and require v0.7, exact expected HEAD, idempotency identity, closed payloads, and the established confirmation flow. They edit authored source; they do not authorize inferred topology, geometry, metrics, horizons, or identity.

An explorer author lens selects audience/perspective for presentation. It does not confer trusted character identity or confidentiality. Exact authored visibility and horizon filters continue to apply before counts and paging. The original spatial ADR A01M48NPY19KENPF8EWB8A4W95W retains its 2026-08-30 approval-stage wording and vectors; this migration does not silently rewrite that approval as a later deployment claim.

Executable component fixture: [spatial-source-component-v07.yaml](../../examples/spatial-source-component-v07.yaml). Its semantic cases and original stage comments are preserved unchanged; those comments do not state current generic runtime acceptance.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiYWFjMzY3NDUyNThlNmY5MzQ4NzQ4MTU2NGRjMWM0MGRhZWIzMjdlZiIsImkiOiJzaGEyNTY6NEs5RElSU2Z0WWsyTjV6cm0tSjRwZGxGZFZNaUdMZVUwZGtDdzRYa0FwOCIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WlJEUDU2OVNaNkcxMEgxUjY4MjJUIiwib3AiOiJPMDFNNDhaUkRQNTY5U1o2RzEwSDFSNjgyMlQiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpWT1hXYzRRVklYTEcyYU9ZN3NON29sWkNDanRJdjUxTWUzdGxtTlNfSTVJIiwidCI6MTc5MTMwMzEwNDE5NywidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
