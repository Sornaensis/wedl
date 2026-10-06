+++
schema = "adrai/decision/v1"
adr = "A01M4971W13BPKM0G1FPZ86MA9Z"
record = "R01M4971W7AABHZS61TNEVG7ZWY"
title = "Historical architectural rationale from release and authoring reports"
summary = "Preserve historical evidence and its authority boundaries during report relocation; no new release qualification."
domains = ["architecture-history"]
+++

# Historical architectural rationale from release and authoring reports

## Decision and authority

Retain the architectural explanations and proposals below as historical provenance
in ADRAI while keeping measured receipts and observations in docs/reports. These
excerpts are nonnormative: they describe their original release/authoring/cleanup
stage, do not ratify old recommendations, and do not redefine current behavior.
No benchmark or release qualification was performed for this transfer.

Current source/storage, context, search and thread contracts retain authority.
The earlier lexical hash hybrid lane described in the 0.4 notes is historical;
the 0.5 profile/provider design and current search contract are distinct stages.
The earlier latent spatial/no-public-support wording is historical: generic
v0.7 source, direct public reads, explorer and confirmed authoring now exist.
Their measurements still do not qualify authored/API/browser performance.
The unresolved Ash stage and Frontiersmen pursuit-stage counts are historical,
not current package inventories. Current source has 262 completed Ash records
and 309 Frontiersmen records; the old 294/17/224/tick195 measurements remain unchanged.

Live boundaries are read through ADRAI: storage A01M48S1T65PSN638XCB59539Q8;
context A01M48RRCERE8D2MG7AZ9CD5JQ5; source A01M48S0B8N1M7HRABA7YSM5ZR7;
threads A01M48RX5WAQT5ECH66KTCFVC0T; spatial query
A01M48ZSKSZ3EKW7EZX5HDTSRET; configured verification
A01M494QCT41GF34E92FKKHQ8AS; scratch policy A01M3Y3S5QMJWMBMQT42HDQPDJS.
The historical cleanup's claims of archived/decompressed receipts are preserved
as recorded history. At this task's entry, its protected output directory exists
but named root-receipts.zip and legacy-retirement/diagnostics.zip are absent.
This transfer neither reconstructs them nor claims they were reverified.

## Original architectural excerpts


### Historical source: `docs/ITERATION_NOTES.md`

This is a better fit for wedl than independently filling CRUD forms. The
changeset should remain event/scene/outcome-oriented even though the source tree
is entity-oriented.

### Historical source: `docs/ITERATION_NOTES.md`

### Context quality improved when coverage was guaranteed

Pure ranking tended to spend too many slots on slightly different evidence
fragments. The packet became more useful after it guaranteed one atom from each
available category before filling by score:

- immediate perception;
- live conversation;
- relevant belief;
- present relationship;
- remembered conversation;
- optional retrieval.

Per-section quotas also prevented a character with many knowledge records from
crowding out the current exchange.

### Historical source: `docs/ITERATION_NOTES.md`

## Bugs found through use and fixed

### Temporary conversation IDs were initially untyped

Appending a turn with `$tmp.…` originally persisted the placeholder instead of
allocating a `turn_…` ID. Recollection IDs had the same risk. Allocation now
occurs before planning, preview returns the mapping, apply uses the typed IDs,
and replay returns the same receipt.

### Interaction projection was accidentally quadratic

Materializing interactions by asking every character pair to scan every event
worked for the small fixture but scaled poorly. It now walks canonical events
once, emits actual participant pairs, and uses precomputed consequence maps.

### Unchanged YAML was reparsed excessively

Before source caching, validation and every query path paid PyYAML cost for the
same Git blobs. Parsed records are now cached by blob ID and parser fingerprint,
and exact compiled revisions can be recognized without source loading.

### Warm embedding reuse still performed useless writes

The cache originally updated `last_used` for every hit. The large stress world
spent roughly 250 ms rewriting unchanged rows. Cache vectors are immutable and
no eviction policy uses that field, so warm compiles now write only missing
hashes.

### Hybrid vector scoring scanned too broadly

The bundled hash vector is a lexical feature representation, not a neural
semantic encoder. Exact-scanning every authorized vector added cost without a
corresponding semantic benefit. Hybrid mode now uses FTS candidates and vector
reranking; explicit vector mode remains available for exact scans and for future
real embedding providers.

### Historical source: `docs/ITERATION_NOTES.md`

