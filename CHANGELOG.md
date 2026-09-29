# Changelog

## Unreleased

- Added explicit v0.7 Ash Archive and Frontiersmen bootstrap choices and a
  v0.7 chronology conformance copy, generated from pinned legacy packages by
  an offline, checkable conversion tool. Existing examples and init defaults
  remain pinned to their original schemas.

- Added the public `wedl-chronology/v1` read protocol and confirmed complete
  chronology replacement authoring surface.

- Added the v0.6 conformance fixture and report builder, plus the confirmed
  local `upgrade-v06` route for homogeneous v0.3/v0.5 source. It preserves
  grouping data, creates no inferred dates, and is a validator-checked no-op
  for an already-valid v0.6 repository.

- Added local-only `wedl migrate preview|apply` for the narrow confirmed v0.3
  upgrade, quarantined v0.4 recovery, and forward rollback paths. It does not
  add an HTTP/browser migration surface or an in-place SQLite migration.

## 0.6.0

- Added **The Frontiersmen** as a second first-class executable example world.
- Authored the 294-record campaign through six atomic Git revisions using wedl changesets and perspective queries.
- Added 20 characters, 29 locations, 28 objects, 16 environments, 44 events, 61 knowledge records, 36 directional relationships, 20 scenes, 22 story points, and 17 canonical conversations.
- Added 224 immutable verbatim turns and 76 character-specific recollections across the Frontier campaign.
- Added named example selection to `wedl init`: `--example ash-archive` or `--example frontiersmen`.
- Added resumable `tools/build_frontiersmen.py --start N --through M` and the deterministic campaign specification.
- Fixed future-NPC leakage from predeclared relationship records by beginning those histories at the actual encounter.
- Separated distant conversation audibility from physical scene presence; Rootjaw can hear the chase without becoming a scene participant.
- Grouped equivalent repeated objects such as party guild badges in compact context packets.
- Added source validation for duplicate tags and aliases and defensive compiler deduplication.
- Added Frontiersmen world, narrative, interaction, authoring, leakage, conversation, budget, and story-point regression tests.
- Added campaign documentation, narrative walkthrough, world guide, interaction log, benchmarks, and a readable campaign chronicle.

## 0.5.1

- Completed the Ash Archive mystery through `An Honest Absence`.
- Expanded the executable fixture from 238 to 262 records, 16 conversations, 138 verbatim turns, and 58 recollections.
- Added the close-third novella `docs/THE_ASH_ARCHIVE.md`.
- Added a threefold custody ending, River Gate pursuit, Ember Hall reckoning, and explicit epilogue.
- Resolved the principal long-arc story points and corrected stale missing-person beliefs after Ilyra's return.
- Fixed a historical-search leak in which future knowledge prose could enter the timeless generic self-dossier index. Knowledge is now indexed only through its time-scoped lane.
- Added completed-story, historical-search, late-participant, final-context, and belief-coherence regression tests.
- Preserved the 0.5.0 compilation-profile and normalized-vector implementation.

## 0.5.0

- Added explicit `state`, `fts`, `vector`, and `hybrid` compilation profiles.
- Replaced the default lexical feature hash with local TF-IDF plus truncated-SVD
  latent-semantic vectors; retained feature hashing only as `hash-test`.
- Added optional sentence-transformers and OpenAI-compatible embedding providers.
- Added strict finite-value, dimension, and L2-normalization checks for every
  vector provider and stored vector.
- Normalized vector storage into model, unique content-hash vector, and
  document-link tables, eliminating repeated BLOBs.
- Added a persistent model/vector cache and warm forced-build reuse.
- Trained character LSA spaces only from public documents before projecting
  private authorized material, preserving filter-before-ranking confidentiality.
- Expanded FTS5 to weighted title, aliases, headings, prose, domain, and tags
  with Porter stemming and quoted-phrase parsing.
- Made vector retrieval independent of FTS candidates and fused the complete
  top-lane union with weighted reciprocal-rank fusion.
- Added in-process immutable vector-matrix, query-vector, and search-state caches.
- Vectorized exact cosine scoring with NumPy and delayed document/snippet loading
  until winning vector IDs are known.
- Added profile-specific compiler/search benchmarks and regression tests proving
  FTS-only, vector-only, and genuinely independent hybrid behavior.

## 0.4.0

- Expanded the Ash Archive from 152 to 238 records and added a complete second
  act through the unresolved `The Choice of Records` scene.
- Increased the fixture from 5 to 13 canonical conversations, 26 to 95 verbatim
  turns, and 11 to 38 subjective recollections.
- Added characters, locations, evidence objects, environments, events,
  relationships, knowledge, scenes, and story points for the pressure-gate,
  listening-office, ledger-hearing, and Ilyra-return arcs.
- Added typed temporary IDs for appended conversation turns and recollections.
- Added a persistent blob-keyed parsed-source cache and batched Git blob reads.
- Added an exact-revision compile fast path that avoids source parsing entirely.
- Replaced pairwise interaction materialization with one event pass.
- Batched embedding-cache lookups and removed useless hit-timestamp rewrites.
- Added per-substage search-projection timing.
- Moved ordinary indexes after bulk insertion and bulk-loaded core projections.
- Added in-process compiled-world reuse for repeated query/context/API calls.
- Bounded hybrid feature-vector reranking to FTS candidates while retaining
  explicit exact-vector mode.
- Added generated small, medium, and large stress worlds and a repeatable
  benchmark harness.
- Added second-act continuity tests, heavy API exercise, and interaction notes.

## 0.3.0

- Removed all runtime migration machinery.
- Added first-class verbatim conversations and time-versioned recollections.
- Replaced broad context dumps with hard-budgeted prose-first writing packets.
- Added timed participant and observation visibility.
- Added filter-before-ranking FTS/vector retrieval.
- Batched Git blob reads and added persistent embedding reuse.
- Expanded Ash Archive from 120 to 152 records across a longer mystery arc.
