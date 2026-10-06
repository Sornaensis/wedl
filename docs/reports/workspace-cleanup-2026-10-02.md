# Workspace cleanup

> Historical report. Original versions, fixture counts, outcomes and measurements below belong to the recorded exercise. This migration performs no fresh release/performance qualification. Architectural rationale is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; custody is in ADRAI A01M49735W4D3CZ2PJ129HTJCGG. Read these with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in the WEDL development/source checkout.

Custody entry check (2026-10-06, source revision `4d81ccc3c3cfc52381396c24458866cf1e6fb2ad`): `output/repository-cleanup-20261002/` exists, but its named `root-receipts.zip` and `legacy-retirement/diagnostics.zip` are absent in this checkout. The statements below preserve the recorded 2026-10-02 outcomes; this task did not reverify, restore or delete that protected output. See [retention/dispositions](retention-and-dispositions.md).

The 2026-10-02 cleanup reduced the repository root from 981 entries to 23 and removed 55,913,760,183 logical file bytes. It archived 193 root receipts, then retired all 742 closed historical run directories after preserving their diagnostic records. Retirement removed 1,756,584 regular files, 610,745 directories and one link entry. Physical disk savings are unmeasured because files can share hardlink allocations.

The catalogue, inventories, exact manifests, archive indices, provenance and execution receipts are in `output/repository-cleanup-20261002/`. The narrow ignore rule keeps this local evidence out of Git status. Main source, eight inherited modified files, both story Git links, registered worktrees, and unresolved spatial and warm-read proof inputs remain protected. `_aef_review_fix_mig_clean` remains after Windows denied its move; no ACL repair was attempted. The empty `tmpvx2hjok2` has unknown provenance and remains outside the cleanup scope.

## Preserved diagnostics

`root-receipts.zip` and its index preserve all 193 earlier root XML/log/process-stream receipts byte for byte. The retirement archive, `legacy-retirement/diagnostics.zip`, has 91,527 verified members: 58,328 raw records and 33,199 derived metadata envelopes. Its index maps current and original paths, lengths, fingerprints, and each retained member's SHA256. Every member was independently decompressed and checked before retirement.

Raw records are byte-restorable. Derived envelopes preserve diagnostic state, hashes, lengths, journal/index metadata and bounded SQLite revision/compile metrics; they cannot restore the omitted database, source, Git-object, or beforeimage payloads. Large journal image fields have explicit omitted-field paths and encoded hashes/lengths. Malformed journals retain actual parse errors rather than inferred outcomes. SQLite inspection used immutable metadata reads, without WAL replay; unknown malformed stores and omitted companions are recorded.

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

## Recorded maintenance tools

The historical cleanup used `tools/cleanup_workspace.ps1`, `tools/organize_workspace_runs.ps1`, and `tools/retire_workspace_runs.ps1`. Their original guard, native-identity, checkpoint and process-visibility rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z. This report does not authorize another cleanup.


## Separate follow-up work

Runner retention and other historical source/fixture cleanup groups remained separate work. The [testing guide](../guides/testing.md) records the present runner limitations. Current policy is read through ADRAI A01M3Y3S5QMJWMBMQT42HDQPDJS; existing required evidence remains protected.
