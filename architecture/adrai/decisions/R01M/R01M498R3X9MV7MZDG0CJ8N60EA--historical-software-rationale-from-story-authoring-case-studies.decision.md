+++
schema = "adrai/decision/v1"
adr = "A01M498R304BB0EEZ27RKTSN6QW"
record = "R01M498R3X9MV7MZDG0CJ8N60EA"
title = "Historical software rationale from story-authoring case studies"
summary = "Preserve nonnormative case-study architecture while retaining manuscripts, unique transcripts and accurate stage-specific workflows."
domains = ["architecture-history"]
+++

# Historical software rationale from story-authoring case studies

## Decision and authority

Preserve the following software explanations/proposals through ADRAI while keeping
narrative prose, transcripts, practical workflows and dated incidents in story guides.
These are nonnormative historical excerpts, not new approval of a proposed editor,
macro, path contract, dashboard or committed-but-compile-failed/last-good guarantee.
Original decisions and their approval/supersession provenance remain intact.

Current authority: source A01M48S0B8N1M7HRABA7YSM5ZR7; context
A01M48RRCERE8D2MG7AZ9CD5JQ5; conversations A01M48RVATM4A6XDWHY9S6Z4ESB;
search A01M48RW32X6AEHK2376V4YH3KP; event consequences
A01M48VHYA0RGNJWBFZJZMGVZX6; command/automation
A01M494PM59C6BG1W7K1CXXYR0H; spatial source A01M48ZRCS8CP9EC5H5YH2MRVQ4.
Current source implements typed consequence authoring and staged preview/confirmation;
that does not silently approve every older macro or UI proposal below. Canonical
Git source and disposable compilation retain their existing contract boundaries.

The lexical/generic knowledge-index exclusions describe the original Ash authoring
incident; the current typed generational knowledge extension is distinct and its
internal knowledge-history does not create a public operation. Earlier spatial
component-only/no-generic-support statements describe their original stage:
generic v0.7 source, compiled/public queries, explorer and confirmed authoring now
exist. These excerpts do not withdraw those approved/implemented boundaries.

Snapshot provenance: Ash seed lists 119 non-world IDs plus world (120) at tick121;
unresolved Ash is 238 records at tick178:10; completed package is 262 at tick208.
Current Frontiersmen is 309 records/19 conversations/248 turns/main210:0; its earlier
307 non-world rebuild claim is inherited historical provenance, not a fresh rebuild.
The unchanged pursuit manuscript and old 294-record measurements are separate history.

## Original software excerpts


### Historical source: `docs/STORY_AUTHORING_NOTES.md`

This is an example of useful friction. wedl does not silently infer that seeing Ilyra must rewrite every person's belief. The author has to state the epistemic consequence, but the query system then makes the omission visible.

### Historical source: `docs/STORY_AUTHORING_NOTES.md`

The search compiler now excludes knowledge records from generic section indexing. Knowledge is indexed only through its dedicated temporal lane. Regression coverage verifies that the tick-121 character search cannot return the later records for:

### Historical source: `docs/STORY_AUTHORING_NOTES.md`

The packets differ because the underlying knowledge and recollection records differ, not because a prompt asks the model to role-play harder.

### Historical source: `docs/STORY_AUTHORING_NOTES.md`

### Large coordinated changesets are hard to author manually

The ending changeset was correct and valuable, but constructing dozens of interdependent operations by hand is still cumbersome. A higher-level scene-outcome editor should be able to generate:

- an event;
- knowledge transitions caused by it;
- relationship transitions;
- object placement;
- story-point transitions;
- scene closure and next-scene creation;
- conversation turns and recollections;

from one event-centered form.

### Historical source: `docs/STORY_AUTHORING_NOTES.md`

### Titles of historical knowledge records can become misleading

A knowledge record titled `Oren believes Ilyra has disappeared` remains the same source entity after its active transition becomes `rejected`. Query output is correct, but source browsing may overemphasize the initial state. A future UI should display the active stance beside the record title or support a neutral title such as `Ilyra's missing status according to Oren`.

### Historical source: `docs/STORY_AUTHORING_NOTES.md`

### A completed world has no implicit context cursor

The final example intentionally has no active scene. Context commands must specify `An Honest Absence` or another historical scene explicitly. This is semantically honest, but the UI should make a completed world's latest closed scene easy to select without pretending it is still active.

### Historical source: `docs/STORY_AUTHORING_NOTES.md`

### Search remains associative

Removing the future-knowledge document leak prevents direct temporal disclosure, but hybrid/vector search may still return older records that share terms with a future concept. That is expected associative retrieval. Tests therefore assert that future record IDs are absent rather than requiring all results to be lexically unrelated.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

