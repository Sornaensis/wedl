+++
schema = "adrai/decision/v1"
adr = "A01M48RX5WAQT5ECH66KTCFVC0T"
record = "R01M48RX627DECN1KJC0G1GEKFS"
title = "Shared-world threads and quarantined v0.4 recovery"
summary = "Migrated THREAD_SCHEMA_CONTRACT contract and provenance; architecture is maintained through ADRAI."
domains = ["shared-world"]
+++

## Authority and provenance

Migrated from `docs/THREAD_SCHEMA_CONTRACT.md` at Git revision `996f5d18b4d982fd67c777ff833c667d80e83e29`. This relocation records existing documentation; it does not create a new historical approval. Named approvals and separately owned domain decisions retain their authority.

The staged status and v0.3/v0.5 validation wording below describes the historical thread delivery slice, not the complete current source-version roster. v0.6 and v0.7 now compose the same shared-world grouping and recovery invariants; ordinary current loading/validation/compilation supports those versions through their separate domain contracts. Local `upgrade-v06` is active, with no HTTP or browser migration route; tick gap never implies duration. This record keeps the exact historical contract and diagnostic families without treating old vector delivery status as a current release limitation.

# Shared-world thread schema and migration contract

**Status:** Normative staged implementation; v0.5 source validation and the local-only migration/recovery kernel are available.

This contract implements the data boundary approved by
[ADR 0002] (read with `adrai --repo . show A01M48NHJ5Z6AX617K56WRVFYWT --json`). It
reserves `wedl/v0.5`; the approved source/model/validation slices now enforce
this contract. Query, search, API, and browser delivery remain separately
gated from the local migration/recovery kernel.

## 1. One global world model

A homogeneous v0.5 source retains v0.3 records and `StoryTime`. Its world has
exactly one `timelines` item, and `default_timeline` must equal that item.
`current_time` remains optional and, when present, is the one global cursor.
There is no per-thread clock, calendar interpretation, cursor, state, or
canon. Characters, locations, mutable world state, causes, and the meaning of
ticks/calendar remain shared across every thread.

The global ordering governs ordinary causality: a cause must be strictly
earlier than its effect in that one global `StoryTime`. Thread grouping neither
authorizes a causal edge nor changes state or cursor semantics.

## 2. Reserved grouping membership

The world adds the canonical list:

```yaml
threads:
  - id: thread_<26 Crockford Base32 characters>
    label: <nonempty string>
```

`threads` is sorted by `id`. Each item has exactly `id` and `label`; it has no
extra fields. This contract reserves no thread hierarchy, status, or lifecycle.

Every non-world, non-hypothesis record may omit or carry `threads`, a sorted,
unique list of declared thread IDs. An omitted membership normalizes to `[]`.
Membership is zero-to-many grouping only: a record may belong to no thread,
one thread, or many threads without duplicating the record or changing its
canon, time, state, visibility, causality, cursor, or search corpus.
Hypotheses reject thread membership.

The following are prohibited on a thread or as thread-scoped behavior:
`parent`, `status`, `canon`, `time`, `horizon`, `presentation`, `search`,
`model`, and `cursor`. The source also rejects per-thread `timeline`, `domain`,
`fork`, `continuity`, `strand`, `sync`, `retcon`, `projection`, and
`author-all` fields. No omitted membership, title, path, tag, scene, character,
location, causal link, or timestamp may infer a thread.

## 3. v0.3 to v0.5 reserved migration

The reserved migration begins with a dry run against an exact expected Git
HEAD and a clean working tree. It audits a homogeneous v0.3 source and records
its source hash before any write. Automatic conversion is permitted only when
the world has exactly one timeline and `default_timeline` equals it.

For that narrow case, the atomic, confirmed write creates a backup, changes
every source record's schema marker to `wedl/v0.5`, adds `threads: []` on the
world, and leaves every record membership omitted. It does not infer or create
threads. A multi-timeline source blocks automatic conversion. The operation is
idempotent: an already converted homogeneous v0.5 source reports that state;
mixed source schemas are rejected. Disposable caches rebuild from the changed
source fingerprint after a successful conversion.

## 4. Quarantined v0.4 recovery

`wedl/v0.4`, `wedl-continuity/v1`, and `WDL-CONT` continuity IDs are withdrawn
and superseded. They must never be silently treated as threads. Normal v0.4
load, validate, compile, query, and upgrade paths stop immediately with the
stable `v04_superseded` diagnostic and a recovery reference; they must not
load topology or reinterpret continuity data.

Recovery starts with a raw-frontmatter audit, not a normal v0.4 topology read.
Automatic recovery is permitted only if the audit finds exactly one root
continuity/domain, that continuity has status `primary`, and
`default_continuity` names it; no parent, fork, retcon, synchronization,
handoff, vector or synchronized horizon, or presentation frame; global
membership only with empty strands; and every coordinate maps to the one
global legacy timeline. A scalar horizon may map to global `current_time`.
An alternate, experimental, retired, or mismatched-default sole continuity
blocks automatic recovery.

