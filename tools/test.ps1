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
    # Representative observed call durations; shorter cases receive a measured-work fallback.
    $observedSeconds = @{
        "tests/test_changeset.py::test_changeset_journal_surfaces_deferral_recovery_and_replay" = 18.886
        "tests/test_changeset.py::test_legacy_receipt_fault_hook_cannot_publish_a_partial_receipt" = 5.146
        "tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[100663296]" = 7.845
        "tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[1]" = 9.488
        "tests/test_changeset.py::test_non_cache_revision_entries_count_toward_scan_limit" = 9.898
        "tests/test_changeset.py::test_oversized_existing_cache_defers_before_compiler_construction" = 7.258
        "tests/test_changeset.py::test_revision_enumeration_limit_defers_without_building_cache" = 7.742
        "tests/test_changeset.py::test_surface_publication_failure_restores_cache_and_receipt_together" = 12.416
        "tests/test_chronology_web_ui.py::test_chronology_ui_assets_are_served_and_the_legacy_catalog_has_an_honest_panel" = 5.916
        "tests/test_generational_api.py::test_discovery_bootstrap_horizon_labels_and_cursor" = 21.223
        "tests/test_generational_api.py::test_discovery_overlong_names_close_direct_and_http" = 10.045
        "tests/test_generational_api.py::test_discovery_prefix_includes_supplementary_unicode_direct_and_http" = 6.518
        "tests/test_generational_api.py::test_discovery_uses_compiled_rows_and_rebuilds_malformed_cache" = 6.775
        "tests/test_generational_api.py::test_former_roles_name_first_cli_http_and_openapi" = 16.879
        "tests/test_generational_api.py::test_name_first_parents_horizons_and_character_controls" = 7.530
        "tests/test_generational_api.py::test_read_operations_and_scope_bound_cursor" = 19.253
        "tests/test_generational_api.py::test_reverse_hidden_and_future_organization_selector_parity" = 9.763
        "tests/test_generational_api.py::test_reverse_legacy_index_rebuilds_from_unchanged_source" = 5.969
        "tests/test_generational_api.py::test_reverse_union_nested_limit_direct_cli_http_schema_parity" = 5.397
        "tests/test_generational_authoring.py::test_starter_and_eight_kind_batch_share_one_confirmed_compiled_commit" = 71.800
        "tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields0-object-object_affordances]" = 6.210
        "tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields1-object-object_affordances]" = 5.824
        "tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields3-character-capabilities]" = 5.508
        "tests/test_object_affordances.py::test_upgrade_preview_apply_replay_detail_and_rollback" = 41.506
        "tests/test_object_affordances.py::test_v07_validation_rejects_mixed_non_object_and_bad_values" = 9.833
        "tests/test_performance_cache.py::test_in_memory_authoring_cache_matches_direct_compiler_projection" = 5.357
        "tests/test_performance_cache.py::test_malformed_profile_in_valid_cache_defers_without_losing_source_commit" = 5.910
        "tests/test_spatial_api.py::test_codec_cli_http_share_one_exact_spatial_outcome" = 19.966
        "tests/test_spatial_api.py::test_legacy_bbox_compiled_capability_absence_is_a_typed_public_unavailable" = 6.822
        "tests/test_spatial_api.py::test_rounded_two_edge_metric_is_typed_unavailable_across_public_transports" = 13.982
        "tests/test_spatial_authoring.py::test_malformed_spatial_authoring_enums_and_numbers_are_wedl_http_errors" = 6.782
        "tests/test_spatial_authoring.py::test_map_create_preview_apply_and_exact_replay_use_real_journal" = 26.159
        "tests/test_spatial_authoring.py::test_overlay_create_and_route_update_preview_apply_and_replay_through_real_journal" = 16.551
        "tests/test_spatial_authoring.py::test_spatial_location_update_keeps_detail_context_and_whereabouts_compatible" = 11.838
        "tests/test_spatial_authoring.py::test_spatial_source_and_receipt_roll_back_on_journal_publication_failure" = 10.918
        "tests/test_spatial_explorer_api.py::test_all_place_modes_and_map_features_match_authored_source" = 6.522
        "tests/test_spatial_explorer_api.py::test_catalog_places_viewport_layers_direct_http_and_selected_cursor" = 9.750
        "tests/test_spatial_explorer_api.py::test_five_http_only_routes_openapi_and_examples" = 5.285
        "tests/test_spatial_explorer_api.py::test_forged_cursor_ordinals_and_surrogates_close_as_invalid" = 5.163
        "tests/test_spatial_explorer_api.py::test_non_ok_state_matrix_and_guard_parity" = 8.887
        "tests/test_spatial_explorer_api.py::test_routes_direct_http_direction_modes_closed_and_cursor" = 6.364
        "tests/test_spatial_explorer_api.py::test_routes_nonself_two_way_directed_cards_and_mounted_http" = 6.637
        "tests/test_spatial_release_contract.py::test_authored_deep_wide_cycle_and_unknown_costs" = 8.380
        "tests/test_suite_replan_boundaries.py::test_small_generational_transport_matrix_and_closed_revision" = 49.036
        "tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.3]" = 19.877
        "tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.5]" = 22.893
        "tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.6]" = 28.314
        "tests/test_suite_replan_boundaries.py::test_small_spatial_source_compiled_rebuild_and_closed_transport" = 10.773
        "tests/test_suite_replan_boundaries.py::test_small_thread_filter_preserves_rank_and_never_backfills" = 0.016
        "tests/test_thread_compile_integration.py::test_valid_v05_compiles_publishes_threads_and_keeps_search_vector_inputs_stable" = 5.388
    }
    $weighted = foreach ($node in $nodes) {
        $weight = if ($observedSeconds.ContainsKey($node)) { [double]$observedSeconds[$node] } else { 0.5 }
        [pscustomobject]@{ Node = $node; Weight = $weight }
    }
    $ordered = @($weighted | Sort-Object -Property @{ Expression = "Weight"; Descending = $true }, @{ Expression = "Node"; Descending = $false })
    $shards = @(
        [pscustomobject]@{ Name = "a"; Nodes = [System.Collections.Generic.List[string]]::new(); Collected = @(); Weight = 0.0; PerformanceCount = 0 },
        [pscustomobject]@{ Name = "b"; Nodes = [System.Collections.Generic.List[string]]::new(); Collected = @(); Weight = 0.0; PerformanceCount = 0 },
        [pscustomobject]@{ Name = "c"; Nodes = [System.Collections.Generic.List[string]]::new(); Collected = @(); Weight = 0.0; PerformanceCount = 0 },
        [pscustomobject]@{ Name = "d"; Nodes = [System.Collections.Generic.List[string]]::new(); Collected = @(); Weight = 0.0; PerformanceCount = 0 },
        [pscustomobject]@{ Name = "e"; Nodes = [System.Collections.Generic.List[string]]::new(); Collected = @(); Weight = 0.0; PerformanceCount = 0 }
    )
    $legacyLifecycle = @($ordered | Where-Object {
        $_.Node.StartsWith('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[')
    })
    $generationalTransport = @($ordered | Where-Object {
        $_.Node -eq 'tests/test_suite_replan_boundaries.py::test_small_generational_transport_matrix_and_closed_revision'
    })
    $spatialLocality = @($ordered | Where-Object {
        $_.Node.StartsWith('tests/test_spatial_api.py::') -or
        $_.Node.StartsWith('tests/test_spatial_explorer_api.py::') -or
        $_.Node -eq 'tests/test_suite_replan_boundaries.py::test_small_spatial_source_compiled_rebuild_and_closed_transport'
    })
    $other = @($ordered | Where-Object {
        -not $_.Node.StartsWith('tests/test_spatial_api.py::') -and
        -not $_.Node.StartsWith('tests/test_spatial_explorer_api.py::') -and
        -not $_.Node.StartsWith('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[') -and
        $_.Node -ne 'tests/test_suite_replan_boundaries.py::test_small_generational_transport_matrix_and_closed_revision' -and
        $_.Node -ne 'tests/test_suite_replan_boundaries.py::test_small_spatial_source_compiled_rebuild_and_closed_transport'
    })
    # Keep costly S fixture families together while balancing measured work.
    foreach ($item in $legacyLifecycle) {
        $shards[0].Nodes.Add($item.Node)
        $shards[0].Weight += $item.Weight
    }
    foreach ($item in $generationalTransport) {
        $shards[1].Nodes.Add($item.Node)
        $shards[1].Weight += $item.Weight
    }
    foreach ($item in $spatialLocality) {
        $target = @($shards[2..3] | Sort-Object -Property Weight, Name)[0]
        $target.Nodes.Add($item.Node)
        $target.Weight += $item.Weight
    }
    foreach ($item in $other) {
        $target = @($shards | Sort-Object -Property Weight, Name)[0]
        $target.Nodes.Add($item.Node)
        $target.Weight += $item.Weight
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
        Write-Output "SHARD $($shard.Name.ToUpperInvariant()) COLLECTION: $($shard.Collected.Count) nodes; estimated call weight $([math]::Round($shard.Weight, 3)) seconds."
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
