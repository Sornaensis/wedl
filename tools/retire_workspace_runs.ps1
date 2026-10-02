[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedManifestSha256,
    [Parameter(Mandatory=$true)][string]$Receipt,
    [switch]$Apply
)
$ErrorActionPreference='Stop'
$repoRoot=[IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\')
$rootPrefix=$repoRoot+'\'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Collections.Generic;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class WedlRetireNative {
 [StructLayout(LayoutKind.Sequential)] public struct Time {public uint Low,High;}
 [StructLayout(LayoutKind.Sequential)] public struct Info {public uint Attributes;public Time Creation,Access,Write;public uint Volume,SizeHigh,SizeLow,Links,IndexHigh,IndexLow;}
 [StructLayout(LayoutKind.Sequential)] struct Disposition {public uint Flags;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFileW(string path,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetFileInformationByHandle(SafeFileHandle h,out Info info);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetFileInformationByHandle(SafeFileHandle h,int kind,ref Disposition info,uint size);
 public sealed class Identity {public string fileId,volume,creation100ns;public uint attributes;}
 public sealed class FileStamp {public Identity identity;public long bytes;public string modifiedUtcTicks,sha256;}
 public sealed class Result {public string status="complete";public long visitedFiles,visitedDirectories,visitedLinks,deletedFiles,deletedDirectories,deletedLinks,deletedLogicalBytes,visitedLogicalBytes,errorCount;public List<string> errors=new List<string>();}
 // Controls can inject a barrier before a child is opened; production never sets this callback.
 public static Action<string> BeforeChildForControl;
 public static Action<string> BeforeAncestorForControl;
 static SafeFileHandle Open(string path,bool deleting) {
  var full=Path.GetFullPath(path);var extended=full.StartsWith(@"\\?\")?full:@"\\?\"+full;
  var h=CreateFileW(extended,deleting?0x00010080u:0x00000080u,1,IntPtr.Zero,3,0x02200000,IntPtr.Zero);
  if(h.IsInvalid){var error=Marshal.GetLastWin32Error();h.Dispose();throw new System.ComponentModel.Win32Exception(error,path);}
  return h;
 }
 static Info Read(SafeFileHandle h){Info i;if(!GetFileInformationByHandle(h,out i))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());return i;}
 static Identity Convert(Info i){return new Identity{fileId=i.IndexHigh.ToString("x8")+i.IndexLow.ToString("x8"),volume=i.Volume.ToString("x8"),creation100ns=(((ulong)i.Creation.High<<32)|i.Creation.Low).ToString(),attributes=i.Attributes};}
 public static Identity Inspect(string path){using(var h=Open(path,false)){return Convert(Read(h));}}
 public static FileStamp InspectFile(string path,bool hash){
  var full=Path.GetFullPath(path);var extended=full.StartsWith(@"\\?\")?full:@"\\?\"+full;
  using(var h=CreateFileW(extended,hash?0x00000081u:0x00000080u,1,IntPtr.Zero,3,0x02200000,IntPtr.Zero)){
   if(h.IsInvalid)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(),path);
   var i=Read(h);if((i.Attributes&(16|1024))!=0)throw new IOException("Diagnostic source is a directory or reparse point.");
   var stamp=new FileStamp{identity=Convert(i),bytes=((long)i.SizeHigh<<32)|i.SizeLow,modifiedUtcTicks=DateTime.FromFileTimeUtc((long)(((ulong)i.Write.High<<32)|i.Write.Low)).Ticks.ToString()};
   if(hash){using(var stream=new FileStream(h,FileAccess.Read))using(var sha=SHA256.Create()){stamp.sha256=BitConverter.ToString(sha.ComputeHash(stream)).Replace("-","").ToLowerInvariant();}}
   return stamp;
  }
 }
 public static void VerifyArchive(string path,string[] names,string[] hashes,long[] lengths){
  if(names.Length!=hashes.Length||names.Length!=lengths.Length)throw new IOException("Archive index arrays differ.");
  var expected=new Dictionary<string,int>(StringComparer.Ordinal);
  for(int n=0;n<names.Length;n++)expected.Add(names[n],n);
  var seen=new HashSet<string>(StringComparer.Ordinal);
  using(var zip=ZipFile.OpenRead(path)){
   if(zip.Entries.Count!=names.Length)throw new IOException("Archive member count mismatch.");
   foreach(var item in zip.Entries){int n;if(!seen.Add(item.FullName)||!expected.TryGetValue(item.FullName,out n)||item.Length!=lengths[n])throw new IOException("Archive member path/length mismatch.");
    using(var stream=item.Open())using(var sha=SHA256.Create()){var digest=BitConverter.ToString(sha.ComputeHash(stream)).Replace("-","").ToLowerInvariant();if(digest!=hashes[n])throw new IOException("Decompressed archive digest mismatch.");}
   }
  }
 }
 static void Unlink(SafeFileHandle h){
  // DELETE | POSIX_SEMANTICS | IGNORE_READONLY_ATTRIBUTE. Never mutate shared inode attributes.
  var d=new Disposition{Flags=0x13};
  if(!SetFileInformationByHandle(h,21,ref d,(uint)Marshal.SizeOf(typeof(Disposition))))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(),"Native no-follow unlink refused; no fallback.");
 }
 static void Error(Result r,string path,Exception e){r.errorCount++;if(r.errors.Count<64)r.errors.Add(path+": "+e.GetType().Name+": "+e.Message);}
 static void Walk(string path,string boundary,bool apply,Result r,int depth) {
  if(depth>256)throw new IOException("Traversal depth exceeds256.");
  var full=Path.GetFullPath(path);
  if(full!=boundary&&!full.StartsWith(boundary+"\\",StringComparison.OrdinalIgnoreCase))throw new IOException("Traversal escaped exact root.");
  using(var h=Open(full,apply)){
   var info=Read(h);bool directory=(info.Attributes&16)!=0,reparse=(info.Attributes&1024)!=0;
   if(reparse){r.visitedLinks++;if(apply){Unlink(h);r.deletedLinks++;}return;}
   if(directory){
    r.visitedDirectories++;
    // The no-follow handle denies write/delete sharing while names are enumerated.
    // A descendant substituted before its handle opens is inspected as a link, never traversed.
    var children=Directory.GetFileSystemEntries(full);Array.Sort(children,StringComparer.Ordinal);
    foreach(var child in children){
     try{if(BeforeChildForControl!=null)BeforeChildForControl(child);Walk(child,boundary,apply,r,depth+1);}
     catch(Exception ex){Error(r,child,ex);}
    }
    if(apply){Unlink(h);r.deletedDirectories++;}
   }else{
    var bytes=((long)info.SizeHigh<<32)|info.SizeLow;r.visitedFiles++;r.visitedLogicalBytes+=bytes;
    if(apply){Unlink(h);r.deletedFiles++;r.deletedLogicalBytes+=bytes;}
   }
  }
 }
 public static Result Purge(string path,Identity expected,bool apply,string workspace,Dictionary<string,Identity> expectedAncestors){
  var r=new Result();var boundary=Path.GetFullPath(path);
  var heldAncestors=new List<SafeFileHandle>();
  try{
   var repository=Path.GetFullPath(workspace).TrimEnd('\\');
   if(!boundary.StartsWith(repository+"\\",StringComparison.OrdinalIgnoreCase))throw new IOException("Exact root escaped workspace.");
   var parent=Path.GetDirectoryName(boundary);var paths=new List<string>{repository};
   var cursor=repository;
   foreach(var part in parent.Substring(repository.Length).TrimStart('\\').Split(new[]{'\\'},StringSplitOptions.RemoveEmptyEntries)){cursor=Path.Combine(cursor,part);paths.Add(cursor);}
   foreach(var ancestor in paths){
    if(BeforeAncestorForControl!=null)BeforeAncestorForControl(ancestor);
    var hold=Open(ancestor,false);heldAncestors.Add(hold);var actual=Convert(Read(hold));Identity sealedIdentity;
    if((actual.attributes&16)==0||(actual.attributes&1024)!=0||!expectedAncestors.TryGetValue(ancestor,out sealedIdentity)||actual.fileId!=sealedIdentity.fileId||actual.volume!=sealedIdentity.volume||actual.creation100ns!=sealedIdentity.creation100ns||actual.attributes!=sealedIdentity.attributes)throw new IOException("Retirement ancestor identity changed or became a link.");
   }
   using(var hold=Open(boundary,apply)){
    var actual=Convert(Read(hold));
    if(actual.fileId!=expected.fileId||actual.volume!=expected.volume||actual.creation100ns!=expected.creation100ns||actual.attributes!=expected.attributes||(actual.attributes&1024)!=0)throw new IOException("Exact root identity changed or root is a reparse point.");
    // Root is held throughout traversal; reuse the existing handle to avoid conflicting delete sharing.
    var children=Directory.GetFileSystemEntries(boundary);Array.Sort(children,StringComparer.Ordinal);r.visitedDirectories++;
    foreach(var child in children){try{if(BeforeChildForControl!=null)BeforeChildForControl(child);Walk(child,boundary,apply,r,1);}catch(Exception ex){Error(r,child,ex);}}
    if(apply){Unlink(hold);r.deletedDirectories++;}
   }
  }catch(Exception ex){Error(r,boundary,ex);}
  finally{for(int n=heldAncestors.Count-1;n>=0;n--)heldAncestors[n].Dispose();}
  if(r.errorCount>0)r.status=(r.deletedFiles+r.deletedDirectories+r.deletedLinks)>0?"partial":"refused";
  return r;
 }
}
'@
function Resolve-Contained([string]$relative){
 if([string]::IsNullOrWhiteSpace($relative)-or[IO.Path]::IsPathRooted($relative)-or$relative.Contains(':')-or$relative.Contains('*')-or$relative.Contains('?')){throw 'Invalid contained relative path.'}
 $parts=$relative.Replace('\','/').Split('/')
 if(@($parts|Where-Object{$_-in@('','.','..')}).Count){throw 'Invalid path component.'}
 $p=[IO.Path]::GetFullPath((Join-Path $repoRoot $relative))
 if(-not$p.StartsWith($rootPrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Path escaped repository.'}
 $cursor=$repoRoot
 foreach($part in $parts){$cursor=Join-Path $cursor $part;if(Test-Path -LiteralPath $cursor){if((Get-Item -LiteralPath $cursor -Force).Attributes-band[IO.FileAttributes]::ReparsePoint){throw "Reparse ancestor: $cursor"}}}
 return $p
}
$receiptAbsolute=Resolve-Contained $Receipt
if(-not$Receipt.StartsWith('output/repository-cleanup-20261002/legacy-retirement/',[StringComparison]::Ordinal)-or(Test-Path -LiteralPath $receiptAbsolute)){throw 'Receipt must be a new retirement evidence path.'}
$run=[ordered]@{taskId='06ca90b7-b2d9-40f9-a829-345a720b0192';mode=$(if($Apply){'apply'}else{'dry-run'});manifestSha256=$ExpectedManifestSha256.ToLowerInvariant();startedUtc=[DateTime]::UtcNow.ToString('o');status='preflight';roots=@();deletedFiles=0L;deletedDirectories=0L;deletedLinks=0L;deletedLogicalBytes=0L;failure=$null}
function Publish-Receipt([bool]$final){
 $run.finishedUtc=[DateTime]::UtcNow.ToString('o')
 $path=if($final){$receiptAbsolute}else{$receiptAbsolute+'.checkpoint-'+$run.roots.Count.ToString('D6')+'.json'}
 $payload=if($final){$run}else{[ordered]@{status=$run.status;finishedUtc=$run.finishedUtc;manifestSha256=$run.manifestSha256;rootCount=$run.roots.Count;deletedFiles=$run.deletedFiles;deletedDirectories=$run.deletedDirectories;deletedLinks=$run.deletedLinks;deletedLogicalBytes=$run.deletedLogicalBytes;lastRoot=$(if($run.roots.Count){$run.roots[-1]}else{$null})}}
 $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($payload|ConvertTo-Json -Depth 10))
 $null=Resolve-Contained $Receipt
 $stream=[IO.FileStream]::new($path+'.pending',[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)}finally{$stream.Dispose()}
 [IO.File]::Move($path+'.pending',$path)
}
try{
 $manifestAbsolute=Resolve-Contained $Manifest
 if((Get-FileHash -LiteralPath $manifestAbsolute).Hash.ToLowerInvariant()-ne$ExpectedManifestSha256.ToLowerInvariant()){throw 'Reviewed manifest digest mismatch.'}
 $definition=Get-Content -LiteralPath $manifestAbsolute -Raw|ConvertFrom-Json
 if($definition.taskId-ne$run.taskId-or$definition.schemaVersion-ne1-or[IO.Path]::GetFullPath($definition.root).TrimEnd('\')-ne$repoRoot-or$definition.workspaceId-ne'376d138f-4edb-4516-919b-22ca0f064d34'-or(Get-Content -LiteralPath (Join-Path $repoRoot '.hmem.workspace') -Raw).Trim()-ne$definition.workspaceId){throw 'Repository/workspace/task mismatch.'}
 $archive=Resolve-Contained $definition.archive.path;$indexPath=Resolve-Contained $definition.archive.indexPath
 if((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant()-ne$definition.archive.sha256-or(Get-FileHash -LiteralPath $indexPath).Hash.ToLowerInvariant()-ne$definition.archive.indexSha256){throw 'Archive/index digest mismatch.'}
 $index=Get-Content -LiteralPath $indexPath -Raw|ConvertFrom-Json
 [WedlRetireNative]::VerifyArchive($archive,[string[]]@($index.entries.member),[string[]]@($index.entries.retainedSha256),[long[]]@($index.entries.retainedBytes))
 $rootDiagnostics=[Collections.Generic.Dictionary[string,Collections.Generic.List[object]]]::new([StringComparer]::Ordinal)
 foreach($entry in $index.entries){if(-not$rootDiagnostics.ContainsKey([string]$entry.root)){$rootDiagnostics.Add([string]$entry.root,[Collections.Generic.List[object]]::new())};$rootDiagnostics[[string]$entry.root].Add($entry)}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
 $ancestors=[Collections.Generic.Dictionary[string,WedlRetireNative+Identity]]::new([StringComparer]::OrdinalIgnoreCase)
 foreach($saved in $definition.ancestors){
  $p=if($saved.path-eq'.'){$repoRoot}else{Resolve-Contained $saved.path}
  $id=[WedlRetireNative+Identity]::new();$id.fileId=$saved.identity.fileId;$id.volume=$saved.identity.volume;$id.creation100ns=$saved.identity.creation100ns;$id.attributes=$saved.identity.attributes
  $ancestors.Add($p,$id)
 }
 foreach($entry in $definition.entries){
  if(-not$seen.Add([string]$entry.path)){throw 'Duplicate exact root.'}
  $allowed=$entry.path-match'^output/repository-cleanup-20261002/legacy-test-runs/[^/]+$'-or$entry.path-eq'_aef_review_fix_mig_clean'
  $control=$definition.control-eq$true-and$entry.path-match'^output/repository-cleanup-20261002/legacy-retirement/controls/owned-[a-z0-9-]+$'
  if(-not($allowed-or$control)){throw 'Root is outside retired historical boundary.'}
  $absolute=Resolve-Contained $entry.path
  foreach($worktree in $definition.registeredWorktrees){$w=[IO.Path]::GetFullPath($worktree).TrimEnd('\');if($w-eq$absolute-or$w.StartsWith($absolute+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Registered worktree lies inside retirement root.'}}
  $actual=[WedlRetireNative]::Inspect($absolute)
  if($actual.fileId-ne$entry.identity.fileId-or$actual.volume-ne$entry.identity.volume-or$actual.creation100ns-ne$entry.identity.creation100ns-or$actual.attributes-ne$entry.identity.attributes){throw 'Exact root identity changed.'}
 }
 $queryStarted=[DateTime]::UtcNow
 $prefixes=@($rootPrefix+'output\repository-cleanup-20261002\legacy-test-runs\',$rootPrefix+'_aef_review_fix_mig_clean')
 if($definition.control){$prefixes+= $rootPrefix+'output\repository-cleanup-20261002\legacy-retirement\controls\'}
 $clauses=@($prefixes|ForEach-Object{$p=$_.Replace('\','\\').Replace("'","\'");"CommandLine LIKE '%$p%'"})
 $rows=@(Get-CimInstance -Namespace root/cimv2 -Query ('SELECT ProcessId, CommandLine FROM Win32_Process WHERE '+($clauses-join' OR ')) -OperationTimeoutSec 5 -ErrorAction Stop|Select-Object -First 65)
 if($rows.Count-gt64){throw 'Current-use query exceeded row bound.'}
 $matches=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);$payloadBytes=0
 $processRows=@(foreach($row in $rows){
  if([int]$row.ProcessId-eq$PID){continue}
  $cmd=[string]$row.CommandLine;$bytes=[Text.Encoding]::UTF8.GetByteCount($cmd);$payloadBytes+=$bytes
  if($bytes-gt8192-or$payloadBytes-gt65536){throw 'Current-use query exceeded payload bound.'}
  $labels=@(foreach($entry in $definition.entries){$p=[IO.Path]::GetFullPath((Join-Path $repoRoot $entry.path));if($cmd.IndexOf($p,[StringComparison]::OrdinalIgnoreCase)-ge0){$null=$matches.Add([string]$entry.path);$entry.path}})
  $h=[Security.Cryptography.SHA256]::Create();try{$sha=[BitConverter]::ToString($h.ComputeHash([Text.Encoding]::UTF8.GetBytes($cmd))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose()}
  [ordered]@{pid=[int]$row.ProcessId;labels=$labels;commandSha256=$sha;commandBytes=$bytes}
 })
 $queryWall=([DateTime]::UtcNow-$queryStarted).TotalSeconds
 if($queryWall-gt10){throw 'Current-use query exceeded complete time bound.'}
 $run.processCheck=[ordered]@{status='scoped-complete';rows=$processRows;queryWallSeconds=$queryWall;observerPid=$PID;limitation='Absolute canonical prefixes only; aliases/relative/bare working directories are not observed; no global inactivity claim.'}
 $run.status='validated';Publish-Receipt $false
 foreach($entry in $definition.entries){
  $absolute=Resolve-Contained $entry.path
  if($matches.Contains([string]$entry.path)){$result=[ordered]@{path=$entry.path;status='deferred-current-use';deletedFiles=0;deletedDirectories=0;deletedLinks=0;deletedLogicalBytes=0}}
  else{
   $diagnostics=if($rootDiagnostics.ContainsKey([string]$entry.path)){$rootDiagnostics[[string]$entry.path]}else{@()}
   foreach($saved in $diagnostics){
    $p=Resolve-Contained $saved.currentPath
    $item=[WedlRetireNative]::InspectFile($p,[bool]$saved.originalSha256)
    if($item.bytes-ne$saved.originalBytes-or$item.modifiedUtcTicks-ne[string]$saved.originalModifiedUtcTicks){throw 'Diagnostic source metadata changed after archive.'}
    if($saved.originalSha256-and$item.sha256-ne$saved.originalSha256){throw 'Diagnostic source digest changed after archive.'}
    if($saved.originalIdentity-and($item.identity.fileId-ne$saved.originalIdentity.fileId-or$item.identity.volume-ne$saved.originalIdentity.volume-or$item.identity.creation100ns-ne$saved.originalIdentity.creation100ns)){throw 'Diagnostic source native identity changed after archive.'}
    if($saved.nativeFileId-and$item.identity.fileId-ne([Convert]::ToUInt64([string]$saved.nativeFileId)).ToString('x16')){throw 'Reconciled diagnostic native file identity changed after archive.'}
   }
   $identity=[WedlRetireNative+Identity]::new();$identity.fileId=$entry.identity.fileId;$identity.volume=$entry.identity.volume;$identity.creation100ns=$entry.identity.creation100ns;$identity.attributes=$entry.identity.attributes
   $native=[WedlRetireNative]::Purge($absolute,$identity,$Apply,$repoRoot,$ancestors)
   $result=[ordered]@{path=$entry.path;status=$native.status;visitedFiles=$native.visitedFiles;visitedDirectories=$native.visitedDirectories;visitedLinks=$native.visitedLinks;visitedLogicalBytes=$native.visitedLogicalBytes;deletedFiles=$native.deletedFiles;deletedDirectories=$native.deletedDirectories;deletedLinks=$native.deletedLinks;deletedLogicalBytes=$native.deletedLogicalBytes;errorCount=$native.errorCount;errors=$native.errors;rootPresent=(Test-Path -LiteralPath $absolute)}
  }
  $run.roots+=$result;$run.deletedFiles+=$result.deletedFiles;$run.deletedDirectories+=$result.deletedDirectories;$run.deletedLinks+=$result.deletedLinks;$run.deletedLogicalBytes+=$result.deletedLogicalBytes
  Publish-Receipt $false
  Write-Output ("Root "+$run.roots.Count+"/"+$definition.entries.Count+": "+$result.status+"; deleted logical bytes="+$run.deletedLogicalBytes)
 }
 $run.status=if(@($run.roots|Where-Object status -NE 'complete').Count){'complete-with-deferrals'}elseif($Apply){'complete'}else{'dry-run-complete'}
 Publish-Receipt $true
}catch{$run.status='failed';$run.failure=$_.Exception.Message;try{Publish-Receipt $true}catch{[Console]::Error.WriteLine($_.Exception.Message)};throw}
