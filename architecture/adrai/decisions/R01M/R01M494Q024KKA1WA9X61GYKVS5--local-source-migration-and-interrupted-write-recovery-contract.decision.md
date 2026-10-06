+++
schema = "adrai/decision/v1"
adr = "A01M494PZVEJB05Y3RDNHAAKMWD"
record = "R01M494Q024KKA1WA9X61GYKVS5"
title = "Local source migration and interrupted-write recovery contract"
summary = "Preserve the implemented operational contract and correct documented interface limits; practical usage remains in guides."
domains = ["recovery", "source"]
+++

# Local source migration and interrupted-write recovery contract

`wedl migrate` is a deliberately narrow, local-only maintenance command. It
has no HTTP endpoint and no browser workflow. Markdown and Git remain the
authoritative source; SQLite is rebuilt after a successful write and is never
migrated in place.

`upgrade-v07` is the active coordinated local-only route for a clean,
homogeneous v0.3, v0.5, or v0.6 source tree. It rewrites schema markers, maps legacy object `capabilities`
to `object_affordances`, and adds the world-only ordered capability envelope
`[generational-core-v1, spatial-core-v1]`; Markdown bodies, record IDs,
paths, frontmatter semantics, provenance, threads, chronology, and authored
spatial facts are preserved. Serializer field spellings are distinguished from
semantic preservation; the new component envelope does not infer new facts
or opt into `generational-knowledge-v1`. A valid homogeneous v0.7 tree previews as
an exact-capability no-op. The UI reports capabilities but never migrates
source.

## Preview first

Run a dry run against the exact current commit in a clean managed tree:

```bash
wedl migrate preview --repo . --mode upgrade-v07 --expected-head <HEAD> --idempotency-key upgrade-2026-08
```

The `wedl-migration/v1` response includes `sourceSnapshotHash`, `backupRef`, a
unified `diff`, diagnostics, `requestHash`, and `confirmationToken`. Preview
does not write source files, Git refs, receipts, parsed-source cache, or
compiled SQLite cache.

Apply only the exact preview. Copy its source hash and token verbatim:

```bash
wedl migrate apply --repo . --mode upgrade-v07 --expected-head <HEAD> \
  --source-snapshot-hash <SHA256> --idempotency-key upgrade-2026-08 \
  --confirm <TOKEN>
```

`upgrade-v07` rejects mixed source versions with `GEN-VERSION-001`, validates
the complete candidate before it creates a backup or writes, and derives its
target capabilities on the server. It never infers geometry, routes, overlays,
lineage, reverse edges, adjacency, or time conversion. The existing
`upgrade-v03` and `upgrade-v06` modes retain their documented compatibility
behavior for staged legacy transitions.

## Packaged examples

The original `ash-archive` and `frontiersmen` init choices stay pinned to
`wedl/v0.3`. Select `ash-archive-v07` or `frontiersmen-v07` explicitly to start
from separately packaged v0.7 copies. The `chronology_conformance_v07` copy is
available to conformance tooling; it is not an init choice. No init choice
upgrades an existing repository, and the empty-world default is unchanged.

Run `python tools/build_v07_packaged_examples.py --check` from a source checkout
to verify the checked-in copies. `--write` regenerates only the derived package
trees from their pinned legacy sources. The tool rejects any path or content
drift from those exact sources, and any linked output path, before writing.
Each copy retains record paths, IDs, Markdown bodies, and authored facts; the
v0.7 serializer spells legacy location `parent` as
`parent_id`. Legacy object `capabilities` map exactly to v0.7
`object_affordances`, preserving absent fields, explicit empty arrays, token
order, and duplicates. Only the world declares component `capabilities`. The
copied worlds declare `[generational-core-v1, spatial-core-v1]` and contain no
new spatial or generational facts.

For an existing repository, use the preview and confirmation commands above
on a clean managed tree. Keep the reported backup ref. To restore the pinned
source, use the forward rollback commands below, then rebuild the disposable
SQLite cache with `wedl compile --repo .`. Source compatibility is decided by
the versioned validator; a compiled cache never changes the Markdown version.

## Quarantined v0.4 recovery

