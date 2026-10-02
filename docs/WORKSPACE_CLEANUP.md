# Workspace cleanup

The 2026-10-02 cleanup reduced the repository root from 981 entries to 23. It removed 4,274,760,618 **logical file bytes**, archived 193 historical receipts, and relocated 742 historical run directories into `output/repository-cleanup-20261002/legacy-test-runs/`. Physical disk savings are unknown: retained revision databases can use hardlinks to world databases. Directory relocation frees no storage.

The readable catalogue, inventories, removal manifests, archive restoration index, rename/restore maps, and execution receipts are under `output/repository-cleanup-20261002/`. That directory is local evidence. Historical source copies, nested Git repositories, unique receipts, patches, scripts, and current investigation inputs remain preserved. The eight inherited modified files retain their original hashes. `_aef_review_fix_mig_clean` remains at the root because Windows denied its namespace move; `tmpvx2hjok2` has unknown provenance and remains untouched.

## Exact removal

`tools/cleanup_workspace.ps1` implements this maintenance task's exact manifest categories. It does not discover disposable content from filenames. An independently reviewed manifest and its SHA256 are required; dry-run is the default, and `-Apply` performs removal. Use a fresh receipt path for every invocation.

```powershell
./tools/cleanup_workspace.ps1 -Manifest output/repository-cleanup-20261002/removal-manifest.json -ExpectedManifestSha256 <reviewed-sha256> -Receipt output/repository-cleanup-20261002/new-dry-run.json
# For an approved manifest, use a fresh receipt path and append -Apply.
```

The tool checks workspace identity, containment, nonreparse ancestors, file hashes/lengths/mtime, and directory child rosters. Files use literal-path removal; directories use atomic nonrecursive deletion that refuses new children. Archived receipts require validation of every decompressed entry against the retained name/length/SHA256 index before originals can be removed. A changed file, unexpected child, inaccessible entry, link, or unsupported category refuses the operation. Each successful action publishes an immutable flushed checkpoint; partial failures retain completed actions. A crash between an action and checkpoint publication can leave an unrecorded missing target, so reconcile existence before preparing another manifest.

Removed cache bytes are no longer retained. Task959 rebuild provenance and historical source/scripts remain in the manifests. The synthetic browser100k cache requires its preserved historical compiled-browser patch and browser-small seed recipe; ordinary source compilation is not its reconstruction recipe. Removing caches does not change the original historical receipt claims or certify current product gates.

## Reversible run organization

`tools/organize_workspace_runs.ps1` accepts an independently reviewed exact rename manifest and SHA256. Dry-run is the default; `-Apply` moves directories and `-Restore` reverses successful entries. For restoring a subset, prepare and review an exact subset manifest using the recorded identities and restore maps. The original manifest includes one refused entry, so blindly restoring the full roster is inappropriate.

```powershell
./tools/organize_workspace_runs.ps1 -Manifest output/repository-cleanup-20261002/rename-manifest.extra16.json -ExpectedManifestSha256 <reviewed-sha256> -Receipt output/repository-cleanup-20261002/new-restore-dry-run.json -Restore
# After reviewing the dry-run, use another fresh receipt and append -Apply.
```

Moves use same-volume `Directory.Move` with native directory identity and ancestor/location checks. There is no descendant traversal, copy/delete fallback, link following, ACL repair, or process termination. Each successful move preserves the directory identity and publishes a checkpoint. Opaque descendants remain intact; historical absolute paths or relative links may require restoration to the original location before replay. Relocation does not establish that a fixture runs from its new path.

A bounded process query checks absolute original and retained prefixes for the actual direction of movement. Matches are deferred; query errors, deadlines, and caps refuse the invocation. Relative paths, aliases, and processes whose working directory alone identifies a fixture are outside this check. Coordinate with known owners; the query is not evidence that no process on the host uses the inputs.

## Retention and prevention

Canonical source, tests, documentation, examples, supported tools, the development environment, runtime state, both Git links, registered worktrees, and current spatial/generational investigation inputs remain protected. Root XML/log/process streams are preserved in `root-receipts.zip`; `root-receipts-index.json` records original relative names, lengths, hashes, and timestamps. Verify decompressed bytes before restoring an individual receipt to an absent contained original path.

Root-anchored ignore rules cover observed run families and receipt extensions. Ignoring a file does not prove it is disposable or expire it. Follow-up work should direct runner scratch into one documented ignored run directory with a retention policy, reconcile stale suite timing documentation, and review closed output bulk by provenance. Legacy and v0.7 package-data copies are compatibility fixtures; byte similarity alone is insufficient to remove them. Source architecture and dead-code changes require their own behavior review.