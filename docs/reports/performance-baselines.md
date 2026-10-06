# Performance

> Historical report. Original versions, fixture counts, outcomes and measurements below belong to the recorded exercise. This migration performs no fresh release/performance qualification. Architectural rationale is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; custody is in ADRAI A01M49735W4D3CZ2PJ129HTJCGG. Read these with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in the WEDL development/source checkout.

## 0.5.0 search-profile measurements

wedl now measures compilation and retrieval by explicit profile rather than
reporting one blended build. The figures below were collected in the validation
container and include Python, Git, SQLite, NumPy, SciPy, and scikit-learn
runtime overhead. They are regression baselines, not universal hardware claims.

### Authored Ash Archive — 238 records, 931 search documents

| Profile | Cold forced compile | Warm forced compile | Exact revision reuse | Database size |
|---|---:|---:|---:|---:|
| `state` | 53.7 ms | 44.7 ms | 5.6 ms | 1.42 MB |
| `fts` | 77.1 ms | 68.6 ms | 5.2 ms | 2.59 MB |
| `vector` | 756.3 ms | 95.1 ms | 5.9 ms | 5.89 MB |
| `hybrid` | 551.6 ms | 102.6 ms | 5.9 ms | 6.30 MB |

The reported local LSA cold fit was the visible cost. The measured hybrid/vector databases contained 931 document links backed by 626 unique normalized vectors. Cache-design explanation is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.

For `legal authenticity coercion`, median in-process searches were:

| Lane | Median | p95 | Results |
|---|---:|---:|---:|
| FTS | 3.2 ms | 11.4 ms | 4 |
| Vector | 5.3 ms | 10.5 ms | 20 |
| Hybrid | 5.9 ms | 6.4 ms | 20 |

Hybrid returned three documents supported by both lanes and sixteen vector-only
documents. This demonstrates that the vector lane is not merely reranking an
FTS candidate set.

### Generated medium world — 4,303 records, 10,289 search documents

| Profile | Cold forced compile | Warm forced compile | Exact revision reuse | Database size |
|---|---:|---:|---:|---:|
| `state` | 432 ms | 388 ms | 8.7 ms | 13.8 MB |
| `fts` | 681 ms | 802 ms | 5.4 ms | 23.0 MB |
| `vector` | 2.95 s | 1.04 s | 7.8 ms | 42.5 MB |
| `hybrid` | 3.90 s | 949 ms | 8.4 ms | 45.2 MB |

The vector/hybrid databases contain 10,289 document links backed by 9,369
unique vectors. For `consistency checksum neighboring records`, medians were
approximately 11 ms for FTS, 38 ms for vector, and 38 ms for hybrid in the
full benchmark process. Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

### Historical optimization interpretation

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

# SQLite compilation and query performance

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

## Spatial-index benchmark boundary

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


## Latent compiled spatial-query benchmark

This was a synthetic compiled-projection measurement at an earlier implementation stage. It did not qualify authored source, CLI, HTTP or UI performance. Current spatial support is described by the source/query contracts, independently of these old timings. On the deterministic full
envelope of 100,000 locations (including depth 128), 32 maps, 250,000 routes,
100 portals, and 10,000 overlays, three median in-memory reads measured 0.262 ms
indexed containment, 0.052 ms bounds, 0.057 ms same-map nearby, 0.080 ms
overlay-as-of, and 1.320 ms weighted path. All five results were repeat-stable
with evidence digest
`c426b9e7e050302a1c97a96c3728b866972a3cd267af45042b4662292df9029c`.
Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout. Full counts, result digests, and timings are in
[`spatial-query-benchmark.json`](spatial-query-benchmark.json), produced by
`tools/benchmark_spatial_query.py`.

## Implemented performance work

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


## Benchmark worlds

| Profile | Records | Purpose |
|---|---:|---|
| Authored Ash Archive | 238 | Narrative quality, perspective, continuity, changesets |
| Generated small | 594 | Fast local stress regression |
| Generated medium | 4,302 | Main scaling regression |
| Generated large | 10,662 | Find nonlinear compile/query behavior |

The generated worlds are separate from the authored example so synthetic
volume does not dilute narrative quality.

## Compile results

Measurements below were produced in this container with Python and Git overhead
included. They are regression baselines, not portable guarantees.

| World | Cold forced | Warm forced | Exact revision reuse |
|---|---:|---:|---:|
| 238 authored records | 106 ms | 72 ms | 2.9 ms |
| 4,302 generated records | 1.09 s | 719 ms | 3.8 ms |
| 10,662 generated records | 2.89 s | 1.95 s | 3.5 ms |

### Large-world cold phases

| Phase | Milliseconds |
|---|---:|
| Resolve revision | 3.2 |
| Load source | 3.1 |
| Whole-world validation | 186.2 |
| Insert entities | 350.4 |
| Insert narrative projections | 132.1 |
| Derive current state | 235.4 |
| Build search projection | 1,686.8 |
| Build ordinary indexes | 101.4 |

### Large-world cold search subphases

| Subphase | Milliseconds |
|---|---:|
| Build perspective-scoped documents | 303.3 |
| Hash inputs | 14.0 |
| Embedding-cache lookup | 26.7 |
| Generate missing feature vectors | 698.6 |
| Persist new cache entries | 273.1 |
| Prepare insertion rows | 97.4 |
| Insert search documents | 80.1 |
| Insert FTS5 rows | 85.0 |
| Insert embedding rows | 80.4 |

Search projection is now the dominant cold-build cost. That is useful evidence
for the next optimization boundary: semantic state and source parsing are no
longer the primary bottlenecks.

## Query results

Median in-process operation latency:

| Operation | Authored 238 | Medium 4,302 | Large 10,662 |
|---|---:|---:|---:|
| Validate loaded world | 8.0 ms | 73.9 ms | 200.1 ms |
| Author hybrid search | 5.6 ms | 3.7 ms | 4.7 ms |
| Character hybrid search | 5.4 ms | 4.1 ms | 5.1 ms |
| Character context | 24.1 ms | 10.9 ms | 14.9 ms |
| Author context | 5.7 ms | 3.7 ms | 3.8 ms |
| Conversation author view | 2.7 ms | 2.5 ms | 3.0 ms |
| Story-point evaluation | 4.2 ms | 2.6 ms | 2.6 ms |

The authored context is slower than stress context because it has more varied
knowledge, relationships, prose, and retrieval candidates per active
character—the stress corpus is large but structurally regular.

## HTTP exercise

Twenty-five repeated calls per read endpoint against the authored fixture gave:

| Endpoint workflow | Median | p95 |
|---|---:|---:|
| Status | 6.1 ms | 19.0 ms |
| Character list | 4.1 ms | 10.5 ms |
| Author search | 6.6 ms | 12.4 ms |
| Mara perspective search | 7.5 ms | 22.6 ms |
| Mara 5,000-character context | 26.5 ms | 33.8 ms |
| Ansel 5,000-character context | 8.9 ms | 16.3 ms |
| Mara conversation view | 4.3 ms | 12.1 ms |
| Ansel late-entry conversation view | 3.8 ms | 6.6 ms |
| Story points | 5.6 ms | 39.0 ms |

A complete guarded changeset apply, including Git commit and recompilation, took
about 172 ms. Idempotent replay returned in about 1.8 ms.

Full raw measurements are retained in [`BENCHMARKS.json`](BENCHMARKS.json).
