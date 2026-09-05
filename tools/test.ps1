[CmdletBinding()]
param(
    [ValidateRange(1, 300)]
    [int]$TimeoutSeconds = 300
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$runId = [guid]::NewGuid().ToString("N")
$temporaryRoot = [System.IO.Path]::GetTempPath()
$stdoutPath = Join-Path $temporaryRoot "wedl-pytest-$runId.stdout"
$stderrPath = Join-Path $temporaryRoot "wedl-pytest-$runId.stderr"
$diagnosticsPath = Join-Path $temporaryRoot "wedl-pytest-$runId.json"
$testTempRoot = Join-Path $root ".wedl-test-tmp-$runId"
$started = [System.Diagnostics.Stopwatch]::StartNew()

$previousPythonPath = $env:PYTHONPATH
$previousPlugins = $env:PYTEST_PLUGINS
$previousDiagnostics = $env:WEDL_TEST_DIAGNOSTICS
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$script:timedOut = $false
$runnerPaths = @($PSScriptRoot, (Join-Path $root "src"))
if ($previousPythonPath) { $runnerPaths += $previousPythonPath }
$env:PYTHONPATH = $runnerPaths -join [System.IO.Path]::PathSeparator
$env:PYTEST_PLUGINS = if ($previousPlugins) { "$previousPlugins,pytest_timeout_diagnostics" } else { "pytest_timeout_diagnostics" }
$env:WEDL_TEST_DIAGNOSTICS = $diagnosticsPath
$null = New-Item -ItemType Directory -Path $testTempRoot
$env:TEMP = $testTempRoot
$env:TMP = $testTempRoot
$venvPython = Join-Path $root ".venv\\Scripts\\python.exe"
$python = if (Test-Path -LiteralPath $venvPython) { $venvPython } else { "python" }

function Restore-Environment {
    $env:PYTHONPATH = $previousPythonPath
    $env:PYTEST_PLUGINS = $previousPlugins
    $env:WEDL_TEST_DIAGNOSTICS = $previousDiagnostics
    $env:TEMP = $previousTemp
    $env:TMP = $previousTmp
    if ($script:timedOut) {
        return
    }
    Remove-Item -LiteralPath $stdoutPath, $stderrPath, $diagnosticsPath, ([System.IO.Path]::ChangeExtension($diagnosticsPath, "tmp")) -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $testTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

function Read-Diagnostics {
    if (-not (Test-Path -LiteralPath $diagnosticsPath)) {
        return $null
    }
    try {
        return Get-Content -LiteralPath $diagnosticsPath -Raw | ConvertFrom-Json
    } catch {
        return $null
    }
}

try {
    Push-Location $root
    $process = Start-Process -FilePath $python -ArgumentList @("-m", "pytest", "-q", "--durations=25", "tests", "--basetemp=$testTempRoot") -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -NoNewWindow -PassThru
    $processHandle = $process.Handle
    while (-not $process.HasExited) {
        $remainingMilliseconds = [math]::Floor(($TimeoutSeconds - $started.Elapsed.TotalSeconds) * 1000)
        if ($remainingMilliseconds -le 0) {
            break
        }
        Start-Sleep -Milliseconds ([math]::Min(250, $remainingMilliseconds))
        $process.Refresh()
    }

    if (-not $process.HasExited) {
        $script:timedOut = $true
        $snapshot = Read-Diagnostics
        $killer = Start-Process -FilePath "taskkill.exe" -ArgumentList @("/pid", $process.Id, "/t", "/f") -NoNewWindow -PassThru
        $killer.WaitForExit(2000) | Out-Null
        $completed = if ($snapshot -and $snapshot.completed) {
            ($snapshot.completed | Select-Object -First 25 | ForEach-Object { "  {0:N3}s  {1} ({2})" -f $_.duration, $_.nodeid, $_.outcome }) -join [Environment]::NewLine
        } else { "  No completed test reports were recorded." }
        $active = if ($snapshot -and $snapshot.active) { $snapshot.active } else { "unknown" }
        [Console]::Error.WriteLine("Normal test suite exceeded its $TimeoutSeconds-second deadline after $([math]::Round($started.Elapsed.TotalSeconds, 3)) seconds. Active test: $active`nSlowest completed tests:`n$completed`nProcess-tree termination was requested; temporary files were left at $testTempRoot to avoid post-deadline cleanup delay.")
        exit 124
    }

    $process.WaitForExit() | Out-Null
    $process.Refresh()
    $exitCode = $process.ExitCode
    $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
    $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
    if ($stdout) { Write-Output $stdout.TrimEnd() }
    if ($stderr) { [Console]::Error.WriteLine($stderr.TrimEnd()) }
    if ($exitCode -ne 0) {
        [Console]::Error.WriteLine("Normal test suite failed after $([math]::Round($started.Elapsed.TotalSeconds, 3)) seconds (exit code $exitCode).")
        exit $exitCode
    }

    $count = if ($stdout -match "(?m)(\d+\s+passed(?:,\s+\d+\s+\w+)*)\s+in\s+") { $Matches[1] } else { "summary unavailable" }
    Write-Output "NORMAL TEST SUITE PASSED: $count; elapsed $([math]::Round($started.Elapsed.TotalSeconds, 3)) seconds (cap $TimeoutSeconds seconds)."
    exit 0
} finally {
    Pop-Location -ErrorAction SilentlyContinue
    Restore-Environment
}