The boundary-tightening follow-up ends the closed Hunt and Running intervals at
`main 195:99`, strictly before the Drowned Waymark begins at `main 196:0` under
wedl's inclusive endpoint semantics. It also removes the Hunt's stale `active`
tag without changing the narrative sequence.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

The ID normalization replaces request-hash-derived recollection identifiers
with stable IDs seeded by conversation and authoring slug. A clean seven-act
rebuild reproduces the 307 non-world canonical Markdown records byte-for-byte.
The remaining `story/world.md` record retains the repository-local ID generated
when the rebuild target is initialized, so it is byte-for-byte identical only
when that target was initialized with the same world ID.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

**Fix:** the context composer groups equivalent objects into one compact phrase when their narrative role and state are identical.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

**Fixes:**

- compiler insertion defensively deduplicates tags and aliases;
- source validation reports duplicate tags/aliases before apply;
- fixture generation uses stable set-like construction.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

The incident also highlighted an architectural concern: a changeset can advance Git and then fail during derived compilation. Source remains valid and canonical, but the user receives a failed post-commit compile. Future work should make the receipt distinguish `committed-but-compile-failed` explicitly and improve automatic last-good behavior.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

### Large frontmatter records

Conversations with many turns and recollections are powerful but verbose. For human authors, a transcript-oriented Markdown syntax or dedicated conversation editor would be more ergonomic than editing large arrays.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

### Cross-record event consequences

A major beat can legitimately affect five characters' knowledge, several relationships, object state, and multiple story points. The changeset model handles this correctly, but manually building such payloads is laborious. Higher-level authoring macros such as `record_scene_outcome` would help.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

### Source-path readability

Generated IDs are stable and safe, but the Git tree remains easier to browse when paths include human slugs. The current generator does this reasonably well; it should remain a contract.

### Historical source: `docs/FRONTIERSMEN_AUTHORING_NOTES.md`

### Current versus completed campaign

This example intentionally stops on an active southward journey after resolving only the immediate pursuit. The UI should make “campaign currently live” distinct from “fixture incomplete.” A campaign dashboard could show active scene, unresolved high-priority points, outstanding duties, and last canonical conversation.

### Historical source: `docs/examples/README.md`

For human readability, fixture paths use short stable slugs. Identity comes
exclusively from frontmatter IDs, so renaming a fixture file does not alter its
semantics. The current CLI accepts IDs, titles, aliases, and slugs as entity
references; it does not impose a generated `<slug>--<id>.md` filename format.

### Historical source: `docs/examples/README.md`

## Latent spatial component fixture

`spatial-source-component-v07.yaml` is a source-validator fixture for the
accepted spatial contract.  It is not a generic runtime-accepted source world.

### Historical source: `docs/examples/MANIFEST.md`

## Latent v0.7 component fixture

`spatial-source-component-v07.yaml` exercises component-only maps, opaque
references, directed routes, and bounded overlays.  It does not activate a
migration, public endpoint, compiled index, or UI.

### Historical source: `docs/examples/STORY_GUIDE.md`

None is canon until a new event/change set is committed.

### Historical source: `docs/examples/EXPECTED_QUERIES.md`

`--query` raises relevance for matching optional material; it does not turn
author information into character knowledge. A context response is hard-limited
by `--max-characters`. The minimum is at least 1,800 characters, and the command
reports a larger exact minimum when a particular packet needs one.

### Historical source: `docs/examples/EXPECTED_QUERIES.md`

Hybrid output identifies the available retrieval lanes and their ranks. The
character filter and fictional-time boundary are applied before ranking, so a
result cannot reveal material inaccessible to Rhea at that moment.

### Historical source: `docs/FRONTIERSMEN_WORLD_GUIDE.md`

Character search must never expose that token.

## Transfer provenance

Original inputs were inspected at `aa9ea905355b6d5a8e12fe6d2fdb7df17f81dd32`. Two prose manuscripts and the seed
changeset payload retain exact bytes; unique transcripts/IDs remain in story guides.
No source story, runtime, test assertion, performance budget or qualification changes.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiYWE5ZWE5MDUzNTViNmQ1YThlMTJmZTZkMmZkYjdkZjE3ZjgxZGQzMiIsImkiOiJzaGEyNTY6M3F4R3VicWdxTnJtdi1QTFpGNDkyODAtWEI3UFhqVmdNRjNpV3hrbmZwbyIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5OFIzWDlNVjdNWkRHMENKOE42MEVBIiwib3AiOiJPMDFNNDk4UjNYOU1WN01aREcwQ0o4TjYwRUEiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpVSWE2RDlWaG9MUlZVaVQ5MzBaMzJUQl9CXzZ5ekVHa1lZNHhZLXRxLVM0IiwidCI6MTc5MTMxMjUzMTM2OSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
