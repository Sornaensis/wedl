[CmdletBinding()]
param(
    [string]$Manifest,
    [string]$ExpectedManifestSha256,
    [string]$Receipt,
    [switch]$Apply,
    [switch]$Restore,
    [string[]]$InspectDirectories
)
$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\')
$rootPrefix = $repoRoot + '\'
if (-not ('WedlCleanupDirectoryIdentity' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class WedlCleanupDirectoryIdentity {
    [StructLayout(LayoutKind.Sequential)] public struct Time { public uint Low; public uint High; }
    [StructLayout(LayoutKind.Sequential)] public struct Info {
        public uint Attributes; public Time Creation; public Time Access; public Time Write;
        public uint Volume; public uint SizeHigh; public uint SizeLow; public uint Links; public uint IndexHigh; public uint IndexLow;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle handle, out Info info);
    public static Info Read(string path) {
        using (var handle = CreateFileW(path, 0, 7, IntPtr.Zero, 3, 0x02200000, IntPtr.Zero)) {
            if (handle.IsInvalid) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            Info info; if (!GetFileInformationByHandle(handle, out info)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            return info;
        }
    }
    public static string Birth(Info info) { return (((ulong)info.Creation.High << 32) | info.Creation.Low).ToString(); }
}
'@
}
function Resolve-Contained([string]$relative) {
    if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative) -or $relative.Contains(':') -or $relative.Contains('*') -or $relative.Contains('?')) { throw 'Invalid relative path.' }
    $parts = $relative.Replace('\','/').Split('/')
    if (@($parts | Where-Object { $_ -in @('', '.', '..', '.git', '.venv', '.hmem.workspace') }).Count) { throw 'Protected or invalid path component.' }
    $absolute = [IO.Path]::GetFullPath((Join-Path $repoRoot $relative))
    if (-not $absolute.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Path escaped repository.' }
    $cursor = $repoRoot
    foreach ($part in @('') + $parts) {
        if ($part) { $cursor = Join-Path $cursor $part }
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse ancestor: $cursor" }
        }
    }
    return $absolute
}
function Get-DirectoryIdentity([string]$absolute) {
    $info = [WedlCleanupDirectoryIdentity]::Read($absolute)
    if (($info.Attributes -band 0x10) -eq 0 -or ($info.Attributes -band 0x400) -ne 0) { throw 'Target is not a plain directory.' }
    return [ordered]@{ volume = $info.Volume.ToString('x8'); fileId = $info.IndexHigh.ToString('x8') + $info.IndexLow.ToString('x8'); creation100ns = [WedlCleanupDirectoryIdentity]::Birth($info); attributes = $info.Attributes }
}
function Assert-Name([string]$name) {
    if ($name -eq '.pytest_cache' -or $name -notmatch '^(?:\.task|\.pytest|\.authoring-|\.frontiersmen-|\.tmp-|\.wedl-test-tmp-|\.wedl-task6aee-|\._t92_|\._task92_|_aef|_b9|_t92|_validation|_task|_pytest|_ce365)[^/\\]*$') { throw "Unsupported historical root: $name" }
}
if ($InspectDirectories) {
    $rows = @(foreach ($name in $InspectDirectories) {
        try { Assert-Name $name; $absolute = Resolve-Contained $name; $identity = Get-DirectoryIdentity $absolute; [ordered]@{name=$name;identity=$identity;status='known'} }
        catch { [ordered]@{name=$name;status='deferred';reason=$_.Exception.Message} }
    })
    $rows | ConvertTo-Json -Depth 5 -Compress
    return
}
if (-not $Manifest -or $ExpectedManifestSha256 -notmatch '^[a-fA-F0-9]{64}$' -or -not $Receipt) { throw 'Manifest, reviewed SHA256 and receipt are required.' }
$receiptAbsolute = Resolve-Contained $Receipt
if (-not $Receipt.StartsWith('output/repository-cleanup-20261002/', [StringComparison]::Ordinal) -or (Test-Path -LiteralPath $receiptAbsolute)) { throw 'Receipt must be new and contained in cleanup evidence.' }
if (-not (Test-Path -LiteralPath (Split-Path -Parent $receiptAbsolute) -PathType Container)) { throw 'Receipt parent must exist.' }
$run = [ordered]@{schemaVersion=1;mode=$(if($Apply){if($Restore){'restore'}else{'apply'}}else{'dry-run'});root=$repoRoot;manifestSha256=$ExpectedManifestSha256.ToLowerInvariant();startedUtc=[DateTime]::UtcNow.ToString('o');status='preflight';moved=0;deferred=0;diskBytesRecovered=0;actions=@();processCheck=$null;failure=$null}
function Publish-Receipt([bool]$final) {
    $run.finishedUtc = [DateTime]::UtcNow.ToString('o')
    $null = Resolve-Contained $Receipt
    $destination = if($final){$receiptAbsolute}else{$receiptAbsolute+'.checkpoint-'+$run.actions.Count.ToString('D6')+'.json'}
    $payload = if($final){$run}else{[ordered]@{mode=$run.mode;manifestSha256=$run.manifestSha256;finishedUtc=$run.finishedUtc;moved=$run.moved;deferred=$run.deferred;lastAction=$(if($run.actions.Count){$run.actions[-1]}else{$null})}}
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($payload|ConvertTo-Json -Depth 8))
    $stream = [IO.FileStream]::new($destination+'.pending',[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)}finally{$stream.Dispose()}
    [IO.File]::Move($destination+'.pending',$destination)
}
try {
    $manifestAbsolute=Resolve-Contained $Manifest
    if((Get-FileHash -LiteralPath $manifestAbsolute).Hash.ToLowerInvariant() -ne $ExpectedManifestSha256.ToLowerInvariant()){throw 'Reviewed manifest digest changed.'}
    $definition=Get-Content -LiteralPath $manifestAbsolute -Raw|ConvertFrom-Json
    if($definition.schemaVersion -ne 1 -or $definition.taskId -ne '43f822e5-3f95-48e6-9ad4-0561c6cc73fd' -or [IO.Path]::GetFullPath($definition.root).TrimEnd('\') -ne $repoRoot -or $definition.workspaceId -ne '376d138f-4edb-4516-919b-22ca0f064d34' -or (Get-Content -LiteralPath (Join-Path $repoRoot '.hmem.workspace') -Raw).Trim() -ne $definition.workspaceId){throw 'Repository/task/workspace identity mismatch.'}
    $rootIdentity=Get-DirectoryIdentity $repoRoot
    $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach($entry in $definition.entries){
        Assert-Name $entry.source
        if(-not $seen.Add([string]$entry.source)){throw 'Duplicate source root.'}
        if($entry.destination -ne ('output/repository-cleanup-20261002/legacy-test-runs/'+$entry.source)){throw 'Destination differs from restoration map.'}
        $source=Resolve-Contained $entry.source;$destination=Resolve-Contained $entry.destination
        foreach($worktree in $definition.registeredWorktrees){
            $registered=[IO.Path]::GetFullPath($worktree).TrimEnd('\')
            if($registered -eq $source -or $registered.StartsWith($source+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Registered Git worktree is a selected source.'}
        }
        if($entry.identity.volume -ne $rootIdentity.volume){throw 'Source volume differs from repository volume.'}
        $parentIdentity=Get-DirectoryIdentity (Split-Path -Parent $destination)
        if($parentIdentity.volume -ne $rootIdentity.volume){throw 'Destination parent is on another volume.'}
    }
    # One server-filtered current-use query per invocation. No command lines are persisted.
    $queryStarted=[DateTime]::UtcNow
    $families=@('.task','.pytest','.authoring-','.frontiersmen-','.tmp-','.wedl-test-tmp-','.wedl-task6aee-','._t92_','._task92_','_aef','_b9','_t92','_validation','_task','_pytest','_ce365')
    $clauses=@(foreach($family in $families){$literal=($rootPrefix+$family).Replace('\','\\').Replace("'","\'");"CommandLine LIKE '%$literal%'"})
    $retainedPrefix = $rootPrefix + 'output\repository-cleanup-20261002\legacy-test-runs\'
    $retainedLiteral = $retainedPrefix.Replace('\','\\').Replace("'","\'")
    $clauses += "CommandLine LIKE '%$retainedLiteral%'"
    $query="SELECT ProcessId, CommandLine FROM Win32_Process WHERE "+($clauses -join ' OR ')
    $rows=@(Get-CimInstance -Namespace root/cimv2 -Query $query -OperationTimeoutSec 5 -ErrorAction Stop|Select-Object -First 65)
    if($rows.Count -gt 64){throw 'Current-use query exceeded 64 rows; incomplete scope deferred.'}
    $acceptedBytes=0;$matches=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $processRows=@(foreach($row in $rows){
        if([int]$row.ProcessId -eq $PID){continue}
        $command=[string]$row.CommandLine;$payloadBytes=[Text.Encoding]::UTF8.GetByteCount($command);$acceptedBytes+=$payloadBytes
        if($payloadBytes -gt 8192 -or $acceptedBytes -gt 65536){throw 'Current-use query payload exceeded bounds.'}
        $labels=@(foreach($entry in $definition.entries){$needle=if($Restore){[IO.Path]::GetFullPath((Join-Path $repoRoot $entry.destination))}else{$rootPrefix+$entry.source};if($command.IndexOf($needle,[StringComparison]::OrdinalIgnoreCase) -ge 0){$null=$matches.Add([string]$entry.source);$entry.source}})
        $hasher=[Security.Cryptography.SHA256]::Create()
        try{$digest=[BitConverter]::ToString($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($command))).Replace('-','').ToLowerInvariant()}finally{$hasher.Dispose()}
        [ordered]@{pid=[int]$row.ProcessId;labels=$labels;commandSha256=$digest;commandBytes=$payloadBytes}
    })
    $queryWall=([DateTime]::UtcNow-$queryStarted).TotalSeconds
    if($queryWall -gt 10){throw 'Current-use query exceeded ten-second complete bound.'}
    $run.processCheck=[ordered]@{status='scoped-complete';rows=$processRows;queryWallSeconds=$queryWall;acceptedBytes=$acceptedBytes;observerPid=$PID;limitation='Absolute canonical-prefix command lines only; relative paths, alternate aliases and bare working directories are not observed. No global host absence or process lifecycle claim.'}
    $run.status='validated';Publish-Receipt $false
    foreach($entry in $definition.entries){
        $action=[ordered]@{source=$entry.source;destination=$entry.destination;status='pending';reason=$null}
        $moveAttempted=$false;$from=$null;$to=$null
        try {
            if($matches.Contains([string]$entry.source)){throw 'Current command line uses this candidate; deferred.'}
            $original=Resolve-Contained $entry.source;$retained=Resolve-Contained $entry.destination
            $from=if($Restore){$retained}else{$original};$to=if($Restore){$original}else{$retained}
            if(Test-Path -LiteralPath $to){throw 'Destination already exists.'}
            $actual=Get-DirectoryIdentity $from
            if($actual.fileId -ne $entry.identity.fileId -or $actual.volume -ne $entry.identity.volume -or $actual.creation100ns -ne $entry.identity.creation100ns -or $actual.attributes -ne $entry.identity.attributes){throw 'Directory identity changed.'}
            if($Apply){
                $moveAttempted=$true
                [IO.Directory]::Move($from,$to)
                $post=Get-DirectoryIdentity $to
                if($post.fileId -ne $actual.fileId -or $post.volume -ne $actual.volume -or $post.creation100ns -ne $actual.creation100ns -or (Test-Path -LiteralPath $from)){throw 'Post-rename identity/location mismatch.'}
                $run.moved++;$action.status=if($Restore){'restored'}else{'relocated'}
            }else{$action.status='would-relocate'}
        }catch{
            $action.status='deferred';$action.reason=$_.Exception.Message;$run.deferred++
            if($moveAttempted -and (Test-Path -LiteralPath $to) -and -not(Test-Path -LiteralPath $from)){$run.actions+= $action;throw 'Post-move ambiguity: stopping without fallback.'}
        }
        $run.actions+= $action
        if($Apply){Publish-Receipt $false}
    }
    $run.status=if($run.deferred){'complete-with-deferrals'}elseif($Apply){'complete'}else{'dry-run-complete'}
    Publish-Receipt $true
    Write-Output "$($run.status): $($run.moved) directory renames, $($run.deferred) deferrals; zero bytes recovered."
}catch{
    $originalFailure=$_;$run.status='failed';$run.failure=$_.Exception.Message
    try{Publish-Receipt $true}catch{[Console]::Error.WriteLine("Receipt publication failed: $($_.Exception.Message)")}
    throw $originalFailure
}
