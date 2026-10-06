+++
schema = "adrai/decision/v1"
adr = "A01M48NSD0Q3SVF3RRXVR520H9G"
record = "R01M48NSE20KVVH8NHZ794W57YB"
title = "Preserve authored object affordances in v0.7"
summary = "Preserve legacy authored object capabilities exactly as optional v0.7 object_affordances through the explicit upgrade-v07 migration."
domains = ["source-migration"]
+++

# ADR 0006: Preserve authored object affordances in v0.7

- **Status:** Accepted by the project owner on 2026-09-25
- **Scope:** v0.7 source validation and the explicit `upgrade-v07` migration
- **Example:** [object-affordances-v07.yaml](../../examples/object-affordances-v07.yaml)
- **Related decisions:** [ADR 0004](../../../../docs/ADR.md#adr-0004) and [ADR 0005](../../../../docs/ADR.md#adr-0005)

## Context

Legacy object records use `capabilities` for authored, non-world affordances such
as `can-be-opened`. The existing v0.7 upgrade removed that field from every
non-world record because v0.7 reserves `capabilities` for the world component
envelope. That erased authored object meaning. The three legacy packages contain
53 such fields across Ash Archive and Frontiersmen: 25 explicit empty lists and
28 nonempty lists containing 19 distinct tokens. An absent field has a different
source shape from an explicit empty list and must stay absent.

## Decision

An object record in `wedl/v0.7` may have an optional `object_affordances` array of
strings. Array order, duplicates, and token spelling are authored data; the
validator does not sort, deduplicate, or interpret them. The field is invalid
on every other record kind. On v0.7 records, `capabilities` remains a closed,
ordered, world-only component declaration. An object cannot mix both fields.
This additive field does not change the `wedl/v0.7` version or world capability
registry and grants no spatial, generational, or runtime operation by itself.

An explicit `upgrade-v07` maps each legacy object's `capabilities` value to
`object_affordances` exactly. It preserves an absent field as absent and `[]`
as `[]`. It rejects non-string-list legacy values, mixed legacy fields, and
non-object placement before writing. Other authored fields, Markdown bodies,
chronology, IDs, and paths retain their established migration behavior. A
homogeneous v0.7 source remains a validated no-op. Preview, apply, replay,
backup, and rollback retain the existing migration protocol. No legacy schema
validator or package is changed; no source is upgraded at runtime.

## Consequences

Source and compiled detail can expose the same authored object affordances.
Consumers may display the strings but cannot treat them as executable powers.
The migration can now prove semantic preservation without excluding every
`capabilities` field from its before/after comparison. The world component
envelope remains unchanged.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleCIsImsiOiJsbG0iLCJtIjoiZ3B0LTYuMS1zb2wifSwiYiI6IjcxMmMwNzIxODYzMTUzZDdiZWQ2Mjc2NzU1NWY5M2YxNzhkOWI0ZjkiLCJpIjoic2hhMjU2OjJqbGpLejh0RXB3a1ZPaE1oZWFLYWZQeWViNHpaV0s1NktpbmRNVHRDYjQiLCJrIjoiZGVjaXNpb24uY3JlYXRlIiwibyI6IlIwMU00OE5TRTIwS1ZWSDhOSFo3OTRXNTdZQiIsIm9wIjoiTzAxTTQ4TlNFMjBLVlZIOE5IWjc5NFc1N1lCIiwiciI6Im1hc3RlciIsInMiOiJzaGEyNTY6Y2NSWGhjZDNLcVVvUmstZHUyaDZ3cE9Va3QwZ3NEUmo1S0NvV2RhcUxMTSIsInQiOjE3OTEyOTI2NTE1ODQsInYiOjEsIngiOiJhZHJhaS8xLjAuMCJ9 -->
