# Historical spatial index benchmark

Migrated from `docs/SPATIAL_INDEX_BENCHMARK.md` at source Git revision `aac36745258e6f93487481564dc1c40daeb327ef`. The original measurement record did not name its measurement date or input Git revision; none is inferred here. These values preserve the original measured component result and do not certify current authored-source, API, or browser performance.

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

## Reading and reproducing the evidence

The [spatial query JSON](spatial-query-benchmark.json) retains its original synthetic-projection counts, outcomes, medians and digests unchanged. Index methodology and architectural requirements are in ADRAI A01M48ZRZ2FB3S5BWBCBK9KS13N; compiled query semantics and budgets are in A01M48ZSKSZ3EKW7EZX5HDTSRET. Read them in the WEDL development/source checkout using `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json`.

For a separate contemporary synthetic query run in that checkout, `uv run python tools/benchmark_spatial_query.py` prints fresh JSON to stdout. The index helper exposes `benchmark_spatial_index.smoke()` and `full()` for its synthetic envelopes, and `measure(projection)` for a supplied projection. Record the actual input/environment for any new result and use a fresh caller-owned output destination with a defined retention lifetime. Preserve this report and its companion JSON as historical evidence; this documentation cleanup runs no benchmark qualification.
