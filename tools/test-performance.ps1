[CmdletBinding()]
param(
    [ValidateRange(1, 1800)]
    [int]$TimeoutSeconds = 1800,
    [switch]$ValidateOnly
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$runId = [guid]::NewGuid().ToString("N")
$temporaryRoot = [System.IO.Path]::GetTempPath()
$scratch = Join-Path $temporaryRoot "wedl-performance-$runId"
$previousPythonPath = $env:PYTHONPATH
$previousPlugins = $env:PYTEST_PLUGINS
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$previousOutcomes = $env:WEDL_TEST_OUTCOMES
$previousContext = $env:WEDL_PERFORMANCE_CONTEXT
$previousCodeRoot = $env:WEDL_CODE_ROOT
$previousBytecode = $env:PYTHONDONTWRITEBYTECODE
$started = [System.Diagnostics.Stopwatch]::StartNew()
$script:activeProcess = $null
$script:activeStartedAt = [datetime]::MinValue
$script:stageProcesses = New-Object System.Collections.ArrayList
$script:phase = "enrollment"
$script:success = $false
$script:exitCode = 1
$runnerPaths = @($PSScriptRoot, (Join-Path $root "src"))
if ($previousPythonPath) { $runnerPaths += $previousPythonPath }
$env:PYTHONPATH = $runnerPaths -join [System.IO.Path]::PathSeparator
$env:PYTEST_PLUGINS = if ($previousPlugins) { "$previousPlugins,pytest_timeout_diagnostics" } else { "pytest_timeout_diagnostics" }
$venvPython = Join-Path $root ".venv\Scripts\python.exe"
$python = if (Test-Path -LiteralPath $venvPython) { $venvPython } else { "python" }

function Assert-Budget {
    if ($started.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
        $script:exitCode = 124
        throw "Performance suite reached its $TimeoutSeconds-second hard deadline during $script:phase."
    }
}

function Quote-Argument([string]$value) {
    if ($value.Contains('"')) { throw "A performance argument contains a double quote." }
    return '"' + $value + '"'
}

function Stop-ExitedDescendants($process, [datetime]$startedAt) {
    $exitedAt = $process.ExitTime
    if ($null -eq $exitedAt -or $exitedAt -lt $startedAt) {
        throw "Original process exit time is unavailable; descendant cleanup is ambiguous."
    }
    $snapshot = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, CreationDate -ErrorAction Stop)
    if (@($snapshot | Where-Object { [int]$_.ProcessId -eq $process.Id }).Count -ne 0) {
        throw "Exited parent PID was reused before descendant cleanup."
    }
    if (@($snapshot | Where-Object {
        [int]$_.ParentProcessId -eq $process.Id -and [datetime]$_.CreationDate -gt $exitedAt
    }).Count -ne 0) {
        throw "A process reports the original parent PID after its exit time; refusing ambiguous cleanup."
    }
    $known = [System.Collections.Generic.HashSet[int]]::new()
    $null = $known.Add($process.Id)
    $createdById = [System.Collections.Generic.Dictionary[int,datetime]]::new()
    $createdById[[int]$process.Id] = [datetime]$startedAt
    $descendants = New-Object System.Collections.ArrayList
    for ($depth = 0; $depth -lt 32; $depth++) {
        $found = $false
        foreach ($item in $snapshot) {
            $id = [int]$item.ProcessId
            $parentId = [int]$item.ParentProcessId
            $created = [datetime]$item.CreationDate
            if ($known.Contains($parentId) -and -not $known.Contains($id) -and
                $created -ge $createdById[$parentId] -and ($parentId -ne $process.Id -or $created -le $exitedAt)) {
                if ($descendants.Count -ge 128) { throw "Exited process descendant bound exceeded." }
                $null = $known.Add($id)
                $createdById[$id] = $created
                $null = $descendants.Add([pscustomobject]@{ Id = $id; Created = $created })
                $found = $true
            }
        }
        if (-not $found) { break }
    }
    if ($depth -eq 32) {
        foreach ($item in $snapshot) {
            $id = [int]$item.ProcessId
            $parentId = [int]$item.ParentProcessId
            $created = [datetime]$item.CreationDate
            if ($known.Contains($parentId) -and -not $known.Contains($id) -and
                $created -ge $createdById[$parentId] -and ($parentId -ne $process.Id -or $created -le $exitedAt)) {
                [Console]::Error.WriteLine("Exited PID $($process.Id) descendant sweep reached 32 levels with valid unseen descendant PID $id; cleanup may leave an orphan.")
                break
            }
        }
    }
    for ($index = $descendants.Count - 1; $index -ge 0; $index--) {
        $child = $descendants[$index]
        $live = Get-Process -Id $child.Id -ErrorAction SilentlyContinue
        if ($live) {
            if ([math]::Abs(($live.StartTime - $child.Created).TotalMilliseconds) -gt 50) {
                throw "Descendant PID $($child.Id) creation time differs from the process snapshot."
            }
            Stop-Process -Id $child.Id -Force -ErrorAction Stop
            $null = $live.WaitForExit(5000)
            if (-not $live.HasExited) { throw "Could not confirm descendant termination for PID $($child.Id)." }
        }
    }
}

