[CmdletBinding()]
param(
    [ValidateRange(1, 600)]
    [int]$TimeoutSeconds = 600
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
$previousOutcomes = $env:WEDL_TEST_OUTCOMES
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
$script:jobs = New-Object System.Collections.ArrayList
$script:artifactPaths = New-Object System.Collections.ArrayList
$script:timedOut = $false
$script:passed = $false
$script:exitCode = 1
$script:failureExitCode = 1
$script:phase = "preparation"
$script:collectionDeselected = @{}

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
    $outcomes = Join-Path $temporaryRoot "wedl-pytest-$runId-$name.outcomes.json"
    $null = New-Item -ItemType Directory -Path $temp -Force
    foreach ($path in @($stdout, $stderr, $diagnostics, $outcomes, ([System.IO.Path]::ChangeExtension($diagnostics, "tmp")))) {
        $null = $script:artifactPaths.Add($path)
    }
    return [pscustomobject]@{
        Name = $name
        Temp = $temp
        Basetemp = $basetemp
        Stdout = $stdout
        Stderr = $stderr
        Diagnostics = $diagnostics
        Outcomes = $outcomes
    }
}

function Start-RunnerProcess([string]$name, [string[]]$selection, [bool]$collect, [string]$suite) {
    Assert-Budget
    if ($selection.Count -eq 0) { throw "The $name selection is empty." }
    $paths = New-RunnerPaths $name
    $arguments = @("-m", "pytest", "-q", "--durations=25", "--strict-markers", "--wedl-strict", "--wedl-suite=$suite")
    if ($collect) { $arguments += "--collect-only" }
    $arguments += $selection
    $arguments += "--basetemp=$($paths.Basetemp)"
    $quoted = @($arguments | ForEach-Object { Quote-Argument $_ })
    $env:TEMP = $paths.Temp
    $env:TMP = $paths.Temp
    $env:WEDL_TEST_DIAGNOSTICS = $paths.Diagnostics
    $env:WEDL_TEST_OUTCOMES = $paths.Outcomes
    try {
        $process = Start-Process -FilePath $python -ArgumentList $quoted -RedirectStandardOutput $paths.Stdout -RedirectStandardError $paths.Stderr -NoNewWindow -PassThru
    } finally {
        $env:TEMP = $previousTemp
        $env:TMP = $previousTmp
        $env:WEDL_TEST_DIAGNOSTICS = $previousDiagnostics
        $env:WEDL_TEST_OUTCOMES = $previousOutcomes
    }
    $processHandle = $process.Handle
    $job = [pscustomobject]@{ Name = $name; Process = $process; StartTime = $process.StartTime; Handle = $processHandle; Paths = $paths; Selection = $selection }
    $null = $script:jobs.Add($job)
    return $job
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
        }
        catch { [Console]::Error.WriteLine("Could not terminate pytest PID $($process.Id): $($_.Exception.Message)") }
    }
    $process.Refresh()
    if ($includeExitedDescendants -and $process.HasExited) {
        try { Stop-ExitedDescendants $process $startedAt }
        catch { [Console]::Error.WriteLine("PID $($process.Id) descendant cleanup could not be verified: $($_.Exception.Message)") }
    }
}

