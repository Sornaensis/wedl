+++
schema = "adrai/decision/v1"
adr = "A01M48ZRZ2FB3S5BWBCBK9KS13N"
record = "R01M48ZRZ8C70DQ0N4M3HQ51M4Y"
title = "Spatial disposable projection, acceleration and benchmark methodology"
summary = "Migrated SPATIAL_INDEX_CONTRACT.md; preserved exact spatial semantics and current delivery boundaries."
domains = ["spatial"]
+++

## Authority and provenance

Migrated from `docs/SPATIAL_INDEX_CONTRACT.md` at Git revision `aac36745258e6f93487481564dc1c40daeb327ef` and architectural methodology from `docs/SPATIAL_INDEX_BENCHMARK.md`. Existing approval, aliases, historical editions and semantic authority remain intact; this records current implementation boundaries without inventing ratification or redesigning the product.

# Spatial SQLite projection contract

The `wedl/v0.7` spatial component is accepted by the generic loader and compiler. `wedl.spatial_index` constructs its disposable projection after complete validation. It was originally developed as an opt-in component projection before generic registration; Markdown remains canonical.

`spatial_capability`, `spatial_map`, `spatial_location`,
`spatial_location_vertex`, and `spatial_hierarchy` preserve declared
capabilities, maps, authored geometry vertices and bounds, and direct parent
adjacency. Coordinate-free legacy locations carry an explicit false spatial
sentinel. There is deliberately no transitive-closure table and no inferred
containment. Both the v0.7 `parent_id` spelling and accepted legacy `parent`
spelling produce the same direct hierarchy row. `spatial_location_link`
projects every authored directional location link in source order; its exact
JSON distinguishes plain IDs from detailed link objects without manufacturing
reciprocity or topology. `spatial_route` holds the authored directed route;
`spatial_route_edge` has one forward edge and a second edge only when the
authored route explicitly says `two-way`; route and portal modes are normalized
separately.

Anchors, portals, overlays, overlay memberships, audiences, and perspectives
retain their declared identifiers, coordinates, metrics, timelines, source
ordinals, and JSON definition. The model does not infer reverse portals,
proximity, map conversions, membership, duration, or StoryTime from those
facts. A portal target uses separate nullable location and map foreign keys,
with a database `CHECK` enforcing exactly one of the authored location-target
or map-position forms.

Finite integer ordinates and metrics must fit signed i64 before binding to
SQLite; the two boundary values are accepted and values immediately outside
them are refused before insertion. Spatial numeric columns use SQLite NUMERIC
affinity: signed-i64 source integers remain exact INTEGER values, including
values beyond binary64's exact range, while finite floating-point ordinates
remain REAL values. Exact authored numeric spelling and all portal/anchor
coordinates also remain in canonical definition JSON for source/projection
comparison.

The portable Btree indexes are the required candidate-read strategy. A caller
may install an SQLite RTree over already-projected location bounds when that
feature is available; its absence deterministically leaves the Btree path in
place. RTree lower bounds are rounded down and upper bounds up by SQLite, so it
is conservative candidate generation only; every candidate is post-filtered
against the exact NUMERIC base-table bounds. Indexes are advisory accelerators,
not new authored facts.

The SQLite schema and compiler fingerprint change whenever this projection
changes. Readiness verifies SQLite integrity, foreign keys, the required
spatial tables and indexes, and the portal-target invariant rather than trusting
revision metadata alone. A non-strict read atomically rebuilds an incompatible
cache; a strict compiled-only read fails closed. A cache-miss compile loads,
validates, and projects source once. Runtime registration, migration, public query semantics, HTTP, and UI are implemented separately; the earlier document described them as later task ownership during the original component stage.

## Optional acceleration and bounded explorer availability

Ordinary core `bbox` reads without an explorer candidate budget keep the required exact Btree path when RTree is absent. Budgeted explorer viewport reads require the map-scoped `spatial_location_rtree`; absence is an unavailable geometry outcome rather than an unbounded Btree fallback. Exact NUMERIC post-filtering remains mandatory in either candidate strategy. The installer creates location and temporal-overlay RTrees together and removes both on a partial creation or population failure. The temporal RTree preserves exact signed tick/order interval decomposition and is candidate-only; final exact horizon/lens filtering remains authoritative. These are current `src/wedl/spatial_index.py` and `src/wedl/spatial_query.py` boundaries.

# Component benchmark methodology and preserved requirements

