+++
schema = "adrai/decision/v1"
adr = "A01M48XY4R2CJ698Z2VQM9SG4ZZ"
record = "R01M4DN8GJ14ME8CDBFH7JQ9DCK"
title = "Local chronology upgrade-v06 source transaction"
summary = "Migrated CHRONOLOGY_MIGRATION_CONTRACT.md; current implementation boundaries and original contract semantics."
domains = ["chronology"]
+++

## Authority and provenance

Migrated from `docs/CHRONOLOGY_MIGRATION_CONTRACT.md` at Git revision `daf78b54ebfa40a7e9757bb7cdb1179db779b7b3`. This records the existing contract and corrects current implementation descriptions; it does not create a historical approval or replace the authority of the original calendar chronology decision A01M48NNH43QCFRPC68NQQTFT63. Earlier approval-stage wording and immutable measured evidence retain their historical meaning.

# Chronology migration contract (`wedl-migration/v1`)

This contract specifies the local source transaction for moving a
homogeneous repository to `wedl/v0.6`. v0.6 loading, validation, compilation,
and internal indexed reads plus public chronology reads and confirmed complete
replacement authoring are active. `upgrade-v06` is available only through the
local confirmed migration command; it does not create an HTTP migration route.

The release gate requires the v0.6 validator **and** compiler; both validate
the candidate before the migration creates a backup ref or source commit.

The validator and compiler prerequisite is satisfied in this release. Both are
present before `upgrade-v06` is offered, so the command is active rather than
a future workflow.

Markdown and Git are canonical. Compiled SQLite is disposable output: it is
never migrated, copied, preserved, or rolled back. A successful source commit
invalidates it; the first compatible compiler invocation rebuilds it from the
new source snapshot.

## Version and capability policy for `upgrade-v06`

The selected strategy is **bounded multi-version read with explicit forward
normalization**:

| source schema | `upgrade-v06` input policy | chronology authoring policy | `upgrade-v06` result |
|---|---|---|---|
| `wedl/v0.3` | pinned legacy reader/schema | chronology writes fail with `WDL-MIG-V06-004 upgrade_required` | safe one-shot upgrade when its one-timeline precondition holds |
| `wedl/v0.5` | pinned legacy reader/schema | chronology writes fail with `WDL-MIG-V06-004 upgrade_required` | safe one-shot upgrade preserving grouping data |
| lower/retired | reject `WDL-MIG-V06-007 source_schema_too_old` | unavailable | reject |
| `wedl/v0.4` | quarantined; never normally read, validate, compile, or upgrade | unavailable | reject `WDL-MIG-V06-003 recover_v04_to_v05_first` |
| `wedl/v0.6` | loaded, validated, compiled, internally and publicly readable | confirmed complete replacement | deterministic no-op |
| greater/unknown | reject `WDL-MIG-V06-006 source_schema_too_new` | unavailable | reject |

`wedl/v0.6` is accepted by its loader/validator/compiler and public chronology
read and confirmed complete-replacement authoring surfaces. The local upgrade
is active; the UI consumes capability but never performs migration.
Older loaded worlds retain their pinned behavior. Asking an authoring command
to create or edit `chronology` while the loaded schema is v0.3 or v0.5 must
produce the upgrade-required diagnostic before creating a changeset or writing
a file. Reading, validation, and compilation never rewrite canonical source.

In all source versions, ordinary record grouping membership is persisted only
as `threads`, a list of declared thread IDs. World `threads` is the declaration
list; non-world `threads` is the membership list. `threadIds` is never a
persisted source field, is never accepted as an alias during upgrade, and must
not appear in an upgraded v0.6 envelope. This preserves one shared world:
groups neither create a second state nor change StoryTime.

The `upgrade-v06` support window is v0.3, v0.5, and v0.6. Lower retired
schemas require an earlier documented upgrade route and are never guessed by
this command; v0.4 is a permanent recovery-only exception; newer schemas
are outside this mode. Generic v0.7 loading, validation, and compilation are active; this local mode still rejects v0.7 as `source_schema_too_new`, so its v0.7 negative vector remains applicable. Mixed schema sets are always rejected rather than
normalized record by record.

