# The Frontiersmen — Interactive Exercise Log

## Scope

The authored repository was exercised repeatedly at each act boundary. Outputs are retained under `examples/frontiersmen/interactive/` and include contexts, searches, conversation views, story-point evaluations, preview receipts, apply receipts, and validation results.

## Act-by-act observations

### Foundation and arrival

- The world began as one valid world record and was expanded through an atomic foundation changeset.
- Five distinct protagonist profiles produced clearly different packets before any mystery evidence existed.
- Author search could retrieve the Blackroot secret marker; character search could not.
- The pay-table conversation established that amber was currency without explaining it magically.

### Harrowcross

- The Last Watch transcript let the author compare canonical slaughter with Kellan's traumatized account.
- A second manifestation tied the amber chest to the attack without granting every character the same theory.
- `story-points` correctly moved caravan duty to resolved and dead-watch replacement to active.
- FTS found exact ledger and amber terms; vector search grouped blood, fear, walls, and remembered animals; hybrid returned both lanes.

### Saint Orra and the caverns

- Jorund's refusal generated useful suspicion without making him a villain.
- Context packets at the mine independently foregrounded the altered tally, missing ecology, bell rope, omitted drifts, and lamp color.
- Cave-in state changes moved the party underground while preserving Jorund's uncertain status.
- Historical context before the cave-in did not expose Sunken Hall knowledge.
- The second-night recollections remained different after the same exact conversation.

### Tree King camp

- Author context included Aldren's former guild identity and doctrine.
- Character packets included only what the party heard or inferred.
- Moth remained mute across the exact transcript.
- Search for the concealed name `Lio Vane` returned no character results.
- Blood-sport and escape story points transitioned independently.

### Pursuit

- All five final character packets stayed under 5,000 serialized characters:
  - Rhea: 4,937
  - Sylvi: 4,993
  - Garran: 4,990
  - Veyra: 4,965
  - Pip: 4,988
- Rootjaw's conversation view contains four audible turns beginning at tick `193:10`.
- Rootjaw's scene context is rejected because he is not physically present.
- Historical tick-55 search does not reveal the Tree King through future relationships.
- The Frontier secret marker remains absent from character FTS, vector, and hybrid results.

## Final world statistics

- Records: 294
- Characters: 20
- Locations: 29
- Objects: 28
- Environments: 16
- Events: 44
- Knowledge records: 61
- Directional relationships: 36
- Scenes: 20
- Story points: 22
- Conversations: 17
- Verbatim turns: 224
- Recollections: 76

## Outcome

The application supported the complete requested arc as a coherent, queryable campaign world. The most valuable behavior was not record count; it was the ability to repeatedly ask:

- What does this character actually know now?
- Which exact words are canonical?
- How does this character remember them later?
- What author truth is still hidden?
- Which story obligations remain live?
- Does the next beat follow from accumulated evidence rather than author fiat?