### Likely next performance work

- Normalize vectors by content hash so duplicate document rows reference one
  stored vector instead of repeating BLOBs.
- Permit separate compilation profiles: state/FTS only, hash-vector, and external
  semantic embedding.
- Reuse unchanged search documents during fast-forward compilation rather than
  rebuilding the complete search projection.
- Measure a real embedding provider before designing ANN support.
- Add a bounded compile-worker process if external embedding failures or memory
  pressure justify isolation.

### Historical source: `docs/ITERATION_NOTES.md`

## Remaining product limitations

- Changed revisions still perform a safe full SQLite rebuild rather than
  dependency-scoped row patching.
- The hash-vector provider is lexical and should not be described as semantic
  search.
- Author search may return two sections from the same logical entity; the
  context composer removes near-duplicates, but the raw search UI could expose
  a “one result per entity” option.
- The local FastAPI service is single-user and loopback-oriented.
- The browser interface remains JavaScript rather than the planned Elm client.
- The example’s final custody choice is intentionally unresolved; it is a
  continuation point, not missing fixture data.

### Historical source: `docs/ITERATION_NOTES.md`

Performance is adequate for a single-author local tool at several thousand
records, and exact revision reuse is effectively immediate. The next major
technical investment should be incremental search projection, not more source
schema machinery.

### Historical source: `docs/SEARCH_ITERATION_NOTES.md`

## Baseline problem

The 0.4 prototype exposed one search setting and called a deterministic lexical
feature hash “vector search.” Hybrid mode generated FTS candidates first and
used the feature hash only as a reranker. That design was fast but did not prove
independent semantic recall and could not express the storage/latency tradeoff
between state-only, lexical, vector-only, and combined repositories.

### Historical source: `docs/SEARCH_ITERATION_NOTES.md`

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

### Historical source: `docs/SEARCH_ITERATION_NOTES.md`

The character-scoped query for `ASH-SECRET-LETTER-CONTENTS-7F3Q` returned no
results in hybrid mode. The character LSA basis is trained only on public text,
so hidden author prose cannot influence a character’s latent space even before
SQL authorization filters document links.

### Historical source: `docs/SEARCH_ITERATION_NOTES.md`

At this scale, vector fitting and disposable
projection rebuild are the useful optimization boundary—not source parsing or
FTS itself.

### Historical source: `docs/SEARCH_ITERATION_NOTES.md`

## Remaining search work

- Add quality fixtures with human relevance judgments rather than only expected
  top records.
- Measure a real sentence-transformer model on local hardware.
- Reuse unchanged search rows across fast-forward revision builds.
- Consider ANN only if exact authorized scans exceed an accepted latency target.
- Explore model-specific chunk sizes for neural embeddings without weakening
  source citations or audience boundaries.

### Historical source: `docs/PERFORMANCE.md`

The local LSA cold fit is the visible cost. Warm forced compilation reuses the
content-addressed model/vector cache. The hybrid/vector databases contain 931
document links backed by 626 unique normalized vectors.

### Historical source: `docs/PERFORMANCE.md`

A subsequent runtime optimization caches the immutable
model matrix and query vectors in-process; repeated exact-vector queries no
longer decode all BLOBs.

### Historical source: `docs/PERFORMANCE.md`

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
[`SEARCH_PROFILE_BENCHMARKS.json`](../../../../docs/reports/SEARCH_PROFILE_BENCHMARKS.json) and
[`MEDIUM_SEARCH_PROFILE_BENCHMARKS.json`](../../../../docs/reports/MEDIUM_SEARCH_PROFILE_BENCHMARKS.json).

### Historical source: `docs/PERFORMANCE.md`

wedl performs a safe full rebuild for changed revisions and exact reuse for an
unchanged revision. Git Markdown remains canonical; SQLite and both content
caches are disposable.

### Historical source: `docs/PERFORMANCE.md`

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
did not claim a public spatial-query latency before the query contract was
implemented.

### Historical source: `docs/PERFORMANCE.md`

The compiled-only v0.7 query boundary uses exact SQLite projection rows and is
still not generic source, CLI, HTTP, or UI support.

### Historical source: `docs/PERFORMANCE.md`

Each read remains bounded by the ratified 100-result, 1,000-hierarchy/route
expansion, 2,000-authorized-overlay-candidate, and 10,000-geometry-candidate
limits.

### Historical source: `docs/PERFORMANCE.md`

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

