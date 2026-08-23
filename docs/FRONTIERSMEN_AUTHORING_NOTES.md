# Authoring The Frontiersmen with wedl

## Method

The world was not written as one static YAML generation. It was authored in seven act-sized Git revisions through wedl changesets, followed by a reviewed boundary-tightening changeset and a deterministic-ID normalization changeset:

1. **Foundation** — geography, cast, amber economy, historical Blackroot truth, initial relationships and story points.
2. **Arrival and eastroad** — Keldmouth, guild hiring, wages, caravan travel, first glimmer, Harrowcross arrival.
3. **Harrowcross investigation** — abandonment, protection ledger, first wretch, dowsing, orchard, Saint Orra lead.
4. **Under the Frontier** — mine negotiation, cave-in, two days below, Sunken Hall, reliquary, gallery, hilltop exit.
5. **Tree King and pursuit** — masked camp, exact audience, blood sport, Moth's intervention, escape, chase.
6. **Perspective cleanup** — recollections, scene-presence correction, relationship timing, compact object grouping.
7. **Drowned Waymark** — running water breaks the immediate pursuit, a tightly bounded Blackroot counterfoil is recovered, and the five take an uncertain abandoned road south.

The boundary-tightening follow-up ends the closed Hunt and Running intervals at
`main 195:99`, strictly before the Drowned Waymark begins at `main 196:0` under
wedl's inclusive endpoint semantics. It also removes the Hunt's stale `active`
tag without changing the narrative sequence.

The ID normalization replaces request-hash-derived recollection identifiers
with stable IDs seeded by conversation and authoring slug. A clean seven-act
rebuild reproduces the 307 non-world canonical Markdown records byte-for-byte.
The remaining `story/world.md` record retains the repository-local ID generated
when the rebuild target is initialized, so it is byte-for-byte identical only
when that target was initialized with the same world ID.

After each act I ran:

- whole-world validation;
- SQLite compilation;
- character, author, and dramatic-irony contexts;
- FTS, vector, and hybrid searches;
- conversation views from multiple participants;
- story-point evaluation;
- secret-marker probes;
- changeset preview and atomic apply.

The act-sized commits were much easier to reason about than a single campaign-sized changeset. They also made the Git history meaningful: each commit corresponds to a narrative movement and can be compiled or queried independently.

## How the character packets affected the story

The context system was most useful when it did **not** converge the party onto one correct theory.

At Harrowcross:

- Rhea's packet emphasized contractual abandonment and responsibility.
- Sylvi's emphasized missing animals and the ecological boundary around the attack.
- Garran's emphasized wounds, thresholds, and whether the apparition suffered.
- Veyra's emphasized amber warmth and sympathetic response.
- Pip's emphasized Maela's preparations to leave before the reassignment was announced.

That divergence determined the investigation. Instead of an NPC explaining amber, each hero contributed a different evidentiary lane. The party only reached Saint Orra after testimony, physical recurrence, dowsing, and ledger evidence aligned.

At the hilltop, every packet advised caution, but for different reasons. The party approached the camp because food, injury, and loss of route made controlled contact less dangerous than indefinite wandering. This avoided the familiar authorial cheat in which competent characters walk into an obviously evil camp because the plot needs capture.

## Conversation transcripts as authoring anchors

The exact transcripts were particularly valuable in three ways.

### Repeatable wording

Aldren's claims do not drift between scene notes, recollections, and later context. The canonical line—“Aldren Veyl, then. Tree King, now.”—exists once as a stable turn. Every later memory can cite it, omit it, or approximately distort it without changing what was said.

### Presence-aware knowledge

Nessa-style late-arrival problems from the earlier prototype are avoided. In this world:

- Nara hears the pay-table explanation but not the dock conversation.
- Moth is present during captivity but never receives a spoken line.
- Rootjaw's pursuit view begins only when his distant bellow and the later shouted exchange become audible.

### Character memory without perfect transcript replay

After an exchange ends, a character packet can use the character's recollection instead of reproducing the full transcript. This is more useful for fiction. A recollection carries emotional interpretation and selective wording; it is exactly the kind of continuity an author needs when writing a later reaction.

## Problems found and corrected

### 1. Future relationship leakage