function Stop-ProcessTree($process, [bool]$includeExitedDescendants, [datetime]$startedAt) {
    if ($null -eq $process) { return }
    $process.Refresh()
    if ($process.HasExited) {
        if ($includeExitedDescendants) {
            try { Stop-ExitedDescendants $process $startedAt }
            catch { [Console]::Error.WriteLine("Exited PID $($process.Id) descendant cleanup could not be verified: $($_.Exception.Message)") }
        }
        return
    }
    try {
        $killer = Start-Process -FilePath "taskkill.exe" -ArgumentList @("/pid", $process.Id, "/t", "/f") -WindowStyle Hidden -PassThru
        $null = $killer.WaitForExit(5000)
        $null = $process.WaitForExit(5000)
    } catch {
        [Console]::Error.WriteLine("Process-tree termination failed for PID $($process.Id): $($_.Exception.Message)")
    }
    $process.Refresh()
    if (-not $process.HasExited) {
        try {
            $live = Get-Process -Id $process.Id -ErrorAction Stop
            if ($live.StartTime -ne $startedAt) { throw "PID was reused" }
            Stop-Process -Id $process.Id -Force -ErrorAction Stop
        } catch {
            [Console]::Error.WriteLine("Could not terminate stage PID $($process.Id): $($_.Exception.Message)")
        }
    }
    $process.Refresh()
    if ($includeExitedDescendants -and $process.HasExited) {
        try { Stop-ExitedDescendants $process $startedAt }
        catch { [Console]::Error.WriteLine("PID $($process.Id) descendant cleanup could not be verified: $($_.Exception.Message)") }
    }
}