Normal v0.4 load, validation, compile, query, and server paths remain
quarantined. Use `recover-v04` only after preview confirms the narrow sole
primary-continuity audit described in the thread schema contract. The audit
reads raw Markdown envelopes from the immutable Git snapshot; it does not use
the normal v0.4 parser or archived vectors. Anything ambiguous produces a
stable mapping-required diagnostic and writes nothing.

## Forward rollback

Every applied migration creates a compare-and-swap backup ref below
`refs/wedl/backups/migration/`. To restore one, create another confirmed
forward commit:

```bash
wedl migrate preview --repo . --mode rollback --expected-head <HEAD> \
  --rollback-backup-ref <BACKUP_REF_FROM_PREVIEW> \
  --idempotency-key rollback-2026-08
```

Apply the returned preview in the same way. The confirmation binds both the
backup ref's resolved Git object ID and its source snapshot hash; moving that
ref after preview rejects the apply before it writes. A rollback of a
`recover-v04` backup restores its raw Markdown bytes only through this explicit
path, skips cache compilation, and leaves ordinary v0.4 load/validation/query
quarantine intact. Rollback never uses `reset`, never rewrites a branch, and
retains both the original and rollback backup refs.

## Interrupted local writes

WEDL serializes its own real-index publication with
`.git/wedl-canonical-write.lock`. If an interrupted process leaves that file,
Ordinary locks are not stolen. One automatic reclaim is permitted only when
`TransactionJournal.reclaim_owned_stale_lock` authenticates a dead owner and an
unfinished journal against its recorded canonical-lock identity and digest.
It checks a regular lock file without symlink or reparse traversal, bounded content and a stable
hard-link claim, and rechecks identity before unlink; failures leave contention
intact. PID death alone is not deletion authority for a journal-bound lock.
Normal changeset write paths invoke transaction recovery; there is no separate
recovery CLI command. Preserve the reported lock, journal and candidate
evidence and investigate ownership before any manual intervention. For an
ordinary non-journal lock, confirm its recorded owner and that no write is
active; uncertainty requires diagnosis rather than deletion. Repeat `preview`
only after that owned contention is resolved. Never remove Git's
`.git/index.lock`: it belongs to the Git process that created it and must be
resolved through that process.

## Transaction boundaries and implementation evidence

The canonical-write lock and real-index publication remain serialized. The
transaction journal owns only authenticated transaction artifacts; recovery
must not infer ownership from a dead PID or steal Git process locks. Current
reclaim admission is in `repository.py:632–637`; validation, hard-link claim
and immediate pre-unlink checks are in `transaction_recovery.py:2421` onward.
`changeset.py` write paths call recovery, not a standalone user verb. Existing
source/threads decisions A01M48S0B8N1M7HRABA7YSM5ZR7 and
A01M48RX5WAQT5ECH66KTCFVC0T remain authoritative for source fidelity and
quarantined v0.4 audit; chronology migration A01M48XY4R2CJ698Z2VQM9SG4ZZ
retains the mode-specific v0.6 oracle and rollback contract.


## Provenance and authority

Transferred from `docs/MIGRATION_AND_RECOVERY.md` at source revision `1f724998bb35bb523fee70da9b300860c4c7f16d` during documentation maintenance. This record preserves the implemented contract and its unique rationale; it does not invent a new historical approval. Existing domain decisions retain their authority. Current interface limitations are distinguished from approved canonical source semantics. Practical invocation is in the corresponding [usage guides](../../../../docs/guides/).

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiOWEzYTMzNjNjOGU3MTg2MTE4ZmU4MWI2M2NhYjllYzM0OGJlYzE0ZCIsImkiOiJzaGEyNTY6bUFZQllieF9qRXIxd0NFWjAyc0VVNnNfRWFiZlUycVljODdpN3Bfa0hGWSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5NFEwMjRLS0ExV0E5WDYxR1lLVlM1Iiwib3AiOiJPMDFNNDk0UTAyNEtLQTFXQTlYNjFHWUtWUzUiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1Njp2WjFTX1UzTUpyR3JMVUcyS0tPSTVoc2piX0c1dW9oMUVXcHZkamVSZ0lFIiwidCI6MTc5MTMwODMwMDM1NiwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