The foundation originally predeclared relationships between Rhea and later characters such as the Tree King and Moth. Even with neutral metrics, the existence and title of the future endpoint could influence early character search.

**Fix:** those relationships now begin with timed transitions at the actual encounter. Historical tick-55 search for `Tree King Aldren Veyl` does not retrieve the Tree King.

### 2. Audible dialogue versus physical presence

Rootjaw bellows from a distance during the final pursuit conversation. Initially he was also listed as a scene participant, which let the context system treat him as physically present.

**Fix:** he remains a conversation participant with a timed audible entry, but is not a scene participant. His conversation view is valid; his scene context is rejected.

### 3. Repeated guild-badge bloat

The final party scene contains five functionally identical Lantern Pike badges. Listing each badge separately consumed context space that should have gone to Moth's bone key, the reliquary, the map, or the broken spear.

**Fix:** the context composer groups equivalent objects into one compact phrase when their narrative role and state are identical.

### 4. Wrong compilation profile during authoring

An early scratch repository used an FTS-only compiled database while the context command requested hybrid retrieval. The failure was correct but initially inconvenient.

**Fix:** the example world declares `hybrid` as its default compilation profile. The CLI still fails clearly when a caller requests a lane the database does not contain.

### 5. Duplicate tags discovered after apply

One expansion record carried a duplicate tag. SQLite's normalized tag table rejected it during post-commit compilation.

**Fixes:**

- compiler insertion defensively deduplicates tags and aliases;
- source validation reports duplicate tags/aliases before apply;
- fixture generation uses stable set-like construction.

The incident also highlighted an architectural concern: a changeset can advance Git and then fail during derived compilation. Source remains valid and canonical, but the user receives a failed post-commit compile. Future work should make the receipt distinguish `committed-but-compile-failed` explicitly and improve automatic last-good behavior.

### 6. Builder reruns

The initial builder assumed a fresh repository. Rerunning from act zero collided with idempotency receipts or existing records.

**Fix:** `tools/build_frontiersmen.py` now accepts `--start` and `--through`, allowing a build to resume from a known act index.

## Where the tool was strong

- **Whole-world diagnostics:** wrong reference kinds, inverse relationships, transition ordering, and story-point dependencies were found before final commits.
- **Atomic narrative outcomes:** one act could create events, knowledge, relationships, scenes, conversations, and story-point transitions as one Git revision.
- **Perspective testing:** author and character retrieval made leaks visible immediately.
- **Temporal queries:** the party's theory could be inspected before and after each clue without rewriting history.
- **Conversation provenance:** exact words and subjective memory remained cleanly separate.
- **Context budgets:** the final five packets fit below 5,000 serialized characters without becoming identical.
- **Independent retrieval lanes:** FTS was strongest for exact Frontier terms; vectors helped with conceptual probes such as wounded memory taking animal form; hybrid preserved both.

## Where authoring was cumbersome

### Large frontmatter records

Conversations with many turns and recollections are powerful but verbose. For human authors, a transcript-oriented Markdown syntax or dedicated conversation editor would be more ergonomic than editing large arrays.

### Cross-record event consequences

A major beat can legitimately affect five characters' knowledge, several relationships, object state, and multiple story points. The changeset model handles this correctly, but manually building such payloads is laborious. Higher-level authoring macros such as `record_scene_outcome` would help.

### Source-path readability

Generated IDs are stable and safe, but the Git tree remains easier to browse when paths include human slugs. The current generator does this reasonably well; it should remain a contract.

### Current versus completed campaign

This example intentionally stops on an active southward journey after resolving only the immediate pursuit. The UI should make “campaign currently live” distinct from “fixture incomplete.” A campaign dashboard could show active scene, unresolved high-priority points, outstanding duties, and last canonical conversation.

## Recommended next authoring iteration

The next movement should not immediately explain Moth. Act 6 has already forced the party to choose bounded evidence and an uncertain southward route; a later movement can test the consequences without collapsing those mysteries:

- learn where the abandoned warden road actually leads;
- decide how and when to present the counterfoil;
- destroy or conceal the reliquary;
- rescue Jorund or return to Harrowcross;
- confront Lantern Pike with evidence that is still partly Aldren's testimony.

A strong continuation would make Moth's identity a consequence of how the party treats captured or masked people, not merely a lore reveal.
