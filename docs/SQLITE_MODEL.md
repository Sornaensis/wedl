# SQLite Read Model — Python 0.5.0

## 1. Contract

Markdown committed to Git is canonical. `.wedl/world.sqlite` is a disposable,
revision-stamped read model. Deleting `.wedl/` must never lose authored
information; a clean compile reconstructs the database from the selected Git
revision.

A candidate database is built with fast disposable-file pragmas, checked with
`PRAGMA integrity_check`, and atomically replaces the current database only
when complete.

## 2. Revision and compilation profile

The single `revision` row records:

```text
head_commit
tree_oid
source_schema
sqlite_schema
compiler_version
compiler_fingerprint
search_profile
profile_json
vector_models_json
record_count
compiled_at
build_mode
```

`profile_json` is the complete operational search profile. Exact-revision reuse
is allowed only when source, schema, compiler, tree, and profile fingerprints
match.

The four profiles are:

- `state`: narrative and temporal projections only;
- `fts`: state plus FTS5;
- `vector`: state plus dense-vector models and links;
- `hybrid`: state plus both search lanes.

## 3. Source and narrative projections

The read model contains normalized tables for:

- entities, tags, aliases, and references;
- events, participants, and state effects;
- knowledge claims and epistemic transitions;
- directional relationships and transitions;
- scenes, presence intervals, and observations;
- conversations, verbatim turns, and subjective recollections;
- story points and lifecycle transitions;
- materialized current state, knowledge, relationships, story-point state, and
  character interactions.

These tables are implementation details behind the Python query API. Temporal
resolution remains defined by the source model and deterministic `(timeline,
tick, order, stable ID)` ordering.

## Spatial component projection (latent v0.7)

The v0.7 spatial component reserves disposable normalized tables for maps,
locations and direct hierarchy adjacency, authored route edges, anchors,
portals, overlays, overlay membership, and every authored directional legacy
location link. Link rows preserve source order and exact JSON, so plain and
detailed links remain distinguishable without inferring reverse edges.
Portal location targets and map-position targets use separate foreign keys and
an exclusive database check. It is not a generic source-schema
registration: the migration task must explicitly enable runtime compilation.
The portable base uses Btree candidate indexes; an optional RTree can accelerate
authored location bounds when supported by the local SQLite build. Spatial rows
retain entity foreign keys; direct parent references are deferred so canonical
source-path ordering never changes their authored meaning.
Integer spatial values are checked against SQLite's signed-i64 binding range
before insertion and NUMERIC-affinity columns retain them as exact INTEGER
values. Valid floating-point values remain REAL. Required Btree predicates run
against those exact bounds. Optional RTree bounds round conservatively outward
and are candidate-only; exact NUMERIC post-filtering is mandatory. Cache
readiness verifies required spatial tables/indexes, integrity, foreign keys,
and portal target shape before accepting revision metadata; an incompatible
cache is rebuilt by atomic replacement.

## 4. Search documents

Search is built from bounded, audience-labelled chunks:

```text
search_document(
  document_id UNIQUE,
  entity_id,
  document_kind,
  heading,
  audience_kind,
  audience_character_id,
  scene_id,
  timeline,
  from_tick,
  from_order,
  until_tick,
  until_order,
  text,
  metadata_json,
  chunk_hash
)
```

The compiler emits separate author, public, self/private, knowledge,
observation, conversation-turn, recollection, and transition projections.
Author prose that carries an event, environment, observation, conversation,
recollection, knowledge claim, or transition is stored with a story-time
boundary; static entity metadata remains timeless. Conversation and scene body
summaries are aggregate prose: closed summaries begin at their truthful end,
while live aggregates are omitted in favor of their individually bounded turns
and observations. Both author and character queries construct an allowed SQL
set using perspective and fictional time before either search lane ranks
candidates.

## 5. FTS5

The FTS table uses six columns:

```text
search_fts(title, aliases, heading, text, domain, tags)
```

It uses Porter stemming over Unicode tokenization with diacritic removal. BM25
weights are `10, 6, 4, 1, 2, 2` respectively. Quoted input is preserved as a
phrase; stop words are removed from unquoted terms. Snippets are generated only
from the pre-authorized row set.

## 6. Dense-vector models

Vector storage is relationally normalized:

```text
vector_model(
  scope PRIMARY KEY,
  model_id,
  provider,
  model_name,
  dimensions,
  normalized,
  corpus_hash,
  config_json,
  model_blob
)

vector_embedding(
  vector_id PRIMARY KEY,
  model_id,
  input_hash,
  dimensions,
  norm,
  vector,
  UNIQUE(model_id, input_hash)
)

document_vector(document_id PRIMARY KEY, vector_id)
```

The BLOB is little-endian `Float32`. Nonzero vectors must have L2 norm 1 within
tolerance. One content vector can serve multiple search documents; in the
current authored fixture, 931 document links reference 626 unique vectors.

The default local model is word unigram/bigram TF-IDF projected through
truncated SVD. Model terms, IDF values, and projection components are stored in
`vector_model.model_blob` so queries use the exact compiled model. Optional
sentence-transformers and OpenAI-compatible providers store provider identity
and dimensions through the same interface.

The author and character scopes are separate. Corpus-trained character LSA is
fit only on globally public chunks and then used to project authorized private
material. This prevents hidden author text from changing a character’s latent
basis.

## 7. Exact vector retrieval

Exact vector search:

1. applies authorization and time predicates to document/vector links;
2. loads the matching scope model;
3. embeds and normalizes the query;
4. scores every authorized unique vector by cosine similarity;
5. loads full document metadata only for winning vector IDs;
6. applies deterministic diversity limits.

The immutable normalized matrix is cached in-process by database identity and
model ID and scored with NumPy. Atomic database replacement changes the cache
identity and invalidates the matrix automatically.

No FTS candidate set is involved in vector mode.

## 8. Hybrid fusion

Hybrid search independently retrieves top candidates from FTS5 and vector
search, takes the union, and applies weighted reciprocal-rank fusion. The result
retains lane membership and component ranks/scores for debugging.

This is intentionally different from vector reranking of FTS hits: semantic
results with no lexical match remain eligible for the final response.

## 9. Persistent caches

Two disposable caches reduce repeated work:

- `source-cache.sqlite`: parsed source records keyed by parser fingerprint and
  Git blob ID;
- `vector-cache-v2.sqlite`: vector models and normalized vector values keyed by
  profile/model identity and content hash.

Corpus-trained LSA model cache keys include the safe training corpus hash.
Pretrained-provider cache keys omit corpus identity and reuse unchanged content
across revisions and scopes.

## 10. Incremental boundary

Exact-revision reuse is immediate and fast-forward changes reuse parsed records,
vector models, and unchanged vectors. The semantic database itself is still
rebuilt atomically for a changed revision. Row-level incremental mutation is a
future optimization and must remain semantically equivalent to a clean build.

See [`SEARCH_PROFILES.md`](SEARCH_PROFILES.md) for provider and ranking details
and [`PERFORMANCE.md`](PERFORMANCE.md) for measured profile costs.