Only after those checks, the confirmed recovery uses the exact expected HEAD,
clean tree, dry-run hash, and backup. It atomically changes every source
record's schema marker to homogeneous `wedl/v0.5`, drops the continuity
wrappers, adds world `threads: []`, and leaves record memberships omitted.
The backup supplies recoverable rollback on failure; partial or mixed sources
are never left behind. Otherwise recovery blocks and requires an
author-supplied mapping.

No v0.4 recovery may infer threads, change causal ordering, create an
alternate canon, preserve a per-thread corpus, or use archived vectors as a
schema. Caches rebuild after successful recovery.

## 5. Local migration and recovery command

`wedl migrate preview|apply` is local-only; there is no HTTP or browser route.
Its request protocol is `wedl-migration/v1`. A preview names an exact Git
`expectedHead`, explicit mode (`upgrade-v03`, `recover-v04`, or `rollback`),
and stable `idempotencyKey`, then returns the exact source snapshot hash. Apply
requires that preview hash as well as the exact confirmed preview. A preview is read-only: it
does not create a source or compiled cache and returns the versioned diff,
diagnostics, request hash, deterministic backup ref, and confirmation token.
Apply accepts only that exact confirmed preview.

The recovery audit accepts only this narrow v0.4 shape: one world; one sole
`primary` continuity with matching `default_continuity`; one matching legacy
timeline and default; empty global `strands`; and an optional integer world
`horizon`. It blocks every parent, fork, retcon, synchronization, handoff,
vector, synchronized-horizon, and presentation field, and blocks every
coordinate on another timeline. The scalar horizon becomes global
`current_time`; wrappers are dropped; no thread is inferred.

Before a write, a compare-and-swap backup is recorded below
`refs/wedl/backups/migration/`. The write is a normal forward Git commit and
then rebuilds the disposable compiled cache. A rollback restoring a quarantined
v0.4 backup restores only its exact raw source bytes through this explicit path
and deliberately does not compile it; ordinary v0.4 load/validation/query
quarantine remains intact. Rollback also creates a forward
commit from a named backup ref; it never resets, rewrites Git history, or
deletes the backup evidence. See [Migration and recovery](../../../../docs/MIGRATION_AND_RECOVERY.md).

## 6. Validation diagnostics

Validation accepts only homogeneous `wedl/v0.3` or `wedl/v0.5` record sets.
An exact `wedl/v0.4` source marker raises `v04_superseded` before topology is
read, with recovery guidance at
this ADRAI record, section 4, Quarantined v0.4 recovery.

| Code | Field | Message |
| --- | --- | --- |
| `WDL-SRC-008` | `schema` | source records must use one homogeneous schema |
| `WDL-SRC-009` | offending member path | member is incompatible with this source schema |
| `WDL-TIMELINE-012` | `timelines` | wedl/v0.5 requires exactly one timeline declaration |
| `WDL-THREAD-001` | `threads` | wedl/v0.5 world threads must be an array |
| `WDL-THREAD-002` | `threads[i]` | thread declaration must be exactly an id and label mapping |
| `WDL-THREAD-003` | `threads[i].id` | thread id must use the thread_&lt;26 Crockford&gt; format |
| `WDL-THREAD-004` | `threads[i].id` | thread declaration id is duplicated |
| `WDL-THREAD-005` | `threads[i].label` | thread label must be a non-empty string |
| `WDL-THREAD-006` | `threads` | thread declarations must be sorted by id |
| `WDL-THREAD-007` | `threads` | record thread membership must be an array of thread ids |
| `WDL-THREAD-008` | `threads` | record thread memberships must be unique and sorted by id |
| `WDL-THREAD-009` | `threads[i]` | record thread membership is not declared by the world |

For deterministic output, thread declaration diagnostics precede record
membership diagnostics. Record memberships are then checked by source path
casefold and record ID, followed by membership index. Thread membership is
grouping only and does not alter global StoryTime, causality, state, cursor,
or concurrent-scene semantics.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiY2NkZjBjYzdiNjllMWQzMjFkNDY3MjAyZTlmNGZmMDBlNGZlZjdiOCIsImkiOiJzaGEyNTY6N1VPNXgtckN6OFFGUkc4U2dDQVFrM0RKbVpmWnY3WUptU1pPaGRmSWdCSSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4Ulg2MjdERUNOMUtKQzBHMUdFS0ZTIiwib3AiOiJPMDFNNDhSWDYyN0RFQ04xS0pDMEcxR0VLRlMiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjozeUdHNkRGYkxtcEhFc2xfd2Fna21CQVk2ZzJMVFpaOXU4Z1l1bFNubnpFIiwidCI6MTc5MTI5NTkyMDE5OSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
