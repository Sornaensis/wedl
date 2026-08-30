# Expanded Example World — The Ash Archive

This directory contains a coherent, deliberately nontrivial Ash Archive source
tree. It is documentation and an executable fixture for the current parser,
compiler, and query behavior.

For human readability, fixture paths use short stable slugs. Identity comes
exclusively from frontmatter IDs, so renaming a fixture file does not alter its
semantics. The current CLI accepts IDs, titles, aliases, and slugs as entity
references; it does not impose a generated `<slug>--<id>.md` filename format.

## Corpus size

- **Characters:** 8
- **Locations:** 11
- **Objects:** 10
- **Environments:** 7
- **Events:** 15
- **Knowledge Records:** 34
- **Relationships:** 18
- **Scenes:** 6
- **Story Points:** 10

## Current story state

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

See [`STORY_GUIDE.md`](STORY_GUIDE.md), [`EXPECTED_QUERIES.md`](EXPECTED_QUERIES.md), [`CHANGESET_EXAMPLE.md`](CHANGESET_EXAMPLE.md), and [`MANIFEST.md`](MANIFEST.md).

## Latent spatial component fixture

`spatial-source-component-v07.yaml` is a source-validator fixture for the
accepted spatial contract.  It is not a generic runtime-accepted source world.
