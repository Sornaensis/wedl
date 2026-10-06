# The Frontiersmen — Performance and Retrieval Exercise

> Historical report. Original versions, fixture counts, outcomes and measurements below belong to the recorded exercise. This migration performs no fresh release/performance qualification. Architectural rationale is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; custody is in ADRAI A01M49735W4D3CZ2PJ129HTJCGG. Read these with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in the WEDL development/source checkout.

The 294-record/17-conversation/224-turn pursuit snapshot at main195 is historical. Maintained legacy/v0.7 packages have 309 records, 19 conversations and 248 turns with Southward Cut active at main210:0; those current facts do not replace old timing inputs. No rebuild or replay of the old snapshot was performed.

## Test world

The measurements below use the historical authored Frontiersmen fixture:

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

The exercise attributed vector/hybrid cold cost to local TF-IDF/truncated-SVD fitting. The original cache/profile design interpretation is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.

The hybrid exact-reuse measurement is noticeably higher than the other profiles in this particular run. It remained far below a forced rebuild. The proposed further profiling is historical rationale in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.

## Independent search lanes

Twenty repeated in-process calls were made for each mode and query.

| Probe | FTS median | Vector median | Hybrid median |
|---|---:|---:|---:|
| `blood quickens amber` | 7.2 ms | 6.2 ms | 9.5 ms |
| `memory of wounded animals taking shape through stone` | 4.7 ms | 5.8 ms | 6.9 ms |
| `former guild hunter using terror to strengthen masked followers` | 5.2 ms | 5.1 ms | 7.9 ms |
| `bone key reliquary rain pursuit through spruce` | 4.1 ms | 5.0 ms | 6.8 ms |

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

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

## Observations retained from the authoring exercise

The exercise used six act commits and reported builder `--start`/`--through` support. A 20-result search response approached 19 KB while the final model packet was around 5 KB. Cold fitting remained the reported cost centre, and exact hybrid reuse measured 85.6 ms versus roughly 3–16 ms for other profiles. The original granularity, browser-context and future-provider recommendations are preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.
