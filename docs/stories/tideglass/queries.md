# Tideglass query and edit recipes

Run these PowerShell blocks in order after the [walkthrough](walkthrough.md).
They use its `$repo` disposable copy. Request files are created in your working
directory. Save the initialized world's own revision from `status`; a source
checkout revision is not that world's HEAD.

Use this small writer for UTF-8 JSON without a byte-order mark, including in
Windows PowerShell 5.1:

```powershell
function Write-TideglassJson($path, $value) {
    [IO.File]::WriteAllText((Join-Path (Get-Location) $path), ($value | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
}
```

## 5. Directed routes and the flood boundary

```powershell
$revision = (wedl --compact status --repo $repo | ConvertFrom-Json).revision
$capabilities = @('generational-core-v1', 'spatial-core-v1', 'geometry-v1', 'route-v1', 'overlay-v1')
$adjacency = @{protocol='wedl-spatial/v1'; revision=$revision; capabilities=$capabilities; limit=10; cursor=$null; locationId='loc_0708459BXR2PVVHA4VASXHB69C'; modes=@('foot')}
Write-TideglassJson adjacency.json $adjacency
$quayRoute = wedl --compact spatial adjacency adjacency.json --repo $repo --require-compiled | ConvertFrom-Json
$adjacency.locationId = 'loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ'
Write-TideglassJson adjacency.json $adjacency
$beaconRoute = wedl --compact spatial adjacency adjacency.json --repo $repo --require-compiled | ConvertFrom-Json
$overlay = @{protocol='wedl-spatial/v1'; revision=$revision; capabilities=$capabilities; limit=10; cursor=$null; queryScope='location'; locationId='loc_0708459BXR2PVVHA4VASXHB69C'; audience='author'; perspective='author'; asOf=@{timeline='main'; tick='30'; order='1'}}
Write-TideglassJson overlay.json $overlay
$floodAtEnd = wedl --compact spatial overlay-as-of overlay.json --repo $repo --require-compiled | ConvertFrom-Json
$overlay.asOf.order = '2'
Write-TideglassJson overlay.json $overlay
$floodAfter = wedl --compact spatial overlay-as-of overlay.json --repo $repo --require-compiled | ConvertFrom-Json
```

The two route results are `ok`, with `route:quay-beacon` and
`route:beacon-quay` respectively. `overlay:flood-warning` is present at `30:1`
and absent at `30:2`. Keep the complete capability list in both spatial and
generational requests, including components unrelated to the selected leaf command.

## 6. Explicit family, membership and keeper history

```powershell
$history = @{protocol='wedl-generational/v1'; operation='parents'; revision=$revision; capabilities=$capabilities; mode='author-as-of'; timeline='main'; at=@{timeline='main'; tick='0'; order='0'}; subject='char_4JVNDN6QW0BAFFV379AJJSMEPZ'; items=10; depth=8}
Write-TideglassJson history.json $history
$parents = wedl --compact generational parents history.json --repo $repo | ConvertFrom-Json
$history.operation = 'organization'; $history.subject = 'organization_4YNRVZSRYTGBB27YKD7NW1T7CZ'
Write-TideglassJson history.json $history
$cooperative = wedl --compact generational organization history.json --repo $repo | ConvertFrom-Json
$history.subject = 'organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG'
Write-TideglassJson history.json $history
$guild = wedl --compact generational organization history.json --repo $repo | ConvertFrom-Json
$history.operation = 'vital'; $history.subject = 'char_39E9EWWC017E40519033S4RW4X'; $history.at.tick = '-31'
Write-TideglassJson history.json $history
$beforeBirth = wedl --compact generational vital history.json --repo $repo | ConvertFrom-Json
$history.at.tick = '-30'
Write-TideglassJson history.json $history
$selaBorn = wedl --compact generational vital history.json --repo $repo | ConvertFrom-Json
$history.operation = 'legacy'; $history.subject = 'legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ'; $history.at.tick = '20'
Write-TideglassJson history.json $history
$keeper = wedl --compact generational legacy history.json --repo $repo | ConvertFrom-Json
```

Mina has exactly one parent relation, to Sela. The active organization roles
are Mina's `repair coordinator` and Orin's `ferry operator`. Sela's vital result
is `unknown` before the authored birth and `living` at `-30:0`; its citation
names that explicit point. The keeper result has Cora as its sole holder,
Tavi's claim in `disputed` state, and no succession edges.

## 7. Preview, confirm and replay a disposable edit

Finish the read-only exercises before this edit. Keep the identical request and
key for replay. Preview does not change HEAD or source; inspect `valid`, `files`
and the diff before applying. This example changes only the survey staff title
to `Calibrated Survey Staff`; its other frontmatter and body stay intact.

```powershell
$change = @{protocol='wedl-changeset/v1'; expectedHead=$revision; idempotencyKey='tideglass-survey-title-v1'; summary='Calibrate the survey staff title'; operations=@(@{type='entity.update'; entityId='obj_76VC9SVNSR7ASZK025X7PTRBJH'; frontmatterPatch=@{title='Calibrated Survey Staff'}})}
Write-TideglassJson change.json $change
$preview = wedl --compact changeset preview change.json --repo $repo | ConvertFrom-Json
wedl --compact changeset apply change.json --repo $repo --confirm mismatch
$stale = $change.Clone(); $stale.expectedHead = '0000000000000000000000000000000000000000'
Write-TideglassJson stale.json $stale
wedl --compact changeset preview stale.json --repo $repo
$applied = wedl --compact changeset apply change.json --repo $repo --confirm $preview.confirmationToken | ConvertFrom-Json
$afterEdit = wedl --compact status --repo $repo | ConvertFrom-Json
$replay = wedl --compact changeset apply change.json --repo $repo --confirm $preview.confirmationToken | ConvertFrom-Json
$afterReplay = wedl --compact status --repo $repo | ConvertFrom-Json
```

The deliberate wrong-token and stale-HEAD commands exit with code 2 and refuse
the operation. The valid apply creates one commit and reports
`idempotentReplay: false`. The replay reports `true`; `$afterEdit.revision` and
`$afterReplay.revision` are identical. A newly keyed request that still uses the
old HEAD must be previewed again against the new revision before applying.

Keep or remove your disposable world and request files when you finish. The
maintained example in the installed package was never edited. For other recipes,
see the [command-line guide](../../guides/command-line.md) and
[generational guide](../../guides/generational.md).
