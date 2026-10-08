+++
schema = "adrai/decision/v1"
adr = "A01M48NSD0Q3SVF3RRXVR520H9G"
record = "R01M4DMX8MY14JQBNWGTJ5H7T4W"
title = "Preserve authored object affordances in v0.7"
summary = "Preserve legacy authored object capabilities exactly as optional v0.7 object_affordances through the explicit upgrade-v07 migration."
domains = ["source-migration"]
+++

# ADR 0006: Preserve authored object affordances in v0.7

- **Status:** Accepted by the project owner on 2026-09-25
- **Scope:** v0.7 source validation and the explicit `upgrade-v07` migration
- **Example:** [object-affordances-v07.yaml](../../../../docs/examples/object-affordances-v07.yaml)
- **Related decisions:** [ADR 0004] (read with `adrai --repo . show A01M48NPY19KENPF8EWB8A4W95W --json`) and [ADR 0005] (read with `adrai --repo . show A01M48NQZ50KH9V094Q5A3138R0 --json`)

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

<!-- @adrai:eyJhIjp7ImkiOiJhcmNoaXZlLWV4YW1wbGVzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNDJmZmNhZTY5NjA0MWM0NzVkMjkyYjZkODE0OWYxNDQzOTZmM2Q1YiIsImkiOiJzaGEyNTY6TnZEdkcxZG1oeHdzUVIxVVc1NkNzR0JTN2EtbDczNUZSdGJ3YnhrX3ZpZyIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNERNWDhNWTE0SlFCTldHVEo1SDdUNFciLCJvcCI6Ik8wMU00RE1YOE1ZMTRKUUJOV0dUSjVIN1Q0VyIsInAiOlsiUjAxTTQ4U0NORDVRRjNYM1FYUzk5UE1LVDhIIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2Ok84OWVpZjM1U2ZuV3N5dG8wdTBzNVQxa04tVTBwaXlmanlSdGxOczFES1EiLCJ0IjoxNzkxNDU5NTAwNzAyLCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
