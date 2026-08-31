# Migration and recovery

`wedl migrate` is a deliberately narrow, local-only maintenance command. It
has no HTTP endpoint and no browser workflow. Markdown and Git remain the
authoritative source; SQLite is rebuilt after a successful write and is never
migrated in place.

`upgrade-v07` is the active coordinated local-only route for a clean,
homogeneous v0.3, v0.5, or v0.6 source tree. It rewrites only schema markers
and adds the world-only ordered capability envelope
`[generational-core-v1, spatial-core-v1]`; Markdown bodies, record IDs,
paths, frontmatter semantics, provenance, threads, chronology, and authored
spatial facts are retained exactly. A valid homogeneous v0.7 tree previews as
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
WEDL refuses a new write rather than guessing whether the owner is stale.
Inspect the recorded PID, confirm that process is no longer running, then
remove only that WEDL-owned lock and repeat `preview`. Never remove Git's
`.git/index.lock`: it belongs to the Git process that created it and must be
resolved through that process.