## Active command and input envelope

The local-only command is `wedl migrate preview|apply --mode upgrade-v06` and
uses protocol `wedl-migration/v1`. There is no HTTP endpoint or browser
workflow. A request contains these exact identity fields:

```json
{
  "protocol": "wedl-migration/v1",
  "mode": "upgrade-v06",
  "expectedHead": "<40-or-64-hex-git-oid>",
  "idempotencyKey": "<caller-stable-key>"
}
```

Preview requires that `expectedHead` equals the checked-out HEAD and that the
managed source tree and managed index are clean. Unrelated files and staged
changes are not absorbed, cleaned, staged, or committed. Preview snapshots
the exact managed blobs at that HEAD and deterministically returns this
existing `wedl-migration/v1` response shape:

```json
{
  "protocol": "wedl-migration/v1",
  "phase": "preview",
  "mode": "upgrade-v06",
  "expectedHead": "<oid>",
  "sourceSnapshotHash": "<64 lowercase hex>",
  "idempotencyKey": "<caller-stable-key>",
  "requestHash": "<64 lowercase hex>",
  "confirmationToken": "<bound-preview-token>",
  "backupRef": "refs/wedl/backups/migration/<base-request-hash>",
  "files": ["story/world.md", "..."],
  "diff": "<deterministic-unified-diff>",
  "diagnostics": [],
  "valid": true,
  "noOp": false
}
```

`files`, unified diff, diagnostics, and JSON keys use deterministic order.
Preview writes no source file, Git ref, receipt, parsed-source cache, or
SQLite database. `sourceSnapshotHash` is the bare SHA-256 hex digest of the
sorted managed source material (path UTF-8 bytes, NUL, each blob's SHA-256
digest, newline); it has no `sha256:` prefix. The hash covers the exact managed
source snapshot, not a working-tree interpretation.

The migration implementation must retain the current derivation order. First
form the normalized base request with `protocol`, `mode`, `expectedHead`,
bare `sourceSnapshotHash`, and trimmed `idempotencyKey` (plus rollback identity
fields only for rollback). Call the bare SHA-256 hex of canonical JSON of that
base `baseRequestHash`; derive `backupRef` as
`refs/wedl/backups/migration/<baseRequestHash>`. Append `backupRef`; only then
derive the distinct `requestHash` as the bare SHA-256 hex of canonical JSON of
the resulting normalized request. Confirmation uses that normalized request and
request hash. `upgrade-v06` adds a mode to the existing protocol; it does not
define an alternative v1 envelope.

Apply repeats the same request plus `sourceSnapshotHash` and the exact
confirmation token returned by preview. It must re-check HEAD, clean managed
tree/index, snapshot hash, request hash, and confirmation binding before any
write. Mismatch rejects with no source mutation. Its committed response retains
the current `wedl-migration/v1` fields including `protocol`, `phase: "apply"`,
`status`, `mode`, `previousHead`, `newHead`, `sourceSnapshotHash`,
`idempotencyKey`, `backupRef`, `requestHash`, `compile`, `compileSkipped`, and
`idempotentReplay`. A no-op apply retains the preview fields, changes phase to
`apply`, and returns `status: "noop"` plus `idempotentReplay: false`; it removes
exactly the public `diff` and `files` fields, as current v1 does.

Receipts remain keyed by the trimmed `idempotencyKey`; each value is exactly a
`request` (the complete normalized request), `confirmationToken`, and `result`.
Replay compares the complete identity exposed by the current protocol:
`protocol`, `mode`, `expectedHead`, `sourceSnapshotHash`, `idempotencyKey`,
`rollbackBackupRef`, and any supplied `backupRef`; rollback additionally
rechecks its saved backup object/hash. A matching receipt returns its stored
result with `idempotentReplay: true` rather than deriving a new response.

## Exact source transformation

