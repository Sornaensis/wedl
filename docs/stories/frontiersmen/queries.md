# Expected Query Walkthroughs

These walkthroughs distinguish current Southward Cut from historical Hunt reads in the maintained Frontiersmen package,
not byte-for-byte CLI snapshots. Run from the directory that contains the
initialized `frontiersmen` repository and use `--repo frontiersmen`; from
inside that repository, replace it with `--repo .`.

## Historical Rhea at the Hunt (main195:99)

```text
wedl context Rhea --repo frontiersmen --scene "The Hunt Begins" --timeline main --tick 195 --order 99 --perspective character --query "What escape route can Rhea justify?" --max-characters 4000
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

Use `--query` to focus the requested packet. Choose `--max-characters` at least 1,800; the command reports a larger minimum if needed. Historical software design rationale is preserved in ADRAI A01M498R304BB0EEZ27RKTSN6QW. Read it with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M498R304BB0EEZ27RKTSN6QW --json` in the WEDL development/source checkout.

## State and knowledge at tick 195

```text
wedl state "Amber Reliquary" --repo frontiersmen --timeline main --tick 195 --order 99
wedl knowledge Rhea --repo frontiersmen --timeline main --tick 195 --order 99
```

The reliquary is closed, contains five shadow-impressions, and is held by
Veyra. Rhea’s knowledge includes accepted and suspected claims separately; in
particular, her belief that Lantern Pike concealed quick-amber rules remains a
suspicion rather than settled fact.

## Interactions, story points, and conversation

```text
wedl interactions Rhea Veyra --repo frontiersmen
wedl story-points --repo frontiersmen --scene "The Hunt Begins" --timeline main --tick 195 --order 99
wedl conversation show "Running Under the Drums" --repo frontiersmen --perspective character --character Rhea --timeline main --tick 195 --order 99
```

The first command lists canonical events in which Rhea and Veyra both
participate. The historical main195:99 story-point inspection includes pressures
such as **Flee the Root Host**, **Replace the Dead Watch**, and **Expose the
Blackroot Compact**. Rhea’s character-perspective conversation output contains
only turns audible during her presence interval, plus any applicable subjective
recollection; an author view has the full canonical transcript.

## Perspective-safe retrieval

```text
wedl search "guild concealed amber manifestations" --repo frontiersmen --mode hybrid --perspective character --character Rhea --scene "The Hunt Begins" --timeline main --tick 195 --order 99
```

Inspect the returned lanes, ranks and citations for this historical query. The authorization/ranking contract is read through ADRAI A01M48RW32X6AEHK2376V4YH3KP; the original explanation is in ADRAI A01M498R304BB0EEZ27RKTSN6QW.

## Current Southward Cut (main210:0)

```text
wedl context Rhea --repo frontiersmen --scene "Southward Cut" --timeline main --tick 210 --order 0 --perspective character --query "What does the counterfoil prove, and what does the road promise?" --max-characters 4000
wedl story-points --repo frontiersmen --scene "Southward Cut" --timeline main --tick 210 --order 0
```

The immediate pursuit is resolved. Rhea carries the narrow counterfoil, the road
is unconfirmed, and Moth/Jorund remain unresolved; the old Flee the Root Host
pressure above belongs to the historical Hunt. The current package has 309 records,
19 conversations and 248 turns. See the [world guide](world-guide.md).
