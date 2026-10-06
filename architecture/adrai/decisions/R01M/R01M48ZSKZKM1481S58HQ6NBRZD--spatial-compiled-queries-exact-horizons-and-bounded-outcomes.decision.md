+++
schema = "adrai/decision/v1"
adr = "A01M48ZSKSZ3EKW7EZX5HDTSRET"
record = "R01M48ZSKZKM1481S58HQ6NBRZD"
title = "Spatial compiled queries, exact horizons and bounded outcomes"
summary = "Migrated SPATIAL_QUERY_CONTRACT.md; preserved exact spatial semantics and current delivery boundaries."
domains = ["spatial"]
+++

## Authority and provenance

Migrated from `docs/SPATIAL_QUERY_CONTRACT.md` at Git revision `aac36745258e6f93487481564dc1c40daeb327ef`. Existing approval, aliases, historical editions and semantic authority remain intact; this records current implementation boundaries without inventing ratification or redesigning the product.

# Compiled spatial query contract

`wedl.spatial_query.SQLiteSpatialStore` is a typed, compiled-SQLite-only
boundary for the validated v0.7 spatial projection. It does not load Markdown,
write source, compile a world, mutate characters, convert calendars, invoke a
provider, or expose HTTP/CLI behavior. Each outcome is revision-citable and
uses the closed states `ok`, `invalid`, `unavailable`, `forbidden`, and `limit`
with the ratified `SPATIAL-*-001` codes.

Each closed non-success may additionally carry one typed internal subreason:
`unknown-coordinate`, `unknown-metric`, `unavailable-edge`, `closed-edge`,
`cross-map-discontinuity`, `incompatible-unit`, `ambiguous-unit`, or
`unreachable`. These are machine-readable
distinctions beneath—not replacements for—the ratified public reason code.

The store supports authored parent paths and children; exact same-map bounds
`intersects`/`within`; Euclidean same-map nearby candidates; directed route and
location-portal adjacency/reachability; typed weighted route paths; and
overlay-as-of reads. Geometry, routes, portals, and overlays are independently
capability-gated. Missing geometry and unavailable/closed routes are never
treated as empty topology; an endpoint portal to a map position is terminal and
does not imply containment or a route.

Bounds use the SQLite Btree projection as a candidate boundary and retain exact
numeric post-filtering. Nearby first culls by bounds and then computes the exact
point-to-authored point, line, or polygon geometry distance, so a bounding-box
corner cannot become a false nearby result. A stale candidate with missing or
malformed coordinates closes as `SPATIAL-GEOMETRY-001` with the
`unknown-coordinate` subreason. Nearby comparisons require identical authored
map, CRS, axes, unit, and dimensions; anchors never create a conversion. Route paths
are directed, use `availability: open`, exact requested modes, an authored
metric, one metric unit per path, and at most 1,000 expansions. Missing metrics
are never zero. Results use source ordinal then stable ID, except weighted path
ties use exact cost then authored route ordinal/ID. Integral SQLite metrics stay
integers through accumulation (including values above 2^53 and totals above
signed 64-bit); finite non-integral SQLite numbers are compared through their
exact stored binary value. A non-finite, negative, or non-numeric stale-cache
weight is unavailable, never coerced. Path result cardinality is also bounded
1–100 and an overlength shortest path returns SPATIAL-LIMIT-001 without a
partial route.

Successful catalogue, reachability, and path payloads always state
`partial: false` and `unknown: false`; a bounded query never returns a partial
success. Metric summaries additionally state `complete: true`, `partial: false`,
and `unknown: false`. Route-based successes expose their applied exact `modes`
and `availability: ["open"]` filters. A missing metric, closed/restricted/unknown
edge, absent cross-map authored connection, and lack of any directed path remain
closed outcomes, distinguished by their internal subreason rather than a
fabricated zero or empty result. A unitless path may succeed only when exactly
one authored unit reaches its endpoint; multiple complete units are
`ambiguous-unit`, and a requested unit with no compatible route is
`incompatible-unit`. Costs are compared only within one resolved unit. A
zero-length identity path preserves an explicitly requested unit.

Containment follows indexed location primary-key parent lookups, bounded to
1,000 ancestors. It never materializes or scans the location catalogue. Missing
parents and cycles in a disposable corrupt cache close as unavailable; exceeding
the traversal cap closes as limit. Adjacency, reachability, path, and overlay
queries validate location endpoints before reading edges or memberships. An
absent endpoint is unavailable, while an existing disconnected location or an
existing location with no visible overlay remains a successful empty control.