The transaction requires all managed record envelopes to have the same source
schema. Before transforming any v0.3 or v0.5 source, it must parse that
homogeneous source, reject **any** pre-existing `chronology` world configuration
or non-world chronology annotation with
`WDL-MIG-V06-008 legacy_chronology_present`. This explicit gate prevents
manually inserted legacy chronology from being carried forward or overwritten,
then fully validate the input through its pinned legacy parser and validator.
Legacy validation failure rejects `WDL-MIG-V06-010 pinned_legacy_invalid`.
The transform runs only after that successful preflight.

Preview must next validate the complete transformed homogeneous v0.6 candidate
through the shipped v0.6 validator before creating any backup ref, source
commit, receipt, or cache. Candidate validation failure rejects
`WDL-MIG-V06-009 v06_candidate_invalid` with no write/ref/receipt/cache. Apply
reconstructs the same candidate from its bound request/source identity and
rechecks that v0.6 validation before `ensure_backup_ref` or commit. This
requirement is enforced by the shipped v0.6 validator before any release
offers `upgrade-v06`. If legacy validation,
candidate validation, chronology preflight, or commit construction fails, no
partial source rewrite is allowed.

### v0.3 to v0.6

This route is permitted only for a v0.3 world with exactly one declared
timeline and a matching default timeline. It changes every managed record's
schema marker to `wedl/v0.6`, adds `threads: []` and exactly this empty world
chronology declaration to the world record:

```yaml
chronology:
  calendars: []
  eras: []
  anchors: []
```

It adds no record membership, no non-world chronology annotation, calendar,
era, anchor, date, duration, mapping, or inferred StoryTime. Existing
Markdown bodies and all non-migration semantics are preserved. A v0.3 world
that fails the one-timeline precondition rejects with
`WDL-MIG-V06-001 v03_requires_one_timeline`.

### v0.5 to v0.6

This route changes every schema marker to `wedl/v0.6` and adds only the empty
world chronology declaration above. It preserves the existing world `threads`
declaration and every non-world `threads` membership value **byte-semantically**:
their sequence, scalar values, labels, membership relation, and absence from records
that have no membership are unchanged. It does not relabel, sort, deduplicate,
infer, or otherwise reinterpret groups. It adds no date or chronology
annotation outside the world declaration.

### v0.4, mixed, and too-new sources

`upgrade-v06` never consumes v0.4 directly. Operators must first run the
existing raw-snapshot `recover-v04` path to homogeneous v0.5, inspect and
confirm that result, then run `upgrade-v06`. Mixed v0.3/v0.5/v0.6 repositories
reject with `WDL-MIG-V06-002 mixed_source_schema`; no per-file repair mode
exists. Homogeneous lower/retired schemas reject with
`WDL-MIG-V06-007 source_schema_too_old`; homogeneous unknown or greater
versions reject with `WDL-MIG-V06-006 source_schema_too_new`.

## Atomic apply, backup, and recovery

Before the compare-and-swap source commit, apply creates an immutable backup
ref at `refs/wedl/backups/migration/<base-request-hash>` pointing to `expectedHead`.
The source write is one forward commit built from the candidate managed blobs;
it never performs `reset`, branch rewriting, `checkout`, or a broad index
commit. A crash before commit leaves HEAD and managed source unchanged; a
crash after the source commit leaves one valid forward source commit and its
backup. A failed cache rebuild does not undo the valid source commit, because
the database is rebuildable output.

The receipt is atomically written only after the source commit. Its exact
existing shape is the idempotency-key map described above: normalized request,
confirmation token, and complete apply result. Receipt-write failure reports an
incomplete receipt but may not claim the source commit was rolled back; retry
uses the complete protocol identity to recover a clear already-current or
replay result.

Rollback is a separate confirmed **forward** migration request referencing the
literal backup ref. Its preview binds both the backup ref's resolved object ID
and its source snapshot hash. Apply rechecks both, creates a new backup of the
then-current source, and commits the backup blobs forward. It neither moves a
branch nor migrates SQLite. Restoring a v0.4 backup restores raw source bytes
only; ordinary v0.4 quarantine remains in force.

## Required release evidence

