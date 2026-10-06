+++
schema = "adrai/decision/v1"
adr = "A01M48S1T65PSN638XCB59539Q8"
record = "R01M4920B6EEK0AE1R3MDGA7YG3"
title = "Disposable SQLite projections and compilation fingerprints"
summary = "Migrated SQLITE_MODEL contract and provenance; architecture is maintained through ADRAI."
domains = ["storage"]
+++

## Authority and provenance

Migrated from `docs/SQLITE_MODEL.md` at Git revision `996f5d18b4d982fd67c777ff833c667d80e83e29`. This relocation records existing documentation; it does not create a new historical approval. Named approvals and separately owned domain decisions retain their authority.



# Disposable SQLite read model

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

## Spatial component projection (v0.7)

The v0.7 spatial component reserves disposable normalized tables for maps,
locations and direct hierarchy adjacency, authored route edges, anchors,
portals, overlays, overlay membership, and every authored directional legacy
location link. Link rows preserve source order and exact JSON, so plain and
detailed links remain distinguishable without inferring reverse edges.
Portal location targets and map-position targets use separate foreign keys and
an exclusive database check. The generic v0.7 runtime writes this disposable
projection only after complete candidate validation.
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

## Generational component projection (v0.7)

The v0.7 generational component stores one row per authored record in
`generational_record`, typed endpoint rows for each of the eight kinds, ordered
`generational_union_participant` rows, and exact authored transitions in
`generational_transition`. Btree indexes support child-to-parent, organization
parent-to-child reverse traversal, containment, legacy-to-tenure/claim,
character-to-vital, and per-record as-of
replay. `generational_current` is a disposable fold at the world cursor.
The two covering parentage endpoint/time indexes let ancestry, descendants,
and relative paths merge each breadth-first level in authored order and stop
work at the item bound.

`generational_candidate` holds private structural search material with timeline,
applicability, audience, perspective, and citation fields.
`generational_search_prefix` stores distinct per-record normalized structural
token prefixes with first authorized audience/perspective/timeline applicability;
`generational_search_lookup_idx` supports bounded prefix discovery without
scanning unrelated transitions or the generic search corpus. Withheld vital
records and nonaffirmative vital initialization are excluded. Generational source
records are excluded from generic `search_document`, FTS5, and vector models;
the viewer-aware query layer filters private candidates before
ranking. See `adrai --repo WEDL_SOURCE_CHECKOUT show A01M491Y1RVN98VZDF318XJ1ARW --json` for
ordering, replay, citations, and the internal query boundary.

The disposable schema is `wedl-sqlite/v15`; the generational generation token is
`wedl-generational-index/v9`. These are the actual constants at the input revision, not the earlier v11/v4 documentation or the planning-time v8 expectation. The complete compiler prefix also contains `wedl-document-generation/v5`, `wedl-chronology-index/v3`, and `wedl-spatial-index/v5`. The parent-led
`generational_parentage_parent_idx(parent_id,child_id,id)` is required for
reverse descendant reads. Readiness checks require the
generational tables and indexes, their exact column/foreign-key/index shapes,
as well as the spatial shape, SQLite integrity,
and foreign-key integrity. A stale or damaged cache is rebuilt atomically.

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
historical Python 0.5 fixture, 931 document links referenced 626 unique vectors; these counts are a dated example rather than a present-day corpus claim.

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

See `SEARCH_PROFILES.md` (read with `adrai --repo . show A01M48RW32X6AEHK2376V4YH3KP --json`) for provider and ranking details
and [`PERFORMANCE.md`](../../../../docs/PERFORMANCE.md) for measured profile costs.

## Current generational projection details

The v15 DDL also includes `generational_discovery_name`, `generational_discovery_time`, `generational_discovery_segment`, and `generational_discovery_lens`, with their key/ID/label/segment indexes. Typed genealogy beliefs use `generational_knowledge_assertion`, `generational_knowledge_endpoint`, and `generational_knowledge_evidence` plus knower/endpoint/state indexes. These are disposable read-model structures, not new canonical record kinds or permission to infer genealogy. Endpoint/time covering parentage indexes, prefix lookup and viewer-specific discovery structures support the accepted bounded query contracts; source records and positive authored knowledge retain their own privacy and citation rules. Readiness checks exact required table/index/foreign-key shapes before accepting the revision fingerprint.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNjRkYmU5N2M0MDFhYjM0ZGYwMDhlZTJkYjU3MjFkMTE0MWJmMDY4ZSIsImkiOiJzaGEyNTY6M2RhM200aDZ6b0Y3MmRpTlRROUpCcE8tdnFiTjlEaG9HMVdhbWJfZXFyTSIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNDkyMEI2RUVLMEFFMVIzTURHQTdZRzMiLCJvcCI6Ik8wMU00OTIwQjZFRUswQUUxUjNNREdBN1lHMyIsInAiOlsiUjAxTTQ4UzFXMTFSMTdZM1ROUE41QjlZWkZLIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OkN3UFJJR1ZlOUx1d3VvME5XMWZiOFVZUVJxYk53S0ZLMTFSU2V2S2gtS0kiLCJ0IjoxNzkxMzA1NDYwOTQyLCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