function Stop-LiveJobs([bool]$includeExitedDescendants = $false) {
    foreach ($job in $script:jobs) {
        Stop-ProcessTree $job.Process $includeExitedDescendants $job.StartTime
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

function Get-CollectionSummary([string]$stdout, [string]$name, [bool]$allMode = $false) {
    $matches = [regex]::Matches($stdout, '(?m)^(?<selected>\d+)(?:/(?<total>\d+))? tests? collected(?: \((?<deselected>\d+) deselected\))? in [\d.]+s[ \t]*\r?$')
    if ($matches.Count -ne 1) { throw "$name has no unique parseable collection count." }
    $summary = $matches[0]
    $selected = [int]$summary.Groups["selected"].Value
    $fraction = $summary.Groups["total"].Success
    $total = if ($fraction) { [int]$summary.Groups["total"].Value } else { $selected }
    $deselected = if ($summary.Groups["deselected"].Success) { [int]$summary.Groups["deselected"].Value } else { 0 }
    if ($selected -gt $total -or $total -ne ($selected + $deselected) -or
        ($fraction -ne $summary.Groups["deselected"].Success) -or
        ($allMode -and ($fraction -or $deselected -ne 0))) {
        throw "$name has an inconsistent selected/total/deselected collection summary."
    }
    return [pscustomobject]@{ Selected = $selected; Total = $total; Deselected = $deselected }
}

function Collect-Nodes($job, [bool]$allowEmpty = $false) {
    Wait-RunnerProcess $job
    $stdout = Read-JobText $job "stdout"
    if ($allowEmpty -and $job.Process.ExitCode -eq 5 -and $stdout -match 'no tests collected') { return @() }
    if ($job.Process.ExitCode -ne 0) {
        $script:failureExitCode = $job.Process.ExitCode
        throw "$($job.Name) exited $($job.Process.ExitCode) during collection."
    }
    $summary = Get-CollectionSummary $stdout $job.Name ($job.Name -eq "collect-all")
    $nodes = New-Object System.Collections.Generic.List[string]
    foreach ($line in ($stdout -split '\r?\n')) {
        $node = $line.Trim()
        if ($node -match '^tests[/\\].+::.+$') { $nodes.Add($node.Replace('\', '/')) }
    }
    if ($nodes.Count -ne $summary.Selected) {
        throw "$($job.Name) collected $($summary.Selected) but exposed $($nodes.Count) node IDs."
    }
    $script:collectionDeselected[$job.Name] = $summary.Deselected
    return $nodes.ToArray()
}

function Assert-Partition([string[]]$all, [string[]]$normal, [string[]]$performance) {
    $allCounts = New-NodeCounts $all
    $normalCounts = New-NodeCounts $normal
    $performanceCounts = New-NodeCounts $performance
    if ($all.Count -ne $allCounts.Count -or $normal.Count -ne $normalCounts.Count -or
        $performance.Count -ne $performanceCounts.Count -or
        $all.Count -ne ($normal.Count + $performance.Count)) {
        throw "WEDL all/normal/performance collections have a missing, duplicate, or overlapping node ID."
    }
    foreach ($node in $allCounts.Keys) {
        if (($normalCounts.ContainsKey($node)) -eq ($performanceCounts.ContainsKey($node))) {
            throw "WEDL all/normal/performance partition differs at $node."
        }
    }
}

function Assert-ExactOutcomes($job, [string[]]$expected) {
    if (-not (Test-Path -LiteralPath $job.Paths.Outcomes)) { throw "$($job.Name) has no exact outcome record." }
    $json = Get-Content -LiteralPath $job.Paths.Outcomes -Raw
    if ($PSVersionTable.PSVersion.Major -ge 7) {
        $records = ConvertFrom-Json -InputObject $json -NoEnumerate -ErrorAction Stop
    } else {
        $records = ConvertFrom-Json -InputObject $json -ErrorAction Stop
    }
    if ($records -isnot [array]) { throw "$($job.Name) exact outcome record is not an array." }
    $nodes = New-NodeCounts $expected
    if ($records.Count -ne $expected.Count) { throw "$($job.Name) exact outcome count differs from collection." }
    for ($index = 0; $index -lt $records.Count; $index++) {
        $record = $records[$index]
        if ($record -isnot [array] -or $record.Count -ne 2 -or
            $record[0] -isnot [string] -or $record[1] -isnot [string] -or
            $record[1] -cne "passed" -or -not $nodes.ContainsKey($record[0])) {
            throw "$($job.Name) has an unknown or non-passing exact outcome."
        }
        if ($record[0] -cne $expected[$index]) {
            throw "$($job.Name) exact outcome order differs from collection."
        }
        $nodes[$record[0]]--
    }
    if (@($nodes.Values | Where-Object { $_ -ne 0 }).Count -ne 0) {
        throw "$($job.Name) has missing or duplicate exact outcomes."
    }
}

function New-NodeCounts([string[]]$nodes) {
    $counts = [System.Collections.Generic.Dictionary[string,int]]::new([System.StringComparer]::Ordinal)
    foreach ($node in $nodes) {
        if ($counts.ContainsKey($node)) { $counts[$node]++ } else { $counts.Add($node, 1) }
    }
    return $counts
}

function Assert-CollectionParity([string[]]$serial, [object[]]$shards) {
    $reference = New-NodeCounts $serial
    $selected = New-Object System.Collections.Generic.List[string]
    foreach ($shard in $shards) {
        if ($shard.Nodes.Count -eq 0) { throw "A normal test shard is empty." }
        foreach ($node in $shard.Collected) { $selected.Add($node) }
    }
    $combined = New-NodeCounts $selected.ToArray()
    if ($serial.Count -ne $selected.Count -or $reference.Count -ne $serial.Count -or
        $combined.Count -ne $serial.Count) {
        throw "Serial and shard node counts differ or contain duplicate IDs."
    }
    foreach ($node in $reference.Keys) {
        if (-not $combined.ContainsKey($node) -or $combined[$node] -ne $reference[$node]) {
            throw "Serial and shard node-ID multisets differ at $node."
        }
    }
}

function New-ShardSelections([string[]]$nodes) {
    if ($nodes.Count -lt 5) { throw "The normal suite cannot form five nonempty shards." }
    # Attributed setup + call + teardown totals; unknown nodes use an unmeasured fallback.
    $observedSeconds = [System.Collections.Generic.Dictionary[string,double]]::new([System.StringComparer]::Ordinal)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_conversation_discovery_examples_show_typed_beats_but_keep_legacy_turns_speech_only', 0.017455399967730045)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_every_contract_success_example_validates_against_its_component', 0.43694980000145733)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_every_declared_error_has_a_structured_example', 2.548018999863416)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_authoring_examples_cover_eight_kinds_and_four_variants', 0.209754099836573)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_nested_responses_require_cited_closed_shapes', 0.04942690022289753)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_parser_contract_and_closed_request_variants', 0.3813551999628544)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_reference_errors_unknown_context_and_schema_are_closed', 0.16086239996366203)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_handler_shaped_context_and_conversation_variants_validate', 0.0202182000502944)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_authoring_union_rejects_title_update_and_static_validity', 0.05607109982520342)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_command_http_request_and_status_matrix_is_exact', 0.0004936999175697565)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_explorer_http_only_examples_and_closed_outcomes', 0.49603190016932786)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_route_filters_allow_omitted_nullable_modes_but_remain_closed', 0.03274580021388829)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_timeline_presence_and_conversation_beats_have_typed_safe_projections', 0.03522929991595447)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_timeline_schema_requires_exact_string_coordinates_and_named_entries', 0.06029989989474416)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_whereabouts_schema_keeps_journeys_named_and_coordinates_exact', 0.013787000207230449)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_anchor_aliases_rejections_mapping_order_and_era_overlap', 0.013140599941834807)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_civil_axis_precision_approximation_and_conflict_cases', 0.012377400184050202)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_thread_membership_does_not_change_shared_axis_or_story_time', 0.011601599864661694)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_vector_is_parseable_concrete_and_adr_defines_every_semantic_id', 0.01128609967418015)
    $observedSeconds.Add('tests/test_causality.py::test_causal_cli_and_api_contract_expose_the_same_read_surface', 6.514814600115642)
    $observedSeconds.Add('tests/test_causality.py::test_causal_coordinates_reject_cross_timeline_and_same_coordinate_edges', 0.5971719999797642)
    $observedSeconds.Add('tests/test_causality.py::test_causal_directions_and_same_tick_horizon_do_not_leak', 4.4122092998586595)
    $observedSeconds.Add('tests/test_causality.py::test_causality_is_named_deterministic_and_horizon_clipped', 0.7248532001394778)
    $observedSeconds.Add('tests/test_causality.py::test_causality_rejects_noncanonical_focus_without_a_key_error', 0.6643779003061354)
    $observedSeconds.Add('tests/test_causality.py::test_disjoint_fronts_rejoin_only_through_explicit_causal_edges', 0.13183590001426637)
    $observedSeconds.Add('tests/test_causality.py::test_event_cause_rules_retain_direct_edges_and_reject_invalid_graphs', 0.671519200084731)
    $observedSeconds.Add('tests/test_causality.py::test_plot_trail_hides_a_future_typed_cause_even_for_invalid_source', 0.08000790001824498)
    $observedSeconds.Add('tests/test_causality.py::test_plot_trails_and_advisories_are_horizon_sliced', 1.2021194000262767)
    $observedSeconds.Add('tests/test_causality.py::test_typed_cause_plot_and_scene_outcome_relations_are_checked', 0.09081780002452433)
    $observedSeconds.Add('tests/test_changeset.py::test_changeset_journal_surfaces_deferral_recovery_and_replay', 37.54862849996425)
    $observedSeconds.Add('tests/test_changeset.py::test_commit_before_image_capacity_rejected_before_repository_commit', 5.964535500155762)
    $observedSeconds.Add('tests/test_changeset.py::test_legacy_receipt_fault_hook_cannot_publish_a_partial_receipt', 11.270044299773872)
    $observedSeconds.Add('tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[100663296]', 10.475089400308207)
    $observedSeconds.Add('tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[1]', 13.312183000147343)
    $observedSeconds.Add('tests/test_changeset.py::test_non_cache_revision_entries_count_toward_scan_limit', 13.446514799958095)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_author_impact_fails_before_preview_or_ref', 3.9168690000660717)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_existing_cache_defers_before_compiler_construction', 14.803728000028059)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_request_rejects_before_hash_or_source_metadata', 2.6873107999563217)
    $observedSeconds.Add('tests/test_changeset.py::test_revision_enumeration_limit_defers_without_building_cache', 16.180829499848187)
    $observedSeconds.Add('tests/test_changeset.py::test_source_capacity_rejected_before_preview[16777217]', 6.830172499874607)
    $observedSeconds.Add('tests/test_changeset.py::test_source_capacity_rejected_before_preview[None]', 3.529276499990374)
    $observedSeconds.Add('tests/test_changeset.py::test_surface_publication_failure_restores_cache_and_receipt_together', 21.316428600344807)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target0]', 0.000725599704310298)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target1]', 0.0008375998586416245)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target2]', 0.0009354001376777887)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target3]', 0.0008545001037418842)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target4]', 0.0008285997901111841)
    $observedSeconds.Add('tests/test_chronology_api.py::test_open_ranges_and_bounded_conflicts_are_closed_public_shapes', 0.0006328995805233717)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value0]', 0.0006387997418642044)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value1]', 0.0007547999266535044)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value2]', 0.0007343001198023558)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value3]', 0.0007175002247095108)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value4]', 0.0007299000862985849)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_are_strings_and_round_trip_without_integer_leakage', 0.0006609000265598297)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[ 1]', 0.0007650998886674643)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[+1]', 0.0008141000289469957)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[-0]', 0.0009091999381780624)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[01]', 0.0008099002297967672)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[0]', 0.0006726998835802078)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[1.0]', 0.0007023999933153391)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[9223372036854775808]', 0.0007957997731864452)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[True]', 0.0005080001428723335)
    $observedSeconds.Add('tests/test_chronology_api.py::test_read_conflicts_are_not_advertised_for_format_or_convert_but_depth_is_bounded', 0.0013107999693602324)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[None-None]', 0.000812700018286705)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[None-upper2]', 0.0008627001661807299)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[lower0-upper0]', 0.0010057000909000635)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[lower1-None]', 0.0009144998621195555)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_conflict_depth_is_source_aligned_and_never_recurses_unbounded', 0.0012622997164726257)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_extensions_preserve_every_declaration_and_value_boundary', 0.0008859001100063324)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value0-civil]', 0.0008118001278489828)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value1-era]', 0.0007993001490831375)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value2-approx]', 0.0008018999360501766)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value3-relative]', 0.0006510999519377947)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value4-duration]', 0.0008377996273338795)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value5-conflict]', 0.0007554003968834877)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_catalog_replacement_requires_complete_collections_and_scoped_temporary_ids', 0.0007122999522835016)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_public_chronology_authoring_transcodes_only_exact_identifier_fields', 0.0005271998234093189)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_qualitative_approximation_omits_its_public_calendar_id', 0.000930099980905652)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_relative_and_duration_annotations_round_trip_to_source_grammar', 0.0006097001023590565)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_mapping_ambiguity_and_query_advisory_are_explicit_source_and_strict', 5.874083099653944)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_public_outcome_and_advisory_matrix_is_exact_source_and_strict', 5.116884400136769)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_public_reads_have_source_and_strict_compiled_parity', 5.776455400278792)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_validation_mutations_and_cli_http_strict_read_parity', 5.942467800108716)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_packaged_conformance_fixture_is_valid_and_declares_release_coverage', 0.10001530009321868)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_release_docs_share_the_local_v06_capability_boundary', 0.0040219000075012445)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_anchor_duplicate_tie_cross', 0.0007630998734384775)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_axis_round_trip_negative_year_and_monotonicity', 0.002306099981069565)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_cycle_layout_and_table_inversion', 0.0009126001968979836)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_era_and_open_range_semantics_remain_noncanonical', 0.0007215000223368406)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_exact_reserved_ids', 0.0007909000851213932)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_malformed_era_is_invalid_not_exception', 0.0007257000543177128)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_nested_anchor_and_era_bounds_are_total_invalid_results', 0.0006345000583678484)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_table_gap_is_unavailable_but_bad_epoch_is_definition_error', 0.0007698999252170324)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_table_requires_at_least_one_year', 0.0007257999386638403)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_total_boundaries_and_caps', 0.0030487999320030212)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_total_malformed_and_definition_endpoint_classifications', 0.0006872003432363272)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_zero_skip_i64_and_approx_conflict_refusal', 0.0005749999545514584)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_compatibility_matrix_and_exact_migration_protocol_are_stable', 0.011704600183293223)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_cross_contract_persists_group_membership_as_threads_never_thread_ids', 0.0010438000317662954)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_document_binds_atomic_preview_apply_and_disposable_cache_policy', 0.010053399950265884)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_invalid_pinned_legacy_or_transformed_v06_candidate_writes_nothing', 0.010089300107210875)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_legacy_chronology_is_rejected_before_transform_without_writing_input', 0.009035700000822544)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_rollback_pointer_uses_only_the_literal_preview_backup_ref', 0.0009577001910656691)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_same_v1_wire_parity_uses_current_hash_order_and_response_identity', 0.009844999993219972)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v03_and_v05_goldens_transform_only_the_documented_fields', 0.009737299988046288)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v05_grouping_data_is_preserved_without_reinterpretation', 0.009252799907699227)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v06_is_an_explicit_no_op_and_invalid_paths_are_stable', 0.009269800037145615)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_anchor_aliases_coalesce_and_positive_boundaries_are_valid', 0.03372439998202026)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_complete_closed_world_and_annotation_shapes', 0.028329400112852454)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_complete_negative_documents_use_the_same_validator', 0.06513610016554594)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_documented_contract_matches_the_pure_closed_oracle', 0.0007020002231001854)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_effective_calendar_civil_intervals_order_partial_bounds', 0.0255903999786824)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_new_generated_chronology_ids_are_uppercase_while_existing_ids_stay_valid', 0.0006266003474593163)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_version_classification_is_distinct_from_reserved_v06_validation', 0.0004952999297529459)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_x_extensions_are_accepted_without_changing_closed_meaning', 0.03280869987793267)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_zero_skipping_era_display_ordinals_are_checked_and_affine', 0.028023099759593606)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_anchor_map_failures_have_exact_authored_leaves', 0.0006036004051566124)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_calendar_diagnostics_preserve_source_identity_and_exact_field', 0.0006792999338358641)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_canonical_definition_id_collisions_are_owned_by_second_source_id', 0.0009954997804015875)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_checked_arithmetic_keeps_the_authored_operand_leaf', 0.0031910999678075314)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_complete_schema_positive_corpus_is_clean_through_the_boundary', 0.03982100007124245)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_definition_and_annotation_replay_stays_at_authored_leaves', 0.0012618000619113445)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_empty_table_is_a_single_precise_definition_error', 0.0006559998728334904)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_endpoint_layout_validation_precedes_range_and_approximation_ordering', 0.0013794000260531902)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_endpoint_normalization_replays_axis_offset_at_authored_precision_before_ordering', 0.0031839997973293066)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_era_bounds_use_selected_layout_endpoints_then_lower_for_reversal', 0.0010691997595131397)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_era_conversion_replays_checked_subtraction_then_addition', 0.0010716002434492111)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_every_schema_negative_document_runs_through_the_source_boundary', 0.10111359995789826)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_existing_runtime_versions_stay_on_the_existing_validator_path', 0.0006117997691035271)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_mixed_versions_are_rejected_without_generic_reference_noise', 0.0006977000739425421)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_nested_lifecycle_chronology_and_one_sided_approximation', 0.0006469003856182098)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_prepared_kernel_cycle_prefix_and_calendar_gate_keep_exact_leaves', 0.0015082000754773617)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_production_registry_markdown_and_schema_vector_are_bidirectionally_exact', 0.01723090000450611)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_published_endpoint_precedence_and_checked_arithmetic_examples_are_exact', 0.016460300190374255)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_selected_signed_year_layout_drives_epoch_date_range_and_approximation_leaves', 0.0012148001696914434)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_table_gap_is_a_date_diagnostic_at_the_authored_year_leaf', 0.0007786999922245741)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_v06_validation_declares_the_active_read_side_boundary', 0.0008067998569458723)
    $observedSeconds.Add('tests/test_chronology_web_ui.py::test_chronology_ui_assets_are_served_and_the_legacy_catalog_has_an_honest_panel', 9.115721799898893)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_command_is_parser_defined_and_does_not_require_a_repository', 0.10248660016804934)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_docs_cover_one_session_persistent_setup_and_raw_output_contract', 0.00100539973936975)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_scripts_are_deterministic_and_cover_parser_commands_and_options', 0.028593000024557114)
    $observedSeconds.Add('tests/test_cli_completion.py::test_top_level_help_includes_completion_and_first_run_sequence', 0.0406444997061044)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_help_exposes_canonical_kinds_and_numeric_limits', 0.04843810014426708)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments0-wedl entity list-invalid choice]', 0.025515899760648608)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments1-wedl search-must be between 1 and 50]', 0.03937110002152622)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments2-wedl search-must be between 1 and 50]', 0.026007300009950995)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments3-wedl context-must be at least 1800]', 0.03968459996394813)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments4-wedl context-must be at least 1]', 0.04439790011383593)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments5-wedl serve-must be between 1 and 65535]', 0.0358519998844713)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments6-wedl serve-must be between 1 and 65535]', 0.035824900260195136)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_parser_accepts_each_validation_boundary', 0.25655279983766377)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_parser_accepts_exactly_the_canonical_entity_kinds', 0.7010641999077052)
    $observedSeconds.Add('tests/test_cli_serve.py::test_browser_open_failure_has_a_manual_url_fallback', 0.0008009998127818108)
    $observedSeconds.Add('tests/test_cli_serve.py::test_generational_cli_preserves_semantic_exit_matrix', 0.1475969001185149)
    $observedSeconds.Add('tests/test_cli_serve.py::test_local_server_url_brackets_ipv6', 0.0005411002784967422)
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_keeps_the_server_loopback_only', 0.0006641000509262085)
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_rejects_a_nonloopback_hostname_resolution_before_binding', 0.0007608002051711082)
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_reports_port_conflicts_with_actionable_details', 0.0004729002248495817)
    $observedSeconds.Add('tests/test_cli_serve.py::test_ready_server_calls_back_once_across_repeated_startups', 0.002625700319185853)
    $observedSeconds.Add('tests/test_cli_serve.py::test_ready_server_shuts_down_when_the_ready_callback_fails', 0.0017678001895546913)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_dispatch_announces_ready_without_opening_browser', 0.04059710027649999)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_dispatch_opens_once_after_ready_and_compact_announces_url', 0.040551699697971344)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_help_explains_readiness_and_opt_in_opening', 0.02982360008172691)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_open_failure_is_structured_after_readiness', 0.02740779984742403)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_port_preflight_failure_is_structured_and_does_not_start_server', 0.0237249000929296)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_ready_output_flushes_for_piped_callers', 0.042058799881488085)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_rejects_an_invalid_parseable_world_before_socket_activity', 0.03707649977877736)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_rejects_git_root_without_a_wedl_world_before_socket_activity', 0.02638330007903278)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_repository_preflight_failure_is_structured_and_does_not_bind', 0.9490339001640677)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_translates_a_port_collision_after_preflight_before_uvicorn_starts', 0.026161100016906857)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_concurrent_scenes_reject_overlapping_present_casts', 0.6785748002585024)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_every_active_scene_cursor_must_exactly_match_the_world_cursor', 0.6924536998849362)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_explicit_singleton_world_cursor_must_match_its_active_scene', 2.035740200197324)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_historical_double_booking_names_the_character_and_scenes', 3.2652745998930186)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_historical_split_and_reunion_with_independent_characters_is_valid', 0.2690754998475313)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_implicit_scene_is_ambiguous_generically_but_resolves_for_a_character', 2.676610400201753)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_location_link_mapping_requires_exactly_one_valid_target_alias', 1.3866862000431865)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_location_parents_and_routes_are_kind_safe_and_directional', 0.7522153998725116)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_malformed_location_parent_is_diagnostic_not_a_cycle_checker_crash', 0.7599345999769866)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_multiple_active_scenes_require_and_accept_a_shared_cursor', 2.4981621000915766)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_coordinate_event_participation_requires_one_place', 3.0416464002337307)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_coordinate_two_place_presence_is_invalid_but_next_order_handoff_is_valid', 1.9144845998380333)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_story_time_state_writes_are_a_validation_conflict', 0.7250780998729169)
    $observedSeconds.Add('tests/test_conversation_web_ui.py::test_conversation_web_ui_uses_author_scopes_and_safe_static_contract', 5.133052899967879)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_bootstrap_horizon_labels_and_cursor', 29.21626690006815)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_overlong_names_close_direct_and_http', 27.816787200048566)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_prefix_includes_supplementary_unicode_direct_and_http', 11.825164099922404)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_request_bounds_match_openapi', 10.538468300132081)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_uses_compiled_rows_and_rebuilds_malformed_cache', 11.379338199971244)
    $observedSeconds.Add('tests/test_generational_api.py::test_former_roles_name_first_cli_http_and_openapi', 61.76890620030463)
    $observedSeconds.Add('tests/test_generational_api.py::test_name_first_parents_horizons_and_character_controls', 31.57831220002845)
    $observedSeconds.Add('tests/test_generational_api.py::test_read_operations_and_scope_bound_cursor', 90.8871083999984)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_hidden_and_future_organization_selector_parity', 36.18814449990168)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_legacy_index_rebuilds_from_unchanged_source', 19.16752939973958)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_read_horizons_stale_revision_and_closed_modes', 34.09041219973005)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_union_nested_limit_direct_cli_http_schema_parity', 15.286659000441432)
    $observedSeconds.Add('tests/test_generational_authoring.py::test_starter_and_eight_kind_batch_share_one_confirmed_compiled_commit', 100.68110319972038)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ancestry_item_bound_counts_only_visible_active_edges', 0.16747090010903776)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ancestry_orders_each_depth_globally_by_authored_time', 0.06301279994659126)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_cited_bounded_traversal_and_private_search_candidates', 0.05318360007368028)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_distinct_authored_parentage_to_same_parent_keeps_both_citations', 0.06398379965685308)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_missing_or_malformed_generational_shape_rejects_cache', 0.08837769995443523)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_private_discovery_shape_and_generation_reject_old_cache', 0.047022700076922774)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_private_structural_search_shape_is_required', 0.019725800026208162)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ratified_vector_compiles_all_kinds_and_replays_boundaries', 0.05282109999097884)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reparent_cycle_fails_closed_without_partial_containment_path', 0.06460090004839003)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_replacement_reparenting_and_timeline_are_exact', 0.05553400004282594)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reverse_legacy_organization_index_shape_is_required', 0.011666999664157629)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reverse_parentage_index_is_required_and_has_parent_lead', 0.013981400057673454)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_shared_full_and_fast_forward_paths_have_identical_rows', 0.07811490003950894)
    $observedSeconds.Add('tests/test_generational_context.py::test_closed_character_reads_make_no_context', 0.0006194999441504478)
    $observedSeconds.Add('tests/test_generational_context.py::test_context_uses_only_authorized_results', 0.0004384999629110098)
    $observedSeconds.Add('tests/test_generational_context.py::test_subquery_traversal_limit_closes_entire_packet', 0.0006922997999936342)
    $observedSeconds.Add('tests/test_generational_fixture.py::test_bounded_builder_is_valid_reproducible_and_linked', 0.0986733001191169)
    $observedSeconds.Add('tests/test_generational_fixture.py::test_bounded_source_compiled_privacy_horizon_and_confirmed_transports', 230.95855060010217)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_adr_is_accepted_indexed_and_contract_only', 0.000534099992364645)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_literal_frontmatter_uses_one_transition_model', 0.042456599650904536)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_migration_binds_closed_target_capabilities_to_preview_apply', 0.005670200102031231)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_query_vectors_close_privacy_time_and_citation_applicability', 0.052220400189980865)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_absent_future_secret_are_identical_and_scope_is_closed', 0.06658370001241565)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_and_public_leak_matrix_for_claim_role_vital_and_search', 0.05172590003348887)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_malformed_search_and_relative_target_are_invalid', 0.0578958997502923)
    $observedSeconds.Add('tests/test_generational_query.py::test_containment_one_parent_fits_one_item', 0.05937029980123043)
    $observedSeconds.Add('tests/test_generational_query.py::test_cursor_is_bound_to_scope_and_revision', 0.04979610024020076)
    $observedSeconds.Add('tests/test_generational_query.py::test_discovery_closes_overlong_titles_and_aliases', 0.06054689991287887)
    $observedSeconds.Add('tests/test_generational_query.py::test_discovery_name_is_admitted_at_exact_same_tick_order', 0.06110900011844933)
    $observedSeconds.Add('tests/test_generational_query.py::test_former_roles_exact_horizon_visibility_role_and_combined_cap', 0.07538169994950294)
    $observedSeconds.Add('tests/test_generational_query.py::test_former_roles_request_is_strictly_opt_in', 0.061150000197812915)
    $observedSeconds.Add('tests/test_generational_query.py::test_parent_item_limit_counts_only_active_authorized_edges', 0.05689500016160309)
    $observedSeconds.Add('tests/test_generational_query.py::test_parentage_descendants_and_boundary_order', 0.05623570014722645)
    $observedSeconds.Add('tests/test_generational_query.py::test_relative_path_merges_equal_depth_edges_by_global_applicability', 0.05834330036304891)
    $observedSeconds.Add('tests/test_generational_query.py::test_relatives_are_one_continuous_cited_path_in_both_directions', 0.05762100010178983)
    $observedSeconds.Add('tests/test_generational_query.py::test_reverse_union_membership_and_organization_legacies_fold_before_budget', 0.06649530027061701)
    $observedSeconds.Add('tests/test_generational_query.py::test_source_compiled_and_rebuilt_query_parity', 0.332553300075233)
    $observedSeconds.Add('tests/test_generational_query.py::test_union_history_and_active_roster_obey_items_limit', 0.058774499921128154)
    $observedSeconds.Add('tests/test_generational_query.py::test_union_organization_legacy_vital_and_private_search', 0.06373219983652234)
    $observedSeconds.Add('tests/test_generational_query.py::test_vital_direct_and_search_agree_before_birth_and_when_withheld', 0.07390939956530929)
    $observedSeconds.Add('tests/test_generational_source.py::test_generational_generated_paths_are_canonical', 0.0005491999909281731)
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_accessor_is_immutable_and_rejects_extra_leaf_fields', 0.0005053002387285233)
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[organization-initialize-payload0]', 0.000740900170058012)
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[parentage-initialize-payload2]', 0.0006804997101426125)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[aliases-bad_value6]', 0.0005234999116510153)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[audience-bad_value5]', 0.0005188998766243458)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[domain-history]', 0.000864299712702632)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[status-bad_value2]', 0.0005403000395745039)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[threads-bad_value4]', 0.0004475000314414501)
    $observedSeconds.Add('tests/test_generational_source.py::test_tenure_transfer_requires_a_cause_event', 0.0006086998619139194)
    $observedSeconds.Add('tests/test_generational_validation.py::test_all_eight_runtime_kinds_accept_and_cross_record_integrity_rejects', 0.005164899863302708)
    $observedSeconds.Add('tests/test_generational_validation.py::test_closed_state_tables_cover_every_generational_kind_and_vital_sequence', 0.00030039972625672817)
    $observedSeconds.Add('tests/test_generational_validation.py::test_generational_serialization_keeps_transition_and_payload_order', 0.000610200222581625)
    $observedSeconds.Add('tests/test_generational_validation.py::test_generational_validation_accepts_literal_union_and_rejects_bad_payload', 0.000907800393179059)
    $observedSeconds.Add('tests/test_generational_validation.py::test_transition_replacement_cause_and_malformed_leaves_are_total', 0.0014878001529723406)
    $observedSeconds.Add('tests/test_generational_validation.py::test_validation_is_total_and_reports_exact_transition_leaves_and_paths', 0.0006051999516785145)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_admission_and_commit_share_one_deadline_cell', 14.862451300024986)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_consumer_persistent_lock_keeps_head_source_and_index', 7.871954400092363)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_consumer_waits_for_a_transient_lock', 13.446238800184801)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_released_migration_lock_rechecks_head_before_creating_absent_backup', 7.058691499987617)
    $observedSeconds.Add('tests/test_multi_strand_continuity_adr.py::test_multi_strand_adr_is_historical_and_explicitly_superseded', 0.0007615999784320593)
    $observedSeconds.Add('tests/test_multi_strand_schema_contract.py::test_withdrawn_continuity_schema_material_is_nonnormative_only', 0.0011287000961601734)
    $observedSeconds.Add('tests/test_no_v04_continuity_residue.py::test_runtime_has_no_withdrawn_v04_continuity_residue', 0.036479099886491895)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields0-object-object_affordances]', 20.359969600103796)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields1-object-object_affordances]', 21.19464479992166)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields2-object-capabilities]', 14.326780899660662)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields3-character-capabilities]', 14.464893199736252)
    $observedSeconds.Add('tests/test_object_affordances.py::test_upgrade_preview_apply_replay_detail_and_rollback', 73.94942749966867)
    $observedSeconds.Add('tests/test_object_affordances.py::test_v07_validation_rejects_mixed_non_object_and_bad_values', 31.76483969995752)
    $observedSeconds.Add('tests/test_performance_cache.py::test_bounded_git_tree_metadata_defers_before_world_load', 5.344824299681932)
    $observedSeconds.Add('tests/test_performance_cache.py::test_in_memory_authoring_cache_matches_direct_compiler_projection', 10.40910020004958)
    $observedSeconds.Add('tests/test_performance_cache.py::test_malformed_or_unknown_git_tree_metadata_has_no_capacity_proof', 5.79455400002189)
    $observedSeconds.Add('tests/test_performance_cache.py::test_malformed_profile_in_valid_cache_defers_without_losing_source_commit', 18.063879899913445)
    $observedSeconds.Add('tests/test_performance_cache.py::test_non_source_git_tree_output_is_bounded_before_world_load', 1.0490697000641376)
    $observedSeconds.Add('tests/test_performance_cache.py::test_sqlite_serialization_failure_defers_without_shared_cache_write', 9.145430599804968)
    $observedSeconds.Add('tests/test_performance_cache.py::test_unknown_authoring_capacity_defers_before_world_load', 4.596345900092274)
    $observedSeconds.Add('tests/test_semantics.py::test_expanded_knowledge_appears_at_correct_time', 8.806634900160134)
    $observedSeconds.Add('tests/test_semantics.py::test_story_point_history_is_not_rewritten', 10.91009660018608)
    $observedSeconds.Add('tests/test_semantics.py::test_temporal_object_holder', 8.743100400082767)
    $observedSeconds.Add('tests/test_shared_world_thread_schema_contract.py::test_v05_migration_and_quarantined_v04_recovery_are_narrow_and_noninferential', 0.007081199903041124)
    $observedSeconds.Add('tests/test_shared_world_thread_schema_contract.py::test_v05_thread_reservation_has_one_global_timeline_and_grouping_only_membership', 0.00487289996817708)
    $observedSeconds.Add('tests/test_shared_world_threads_adr.py::test_coordination_manifest_withdraws_only_thread_reservation_and_preserves_other_projects', 0.0027372001204639673)
    $observedSeconds.Add('tests/test_shared_world_threads_adr.py::test_shared_world_thread_adr_governs_one_global_world_without_schema_invention', 0.0027912999503314495)
    $observedSeconds.Add('tests/test_source_validation.py::test_crlf_frontmatter_envelope_is_accepted', 0.0006986001972109079)
    $observedSeconds.Add('tests/test_source_validation.py::test_duplicate_yaml_keys_are_rejected', 0.0006782999262213707)
    $observedSeconds.Add('tests/test_source_validation.py::test_expanded_fixture_validates_and_has_conversations', 11.500920200254768)
    $observedSeconds.Add('tests/test_source_validation.py::test_local_migration_is_cli_only', 0.08345860010012984)
    $observedSeconds.Add('tests/test_source_validation.py::test_yaml_sequence_mapping_keys_are_parse_errors_not_type_errors', 0.0009631998836994171)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_adr_is_accepted_indexed_and_explicitly_non_runtime', 0.0006584001239389181)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_explorer_adjunct_decision_records_bounded_selected_lens', 0.0008191999513655901)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_query_vectors_close_states_budgets_time_and_audience', 0.1510398997925222)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_spatial_shape_preserves_coordinate_free_worlds_and_non_inference', 0.007955400040373206)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_version_migration_and_cross_project_boundary_are_closed', 0.008327199844643474)
    $observedSeconds.Add('tests/test_spatial_api.py::test_capability_order_and_operation_fields_are_closed_before_lookup', 0.0007853996939957142)
    $observedSeconds.Add('tests/test_spatial_api.py::test_codec_cli_http_share_one_exact_spatial_outcome', 71.03786240005866)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[9007199254740992]', 0.0006348998285830021)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[True]', 0.000596500001847744)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[inf]', 0.0004952000454068184)
    $observedSeconds.Add('tests/test_spatial_api.py::test_hidden_overlay_and_no_overlay_control_have_identical_public_result', 6.296710700029507)
    $observedSeconds.Add('tests/test_spatial_api.py::test_legacy_bbox_compiled_capability_absence_is_a_typed_public_unavailable', 15.672962399898097)
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[0]', 0.000667099840939045)
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[1.0]', 0.0008021998219192028)
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[101]', 0.0007377995643764734)
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[True]', 0.0008262000046670437)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[adjacency]', 0.0009062997996807098)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[bbox]', 0.0009327000007033348)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[children]', 0.0005151999648660421)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[containment]', 0.0007494997698813677)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[nearby]', 0.0005962001159787178)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[overlay-as-of]', 0.0007813000120222569)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[path]', 0.0006101001054048538)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[reachability]', 0.0006119997706264257)
    $observedSeconds.Add('tests/test_spatial_api.py::test_rounded_two_edge_metric_is_typed_unavailable_across_public_transports', 32.635007000295445)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[+1]', 0.0005886000581085682)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[-0]', 0.0005638001020997763)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[01]', 0.000624299980700016)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[0]', 0.0007114000618457794)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[9223372036854775808]', 0.0005216998979449272)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[True]', 0.0005763000808656216)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[bbox-relation]', 5.235261199763045)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[overlay-as-of-queryScope]', 30.315743599785492)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[path-metric]', 7.524920699885115)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.location.update-payload4]', 0.0060946999583393335)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.map.create-payload0]', 0.004286999814212322)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.map.update-payload3]', 0.006384700071066618)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.overlay.create-payload2]', 0.006079800194129348)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.overlay.update-payload6]', 0.006371000083163381)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.route.create-payload1]', 0.005604899954050779)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.route.update-payload5]', 0.004011500161141157)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_intent_payloads_and_version_gate_are_closed', 0.009449100121855736)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_malformed_spatial_authoring_enums_and_numbers_are_wedl_http_errors', 29.30520320008509)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_map_create_preview_apply_and_exact_replay_use_real_journal', 67.17788450000808)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_overlay_create_and_route_update_preview_apply_and_replay_through_real_journal', 31.906439299695194)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_apply_refuses_unconfirmed_bypass_before_repository_access', 0.0007231999188661575)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_location_update_keeps_detail_context_and_whereabouts_compatible', 57.14138530008495)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_preview_is_read_only_for_source_cache_receipt_and_head', 30.468210499966517)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_source_and_receipt_roll_back_on_journal_publication_failure', 53.28983540018089)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.location.update-payload1]', 0.012249700026586652)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.map.update-payload0]', 0.01604099990800023)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.overlay.update-payload3]', 0.011899400036782026)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.route.update-payload2]', 0.018751600058749318)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_all_place_modes_and_map_features_match_authored_source', 17.635161199839786)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_catalog_places_viewport_layers_direct_http_and_selected_cursor', 19.4556202001404)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_compiled_read_errors_redact_host_paths', 31.679757299833)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_compiled_same_tick_overlay_respects_both_order_boundaries', 0.008001499809324741)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_five_http_only_routes_openapi_and_examples', 0.33665539999492466)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_forged_cursor_ordinals_and_surrogates_close_as_invalid', 14.886707300087437)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_layer_pages_bind_lens_horizon_and_member_order', 0.010743799852207303)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_legacy_catalog_is_useful_and_non_map_reads_close', 1.1879976000636816)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_non_ok_state_matrix_and_guard_parity', 33.362283599795774)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_places_modes_lens_and_closed_validation', 13.810901599703357)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_direct_http_direction_modes_closed_and_cursor', 16.769014799734578)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_nonself_two_way_directed_cards_and_mounted_http', 27.79667149996385)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_two_way_self_loop_and_position_portal_compiled_cards', 0.01919639972038567)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_signed_large_bounds_and_unsafe_geometry_close', 5.542160600190982)
    $observedSeconds.Add('tests/test_spatial_query.py::test_absent_endpoints_are_unavailable_but_valid_empty_controls_stay_ok', 0.009946899954229593)
    $observedSeconds.Add('tests/test_spatial_query.py::test_bbox_requires_geometry_capability_and_map_dimensionality', 0.010564400115981698)
    $observedSeconds.Add('tests/test_spatial_query.py::test_bbox_same_map_pagination_and_cursor_revision_binding', 0.007454399950802326)
    $observedSeconds.Add('tests/test_spatial_query.py::test_canonical_query_vector_and_source_compiled_fixture_stay_in_parity', 0.019454299937933683)
    $observedSeconds.Add('tests/test_spatial_query.py::test_children_bbox_within_adjacency_and_portal_reachability_are_authored_only', 0.007762599969282746)
    $observedSeconds.Add('tests/test_spatial_query.py::test_context_preserves_validated_registry_order_for_combined_capabilities', 0.02097530011087656)
    $observedSeconds.Add('tests/test_spatial_query.py::test_directed_path_does_not_infer_reverse_and_unknown_metric_is_closed', 0.010810500010848045)
    $observedSeconds.Add('tests/test_spatial_query.py::test_explorer_bbox_requires_map_scoped_rtree_but_legacy_bbox_stays_available', 0.007835200056433678)
    $observedSeconds.Add('tests/test_spatial_query.py::test_failed_rtree_population_leaves_no_partial_explorer_index', 0.008819400100037456)
    $observedSeconds.Add('tests/test_spatial_query.py::test_failed_temporal_index_population_removes_both_explorer_rtrees', 0.008195200003683567)
    $observedSeconds.Add('tests/test_spatial_query.py::test_hierarchy_is_authored_parent_path_and_coordinate_free_locations_are_readable', 0.010373499942943454)
    $observedSeconds.Add('tests/test_spatial_query.py::test_nearby_is_same_map_candidate_read_not_authored_topology', 0.010019199922680855)
    $observedSeconds.Add('tests/test_spatial_query.py::test_nearby_target_radius_aliases_and_exact_radius_postfilter', 0.007551500108093023)
    $observedSeconds.Add('tests/test_spatial_query.py::test_overlay_story_time_never_crosses_timelines_or_converts_duration', 0.006957999896258116)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_keeps_incompatible_units_in_separate_typed_graphs', 0.007657300215214491)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_preserves_exact_large_integers_overflow_and_near_equal_ordering', 0.020389100071042776)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_refuses_to_rank_mixed_units_and_preserves_requested_identity_unit', 0.007830699905753136)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_reports_downstream_closed_and_restricted_edges', 0.00745710008777678)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_uses_exact_mixed_float_comparison_and_authored_tie_keys', 0.014079600106924772)
    $observedSeconds.Add('tests/test_spatial_query.py::test_positive_keyset_cursor_paging_is_complete_and_request_bound', 0.007834899937734008)
    $observedSeconds.Add('tests/test_spatial_query.py::test_repeated_path_order_is_stable_across_cycles_and_insertion_order', 0.011865799780935049)
    $observedSeconds.Add('tests/test_spatial_query.py::test_signed_i64_story_time_boundaries_and_cross_map_discontinuity_are_closed', 0.00886470009572804)
    $observedSeconds.Add('tests/test_spatial_query.py::test_story_time_rtree_boxes_cover_exact_signed_tick_order_ranges', 0.0005517001263797283)
    $observedSeconds.Add('tests/test_spatial_query.py::test_success_summaries_expose_closed_partial_unknown_filter_and_horizon_semantics', 0.008308600168675184)
    $observedSeconds.Add('tests/test_spatial_query.py::test_typed_path_has_stable_ties_and_respects_mode_and_availability', 0.00935309985652566)
    $observedSeconds.Add('tests/test_spatial_release_contract.py::test_authored_deep_wide_cycle_and_unknown_costs', 45.993470000103116)
    $observedSeconds.Add('tests/test_spatial_release_contract.py::test_mixed_legacy_source_is_rejected_without_side_effects', 12.811940199928358)
    $observedSeconds.Add('tests/test_spatial_source.py::test_spatial_ids_are_narrow_and_do_not_change_legacy_id_acceptance', 0.0003955999854952097)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capabilities_are_closed_and_protocol_ordered', 0.00032600038684904575)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value0-None]', 0.0006585000082850456)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value1-expected1]', 0.00046200002543628216)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value10-expected10]', 0.0008255001157522202)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value2-expected2]', 0.0006046001799404621)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value3-None]', 0.0004803999327123165)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value4-None]', 0.0006750999018549919)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value5-None]', 0.0005814998876303434)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value6-expected6]', 0.00044269999489188194)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value8-expected8]', 0.0005152998492121696)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value9-expected9]', 0.0007885999511927366)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_accessor_and_validator_reject_the_same_spatial_leaf_shapes', 0.007371300365775824)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_anchor_conversion_and_detailed_location_link_rules_are_shared_and_canonical', 0.005749299889430404)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_arbitrary_record_frontmatter_never_raises', 0.005508899921551347)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_fixture_covers_coordinate_free_maps_routes_and_timed_overlays', 0.0025701001286506653)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_paths_ordering_and_runtime_registration_are_canonical', 0.0028127001132816076)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_rejects_nonfinite_geometry_cycles_and_unapproved_boundary', 0.008076000027358532)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_requires_world_ordered_capability_declaration_and_keeps_legacy_inert', 0.0025128000415861607)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_diagnostic_catalog_is_closed', 0.002961000194773078)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_envelope_and_map_diagnostics_identify_the_exact_leaf', 0.004676300100982189)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_feature_gates_legacy_locations_and_total_malformed_yaml_diagnostics', 0.006143800215795636)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_frontmatter_round_trip_preserves_v07_body_but_keeps_legacy_canonical_bytes', 0.0007668000180274248)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generational_only_envelope_accepts_coordinate_free_records_but_gates_spatial_features', 0.0027864999137818813)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generic_v07_validation_composes_core_and_record_time_checks', 0.002775899600237608)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generic_validation_uses_component_envelope_and_never_writes', 0.0029684999026358128)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_invalid_fixture_bool_nan_and_duplicate_edges_are_leaf_diagnostics', 0.003450499614700675)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_multimap_geodetic_anchor_portal_and_legacy_location_fixture', 0.004817800363525748)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_typed_accessors_are_deeply_immutable_and_include_optional_values', 0.0026525999419391155)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_envelope_legacy_links_and_geometry_variants_are_literal_not_routes', 0.0029627999756485224)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_envelope_uses_full_chronology_validation_and_listed_default_timeline', 0.0024624003563076258)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_location_accepts_inherited_chronology_annotations', 0.003028199775144458)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_parent_compatibility_input_serializes_to_adr_canonical_parent_id', 0.004063900094479322)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_z_policy_is_consistent_for_bounds_origin_geometry_and_accessors', 0.005370700033381581)
    $observedSeconds.Add('tests/test_spatial_validation.py::test_v07_component_is_not_generic_source_acceptance', 0.0006328001618385315)
    $observedSeconds.Add('tests/test_story_points_web_ui.py::test_story_points_web_ui_uses_selected_scene_and_safe_static_contract', 5.072810600046068)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_generational_transport_matrix_and_closed_revision', 168.04502010019496)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.3]', 77.661806399934)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.5]', 83.48334779962897)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.6]', 89.82331330003217)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_spatial_source_compiled_rebuild_and_closed_transport', 30.463866399833933)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_thread_filter_preserves_rank_and_never_backfills', 0.0006157001480460167)
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_invalid_v05_stops_before_database_revision_or_vector_publication', 7.713743700180203)
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_v04_compiled_cache_metadata_is_incompatible', 8.798066399991512)
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_valid_v05_compiles_publishes_threads_and_keeps_search_vector_inputs_stable', 6.753575399983674)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_current_fingerprint_v04_cache_entry_is_evicted_before_record_rehydration', 6.125842700013891)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_exact_v04_is_quarantined_before_record_construction_with_recovery_link', 0.0005359000060707331)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_thread_ids_are_non_entity_identifiers_and_source_schema_cache_contract', 0.00037180003710091114)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_v05_source_round_trip_canonicalizes_thread_lists_without_synthesizing_memberships', 0.0007029001135379076)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_v05_world_and_record_expose_grouping_membership_without_entity_refs', 0.0005652999971061945)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_catalog_preserves_v04_quarantine_boundary', 4.65541459992528)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_v03_catalog_has_no_synthetic_grouping_and_cli_http_query_parity', 17.96412319992669)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_v05_catalog_is_sorted_and_discloses_only_public_declarations', 7.7868085000664)
    $observedSeconds.Add('tests/test_thread_search_filter.py::test_resolve_thread_filter_requires_normalized_declared_v05_ids', 0.00198079994879663)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_capture_rejects_parent_reparse_substitution', 0.006410300265997648)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_exact_limit_and_growth', 0.0019973001908510923)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_rejects_nonregular_path', 0.002297600032761693)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_revalidates_named_path', 0.0032042998354882)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_commit_before_limit_preserves_ref_index_and_source', 6.481590299867094)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_external_touched_entry_after_wait_aborts_before_ref_or_source_publish', 10.174493899801746)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_ignores_overridden_str_methods', 0.0005467000883072615)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[all-controls]', 0.0005584999453276396)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[astral]', 0.00052429991774261)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[high-surrogate]', 0.0004841000773012638)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[latin-one]', 0.0007066002581268549)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[long-base64]', 0.022012899862602353)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[low-surrogate]', 0.0004849000833928585)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[plain]', 0.0005447000730782747)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[quotes-and-slashes]', 0.0006388998590409756)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[repeated-controls]', 0.0008618002757430077)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_wire_bound_rejects_malformed_values_and_overflow', 0.000828899908810854)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_transient_external_lock_releases_within_the_private_window', 9.741002599941567)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_two_external_lock_seams_share_the_first_lazy_deadline', 0.6049504999537021)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_check_reports_drift_without_modifying_packages', 0.07570480019785464)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_explicit_bootstrap_choices_keep_legacy_default', 0.04949830030091107)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_missing_legacy_record_fails_before_publication', 1.1519351999741048)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_mixed_schema_fails_before_publication', 0.033775099785998464)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_semantic_projection_preserves_absent_empty_order_and_duplicates', 0.0005891001783311367)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_small_packaged_upgrade_stale_head_and_forward_rollback', 51.5219553001225)
    $unknownSeconds = 0.5
    $groups = [System.Collections.Generic.Dictionary[string,object]]::new([System.StringComparer]::Ordinal)
    foreach ($node in $nodes) {
        $key = $node
        foreach ($module in @('tests/test_spatial_api.py', 'tests/test_spatial_explorer_api.py')) {
            if ($node.StartsWith($module + '::', [System.StringComparison]::Ordinal)) {
                $key = $module
                break
            }
        }
        if (-not $groups.ContainsKey($key)) {
            $groups.Add($key, [pscustomobject]@{
                Name = $key; Nodes = [System.Collections.Generic.List[string]]::new(); Weight = 0.0
            })
        }
        $groups[$key].Nodes.Add($node)
    }
    if ($groups.Count -lt 5) { throw "The normal suite cannot form five nonempty fixture groups." }
    foreach ($group in $groups.Values) {
        $group.Nodes.Sort([System.StringComparer]::Ordinal)
        foreach ($node in $group.Nodes) {
            $weight = if ($observedSeconds.ContainsKey($node)) { $observedSeconds[$node] } else { $unknownSeconds }
            $group.Weight += $weight
        }
    }
    $ordered = [object[]]@($groups.Values)
    $groupComparer = [System.Collections.Generic.Comparer[object]]::Create([System.Comparison[object]]{
        param($left, $right)
        $byWeight = $right.Weight.CompareTo($left.Weight)
        if ($byWeight -ne 0) { return $byWeight }
        return [System.StringComparer]::Ordinal.Compare($left.Name, $right.Name)
    })
    [System.Array]::Sort($ordered, $groupComparer)
    $shards = @(foreach ($name in @('a', 'b', 'c', 'd', 'e')) {
        [pscustomobject]@{
            Name = $name; Nodes = [System.Collections.Generic.List[string]]::new()
            Collected = @(); Weight = 0.0; PerformanceCount = 0
        }
    })
    foreach ($group in $ordered) {
        $target = $shards[0]
        foreach ($candidate in $shards) {
            if ($candidate.Weight -lt $target.Weight -or
                ($candidate.Weight -eq $target.Weight -and
                 [System.StringComparer]::Ordinal.Compare($candidate.Name, $target.Name) -lt 0)) {
                $target = $candidate
            }
        }
        foreach ($node in $group.Nodes) { $target.Nodes.Add($node) }
        $target.Weight += $group.Weight
    }
    foreach ($shard in $shards) { $shard.Nodes.Sort([System.StringComparer]::Ordinal) }
    return $shards
}

