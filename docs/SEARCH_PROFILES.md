# Search Profiles and Dense-Vector Retrieval

## 1. Why search is compiled by profile

Not every repository needs the same derived data. `wedl compile` therefore
selects one explicit profile and stores it in the revision metadata:

| Profile | Narrative state | FTS5 | Dense vectors | Intended use |
|---|:---:|:---:|:---:|---|
| `state` | ✓ |  |  | validation, temporal queries, minimal database |
| `fts` | ✓ | ✓ |  | fast exact/lexical authoring and constrained machines |
| `vector` | ✓ |  | ✓ | semantic retrieval without lexical index overhead |
| `hybrid` | ✓ | ✓ | ✓ | normal local authoring and LLM context retrieval |

A database compiled with `fts` rejects vector and hybrid searches. A database
compiled with `vector` rejects FTS and hybrid searches. The program does not
silently substitute a different lane.

The selected profile persists across ordinary commits and changeset-triggered
recompilation until the author explicitly selects another profile.

## 2. FTS5 lane

The FTS table has six independently weighted columns:

1. title — weight 10;
2. aliases — weight 6;
3. heading — weight 4;
4. prose — weight 1;
5. domain — weight 2;
6. tags — weight 2.

It uses the FTS5 Porter tokenizer layered over Unicode tokenization with
diacritic removal. Quoted input remains a phrase; unquoted content words are
combined as independent terms. Stop words are removed before constructing the
query expression.

Perspective, scene, and fictional-time predicates are materialized in a SQL
`allowed` CTE before `MATCH`, BM25, or snippet generation. A forbidden document
cannot influence visible ranks or snippets.

Author as-of search uses the same predicate. The compiler does not leave a
timeless aggregate chunk containing future events, turns, recollections,
knowledge claims, environment/observation facts, or transitions: each
time-bearing author projection carries a truthful boundary. Static entity
metadata remains timeless. `--all-time` is the explicit
author-only opt-in when that boundary is intentionally not wanted.

## 3. Vector providers

### Local LSA — default

`lsa` is dependency-local and deterministic. It builds:

- word unigram and bigram counts;
- TF-IDF with normalized sparse rows;
- deterministic randomized truncated SVD;
- dense `Float32` vectors;
- final L2 normalization.

The requested dimension is an upper bound. Very small or visibility-restricted
corpora may yield fewer dimensions because SVD rank cannot exceed the available
training documents and features. The actual dimension is stored per model.

The author model is trained over author-visible chunks. The character model is
trained **only over globally public chunks**, then authorized self material,
knowledge, observations, dialogue, and recollections are projected into that
fixed public basis. Private or author-only text therefore cannot change the
latent basis used by a character query.

### Sentence Transformers — optional

```bash
pip install 'wedl[sentence-transformers]'
wedl compile --profile hybrid \
  --vector-provider sentence-transformers \
  --vector-model sentence-transformers/all-MiniLM-L6-v2
```

The provider requests normalized NumPy embeddings from the configured model.
Model loading is process-cached. Model weights are not bundled in the wedl
archive.

### OpenAI-compatible endpoint — optional

```bash
export WEDL_EMBEDDING_ENDPOINT=http://127.0.0.1:8000/v1/embeddings
export WEDL_EMBEDDING_API_KEY=...
wedl compile --profile vector \
  --vector-provider openai-compatible \
  --vector-model text-embedding-3-small
```

The endpoint is opt-in. wedl never silently sends source text to a remote
service. Returned vectors are validated for consistent dimensions, finite
values, and normalized before storage.

### Feature hash — tests only

`hash-test` exists for deterministic test and diagnostic cases. It is explicitly
reported as a lexical feature hash and is never the default or described as a
semantic model.

## 4. Structured vector inputs

Each vector input combines only audience-safe fields:

```text
record title
aliases
section heading
domain
tags
chunk prose
```

The display snippet remains the original chunk prose. Structured fields improve
entity disambiguation and allow the vector lane to recognize aliases and typed
story vocabulary without polluting the rendered excerpt.

## 5. Normalized relational storage

Vectors are not repeated in every search-document row:

```text
vector_model(scope, model_id, provider, dimensions, normalized, model_blob, ...)
vector_embedding(vector_id, model_id, input_hash, dimensions, norm, vector)
document_vector(document_id, vector_id)
```

`vector_embedding` is unique by `(model_id, input_hash)`. The same normalized
vector may serve multiple public/private document projections or repeated
chunks. BLOBs are little-endian `Float32`; nonzero rows must have norm 1 within
validation tolerance.

The persistent disposable vector cache uses the same content identity. Warm
forced builds reuse unchanged model projections and normalized vectors rather
than recomputing them.

## 6. Exact vector retrieval

Vector-only search:

1. applies perspective, scene, and fictional-time authorization in SQL;
2. selects only the authorized vector IDs;
3. embeds and normalizes the query with the matching scope model;
4. scores every authorized unique vector by exact cosine similarity;
5. maps winning vector IDs back to source documents;
6. applies deterministic per-entity diversity limits.

The immutable model matrix is cached in-process and scored with one NumPy matrix
multiplication. The cache is keyed by database path, file identity, and model
ID, so atomic database replacement invalidates it automatically.

No FTS candidate list is used in vector mode.

## 7. Hybrid retrieval

Hybrid mode runs the two authorized lanes independently:

```text
allowed corpus -> top K FTS5/BM25
allowed corpus -> top K exact vector/cosine
              \-> weighted reciprocal-rank fusion of the union
```

Results include:

- `lanes`: `fts`, `vector`, or both;
- `ftsRank`, `ftsBm25`, and normalized FTS contribution;
- `vectorRank` and cosine similarity;
- final fused score.

The profile controls FTS/vector candidate limits, lane weights, and the RRF
constant. In the authored benchmark query `legal authenticity coercion`, FTS
returned four direct lexical results while vector retrieval returned twenty;
hybrid retained sixteen vector-only results as well as documents supported by
both lanes.

## 8. Choosing a profile

- Use `state` for CI validation or headless simulations that never search.
- Use `fts` when startup, disk, and predictable lexical matching matter most.
- Use `vector` when semantic recall matters but FTS snippets/phrases do not.
- Use `hybrid` for authoring and LLM context, where precise names and phrases
  should coexist with latent-semantic recall.

For a large repository, begin with exact vector search and measure it. Add ANN
only after the authorized exact scan exceeds an accepted latency budget; ANN
must remain behind the same filter-before-ranking contract.