### Historical source: `docs/VALIDATION.md`

## Causal and continuity validation (source schema `wedl/v0.3`)

The causal layer is source-only: it adds no SQLite migration and does not infer
facts from prose, shared cast, proximity, routes, or duration. `event.causes`
remains a direct list of authored event references. Validation requires its
members to be unique canonical events on the same timeline and strictly earlier
than the caused event; cycles are rejected. `causing_event` citations on
knowledge, relationship, and story-point transitions must reference canonical
events on the same timeline at or before the transition coordinate (`<=`;
same-coordinate causation is valid).

`story-point.outcome_events` and optional `scene.outcome_events` are typed
source relations. The former accepts ordered canonical event references; the
latter accepts unique canonical events inside the scene interval. The read APIs
also return advisory-only asymmetric plot links and cross-location causal edges;
these are prompts to record a handoff, never inferred canon or validation
failures.

Existing concurrent-place codes are intentionally unchanged:

- `WDL-EVENT-006` still reports an individual participant at two places in
  same-coordinate events.
- `WDL-SCENE-025`/`WDL-SCENE-026` still report historical overlapping scene
  presence and the resulting two-place contradiction.

New event-cause checks therefore begin at `WDL-EVENT-007`, and optional scene
outcome checks begin at `WDL-SCENE-027`; no existing diagnostic was renumbered.

### Historical source: `docs/VALIDATION.md`

The stress generator now creates a canonical relocation event before its active
scene, so generated participants and objects satisfy the same physical-scene
invariants as authored worlds.

### Historical source: `docs/VALIDATION.md`

The runtime now caches
the immutable normalized matrix and exact query vectors, so repeated in-process
exact scans avoid BLOB decoding and query reprojection.

### Historical source: `docs/VALIDATION.md`

Thread grouping is checked as empty after conversion and as a projection-only
layer once a declaration is authored: it does not create a second StoryTime,
cursor, causal graph, state projection, corpus, rank, vector, or embedding
space. The bounded membership projection uses one `record_thread`/`entity`
join for a batch (see the one-statement regression in
`tests/test_thread_memberships.py`); a no-selection browser view issues no
membership request.

### Historical source: `docs/VALIDATION-0.5.1.md`

A defect found during authoring allowed future knowledge prose to enter a timeless generic character-self index. The generic section compiler now excludes knowledge records; knowledge is indexed only through its time-scoped lane.

### Historical source: `docs/WORKSPACE_CLEANUP.md`

To restore a raw receipt, select its indexed member, verify the decompressed length and SHA256, and write only to an absent contained destination. Old rename/restore maps are historical provenance; after payload retirement they cannot restore whole fixtures. Rebuild future fixtures from maintained source.

### Historical source: `docs/WORKSPACE_CLEANUP.md`

## Guarded maintenance tools

These tools implement exact reviewed manifests for this maintenance task. They do not discover disposable content from a filename. Dry-run is the default; `-Apply` changes the filesystem. Supply the reviewed manifest SHA256 and a new receipt path on each invocation.

`tools/cleanup_workspace.ps1` checks workspace identity, containment, nonreparse ancestors, file hashes/lengths/timestamps and exact directory child rosters. Files use literal-path removal; directories use atomic nonrecursive deletion that refuses new children. Receipt-backed removal first verifies every decompressed archive member.

`tools/organize_workspace_runs.ps1` performs exact same-volume directory moves and reviewed restoration subsets with native identities and ancestor/location checks. There is no copy/delete fallback. Its 2026-10-02 maps describe the historical organization step, preceding retirement.

`tools/retire_workspace_runs.ps1` verifies the archive/index, exact root and ancestor identities, worktree exclusions, and preserved diagnostics' locked native fingerprints. NTFS hardlink aliases can expose stale directory-entry timestamps, so freshness uses handle metadata. All repository-to-root ancestor handles remain held during deletion. Descendant reparse entries are unlinked without following targets. Native unlink avoids changing read-only attributes shared with retained hardlinks; unsupported operations refuse rather than fall back.

```powershell
./tools/retire_workspace_runs.ps1 -Manifest output/repository-cleanup-20261002/legacy-retirement/retirement-manifest.json -ExpectedManifestSha256 <reviewed-sha256> -Receipt output/repository-cleanup-20261002/legacy-retirement/new-dry-run.json
# Append -Apply only for the independently approved exact manifest.
```

