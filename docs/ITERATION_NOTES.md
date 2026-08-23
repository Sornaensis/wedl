# 0.4 interaction and iteration notes

This document records what it was like to use wedl while expanding the example,
profiling it, and repeatedly querying it rather than merely listing implemented
features.

## What was exercised

The iteration used wedl itself to:

1. Expand the Ash Archive through a complete second act in one 94-file candidate
   change set.
2. Preview the candidate world, inspect generated IDs, and correct temporal and
   inverse-relationship diagnostics before apply.
3. Apply the expansion as one validated Git commit.
4. Add a continuity follow-up resolving story points and refreshing enduring
   character goals that had become stale.
5. Query five characters from the same active scene.
6. Compare author, character, and dramatic-irony packets.
7. Inspect the active transcript from early and late participants.
8. Search for author-only secret markers from author and character scopes.
9. Append another verbatim turn and two conflicting recollections through the
   public changeset format.
10. Preview, apply, and replay that changeset through both Python and HTTP
    surfaces.
11. Generate and compile 594-, 4,302-, and 10,662-record worlds.
12. Re-run the full regression suite after each performance or semantic fix.

## How the narrative workflow felt

### The story was easiest to extend around decisions, not encyclopedic records

The most productive unit was a scene-ending choice that naturally generated
several record types at once. For example, the Flood Gallery sequence created:

- physical state changes;
- a canonical conversation;
- different character memories of that conversation;
- new evidence objects;
- relationship movement;
- knowledge transitions;
- story-point transitions;
- and a new active scene.

This is a better fit for wedl than independently filling CRUD forms. The
changeset should remain event/scene/outcome-oriented even though the source tree
is entity-oriented.

### Verbatim transcript plus recollection is materially useful

The active conversation demonstrates the distinction well:

- Mara, Ilyra, Nessa, and Ysabet hear the first six turns.
- Sister Ansel enters at tick 178 and hears only: “The shutters are closing.
  Argue while walking.”
- Later recollections can preserve exact turn IDs or approximate wording.
- Nessa can remember Mara’s proposal as reluctant acceptance while Mara
  remembers it as provisional operational necessity.

The transcript gives repeatable provenance. The recollection gives usable
character continuity. Neither needs to pretend to be the other.

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

### Different packet sizes can be correct

Mara’s 5,000-character packet is almost full because she has extensive
knowledge and relationships. Sister Ansel’s packet is smaller because she is a
late entrant with narrower Archive history. Padding Ansel’s packet with generic
lore would reduce perspective fidelity rather than improve it.

### Author context exposed stale continuity quickly

After expanding the story, several early story points remained eligible even
though later events had completed their substance. Character goals also still
referred to earlier phases. These were not parser bugs; they were authoring
continuity debt made obvious by the query surface. A final example-authoring
pass now resolves completed points and retains only genuinely open branches.

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

## Performance observations

### What is no longer the bottleneck

- Git process count.
- Repeated YAML parsing.
- Pairwise interaction materialization.
- Reconstructing the compiled world for every in-process query.
- Updating every cached embedding on warm compile.

### What now dominates cold large compilation

For the 10,662-record world, search projection accounts for the majority of cold
compile time. Within it, the largest costs are:

1. Generating missing feature vectors.
2. Persisting new vector-cache entries.
3. Building the perspective-scoped document corpus.
4. Preparing and inserting search/FTS/embedding rows.

This is a healthy next boundary: narrative semantics and source parsing are now
small compared with intentionally optional retrieval projection.

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

## Overall assessment

The prototype now supports a realistically long investigative arc without
context collapsing into a database dump. The most convincing feature is not the
number of entity types but the ability to answer three different questions from
one source history:

- What was literally said?
- What does this character remember and believe?
- What does the author know that the character must not use?

Performance is adequate for a single-author local tool at several thousand
records, and exact revision reuse is effectively immediate. The next major
technical investment should be incremental search projection, not more source
schema machinery.
