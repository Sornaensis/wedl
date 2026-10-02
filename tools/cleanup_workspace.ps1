[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Manifest,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedManifestSha256,
    [Parameter(Mandatory = $true)][string]$Receipt,
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\')
$rootPrefix = $repoRoot + '\'
$run = [ordered]@{ schemaVersion = 1; mode = $(if ($Apply) { 'apply' } else { 'dry-run' }); startedUtc = [DateTime]::UtcNow.ToString('o'); root = $repoRoot; manifestSha256 = $ExpectedManifestSha256.ToLowerInvariant(); status = 'preflight'; deletedFiles = 0; deletedBytes = 0L; deletedDirectories = 0; actions = @(); failure = $null }
$verifiedArchivePaths = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
function Resolve-Contained([string]$relative) {
    if ([string]::IsNullOrWhiteSpace($relative) -or $relative.Contains(':') -or $relative.Contains('*') -or $relative.Contains('?') -or [System.IO.Path]::IsPathRooted($relative)) { throw "Invalid relative path: $relative" }
    $parts = $relative.Replace('\', '/').Split('/')
    if (@($parts | Where-Object { $_ -eq '' -or $_ -eq '.' -or $_ -eq '..' }).Count) { throw "Invalid path component: $relative" }
    $absolute = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $relative))
    if (-not $absolute.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Outside repository: $relative" }
    $mirroredBytecode = $relative.Replace('\','/').StartsWith('.task18b-remediation14-migration-20260901/pycache/', [StringComparison]::Ordinal)
    if ($parts -contains '.git' -or ($parts -contains '.venv' -and -not $mirroredBytecode) -or $parts -contains '.hmem.workspace') { throw "Protected metadata/runtime: $relative" }
    $cursor = $repoRoot
    $rootItem = Get-Item -LiteralPath $cursor -Force
    if ($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw 'Repository root is a reparse point.' }
    foreach ($part in $parts) {
        $cursor = Join-Path $cursor $part
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw "Reparse point: $cursor" }
        }
    }
    return $absolute
}
function Assert-Target($entry) {
    $relative = [string]$entry.path
    $absolute = Resolve-Contained $relative
    $allowedCache = $relative -match '^(?:\.task959-benchmark-[123]/repository|output/generational-task959-resume/work/repository)/\.wedl/(?:world\.sqlite|vector-cache-v2\.sqlite|revisions/[a-f0-9]{40}\.sqlite)$'
    $allowedBuild = $relative -eq 'build' -or $relative.StartsWith('build/', [StringComparison]::Ordinal)
    $allowedEmpty = $relative -match '^(?:_b9_(?:native_disposition_probe(?:3|4|5)?|native_link_probe|pytest_migration_collect)|_pytest_migration_eighth|\.task959-temp|\.wedl-test-tmp-[a-f0-9]{32})(?:/[^/]+)*$'
    $allowedBrowserCache = $relative -match '^output/spatial-release-scale/browser-100k-typed/fixture/\.wedl/(?:world\.sqlite|vector-cache-v2\.sqlite|revisions/[a-f0-9]{40}\.sqlite)$' -and $entry.kind -eq 'file'
    $allowedBytecode = ($relative -eq '.task18b-remediation14-migration-20260901' -or $relative.StartsWith('.task18b-remediation14-migration-20260901/', [StringComparison]::Ordinal)) -and ($entry.kind -eq 'directory' -or $relative.EndsWith('.pyc', [StringComparison]::Ordinal))
    $allowedProbe = $relative -eq '_b9_native_disposition_probe4/root/probe.bin' -and $entry.kind -eq 'file' -and $entry.bytes -eq 0
    $allowedEmptyCache = $relative -match '^pytest-cache-files-(?:feinza2s|iewjpmf9|ly_05gb_|nxhdlj4w|st3fvg7c)$' -and $entry.kind -eq 'directory'
    $allowedEmptyNodeids = $relative -eq 'v/cache/nodeids' -and $entry.kind -eq 'file' -and $entry.bytes -eq 2 -and $entry.sha256 -eq '4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945'
    $allowedNodeidsDirectory = $relative -in @('v', 'v/cache') -and $entry.kind -eq 'directory'
    $allowedArchived = $entry.kind -eq 'file' -and $verifiedArchivePaths.Contains($relative)
    $allowedArchivedDirectory = $entry.kind -eq 'directory' -and (($relative -eq '.task18b-rem16-migration-3252-1543548686' -and $verifiedArchivePaths.Contains($relative + '/migration.junit.xml')) -or ($relative -eq '.task91-full-regression-0077965ef3b44a48a8c1c4d340caaa3f' -and $verifiedArchivePaths.Contains($relative + '/full-regression.junit.xml')))
    if (-not ($allowedCache -or $allowedBuild -or $allowedBrowserCache -or $allowedBytecode -or $allowedProbe -or $allowedEmptyCache -or $allowedEmptyNodeids -or $allowedNodeidsDirectory -or $allowedArchived -or $allowedArchivedDirectory -or ($allowedEmpty -and $entry.kind -eq 'directory'))) { throw "Target is outside supported disposable categories: $relative" }
    if ($entry.kind -notin @('file', 'directory')) { throw "Unknown entry kind: $relative" }
    if ($allowedCache -and $entry.kind -ne 'file') { throw "Cache targets must be exact files: $relative" }
    $item = Get-Item -LiteralPath $absolute -Force
    if ($entry.kind -eq 'file') {
        if ($item.PSIsContainer -or $item.Length -ne [long]$entry.bytes -or $item.LastWriteTimeUtc.Ticks -ne [long]$entry.modifiedUtcTicks) { throw "File metadata changed: $relative" }
        if ($entry.sha256 -notmatch '^[a-f0-9]{64}$' -or (Get-FileHash -LiteralPath $absolute -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "File digest changed: $relative" }
    } elseif (-not $item.PSIsContainer) { throw "Directory became a file: $relative" }
    return $absolute
}
$receiptAbsolute = Resolve-Contained $Receipt
if (-not $Receipt.Replace('\','/').StartsWith('output/repository-cleanup-20261002/', [StringComparison]::Ordinal)) { throw 'Receipt must be inside the cleanup evidence directory.' }
if (Test-Path -LiteralPath $receiptAbsolute) { throw 'Refusing to overwrite an existing receipt.' }
if (-not (Test-Path -LiteralPath (Split-Path -Parent $receiptAbsolute) -PathType Container)) { throw 'Receipt parent must already exist.' }
function Publish-Receipt {
    $run.finishedUtc = [DateTime]::UtcNow.ToString('o')
    $null = Resolve-Contained $Receipt
    $final = $run.status -in @('complete', 'dry-run-complete', 'failed')
    $destination = if ($final) { $receiptAbsolute } else { $receiptAbsolute + '.checkpoint-' + $run.actions.Count.ToString('D6') + '.json' }
    $payload = if ($final) { $run } else { [ordered]@{ status = $run.status; root = $run.root; manifestSha256 = $run.manifestSha256; finishedUtc = $run.finishedUtc; actionCount = $run.actions.Count; deletedFiles = $run.deletedFiles; deletedBytes = $run.deletedBytes; deletedDirectories = $run.deletedDirectories; lastAction = $(if ($run.actions.Count) { $run.actions[-1] } else { $null }) } }
    $pendingReceipt = $destination + '.pending'
    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes(($payload | ConvertTo-Json -Depth 8))
    $stream = [System.IO.FileStream]::new($pendingReceipt, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
    # Immutable atomic publication avoids truncating a prior action receipt.
    [System.IO.File]::Move($pendingReceipt, $destination)
}
try {
    $manifestAbsolute = Resolve-Contained $Manifest
    if ((Get-FileHash -LiteralPath $manifestAbsolute -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedManifestSha256.ToLowerInvariant()) { throw 'Manifest digest differs from the reviewed digest.' }
    $definition = Get-Content -LiteralPath $manifestAbsolute -Raw | ConvertFrom-Json
    if ($definition.schemaVersion -ne 1 -or $definition.taskId -ne '43f822e5-3f95-48e6-9ad4-0561c6cc73fd') { throw 'Unsupported manifest identity.' }
    if ([System.IO.Path]::GetFullPath($definition.root).TrimEnd('\') -ne $repoRoot -or $definition.workspaceId -ne '376d138f-4edb-4516-919b-22ca0f064d34') { throw 'Manifest repository/workspace mismatch.' }
    if ((Get-Content -LiteralPath (Join-Path $repoRoot '.hmem.workspace') -Raw).Trim() -ne $definition.workspaceId) { throw 'Repository workspace marker changed.' }
    if ($definition.archiveProof) {
        $archiveAbsolute = Resolve-Contained $definition.archiveProof.path
        $indexAbsolute = Resolve-Contained $definition.archiveProof.indexPath
        if ($definition.archiveProof.path -ne 'output/repository-cleanup-20261002/root-receipts.zip' -or $definition.archiveProof.indexPath -ne 'output/repository-cleanup-20261002/root-receipts-index.json') { throw 'Unsupported archive/index paths.' }
        if ((Get-FileHash -LiteralPath $archiveAbsolute).Hash.ToLowerInvariant() -ne $definition.archiveProof.sha256 -or (Get-FileHash -LiteralPath $indexAbsolute).Hash.ToLowerInvariant() -ne $definition.archiveProof.indexSha256) { throw 'Archive/index digest changed.' }
        $archiveIndex = Get-Content -LiteralPath $indexAbsolute -Raw | ConvertFrom-Json
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [System.IO.Compression.ZipFile]::OpenRead($archiveAbsolute)
        try {
            if ($zip.Entries.Count -ne $definition.archiveProof.entries -or $archiveIndex.entries.Count -ne $zip.Entries.Count) { throw 'Archive entry count mismatch.' }
            foreach ($saved in $archiveIndex.entries) {
                $null = Resolve-Contained $saved.path
                if ($saved.path -notmatch '^(?:\.pytest-.*\.xml|\.task.*\.xml|\.chronology-.*\.xml|_t92.*\.log|\.tmp-task91-(?:focus|gate)\.(?:exit|pid|stderr|stdout)|\.task18b-rem16-migration-3252-1543548686/migration\.junit\.xml|\.task91-full-regression-0077965ef3b44a48a8c1c4d340caaa3f/full-regression\.junit\.xml)$') { throw 'Unsupported archived receipt category.' }
                if (-not $verifiedArchivePaths.Add([string]$saved.path)) { throw 'Duplicate archive index path.' }
                $members = @($zip.Entries | Where-Object { $_.FullName -ceq $saved.path })
                if ($members.Count -ne 1 -or $members[0].Length -ne $saved.bytes) { throw 'Archive path/length mismatch.' }
                $stream = $members[0].Open(); $hasher = [System.Security.Cryptography.SHA256]::Create()
                try { $digest = [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() } finally { $stream.Dispose(); $hasher.Dispose() }
                if ($digest -ne $saved.sha256) { throw 'Archived decompressed digest mismatch.' }
                $targets = @($definition.entries | Where-Object { $_.path -ceq $saved.path -and $_.kind -eq 'file' })
                if ($targets.Count -ne 1 -or $targets[0].sha256 -ne $saved.sha256 -or $targets[0].bytes -ne $saved.bytes) { throw 'Manifest receipt differs from archived index.' }
            }
        } finally { $zip.Dispose() }
    }
    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $definition.entries) {
        if (-not $seen.Add([string]$entry.path)) { throw "Duplicate manifest target: $($entry.path)" }
        $null = Assert-Target $entry
    }
    # Every directory child must be an exact manifest entry. Enumeration errors fail closed.
    foreach ($entry in $definition.entries | Where-Object kind -eq 'directory') {
        $absolute = Resolve-Contained $entry.path
        foreach ($child in Get-ChildItem -LiteralPath $absolute -Force -ErrorAction Stop) {
            $relative = $child.FullName.Substring($rootPrefix.Length).Replace('\','/')
            if (-not $seen.Contains($relative)) { throw "Unlisted directory child: $relative" }
        }
    }
    $run.status = 'validated'
    Publish-Receipt
    $ordered = @($definition.entries | Where-Object kind -eq 'file') + @($definition.entries | Where-Object kind -eq 'directory' | Sort-Object { $_.path.Length } -Descending)
    foreach ($entry in $ordered) {
        $absolute = Assert-Target $entry
        if ($entry.kind -eq 'directory' -and $Apply -and @(Get-ChildItem -LiteralPath $absolute -Force -ErrorAction Stop).Count) { throw "Directory is no longer empty: $($entry.path)" }
        if ($Apply) {
            # Deliberately non-recursive: an unexpected child prevents directory deletion.
            if ($entry.kind -eq 'directory') { [System.IO.Directory]::Delete($absolute, $false) }
            else { Remove-Item -LiteralPath $absolute -Force -ErrorAction Stop }
            if (Test-Path -LiteralPath $absolute) { throw "Removal did not complete: $($entry.path)" }
            if ($entry.kind -eq 'file') { $run.deletedFiles++; $run.deletedBytes += [long]$entry.bytes } else { $run.deletedDirectories++ }
        }
        $run.actions += [ordered]@{ path = $entry.path; kind = $entry.kind; status = $(if ($Apply) { 'deleted' } else { 'would-delete' }) }
        if ($Apply) { Publish-Receipt }
    }
    $run.status = $(if ($Apply) { 'complete' } else { 'dry-run-complete' })
    Publish-Receipt
    Write-Output "$($run.status): $($run.deletedFiles) files, $($run.deletedBytes) bytes, $($run.deletedDirectories) directories removed."
} catch {
    $originalFailure = $_
    $run.status = 'failed'
    $run.failure = [ordered]@{ phase = $(if ($run.actions.Count) { 'cleanup' } else { 'preflight' }); message = $_.Exception.Message }
    try { Publish-Receipt } catch { [Console]::Error.WriteLine("Receipt publication failed: $($_.Exception.Message)") }
    throw $originalFailure
}
