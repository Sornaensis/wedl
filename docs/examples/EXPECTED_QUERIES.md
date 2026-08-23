# Expected Query Walkthroughs

These are semantic expectations for the current packaged Frontiersmen world,
not byte-for-byte CLI snapshots. Run from the directory that contains the
initialized `frontiersmen` repository and use `--repo frontiersmen`; from
inside that repository, replace it with `--repo .`.

## Rhea at the active scene

```text
wedl context Rhea --repo frontiersmen --scene "The Hunt Begins" --perspective character --query "What escape route can Rhea justify?" --max-characters 4000
```

Expected inclusions:

- Rhea’s concise, practical voice and the immediate Northwood pursuit.
- The party’s flight, Root Host pressure, and the relevant audible exchange.
- Rhea’s accepted and suspected knowledge, including uncertainty around the
  guild’s handling of amber and the masked acolyte’s assistance.
- Citations and a perspective boundary.

Expected exclusions:

- Other characters’ private recollections or author-only notes.
- Claims the party has not yet learned merely because they are true elsewhere
  in the canonical record.

`--query` raises relevance for matching optional material; it does not turn
author information into character knowledge. A context response is hard-limited
by `--max-characters`. The minimum is at least 1,800 characters, and the command
reports a larger exact minimum when a particular packet needs one.

## State and knowledge at tick 195

```text
wedl state "Amber Reliquary" --repo frontiersmen --timeline main --tick 195
wedl knowledge Rhea --repo frontiersmen --timeline main --tick 195
```

The reliquary is closed, contains five shadow-impressions, and is held by
Veyra. Rhea’s knowledge includes accepted and suspected claims separately; in
particular, her belief that Lantern Pike concealed quick-amber rules remains a
suspicion rather than settled fact.

## Interactions, story points, and conversation

```text
wedl interactions Rhea Veyra --repo frontiersmen
wedl story-points --repo frontiersmen --scene "The Hunt Begins"
wedl conversation show "Running Under the Drums" --repo frontiersmen --perspective character --character Rhea --timeline main --tick 195
```

The first command lists canonical events in which Rhea and Veyra both
participate. The active-scene story-point inspection includes active pressures
such as **Flee the Root Host**, **Replace the Dead Watch**, and **Expose the
Blackroot Compact**. Rhea’s character-perspective conversation output contains
only turns audible during her presence interval, plus any applicable subjective
recollection; an author view has the full canonical transcript.

## Perspective-safe retrieval

```text
wedl search "guild concealed amber manifestations" --repo frontiersmen --mode hybrid --perspective character --character Rhea --scene "The Hunt Begins" --timeline main --tick 195
```

Hybrid output identifies the available retrieval lanes and their ranks. The
character filter and fictional-time boundary are applied before ranking, so a
result cannot reveal material inaccessible to Rhea at that moment.
