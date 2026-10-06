# 0.4 interaction and iteration notes

> Historical report. Original versions, fixture counts, outcomes and measurements below belong to the recorded exercise. This migration performs no fresh release/performance qualification. Architectural rationale is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; custody is in ADRAI A01M49735W4D3CZ2PJ129HTJCGG. Read these with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in the WEDL development/source checkout.

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

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

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

### Context quality observations

The exercise reported better coverage and less crowding after its packet-selection changes. Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

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

## Bugs observed during the historical exercise

The exercise found untyped temporary conversation/recollection IDs, quadratic interaction projection, repeated YAML parsing, warm embedding-cache writes, and overly broad hybrid vector scoring. The large stress world had spent roughly 250 ms rewriting unchanged embedding-cache rows. These reported incidents and their original implementation explanations are preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.


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

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


## Fixture-stage limitation

The then-authored example intentionally left its final custody choice unresolved. This belongs to the earlier 238-record stage, not the subsequently completed 262-record package. Other historical implementation limitations and proposals are preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.


## Overall assessment

The prototype now supports a realistically long investigative arc without
context collapsing into a database dump. The most convincing feature is not the
number of entity types but the ability to answer three different questions from
one source history:

- What was literally said?
- What does this character remember and believe?
- What does the author know that the character must not use?

The exercise assessed local interactive performance as adequate at its measured scale. Its proposed next technical investment is historical rationale in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z, not a current roadmap.
