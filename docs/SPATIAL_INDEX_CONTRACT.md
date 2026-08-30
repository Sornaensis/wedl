# Spatial SQLite projection contract

The `wedl/v0.7` spatial component is not yet accepted by the generic loader or
compiler. `wedl.spatial_index` is an opt-in, disposable projection for
component validation and future read-model work; Markdown remains canonical.

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
validates, and projects source once. Runtime registration, migration, public
query semantics, HTTP, and UI remain owned by later tasks.
