# Performance

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

The local LSA cold fit is the visible cost. Warm forced compilation reuses the
content-addressed model/vector cache. The hybrid/vector databases contain 931
document links backed by 626 unique normalized vectors.

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
full benchmark process. A subsequent runtime optimization caches the immutable
model matrix and query vectors in-process; repeated exact-vector queries no
longer decode all BLOBs.

### Current optimization boundary

- State-only and FTS compilation are already small compared with source authoring
  operations.
- Cold LSA fitting dominates local vector compilation.
- Warm vector builds are bounded mainly by rebuilding the disposable SQLite
  projection and linking cached vectors.
- Exact authorized cosine scanning remains practical at roughly ten thousand
  documents; the medium query returns in tens of milliseconds.
- ANN should be considered only after a measured repository exceeds its latency
  budget, and must preserve filter-before-ranking semantics.

Raw profile results are retained in
[`SEARCH_PROFILE_BENCHMARKS.json`](SEARCH_PROFILE_BENCHMARKS.json) and
[`MEDIUM_SEARCH_PROFILE_BENCHMARKS.json`](MEDIUM_SEARCH_PROFILE_BENCHMARKS.json).

# SQLite compilation and query performance

wedl performs a safe full rebuild for changed revisions and exact reuse for an
unchanged revision. Git Markdown remains canonical; SQLite and both content
caches are disposable.

## Spatial-index benchmark boundary

The latent v0.7 spatial projection is measured separately before public query
work lands. Its acceptance envelope is bounded batch insertion, deterministic
map/location/route/overlay row ordering, Btree-backed map-bounds and adjacency
candidate reads, and optional-RTree fallback. Repeated builds also compare the
serialized database SHA-256, canonical row SHA-256, byte size, and representative
query plans. Large signed-integer bounds stay exact in the NUMERIC Btree model;
optional outward-rounded RTree bounds remain candidate-only and use an exact
base-table post-filter. Cache-miss compilation performs one source load/validation pass;
cache reuse includes structural and integrity checks before the fast path. It
does not claim a public
spatial-query latency until the query contract is implemented.

## Implemented performance work

### Source loading

- One `git ls-tree` plus batched `git cat-file --batch` reads.
- Parsed records cached by Git blob ID and parser fingerprint in
  `.wedl/source-cache.sqlite`.
- An unchanged exact revision can be recognized from commit/tree/compiler
  metadata before loading or parsing source.

### Semantic projection

- Entity kinds and canonical event times cached inside a loaded world.
- Effects indexed by target instead of rescanning every event per state query.
- Knowledge and relationships indexed by owning/source character.
- Character interactions materialized in one event pass rather than rescanning
  all events for every character pair.
- Bulk SQLite insertion followed by ordinary index creation.

### Search and embeddings

- Search documents built once per compile.
- Embedding cache lookups batched by content hash.
- Hash vectors generated once per unique input and reused across revisions.
- Cache-hit timestamps are not rewritten during every forced compile.
- Hybrid search bounds the bundled lexical feature-vector lane to FTS
  candidates; explicit vector mode still exact-scores every authorized vector.
- Detailed substage timing identifies document construction, hashing, cache
  lookup, vector generation, cache persistence, row preparation, FTS insertion,
  and embedding insertion separately.

### Runtime queries

- The current compiled `World` is cached inside one repository runtime by
  revision, tree, database mtime, and database size.
- Repeated API/context/search calls no longer reconstruct all records from
  SQLite on every request.

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