function Invoke-Stage([string]$name, [string[]]$arguments, [string]$resultPath, [bool]$requireJson = $true) {
    Assert-Budget
    $script:phase = $name
    $stageRoot = Join-Path $scratch $name
    $temp = Join-Path $stageRoot "temp"
    $null = New-Item -ItemType Directory -Path $temp -Force
    $stdout = Join-Path $stageRoot "stdout.txt"
    $stderr = Join-Path $stageRoot "stderr.txt"
    $env:TEMP = $temp
    $env:TMP = $temp
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $quoted = @($arguments | ForEach-Object { Quote-Argument $_ })
        $script:activeProcess = Start-Process -FilePath $python -ArgumentList $quoted -RedirectStandardOutput $stdout -RedirectStandardError $stderr -NoNewWindow -PassThru
        $script:activeStartedAt = $script:activeProcess.StartTime
        $null = $script:stageProcesses.Add([pscustomobject]@{ Process = $script:activeProcess; StartTime = $script:activeStartedAt })
    } finally {
        $env:TEMP = $previousTemp
        $env:TMP = $previousTmp
    }
    while ($true) {
        $script:activeProcess.Refresh()
        if ($script:activeProcess.HasExited) { break }
        Assert-Budget
        Start-Sleep -Milliseconds 200
    }
    $null = $script:activeProcess.WaitForExit()
    $code = $script:activeProcess.ExitCode
    $script:activeProcess = $null
    if ($code -ne 0) { throw "Stage $name exited $code; inspect $stdout and $stderr." }
    Assert-Budget
    if (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) { throw "Stage $name omitted result $resultPath." }
    $result = Get-Item -LiteralPath $resultPath
    if ($result.Length -eq 0) { throw "Stage $name emitted an empty result." }
    if ($requireJson) {
        try { $null = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json }
        catch { throw "Stage $name did not emit parseable JSON: $($_.Exception.Message)" }
    }
    return [pscustomobject]@{
        name = $name
        arguments = $arguments
        wall_seconds = [math]::Round($clock.Elapsed.TotalSeconds, 3)
        result = $resultPath
        result_bytes = $result.Length
        result_sha256 = (Get-FileHash -LiteralPath $resultPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Get-NodeIds([string]$path, [bool]$allMode = $false) {
    $raw = Get-Content -LiteralPath $path -Raw
    $nodes = @([regex]::Matches($raw, '(?m)^tests[/\\].+::.+$') | ForEach-Object { $_.Value.Trim().Replace('\', '/') })
    $summaries = [regex]::Matches($raw, '(?m)^(?<selected>\d+)(?:/(?<total>\d+))? tests? collected(?: \((?<deselected>\d+) deselected\))? in [\d.]+s[ \t]*\r?$')
    if ($summaries.Count -ne 1) { throw "Collection $path has no unique parseable count." }
    $summary = $summaries[0]
    $selected = [int]$summary.Groups['selected'].Value
    $fraction = $summary.Groups['total'].Success
    $total = if ($fraction) { [int]$summary.Groups['total'].Value } else { $selected }
    $deselected = if ($summary.Groups['deselected'].Success) { [int]$summary.Groups['deselected'].Value } else { 0 }
    if ($nodes.Count -ne $selected -or $selected -gt $total -or $total -ne ($selected + $deselected) -or
        ($fraction -ne $summary.Groups['deselected'].Success) -or
        ($allMode -and ($fraction -or $deselected -ne 0)) -or
        @($nodes | Select-Object -Unique).Count -ne $nodes.Count) {
        throw "Collection $path has missing or duplicate node IDs."
    }
    return $nodes
}

function Invoke-Contract([string]$name, [string[]]$arguments) {
    return Invoke-Stage $name (@("tools/benchmark_spatial_browser.py", "--context", $contextPath) + $arguments) (Join-Path $scratch "$name/stdout.txt") $false
}

function Assert-StageManifest([string]$name, [string]$result) {
    $arguments = @("--validate-manifest", $result, "--stage", $name)
    if ($name -eq "object-affordance-corpus") { $arguments += @("--outcomes", $outcomePath) }
    $null = Invoke-Contract "validate-$name" $arguments
}

try {
    Push-Location $root
    $null = New-Item -ItemType Directory -Path $scratch
    $evidence = Join-Path $scratch "evidence"
    $null = New-Item -ItemType Directory -Path $evidence

    $requiredFiles = @(
        "tools/benchmark.py", "tools/benchmark_search_profiles.py", "tools/benchmark_frontiersmen.py",
        "tools/benchmark_frontiersmen_api.py", "tools/benchmark_chronology_index.py",
        "tools/benchmark_spatial_index.py", "tools/benchmark_spatial_query.py",
        "tools/benchmark_spatial_release.py", "tools/benchmark_spatial_browser.py",
        "tools/benchmark_generational.py", "tools/generate_generational_fixture.py",
        "tools/benchmark_packaged_examples.py", "tools/build_v07_packaged_examples.py"
    )
    foreach ($file in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $root $file) -PathType Leaf)) {
            throw "Mandatory benchmark entry point missing: $file"
        }
    }
    if (-not (Test-Path -LiteralPath $venvPython -PathType Leaf)) { throw "Pinned virtualenv Python is required." }
    $contextPath = Join-Path $evidence "context.json"
    $sourceResult = Join-Path $evidence "spatial-source"
    $rawGenerational = Join-Path $evidence "generational-raw.json"
    $affordanceNode = "tests/test_object_affordances.py::test_legacy_corpus_affordances_survive_candidate_conversion"
    $nonce = [guid]::NewGuid().ToString("N")
    $futureStages = [ordered]@{
        "actual-100k-browser" = @{ kind = "executable"; arguments = @("tools/benchmark_spatial_browser.py", "--context", $contextPath); result = (Join-Path $evidence "actual-100k-browser.json") }
        "generational-5k-10k" = @{ kind = "executable"; arguments = @("tools/benchmark_generational.py", "--output", $rawGenerational, "--work-dir", (Join-Path $scratch "generational-fixture"), "--repeats", "5"); result = (Join-Path $evidence "generational-5k-10k.json") }
        "object-affordance-corpus" = @{ kind = "pytest-attested"; node = $affordanceNode; result = (Join-Path $evidence "object-affordance-corpus.json") }
        "packaged-example-corpus" = @{ kind = "executable"; arguments = @("tools/benchmark_packaged_examples.py", "--context", $contextPath); result = (Join-Path $evidence "packaged-example-corpus.json") }
    }
    if ($futureStages.Count -ne 4 -or @($futureStages.Values | Where-Object { $_.kind -eq "executable" }).Count -ne 3) {
        throw "Exactly four mandatory stages and three executable commands are required."
    }
    foreach ($stage in $futureStages.Keys) {
        if ($null -eq $futureStages[$stage]) {
            throw "Mandatory performance stage is not enrolled: $stage"
        }
        $spec = $futureStages[$stage]
        if ($spec.kind -eq "pytest-attested") {
            if ($stage -ne "object-affordance-corpus" -or $spec.node -ne $affordanceNode -or -not $spec.result -or $spec.arguments) {
                throw "Mandatory affordance once-only attestation wiring is invalid."
            }
        } elseif ($spec.kind -ne "executable" -or -not $spec.arguments -or -not $spec.result -or
            $spec.arguments[0] -notlike "tools/*" -or
            -not (Test-Path -LiteralPath (Join-Path $root $spec.arguments[0]) -PathType Leaf)) {
            throw "Mandatory performance stage has no executable command or result path: $stage"
        }
    }
    $codeHashes = [ordered]@{}
    foreach ($file in (@($requiredFiles) + @("tools/test-performance.ps1", "tests/test_object_affordances.py", "tests/conftest.py"))) {
        $codeHashes[$file] = (Get-FileHash -LiteralPath (Join-Path $root $file) -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    foreach ($file in Get-ChildItem -LiteralPath (Join-Path $root "src/wedl") -Filter "*.py" -File -Recurse) {
        $relative = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
        $codeHashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $inputHashes = [ordered]@{}
    foreach ($package in @("ash_archive", "frontiersmen", "chronology_conformance", "ash_archive_v07", "frontiersmen_v07", "chronology_conformance_v07")) {
        $packageRoot = if ($package.EndsWith("_v07")) { "src/wedl/data" } else { "tests/fixtures/legacy_worlds" }
        foreach ($file in Get-ChildItem -LiteralPath (Join-Path $root "$packageRoot/$package/story") -Filter "*.md" -File -Recurse) {
            $relative = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
            $inputHashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $node = if ($env:WEDL_PERFORMANCE_NODE) { $env:WEDL_PERFORMANCE_NODE } else { "C:/Program Files/nodejs/node.exe" }
    $browser = if ($env:CHROME_PATH) { $env:CHROME_PATH } else { "C:/Program Files/Google/Chrome/Application/chrome.exe" }
    $playwright = if ($env:PLAYWRIGHT_CORE_PATH) { $env:PLAYWRIGHT_CORE_PATH } else { "D:/Projects/hmem/hmem-server/frontend/node_modules/playwright-core" }
    $runtime = [ordered]@{}
    foreach ($binding in @(@("node", $node), @("browser", $browser), @("playwrightPackage", (Join-Path $playwright "package.json")), @("playwrightEntry", (Join-Path $playwright "index.js")))) {
        if (-not (Test-Path -LiteralPath $binding[1] -PathType Leaf)) { throw "Mandatory browser runtime missing: $($binding[0])" }
        $runtime[$binding[0]] = @{ path = [IO.Path]::GetFullPath($binding[1]); sha256 = (Get-FileHash -LiteralPath $binding[1] -Algorithm SHA256).Hash.ToLowerInvariant() }
    }
    $runtime["playwrightVersion"] = (Get-Content -LiteralPath (Join-Path $playwright "package.json") -Raw | ConvertFrom-Json).version
    $libraryFiles = [ordered]@{}
    foreach ($file in Get-ChildItem -LiteralPath $playwright -File -Recurse | Where-Object { $_.Extension -in @(".js", ".mjs", ".json") }) {
        $relative = $file.FullName.Substring([IO.Path]::GetFullPath($playwright).Length + 1).Replace('\', '/')
        $libraryFiles[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $runtime["libraryFiles"] = $libraryFiles
    $runtime["nodeVersion"] = (Get-Item -LiteralPath $node).VersionInfo.ProductVersion
    $runtime["browserVersion"] = (Get-Item -LiteralPath $browser).VersionInfo.ProductVersion
    $results = [ordered]@{}
    $stageSpecs = [ordered]@{}
    foreach ($name in $futureStages.Keys) {
        $results[$name] = $futureStages[$name].result
        $stageSpecs[$name] = $futureStages[$name]
    }
    $context = [ordered]@{
        schemaVersion = 1; runId = $runId; nonce = $nonce; root = $root; scratch = $scratch
        head = (& git -C $root rev-parse HEAD).Trim()
        registrySha256 = (Get-FileHash -LiteralPath (Join-Path $root "pytest.ini") -Algorithm SHA256).Hash.ToLowerInvariant()
        codeHashes = $codeHashes; inputHashes = $inputHashes; results = $results; stageSpecs = $stageSpecs
        affordanceNode = $affordanceNode; contextPath = $contextPath
        sourceManifest = (Join-Path $sourceResult "manifest.json"); codeRoot = (Join-Path $scratch "exact-head-code")
        python = @{ path = $python; sha256 = (Get-FileHash -LiteralPath $python -Algorithm SHA256).Hash.ToLowerInvariant() }
        browserRuntime = $runtime
    }
    $context | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath $contextPath -Encoding UTF8
    $null = Invoke-Contract "browser-preflight" @("--preflight")
    $null = Invoke-Stage "package-preflight" @("tools/benchmark_packaged_examples.py", "--context", $contextPath, "--preflight") (Join-Path $scratch "package-preflight/stdout.txt") $false
    if ($ValidateOnly) {
        Write-Output "Exactly four mandatory stages enrolled: actual-100k-browser, generational-5k-10k, object-affordance-corpus (existing P once-only attestation), packaged-example-corpus; three executable commands. Contracts/runtime verified; no performance execution. Context/evidence retained at $evidence"
        $script:success = $true
        $script:exitCode = 0
    } else {
        $allPath = Join-Path $scratch "collect-all/stdout.txt"
        $normalPath = Join-Path $scratch "collect-normal/stdout.txt"
        $perfPath = Join-Path $scratch "collect-performance/stdout.txt"
        $script:phase = "collection"
        $allStage = Invoke-Stage "collect-all" @("-m", "pytest", "-q", "--collect-only", "--strict-markers", "--wedl-strict", "--wedl-suite=all", "tests", "--basetemp=$(Join-Path $scratch 'all-base')") $allPath $false
        $normalStage = Invoke-Stage "collect-normal" @("-m", "pytest", "-q", "--collect-only", "--strict-markers", "--wedl-strict", "--wedl-suite=normal", "tests", "--basetemp=$(Join-Path $scratch 'normal-base')") $normalPath $false
        $perfStage = Invoke-Stage "collect-performance" @("-m", "pytest", "-q", "--collect-only", "--strict-markers", "--wedl-strict", "--wedl-suite=performance", "tests", "--basetemp=$(Join-Path $scratch 'performance-base')") $perfPath $false
        $all = @(Get-NodeIds $allPath $true)
        $normal = @(Get-NodeIds $normalPath)
        $performance = @(Get-NodeIds $perfPath)
        if (@($performance | Where-Object { $_ -eq $affordanceNode }).Count -ne 1) { throw "Exact affordance P node is missing or duplicated." }
        if ($performance.Count -eq 0 -or $all.Count -ne ($normal.Count + $performance.Count) -or
            @($normal | Where-Object { $performance -contains $_ -or $all -notcontains $_ }).Count -ne 0 -or
            @($performance | Where-Object { $all -notcontains $_ }).Count -ne 0) {
            throw "All/normal/performance node partition is empty, overlapping, or incomplete."
        }
        $script:phase = "performance pytest"
        $outcomePath = Join-Path $evidence "pytest-outcomes.json"
        $env:WEDL_TEST_OUTCOMES = $outcomePath
        $env:WEDL_PERFORMANCE_CONTEXT = $contextPath
        try {
            $pytestStage = Invoke-Stage "performance-pytest" @("-m", "pytest", "-q", "--strict-markers", "--wedl-strict", "--wedl-suite=performance", "tests", "--basetemp=$(Join-Path $scratch 'pytest-base')") $outcomePath
        } finally { $env:WEDL_TEST_OUTCOMES = $previousOutcomes; $env:WEDL_PERFORMANCE_CONTEXT = $previousContext }
        $outcomes = @(Get-Content -LiteralPath $outcomePath -Raw | ConvertFrom-Json)
        if ($outcomes.Count -ne $performance.Count -or @($outcomes | Where-Object { $_[1] -ne "passed" -or $performance -notcontains $_[0] }).Count -ne 0 -or
            @($outcomes | ForEach-Object { $_[0] } | Select-Object -Unique).Count -ne $performance.Count) {
            throw "Performance pytest exact node outcomes differ from collection."
        }
        Assert-StageManifest "object-affordance-corpus" $futureStages["object-affordance-corpus"].result

        $generalRepo = Join-Path $scratch "general-medium"
        $profilesRepo = Join-Path $scratch "profiles-medium"
        $stages = New-Object System.Collections.ArrayList
        $null = $stages.Add((Invoke-Stage "generate-general-medium" @("tools/generate_stress_world.py", $generalRepo, "--profile", "medium") (Join-Path $scratch "generate-general-medium/stdout.txt") $false))
        $null = $stages.Add((Invoke-Stage "general" @("tools/benchmark.py", "--repo", $generalRepo, "--repeats", "7", "--output", (Join-Path $evidence "general.json")) (Join-Path $evidence "general.json")))
        $null = $stages.Add((Invoke-Stage "generate-profiles-medium" @("tools/generate_stress_world.py", $profilesRepo, "--profile", "medium") (Join-Path $scratch "generate-profiles-medium/stdout.txt") $false))
        $null = $stages.Add((Invoke-Stage "profiles" @("tools/benchmark_search_profiles.py", "--repo", $profilesRepo, "--repeats", "9", "--output", (Join-Path $evidence "profiles.json")) (Join-Path $evidence "profiles.json")))
        $null = $stages.Add((Invoke-Stage "frontiersmen" @("tools/benchmark_frontiersmen.py", (Join-Path $evidence "frontiersmen.json")) (Join-Path $evidence "frontiersmen.json")))
        $null = $stages.Add((Invoke-Stage "frontiersmen-api" @("tools/benchmark_frontiersmen_api.py", (Join-Path $evidence "frontiersmen-api.json")) (Join-Path $evidence "frontiersmen-api.json")))
        $null = $stages.Add((Invoke-Stage "chronology" @("tools/benchmark_chronology_index.py") (Join-Path $scratch "chronology/stdout.txt")))
        $null = $stages.Add((Invoke-Stage "spatial-index" @("-c", "import json; from benchmark_spatial_index import full; print(json.dumps(full(), sort_keys=True))") (Join-Path $scratch "spatial-index/stdout.txt")))
        $null = $stages.Add((Invoke-Stage "spatial-query" @("tools/benchmark_spatial_query.py") (Join-Path $scratch "spatial-query/stdout.txt")))
        $null = $stages.Add((Invoke-Contract "prepare-spatial-code" @("--prepare-code")))
        $env:WEDL_CODE_ROOT = $context.codeRoot
        $env:PYTHONPATH = (Join-Path $context.codeRoot "src") + [IO.Path]::PathSeparator + ($runnerPaths -join [IO.Path]::PathSeparator)
        $env:PYTHONDONTWRITEBYTECODE = "1"
        try {
            $null = $stages.Add((Invoke-Stage "spatial-source" @("tools/benchmark_spatial_release.py", "--output", $sourceResult, "--places", "100000", "--maps", "32", "--routes", "250000", "--portals", "100", "--overlays", "10000", "--repeats", "3", "--retain-fixture") (Join-Path $sourceResult "manifest.json")))
        } finally {
            $env:WEDL_CODE_ROOT = $previousCodeRoot
            $env:PYTHONPATH = $runnerPaths -join [IO.Path]::PathSeparator
            $env:PYTHONDONTWRITEBYTECODE = $previousBytecode
        }
        $sourceManifest = Get-Content -LiteralPath (Join-Path $sourceResult "manifest.json") -Raw | ConvertFrom-Json
        if ($sourceManifest.kind -ne "authored-source-to-API" -or
            $sourceManifest.counts.places -lt 100000 -or $sourceManifest.counts.hierarchyDepth -lt 128 -or
            $sourceManifest.counts.rootSiblings -lt 10000 -or $sourceManifest.counts.maps -lt 32 -or
            $sourceManifest.counts.routes -lt 250000 -or $sourceManifest.counts.portals -lt 100 -or
            $sourceManifest.counts.overlays -lt 10000 -or -not $sourceManifest.fixturePath -or
            -not (Test-Path -LiteralPath $sourceManifest.fixturePath -PathType Container)) {
            throw "Authored spatial source counts or retained browser fixture are incomplete."
        }
        foreach ($stage in $futureStages.Keys) {
            $spec = $futureStages[$stage]
            if ($spec.kind -eq "pytest-attested") {
                $null = $stages.Add([pscustomobject]@{ name = $stage; execution = "once in performance pytest"; node = $spec.node; result = $spec.result; result_sha256 = (Get-FileHash -LiteralPath $spec.result -Algorithm SHA256).Hash.ToLowerInvariant() })
                continue
            }
            if (-not $spec.arguments -or -not $spec.result) { throw "Mandatory stage $stage lacks a command or result path." }
            $resultPath = if ($stage -eq "generational-5k-10k") { $rawGenerational } else { $spec.result }
            $arguments = $spec.arguments
            if ($stage -eq "actual-100k-browser") {
                $remaining = [math]::Floor($TimeoutSeconds - $started.Elapsed.TotalSeconds - 5)
                if ($remaining -lt 20) { throw "Insufficient browser cleanup reserve." }
                $arguments += @("--timeout", [string]$remaining)
            }
            $null = $stages.Add((Invoke-Stage $stage $arguments $resultPath))
            if ($stage -eq "generational-5k-10k") { $null = $stages.Add((Invoke-Contract "seal-generational" @("--seal-generational", $rawGenerational))) }
            Assert-StageManifest $stage $spec.result
            $value = Get-Content -LiteralPath $spec.result -Raw | ConvertFrom-Json
            switch ($stage) {
                "actual-100k-browser" {
                    if ($value.kind -ne "actual-browser" -or $value.sourceFixturePath -ne $sourceManifest.fixturePath -or
                        $value.places -lt 100000 -or -not $value.checkpoints.boot -or
                        -not $value.checkpoints.hierarchy -or -not $value.checkpoints.search -or
                        -not $value.checkpoints.viewport -or -not $value.checkpoints.layers -or
                        -not $value.checkpoints.routes -or -not $value.checkpoints.path -or
                        -not $value.checkpoints.offline -or -not $value.domCount -or
                        -not $value.requestCount -or $null -eq $value.networkDestinations) {
                        throw "Actual-browser source fixture or checkpoint evidence is incomplete."
                    }
                }
                "generational-5k-10k" {
                    if ($value.kind -ne "authored-generational" -or $value.counts.characters -lt 5000 -or
                        $value.counts.kinshipEdges -lt 10000 -or $value.counts.generations -lt 100 -or
                        $value.counts.transitions -lt 500 -or -not $value.sourceSha256 -or
                        -not $value.answerSha256 -or -not $value.privacyPassed -or
                        -not $value.horizonPassed -or -not $value.warmQueryMs -or $value.warmQueryMs -gt 250) {
                        throw "Authored generational fixture or query evidence is incomplete."
                    }
                }
                "object-affordance-corpus" {
                    if ($value.kind -ne "object-affordance-corpus" -or
                        $value.fieldCount -lt 53 -or $value.tokenCount -lt 19 -or -not $value.resultSha256) {
                        throw "Object-affordance corpus evidence is incomplete."
                    }
                }
                "packaged-example-corpus" {
                    if ($value.kind -ne "packaged-example-corpus" -or
                        $value.packageCount -lt 3 -or -not $value.conversionSha256 -or
                        -not $value.rebuildSha256) {
                        throw "Packaged-example corpus evidence is incomplete."
                    }
                }
            }
        }
        Assert-Budget
        $elapsed = [math]::Round($started.Elapsed.TotalSeconds, 3)
        $manifest = [ordered]@{
            registry_version = 1
            git_head = (& git -C $root rev-parse HEAD).Trim()
            registry_sha256 = (Get-FileHash -LiteralPath (Join-Path $root "pytest.ini") -Algorithm SHA256).Hash.ToLowerInvariant()
            conftest_sha256 = (Get-FileHash -LiteralPath (Join-Path $root "tests/conftest.py") -Algorithm SHA256).Hash.ToLowerInvariant()
            benchmark_sha256 = @($requiredFiles | ForEach-Object {
                [pscustomobject]@{ path = $_; sha256 = (Get-FileHash -LiteralPath (Join-Path $root $_) -Algorithm SHA256).Hash.ToLowerInvariant() }
            })
            python_version = (& $python --version 2>&1 | Out-String).Trim()
            sqlite_version = (& $python -c "import sqlite3; print(sqlite3.sqlite_version)" | Out-String).Trim()
            all_nodes = $all.Count
            normal_nodes = $normal.Count
            performance_nodes = $performance.Count
            total_wall_seconds = $elapsed
            operating_target_seconds = 1500
            hard_deadline_seconds = $TimeoutSeconds
            meets_operating_target = ($elapsed -le 1500)
            stages = @($allStage, $normalStage, $perfStage, $pytestStage) + @($stages.ToArray())
        }
        $manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $evidence "manifest.json") -Encoding UTF8
        Write-Output "PERFORMANCE SUITE PASSED: $($performance.Count) exact pytest nodes and all enrolled stages; elapsed $elapsed seconds; evidence $evidence"
        if ($elapsed -gt 1500) { Write-Warning "Operating target exceeded; headroom disposition is required before release review." }
        $script:success = $true
        $script:exitCode = 0
    }
} catch {
    foreach ($job in $script:stageProcesses) { Stop-ProcessTree $job.Process $true $job.StartTime }
    [Console]::Error.WriteLine("$($_.Exception.Message) Elapsed $([math]::Round($started.Elapsed.TotalSeconds, 3)) seconds. Evidence retained at $scratch")
} finally {
    Stop-ProcessTree $script:activeProcess $false $script:activeStartedAt
    Pop-Location -ErrorAction SilentlyContinue
    $env:PYTHONPATH = $previousPythonPath
    $env:PYTEST_PLUGINS = $previousPlugins
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    $env:WEDL_TEST_OUTCOMES = $previousOutcomes
    $env:WEDL_PERFORMANCE_CONTEXT = $previousContext
    $env:WEDL_CODE_ROOT = $previousCodeRoot
    $env:PYTHONDONTWRITEBYTECODE = $previousBytecode
}
exit $script:exitCode