The original component-stage qualification method benchmarks a validated v0.7 component by
calling `build_spatial_projection(world)` and then
`benchmark_spatial_index.measure(projection)`. Record source counts, bounded
insert-batch counts, deterministic projection equality across repeated builds,
and the `EXPLAIN QUERY PLAN` output for map-bounds, hierarchy-parent,
location-link target, route-adjacency, and overlay-candidate predicates. The
benchmark records the serialized database byte count and SHA-256, plus a
canonical all-spatial-row SHA-256. Repeated builds must produce identical byte,
row, and query-plan evidence; timing values are deliberately excluded from the
determinism comparison.

Exact-bound cases include `2^53-1`, `2^53`, `2^53+1`, both signed-i64
endpoints, and a large negative integer. They assert INTEGER storage, exact
Btree inclusion, source/projection parity, and RTree candidate inclusion when
available. A finite fractional coordinate separately asserts REAL storage.

Run the same checks with an SQLite build that lacks RTree (or where optional
RTree creation fails). The required Btree indexes must preserve candidate rows
and ordering. RTree is an acceleration only; it must not alter authored facts,
candidate semantics, or output ordering. Its outward-rounded candidates are
always post-filtered by the exact NUMERIC base-table bounds.

Cache conformance also drops one required spatial structure from an otherwise
readable current-revision database. Readiness must classify it as incompatible,
compiled-only access must fail, and ordinary access must replace it through the
temporary-file atomic rebuild path. Portal corruption and signed-i64 coordinate
boundaries are separate database/projection cases.

Do not report component timings as a public API claim. At the original component-only stage, generic loading, source migration, spatial query semantics, and pagination were assigned to separate tasks. That stage marker is historical; current runtime and public surfaces are described below.


This methodology requires the portable base query strategy to remain correct without RTree. It does not require the bounded explorer viewport to use a Btree fallback: that distinct current endpoint requires its map-scoped RTree and closes as unavailable if it is absent. The historical measured result is [the spatial index report](../../../../docs/reports/spatial-index-benchmark.md); retained query measurements are [the unchanged JSON](../../../../docs/reports/spatial-query-benchmark.json). No documentation migration is a new performance qualification.

## Current delivery and authority boundary

Homogeneous `wedl/v0.7` is accepted by the generic source loader, validator, and compiler. The current SQLite schema is `wedl-sqlite/v15` and the spatial compiler fingerprint is `wedl-spatial-index/v5`. Markdown/Git remains canonical and all spatial SQLite structures are disposable projections.

The eight-operation `wedl-spatial/v1` codec is implemented in `src/wedl/spatial_api.py`: containment, children, bbox, nearby, adjacency, reachability, path, and overlay-as-of. The separate `wedl-spatial-explorer/v1` adapter implements catalog, places, viewport, layers, and routes. Browser explorer/editor and ID-only `spatial.<kind>.create|update` authoring intents are also present. Those intents compile to ordinary confirmed changesets and require v0.7, exact expected HEAD, idempotency identity, closed payloads, and the established confirmation flow. They edit authored source; they do not authorize inferred topology, geometry, metrics, horizons, or identity.

An explorer author lens selects audience/perspective for presentation. It does not confer trusted character identity or confidentiality. Exact authored visibility and horizon filters continue to apply before counts and paging. The original spatial ADR A01M48NPY19KENPF8EWB8A4W95W retains its 2026-08-30 approval-stage wording and vectors; this migration does not silently rewrite that approval as a later deployment claim.

Executable component fixture: [spatial-source-component-v07.yaml](../../examples/spatial-source-component-v07.yaml). Its semantic cases and original stage comments are preserved unchanged; those comments do not state current generic runtime acceptance.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiODE2NDljMzM0ZWViNWVhY2E4N2U1MGM0YTE1ZWEwZDU2YzgzYmRmZSIsImkiOiJzaGEyNTY6dVNMTjRsdTRXT2xRdG1aRUh6c084dm45VzlndGtxVVhfODRNWEhXbnRsNCIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WlJaOEM3MERRME40TTNIUTUxTTRZIiwib3AiOiJPMDFNNDhaUlo4QzcwRFEwTjRNM0hRNTFNNFkiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1Njp4dWVjWjRpbWJ6VEF6c1owcDVONF9CZUV2WEk3ZHdBNUw2Mi13eXZyMXlrIiwidCI6MTc5MTMwMzEyMjE4OCwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
