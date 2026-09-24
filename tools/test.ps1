[CmdletBinding()]
param(
    [ValidateRange(1, 300)]
    [int]$TimeoutSeconds = 300
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$runId = [guid]::NewGuid().ToString("N")
$temporaryRoot = [System.IO.Path]::GetTempPath()
$testTempRoot = Join-Path $root ".wedl-test-tmp-$runId"
$started = [System.Diagnostics.Stopwatch]::StartNew()

$previousPythonPath = $env:PYTHONPATH
$previousPlugins = $env:PYTEST_PLUGINS
$previousDiagnostics = $env:WEDL_TEST_DIAGNOSTICS
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$script:jobs = New-Object System.Collections.ArrayList
$script:artifactPaths = New-Object System.Collections.ArrayList
$script:timedOut = $false
$script:passed = $false
$script:exitCode = 1
$script:failureExitCode = 1
$script:phase = "preparation"

$runnerPaths = @($PSScriptRoot, (Join-Path $root "src"))
if ($previousPythonPath) { $runnerPaths += $previousPythonPath }
$env:PYTHONPATH = $runnerPaths -join [System.IO.Path]::PathSeparator
$env:PYTEST_PLUGINS = if ($previousPlugins) { "$previousPlugins,pytest_timeout_diagnostics" } else { "pytest_timeout_diagnostics" }
$venvPython = Join-Path $root ".venv\Scripts\python.exe"
$python = if (Test-Path -LiteralPath $venvPython) { $venvPython } else { "python" }

function Assert-Budget {
    if ($started.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
        $script:timedOut = $true
        throw "Normal test suite exceeded its $TimeoutSeconds-second deadline during $script:phase."
    }
}

function Quote-Argument([string]$value) {
    if ($value.Contains('"')) { throw "A pytest argument contains a double quote." }
    return '"' + $value + '"'
}

function New-RunnerPaths([string]$name) {
    $temp = Join-Path $testTempRoot "$name-temp"
    $basetemp = Join-Path $testTempRoot "$name-basetemp"
    $stdout = Join-Path $temporaryRoot "wedl-pytest-$runId-$name.stdout"
    $stderr = Join-Path $temporaryRoot "wedl-pytest-$runId-$name.stderr"
    $diagnostics = Join-Path $temporaryRoot "wedl-pytest-$runId-$name.json"
    $null = New-Item -ItemType Directory -Path $temp -Force
    foreach ($path in @($stdout, $stderr, $diagnostics, ([System.IO.Path]::ChangeExtension($diagnostics, "tmp")))) {
        $null = $script:artifactPaths.Add($path)
    }
    return [pscustomobject]@{
        Name = $name
        Temp = $temp
        Basetemp = $basetemp
        Stdout = $stdout
        Stderr = $stderr
        Diagnostics = $diagnostics
    }
}

function Start-RunnerProcess([string]$name, [string[]]$selection, [bool]$collect) {
    Assert-Budget
    if ($selection.Count -eq 0) { throw "The $name selection is empty." }
    $paths = New-RunnerPaths $name
    $arguments = @("-m", "pytest", "-q", "--durations=25")
    if ($collect) { $arguments += "--collect-only" }
    $arguments += $selection
    $arguments += "--basetemp=$($paths.Basetemp)"
    $quoted = @($arguments | ForEach-Object { Quote-Argument $_ })
    $env:TEMP = $paths.Temp
    $env:TMP = $paths.Temp
    $env:WEDL_TEST_DIAGNOSTICS = $paths.Diagnostics
    try {
        $process = Start-Process -FilePath $python -ArgumentList $quoted -RedirectStandardOutput $paths.Stdout -RedirectStandardError $paths.Stderr -NoNewWindow -PassThru
    } finally {
        $env:TEMP = $previousTemp
        $env:TMP = $previousTmp
        $env:WEDL_TEST_DIAGNOSTICS = $previousDiagnostics
    }
    $processHandle = $process.Handle
    $job = [pscustomobject]@{ Name = $name; Process = $process; Handle = $processHandle; Paths = $paths; Selection = $selection }
    $null = $script:jobs.Add($job)
    return $job
}

function Stop-ProcessTree($process, [bool]$includeExitedDescendants = $false) {
    if ($null -eq $process) { return }
    $process.Refresh()
    if ($process.HasExited) {
        if (-not $includeExitedDescendants) { return }
        try {
            $startedAt = $process.StartTime
            $known = [System.Collections.Generic.HashSet[int]]::new()
            $null = $known.Add($process.Id)
            $descendants = New-Object System.Collections.Generic.List[int]
            $snapshot = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, CreationDate -ErrorAction Stop)
            do {
                $found = $false
                foreach ($item in $snapshot) {
                    if ($known.Contains([int]$item.ParentProcessId) -and
                        -not $known.Contains([int]$item.ProcessId) -and
                        $item.CreationDate -ge $startedAt) {
                        $null = $known.Add([int]$item.ProcessId)
                        $descendants.Add([int]$item.ProcessId)
                        $found = $true
                    }
                }
            } while ($found)
            foreach ($id in ($descendants.ToArray() | Sort-Object -Descending)) {
                Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
            }
        } catch {
            [Console]::Error.WriteLine("Could not confirm descendant termination for exited PID $($process.Id): $($_.Exception.Message)")
        }
        return
    }
    try {
        $killer = Start-Process -FilePath "taskkill.exe" -ArgumentList @("/pid", $process.Id, "/t", "/f") -NoNewWindow -PassThru
        $null = $killer.WaitForExit(5000)
        $null = $process.WaitForExit(5000)
    } catch {
        [Console]::Error.WriteLine("Could not confirm process-tree termination for PID $($process.Id): $($_.Exception.Message)")
    }
}

