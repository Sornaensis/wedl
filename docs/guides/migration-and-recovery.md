# Migration and recovery usage

Migrate an existing world locally with `wedl migrate`. There is no HTTP
migration or browser upgrade workflow. Use a clean managed tree and record the
exact Git HEAD before previewing. The normal upgrade-v07 inputs are homogeneous
v0.3, v0.5 or v0.6 source; a valid v0.7 input previews as a no-op. For staged
legacy work, inspect `wedl migrate preview --help` for upgrade-v03/upgrade-v06.

## Preview and confirm

Replace the placeholders with your audited HEAD and fresh idempotency key:

```bash
wedl migrate preview --repo . --mode upgrade-v07 --expected-head <HEAD> --idempotency-key upgrade-KEY
```

Review `diff` and diagnostics, and retain the reported `sourceSnapshotHash`,
`backupRef`, `requestHash`, and `confirmationToken`. Preview makes no source or
cache changes. Apply only the exact reviewed preview:

```bash
wedl migrate apply --repo . --mode upgrade-v07 --expected-head <HEAD>   --source-snapshot-hash <SHA256> --idempotency-key upgrade-KEY   --confirm <TOKEN>
```

Copy the hash and token verbatim. Upgrade-v07 also renames legacy object
`capabilities` to `object_affordances`; expect that field spelling in the diff.
It declares the two core capabilities and does not opt into optional typed
generational knowledge. A mixed-version input must be resolved before applying.

## Start from a packaged example

```text
wedl init ash-world --example ash-archive-v07
wedl init frontier-world --example frontiersmen-v07
```

The original ash-archive/frontiersmen choices remain v0.3; choose the -v07
variant explicitly. Init does not upgrade an existing repository. From the
WEDL source checkout, check packaged copies with:

```text
python tools/build_v07_packaged_examples.py --check
```

## Forward rollback

Use the literal backup ref returned by preview, not a ref constructed from a
request hash. Preview rollback against the current HEAD:

```bash
wedl migrate preview --repo . --mode rollback --expected-head <HEAD>   --rollback-backup-ref <BACKUP_REF_FROM_PREVIEW>   --idempotency-key rollback-KEY
```

Review that result, then apply it with its own source hash and token:

```bash
wedl migrate apply --repo . --mode rollback --expected-head <HEAD>   --rollback-backup-ref <BACKUP_REF_FROM_PREVIEW>   --source-snapshot-hash <SHA256> --idempotency-key rollback-KEY --confirm <TOKEN>
```

This is a new forward commit; retain both backup refs. For ordinary restored
source rebuild the disposable cache with `wedl compile --repo .`. A rollback
to raw v0.4 preserves quarantine and skips cache compilation; it does not make
ordinary v0.4 reads available.

## Quarantined v0.4 and interrupted writes

Use recover-v04 only after its preview reports a successful sole-primary audit.
Mapping-required diagnostics need diagnosis before a write; normal v0.4
load/validate/compile/query/server paths remain quarantined. Read the source
checkout's thread/recovery decision A01M48RX5WAQT5ECH66KTCFVC0T for audit bounds.

If `.git/wedl-canonical-write.lock` blocks a write, preserve the reported error,
lock, unfinished transaction journal and candidate evidence. Inspect ownership
and whether the recorded process is still active. PID death alone does not
authorize deleting a journal-bound lock; normal changeset writes can reclaim
only an authenticated owned stale lock. There is no separate recovery command.
For an ordinary non-journal lock, establish its owner and absence of an active
write before manual intervention; leave uncertain ownership intact for diagnosis.
Never remove `.git/index.lock`; resolve it through its owning Git process.
Repeat preview after the owned contention is resolved.

Read the migration/recovery contract in the WEDL development/source checkout:
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M494PZVEJB05Y3RDNHAAKMWD --json`.
