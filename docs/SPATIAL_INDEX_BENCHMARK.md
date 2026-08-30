# Spatial-index benchmark method

Before spatial reads become public, benchmark a validated v0.7 component by
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

Do not report those component timings as a public API claim. Generic loading,
source migration, spatial query semantics, and pagination are intentionally
deferred to their separately reviewed tasks.

## Reference full-envelope run

The validation environment ran the deterministic full fixture with 100,000
locations (a 128-deep chain and 10,000 direct siblings), 32 maps, 250,000
one-way routes, 100 portals, and 10,000 overlays. One in-memory disposable
build inserted 1,094 bounded batches (maximum 1,000 rows) in 3,155.965 ms;
the optional RTree was available. It serialized 265,801,728 bytes with SHA-256
`86b819bbf49bbccbc29d8f7b23cf577e50b6ee644c40772ab380c558406048b2`,
and produced canonical row SHA-256
`73d9449f3486035f60cdead1218892e15141ea411820d1a29c2e7135abdf537d`.
All five representative plans selected their required Btree indexes. The
portable Btree fallback is covered separately and retains the same ordered
candidate semantics. This is a component compilation baseline, not a public
spatial-query service target.