function Stop-LiveJobs([bool]$includeExitedDescendants = $false) {
    foreach ($job in $script:jobs) {
        Stop-ProcessTree $job.Process $includeExitedDescendants
    }
}

function Wait-RunnerProcess($job) {
    while ($true) {
        $job.Process.Refresh()
        if ($job.Process.HasExited) {
            $null = $job.Process.WaitForExit()
            return
        }
        Assert-Budget
        $remaining = [math]::Floor(($TimeoutSeconds - $started.Elapsed.TotalSeconds) * 1000)
        Start-Sleep -Milliseconds ([math]::Min(250, [math]::Max(1, $remaining)))
    }
}

function Read-JobText($job, [string]$kind) {
    $path = if ($kind -eq "stdout") { $job.Paths.Stdout } else { $job.Paths.Stderr }
    if (Test-Path -LiteralPath $path) { return Get-Content -LiteralPath $path -Raw }
    return ""
}

function Read-Diagnostics($job) {
    if (-not (Test-Path -LiteralPath $job.Paths.Diagnostics)) { return $null }
    try { return Get-Content -LiteralPath $job.Paths.Diagnostics -Raw | ConvertFrom-Json }
    catch { return $null }
}

function Collect-Nodes($job) {
    Wait-RunnerProcess $job
    if ($job.Process.ExitCode -ne 0) {
        $script:failureExitCode = $job.Process.ExitCode
        throw "$($job.Name) exited $($job.Process.ExitCode) during collection."
    }
    $stdout = Read-JobText $job "stdout"
    $summary = [regex]::Match($stdout, '(?m)^(?<count>\d+) tests? collected\b')
    if (-not $summary.Success) { throw "$($job.Name) has no parseable collection count." }
    $nodes = New-Object System.Collections.Generic.List[string]
    foreach ($line in ($stdout -split '\r?\n')) {
        $node = $line.Trim()
        if ($node -match '^tests[/\\].+::.+$') { $nodes.Add($node.Replace('\', '/')) }
    }
    if ($nodes.Count -ne [int]$summary.Groups["count"].Value) {
        throw "$($job.Name) collected $($summary.Groups['count'].Value) but exposed $($nodes.Count) node IDs."
    }
    return $nodes.ToArray()
}

function New-NodeCounts([string[]]$nodes) {
    $counts = [System.Collections.Generic.Dictionary[string,int]]::new([System.StringComparer]::Ordinal)
    foreach ($node in $nodes) {
        if ($counts.ContainsKey($node)) { $counts[$node]++ } else { $counts.Add($node, 1) }
    }
    return $counts
}

function Assert-CollectionParity([string[]]$serial, [string[]]$a, [string[]]$b) {
    $reference = New-NodeCounts $serial
    $combined = New-NodeCounts (@($a) + @($b))
    if ($serial.Count -ne ($a.Count + $b.Count) -or $reference.Count -ne $serial.Count -or $combined.Count -ne $serial.Count) {
        throw "Serial and shard node counts differ or contain duplicate IDs."
    }
    foreach ($node in $reference.Keys) {
        if (-not $combined.ContainsKey($node) -or $combined[$node] -ne $reference[$node]) {
            throw "Serial and shard node-ID multisets differ at $node."
        }
    }
}

function Get-AccountedCount([string]$stdout, [string]$name) {
    $summary = [regex]::Match($stdout, '(?m)^(?<outcomes>\d+ [A-Za-z]+(?:, \d+ [A-Za-z]+)*) in [\d.]+s')
    if (-not $summary.Success) { throw "$name has no parseable pytest outcome summary." }
    $accounted = 0
    foreach ($part in ($summary.Groups["outcomes"].Value -split ', ')) {
        if ($part -notmatch '^(\d+) (passed|skipped|xfailed|xpassed|warnings?)$') {
            throw "$name has an unknown outcome token: $part."
        }
        if ($Matches[2] -ne "warning" -and $Matches[2] -ne "warnings") {
            $accounted += [int]$Matches[1]
        }
    }
    return $accounted
}

function Write-JobDiagnostics($job) {
    $stdout = Read-JobText $job "stdout"
    $stderr = Read-JobText $job "stderr"
    if ($stdout.Length -gt 6000) {
        $stdout = $stdout.Substring(0, 2000) + [Environment]::NewLine +
                  "[middle omitted; full output at $($job.Paths.Stdout)]" + [Environment]::NewLine +
                  $stdout.Substring($stdout.Length - 2000)
    }
    if ($stdout) { [Console]::Error.WriteLine("$($job.Name) stdout:$([Environment]::NewLine)$($stdout.TrimEnd())") }
    if ($stderr) { [Console]::Error.WriteLine("$($job.Name) stderr:$([Environment]::NewLine)$($stderr.TrimEnd())") }
    $snapshot = Read-Diagnostics $job
    if ($snapshot) {
        $active = if ($snapshot.active) { $snapshot.active } else { "none" }
        [Console]::Error.WriteLine("$($job.Name) active test: $active")
        if ($snapshot.completed) {
            foreach ($item in ($snapshot.completed | Select-Object -First 25)) {
                [Console]::Error.WriteLine(("  {0:N3}s  {1} ({2})" -f $item.duration, $item.nodeid, $item.outcome))
            }
        }
    }
}

function Remove-SuccessArtifacts {
    $actual = [System.IO.Path]::GetFullPath($testTempRoot)
    $expected = [System.IO.Path]::GetFullPath((Join-Path $root ".wedl-test-tmp-$runId"))
    if (-not [System.StringComparer]::OrdinalIgnoreCase.Equals($actual, $expected) -or
        -not $actual.StartsWith(([System.IO.Path]::GetFullPath($root) + [System.IO.Path]::DirectorySeparatorChar), [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove an unexpected test temp path: $actual"
    }
    foreach ($path in $script:artifactPaths) {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $testTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

try {
    Push-Location $root
    $null = New-Item -ItemType Directory -Path $testTempRoot

    $script:phase = "serial collection"
    $serialNodes = @(Collect-Nodes (Start-RunnerProcess "collect-serial" @("tests") $true))
    if ($serialNodes.Count -eq 0) { throw "The normal suite collected no test nodes." }
    $files = New-Object System.Collections.Generic.List[string]
    $seenFiles = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $testsPrefix = [System.IO.Path]::GetFullPath((Join-Path $root "tests")) + [System.IO.Path]::DirectorySeparatorChar
    foreach ($node in $serialNodes) {
        $file = ($node -split '::', 2)[0]
        $full = [System.IO.Path]::GetFullPath((Join-Path $root $file))
        if (-not $full.StartsWith($testsPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not $file.EndsWith(".py", [System.StringComparison]::OrdinalIgnoreCase) -or
            -not (Test-Path -LiteralPath $full -PathType Leaf)) {
            throw "A collected node has an invalid test file path: $node"
        }
        if ($seenFiles.Add($file)) { $files.Add($file) }
    }
    $heavy = @(
        "tests/test_changeset.py",
        "tests/test_generational_api.py",
        "tests/test_migration_recovery.py",
        "tests/test_spatial_authoring.py",
        "tests/test_spatial_explorer_api.py"
    )
    $aFiles = @($heavy | Where-Object { $seenFiles.Contains($_) })
    $heavySet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($file in $aFiles) { $null = $heavySet.Add($file) }
    $bFiles = @($files | Where-Object { -not $heavySet.Contains($_) })
    if ($aFiles.Count -eq 0 -or $bFiles.Count -eq 0) { throw "The live test inventory cannot form both required shards." }

    $script:phase = "shard A collection"
    $aNodes = @(Collect-Nodes (Start-RunnerProcess "collect-a" $aFiles $true))
    $script:phase = "shard B collection"
    $bNodes = @(Collect-Nodes (Start-RunnerProcess "collect-b" $bFiles $true))
    Assert-CollectionParity $serialNodes $aNodes $bNodes
    Assert-Budget
    $collectionElapsed = $started.Elapsed.TotalSeconds
    Write-Output "NORMAL TEST COLLECTION: $($serialNodes.Count) nodes; shard A $($aNodes.Count), shard B $($bNodes.Count); exact node-ID parity."

    $script:phase = "parallel execution"
    $runA = Start-RunnerProcess "run-a" $aFiles $false
    $runB = Start-RunnerProcess "run-b" $bFiles $false
    while ($true) {
        $runA.Process.Refresh()
        $runB.Process.Refresh()
        if ($runA.Process.HasExited -and $runA.Process.ExitCode -ne 0) {
            $script:failureExitCode = $runA.Process.ExitCode
            throw "Shard A exited $($runA.Process.ExitCode)."
        }
        if ($runB.Process.HasExited -and $runB.Process.ExitCode -ne 0) {
            $script:failureExitCode = $runB.Process.ExitCode
            throw "Shard B exited $($runB.Process.ExitCode)."
        }
        if ($runA.Process.HasExited -and $runB.Process.HasExited) { break }
        Assert-Budget
        $remaining = [math]::Floor(($TimeoutSeconds - $started.Elapsed.TotalSeconds) * 1000)
        Start-Sleep -Milliseconds ([math]::Min(250, [math]::Max(1, $remaining)))
    }
    $null = $runA.Process.WaitForExit()
    $null = $runB.Process.WaitForExit()
    $executionElapsed = $started.Elapsed.TotalSeconds
    $aStdout = Read-JobText $runA "stdout"
    $bStdout = Read-JobText $runB "stdout"
    $aStderr = Read-JobText $runA "stderr"
    $bStderr = Read-JobText $runB "stderr"
    $aAccounted = Get-AccountedCount $aStdout "Shard A"
    $bAccounted = Get-AccountedCount $bStdout "Shard B"
    if ($aAccounted -ne $aNodes.Count -or $bAccounted -ne $bNodes.Count) {
        throw "Executed outcome counts do not match the collected shard node counts."
    }
    Assert-Budget
    Write-Output "SHARD A ($aAccounted nodes):"
    if ($aStdout) { Write-Output $aStdout.TrimEnd() }
    Write-Output "SHARD B ($bAccounted nodes):"
    if ($bStdout) { Write-Output $bStdout.TrimEnd() }
    if ($aStderr) { [Console]::Error.WriteLine($aStderr.TrimEnd()) }
    if ($bStderr) { [Console]::Error.WriteLine($bStderr.TrimEnd()) }
    $elapsed = [math]::Round($started.Elapsed.TotalSeconds, 3)
    if ($elapsed -ge $TimeoutSeconds) {
        $script:timedOut = $true
        throw "Normal test suite exceeded its $TimeoutSeconds-second deadline during aggregation."
    }
    $collectionSeconds = [math]::Round($collectionElapsed, 3)
    $executionSeconds = [math]::Round($executionElapsed - $collectionElapsed, 3)
    $aggregationSeconds = [math]::Round($elapsed - $executionElapsed, 3)
    Write-Output "NORMAL TEST PHASES: collection $collectionSeconds seconds; execution $executionSeconds seconds; aggregation $aggregationSeconds seconds."
    Write-Output "NORMAL TEST SUITE PASSED: $($aAccounted + $bAccounted) accounted nodes; elapsed $elapsed seconds (cap $TimeoutSeconds seconds)."
    $script:passed = $true
    $script:exitCode = 0
} catch {
    $message = $_.Exception.Message
    if ($script:timedOut) { $script:exitCode = 124 } else { $script:exitCode = $script:failureExitCode }
    Stop-LiveJobs $true
    [Console]::Error.WriteLine("$message Elapsed $([math]::Round($started.Elapsed.TotalSeconds, 3)) seconds.")
    foreach ($job in $script:jobs) { Write-JobDiagnostics $job }
    [Console]::Error.WriteLine("Test artifacts retained at $testTempRoot and under $temporaryRoot with run ID $runId.")
} finally {
    Stop-LiveJobs
    Pop-Location -ErrorAction SilentlyContinue
    $env:PYTHONPATH = $previousPythonPath
    $env:PYTEST_PLUGINS = $previousPlugins
    $env:WEDL_TEST_DIAGNOSTICS = $previousDiagnostics
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    if ($script:passed) {
        try { Remove-SuccessArtifacts }
        catch {
            [Console]::Error.WriteLine("Successful test artifacts could not be removed safely: $($_.Exception.Message)")
            $script:exitCode = 1
        }
    }
}
exit $script:exitCode
