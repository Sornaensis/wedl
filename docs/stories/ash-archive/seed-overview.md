# Expanded Example World — The Ash Archive

> Historical tick121 seed material. Maintained packaged Ash is completed at tick208; this page preserves the earlier narrative/IDs, not a current active-world inventory.
This guide documents the historical tick-121 Ash Archive seed. The directory contains documentation, not a story source tree. Maintained completed source is in [ash_archive/story](../../../src/wedl/data/ash_archive/story) and [ash_archive_v07/story](../../../src/wedl/data/ash_archive_v07/story). Initialize those packages with `wedl init ash --example ash-archive` or `wedl init ash-v07 --example ash-archive-v07`.

Historical software design rationale is preserved in ADRAI A01M498R304BB0EEZ27RKTSN6QW. Read it with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M498R304BB0EEZ27RKTSN6QW --json` in the WEDL development/source checkout.

## Historical seed corpus

- **Characters:** 8
- **Locations:** 11
- **Objects:** 10
- **Environments:** 7
- **Events:** 15
- **Knowledge Records:** 34
- **Relationships:** 18
- **Scenes:** 6
- **Story Points:** 10

## Seed story state at tick 121

At tick 121, Oren has just transferred the sealed heron letter to Mara in the active scene [After the Exchange](story:scene_0FVWCX929V1WYY31NEP6Q1B0BX). Ilyra is missing, the Restricted Vault is watch-sealed, Nessa holds Catalog Token 7B, Rusk holds the ash-glass map tile, and several story points are eligible or blocked by explicit dependencies.

Mara may know that Oren deliberately gave her the letter, that Caldrin requested a restricted ledger, that Ilyra disappeared, and that the erased-wing heron marked an old courier network. She does **not** know the letter’s contents, Sable’s role in the handoff, Ilyra’s hidden location, Edrin’s stronger suspicion, or Caldrin’s private objective.

## What the fixture demonstrates

- Historical canonical events before one active scene.
- Closed, active, and planned scenes.
- Explicit object and character state effects.
- More than one character holding different beliefs about the same claims.
- Directional, asymmetric relationship histories.
- Shared and private scene observations.
- Resolved, active, eligible, dormant, and dependency-blocked story machinery.
- Author-only secret markers for perspective-leak tests.
- A future multi-record outcome suitable for change-set tests.

See [seed story guide](seed-story-guide.md), [seed changeset](seed-changeset.md), and [seed manifest](seed-manifest.md). The separate [Frontiersmen queries](../frontiersmen/queries.md) use another world.

## Separate conformance asset

The [spatial conformance asset](../../../architecture/adrai/examples/spatial-source-component-v07.yaml) lives outside the story guide directory. Read current spatial architecture through ADRAI A01M48ZRCS8CP9EC5H5YH2MRVQ4.

The seed lists 119 non-world IDs, plus its world record (120 total). The maintained completed package contains 262 records, 16 conversations, 138 turns and 58 recollections, with a closed epilogue at tick208 and no active scene. See the [completed walkthrough](walkthrough.md); the [238-record unresolved walkthrough](historical-unresolved-walkthrough.md) preserves the intermediate tick178 snapshot.
