# Workspace cleanup

The 2026-10-02 cleanup reduced the repository root from 981 entries to 23 and removed 55,913,760,183 logical file bytes. It archived 193 root receipts, then retired all 742 closed historical run directories after preserving their diagnostic records. Retirement removed 1,756,584 regular files, 610,745 directories and one link entry. Physical disk savings are unmeasured because files can share hardlink allocations.

The catalogue, inventories, exact manifests, archive indices, provenance and execution receipts are in `output/repository-cleanup-20261002/`. The narrow ignore rule keeps this local evidence out of Git status. Main source, eight inherited modified files, both story Git links, registered worktrees, and unresolved spatial and warm-read proof inputs remain protected. `_aef_review_fix_mig_clean` remains after Windows denied its move; no ACL repair was attempted. The empty `tmpvx2hjok2` has unknown provenance and remains outside the cleanup scope.

## Preserved diagnostics

`root-receipts.zip` and its index preserve all 193 earlier root XML/log/process-stream receipts byte for byte. The retirement archive, `legacy-retirement/diagnostics.zip`, has 91,527 verified members: 58,328 raw records and 33,199 derived metadata envelopes. Its index maps current and original paths, lengths, fingerprints, and each retained member's SHA256. Every member was independently decompressed and checked before retirement.

Raw records are byte-restorable. Derived envelopes preserve diagnostic state, hashes, lengths, journal/index metadata and bounded SQLite revision/compile metrics; they cannot restore the omitted database, source, Git-object, or beforeimage payloads. Large journal image fields have explicit omitted-field paths and encoded hashes/lengths. Malformed journals retain actual parse errors rather than inferred outcomes. SQLite inspection used immutable metadata reads, without WAL replay; unknown malformed stores and omitted companions are recorded.

To restore a raw receipt, select its indexed member, verify the decompressed length and SHA256, and write only to an absent contained destination. Old rename/restore maps are historical provenance; after payload retirement they cannot restore whole fixtures. Rebuild future fixtures from maintained source.

## Guarded maintenance tools

These tools implement exact reviewed manifests for this maintenance task. They do not discover disposable content from a filename. Dry-run is the default; `-Apply` changes the filesystem. Supply the reviewed manifest SHA256 and a new receipt path on each invocation.

`tools/cleanup_workspace.ps1` checks workspace identity, containment, nonreparse ancestors, file hashes/lengths/timestamps and exact directory child rosters. Files use literal-path removal; directories use atomic nonrecursive deletion that refuses new children. Receipt-backed removal first verifies every decompressed archive member.

`tools/organize_workspace_runs.ps1` performs exact same-volume directory moves and reviewed restoration subsets with native identities and ancestor/location checks. There is no copy/delete fallback. Its 2026-10-02 maps describe the historical organization step, preceding retirement.

`tools/retire_workspace_runs.ps1` verifies the archive/index, exact root and ancestor identities, worktree exclusions, and preserved diagnostics' locked native fingerprints. NTFS hardlink aliases can expose stale directory-entry timestamps, so freshness uses handle metadata. All repository-to-root ancestor handles remain held during deletion. Descendant reparse entries are unlinked without following targets. Native unlink avoids changing read-only attributes shared with retained hardlinks; unsupported operations refuse rather than fall back.

```powershell
./tools/retire_workspace_runs.ps1 -Manifest output/repository-cleanup-20261002/legacy-retirement/retirement-manifest.json -ExpectedManifestSha256 <reviewed-sha256> -Receipt output/repository-cleanup-20261002/legacy-retirement/new-dry-run.json
# Append -Apply only for the independently approved exact manifest.
```

Immutable flushed checkpoints record each completed root, with explicit partial/refused outcomes. A crash between deletion and checkpoint publication can leave unrecorded removals; reconcile actual paths before preparing a new manifest. No process termination or ACL change is performed.

Bounded process checks see absolute canonical prefixes. Relative paths, aliases and processes identified only by their working directory are outside their visibility. Coordinate with known owners; these checks do not establish host-wide inactivity.

## Prevention and remaining work

The accepted [bounded scratch and test-signal policy](../architecture/adrai/decisions/R01M/R01M48NTY3JQB1XGHTVQB4V60EX--bounded-development-scratch-and-test-signals.decision.md)
requires compact routine pass/fail reporting, one owned scratch area per active
task with a finite lifetime, and bounded diagnostic retention. It governs future
work; existing required evidence stays protected. The current test runner still
retains failure artifacts as described in [TESTING.md](TESTING.md); runner
retention changes remain separate work.

Root-anchored ignore rules cover observed scratch and receipt families. Ignoring a file does not expire it or prove disposability. A future runner change should direct scratch into one documented ignored directory with explicit retention. Current spatial staging/warm-read proof inputs and other historical output groups remain catalogued separately. Compatibility package-data copies, source organization, dead-code review and stale timing documentation need their own behavior review.
