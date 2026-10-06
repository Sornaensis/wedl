# Authoring a Complete Story with wedl

> Historical authoring case study. Incident/rebuild observations below retain their original stage; current architecture is read through ADRAI A01M498R304BB0EEZ27RKTSN6QW. This cleanup does not reauthor the story or certify a rebuild.
This document records an interactive authoring pass over the bundled Ash Archive world. The goal was not to add another collection of entities, but to determine whether wedl could support a coherent mystery from its opening clues through a thematic ending while preserving character-specific knowledge, exact dialogue provenance, temporal state, and retrievable author intent.

## Result

The authored world now contains **262 canonical records**:

| Kind | Count |
|---|---:|
| Characters | 13 |
| Conversations | 16 |
| Environments | 15 |
| Events | 48 |
| Knowledge records | 63 |
| Locations | 20 |
| Objects | 24 |
| Relationships | 24 |
| Scenes | 16 |
| Story points | 22 |
| World | 1 |

The final corpus contains **138 canonical verbatim turns** and **58 character recollections**. The finished novella is available as [the novella](novella.md).

The story now has three complete movements:

1. **The catalog mystery** — the missing cards, Ilyra's disappearance, Oren's letter, Token 7B, and the hidden seventh-drawer rail.
2. **The surveillance and hearing arc** — Flood Gallery N, the defective Council writ, the bell-tube listening network, Rusk's requisitions, the opened letter, the ledger hearing, and Ilyra's return.
3. **The custody ending** — the descendant ledger is divided into proof, route, and identity; the reconstruction cross-index is destroyed under witness; the custodians escape through the river route; Caldrin loses Archive authority; Rusk becomes a compelled witness; and the Archive adopts an explicit record of protected omission.

The final closed scene is **An Honest Absence** at tick 208. The example deliberately has no active scene: it represents a completed story rather than a branch that must always remain mid-action.

## Authoring process

### 1. Reconstruct the current dramatic problem

I began by generating character contexts for Mara, Ilyra, Nessa, Ysabet, and Sister Ansel at `The Choice of Records`, plus an author context and a dramatic-irony context.

Those packets agreed on the central conflict without flattening the characters into one perspective:

- Mara wanted defensible provenance without recreating a weapon.
- Ilyra distrusted any “temporary” central index.
- Nessa proposed dividing proof, route, and identity.
- Ysabet needed an arrangement she could defend procedurally.
- Ansel cared about who could physically reconstruct the route.

This was the strongest part of the system. The entity graph and temporal knowledge records made it difficult to accidentally solve the wrong problem or give one character another character's reasoning.

### 2. Commit the ending as world state

The ending was authored as one validated multi-record changeset. It added or updated 39 records, including:

- a completed custody conversation;
- the Threefold Custody Instrument;
- the Ledger of Honest Absences;
- the destruction of the reconstruction cross-index under witness;
- a River Gate pursuit and exact transcript;
- Rusk returning Sable's map and becoming a witness rather than a custodian;
- the Ember Hall reckoning;
- suspension of Caldrin's Archive authority;
- an epilogue conversation;
- resolution of the principal story points;
- final knowledge and recollection transitions.

The preview exposed the semantic consequences before committing them. This mattered because the ending touched object custody, knowledge, relationships, scenes, conversations, and story-point histories simultaneously. A prose-only edit would not have shown whether the actual world state agreed with the intended ending.

The historical changeset is retained as [the original ending changeset](../../../examples/complete-ash-archive-ending.json).

### 3. Interrogate the finished world

After applying the ending, I queried:

- final contexts for Mara, Ilyra, Nessa, Oren, and the author;
- all major story-point states;
- the final custody state of the route, proof, identity packets, and witness copy;
- Rusk's character view of the River Gate conversation;
- Oren's recollection after seeing Ilyra alive;
- historical Mara contexts at tick 121.

The checks found two coherence problems.

#### Stale missing-person beliefs

Mara, Nessa, and Oren still had active `Ilyra is missing` beliefs after direct later encounters with her. The canonical events were correct, but the subjective state had not been transitioned. A small follow-up changeset added explicit rejected transitions and updated Oren's post-contingency goal.