The implementation must pass the local goldens in
`tests/fixtures/architecture/chronology-migration-v06.yaml` and prove:

- v0.3 golden upgrade, including the one-timeline guard and no inferred data;
- v0.5 grouping preservation using persisted record `threads` and empty
  chronology-only addition;
- full pinned-legacy validation plus no-write rejection of a world chronology
  configuration and a non-world chronology annotation;
- v0.6 candidate validation in preview and revalidation in apply before any
  ref/source/receipt/cache write, including an invalid-candidate no-write case;
- v0.4 direct rejection, mixed-schema rejection, and too-old/too-new rejection;
- stale-HEAD and dirty-managed-tree rejection before writes;
- snapshot-hash/confirmation binding, crash/validation atomicity, and receipt
  replay/idempotency;
- forward rollback and disposable-cache invalidation/rebuild; and
- conformance of both legacy inputs and upgraded v0.6 fixtures through the
  shipped validator/compiler.

## Coupling inventory

This inventory identifies the runtime surfaces updated together by the active
release. It is not authorization to expand this task beyond those surfaces.

| surface | current release coupling |
|---|---|
| `src/wedl/__init__.py` | source constants and supported-schema set |
| `src/wedl/source.py`, `src/wedl/model.py`, `src/wedl/validation.py` | envelope gate, model construction, validation dispatch |
| `src/wedl/repository.py` | parser fingerprint and source snapshot/cache identity |
| `src/wedl/compiler.py` | compiled metadata, compatibility check, disposable SQLite rebuild |
| `src/wedl/migration.py` | local preview/apply protocol, backup/ref/receipt transaction |
| `src/wedl/changeset.py`, `src/wedl/cli.py`, `src/wedl/command_parser.py` | write defaults, authoring mismatch diagnostics, command parsing |
| `src/wedl/api_*.py`, `src/wedl/query.py` | public schema/protocol reporting and read gates |
| fixtures and conformance tests | legacy inputs, upgraded v0.6 inputs, protocol/cache evidence |

## Current source and delivery boundary

The generic loader, validator, and compiler accept homogeneous `wedl/v0.7` worlds. Their chronology projection accepts validated v0.6 and v0.7 declarations; legacy v0.3/v0.5 worlds have no chronology projection. This extends compiled read-model coverage without rewriting the v0.6 grammar or the local `upgrade-v06` input policy.

The existing public adapter has a narrower advertised capability: `chronology_capability` enables calendar chronology only for v0.6, and the catalogue returns empty definitions for v0.7. `chronology.replace` also requires v0.6 and rejects v0.7 before making a changeset. In contrast, format, convert, search, and story-times obtain the compiled chronology store without that capability gate, so a valid v0.7 projection can be evaluated by those operations. This asymmetry is the current implementation boundary, not a new uniform public v0.7 admission policy. Source references: `src/wedl/chronology_api.py`, `src/wedl/chronology_index.py`, and `src/wedl/authoring.py`.

Executable contract vectors: [chronology-migration-v06.yaml](../../../../tests/fixtures/architecture/chronology-migration-v06.yaml). Their semantic cases are preserved unchanged.

<!-- @adrai:eyJhIjp7ImkiOiJhcmNoaXZlLWV4YW1wbGVzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNjI1NmMxZGRkN2IxNzM2MTU3MmYwYzhhMWJhMzk0NWVmNmE4NjU2YiIsImkiOiJzaGEyNTY6YnoxSDVrYUpnZDZMWnIxZTdVM3NBYXRfZWMtYy1zcFJGSHV0TllIdnp0TSIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNEROOEdKMTRNRThDREJGSDdKUTlEQ0siLCJvcCI6Ik8wMU00RE44R0oxNE1FOENEQkZIN0pROURDSyIsInAiOlsiUjAxTTQ4WFk0WFo0SjFXMDRXM0FQMVNONkJaIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OmpXdXRrVEdzZkRMU2tzb01zUVBDZ1lEZTBQUE8yRlhGYXVVc1ZFbnY0TkEiLCJ0IjoxNzkxNDU5ODY5MjQ5LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