Immutable flushed checkpoints record each completed root, with explicit partial/refused outcomes. A crash between deletion and checkpoint publication can leave unrecorded removals; reconcile actual paths before preparing a new manifest. No process termination or ACL change is performed.

Bounded process checks see absolute canonical prefixes. Relative paths, aliases and processes identified only by their working directory are outside their visibility. Coordinate with known owners; these checks do not establish host-wide inactivity.

### Historical source: `docs/WORKSPACE_CLEANUP.md`

## Prevention and remaining work

The accepted bounded scratch and test-signal policy (ADRAI A01M3Y3S5QMJWMBMQT42HDQPDJS)
requires compact routine pass/fail reporting, one owned scratch area per active
task with a finite lifetime, and bounded diagnostic retention. It governs future
work; existing required evidence stays protected. The current test runner still
retains failure artifacts as described in [testing usage](../../../../docs/guides/testing.md); runner
retention changes remain separate work.

Root-anchored ignore rules cover observed scratch and receipt families. Ignoring a file does not expire it or prove disposability. A future runner change should direct scratch into one documented ignored directory with explicit retention. Current spatial staging/warm-read proof inputs and other historical output groups remain catalogued separately. Compatibility package-data copies, source organization, dead-code review and stale timing documentation need their own behavior review.

### Historical source: `docs/FRONTIERSMEN_PERFORMANCE.md`

The vector and hybrid cold builds include local TF-IDF/truncated-SVD fitting. Warm forced builds reuse the content-addressed model/vector cache. `state` remains suitable when retrieval is unnecessary; `fts` is the inexpensive full-text authoring profile; `hybrid` is the packaged default.

### Historical source: `docs/FRONTIERSMEN_PERFORMANCE.md`

It remains far below a forced rebuild, but the difference is worth profiling further: exact reuse should ideally avoid any work beyond revision/profile verification and opening the existing database.

### Historical source: `docs/FRONTIERSMEN_PERFORMANCE.md`

These calls use genuinely independent corpora:

- FTS uses weighted BM25 over title, aliases, heading, body, domain, and tags.
- Vector scans the complete authorized normalized-vector corpus.
- Hybrid retrieves from each lane independently and fuses the ranked union.

### Historical source: `docs/FRONTIERSMEN_PERFORMANCE.md`

## Authoring-performance findings

### Act-sized changesets are the right granularity

A campaign-sized changeset is possible but difficult to inspect. Six act commits made preview diagnostics, temporal reasoning, and idempotent replay manageable. The builder now supports `--start` and `--through` for resumable authoring.

### Search payloads are larger than context packets

A 20-result search response can approach 19 KB because it preserves lane evidence and citations for inspection. The final LLM packet is around 5 KB. This is appropriate for separate inspection and generation APIs, but the browser should not automatically inject raw search responses into model context.

### Local LSA fitting remains the cold-cost center

On this authored world, cold vector/hybrid compilation is dominated by local model fitting. Warm forced builds are much faster due to model and vector reuse. An external neural provider would shift the cost toward network/model latency and should be benchmarked separately.

### Exact hybrid reuse merits another pass

The 85.6 ms exact hybrid reuse is still interactive but unexpectedly above the 3–16 ms measured for the other profiles. The next performance pass should instrument exact-profile verification and retained-database opening separately.

## Transfer provenance

Original tracked inputs were read at `4d81ccc3c3cfc52381396c24458866cf1e6fb2ad`. Historical approvals, versions,
measurements, failures and missing-custody limits are not replaced by this archive
decision. The [exact-path reconciliation](../../../../docs/reports/retention-and-dispositions.md)
records report destinations, owners, retention and preserved receipt hashes.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNGQ4MWNjYzNjM2NmYzUyMzgxMzk2YzI0NDU4ODY2Y2YxZTZmYjJhZCIsImkiOiJzaGEyNTY6Q25RVkEtNDdob2xfTmlOWHlMeTFEY0tjbmZxcGp5ZlE5eVZhZWRYUXNqUSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5NzFXN0FBQkhaUzYxVE5FVkc3WldZIiwib3AiOiJPMDFNNDk3MVc3QUFCSFpTNjFUTkVWRzdaV1kiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpSb0g5TWFDQm5KVDVBUjRzLVFDUzFYY1pFdmtKb3JFejRNdmR0eFR5akIwIiwidCI6MTc5MTMxMDc1NDAyNiwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
