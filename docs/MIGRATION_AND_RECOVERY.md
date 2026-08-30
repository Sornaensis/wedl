# Migration and recovery

`wedl migrate` is a deliberately narrow, local-only maintenance command. It
has no HTTP endpoint and no browser workflow. Markdown and Git remain the
authoritative source; SQLite is rebuilt after a successful write and is never
migrated in place.

`wedl/v0.6` source loading, validation, compilation, internal typed reads,
public chronology CLI/HTTP reads, and confirmed complete-replacement authoring
are active. `upgrade-v06` preview/apply is the active local-only route; the UI
reports capability but never migrates source. Its version policy, recovery
rules, and runtime coupling inventory are in the
[Chronology migration contract](CHRONOLOGY_MIGRATION_CONTRACT.md).

## Preview first

Run a dry run against the exact current commit in a clean managed tree:

```bash
wedl migrate preview --repo . --mode upgrade-v06 --expected-head <HEAD> --idempotency-key upgrade-2026-08
```

The `wedl-migration/v1` response includes `sourceSnapshotHash`, `backupRef`, a
unified `diff`, diagnostics, `requestHash`, and `confirmationToken`. Preview
does not write source files, Git refs, receipts, parsed-source cache, or
compiled SQLite cache.

Apply only the exact preview. Copy its source hash and token verbatim:

```bash
wedl migrate apply --repo . --mode upgrade-v03 --expected-head <HEAD> \
  --source-snapshot-hash <SHA256> --idempotency-key upgrade-2026-08 \
  --confirm <TOKEN>
```

`upgrade-v03` accepts only a homogeneous v0.3 world with exactly one timeline
and matching default. It changes every schema marker to v0.5, writes an empty
world `threads` declaration, and omits all record memberships. It does not
infer groups or change StoryTime.

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