Overlay reads apply exact timeline/horizon, audience, perspective, and
membership before the 2,000-candidate budget, ordering, cursor, or output.
Catalogue hidden overlays are indistinguishable from no overlays. A forbidden
outcome is reserved for a hidden explicit overlay ID. Every successful overlay
payload contains its exact `filters.horizon` StoryTime tuple alongside audience
and perspective filters. StoryTime is the exact
signed `(timeline, tick, order)` tuple and never implies a duration or calendar
conversion.

Pages are bounded to 1–100 results. Their opaque keyset cursor binds the
normalized request and exact revision; misuse returns `SPATIAL-CURSOR-001`.
Nearby and path apply the same closed limit boundary even when no page cursor is
returned. Compiled capability context is emitted in validated v0.7 registry
order, preserving combined generational and spatial declarations rather than
lexically re-sorting them.

Conformance tests load the canonical query vector and source fixture, then
exercise source/compiled fact parity, real weighted ties and cycles, absent
endpoints, cross-map discontinuity, positive keyset paging, signed-i64
StoryTime endpoints, exact and mixed metrics, stale-cache hierarchy and registry
corruption, repeated-order properties, and authorization-before-budget overlay
leak controls. The reproducible benchmark uses the complete ratified envelope:
100,000 locations at depth 128, 32 maps, 250,000 directed route rows, 100
portals, and 10,000 changing overlays.

## Bounded explorer candidate path

The core bbox method retains its Btree fallback without a candidate budget. A bounded explorer viewport passes a candidate budget and requires the map-scoped RTree; it returns an unavailable geometry outcome when that index is absent. It applies exact NUMERIC predicates to RTree candidates, closes as limit if the candidate budget is exceeded, then retains authored source ordinal/ID ordering. Optional index failure never permits partial candidate tables or fabricated empty success. Explorer lens choices are presentation selections, not trusted character identity.

The full-envelope query benchmark constructs a synthetic in-memory projection. Its recorded outcomes, medians, counts, and digests are immutable historical measurements at `docs/reports/spatial-query-benchmark.json`; they do not qualify authored-source loading/validation, full API transport, or browser performance. The method and bounded budgets remain architectural requirements; measured values remain in the report.

## Current delivery and authority boundary

Homogeneous `wedl/v0.7` is accepted by the generic source loader, validator, and compiler. The current SQLite schema is `wedl-sqlite/v15` and the spatial compiler fingerprint is `wedl-spatial-index/v5`. Markdown/Git remains canonical and all spatial SQLite structures are disposable projections.

The eight-operation `wedl-spatial/v1` codec is implemented in `src/wedl/spatial_api.py`: containment, children, bbox, nearby, adjacency, reachability, path, and overlay-as-of. The separate `wedl-spatial-explorer/v1` adapter implements catalog, places, viewport, layers, and routes. Browser explorer/editor and ID-only `spatial.<kind>.create|update` authoring intents are also present. Those intents compile to ordinary confirmed changesets and require v0.7, exact expected HEAD, idempotency identity, closed payloads, and the established confirmation flow. They edit authored source; they do not authorize inferred topology, geometry, metrics, horizons, or identity.

An explorer author lens selects audience/perspective for presentation. It does not confer trusted character identity or confidentiality. Exact authored visibility and horizon filters continue to apply before counts and paging. The original spatial ADR A01M48NPY19KENPF8EWB8A4W95W retains its 2026-08-30 approval-stage wording and vectors; this migration does not silently rewrite that approval as a later deployment claim.

Executable component fixture: [spatial-source-component-v07.yaml](../../examples/spatial-source-component-v07.yaml). Its semantic cases and original stage comments are preserved unchanged; those comments do not state current generic runtime acceptance.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNDhmYjJiNTI5MjE0YWNhNzAwODI0MTQ0M2RjZTJiY2E1Y2UyOGVhYSIsImkiOiJzaGEyNTY6OG9mNWhpRy0tYW9zdXNtRkU3aUNNM1g3dmxXU0FSR1BoOGF5MUtKSlhFYyIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WlNLWktNMTQ4MVM1OEhRNk5CUlpEIiwib3AiOiJPMDFNNDhaU0taS00xNDgxUzU4SFE2TkJSWkQiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpUWGpLQUJoQW5fX2ZJN1RZOFB6eEd4bVdfaGE1REJxUDhyOFpHazV2em1RIiwidCI6MTc5MTMwMzE0MzQxMSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
