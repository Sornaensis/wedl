# 0.5.0 Search Iteration Notes

## Baseline problem

The 0.4 prototype exposed one search setting and called a deterministic lexical
feature hash “vector search.” Hybrid mode generated FTS candidates first and
used the feature hash only as a reranker. That design was fast but did not prove
independent semantic recall and could not express the storage/latency tradeoff
between state-only, lexical, vector-only, and combined repositories.

## Changes made

- Added explicit `state`, `fts`, `vector`, and `hybrid` compiler profiles.
- Replaced the default hash with corpus-trained TF-IDF plus truncated SVD.
- Added optional sentence-transformers and OpenAI-compatible providers.
- Added provider validation and unconditional L2 normalization.
- Added title, aliases, heading, domain, and tags to vector inputs.
- Expanded FTS to six weighted columns with Porter stemming.
- Normalized vector storage so repeated document projections share one BLOB.
- Added separate author and character models for corpus-trained LSA.
- Made exact vector retrieval scan every authorized unique vector independently
  of the FTS lane.
- Fused independent lane unions through weighted reciprocal-rank fusion.
- Cached immutable NumPy model matrices, exact query vectors, and static search
  state in-process.
- Loaded full document text and metadata only after winning vector IDs were
  known.

## Interactive observations

### Lexical query

For `counterseal fragment`, FTS and vector both ranked **Archive Counterseal
Fragment** first. Hybrid strengthened records supported by both lanes and kept
FTS snippets.

### Semantic query

For `legal authenticity coercion`, FTS found four records with direct lexical
support. The vector lane recovered twenty records, headed by **The Notary at the
Seventh Drawer**, **Ysabet Crane**, **The Seventh Drawer**, and **Council
Inventory Writ**. Hybrid retained sixteen vector-only results and three results
supported by both lanes.

### Technical-story query

For `subterranean water pressure records`, vector retrieval grouped **Pressure
Gate Seven Key**, **Pressure Gate Seven**, **The Pressure Gate**, and **Rising
Flood Pressure** without requiring all query words to appear in every chunk.
Hybrid preserved those semantic connections while allowing exact lexical clues
to outrank weaker similarities.

### Confidentiality

The character-scoped query for `ASH-SECRET-LETTER-CONTENTS-7F3Q` returned no
results in hybrid mode. The character LSA basis is trained only on public text,
so hidden author prose cannot influence a character’s latent space even before
SQL authorization filters document links.

## Performance interpretation

The local LSA provider moves cost from query time to cold compilation. On the
238-record fixture, vector/hybrid cold builds take roughly half to three
quarters of a second; warm forced builds take about a tenth of a second and
exact revision reuse about six milliseconds. Query medians remain single-digit
milliseconds.

On 4,303 records, cold LSA fitting takes several seconds and exact vector scans
remain in tens of milliseconds. At this scale, vector fitting and disposable
projection rebuild are the useful optimization boundary—not source parsing or
FTS itself.

## Remaining search work

- Add quality fixtures with human relevance judgments rather than only expected
  top records.
- Measure a real sentence-transformer model on local hardware.
- Reuse unchanged search rows across fast-forward revision builds.
- Consider ANN only if exact authorized scans exceed an accepted latency target.
- Explore model-specific chunk sizes for neural embeddings without weakening
  source citations or audience boundaries.
