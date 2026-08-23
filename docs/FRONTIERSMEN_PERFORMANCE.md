# The Frontiersmen — Performance and Retrieval Exercise

## Test world

The measurements below use the final authored Frontiersmen fixture:

- 294 records
- 17 conversations
- 224 verbatim turns
- 76 recollections
- 294 Markdown source files
- active scene at `main 195:0`

Values were measured in the validation container and include Python, Git, YAML, SQLite, NumPy, SciPy, and scikit-learn overhead. They are regression baselines rather than universal hardware claims.

## Compilation profiles

| Profile | Cold forced compile | Warm forced compile | Exact reuse | Database size |
|---|---:|---:|---:|---:|
| `state` | 234 ms | 85 ms | 3.1 ms | 2.45 MB |
| `fts` | 295 ms | 219 ms | 9.6 ms | 5.26 MB |
| `vector` | 1.67 s | 277 ms | 15.8 ms | 9.95 MB |
| `hybrid` | 1.91 s | 504 ms | 85.6 ms | 10.83 MB |

The vector and hybrid cold builds include local TF-IDF/truncated-SVD fitting. Warm forced builds reuse the content-addressed model/vector cache. `state` remains suitable when retrieval is unnecessary; `fts` is the inexpensive full-text authoring profile; `hybrid` is the packaged default.

The hybrid exact-reuse measurement is noticeably higher than the other profiles in this particular run. It remains far below a forced rebuild, but the difference is worth profiling further: exact reuse should ideally avoid any work beyond revision/profile verification and opening the existing database.

## Independent search lanes

Twenty repeated in-process calls were made for each mode and query.

| Probe | FTS median | Vector median | Hybrid median |
|---|---:|---:|---:|
| `blood quickens amber` | 7.2 ms | 6.2 ms | 9.5 ms |
| `memory of wounded animals taking shape through stone` | 4.7 ms | 5.8 ms | 6.9 ms |
| `former guild hunter using terror to strengthen masked followers` | 5.2 ms | 5.1 ms | 7.9 ms |
| `bone key reliquary rain pursuit through spruce` | 4.1 ms | 5.0 ms | 6.8 ms |

These calls use genuinely independent corpora:

- FTS uses weighted BM25 over title, aliases, heading, body, domain, and tags.
- Vector scans the complete authorized normalized-vector corpus.
- Hybrid retrieves from each lane independently and fuses the ranked union.

The raw top results, ranks, cosine scores, and fused lane evidence are retained in [`FRONTIERSMEN_BENCHMARKS.json`](FRONTIERSMEN_BENCHMARKS.json).

## Context and semantic reads

| Operation | Median | p95 |
|---|---:|---:|
| Rhea 5,000-character hybrid context | 27.1 ms | 40.3 ms |
| Rootjaw pursuit conversation view | 2.3 ms | 2.9 ms |
| Active story-point evaluation | 2.2 ms | — |

The Rhea packet serialized to 4,937 characters under the 5,000-character limit.

## HTTP exercise

Twenty-five same-process TestClient calls were made for each read workflow after a fresh hybrid compile.

| Endpoint/workflow | Median | p95 | Response size |
|---|---:|---:|---:|
| Status | 5.1 ms | 6.1 ms | 2.3 KB |
| Character list | 3.6 ms | 4.5 ms | 4.3 KB |
| Author FTS search | 7.7 ms | 14.5 ms | 18.2 KB |
| Author vector search | 8.0 ms | 20.1 ms | 19.0 KB |
| Rhea hybrid search | 11.0 ms | 26.0 ms | 19.3 KB |
| Rhea 5,000-character context | 30.2 ms | 49.0 ms | 5.0 KB |
| Rootjaw conversation view | 3.4 ms | 5.1 ms | 3.2 KB |
| Story-point evaluation | 3.5 ms | 7.8 ms | 5.0 KB |

Write authorization behaved as expected:

- unauthenticated compile: HTTP 401;
- authenticated compile: HTTP 200.

Raw HTTP results are in [`FRONTIERSMEN_API_BENCHMARKS.json`](FRONTIERSMEN_API_BENCHMARKS.json).

## Authoring-performance findings

### Act-sized changesets are the right granularity

A campaign-sized changeset is possible but difficult to inspect. Six act commits made preview diagnostics, temporal reasoning, and idempotent replay manageable. The builder now supports `--start` and `--through` for resumable authoring.

### Search payloads are larger than context packets

A 20-result search response can approach 19 KB because it preserves lane evidence and citations for inspection. The final LLM packet is around 5 KB. This is appropriate for separate inspection and generation APIs, but the browser should not automatically inject raw search responses into model context.

### Local LSA fitting remains the cold-cost center

On this authored world, cold vector/hybrid compilation is dominated by local model fitting. Warm forced builds are much faster due to model and vector reuse. An external neural provider would shift the cost toward network/model latency and should be benchmarked separately.

### Exact hybrid reuse merits another pass

The 85.6 ms exact hybrid reuse is still interactive but unexpectedly above the 3–16 ms measured for the other profiles. The next performance pass should instrument exact-profile verification and retained-database opening separately.
