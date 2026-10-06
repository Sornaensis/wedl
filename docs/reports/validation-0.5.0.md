# Validation report — wedl 0.5.0

> Historical report. Original versions, fixture counts, outcomes and measurements below belong to the recorded exercise. This migration performs no fresh release/performance qualification. Architectural rationale is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; custody is in ADRAI A01M49735W4D3CZ2PJ129HTJCGG. Read these with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in the WEDL development/source checkout.

## Causal and continuity validation (source schema `wedl/v0.3`)

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


## Release result

The 0.5.0 search/profile iteration passes its release gate.

- Source archive: populated and ZIP-integrity tested.
- Wheel: valid `py3-none-any`, installed with `pip --no-deps` into an empty
  target directory.
- Exact source-ZIP extraction: 35 tests passed.
- Fresh source and wheel repositories: 238 records, zero validation diagnostics.
- Fresh source and wheel hybrid databases: 6,299,648 bytes each.
- Search projection: 931 documents, 931 FTS rows, 931 vector links, 626 unique
  normalized vectors.

## Automated regression suite

```text
35 tests passed
```

Coverage includes:

- `state`, `fts`, `vector`, and `hybrid` profile behavior;
- rejection of unavailable search lanes rather than silent fallback;
- weighted FTS aliases, headings, prose, domain, and tags;
- quoted phrases and Porter stemming;
- local TF-IDF/truncated-SVD latent-semantic retrieval;
- optional OpenAI-compatible provider normalization through a deterministic fake
  endpoint;
- finite-value, dimensionality, and L2-normalization checks;
- content-hash vector deduplication and document links;
- author/character vector-scope separation;
- independent FTS and vector candidate lanes;
- hybrid union, lane evidence, and vector-only results;
- secret-marker character search exclusion;
- profile persistence after changeset-triggered recompilation;
- source parsing, temporal state, conversation provenance, context budgets,
  author as-of search/conversation future-leak prevention and all-time opt-in,
  atomic Git changesets, idempotency, API authorization, and second-act
  continuity inherited from 0.4.

## Fresh profile smoke test

| Profile | Search documents | FTS rows | Unique vectors | Vector links | Bad nonzero norms |
|---|---:|---:|---:|---:|---:|
| `state` | 0 | 0 | 0 | 0 | 0 |
| `fts` | 931 | 931 | 0 | 0 | 0 |
| `vector` | 931 | 0 | 626 | 931 | 0 |
| `hybrid` | 931 | 931 | 626 | 931 | 0 |

Assertions made against the fresh fixture:

- quoted alias `Chief Sorn` retrieves **Ilyra Sorn** from the alias column;
- tag phrase `hydraulic key` retrieves **Pressure Gate Seven Key**;
- `legal authenticity coercion` retrieves **The Notary at the Seventh Drawer**
  near the top of vector results;
- hybrid contains documents supported by both lanes and vector-only documents
  absent from FTS results;
- a character-scoped query for `ASH-SECRET-LETTER-CONTENTS-7F3Q` returns zero
  results;
- every nonzero stored vector has norm 1 within `0.001`.

The complete result is in `SEARCH_PROFILE_SMOKE.json` and the external release
artifact `wedl-python-0.5.0-smoke.json`.

## Stress validation

- Generated small world: 595 records, zero diagnostics.
- Generated medium world: 4,303 records, zero diagnostics.
- Medium search projection: 10,289 documents and 9,369 unique vectors.

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

## Profile benchmarks

Measurements include Python, Git, SQLite, NumPy, SciPy, and scikit-learn
overhead in the validation container.

### Authored Ash Archive — 238 records

| Profile | Cold forced | Warm forced | Exact reuse | Database |
|---|---:|---:|---:|---:|
| `state` | 53.7 ms | 44.7 ms | 5.6 ms | 1.42 MB |
| `fts` | 77.1 ms | 68.6 ms | 5.2 ms | 2.59 MB |
| `vector` | 756.3 ms | 95.1 ms | 5.9 ms | 5.89 MB |
| `hybrid` | 551.6 ms | 102.6 ms | 5.9 ms | 6.30 MB |