function Get-AccountedCount([string]$stdout, [string]$name, [int]$expectedDeselected) {
    $summaries = [regex]::Matches($stdout, '(?m)^(?<outcomes>\d+ [A-Za-z]+(?:, \d+ [A-Za-z]+)*) in [\d.]+s(?: \([0-9]+:[0-5][0-9]:[0-5][0-9]\))?[ \t]*\r?$')
    if ($summaries.Count -ne 1) { throw "$name has no unique parseable pytest outcome summary." }
    $summary = $summaries[0]
    $accounted = 0
    $deselected = 0
    $seen = New-Object System.Collections.Generic.HashSet[string]
    foreach ($part in ($summary.Groups["outcomes"].Value -split ', ')) {
        if ($part -cnotmatch '^(\d+) (passed|deselected|warnings?)$') {
            throw "$name has an unknown outcome token: $part."
        }
        if (-not $seen.Add($Matches[2])) { throw "$name has a duplicate outcome token: $part." }
        if ($Matches[2] -eq "passed") {
            $accounted += [int]$Matches[1]
        } elseif ($Matches[2] -eq "deselected") {
            $deselected += [int]$Matches[1]
        }
    }
    if (-not $seen.Contains("passed") -or $deselected -ne $expectedDeselected -or
        ($expectedDeselected -gt 0 -and -not $seen.Contains("deselected"))) {
        throw "$name has an inconsistent passed/deselected outcome summary."
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
    $allNodes = @(Collect-Nodes (Start-RunnerProcess "collect-all" @("tests") $true "all"))
    $serialNodes = @(Collect-Nodes (Start-RunnerProcess "collect-serial" @("tests") $true "normal"))
    $performanceNodes = @(Collect-Nodes (Start-RunnerProcess "collect-performance" @("tests") $true "performance") $true)
    Assert-Partition $allNodes $serialNodes $performanceNodes
    if ($script:collectionDeselected["collect-serial"] -ne $performanceNodes.Count -or
        $script:collectionDeselected["collect-performance"] -ne $serialNodes.Count) {
        throw "Full normal/performance deselection counts differ from the exact partition."
    }
    if ($serialNodes.Count -eq 0) { throw "The normal suite collected no test nodes." }
    $shards = @(New-ShardSelections $serialNodes)
    foreach ($shard in $shards) {
        $script:phase = "shard $($shard.Name.ToUpperInvariant()) collection"
        $shard.Collected = @(Collect-Nodes (Start-RunnerProcess "collect-$($shard.Name)" $shard.Nodes.ToArray() $true "normal"))
        if ($script:collectionDeselected["collect-$($shard.Name)"] -ne 0) {
            throw "Explicit normal shard selected a performance node."
        }
    }
    Assert-CollectionParity $serialNodes $shards
    Assert-Budget
    $collectionElapsed = $started.Elapsed.TotalSeconds
    Write-Output "NORMAL TEST COLLECTION: $($serialNodes.Count) nodes; five disjoint exact node-ID shards."
    foreach ($shard in $shards) {
        Write-Output "SHARD $($shard.Name.ToUpperInvariant()) COLLECTION: $($shard.Collected.Count) nodes; attributed total-phase weight $([math]::Round($shard.Weight, 3)) seconds."
    }

    $script:phase = "parallel execution"
    foreach ($shard in $shards) {
        $shard | Add-Member -NotePropertyName Run -NotePropertyValue (Start-RunnerProcess "run-$($shard.Name)" $shard.Nodes.ToArray() $false "normal")
    }
    while ($true) {
        $allExited = $true
        foreach ($shard in $shards) {
            $shard.Run.Process.Refresh()
            if ($shard.Run.Process.HasExited) {
                if ($shard.Run.Process.ExitCode -ne 0) {
                    $script:failureExitCode = $shard.Run.Process.ExitCode
                    throw "Shard $($shard.Name.ToUpperInvariant()) exited $($shard.Run.Process.ExitCode)."
                }
            } else { $allExited = $false }
        }
        if ($allExited) { break }
        Assert-Budget
        $remaining = [math]::Floor(($TimeoutSeconds - $started.Elapsed.TotalSeconds) * 1000)
        Start-Sleep -Milliseconds ([math]::Min(250, [math]::Max(1, $remaining)))
    }
    $executionElapsed = $started.Elapsed.TotalSeconds
    $totalAccounted = 0
    foreach ($shard in $shards) {
        $null = $shard.Run.Process.WaitForExit()
        $stdout = Read-JobText $shard.Run "stdout"
        $stderr = Read-JobText $shard.Run "stderr"
        $accounted = Get-AccountedCount $stdout "Shard $($shard.Name.ToUpperInvariant())" $shard.PerformanceCount
        if ($accounted -ne $shard.Collected.Count) {
            throw "Executed outcome count does not match shard $($shard.Name.ToUpperInvariant()) collection."
        }
        Assert-ExactOutcomes $shard.Run $shard.Collected
        $totalAccounted += $accounted
        $wall = [math]::Round(($shard.Run.Process.ExitTime - $shard.Run.StartTime).TotalSeconds, 3)
        Write-Output "SHARD $($shard.Name.ToUpperInvariant()) ($accounted nodes; process wall $wall seconds):"
        if ($stdout) { Write-Output $stdout.TrimEnd() }
        if ($stderr) { [Console]::Error.WriteLine($stderr.TrimEnd()) }
    }
    Assert-Budget
    $elapsed = [math]::Round($started.Elapsed.TotalSeconds, 3)
    if ($elapsed -ge $TimeoutSeconds) {
        $script:timedOut = $true
        throw "Normal test suite exceeded its $TimeoutSeconds-second deadline during aggregation."
    }
    $collectionSeconds = [math]::Round($collectionElapsed, 3)
    $executionSeconds = [math]::Round($executionElapsed - $collectionElapsed, 3)
    $aggregationSeconds = [math]::Round($elapsed - $executionElapsed, 3)
    Write-Output "NORMAL TEST PHASES: collection $collectionSeconds seconds; execution $executionSeconds seconds; aggregation $aggregationSeconds seconds."
    Write-Output "NORMAL TEST SUITE PASSED: $totalAccounted accounted nodes; elapsed $elapsed seconds (cap $TimeoutSeconds seconds)."
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
    $env:WEDL_TEST_OUTCOMES = $previousOutcomes
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
