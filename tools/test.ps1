[CmdletBinding()]
param(
    [ValidateRange(1, 800)]
    [int]$TimeoutSeconds = 800
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
    # Scheduling estimates: 500 setup + call + teardown totals from run f30116dde90a4ba6a9f6e5dadb8b583f.
    # Entries marked historical retain 96 earlier estimates, not measurements from that run.
    # Estimates guide balancing; collection, startup and contention also affect the deadline.
    $observedSeconds = [System.Collections.Generic.Dictionary[string,double]]::new([System.StringComparer]::Ordinal)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_conversation_discovery_examples_show_typed_beats_but_keep_legacy_turns_speech_only', 0.023211800027638674)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_every_contract_success_example_validates_against_its_component', 0.7326517999172211)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_every_declared_error_has_a_structured_example', 3.842504700180143)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_authoring_examples_cover_eight_kinds_and_four_variants', 0.4966196003369987)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_nested_responses_require_cited_closed_shapes', 0.055254999781027436)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_parser_contract_and_closed_request_variants', 0.5867930999957025)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_reference_errors_unknown_context_and_schema_are_closed', 0.5817586001940072)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_handler_shaped_context_and_conversation_variants_validate', 0.026265700114890933)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_authoring_union_rejects_title_update_and_static_validity', 0.06291889981366694)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_command_http_request_and_status_matrix_is_exact', 0.023313000099733472)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_explorer_http_only_examples_and_closed_outcomes', 0.5740529000759125)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_route_filters_allow_omitted_nullable_modes_but_remain_closed', 0.09298290009610355)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_timeline_presence_and_conversation_beats_have_typed_safe_projections', 0.03965909988619387)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_timeline_schema_requires_exact_string_coordinates_and_named_entries', 0.07008539978414774)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_whereabouts_schema_keeps_journeys_named_and_coordinates_exact', 0.018342099618166685)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_anchor_aliases_rejections_mapping_order_and_era_overlap', 0.011952400207519531)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_civil_axis_precision_approximation_and_conflict_cases', 0.010960499988868833)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_thread_membership_does_not_change_shared_axis_or_story_time', 0.009896100033074617)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_vector_is_parseable_concrete_and_adr_defines_every_semantic_id', 0.015326300170272589)
    $observedSeconds.Add('tests/test_causality.py::test_causal_cli_and_api_contract_expose_the_same_read_surface', 2.586438700091094)
    $observedSeconds.Add('tests/test_causality.py::test_causal_coordinates_reject_cross_timeline_and_same_coordinate_edges', 0.16328939981758595)
    $observedSeconds.Add('tests/test_causality.py::test_causal_directions_and_same_tick_horizon_do_not_leak', 1.2661011000163853)
    $observedSeconds.Add('tests/test_causality.py::test_causality_is_named_deterministic_and_horizon_clipped', 1.6701553000602871)
    $observedSeconds.Add('tests/test_causality.py::test_causality_rejects_noncanonical_focus_without_a_key_error', 0.14938979991711676)
    $observedSeconds.Add('tests/test_causality.py::test_disjoint_fronts_rejoin_only_through_explicit_causal_edges', 0.8647013001609594)
    $observedSeconds.Add('tests/test_causality.py::test_event_cause_rules_retain_direct_edges_and_reject_invalid_graphs', 0.12113400013186038)
    $observedSeconds.Add('tests/test_causality.py::test_plot_trail_hides_a_future_typed_cause_even_for_invalid_source', 0.10869829985313118)
    $observedSeconds.Add('tests/test_causality.py::test_plot_trails_and_advisories_are_horizon_sliced', 0.1331782997585833)
    $observedSeconds.Add('tests/test_causality.py::test_typed_cause_plot_and_scene_outcome_relations_are_checked', 0.9592243998777121)
    $observedSeconds.Add('tests/test_changeset.py::test_changeset_journal_surfaces_deferral_recovery_and_replay', 42.099574299994856)
    $observedSeconds.Add('tests/test_changeset.py::test_commit_before_image_capacity_rejected_before_repository_commit', 8.08470889995806)
    $observedSeconds.Add('tests/test_changeset.py::test_legacy_receipt_fault_hook_cannot_publish_a_partial_receipt', 13.686279699904844)
    $observedSeconds.Add('tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[100663296]', 14.42598140006885)
    $observedSeconds.Add('tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[1]', 15.786113899666816)
    $observedSeconds.Add('tests/test_changeset.py::test_non_cache_revision_entries_count_toward_scan_limit', 18.6781794000417)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_author_impact_fails_before_preview_or_ref', 4.8263778002001345)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_existing_cache_defers_before_compiler_construction', 10.050436499994248)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_request_rejects_before_hash_or_source_metadata', 5.034323500236496)
    $observedSeconds.Add('tests/test_changeset.py::test_revision_enumeration_limit_defers_without_building_cache', 14.083027199609205)
    $observedSeconds.Add('tests/test_changeset.py::test_source_capacity_rejected_before_preview[16777217]', 6.922047699801624)
    $observedSeconds.Add('tests/test_changeset.py::test_source_capacity_rejected_before_preview[None]', 6.767542599933222)
    $observedSeconds.Add('tests/test_changeset.py::test_surface_publication_failure_restores_cache_and_receipt_together', 28.26342840003781)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target0]', 0.000725599704310298) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target1]', 0.0006177998147904873)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target2]', 0.0009354001376777887) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target3]', 0.0006453001406043768)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target4]', 0.0008285997901111841) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_open_ranges_and_bounded_conflicts_are_closed_public_shapes', 0.0005391999147832394)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value0]', 0.0006387997418642044) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value1]', 0.0007547999266535044) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value2]', 0.0007343001198023558) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value3]', 0.0005761999636888504)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value4]', 0.0007299000862985849) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_are_strings_and_round_trip_without_integer_leakage', 0.0006609000265598297) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[ 1]', 0.0006277000065892935)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[+1]', 0.0005600000731647015)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[-0]', 0.0006440998986363411)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[01]', 0.0006733001209795475)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[0]', 0.0005184998735785484)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[1.0]', 0.0007023999933153391) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[9223372036854775808]', 0.0007957997731864452) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[True]', 0.0005080001428723335) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_read_conflicts_are_not_advertised_for_format_or_convert_but_depth_is_bounded', 0.0009500000160187483)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[None-None]', 0.0006217001937329769)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[None-upper2]', 0.0007494997698813677)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[lower0-upper0]', 0.0006265002302825451)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[lower1-None]', 0.0009144998621195555) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_conflict_depth_is_source_aligned_and_never_recurses_unbounded', 0.0012171000707894564)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_extensions_preserve_every_declaration_and_value_boundary', 0.0006065997295081615)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value0-civil]', 0.0005864000413566828)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value1-era]', 0.0007993001490831375) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value2-approx]', 0.0008018999360501766) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value3-relative]', 0.0006510999519377947) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value4-duration]', 0.0006493001710623503)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value5-conflict]', 0.0007554003968834877) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_catalog_replacement_requires_complete_collections_and_scoped_temporary_ids', 0.0007122999522835016) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_public_chronology_authoring_transcodes_only_exact_identifier_fields', 0.0005271998234093189) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_qualitative_approximation_omits_its_public_calendar_id', 0.000930099980905652) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_relative_and_duration_annotations_round_trip_to_source_grammar', 0.0006097001023590565) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_mapping_ambiguity_and_query_advisory_are_explicit_source_and_strict', 7.303612799849361)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_public_outcome_and_advisory_matrix_is_exact_source_and_strict', 5.287282299948856)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_public_reads_have_source_and_strict_compiled_parity', 8.529665699927136)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_validation_mutations_and_cli_http_strict_read_parity', 1.6601705998182297)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_packaged_conformance_fixture_is_valid_and_declares_release_coverage', 0.8120546001009643)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_release_docs_share_the_local_v06_capability_boundary', 0.0013414998538792133)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_anchor_duplicate_tie_cross', 0.0007630998734384775) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_axis_round_trip_negative_year_and_monotonicity', 0.001898100133985281)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_cycle_layout_and_table_inversion', 0.0009126001968979836) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_era_and_open_range_semantics_remain_noncanonical', 0.0007215000223368406) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_exact_reserved_ids', 0.0007909000851213932) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_malformed_era_is_invalid_not_exception', 0.0006896001286804676)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_nested_anchor_and_era_bounds_are_total_invalid_results', 0.0006345000583678484) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_table_gap_is_unavailable_but_bad_epoch_is_definition_error', 0.0007698999252170324) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_table_requires_at_least_one_year', 0.0007257999386638403) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_total_boundaries_and_caps', 0.002595700090751052)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_total_malformed_and_definition_endpoint_classifications', 0.00055720005184412)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_zero_skip_i64_and_approx_conflict_refusal', 0.0006941000465303659)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_compatibility_matrix_and_exact_migration_protocol_are_stable', 0.009563199942931533)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_cross_contract_persists_group_membership_as_threads_never_thread_ids', 0.0006550999823957682)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_document_binds_atomic_preview_apply_and_disposable_cache_policy', 0.009628499625250697)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_invalid_pinned_legacy_or_transformed_v06_candidate_writes_nothing', 0.012147299945354462)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_legacy_chronology_is_rejected_before_transform_without_writing_input', 0.009008699795231223)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_rollback_pointer_uses_only_the_literal_preview_backup_ref', 0.0020267998334020376)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_same_v1_wire_parity_uses_current_hash_order_and_response_identity', 0.012601899914443493)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v03_and_v05_goldens_transform_only_the_documented_fields', 0.007625799858942628)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v05_grouping_data_is_preserved_without_reinterpretation', 0.007051500026136637)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v06_is_an_explicit_no_op_and_invalid_paths_are_stable', 0.009330699918791652)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_anchor_aliases_coalesce_and_positive_boundaries_are_valid', 0.030398400267586112)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_complete_closed_world_and_annotation_shapes', 0.02971499995328486)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_complete_negative_documents_use_the_same_validator', 0.06169450003653765)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_documented_contract_matches_the_pure_closed_oracle', 0.0007020002231001854) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_effective_calendar_civil_intervals_order_partial_bounds', 0.025806800229474902)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_new_generated_chronology_ids_are_uppercase_while_existing_ids_stay_valid', 0.0006973999552428722)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_version_classification_is_distinct_from_reserved_v06_validation', 0.0004952999297529459) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_x_extensions_are_accepted_without_changing_closed_meaning', 0.028269500005990267)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_zero_skipping_era_display_ordinals_are_checked_and_affine', 0.02588260010816157)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_anchor_map_failures_have_exact_authored_leaves', 0.0008792998269200325)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_calendar_diagnostics_preserve_source_identity_and_exact_field', 0.0006606001406908035)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_canonical_definition_id_collisions_are_owned_by_second_source_id', 0.0008858998771756887)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_checked_arithmetic_keeps_the_authored_operand_leaf', 0.0029262001626193523)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_complete_schema_positive_corpus_is_clean_through_the_boundary', 0.04067740007303655)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_definition_and_annotation_replay_stays_at_authored_leaves', 0.0013209001626819372)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_empty_table_is_a_single_precise_definition_error', 0.0006677000783383846)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_endpoint_layout_validation_precedes_range_and_approximation_ordering', 0.0010716000106185675)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_endpoint_normalization_replays_axis_offset_at_authored_precision_before_ordering', 0.0021697997581213713)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_era_bounds_use_selected_layout_endpoints_then_lower_for_reversal', 0.0007641997653990984)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_era_conversion_replays_checked_subtraction_then_addition', 0.0009825997985899448)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_every_schema_negative_document_runs_through_the_source_boundary', 0.10403369995765388)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_existing_runtime_versions_stay_on_the_existing_validator_path', 0.0006117997691035271) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_mixed_versions_are_rejected_without_generic_reference_noise', 0.0006977000739425421) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_nested_chronology_is_owned_by_the_v06_boundary_not_generic_refs', 0.0006869002245366573)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_nested_lifecycle_chronology_and_one_sided_approximation', 0.0006127997767180204)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_prepared_kernel_cycle_prefix_and_calendar_gate_keep_exact_leaves', 0.0012297998182475567)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_production_registry_markdown_and_schema_vector_are_bidirectionally_exact', 0.01652750000357628)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_published_endpoint_precedence_and_checked_arithmetic_examples_are_exact', 0.01583430008031428)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_relative_null_and_x_nested_chronology_keep_precise_paths', 0.0006795001681894064)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_selected_signed_year_layout_drives_epoch_date_range_and_approximation_leaves', 0.0016827001236379147)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_table_gap_is_a_date_diagnostic_at_the_authored_year_leaf', 0.000550700118765235)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_v06_validation_declares_the_active_read_side_boundary', 0.0008067998569458723) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_valid_candidate_is_clean_for_active_v06_read_side_support', 0.0006113999988883734)
    $observedSeconds.Add('tests/test_chronology_web_ui.py::test_chronology_ui_assets_are_served_and_the_legacy_catalog_has_an_honest_panel', 7.550620200112462)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_command_is_parser_defined_and_does_not_require_a_repository', 0.09141190024092793)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_docs_cover_one_session_persistent_setup_and_raw_output_contract', 0.0006927000358700752)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_scripts_are_deterministic_and_cover_parser_commands_and_options', 0.027346099726855755)
    $observedSeconds.Add('tests/test_cli_completion.py::test_top_level_help_includes_completion_and_first_run_sequence', 0.0423872999381274)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_help_exposes_canonical_kinds_and_numeric_limits', 0.04135360009968281)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments0-wedl entity list-invalid choice]', 0.0345184002071619)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments1-wedl search-must be between 1 and 50]', 0.03771609999239445)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments2-wedl search-must be between 1 and 50]', 0.021716299932450056)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments3-wedl context-must be at least 1800]', 0.026193700032308698)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments4-wedl context-must be at least 1]', 0.022745100082829595)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments5-wedl serve-must be between 1 and 65535]', 0.035720399813726544)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments6-wedl serve-must be between 1 and 65535]', 0.03209879994392395)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_parser_accepts_each_validation_boundary', 0.1687042999546975)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_parser_accepts_exactly_the_canonical_entity_kinds', 0.4965325999073684)
    $observedSeconds.Add('tests/test_cli_serve.py::test_browser_open_failure_has_a_manual_url_fallback', 0.0006266999989748001)
    $observedSeconds.Add('tests/test_cli_serve.py::test_generational_cli_preserves_semantic_exit_matrix', 0.267325600143522)
    $observedSeconds.Add('tests/test_cli_serve.py::test_local_server_url_brackets_ipv6', 0.0005411002784967422) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_keeps_the_server_loopback_only', 0.0006641000509262085) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_rejects_a_nonloopback_hostname_resolution_before_binding', 0.0005308997351676226)
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_reports_port_conflicts_with_actionable_details', 0.0006486000493168831)
    $observedSeconds.Add('tests/test_cli_serve.py::test_ready_server_calls_back_once_across_repeated_startups', 0.002280200133100152)
    $observedSeconds.Add('tests/test_cli_serve.py::test_ready_server_shuts_down_when_the_ready_callback_fails', 0.0021299002692103386)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_dispatch_announces_ready_without_opening_browser', 0.022272700211033225)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_dispatch_opens_once_after_ready_and_compact_announces_url', 0.021680900128558278)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_help_explains_readiness_and_opt_in_opening', 0.02360179997049272)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_open_failure_is_structured_after_readiness', 0.03727019974030554)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_port_preflight_failure_is_structured_and_does_not_start_server', 0.02694679982960224)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_ready_output_flushes_for_piped_callers', 0.03821649984456599)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_rejects_an_invalid_parseable_world_before_socket_activity', 0.02211020002141595)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_rejects_git_root_without_a_wedl_world_before_socket_activity', 0.032757099717855453)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_repository_preflight_failure_is_structured_and_does_not_bind', 1.8570877003949136)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_translates_a_port_collision_after_preflight_before_uvicorn_starts', 0.021377400029450655)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_concurrent_scenes_reject_overlapping_present_casts', 6.26619160012342)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_every_active_scene_cursor_must_exactly_match_the_world_cursor', 9.575153199955821)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_explicit_singleton_world_cursor_must_match_its_active_scene', 5.79202020005323)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_historical_double_booking_names_the_character_and_scenes', 7.094626300036907)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_historical_split_and_reunion_with_independent_characters_is_valid', 4.828311600023881)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_implicit_scene_is_ambiguous_generically_but_resolves_for_a_character', 8.877067000139505)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_location_link_mapping_requires_exactly_one_valid_target_alias', 6.235152699751779)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_location_parents_and_routes_are_kind_safe_and_directional', 8.752748400205746)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_malformed_location_parent_is_diagnostic_not_a_cycle_checker_crash', 5.760303100105375)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_multiple_active_scenes_require_and_accept_a_shared_cursor', 5.987483200151473)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_coordinate_event_participation_requires_one_place', 6.02097179973498)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_coordinate_two_place_presence_is_invalid_but_next_order_handoff_is_valid', 7.12955869990401)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_story_time_state_writes_are_a_validation_conflict', 7.328068099915981)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[cause-replacement]', 0.0031733999494463205)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[single]', 0.006044199923053384)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[two-causes]', 0.003231699811294675)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[uncaused]', 0.0022369998041540384)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_composite_delta_separates_static_sections_and_literal_consequences', 0.005239600082859397)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_created_deleted_records_and_indirect_plot_changes_are_honest', 0.00939009990543127)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_delta_bounds_and_invalid_candidate_close_without_partial_success', 0.009980900213122368)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_delta_identity_purity_order_and_json_presence_comparison', 0.008270899998024106)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_future_and_denied_source_sections_do_not_change_delta_or_counts', 0.010138100013136864)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_revision_horizon_delta_is_distinct_from_local_event_time_and_noop', 0.011971499770879745)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_batch_direct_cli_http_raw_body_confirmation_and_full_intent_binding', 70.99395579984412)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_context_revision_and_source_refusal_preserve_loaded_cache', 1.807361600222066)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_exact_git_context_direct_cli_http_parity_and_old_revision_freshness', 35.32440529996529)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_http_context_auth_and_unknown_grants_refuse_before_load', 10.555755000328645)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_parser_discovery_components_and_batch_examples_share_closed_variants', 0.1131301000714302)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_schema_static_cli_is_repository_free_and_context_grammar_is_exact', 0.2680622001644224)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_aggregate_check_item_and_serialized_copy_limits_are_closed', 4.1519272001460195)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_check_refusal_confirmation_and_normal_write_record_exact_report', 34.65712139988318)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_check_time_policy_and_literals_bind_confirmation_and_named_batch', 1.5233785000164062)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_checked_receipt_replays_before_later_head_planning_without_cache_or_index_writes', 66.82012440008111)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_checks_use_final_candidate_original_indexes_and_one_evaluation', 0.8691311997827142)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_closed_check_grammar_and_invalid_source_never_evaluate', 0.2829802001360804)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_receipt_completion_boundary_recovers_equal_head_and_retries_exact_result[receipt-after-completion-True]', 27.277942900313064)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_receipt_completion_boundary_recovers_equal_head_and_retries_exact_result[receipt-before-completion-False]', 29.205992000177503)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_required_advisory_unknown_and_unsupported_without_optional_delta', 1.2379523999989033)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_all_seven_predicates_expose_typed_actuals_and_preserve_check_order', 0.0011823000386357307)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_author_section_privacy_hidden_causes_and_denied_records_do_not_prove_absence', 0.003585800062865019)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_belief_status_is_not_canonical_truth_and_delayed_transition_is_literal', 0.0058195998426526785)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_candidate_comparison_provenance_and_world_cache_purity', 0.0022162001114338636)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_closed_grammar_finite_values_and_strict_signed_time_coordinates', 0.0015964999329298735)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_complete_item_and_byte_limits_never_return_partial_checks', 0.006858399836346507)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_exact_json_null_absence_entity_leaves_and_declared_value_types', 0.0046530000399798155)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_relationship_partial_metrics_outcome_halves_and_advisory_gating', 0.0024558999575674534)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_unknown_unsupported_wrong_kind_and_focus_failure_are_distinct', 0.003257999662309885)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_ambiguous_unknown_and_stale_plans_reuse_resolver_without_writes', 2.863110300153494)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_batch_bounds_precede_reads_and_valid_preview_is_pure', 1.4706528999377042)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_closed_batch_shape_exact_times_and_reference_kinds', 0.8224973997566849)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_complete_intent_confirmation_binds_aliases_and_internal_guard', 0.2173457001335919)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_confirmed_batch_writer_refuses_changed_intent_and_replays_before_resolution', 60.02415560022928)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_every_event_auxiliary_form_preserves_literal_values_and_supplied_ids', 1.6375068000052124)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_generic_operations_and_nested_state_resolve_only_declared_leaves', 2.5571429999545217)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_named_temporary_batches_preserve_history_and_cursor', 0.17308359988965094)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_typed_genealogy_beliefs_reuse_existing_evidence_admission', 0.19266930012963712)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_closed_inputs_auxiliary_identity_and_literal_extensions', 0.15889900014735758)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_compound_operations_preserve_history_and_same_event_order', 0.1728690997697413)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_confirmed_atomic_writes_refusal_and_receipt_replay', 51.437476199818775)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_outcome_links_are_explicit_ordered_and_idempotent', 0.8383867000229657)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_rejects_causes_order_kinds_and_final_conflicts', 0.37030520010739565)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_typed_creates_reuse_genealogy_and_homogeneous_source_validation', 0.47647170000709593)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_actual_repository_semantic_preview_is_pure_and_apply_rechecks_identity', 42.33562669972889)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_closed_request_failures_and_invalid_candidate_never_fold', 0.5201457000803202)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_explicit_horizon_limits_and_optional_request_bind_source_identity', 2.020237600430846)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_head_drift_does_not_substitute_loaded_revision', 0.6943829997908324)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_metadata_retains_every_noop_index_and_reciprocal_outcome_target', 0.23137940000742674)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_named_batch_and_raw_preview_share_ids_delta_but_keep_intent_confirmation', 0.39259770000353456)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_semantic_preview_uses_one_base_final_candidate_and_original_hash', 0.7707923001144081)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_semantic_success_failure_preserves_files_and_parser_cache', 1.5012846002355218)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_actual_preview_closed_semantic_outcomes_and_signed_horizons', 1.3884105999022722)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_actual_rescue_generated_provenance_same_h_delta_and_event_local_t', 1.1991139999590814)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_rescue_discovery_example_and_closed_preview_failure_serialization', 0.2275561997666955)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_rescue_real_direct_cli_http_schema_and_confirmation_parity', 65.15130600030534)
    $observedSeconds.Add('tests/test_consequence_report.py::test_consolidated_report_keeps_occurrence_local_changes_and_current_status_separate', 0.004176700254902244)
    $observedSeconds.Add('tests/test_consequence_report.py::test_genealogy_occurrence_preserves_author_provenance_and_unlearned_is_unknown', 0.004364799940958619)
    $observedSeconds.Add('tests/test_consequence_report.py::test_later_focus_caused_learning_is_history_not_other_event_supersession', 0.003598499810323119)
    $observedSeconds.Add('tests/test_consequence_report.py::test_noop_effects_null_clear_and_root_empty_report_do_not_claim_completeness', 0.00656270026229322)
    $observedSeconds.Add('tests/test_consequence_report.py::test_reciprocal_story_point_asymmetries_are_advisory_and_scene_half_is_normal', 0.00482069980353117)
    $observedSeconds.Add('tests/test_consequence_report.py::test_report_aggregate_bounds_and_invalid_source_emit_only_failure', 0.00403139996342361)
    $observedSeconds.Add('tests/test_consequence_report.py::test_report_candidate_provenance_cache_isolation_and_deterministic_order', 0.004514699801802635)
    $observedSeconds.Add('tests/test_consequence_report.py::test_report_horizon_scope_privacy_and_signed_event_extrema_are_exact', 0.017207000171765685)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_batch_intent_generic_op_openness_links_and_wire_time_match_runtime', 0.019549500197172165)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_closed_appends_match_normalizer_missing_null_coercion_and_auxiliary_ids', 0.0237598002422601)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_context_exact_revision_sanitized_hints_metrics_and_evaluator_parity', 0.004878500243648887)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_context_refuses_denied_unrepresentable_and_unsupported_capabilities_without_leaks', 0.003301100106909871)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_create_histories_are_open_distinct_from_closed_appends_and_source_variants', 0.011378399794921279)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_predicate_and_result_conformance_uses_real_evaluator_actuals', 0.04455500026233494)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_raw_event_classifier_is_disjoint_and_preserves_legacy_open_shapes', 0.008849000092595816)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_real_report_delta_check_only_and_failure_components_are_closed', 0.04228599974885583)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_registry_meta_refs_dispatch_and_openapi_adapters_share_one_definition', 0.18274240009486675)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_signed_wire_time_bounds_reject_source_coercion_and_source_formats_remain_separate', 0.005000500241294503)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_closed_request_before_repository_and_invalid_source_before_scope_or_folds', 0.30487489979714155)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_exact_git_direct_cli_http_purity_statuses_and_deferred_cache', 29.79938690015115)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_horizon_canonical_focus_and_eligible_name_resolution', 1.106163099873811)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_http_auth_precedes_source_and_exact_revision_survives_head_race', 22.486025999998674)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_parser_discovery_closed_components_statuses_and_temporal_description', 0.05287269991822541)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_public_combined_item_exact_limit_and_actual_serialized_byte_limits', 2.127682900056243)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_raw_duplicate_http_members_authentication_redaction_and_unique_control', 19.107092100195587)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_raw_duplicate_members_cli_file_stdin_and_decoder_fail_before_source', 1.3267463999800384)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_required_advisory_unknown_unsupported_and_empty_checks_are_read_outcomes', 1.3172659003175795)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_source_report_flat_checks_same_world_scope_and_cache_purity', 0.48883629986085)
    $observedSeconds.Add('tests/test_conversation_web_ui.py::test_conversation_web_ui_uses_author_scopes_and_safe_static_contract', 9.328114799922332)
    $observedSeconds.Add('tests/test_event_consequences.py::test_authorized_temporal_event_absence_preserves_trigger_folds_and_privacy', 0.004403700120747089)
    $observedSeconds.Add('tests/test_event_consequences.py::test_delayed_transitions_forgotten_rejected_and_canonical_folds', 0.002340999897569418)
    $observedSeconds.Add('tests/test_event_consequences.py::test_equal_time_uncaused_transitions_and_explicit_reverse_links', 0.0015286998823285103)
    $observedSeconds.Add('tests/test_event_consequences.py::test_event_local_extrema_order_and_disjoint_coordinates', 0.00273319985717535)
    $observedSeconds.Add('tests/test_event_consequences.py::test_genealogy_author_occurrence_and_unlearned_fold_boundary', 0.0012244000099599361)
    $observedSeconds.Add('tests/test_event_consequences.py::test_input_cache_isolation_candidate_provenance_and_ordered_citations', 0.0018581999465823174)
    $observedSeconds.Add('tests/test_event_consequences.py::test_limits_identity_and_source_only_dependency_boundary', 0.0016551001463085413)
    $observedSeconds.Add('tests/test_event_consequences.py::test_scope_horizon_sections_and_excluded_poisoning', 0.010118900099769235)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_bootstrap_horizon_labels_and_cursor', 30.344467099988833)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_overlong_names_close_direct_and_http', 28.095977399963886)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_prefix_includes_supplementary_unicode_direct_and_http', 15.646675799973309)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_request_bounds_match_openapi', 17.055659499950707)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_uses_compiled_rows_and_rebuilds_malformed_cache', 15.83629129990004)
    $observedSeconds.Add('tests/test_generational_api.py::test_former_roles_name_first_cli_http_and_openapi', 47.88874019985087)
    $observedSeconds.Add('tests/test_generational_api.py::test_name_first_parents_horizons_and_character_controls', 27.375022999709472)
    $observedSeconds.Add('tests/test_generational_api.py::test_read_operations_and_scope_bound_cursor', 56.88331769988872)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_hidden_and_future_organization_selector_parity', 33.79360889992677)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_legacy_index_rebuilds_from_unchanged_source', 26.5934389999602)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_read_horizons_stale_revision_and_closed_modes', 24.60052809980698)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_union_nested_limit_direct_cli_http_schema_parity', 11.660682100104168)
    $observedSeconds.Add('tests/test_generational_authoring.py::test_starter_and_eight_kind_batch_share_one_confirmed_compiled_commit', 96.61850880016573)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ancestry_item_bound_counts_only_visible_active_edges', 0.054922800278291106)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ancestry_orders_each_depth_globally_by_authored_time', 0.07335360022261739)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_cited_bounded_traversal_and_private_search_candidates', 0.05374670005403459)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_distinct_authored_parentage_to_same_parent_keeps_both_citations', 0.05376179958693683)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_missing_or_malformed_generational_shape_rejects_cache', 0.07766390009783208)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_private_discovery_shape_and_generation_reject_old_cache', 0.04614010010845959)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_private_structural_search_shape_is_required', 0.014556000242009759)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ratified_vector_compiles_all_kinds_and_replays_boundaries', 0.08351429970934987)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reparent_cycle_fails_closed_without_partial_containment_path', 0.05622110003605485)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_replacement_reparenting_and_timeline_are_exact', 0.05091329989954829)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reverse_legacy_organization_index_shape_is_required', 0.013578099897131324)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reverse_parentage_index_is_required_and_has_parent_lead', 0.010618400294333696)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_shared_full_and_fast_forward_paths_have_identical_rows', 0.1003276000265032)
    $observedSeconds.Add('tests/test_generational_context.py::test_closed_character_reads_make_no_context', 0.0006778002716600895)
    $observedSeconds.Add('tests/test_generational_context.py::test_context_uses_only_authorized_results', 0.0006155997980386019)
    $observedSeconds.Add('tests/test_generational_context.py::test_subquery_traversal_limit_closes_entire_packet', 0.0005371998995542526)
    $observedSeconds.Add('tests/test_generational_context.py::test_writing_context_uses_literal_learned_genealogy_and_history', 0.19696940016001463)
    $observedSeconds.Add('tests/test_generational_fixture.py::test_bounded_builder_is_valid_reproducible_and_linked', 0.5416482002474368)
    $observedSeconds.Add('tests/test_generational_fixture.py::test_bounded_source_compiled_privacy_horizon_and_confirmed_transports', 199.93204520014115)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_adr_is_accepted_indexed_and_contract_only', 0.000534099992364645) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_literal_frontmatter_uses_one_transition_model', 0.1400353000499308)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_migration_binds_closed_target_capabilities_to_preview_apply', 0.005075299879536033)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_query_vectors_close_privacy_time_and_citation_applicability', 0.05466140015050769)
    $observedSeconds.Add('tests/test_generational_knowledge_compiler.py::test_knowledge_only_cache_rejects_old_missing_stale_and_damaged_projection', 0.3096421000082046)
    $observedSeconds.Add('tests/test_generational_knowledge_compiler.py::test_literal_assertions_replay_learning_applicability_and_neutral_evidence', 0.07763079996220767)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_parent_learning_is_literal_cited_and_trusted_api_only', 0.13834780012257397)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_paths_are_complete_literal_uncertain_and_cycle_bounded', 0.17166469991207123)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_scope_search_and_unlearned_mutations_stay_closed', 0.12448340025730431)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_union_roles_holders_claims_and_vital_remain_literal', 0.056056000059470534)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_held_historical_names_context_and_discovery_preserve_exact_learning', 0.18655440001748502)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_exact_evidence_is_historical_admitted_and_same_character', 0.035545899998396635)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_retrospective_assertion_learning_and_new_correction_preserve_boundaries', 0.05220450018532574)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_typed_assertion_literals_are_closed_immutable_and_roundtrip', 0.03710980014875531)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_typed_knowledge_exports_do_not_grant_author_only_provenance', 0.1473956003319472)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_wrong_conflicting_and_cyclic_beliefs_opt_in_without_canon_matching', 0.04096939996816218)
    $observedSeconds.Add('tests/test_generational_knowledge_transport.py::test_confirmed_knowledge_authoring_and_cli_http_viewpoints', 305.9513209001161)
    $observedSeconds.Add('tests/test_generational_knowledge_transport.py::test_knowledge_intents_compile_closed_history_and_explicit_opt_in', 0.035909000085666776)
    $observedSeconds.Add('tests/test_generational_knowledge_transport.py::test_viewpoint_and_knowledge_openapi_catalogue_is_closed', 1.2702634003944695)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_absent_future_secret_are_identical_and_scope_is_closed', 0.048075899947434664)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_and_public_leak_matrix_for_claim_role_vital_and_search', 0.0490081999450922)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_malformed_search_and_relative_target_are_invalid', 0.05029070004820824)
    $observedSeconds.Add('tests/test_generational_query.py::test_containment_one_parent_fits_one_item', 0.04926839983090758)
    $observedSeconds.Add('tests/test_generational_query.py::test_cursor_is_bound_to_scope_and_revision', 0.04751209984533489)
    $observedSeconds.Add('tests/test_generational_query.py::test_discovery_closes_overlong_titles_and_aliases', 0.05833679996430874)
    $observedSeconds.Add('tests/test_generational_query.py::test_discovery_name_is_admitted_at_exact_same_tick_order', 0.048768100095912814)
    $observedSeconds.Add('tests/test_generational_query.py::test_former_roles_exact_horizon_visibility_role_and_combined_cap', 0.07994590001180768)
    $observedSeconds.Add('tests/test_generational_query.py::test_former_roles_request_is_strictly_opt_in', 0.05182499997317791)
    $observedSeconds.Add('tests/test_generational_query.py::test_parent_item_limit_counts_only_active_authorized_edges', 0.05218469980172813)
    $observedSeconds.Add('tests/test_generational_query.py::test_parentage_descendants_and_boundary_order', 0.047975999768823385)
    $observedSeconds.Add('tests/test_generational_query.py::test_pinned_git_entry_closes_failed_and_malformed_target_replies', 1.7739252001047134)
    $observedSeconds.Add('tests/test_generational_query.py::test_pinned_git_entry_pairs_lookup_and_checks_integrity_each_read', 6.301154900342226)
    $observedSeconds.Add('tests/test_generational_query.py::test_relative_path_merges_equal_depth_edges_by_global_applicability', 0.056416000006720424)
    $observedSeconds.Add('tests/test_generational_query.py::test_relatives_are_one_continuous_cited_path_in_both_directions', 0.054955200059339404)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_missing_stale_damaged_cache_stays_fail_closed', 0.24355309992097318)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_rejects_alias_and_missing_revision', 0.06395210023038089)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_resolves_once_and_preserves_ready_answers', 0.10666480008512735)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_target_changes_remain_fresh', 0.47665240010246634)
    $observedSeconds.Add('tests/test_generational_query.py::test_reverse_union_membership_and_organization_legacies_fold_before_budget', 0.05465390020981431)
    $observedSeconds.Add('tests/test_generational_query.py::test_source_compiled_and_rebuilt_query_parity', 0.29427940025925636)
    $observedSeconds.Add('tests/test_generational_query.py::test_union_history_and_active_roster_obey_items_limit', 0.05700960010290146)
    $observedSeconds.Add('tests/test_generational_query.py::test_union_organization_legacy_vital_and_private_search', 0.06090159993618727)
    $observedSeconds.Add('tests/test_generational_query.py::test_vital_direct_and_search_agree_before_birth_and_when_withheld', 0.06155949993990362)
    $observedSeconds.Add('tests/test_generational_source.py::test_generational_generated_paths_are_canonical', 0.0005491999909281731) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_accessor_is_immutable_and_rejects_extra_leaf_fields', 0.0005053002387285233) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[organization-initialize-payload0]', 0.000740900170058012) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[organization-initialize-payload1]', 0.00157589977607131)
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[parentage-initialize-payload2]', 0.0005886000581085682)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[aliases-bad_value6]', 0.0005234999116510153) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[audience-bad_value5]', 0.0005188998766243458) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[domain-history]', 0.000864299712702632) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[organization_kind-bad_value3]', 0.0007352998945862055)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[status-bad_value2]', 0.0005403000395745039) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[threads-bad_value4]', 0.0004475000314414501) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_tenure_transfer_requires_a_cause_event', 0.0006033999379724264)
    $observedSeconds.Add('tests/test_generational_validation.py::test_all_eight_runtime_kinds_accept_and_cross_record_integrity_rejects', 0.004539299989119172)
    $observedSeconds.Add('tests/test_generational_validation.py::test_closed_state_tables_cover_every_generational_kind_and_vital_sequence', 0.00030039972625672817) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_validation.py::test_generational_serialization_keeps_transition_and_payload_order', 0.0007783002220094204)
    $observedSeconds.Add('tests/test_generational_validation.py::test_generational_validation_accepts_literal_union_and_rejects_bad_payload', 0.0007536998018622398)
    $observedSeconds.Add('tests/test_generational_validation.py::test_transition_replacement_cause_and_malformed_leaves_are_total', 0.000795499887317419)
    $observedSeconds.Add('tests/test_generational_validation.py::test_validation_is_total_and_reports_exact_transition_leaves_and_paths', 0.0006111001130193472)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_admission_and_commit_share_one_deadline_cell', 14.233945900108665)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_consumer_persistent_lock_keeps_head_source_and_index', 12.87093469989486)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_consumer_waits_for_a_transient_lock', 12.77286050003022)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_released_migration_lock_rechecks_head_before_creating_absent_backup', 7.286254199920222)
    $observedSeconds.Add('tests/test_multi_strand_continuity_adr.py::test_multi_strand_adr_is_historical_and_explicitly_superseded', 0.0007374000269919634)
    $observedSeconds.Add('tests/test_multi_strand_schema_contract.py::test_withdrawn_continuity_schema_material_is_nonnormative_only', 0.0006701000966131687)
    $observedSeconds.Add('tests/test_no_v04_continuity_residue.py::test_runtime_has_no_withdrawn_v04_continuity_residue', 0.01941169984638691)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields0-object-object_affordances]', 18.818997799884528)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields1-object-object_affordances]', 22.962203100090846)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields2-object-capabilities]', 20.302536599803716)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields3-character-capabilities]', 22.232148400042206)
    $observedSeconds.Add('tests/test_object_affordances.py::test_upgrade_preview_apply_replay_detail_and_rollback', 86.38128149998374)
    $observedSeconds.Add('tests/test_object_affordances.py::test_v07_validation_rejects_mixed_non_object_and_bad_values', 35.36948290001601)
    $observedSeconds.Add('tests/test_performance_cache.py::test_bounded_git_tree_metadata_defers_before_world_load', 5.245202600024641)
    $observedSeconds.Add('tests/test_performance_cache.py::test_in_memory_authoring_cache_matches_direct_compiler_projection', 23.840866999933496)
    $observedSeconds.Add('tests/test_performance_cache.py::test_malformed_or_unknown_git_tree_metadata_has_no_capacity_proof', 2.767000300111249)
    $observedSeconds.Add('tests/test_performance_cache.py::test_malformed_profile_in_valid_cache_defers_without_losing_source_commit', 18.34112050035037)
    $observedSeconds.Add('tests/test_performance_cache.py::test_non_source_git_tree_output_is_bounded_before_world_load', 5.713050400372595)
    $observedSeconds.Add('tests/test_performance_cache.py::test_sqlite_serialization_failure_defers_without_shared_cache_write', 6.313316200161353)
    $observedSeconds.Add('tests/test_performance_cache.py::test_unknown_authoring_capacity_defers_before_world_load', 6.497058899607509)
    $observedSeconds.Add('tests/test_semantics.py::test_expanded_knowledge_appears_at_correct_time', 8.997047999873757)
    $observedSeconds.Add('tests/test_semantics.py::test_story_point_history_is_not_rewritten', 8.883394900010899)
    $observedSeconds.Add('tests/test_semantics.py::test_temporal_object_holder', 9.020588599843904)
    $observedSeconds.Add('tests/test_shared_world_thread_schema_contract.py::test_v05_migration_and_quarantined_v04_recovery_are_narrow_and_noninferential', 0.004758500028401613)
    $observedSeconds.Add('tests/test_shared_world_thread_schema_contract.py::test_v05_thread_reservation_has_one_global_timeline_and_grouping_only_membership', 0.0031989002600312233)
    $observedSeconds.Add('tests/test_shared_world_threads_adr.py::test_coordination_manifest_withdraws_only_thread_reservation_and_preserves_other_projects', 0.0016881998162716627)
    $observedSeconds.Add('tests/test_shared_world_threads_adr.py::test_shared_world_thread_adr_governs_one_global_world_without_schema_invention', 0.0017234997358173132)
    $observedSeconds.Add('tests/test_source_validation.py::test_crlf_frontmatter_envelope_is_accepted', 0.0006986001972109079) # Historical estimate.
    $observedSeconds.Add('tests/test_source_validation.py::test_duplicate_yaml_keys_are_rejected', 0.0006782999262213707) # Historical estimate.
    $observedSeconds.Add('tests/test_source_validation.py::test_expanded_fixture_validates_and_has_conversations', 9.252273099729791)
    $observedSeconds.Add('tests/test_source_validation.py::test_local_migration_is_cli_only', 0.04678869992494583)
    $observedSeconds.Add('tests/test_source_validation.py::test_yaml_sequence_mapping_keys_are_parse_errors_not_type_errors', 0.0006784000433981419)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_adr_is_accepted_indexed_and_explicitly_non_runtime', 0.0006584001239389181) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_adr.py::test_explorer_adjunct_decision_records_bounded_selected_lens', 0.0008191999513655901) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_adr.py::test_query_vectors_close_states_budgets_time_and_audience', 0.13668979983776808)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_spatial_shape_preserves_coordinate_free_worlds_and_non_inference', 0.005963099887594581)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_version_migration_and_cross_project_boundary_are_closed', 0.004495999775826931)
    $observedSeconds.Add('tests/test_spatial_api.py::test_capability_order_and_operation_fields_are_closed_before_lookup', 0.0007853996939957142) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_codec_cli_http_share_one_exact_spatial_outcome', 95.40947520011105)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[9007199254740992]', 0.0005626999773085117)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[True]', 0.000596500001847744) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[inf]', 0.0004952000454068184) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_hidden_overlay_and_no_overlay_control_have_identical_public_result', 10.400135700358078)
    $observedSeconds.Add('tests/test_spatial_api.py::test_legacy_bbox_compiled_capability_absence_is_a_typed_public_unavailable', 22.71329959970899)
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[0]', 0.000667099840939045) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[1.0]', 0.0008021998219192028) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[101]', 0.0007377995643764734) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[True]', 0.0008262000046670437) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[adjacency]', 0.0005693999119102955)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[bbox]', 0.0005634999834001064)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[children]', 0.0005915998481214046)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[containment]', 0.0007494997698813677) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[nearby]', 0.0005962001159787178) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[overlay-as-of]', 0.0005768998526036739)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[path]', 0.000583600252866745)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[reachability]', 0.000559699721634388)
    $observedSeconds.Add('tests/test_spatial_api.py::test_rounded_two_edge_metric_is_typed_unavailable_across_public_transports', 43.69513339991681)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[+1]', 0.0005886000581085682) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[-0]', 0.0005638001020997763) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[01]', 0.000624299980700016) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[0]', 0.0007114000618457794) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[9223372036854775808]', 0.0005970997735857964)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[True]', 0.0005763000808656216) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[bbox-relation]', 6.777498699724674)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[overlay-as-of-queryScope]', 7.754467299906537)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[path-metric]', 3.916915000183508)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.location.update-payload4]', 0.004271100275218487)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.map.create-payload0]', 0.003897999878972769)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.map.update-payload3]', 0.00447099981829524)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.overlay.create-payload2]', 0.003860299941152334)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.overlay.update-payload6]', 0.0043029997032135725)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.route.create-payload1]', 0.004612100077793002)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.route.update-payload5]', 0.0039854999631643295)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_intent_payloads_and_version_gate_are_closed', 0.0066369997803121805)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_malformed_spatial_authoring_enums_and_numbers_are_wedl_http_errors', 43.50680699991062)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_map_create_preview_apply_and_exact_replay_use_real_journal', 102.26236309995875)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_overlay_create_and_route_update_preview_apply_and_replay_through_real_journal', 87.4806590997614)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_apply_refuses_unconfirmed_bypass_before_repository_access', 0.008695099968463182)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_location_update_keeps_detail_context_and_whereabouts_compatible', 61.63407460018061)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_preview_is_read_only_for_source_cache_receipt_and_head', 16.88159490004182)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_source_and_receipt_roll_back_on_journal_publication_failure', 55.983412100235)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.location.update-payload1]', 0.012249700026586652) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.map.update-payload0]', 0.012577200075611472)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.overlay.update-payload3]', 0.01669690036214888)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.route.update-payload2]', 0.0038568000309169292)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_all_place_modes_and_map_features_match_authored_source', 55.143627899931744)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_catalog_places_viewport_layers_direct_http_and_selected_cursor', 50.896125599974766)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_compiled_read_errors_redact_host_paths', 6.0903590999078006)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_compiled_same_tick_overlay_respects_both_order_boundaries', 0.010967600159347057)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_five_http_only_routes_openapi_and_examples', 0.40131250000558794)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_forged_cursor_ordinals_and_surrogates_close_as_invalid', 23.786894400138408)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_layer_pages_bind_lens_horizon_and_member_order', 0.009002699982374907)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_legacy_catalog_is_useful_and_non_map_reads_close', 5.644819299923256)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_non_ok_state_matrix_and_guard_parity', 44.151023800019175)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_places_modes_lens_and_closed_validation', 7.6700800999533385)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_direct_http_direction_modes_closed_and_cursor', 13.11565149994567)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_nonself_two_way_directed_cards_and_mounted_http', 15.087831299984828)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_two_way_self_loop_and_position_portal_compiled_cards', 0.00772370002232492)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_signed_large_bounds_and_unsafe_geometry_close', 1.2726709002163261)
    $observedSeconds.Add('tests/test_spatial_query.py::test_absent_endpoints_are_unavailable_but_valid_empty_controls_stay_ok', 0.007756599923595786)
    $observedSeconds.Add('tests/test_spatial_query.py::test_bbox_requires_geometry_capability_and_map_dimensionality', 0.008037099847570062)
    $observedSeconds.Add('tests/test_spatial_query.py::test_bbox_same_map_pagination_and_cursor_revision_binding', 0.009009699802845716)
    $observedSeconds.Add('tests/test_spatial_query.py::test_canonical_query_vector_and_source_compiled_fixture_stay_in_parity', 0.023367000045254827)
    $observedSeconds.Add('tests/test_spatial_query.py::test_children_bbox_within_adjacency_and_portal_reachability_are_authored_only', 0.007581700105220079)
    $observedSeconds.Add('tests/test_spatial_query.py::test_context_preserves_validated_registry_order_for_combined_capabilities', 0.016916500171646476)
    $observedSeconds.Add('tests/test_spatial_query.py::test_directed_path_does_not_infer_reverse_and_unknown_metric_is_closed', 0.007758100284263492)
    $observedSeconds.Add('tests/test_spatial_query.py::test_explorer_bbox_requires_map_scoped_rtree_but_legacy_bbox_stays_available', 0.00901399995200336)
    $observedSeconds.Add('tests/test_spatial_query.py::test_failed_rtree_population_leaves_no_partial_explorer_index', 0.008025499992072582)
    $observedSeconds.Add('tests/test_spatial_query.py::test_failed_temporal_index_population_removes_both_explorer_rtrees', 0.009364000288769603)
    $observedSeconds.Add('tests/test_spatial_query.py::test_hierarchy_is_authored_parent_path_and_coordinate_free_locations_are_readable', 0.008306299801915884)
    $observedSeconds.Add('tests/test_spatial_query.py::test_nearby_is_same_map_candidate_read_not_authored_topology', 0.007758800173178315)
    $observedSeconds.Add('tests/test_spatial_query.py::test_nearby_target_radius_aliases_and_exact_radius_postfilter', 0.007613200228661299)
    $observedSeconds.Add('tests/test_spatial_query.py::test_overlay_story_time_never_crosses_timelines_or_converts_duration', 0.00882920017465949)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_keeps_incompatible_units_in_separate_typed_graphs', 0.010351800126954913)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_preserves_exact_large_integers_overflow_and_near_equal_ordering', 0.015038199722766876)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_refuses_to_rank_mixed_units_and_preserves_requested_identity_unit', 0.007938799913972616)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_reports_downstream_closed_and_restricted_edges', 0.00789789971895516)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_uses_exact_mixed_float_comparison_and_authored_tie_keys', 0.014192799804732203)
    $observedSeconds.Add('tests/test_spatial_query.py::test_positive_keyset_cursor_paging_is_complete_and_request_bound', 0.007943600183352828)
    $observedSeconds.Add('tests/test_spatial_query.py::test_repeated_path_order_is_stable_across_cycles_and_insertion_order', 0.007613100111484528)
    $observedSeconds.Add('tests/test_spatial_query.py::test_signed_i64_story_time_boundaries_and_cross_map_discontinuity_are_closed', 0.008229599799960852)
    $observedSeconds.Add('tests/test_spatial_query.py::test_story_time_rtree_boxes_cover_exact_signed_tick_order_ranges', 0.0005517001263797283) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_query.py::test_success_summaries_expose_closed_partial_unknown_filter_and_horizon_semantics', 0.009818700142204762)
    $observedSeconds.Add('tests/test_spatial_query.py::test_typed_path_has_stable_ties_and_respects_mode_and_availability', 0.009504499845206738)
    $observedSeconds.Add('tests/test_spatial_release_contract.py::test_authored_deep_wide_cycle_and_unknown_costs', 80.03690730012022)
    $observedSeconds.Add('tests/test_spatial_release_contract.py::test_mixed_legacy_source_is_rejected_without_side_effects', 9.991766499821097)
    $observedSeconds.Add('tests/test_spatial_source.py::test_spatial_ids_are_narrow_and_do_not_change_legacy_id_acceptance', 0.0003955999854952097) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capabilities_are_closed_and_protocol_ordered', 0.00032600038684904575) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value0-None]', 0.0006585000082850456) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value1-expected1]', 0.00066359993070364)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value10-expected10]', 0.0008255001157522202) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value2-expected2]', 0.0007616996299475431)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value3-None]', 0.0004803999327123165) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value4-None]', 0.0006750999018549919) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value5-None]', 0.0006884003523737192)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value6-expected6]', 0.000650499714538455)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value8-expected8]', 0.0005152998492121696) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value9-expected9]', 0.0007885999511927366) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_accessor_and_validator_reject_the_same_spatial_leaf_shapes', 0.005201800027862191)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_anchor_conversion_and_detailed_location_link_rules_are_shared_and_canonical', 0.008983999956399202)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_arbitrary_record_frontmatter_never_raises', 0.006637199781835079)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_fixture_covers_coordinate_free_maps_routes_and_timed_overlays', 0.002450300147756934)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_paths_ordering_and_runtime_registration_are_canonical', 0.0026631001383066177)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_rejects_nonfinite_geometry_cycles_and_unapproved_boundary', 0.003075100015848875)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_requires_world_ordered_capability_declaration_and_keeps_legacy_inert', 0.0036289000418037176)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_diagnostic_catalog_is_closed', 0.0027009998448193073)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_envelope_and_map_diagnostics_identify_the_exact_leaf', 0.004757799906656146)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_feature_gates_legacy_locations_and_total_malformed_yaml_diagnostics', 0.005457899998873472)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_frontmatter_round_trip_preserves_v07_body_but_keeps_legacy_canonical_bytes', 0.0005330999847501516)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generational_only_envelope_accepts_coordinate_free_records_but_gates_spatial_features', 0.0032705997582525015)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generic_v07_validation_composes_core_and_record_time_checks', 0.0037757998798042536)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generic_validation_uses_component_envelope_and_never_writes', 0.003002599813044071)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_invalid_fixture_bool_nan_and_duplicate_edges_are_leaf_diagnostics', 0.0029327997472137213)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_multimap_geodetic_anchor_portal_and_legacy_location_fixture', 0.0039013002533465624)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_typed_accessors_are_deeply_immutable_and_include_optional_values', 0.0023704003542661667)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_envelope_legacy_links_and_geometry_variants_are_literal_not_routes', 0.003059800248593092)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_envelope_uses_full_chronology_validation_and_listed_default_timeline', 0.0024894000962376595)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_location_accepts_inherited_chronology_annotations', 0.0027406001463532448)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_parent_compatibility_input_serializes_to_adr_canonical_parent_id', 0.003535099793225527)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_z_policy_is_consistent_for_bounds_origin_geometry_and_accessors', 0.00493840011768043)
    $observedSeconds.Add('tests/test_spatial_validation.py::test_v07_component_is_not_generic_source_acceptance', 0.000855900114402175)
    $observedSeconds.Add('tests/test_story_points_web_ui.py::test_story_points_web_ui_uses_selected_scene_and_safe_static_contract', 8.711652799975127)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_generational_transport_matrix_and_closed_revision', 188.02498590014875)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.3]', 77.661806399934) # Historical estimate.
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.5]', 43.728785400046036)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.6]', 141.79998229979537)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_spatial_source_compiled_rebuild_and_closed_transport', 30.463866399833933) # Historical estimate.
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_thread_filter_preserves_rank_and_never_backfills', 0.0006157001480460167) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_invalid_v05_stops_before_database_revision_or_vector_publication', 7.713743700180203) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_v04_compiled_cache_metadata_is_incompatible', 9.58133629988879)
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_valid_v05_compiles_publishes_threads_and_keeps_search_vector_inputs_stable', 10.291463200002909)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_current_fingerprint_v04_cache_entry_is_evicted_before_record_rehydration', 11.730768699664623)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_exact_v04_is_quarantined_before_record_construction_with_recovery_link', 0.0005359000060707331) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_model_source.py::test_thread_ids_are_non_entity_identifiers_and_source_schema_cache_contract', 0.00037180003710091114) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_model_source.py::test_v05_source_round_trip_canonicalizes_thread_lists_without_synthesizing_memberships', 0.0006524000782519579)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_v05_world_and_record_expose_grouping_membership_without_entity_refs', 0.0005652999971061945) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_catalog_preserves_v04_quarantine_boundary', 5.083259199978784)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_catalog_require_compiled_never_rebuilds_when_cache_is_missing', 19.027949900133535)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_v03_catalog_has_no_synthetic_grouping_and_cli_http_query_parity', 21.792298800079152)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_v05_catalog_is_sorted_and_discloses_only_public_declarations', 7.7868085000664) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_search_filter.py::test_resolve_thread_filter_requires_normalized_declared_v05_ids', 0.0013834999408572912)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_capture_rejects_parent_reparse_substitution', 0.006410300265997648) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_capture_rejects_restored_mtime_content_change', 0.010112399701029062)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_exact_limit_and_growth', 0.0019973001908510923) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_rejects_nonregular_path', 0.001274800393730402)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_revalidates_named_path', 0.0032042998354882) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_commit_before_limit_preserves_ref_index_and_source', 3.1404650998301804)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_complete_index_cas_preserves_an_unrelated_staged_entry', 14.974513599881902)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_external_touched_entry_after_wait_aborts_before_ref_or_source_publish', 10.174493899801746) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_ignores_overridden_str_methods', 0.0005467000883072615) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[all-controls]', 0.0005584999453276396) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[astral]', 0.00052429991774261) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[high-surrogate]', 0.0007170001044869423)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[latin-one]', 0.0007066002581268549) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[long-base64]', 0.017130600288510323)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[low-surrogate]', 0.0004849000833928585) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[plain]', 0.0005447000730782747) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[quotes-and-slashes]', 0.0006388998590409756) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[repeated-controls]', 0.0008618002757430077) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_wire_bound_rejects_malformed_values_and_overflow', 0.000828899908810854) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_persistent_or_substituted_external_lock_never_mutates_it', 3.2493601001333445)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_transient_external_lock_releases_within_the_private_window', 13.706208900082856)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_two_external_lock_seams_share_the_first_lazy_deadline', 0.6049504999537021) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_budgeted_load_reuses_one_authenticated_document_and_exact_images', 0.007862999802455306)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_default_legacy_and_authoritative_cleanup_limits_preserve_loader_contract', 0.013224800117313862)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_graph_bound_exact_budget_admission_and_checked_overflow', 0.0012058999855071306)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_graph_bound_exact_lexical_corpus_and_escape_parity', 0.05683470005169511)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_loaded_identity_and_same_byte_journal_cas_substitution_still_refuse', 0.015194199979305267)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_same_size_descriptor_mutation_re_admits_joined_graph_before_key_sets', 0.0141721002291888)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_check_reports_drift_without_modifying_packages', 0.07570480019785464) # Historical estimate.
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_explicit_bootstrap_choices_keep_legacy_default', 0.04655329999513924)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_hardlinked_bytecode_refused_before_publication', 0.06505510024726391)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_hardlinked_destination_cannot_overwrite_pinned_source', 0.022290100110694766)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_missing_legacy_record_fails_before_publication', 0.56812380021438)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_mixed_schema_fails_before_publication', 0.013157299952581525)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_semantic_projection_preserves_absent_empty_order_and_duplicates', 0.0005891001783311367) # Historical estimate.
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_small_packaged_upgrade_stale_head_and_forward_rollback', 90.07262120000087)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_symlinked_destination_cannot_overwrite_pinned_source', 0.049831599928438663)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_unexpected_bytecode_entries_refused_without_publication', 3.151629399973899)
    # The 17 nodes without measured or historical estimates retain this nominal fallback.
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