The explicit follow-up made the stale beliefs visible and corrected the authored case. Historical software design rationale is preserved in ADRAI A01M498R304BB0EEZ27RKTSN6QW. Read it with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M498R304BB0EEZ27RKTSN6QW --json` in the WEDL development/source checkout.

#### Future-knowledge search leak

A historical search at tick 121 could retrieve the body of a future knowledge record through a generic character-self document. The dedicated knowledge search document was time-scoped correctly; the generic prose index was not.

The recorded regression checked that tick-121 character search did not return the later records for:

- threefold custody;
- the honest-absence rule;
- Caldrin's suspension.

This bug would have been easy to miss in a small fixture. It appeared because the finished story asked the same characters about the same institutions at widely separated fictional times.

## What worked well

### Exact dialogue plus subjective memory

Canonical conversation turns gave the story stable prose anchors. The same exchange could then have incompatible recollections without corrupting what was literally said.

Examples include:

- Mara remembering Oren as protecting a route rather than merely himself;
- Nessa remembering Ysabet's procedural pause as potentially helpful or potentially bait;
- Rusk entering the River Gate conversation late and hearing only the final eleven turns;
- Oren remembering only the closing doorway exchange in `An Honest Absence`.

This separation was particularly valuable when writing the novella. Exact lines could be reused confidently, while close-third narration followed the viewpoint character's recollection rather than the author's omniscient transcript.

### Story-point evaluation as continuity pressure

The ending did not feel complete until the major points were actually resolved. The final checks require these points to be `resolved`:

- Build the Incomplete Archive
- Decide the Fate of the Descendant Ledger
- Survive the Vault Alarm
- Confront Caldrin Vey
- Question Sable at the River Gate
- Recover the Restricted-Vault Key

This prevented an apparent ending that left the world's formal narrative machinery in an earlier state.

### Object custody as narrative structure

The system handled the ending's theme through literal object placement:

- proof under notarial custody;
- route under hydraulic custody;
- identity packets under Archive custody;
- the reconstruction cross-index destroyed under witness;
- the map returned to Sable;
- Rusk retained as a compelled witness rather than a holder of evidence.

The thematic claim—no one office should hold proof, route, and identity—therefore exists as queryable state rather than commentary alone.

### Context packets remained character-specific

The final Mara packet at `An Honest Absence` emphasizes institutional design and defensibility. Oren's packet emphasizes the completed contingency, river routes, and his altered relationship to Ilyra. Rusk's River Gate view begins only at his entrance at tick 184.

Historical software design rationale is preserved in ADRAI A01M498R304BB0EEZ27RKTSN6QW. Read it with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M498R304BB0EEZ27RKTSN6QW --json` in the WEDL development/source checkout.

## Friction and limitations

### Large coordinated changesets were cumbersome

Constructing the ending's dozens of interdependent operations was reported as cumbersome. The proposed editor/macros are historical rationale in ADRAI A01M498R304BB0EEZ27RKTSN6QW.

### Historical knowledge titles

The authoring pass found the unchanged title `Oren believes Ilyra has disappeared` misleading after its state became `rejected`. Its proposed UI remedy is in ADRAI A01M498R304BB0EEZ27RKTSN6QW.

### Selecting a completed-story scene

The completed package has no active scene. Specify `An Honest Absence` or another historical scene when building a context. See the [command guide](../../guides/command-line.md). The old UI proposal is in ADRAI A01M498R304BB0EEZ27RKTSN6QW.

### Historical search observations

The exercise still retrieved older records sharing terms with later concepts. Its regression checked future record IDs rather than requiring lexically unrelated results. The design explanation is in ADRAI A01M498R304BB0EEZ27RKTSN6QW.

## Writing the novella

After the canonical world passed the final checks, I wrote [the novella](novella.md) as a close-third novella centered on Mara.

The manuscript follows the world's information order rather than simply summarizing its database:

1. The catalog gap is established before the route is understood.
2. Ilyra's disappearance remains ambiguous.
3. Oren's warm-sealed letter introduces the courier network.
4. The seventh drawer converts absence into mechanism.
5. The bell-tube network turns a catalog mystery into institutional surveillance.
6. Flood Gallery N supplies evidence but not a simple culprit.
7. The letter and listening office make Ilyra's contingency legible.
8. The hearing forces private evidence into public provenance.
9. Ilyra's return complicates rather than ends Mara's succession.
10. The final custody decision resolves the theme: preservation can include a witnessed, reviewable absence without preserving a central key to vulnerable identities.

The final line—“The Archive admitted what it did not hold.”—is supported by the actual final objects, events, knowledge records, and custody design.

## Recorded regression conclusions

The completed-story exercise recorded these regression checks:

- 262 canonical records;
- 16 conversations;
- 138 verbatim turns;
- 58 recollections;
- zero active scenes after completion;
- a closed epilogue at tick 208;
- resolved major story points;
- hard-bounded final context;
- late-entry conversation filtering;
- rejected stale missing-person belief;
- historical search exclusion for future knowledge bodies;
- author-only secret-marker protection;
- source/SQLite/cache/vector behavior inherited from the 0.5 profile suite.

The dated [0.5.1 validation receipt](../../reports/validation-0.5.1.md) retains its original measurements and packaging provenance. The original ending payload is historical input; its recorded expected HEAD is not a current-package apply instruction.