Median query latency:

| Lane | Median | p95 | Results |
|---|---:|---:|---:|
| FTS | 3.2 ms | 11.4 ms | 4 |
| Vector | 5.3 ms | 10.5 ms | 20 |
| Hybrid | 5.9 ms | 6.4 ms | 20 |

For `legal authenticity coercion`, hybrid returned three results supported by
both lanes and sixteen vector-only results.

### Generated medium world — 4,303 records

| Profile | Cold forced | Warm forced | Exact reuse | Database |
|---|---:|---:|---:|---:|
| `state` | 432 ms | 388 ms | 8.7 ms | 13.8 MB |
| `fts` | 681 ms | 802 ms | 5.4 ms | 23.0 MB |
| `vector` | 2.95 s | 1.04 s | 7.8 ms | 42.5 MB |
| `hybrid` | 3.90 s | 949 ms | 8.4 ms | 45.2 MB |

For `consistency checksum neighboring records`, median full-process queries were
approximately 11 ms FTS, 38 ms vector, and 38 ms hybrid. Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

Raw benchmark files:

- `SEARCH_PROFILE_BENCHMARKS.json`
- `MEDIUM_SEARCH_PROFILE_BENCHMARKS.json`

## Search-quality probes

- `counterseal fragment`: all lanes rank **Archive Counterseal Fragment** first.
- `legal authenticity coercion`: vector retrieval groups the notary scene,
  Ysabet, the seventh drawer, and Council writ despite sparse lexical overlap.
- `subterranean water pressure records`: vector retrieval groups pressure-gate,
  flood-pressure, and underwater-letter material; hybrid preserves exact clues.
- `custody proof route`: both lanes retrieve the distributed-records proposal
  and related custody story points.

## Static and packaging checks

- Python compilation: passed.
- JavaScript syntax: passed.
- Relative Markdown links: passed.
- `git diff --check`: passed.
- Source ZIP integrity: passed.
- Wheel ZIP integrity: passed.
- Wheel metadata and console entry points: accepted by pip.
- Installed wheel reports version `0.5.0`.
- Source and wheel include all 238 Ash Archive Markdown records.
- Source migration is local-only and preview-confirmed; compiled SQLite remains
  disposable and is rebuilt rather than migrated in place.

## Migration/recovery release evidence

The migration release gate exercises the v0.3 upgrade and the deliberately
narrow raw-envelope v0.4 recovery path through their confirmed forward commits,
then reopens/recompiles the resulting v0.5 source. It also verifies the forward
rollback boundary: an ordinary backup is restored as source and recompiled,
while a recovered v0.4 backup is restored byte-for-byte only by explicit
rollback and remains unavailable to ordinary load, validation, compile, and
query paths.

On Windows, the integration fixture defers the changeset and rollback apply
methods' immediate cache publication after a reproducible `os.replace` lock on
the just-read SQLite file. It does not stub migration conversion or release
cache verification: each path is followed by a fresh `Repository` and a real
hybrid rebuild before its public catalog, unfiltered and selected-thread search,
and membership assertions. The release integration checks the authored catalog
label and selected-thread search behavior after the grouping changeset; its
selected results contain only the declared member while the unfiltered search
remains available. Unfiltered search rows remain equal to the pre-grouping
result.
For the archive semantic comparison, the migrated source snapshot is copied to
a fresh non-Git `Repository` before that real rebuild; this avoids Windows
temporary Git/SQLite handle contention while preserving the exact authored
source bytes under test.

The archive comparison asserts only observable data-plane evidence: public
search rows/ranking, causal edges, entity state, search-document chunk hashes,
vector payloads, vector model identity/configuration, and document-vector
links. It does not claim an unavailable general-purpose state or corpus hash.

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

No standalone migration stress-generator or machine-neutral latency budget is
available in this repository. Consequently this release records bounded query
count/plan behavior rather than fabricated conversion, cache-size, latency, or
memory figures. Reproduce environment-specific timings with the existing
`tests/test_performance_cache.py` cache checks and the migration and membership
test suites on the target machine.
