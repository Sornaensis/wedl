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
    # Scheduling estimates: 500 setup + call + teardown totals from run b8bee54070dc4e4685efbf0d4ab4bc85.
    # Entries marked historical retain 107 earlier estimates; six unmeasured nodes keep the nominal fallback.
    # Fixture groups sum complete observations, including private copies and existing cold-start charges.
    # Newly first generational consumers may incur setup and lazy compilation in call; no saving is deducted.
    # These historical weights do not bound future startup, fixture composition, or host contention.
    $observedSeconds = [System.Collections.Generic.Dictionary[string,double]]::new([System.StringComparer]::Ordinal)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_conversation_discovery_examples_show_typed_beats_but_keep_legacy_turns_speech_only', 0.01888160011731088)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_every_contract_success_example_validates_against_its_component', 0.6736469999887049)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_every_declared_error_has_a_structured_example', 3.403841300169006)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_authoring_examples_cover_eight_kinds_and_four_variants', 0.41183410002849996)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_nested_responses_require_cited_closed_shapes', 0.05263729952275753)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_parser_contract_and_closed_request_variants', 0.5358436000533402)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_generational_reference_errors_unknown_context_and_schema_are_closed', 0.529482499929145)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_handler_shaped_context_and_conversation_variants_validate', 0.026875300332903862)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_authoring_union_rejects_title_update_and_static_validity', 0.06925179995596409)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_command_http_request_and_status_matrix_is_exact', 0.0004922000225633383) # Historical estimate.
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_explorer_http_only_examples_and_closed_outcomes', 0.5851749998982996)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_spatial_route_filters_allow_omitted_nullable_modes_but_remain_closed', 0.08672319981269538)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_timeline_presence_and_conversation_beats_have_typed_safe_projections', 0.034482300048694015)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_timeline_schema_requires_exact_string_coordinates_and_named_entries', 0.06793899997137487)
    $observedSeconds.Add('tests/test_api_response_schemas.py::test_whereabouts_schema_keeps_journeys_named_and_coordinates_exact', 0.018139300169423223)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_anchor_aliases_rejections_mapping_order_and_era_overlap', 0.01031550019979477)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_civil_axis_precision_approximation_and_conflict_cases', 0.009903100319206715)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_thread_membership_does_not_change_shared_axis_or_story_time', 0.011605700012296438)
    $observedSeconds.Add('tests/test_calendar_chronology_adr.py::test_vector_is_parseable_concrete_and_adr_defines_every_semantic_id', 0.010393899865448475)
    $observedSeconds.Add('tests/test_causality.py::test_causal_cli_and_api_contract_expose_the_same_read_surface', 2.181633099913597)
    $observedSeconds.Add('tests/test_causality.py::test_causal_coordinates_reject_cross_timeline_and_same_coordinate_edges', 0.14059219975024462)
    $observedSeconds.Add('tests/test_causality.py::test_causal_directions_and_same_tick_horizon_do_not_leak', 0.10326280002482235)
    $observedSeconds.Add('tests/test_causality.py::test_causality_is_named_deterministic_and_horizon_clipped', 2.1515980002004653)
    $observedSeconds.Add('tests/test_causality.py::test_causality_rejects_noncanonical_focus_without_a_key_error', 0.09084810013882816)
    $observedSeconds.Add('tests/test_causality.py::test_disjoint_fronts_rejoin_only_through_explicit_causal_edges', 0.7947233999148011)
    $observedSeconds.Add('tests/test_causality.py::test_event_cause_rules_retain_direct_edges_and_reject_invalid_graphs', 0.8589967000298202)
    $observedSeconds.Add('tests/test_causality.py::test_plot_trail_hides_a_future_typed_cause_even_for_invalid_source', 0.1411795001477003)
    $observedSeconds.Add('tests/test_causality.py::test_plot_trails_and_advisories_are_horizon_sliced', 0.12445229990407825)
    $observedSeconds.Add('tests/test_causality.py::test_typed_cause_plot_and_scene_outcome_relations_are_checked', 0.715521200094372)
    $observedSeconds.Add('tests/test_changeset.py::test_changeset_journal_surfaces_deferral_recovery_and_replay', 39.48939860006794)
    $observedSeconds.Add('tests/test_changeset.py::test_commit_before_image_capacity_rejected_before_repository_commit', 6.633542499970645)
    $observedSeconds.Add('tests/test_changeset.py::test_legacy_receipt_fault_hook_cannot_publish_a_partial_receipt', 14.863289400469512)
    $observedSeconds.Add('tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[100663296]', 12.221226000227034)
    $observedSeconds.Add('tests/test_changeset.py::test_mandatory_enrollment_capacity_fails_before_ref[1]', 16.404005200369284)
    $observedSeconds.Add('tests/test_changeset.py::test_non_cache_revision_entries_count_toward_scan_limit', 17.742206399794668)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_author_impact_fails_before_preview_or_ref', 7.35670640016906)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_existing_cache_defers_before_compiler_construction', 17.2609183001332)
    $observedSeconds.Add('tests/test_changeset.py::test_oversized_request_rejects_before_hash_or_source_metadata', 5.046024200040847)
    $observedSeconds.Add('tests/test_changeset.py::test_revision_enumeration_limit_defers_without_building_cache', 17.97948839981109)
    $observedSeconds.Add('tests/test_changeset.py::test_source_capacity_rejected_before_preview[16777217]', 5.173962299944833)
    $observedSeconds.Add('tests/test_changeset.py::test_source_capacity_rejected_before_preview[None]', 1.3835590002126992)
    $observedSeconds.Add('tests/test_changeset.py::test_surface_publication_failure_restores_cache_and_receipt_together', 25.6871774001047)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target0]', 0.000725599704310298) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target1]', 0.0005316997412592173)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target2]', 0.0005452001933008432)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target3]', 0.0005780002102255821)
    $observedSeconds.Add('tests/test_chronology_api.py::test_conversion_target_is_closed_nonempty_before_repository_access[target4]', 0.0008285997901111841) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_open_ranges_and_bounded_conflicts_are_closed_public_shapes', 0.0005179997533559799) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value0]', 0.000583200016990304)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value1]', 0.0005268000531941652) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value2]', 0.0007343001198023558) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value3]', 0.0005761999636888504) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_date_identifiers_reject_blank_values[value4]', 0.000512400409206748)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_are_strings_and_round_trip_without_integer_leakage', 0.0006609000265598297) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[ 1]', 0.0005377999041229486) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[+1]', 0.0005717000458389521) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[-0]', 0.0005020000971853733)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[01]', 0.0006733001209795475) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[0]', 0.0005313002038747072)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[1.0]', 0.0005735000595450401)
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[9223372036854775808]', 0.0007957997731864452) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_api.py::test_public_decimal_dates_reject_noncanonical_numbers[True]', 0.0005705999210476875)
    $observedSeconds.Add('tests/test_chronology_api.py::test_read_conflicts_are_not_advertised_for_format_or_convert_but_depth_is_bounded', 0.0010769001673907042)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[None-None]', 0.0005057998932898045)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[None-upper2]', 0.0006287000142037868) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[lower0-upper0]', 0.0006395999807864428)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_approximate_source_bounds_contain_only_civil_endpoints[lower1-None]', 0.0007676002569496632) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_conflict_depth_is_source_aligned_and_never_recurses_unbounded', 0.0010013999417424202)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_extensions_preserve_every_declaration_and_value_boundary', 0.0005898003000766039)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value0-civil]', 0.0005326000973582268)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value1-era]', 0.0005222999025136232)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value2-approx]', 0.0006487001664936543) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value3-relative]', 0.0005092998035252094)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value4-duration]', 0.0006493001710623503) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_authoring_value_extensions_are_opaque_for_every_value_kind[value5-conflict]', 0.0005145999602973461)
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_catalog_replacement_requires_complete_collections_and_scoped_temporary_ids', 0.0007122999522835016) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_public_chronology_authoring_transcodes_only_exact_identifier_fields', 0.0004962999373674393) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_qualitative_approximation_omits_its_public_calendar_id', 0.0004749000072479248) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_authoring.py::test_relative_and_duration_annotations_round_trip_to_source_grammar', 0.0004854998551309109) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_mapping_ambiguity_and_query_advisory_are_explicit_source_and_strict', 5.72529900027439)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_public_outcome_and_advisory_matrix_is_exact_source_and_strict', 5.278250800212845)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_public_reads_have_source_and_strict_compiled_parity', 6.75579069997184)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_fixture_validation_mutations_and_cli_http_strict_read_parity', 5.935454800026491)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_packaged_conformance_fixture_is_valid_and_declares_release_coverage', 0.07815089984796941)
    $observedSeconds.Add('tests/test_chronology_conformance_fixture.py::test_release_docs_share_the_local_v06_capability_boundary', 0.0011029001325368881)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_anchor_duplicate_tie_cross', 0.0007630998734384775) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_axis_round_trip_negative_year_and_monotonicity', 0.0017788999248296022)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_cycle_layout_and_table_inversion', 0.0009126001968979836) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_era_and_open_range_semantics_remain_noncanonical', 0.0005358001217246056)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_exact_reserved_ids', 0.0007909000851213932) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_malformed_era_is_invalid_not_exception', 0.0006896001286804676) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_nested_anchor_and_era_bounds_are_total_invalid_results', 0.0006345000583678484) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_table_gap_is_unavailable_but_bad_epoch_is_definition_error', 0.0007698999252170324) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_table_requires_at_least_one_year', 0.0007257999386638403) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_total_boundaries_and_caps', 0.002088800072669983)
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_total_malformed_and_definition_endpoint_classifications', 0.00055720005184412) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_kernel.py::test_zero_skip_i64_and_approx_conflict_refusal', 0.0006941000465303659) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_compatibility_matrix_and_exact_migration_protocol_are_stable', 0.006839600158855319)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_cross_contract_persists_group_membership_as_threads_never_thread_ids', 0.000695300055667758)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_document_binds_atomic_preview_apply_and_disposable_cache_policy', 0.06366349966265261)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_invalid_pinned_legacy_or_transformed_v06_candidate_writes_nothing', 0.006981599843129516)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_legacy_chronology_is_rejected_before_transform_without_writing_input', 0.0074767000041902065)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_rollback_pointer_uses_only_the_literal_preview_backup_ref', 0.001599899958819151)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_same_v1_wire_parity_uses_current_hash_order_and_response_identity', 0.009982200106605887)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v03_and_v05_goldens_transform_only_the_documented_fields', 0.006796600064262748)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v05_grouping_data_is_preserved_without_reinterpretation', 0.007272299844771624)
    $observedSeconds.Add('tests/test_chronology_migration_contract.py::test_v06_is_an_explicit_no_op_and_invalid_paths_are_stable', 0.007978599984198809)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_anchor_aliases_coalesce_and_positive_boundaries_are_valid', 0.07449520006775856)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_complete_closed_world_and_annotation_shapes', 0.021115400129929185)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_complete_negative_documents_use_the_same_validator', 0.05046589975245297)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_documented_contract_matches_the_pure_closed_oracle', 0.0007020002231001854) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_effective_calendar_civil_intervals_order_partial_bounds', 0.02136800019070506)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_new_generated_chronology_ids_are_uppercase_while_existing_ids_stay_valid', 0.0005976001266390085)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_version_classification_is_distinct_from_reserved_v06_validation', 0.0004952999297529459) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_x_extensions_are_accepted_without_changing_closed_meaning', 0.022718599997460842)
    $observedSeconds.Add('tests/test_chronology_schema_contract.py::test_zero_skipping_era_display_ordinals_are_checked_and_affine', 0.021602400112897158)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_anchor_map_failures_have_exact_authored_leaves', 0.000735999783501029)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_calendar_diagnostics_preserve_source_identity_and_exact_field', 0.0006606001406908035) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_canonical_definition_id_collisions_are_owned_by_second_source_id', 0.000838299747556448)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_checked_arithmetic_keeps_the_authored_operand_leaf', 0.002162099815905094)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_complete_schema_positive_corpus_is_clean_through_the_boundary', 0.03677210002206266)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_definition_and_annotation_replay_stays_at_authored_leaves', 0.0010917000472545624)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_empty_table_is_a_single_precise_definition_error', 0.0006677000783383846) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_endpoint_layout_validation_precedes_range_and_approximation_ordering', 0.0012485000770539045)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_endpoint_normalization_replays_axis_offset_at_authored_precision_before_ordering', 0.0021794000640511513)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_era_bounds_use_selected_layout_endpoints_then_lower_for_reversal', 0.0009274000767618418)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_era_conversion_replays_checked_subtraction_then_addition', 0.0009913998655974865)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_every_schema_negative_document_runs_through_the_source_boundary', 0.0901373999658972)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_existing_runtime_versions_stay_on_the_existing_validator_path', 0.0006117997691035271) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_mixed_versions_are_rejected_without_generic_reference_noise', 0.0006977000739425421) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_nested_chronology_is_owned_by_the_v06_boundary_not_generic_refs', 0.0005646999925374985)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_nested_lifecycle_chronology_and_one_sided_approximation', 0.0006314998026937246)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_prepared_kernel_cycle_prefix_and_calendar_gate_keep_exact_leaves', 0.0009990001562982798)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_production_registry_markdown_and_schema_vector_are_bidirectionally_exact', 0.014106200076639652)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_published_endpoint_precedence_and_checked_arithmetic_examples_are_exact', 0.013645699713379145)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_relative_null_and_x_nested_chronology_keep_precise_paths', 0.0005403000395745039)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_selected_signed_year_layout_drives_epoch_date_range_and_approximation_leaves', 0.001766800181940198)
    $observedSeconds.Add('tests/test_chronology_validation.py::test_table_gap_is_a_date_diagnostic_at_the_authored_year_leaf', 0.0005494998767971992) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_v06_validation_declares_the_active_read_side_boundary', 0.0008067998569458723) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_validation.py::test_valid_candidate_is_clean_for_active_v06_read_side_support', 0.0005355002358555794) # Historical estimate.
    $observedSeconds.Add('tests/test_chronology_web_ui.py::test_chronology_ui_assets_are_served_and_the_legacy_catalog_has_an_honest_panel', 6.041132700163871)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_command_is_parser_defined_and_does_not_require_a_repository', 0.10777710005640984)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_docs_cover_one_session_persistent_setup_and_raw_output_contract', 0.0006462999153882265)
    $observedSeconds.Add('tests/test_cli_completion.py::test_completion_scripts_are_deterministic_and_cover_parser_commands_and_options', 0.02641629963181913)
    $observedSeconds.Add('tests/test_cli_completion.py::test_top_level_help_includes_completion_and_first_run_sequence', 0.0230874998960644)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_help_exposes_canonical_kinds_and_numeric_limits', 0.022548100212588906)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments0-wedl entity list-invalid choice]', 0.023088699905201793)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments1-wedl search-must be between 1 and 50]', 0.025857700034976006)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments2-wedl search-must be between 1 and 50]', 0.029814699897542596)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments3-wedl context-must be at least 1800]', 0.026207000017166138)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments4-wedl context-must be at least 1]', 0.031880300026386976)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments5-wedl serve-must be between 1 and 65535]', 0.024623099947348237)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_invalid_values_are_structured_usage_errors_before_runtime_work[arguments6-wedl serve-must be between 1 and 65535]', 0.022117400309070945)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_parser_accepts_each_validation_boundary', 0.1729110002052039)
    $observedSeconds.Add('tests/test_cli_input_validation.py::test_parser_accepts_exactly_the_canonical_entity_kinds', 0.5194625998847187)
    $observedSeconds.Add('tests/test_cli_serve.py::test_browser_open_failure_has_a_manual_url_fallback', 0.0006266999989748001) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_generational_cli_preserves_semantic_exit_matrix', 0.21720070019364357)
    $observedSeconds.Add('tests/test_cli_serve.py::test_local_server_url_brackets_ipv6', 0.0005411002784967422) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_checks_every_distinct_resolved_bind_candidate', 0.0005042001139372587) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_keeps_the_server_loopback_only', 0.0006641000509262085) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_rejects_a_nonloopback_hostname_resolution_before_binding', 0.0005886999424546957)
    $observedSeconds.Add('tests/test_cli_serve.py::test_preflight_reports_port_conflicts_with_actionable_details', 0.0006486000493168831) # Historical estimate.
    $observedSeconds.Add('tests/test_cli_serve.py::test_ready_server_calls_back_once_across_repeated_startups', 0.001824399922043085)
    $observedSeconds.Add('tests/test_cli_serve.py::test_ready_server_shuts_down_when_the_ready_callback_fails', 0.0015555999707430601)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_dispatch_announces_ready_without_opening_browser', 0.024298599688336253)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_dispatch_opens_once_after_ready_and_compact_announces_url', 0.025197999784722924)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_help_explains_readiness_and_opt_in_opening', 0.02271150005981326)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_open_failure_is_structured_after_readiness', 0.022807500092312694)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_port_preflight_failure_is_structured_and_does_not_start_server', 0.02204589988104999)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_ready_output_flushes_for_piped_callers', 0.031206800136715174)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_rejects_an_invalid_parseable_world_before_socket_activity', 0.024553600000217557)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_rejects_git_root_without_a_wedl_world_before_socket_activity', 0.031221600249409676)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_repository_preflight_failure_is_structured_and_does_not_bind', 0.5628066998906434)
    $observedSeconds.Add('tests/test_cli_serve.py::test_serve_translates_a_port_collision_after_preflight_before_uvicorn_starts', 0.02188099967315793)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_concurrent_scenes_reject_overlapping_present_casts', 6.2983536997344345)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_every_active_scene_cursor_must_exactly_match_the_world_cursor', 11.830954100005329)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_explicit_singleton_world_cursor_must_match_its_active_scene', 5.916775199817494)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_historical_double_booking_names_the_character_and_scenes', 7.020757299847901)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_historical_split_and_reunion_with_independent_characters_is_valid', 12.459310099715367)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_implicit_scene_is_ambiguous_generically_but_resolves_for_a_character', 5.744260200066492)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_location_link_mapping_requires_exactly_one_valid_target_alias', 10.633289899909869)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_location_parents_and_routes_are_kind_safe_and_directional', 8.066261799773201)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_malformed_location_parent_is_diagnostic_not_a_cycle_checker_crash', 7.996902999933809)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_multiple_active_scenes_require_and_accept_a_shared_cursor', 11.81159510044381)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_coordinate_event_participation_requires_one_place', 9.053168100072071)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_coordinate_two_place_presence_is_invalid_but_next_order_handoff_is_valid', 7.322191199986264)
    $observedSeconds.Add('tests/test_concurrent_scenes.py::test_same_story_time_state_writes_are_a_validation_conflict', 4.743772899964824)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[cause-replacement]', 0.0023591998033225536)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[single]', 0.007595500210300088)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[two-causes]', 0.0024904999881982803)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_array_attribution_uses_only_changed_members_and_preserves_whole_sections[uncaused]', 0.00231490028090775)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_composite_delta_separates_static_sections_and_literal_consequences', 0.004334899829700589)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_created_deleted_records_and_indirect_plot_changes_are_honest', 0.005749700125306845)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_delta_bounds_and_invalid_candidate_close_without_partial_success', 0.00981119996868074)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_delta_identity_purity_order_and_json_presence_comparison', 0.00561990006826818)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_future_and_denied_source_sections_do_not_change_delta_or_counts', 0.010037100175395608)
    $observedSeconds.Add('tests/test_consequence_delta.py::test_revision_horizon_delta_is_distinct_from_local_event_time_and_noop', 0.010288200108334422)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_batch_direct_cli_http_raw_body_confirmation_and_full_intent_binding', 60.46300289989449)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_context_revision_and_source_refusal_preserve_loaded_cache', 0.1807113999966532)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_exact_git_context_direct_cli_http_parity_and_old_revision_freshness', 32.50471640005708)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_http_context_auth_and_unknown_grants_refuse_before_load', 12.123400999698788)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_parser_discovery_components_and_batch_examples_share_closed_variants', 0.17730730003677309)
    $observedSeconds.Add('tests/test_consequence_discovery.py::test_schema_static_cli_is_repository_free_and_context_grammar_is_exact', 0.22221690020523965)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_aggregate_check_item_and_serialized_copy_limits_are_closed', 0.8929047000128776)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_check_refusal_confirmation_and_normal_write_record_exact_report', 32.949430799810216)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_check_time_policy_and_literals_bind_confirmation_and_named_batch', 1.3567995000630617)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_checked_receipt_replays_before_later_head_planning_without_cache_or_index_writes', 55.03318120003678)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_checks_use_final_candidate_original_indexes_and_one_evaluation', 0.4289345003198832)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_closed_check_grammar_and_invalid_source_never_evaluate', 0.980899500194937)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_receipt_completion_boundary_recovers_equal_head_and_retries_exact_result[receipt-after-completion-True]', 19.70422799955122)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_receipt_completion_boundary_recovers_equal_head_and_retries_exact_result[receipt-before-completion-False]', 28.37823130004108)
    $observedSeconds.Add('tests/test_consequence_enforcement.py::test_required_advisory_unknown_and_unsupported_without_optional_delta', 0.43108579982072115)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_all_seven_predicates_expose_typed_actuals_and_preserve_check_order', 0.001307099824771285)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_author_section_privacy_hidden_causes_and_denied_records_do_not_prove_absence', 0.003315499983727932)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_belief_status_is_not_canonical_truth_and_delayed_transition_is_literal', 0.002300600055605173)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_candidate_comparison_provenance_and_world_cache_purity', 0.0015841000713407993)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_closed_grammar_finite_values_and_strict_signed_time_coordinates', 0.001911200350150466)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_complete_item_and_byte_limits_never_return_partial_checks', 0.006605600006878376)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_exact_json_null_absence_entity_leaves_and_declared_value_types', 0.003744899993762374)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_relationship_partial_metrics_outcome_halves_and_advisory_gating', 0.002136200200766325)
    $observedSeconds.Add('tests/test_consequence_expectations.py::test_unknown_unsupported_wrong_kind_and_focus_failure_are_distinct', 0.0023074999917298555)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_ambiguous_unknown_and_stale_plans_reuse_resolver_without_writes', 1.5674622997175902)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_batch_bounds_precede_reads_and_valid_preview_is_pure', 0.1384040000848472)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_closed_batch_shape_exact_times_and_reference_kinds', 0.7622013997752219)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_complete_intent_confirmation_binds_aliases_and_internal_guard', 0.9336988998111337)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_confirmed_batch_writer_refuses_changed_intent_and_replays_before_resolution', 56.71375379990786)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_every_event_auxiliary_form_preserves_literal_values_and_supplied_ids', 1.6981939000543207)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_generic_operations_and_nested_state_resolve_only_declared_leaves', 3.1563337000552565)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_named_temporary_batches_preserve_history_and_cursor', 0.20633479999378324)
    $observedSeconds.Add('tests/test_consequence_intents.py::test_typed_genealogy_beliefs_reuse_existing_evidence_admission', 0.8671501001808792)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_closed_inputs_auxiliary_identity_and_literal_extensions', 0.20439970027655363)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_compound_operations_preserve_history_and_same_event_order', 0.15381789999082685)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_confirmed_atomic_writes_refusal_and_receipt_replay', 46.56316769984551)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_outcome_links_are_explicit_ordered_and_idempotent', 0.8999614999629557)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_rejects_causes_order_kinds_and_final_conflicts', 1.8077603997662663)
    $observedSeconds.Add('tests/test_consequence_operations.py::test_typed_creates_reuse_genealogy_and_homogeneous_source_validation', 2.4772630999796093)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_actual_repository_semantic_preview_is_pure_and_apply_rechecks_identity', 41.564580200007185)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_closed_request_failures_and_invalid_candidate_never_fold', 0.49667650018818676)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_explicit_horizon_limits_and_optional_request_bind_source_identity', 1.2084844000637531)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_head_drift_does_not_substitute_loaded_revision', 0.9834930000361055)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_metadata_retains_every_noop_index_and_reciprocal_outcome_target', 0.8783842998091131)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_named_batch_and_raw_preview_share_ids_delta_but_keep_intent_confirmation', 1.1636035998817533)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_semantic_preview_uses_one_base_final_candidate_and_original_hash', 0.6799590999726206)
    $observedSeconds.Add('tests/test_consequence_preview.py::test_semantic_success_failure_preserves_files_and_parser_cache', 0.9348720000125468)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_actual_preview_closed_semantic_outcomes_and_signed_horizons', 1.3199007001239806)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_actual_rescue_generated_provenance_same_h_delta_and_event_local_t', 0.6320268998388201)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_rescue_discovery_example_and_closed_preview_failure_serialization', 0.2203501001931727)
    $observedSeconds.Add('tests/test_consequence_preview_schemas.py::test_rescue_real_direct_cli_http_schema_and_confirmation_parity', 55.633285199990496)
    $observedSeconds.Add('tests/test_consequence_report.py::test_consolidated_report_keeps_occurrence_local_changes_and_current_status_separate', 0.0030550998635590076)
    $observedSeconds.Add('tests/test_consequence_report.py::test_genealogy_occurrence_preserves_author_provenance_and_unlearned_is_unknown', 0.004124300321564078)
    $observedSeconds.Add('tests/test_consequence_report.py::test_later_focus_caused_learning_is_history_not_other_event_supersession', 0.004169799853116274)
    $observedSeconds.Add('tests/test_consequence_report.py::test_noop_effects_null_clear_and_root_empty_report_do_not_claim_completeness', 0.0037652996834367514)
    $observedSeconds.Add('tests/test_consequence_report.py::test_reciprocal_story_point_asymmetries_are_advisory_and_scene_half_is_normal', 0.00475149997510016)
    $observedSeconds.Add('tests/test_consequence_report.py::test_report_aggregate_bounds_and_invalid_source_emit_only_failure', 0.0025720999110490084)
    $observedSeconds.Add('tests/test_consequence_report.py::test_report_candidate_provenance_cache_isolation_and_deterministic_order', 0.0044207999017089605)
    $observedSeconds.Add('tests/test_consequence_report.py::test_report_horizon_scope_privacy_and_signed_event_extrema_are_exact', 0.017072300193831325)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_batch_intent_generic_op_openness_links_and_wire_time_match_runtime', 0.01827509980648756)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_closed_appends_match_normalizer_missing_null_coercion_and_auxiliary_ids', 0.024926000041887164)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_context_exact_revision_sanitized_hints_metrics_and_evaluator_parity', 0.004244599957019091)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_context_refuses_denied_unrepresentable_and_unsupported_capabilities_without_leaks', 0.003204899840056896)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_create_histories_are_open_distinct_from_closed_appends_and_source_variants', 0.009552600095048547)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_predicate_and_result_conformance_uses_real_evaluator_actuals', 0.038813999854028225)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_raw_event_classifier_is_disjoint_and_preserves_legacy_open_shapes', 0.009544300381094217)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_real_report_delta_check_only_and_failure_components_are_closed', 0.03377690026536584)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_registry_meta_refs_dispatch_and_openapi_adapters_share_one_definition', 0.27721920027397573)
    $observedSeconds.Add('tests/test_consequence_schemas.py::test_signed_wire_time_bounds_reject_source_coercion_and_source_formats_remain_separate', 0.0055102999322116375)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_closed_request_before_repository_and_invalid_source_before_scope_or_folds', 0.5624881002586335)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_exact_git_direct_cli_http_purity_statuses_and_deferred_cache', 40.74300709972158)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_horizon_canonical_focus_and_eligible_name_resolution', 1.6550936999265105)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_http_auth_precedes_source_and_exact_revision_survives_head_race', 21.355732300085947)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_parser_discovery_closed_components_statuses_and_temporal_description', 0.0570812001824379)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_public_combined_item_exact_limit_and_actual_serialized_byte_limits', 1.1346302998717874)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_raw_duplicate_http_members_authentication_redaction_and_unique_control', 19.214669699780643)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_raw_duplicate_members_cli_file_stdin_and_decoder_fail_before_source', 1.19944590004161)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_required_advisory_unknown_unsupported_and_empty_checks_are_read_outcomes', 0.5441444001626223)
    $observedSeconds.Add('tests/test_consequence_verification.py::test_source_report_flat_checks_same_world_scope_and_cache_purity', 0.6703766998834908)
    $observedSeconds.Add('tests/test_conversation_web_ui.py::test_conversation_web_ui_uses_author_scopes_and_safe_static_contract', 6.53925110003911)
    $observedSeconds.Add('tests/test_event_consequences.py::test_authorized_temporal_event_absence_preserves_trigger_folds_and_privacy', 0.004430700093507767)
    $observedSeconds.Add('tests/test_event_consequences.py::test_delayed_transitions_forgotten_rejected_and_canonical_folds', 0.002535500330850482)
    $observedSeconds.Add('tests/test_event_consequences.py::test_equal_time_uncaused_transitions_and_explicit_reverse_links', 0.0014006998389959335)
    $observedSeconds.Add('tests/test_event_consequences.py::test_event_local_extrema_order_and_disjoint_coordinates', 0.002185999881476164)
    $observedSeconds.Add('tests/test_event_consequences.py::test_genealogy_author_occurrence_and_unlearned_fold_boundary', 0.001267799874767661)
    $observedSeconds.Add('tests/test_event_consequences.py::test_input_cache_isolation_candidate_provenance_and_ordered_citations', 0.001932400045916438)
    $observedSeconds.Add('tests/test_event_consequences.py::test_limits_identity_and_source_only_dependency_boundary', 0.0014120000414550304)
    $observedSeconds.Add('tests/test_event_consequences.py::test_scope_horizon_sections_and_excluded_poisoning', 0.014844600111246109)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_bootstrap_horizon_labels_and_cursor', 32.10227470006794)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_overlong_names_close_direct_and_http', 25.60493090003729)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_prefix_includes_supplementary_unicode_direct_and_http', 13.25059110019356)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_request_bounds_match_openapi', 14.333362000063062)
    $observedSeconds.Add('tests/test_generational_api.py::test_discovery_uses_compiled_rows_and_rebuilds_malformed_cache', 23.19618809991516)
    $observedSeconds.Add('tests/test_generational_api.py::test_former_roles_name_first_cli_http_and_openapi', 44.28734429995529)
    $observedSeconds.Add('tests/test_generational_api.py::test_name_first_parents_horizons_and_character_controls', 23.704878199845552)
    $observedSeconds.Add('tests/test_generational_api.py::test_read_operations_and_scope_bound_cursor', 59.94438060023822)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_hidden_and_future_organization_selector_parity', 36.58440369996242)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_legacy_index_rebuilds_from_unchanged_source', 17.30693899979815)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_read_horizons_stale_revision_and_closed_modes', 16.751996400067583)
    $observedSeconds.Add('tests/test_generational_api.py::test_reverse_union_nested_limit_direct_cli_http_schema_parity', 15.302792399888858)
    $observedSeconds.Add('tests/test_generational_authoring.py::test_starter_and_eight_kind_batch_share_one_confirmed_compiled_commit', 111.29557709977962)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ancestry_item_bound_counts_only_visible_active_edges', 0.04916310007683933)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ancestry_orders_each_depth_globally_by_authored_time', 0.04811429977416992)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_cited_bounded_traversal_and_private_search_candidates', 0.0479950001463294)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_distinct_authored_parentage_to_same_parent_keeps_both_citations', 0.046564599964767694)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_missing_or_malformed_generational_shape_rejects_cache', 0.06740369996987283)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_private_discovery_shape_and_generation_reject_old_cache', 0.06543110008351505)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_private_structural_search_shape_is_required', 0.01439659995958209)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_ratified_vector_compiles_all_kinds_and_replays_boundaries', 0.05086149997077882)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reparent_cycle_fails_closed_without_partial_containment_path', 0.045579199912026525)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_replacement_reparenting_and_timeline_are_exact', 0.06638820027001202)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reverse_legacy_organization_index_shape_is_required', 0.009951800107955933)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_reverse_parentage_index_is_required_and_has_parent_lead', 0.010810199659317732)
    $observedSeconds.Add('tests/test_generational_compiler.py::test_shared_full_and_fast_forward_paths_have_identical_rows', 0.07063769991509616)
    $observedSeconds.Add('tests/test_generational_context.py::test_closed_character_reads_make_no_context', 0.0006778002716600895) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_context.py::test_context_uses_only_authorized_results', 0.0006155997980386019) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_context.py::test_final_serialized_size_and_truncation', 0.0005631998647004366)
    $observedSeconds.Add('tests/test_generational_context.py::test_subquery_traversal_limit_closes_entire_packet', 0.0005371998995542526) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_context.py::test_writing_context_uses_literal_learned_genealogy_and_history', 0.21364310034550726)
    $observedSeconds.Add('tests/test_generational_fixture.py::test_bounded_builder_is_valid_reproducible_and_linked', 0.4262997996993363)
    $observedSeconds.Add('tests/test_generational_fixture.py::test_bounded_source_compiled_privacy_horizon_and_confirmed_transports', 200.67166900006123)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_adr_is_accepted_indexed_and_contract_only', 0.000534099992364645) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_literal_frontmatter_uses_one_transition_model', 0.03433480020612478)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_migration_binds_closed_target_capabilities_to_preview_apply', 0.007180199958384037)
    $observedSeconds.Add('tests/test_generational_history_adr.py::test_query_vectors_close_privacy_time_and_citation_applicability', 0.050680600106716156)
    $observedSeconds.Add('tests/test_generational_knowledge_compiler.py::test_knowledge_only_cache_rejects_old_missing_stale_and_damaged_projection', 0.2709065000526607)
    $observedSeconds.Add('tests/test_generational_knowledge_compiler.py::test_literal_assertions_replay_learning_applicability_and_neutral_evidence', 0.076541299931705)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_parent_learning_is_literal_cited_and_trusted_api_only', 0.17334550013765693)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_paths_are_complete_literal_uncertain_and_cycle_bounded', 0.14419639972038567)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_scope_search_and_unlearned_mutations_stay_closed', 0.12452400010079145)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_character_union_roles_holders_claims_and_vital_remain_literal', 0.056997899897396564)
    $observedSeconds.Add('tests/test_generational_knowledge_query.py::test_held_historical_names_context_and_discovery_preserve_exact_learning', 0.19482370000332594)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_exact_evidence_is_historical_admitted_and_same_character', 0.03612280008383095)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_retrospective_assertion_learning_and_new_correction_preserve_boundaries', 0.09061149996705353)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_typed_assertion_literals_are_closed_immutable_and_roundtrip', 0.03785849967971444)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_typed_knowledge_exports_do_not_grant_author_only_provenance', 0.1556161001790315)
    $observedSeconds.Add('tests/test_generational_knowledge_source.py::test_wrong_conflicting_and_cyclic_beliefs_opt_in_without_canon_matching', 0.04203889984637499)
    $observedSeconds.Add('tests/test_generational_knowledge_transport.py::test_confirmed_knowledge_authoring_and_cli_http_viewpoints', 317.5066484003328)
    $observedSeconds.Add('tests/test_generational_knowledge_transport.py::test_knowledge_intents_compile_closed_history_and_explicit_opt_in', 0.042414200492203236)
    $observedSeconds.Add('tests/test_generational_knowledge_transport.py::test_viewpoint_and_knowledge_openapi_catalogue_is_closed', 1.2762515002395958)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_absent_future_secret_are_identical_and_scope_is_closed', 0.04937239969149232)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_and_public_leak_matrix_for_claim_role_vital_and_search', 0.05065839970484376)
    $observedSeconds.Add('tests/test_generational_query.py::test_character_malformed_search_and_relative_target_are_invalid', 0.05340950004756451)
    $observedSeconds.Add('tests/test_generational_query.py::test_containment_one_parent_fits_one_item', 0.0579071999527514)
    $observedSeconds.Add('tests/test_generational_query.py::test_cursor_is_bound_to_scope_and_revision', 0.048301100032404065)
    $observedSeconds.Add('tests/test_generational_query.py::test_discovery_closes_overlong_titles_and_aliases', 0.05223419959656894)
    $observedSeconds.Add('tests/test_generational_query.py::test_discovery_name_is_admitted_at_exact_same_tick_order', 0.05016649980098009)
    $observedSeconds.Add('tests/test_generational_query.py::test_former_roles_exact_horizon_visibility_role_and_combined_cap', 0.07003819989040494)
    $observedSeconds.Add('tests/test_generational_query.py::test_former_roles_request_is_strictly_opt_in', 0.05401530023664236)
    $observedSeconds.Add('tests/test_generational_query.py::test_parent_item_limit_counts_only_active_authorized_edges', 0.05419520032592118)
    $observedSeconds.Add('tests/test_generational_query.py::test_parentage_descendants_and_boundary_order', 0.05665759998373687)
    $observedSeconds.Add('tests/test_generational_query.py::test_pinned_git_entry_closes_failed_and_malformed_target_replies', 1.0493930999655277)
    $observedSeconds.Add('tests/test_generational_query.py::test_pinned_git_entry_pairs_lookup_and_checks_integrity_each_read', 8.480426900088787)
    $observedSeconds.Add('tests/test_generational_query.py::test_relative_path_merges_equal_depth_edges_by_global_applicability', 0.05501299980096519)
    $observedSeconds.Add('tests/test_generational_query.py::test_relatives_are_one_continuous_cited_path_in_both_directions', 0.05161900003440678)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_missing_stale_damaged_cache_stays_fail_closed', 0.2398335998877883)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_rejects_alias_and_missing_revision', 0.06819339981302619)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_resolves_once_and_preserves_ready_answers', 0.11165009997785091)
    $observedSeconds.Add('tests/test_generational_query.py::test_repository_entry_target_changes_remain_fresh', 0.43820119998417795)
    $observedSeconds.Add('tests/test_generational_query.py::test_reverse_union_membership_and_organization_legacies_fold_before_budget', 0.0542051000520587)
    $observedSeconds.Add('tests/test_generational_query.py::test_source_compiled_and_rebuilt_query_parity', 0.32112699979916215)
    $observedSeconds.Add('tests/test_generational_query.py::test_union_history_and_active_roster_obey_items_limit', 0.06082929973490536)
    $observedSeconds.Add('tests/test_generational_query.py::test_union_organization_legacy_vital_and_private_search', 0.05118050007149577)
    $observedSeconds.Add('tests/test_generational_query.py::test_vital_direct_and_search_agree_before_birth_and_when_withheld', 0.12593499990180135)
    $observedSeconds.Add('tests/test_generational_source.py::test_generational_generated_paths_are_canonical', 0.0005491999909281731) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_accessor_is_immutable_and_rejects_extra_leaf_fields', 0.0005053002387285233) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[organization-initialize-payload0]', 0.0005107000470161438)
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[organization-initialize-payload1]', 0.0005030001047998667)
    $observedSeconds.Add('tests/test_generational_source.py::test_literal_payload_predicate_is_total_for_malformed_yaml_leaves[parentage-initialize-payload2]', 0.0005886000581085682) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[aliases-bad_value6]', 0.0005234999116510153) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[audience-bad_value5]', 0.0005188998766243458) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[domain-history]', 0.0004715996328741312)
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[organization_kind-bad_value3]', 0.0004646000452339649) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[status-bad_value2]', 0.0005403000395745039) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error[threads-bad_value4]', 0.0004475000314414501) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_source.py::test_tenure_transfer_requires_a_cause_event', 0.0005002999678254128)
    $observedSeconds.Add('tests/test_generational_validation.py::test_all_eight_runtime_kinds_accept_and_cross_record_integrity_rejects', 0.004425699822604656)
    $observedSeconds.Add('tests/test_generational_validation.py::test_closed_state_tables_cover_every_generational_kind_and_vital_sequence', 0.00030039972625672817) # Historical estimate.
    $observedSeconds.Add('tests/test_generational_validation.py::test_generational_serialization_keeps_transition_and_payload_order', 0.0005602999590337276)
    $observedSeconds.Add('tests/test_generational_validation.py::test_generational_validation_accepts_literal_union_and_rejects_bad_payload', 0.0007453998550772667)
    $observedSeconds.Add('tests/test_generational_validation.py::test_transition_replacement_cause_and_malformed_leaves_are_total', 0.0011481998953968287)
    $observedSeconds.Add('tests/test_generational_validation.py::test_validation_is_total_and_reports_exact_transition_leaves_and_paths', 0.0006111001130193472) # Historical estimate.
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_admission_and_commit_share_one_deadline_cell', 13.133292600046843)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_consumer_persistent_lock_keeps_head_source_and_index', 9.33034859993495)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_migration_consumer_waits_for_a_transient_lock', 14.022754400037229)
    $observedSeconds.Add('tests/test_migration_recovery.py::test_released_migration_lock_rechecks_head_before_creating_absent_backup', 5.364783400204033)
    $observedSeconds.Add('tests/test_multi_strand_continuity_adr.py::test_multi_strand_adr_is_historical_and_explicitly_superseded', 0.0006198999471962452)
    $observedSeconds.Add('tests/test_multi_strand_schema_contract.py::test_withdrawn_continuity_schema_material_is_nonnormative_only', 0.0054366004187613726) # Historical estimate.
    $observedSeconds.Add('tests/test_no_v04_continuity_residue.py::test_runtime_has_no_withdrawn_v04_continuity_residue', 0.0191558999940753)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields0-object-object_affordances]', 16.111948600038886)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields1-object-object_affordances]', 24.86920369998552)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields2-object-capabilities]', 28.87429720000364)
    $observedSeconds.Add('tests/test_object_affordances.py::test_legacy_upgrade_rejects_invalid_affordance_placement[fields3-character-capabilities]', 22.487917800201103)
    $observedSeconds.Add('tests/test_object_affordances.py::test_upgrade_preview_apply_replay_detail_and_rollback', 78.1503563998267)
    $observedSeconds.Add('tests/test_object_affordances.py::test_v07_validation_rejects_mixed_non_object_and_bad_values', 42.47943509998731)
    $observedSeconds.Add('tests/test_performance_cache.py::test_bounded_git_tree_metadata_defers_before_world_load', 4.319350400008261)
    $observedSeconds.Add('tests/test_performance_cache.py::test_in_memory_authoring_cache_matches_direct_compiler_projection', 11.886541199870408)
    $observedSeconds.Add('tests/test_performance_cache.py::test_malformed_or_unknown_git_tree_metadata_has_no_capacity_proof', 1.9386968002654612)
    $observedSeconds.Add('tests/test_performance_cache.py::test_malformed_profile_in_valid_cache_defers_without_losing_source_commit', 17.287082399707288)
    $observedSeconds.Add('tests/test_performance_cache.py::test_non_source_git_tree_output_is_bounded_before_world_load', 5.002173899905756)
    $observedSeconds.Add('tests/test_performance_cache.py::test_sqlite_serialization_failure_defers_without_shared_cache_write', 4.609175699995831)
    $observedSeconds.Add('tests/test_performance_cache.py::test_unknown_authoring_capacity_defers_before_world_load', 6.535864300094545)
    $observedSeconds.Add('tests/test_semantics.py::test_expanded_knowledge_appears_at_correct_time', 12.705079400213435)
    $observedSeconds.Add('tests/test_semantics.py::test_story_point_history_is_not_rewritten', 11.545340599957854)
    $observedSeconds.Add('tests/test_semantics.py::test_temporal_object_holder', 6.697624200256541)
    $observedSeconds.Add('tests/test_shared_world_thread_schema_contract.py::test_v05_migration_and_quarantined_v04_recovery_are_narrow_and_noninferential', 0.004756700014695525)
    $observedSeconds.Add('tests/test_shared_world_thread_schema_contract.py::test_v05_thread_reservation_has_one_global_timeline_and_grouping_only_membership', 0.003565100021660328)
    $observedSeconds.Add('tests/test_shared_world_threads_adr.py::test_coordination_manifest_withdraws_only_thread_reservation_and_preserves_other_projects', 0.001832600450143218)
    $observedSeconds.Add('tests/test_shared_world_threads_adr.py::test_shared_world_thread_adr_governs_one_global_world_without_schema_invention', 0.0019548998679965734)
    $observedSeconds.Add('tests/test_source_validation.py::test_crlf_frontmatter_envelope_is_accepted', 0.0005272999405860901)
    $observedSeconds.Add('tests/test_source_validation.py::test_duplicate_yaml_keys_are_rejected', 0.0006138000171631575)
    $observedSeconds.Add('tests/test_source_validation.py::test_expanded_fixture_validates_and_has_conversations', 12.694562700111419)
    $observedSeconds.Add('tests/test_source_validation.py::test_local_migration_is_cli_only', 0.05386619991622865)
    $observedSeconds.Add('tests/test_source_validation.py::test_yaml_sequence_mapping_keys_are_parse_errors_not_type_errors', 0.0005563001614063978)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_adr_is_accepted_indexed_and_explicitly_non_runtime', 0.0006584001239389181) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_adr.py::test_explorer_adjunct_decision_records_bounded_selected_lens', 0.0008191999513655901) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_adr.py::test_query_vectors_close_states_budgets_time_and_audience', 0.12689469987526536)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_spatial_shape_preserves_coordinate_free_worlds_and_non_inference', 0.004560600034892559)
    $observedSeconds.Add('tests/test_spatial_adr.py::test_version_migration_and_cross_project_boundary_are_closed', 0.0043963000643998384)
    $observedSeconds.Add('tests/test_spatial_api.py::test_capability_order_and_operation_fields_are_closed_before_lookup', 0.0009208002593368292) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_codec_cli_http_share_one_exact_spatial_outcome', 83.49787029996514)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[9007199254740992]', 0.0006194997113198042)
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[True]', 0.000596500001847744) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[inf]', 0.0004952000454068184) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup[nan]', 0.0005859001539647579)
    $observedSeconds.Add('tests/test_spatial_api.py::test_hidden_overlay_and_no_overlay_control_have_identical_public_result', 9.306762400083244)
    $observedSeconds.Add('tests/test_spatial_api.py::test_legacy_bbox_compiled_capability_absence_is_a_typed_public_unavailable', 26.190216599963605)
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[0]', 0.000667099840939045) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[1.0]', 0.0008021998219192028) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[101]', 0.0007377995643764734) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_limit_rejects_nonintegers_and_out_of_range_values[True]', 0.0008262000046670437) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[adjacency]', 0.0005693999119102955) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[bbox]', 0.0005634999834001064) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[children]', 0.0005915998481214046) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[containment]', 0.0007494997698813677) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[nearby]', 0.0005782002117484808)
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[overlay-as-of]', 0.0005768998526036739) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[path]', 0.000583600252866745) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_request_shape_is_closed_before_database_access[reachability]', 0.0006001999136060476) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_rounded_two_edge_metric_is_typed_unavailable_across_public_transports', 34.19950240012258)
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[+1]', 0.0005886000581085682) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[-0]', 0.0005638001020997763) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[01]', 0.000624299980700016) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[0]', 0.0007114000618457794) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[9223372036854775808]', 0.0005970997735857964) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup[True]', 0.0006243998650461435) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[bbox-relation]', 9.37647240026854)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[overlay-as-of-queryScope]', 6.373421000316739)
    $observedSeconds.Add('tests/test_spatial_api.py::test_unhashable_read_enums_are_ordinary_wedl_http_errors[path-metric]', 10.033484000014141)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.location.update-payload4]', 0.005056100198999047)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.map.create-payload0]', 0.003902000142261386)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.map.update-payload3]', 0.0039431999903172255)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.overlay.create-payload2]', 0.004099500132724643)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.overlay.update-payload6]', 0.00393899972550571)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.route.create-payload1]', 0.004121599951758981)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_all_seven_intents_compile_to_exact_source_only_changeset[spatial.route.update-payload5]', 0.004278899868950248)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_intent_payloads_and_version_gate_are_closed', 0.007152900332584977)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_malformed_spatial_authoring_enums_and_numbers_are_wedl_http_errors', 42.08399479999207)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_map_create_preview_apply_and_exact_replay_use_real_journal', 76.18107259995304)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_overlay_create_and_route_update_preview_apply_and_replay_through_real_journal', 56.304053200175986)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_apply_refuses_unconfirmed_bypass_before_repository_access', 0.008695099968463182) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_location_update_keeps_detail_context_and_whereabouts_compatible', 46.41360530001111)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_preview_is_read_only_for_source_cache_receipt_and_head', 14.412804099731147)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_spatial_source_and_receipt_roll_back_on_journal_publication_failure', 52.01538540003821)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.location.update-payload1]', 0.012249700026586652) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.map.update-payload0]', 0.004169100197032094)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.overlay.update-payload3]', 0.003713000100106001)
    $observedSeconds.Add('tests/test_spatial_authoring.py::test_updates_reject_title_rewrites[spatial.route.update-payload2]', 0.003831199835985899)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_all_place_modes_and_map_features_match_authored_source', 49.55976780015044)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_catalog_places_viewport_layers_direct_http_and_selected_cursor', 53.48422359977849)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_compiled_read_errors_redact_host_paths', 7.525492699816823)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_compiled_same_tick_overlay_respects_both_order_boundaries', 0.008381900377571583)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_five_http_only_routes_openapi_and_examples', 0.37665210012346506)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_forged_cursor_ordinals_and_surrogates_close_as_invalid', 16.63944340008311)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_layer_pages_bind_lens_horizon_and_member_order', 0.009397899964824319)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_legacy_catalog_is_useful_and_non_map_reads_close', 9.130380799993873)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_non_ok_state_matrix_and_guard_parity', 49.46394279971719)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_places_modes_lens_and_closed_validation', 17.89986649993807)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_direct_http_direction_modes_closed_and_cursor', 25.68613959988579)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_nonself_two_way_directed_cards_and_mounted_http', 19.70781260030344)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_routes_two_way_self_loop_and_position_portal_compiled_cards', 0.007977000204846263)
    $observedSeconds.Add('tests/test_spatial_explorer_api.py::test_signed_large_bounds_and_unsafe_geometry_close', 2.0686546000652015)
    $observedSeconds.Add('tests/test_spatial_query.py::test_absent_endpoints_are_unavailable_but_valid_empty_controls_stay_ok', 0.007437299937009811)
    $observedSeconds.Add('tests/test_spatial_query.py::test_bbox_requires_geometry_capability_and_map_dimensionality', 0.007254800060763955)
    $observedSeconds.Add('tests/test_spatial_query.py::test_bbox_same_map_pagination_and_cursor_revision_binding', 0.0074836998246610165)
    $observedSeconds.Add('tests/test_spatial_query.py::test_canonical_query_vector_and_source_compiled_fixture_stay_in_parity', 0.018975900253280997)
    $observedSeconds.Add('tests/test_spatial_query.py::test_children_bbox_within_adjacency_and_portal_reachability_are_authored_only', 0.007157200016081333)
    $observedSeconds.Add('tests/test_spatial_query.py::test_context_preserves_validated_registry_order_for_combined_capabilities', 0.013801599852740765)
    $observedSeconds.Add('tests/test_spatial_query.py::test_directed_path_does_not_infer_reverse_and_unknown_metric_is_closed', 0.00809110002592206)
    $observedSeconds.Add('tests/test_spatial_query.py::test_explorer_bbox_requires_map_scoped_rtree_but_legacy_bbox_stays_available', 0.008619800209999084)
    $observedSeconds.Add('tests/test_spatial_query.py::test_failed_rtree_population_leaves_no_partial_explorer_index', 0.007731200195848942)
    $observedSeconds.Add('tests/test_spatial_query.py::test_failed_temporal_index_population_removes_both_explorer_rtrees', 0.008414799813181162)
    $observedSeconds.Add('tests/test_spatial_query.py::test_hierarchy_is_authored_parent_path_and_coordinate_free_locations_are_readable', 0.007021299796178937)
    $observedSeconds.Add('tests/test_spatial_query.py::test_nearby_is_same_map_candidate_read_not_authored_topology', 0.008861100301146507)
    $observedSeconds.Add('tests/test_spatial_query.py::test_nearby_target_radius_aliases_and_exact_radius_postfilter', 0.01207290031015873)
    $observedSeconds.Add('tests/test_spatial_query.py::test_overlay_story_time_never_crosses_timelines_or_converts_duration', 0.0077049999963492155)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_keeps_incompatible_units_in_separate_typed_graphs', 0.00752820004709065)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_preserves_exact_large_integers_overflow_and_near_equal_ordering', 0.015028499998152256)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_refuses_to_rank_mixed_units_and_preserves_requested_identity_unit', 0.007531699724495411)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_reports_downstream_closed_and_restricted_edges', 0.008404599968343973)
    $observedSeconds.Add('tests/test_spatial_query.py::test_path_uses_exact_mixed_float_comparison_and_authored_tie_keys', 0.014731300296261907)
    $observedSeconds.Add('tests/test_spatial_query.py::test_positive_keyset_cursor_paging_is_complete_and_request_bound', 0.008076300146058202)
    $observedSeconds.Add('tests/test_spatial_query.py::test_repeated_path_order_is_stable_across_cycles_and_insertion_order', 0.008221799973398447)
    $observedSeconds.Add('tests/test_spatial_query.py::test_signed_i64_story_time_boundaries_and_cross_map_discontinuity_are_closed', 0.010123600019142032)
    $observedSeconds.Add('tests/test_spatial_query.py::test_story_time_rtree_boxes_cover_exact_signed_tick_order_ranges', 0.0005517001263797283) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_query.py::test_success_summaries_expose_closed_partial_unknown_filter_and_horizon_semantics', 0.007347000064328313)
    $observedSeconds.Add('tests/test_spatial_query.py::test_typed_path_has_stable_ties_and_respects_mode_and_availability', 0.009487999835982919)
    $observedSeconds.Add('tests/test_spatial_release_contract.py::test_authored_deep_wide_cycle_and_unknown_costs', 62.30939599988051)
    $observedSeconds.Add('tests/test_spatial_release_contract.py::test_mixed_legacy_source_is_rejected_without_side_effects', 22.80376199982129)
    $observedSeconds.Add('tests/test_spatial_source.py::test_spatial_ids_are_narrow_and_do_not_change_legacy_id_acceptance', 0.0003955999854952097) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_spatial_path_codec_is_portable_for_every_colon_kind_and_legacy_locations', 0.0005481999833136797) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capabilities_are_closed_and_protocol_ordered', 0.00032600038684904575) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value0-None]', 0.0006349000614136457)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value1-expected1]', 0.00066359993070364) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value10-expected10]', 0.0005787999834865332)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value2-expected2]', 0.0005083996802568436)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value3-None]', 0.0004803999327123165) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value4-None]', 0.0005047000013291836) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value5-None]', 0.0006599000189453363)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value6-expected6]', 0.0005890997126698494) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value7-expected7]', 0.0005125999450683594) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value8-expected8]', 0.0005535997916013002)
    $observedSeconds.Add('tests/test_spatial_source.py::test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies[value9-expected9]', 0.0006075000856071711)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_accessor_and_validator_reject_the_same_spatial_leaf_shapes', 0.0063559000845998526)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_anchor_conversion_and_detailed_location_link_rules_are_shared_and_canonical', 0.005856000119820237)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_arbitrary_record_frontmatter_never_raises', 0.00499950023368001)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_fixture_covers_coordinate_free_maps_routes_and_timed_overlays', 0.0029058000072836876)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_paths_ordering_and_runtime_registration_are_canonical', 0.0028545998502522707)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_rejects_nonfinite_geometry_cycles_and_unapproved_boundary', 0.0025700002443045378)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_component_requires_world_ordered_capability_declaration_and_keeps_legacy_inert', 0.002552000107243657)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_diagnostic_catalog_is_closed', 0.002755299909040332)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_envelope_and_map_diagnostics_identify_the_exact_leaf', 0.004947599722072482)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_feature_gates_legacy_locations_and_total_malformed_yaml_diagnostics', 0.007389299804344773)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_frontmatter_round_trip_preserves_v07_body_but_keeps_legacy_canonical_bytes', 0.00048079993575811386) # Historical estimate.
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generational_only_envelope_accepts_coordinate_free_records_but_gates_spatial_features', 0.0031954001169651747)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generic_v07_validation_composes_core_and_record_time_checks', 0.0027679000049829483)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_generic_validation_uses_component_envelope_and_never_writes', 0.003306400030851364)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_invalid_fixture_bool_nan_and_duplicate_edges_are_leaf_diagnostics', 0.003450899850577116)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_multimap_geodetic_anchor_portal_and_legacy_location_fixture', 0.003106500022113323)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_typed_accessors_are_deeply_immutable_and_include_optional_values', 0.0030192998237907887)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_envelope_legacy_links_and_geometry_variants_are_literal_not_routes', 0.0028032001573592424)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_envelope_uses_full_chronology_validation_and_listed_default_timeline', 0.0026424999814480543)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_location_accepts_inherited_chronology_annotations', 0.002880900166928768)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_v07_parent_compatibility_input_serializes_to_adr_canonical_parent_id', 0.0031247998122125864)
    $observedSeconds.Add('tests/test_spatial_source_component.py::test_z_policy_is_consistent_for_bounds_origin_geometry_and_accessors', 0.004630600102245808)
    $observedSeconds.Add('tests/test_spatial_validation.py::test_v07_component_is_not_generic_source_acceptance', 0.0006689999718219042)
    $observedSeconds.Add('tests/test_story_points_web_ui.py::test_story_points_web_ui_uses_selected_scene_and_safe_static_contract', 8.13828400010243)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_generational_transport_matrix_and_closed_revision', 185.03182630008087)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.3]', 89.05842089978978)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.5]', 108.93439029995352)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback[wedl/v0.6]', 128.00193830020726)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_spatial_source_compiled_rebuild_and_closed_transport', 43.95548130013049)
    $observedSeconds.Add('tests/test_suite_replan_boundaries.py::test_small_thread_filter_preserves_rank_and_never_backfills', 0.0006407001055777073) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_invalid_v05_stops_before_database_revision_or_vector_publication', 6.999836299801245)
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_v04_compiled_cache_metadata_is_incompatible', 16.961126800160855)
    $observedSeconds.Add('tests/test_thread_compile_integration.py::test_valid_v05_compiles_publishes_threads_and_keeps_search_vector_inputs_stable', 4.736380500020459)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_current_fingerprint_v04_cache_entry_is_evicted_before_record_rehydration', 5.970745100174099)
    $observedSeconds.Add('tests/test_thread_model_source.py::test_exact_v04_is_quarantined_before_record_construction_with_recovery_link', 0.0005359000060707331) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_model_source.py::test_thread_ids_are_non_entity_identifiers_and_source_schema_cache_contract', 0.00037180003710091114) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_model_source.py::test_v05_source_round_trip_canonicalizes_thread_lists_without_synthesizing_memberships', 0.0005894999485462904) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_model_source.py::test_v05_world_and_record_expose_grouping_membership_without_entity_refs', 0.0005652999971061945) # Historical estimate.
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_catalog_preserves_v04_quarantine_boundary', 4.692456600023434)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_catalog_require_compiled_never_rebuilds_when_cache_is_missing', 7.9802868000697345)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_v03_catalog_has_no_synthetic_grouping_and_cli_http_query_parity', 9.238162100082263)
    $observedSeconds.Add('tests/test_thread_public_catalog.py::test_v05_catalog_is_sorted_and_discloses_only_public_declarations', 10.049716000212356)
    $observedSeconds.Add('tests/test_thread_search_filter.py::test_resolve_thread_filter_requires_normalized_declared_v05_ids', 0.0010753998067229986)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_capture_rejects_parent_reparse_substitution', 0.006047200178727508)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_capture_rejects_restored_mtime_content_change', 0.003225899999961257)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_exact_limit_and_growth', 0.001972600119188428)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_rejects_nonregular_path', 0.0012205999810248613)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_bounded_commit_before_revalidates_named_path', 0.0021103997714817524)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_commit_before_limit_preserves_ref_index_and_source', 9.385380700230598)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_complete_index_cas_preserves_an_unrelated_staged_entry', 18.230410599848256)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_external_touched_entry_after_wait_aborts_before_ref_or_source_publish', 8.784400600008667)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_ignores_overridden_str_methods', 0.0005467000883072615) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[all-controls]', 0.0005584999453276396) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[astral]', 0.00052429991774261) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[high-surrogate]', 0.0007170001044869423) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[latin-one]', 0.0006031002849340439) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[long-base64]', 0.016488999826833606)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[low-surrogate]', 0.0004849000833928585) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[plain]', 0.0005447000730782747) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[quotes-and-slashes]', 0.0006388998590409756) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_string_wire_bound_matches_previous_and_canonical_json[repeated-controls]', 0.00046140002086758614) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_json_wire_bound_rejects_malformed_values_and_overflow', 0.0006910997908562422) # Historical estimate.
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_persistent_or_substituted_external_lock_never_mutates_it', 0.954814599826932)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_transient_external_lock_releases_within_the_private_window', 4.997237200150266)
    $observedSeconds.Add('tests/test_transaction_recovery.py::test_two_external_lock_seams_share_the_first_lazy_deadline', 0.8203810998238623)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_budgeted_load_reuses_one_authenticated_document_and_exact_images', 0.007692600134760141)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_default_legacy_and_authoritative_cleanup_limits_preserve_loader_contract', 0.013302300125360489)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_envelope_length_digest_surface_and_early_budget_refuse_before_decode', 0.04468629998154938)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_graph_bound_descriptor_chunk_carry_and_malformed_tail', 0.5461921999230981)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_graph_bound_exact_budget_admission_and_checked_overflow', 0.0009908003266900778)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_graph_bound_exact_lexical_corpus_and_escape_parity', 0.05168969975784421)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_loaded_identity_and_same_byte_journal_cas_substitution_still_refuse', 0.012776499846950173)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_parsed_header_binding_and_persisted_budget_remain_authoritative', 0.01895140018314123)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_resigned_hostile_canonical_documents_refuse_before_full_decode', 0.039310299791395664)
    $observedSeconds.Add('tests/test_transaction_recovery_admission.py::test_same_size_descriptor_mutation_re_admits_joined_graph_before_key_sets', 0.013680400094017386)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_check_reports_drift_without_modifying_packages', 0.07421120000071824)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_explicit_bootstrap_choices_keep_legacy_default', 0.041653200052678585)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_hardlinked_bytecode_refused_before_publication', 0.05478609981946647)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_hardlinked_destination_cannot_overwrite_pinned_source', 0.021206399891525507)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_imported_bytecode_coexists_across_versions_without_rewriting', 0.09220860013738275)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_linked_bytecode_cache_refused_before_publication', 0.8426636001095176)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_missing_legacy_record_fails_before_publication', 0.5670649998355657)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_mixed_schema_fails_before_publication', 0.016402200097218156)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_semantic_projection_preserves_absent_empty_order_and_duplicates', 0.0005617998540401459)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_small_packaged_upgrade_stale_head_and_forward_rollback', 86.91626489977352)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_symlinked_destination_cannot_overwrite_pinned_source', 0.7539037999231368)
    $observedSeconds.Add('tests/test_v07_packaged_examples.py::test_unexpected_bytecode_entries_refused_without_publication', 2.9678158997558057)
    # The six nodes without measured or historical estimates retain this nominal fallback.
    $unknownSeconds = 0.5
    # Only these independent-copy consumers share the module authoring seed.
    $spatialAuthoringConsumers = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($consumer in @(
        'tests/test_spatial_authoring.py::test_malformed_spatial_authoring_enums_and_numbers_are_wedl_http_errors',
        'tests/test_spatial_authoring.py::test_map_create_preview_apply_and_exact_replay_use_real_journal',
        'tests/test_spatial_authoring.py::test_overlay_create_and_route_update_preview_apply_and_replay_through_real_journal',
        'tests/test_spatial_authoring.py::test_spatial_location_update_keeps_detail_context_and_whereabouts_compatible',
        'tests/test_spatial_authoring.py::test_spatial_preview_is_read_only_for_source_cache_receipt_and_head',
        'tests/test_spatial_authoring.py::test_spatial_source_and_receipt_roll_back_on_journal_publication_failure'
    )) { $null = $spatialAuthoringConsumers.Add($consumer) }
    # Share this module fixture while retaining both intentional cache repairs.
    $generationalConsumers = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($consumer in @(
        'tests/test_generational_api.py::test_discovery_bootstrap_horizon_labels_and_cursor',
        'tests/test_generational_api.py::test_discovery_request_bounds_match_openapi',
        'tests/test_generational_api.py::test_discovery_uses_compiled_rows_and_rebuilds_malformed_cache',
        'tests/test_generational_api.py::test_name_first_parents_horizons_and_character_controls',
        'tests/test_generational_api.py::test_read_operations_and_scope_bound_cursor',
        'tests/test_generational_api.py::test_reverse_legacy_index_rebuilds_from_unchanged_source',
        'tests/test_generational_api.py::test_reverse_read_horizons_stale_revision_and_closed_modes',
        'tests/test_generational_api.py::test_reverse_union_nested_limit_direct_cli_http_schema_parity'
    )) { $null = $generationalConsumers.Add($consumer) }
    $generationalGroupKey = 'tests/test_generational_api.py::generational_repo-consumers'
    $generationalSeen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $groups = [System.Collections.Generic.Dictionary[string,object]]::new([System.StringComparer]::Ordinal)
    foreach ($node in $nodes) {
        $key = $node
        foreach ($module in @('tests/test_spatial_api.py', 'tests/test_spatial_explorer_api.py')) {
            if ($node.StartsWith($module + '::', [System.StringComparison]::Ordinal)) {
                $key = $module
                break
            }
        }
        if ($spatialAuthoringConsumers.Contains($node)) {
            $key = 'tests/test_spatial_authoring.py::ash_repo-consumers'
        }
        if ($generationalConsumers.Contains($node)) {
            if (-not $generationalSeen.Add($node)) {
                throw "Duplicate generational fixture consumer: $node"
            }
            $key = $generationalGroupKey
        }
        if (-not $groups.ContainsKey($key)) {
            $groups.Add($key, [pscustomobject]@{
                Name = $key; Nodes = [System.Collections.Generic.List[string]]::new(); Weight = 0.0
            })
        }
        $groups[$key].Nodes.Add($node)
    }
    if ($generationalSeen.Count -ne $generationalConsumers.Count) {
        throw "The normal suite is missing a generational fixture consumer."
    }
    if (-not $groups.ContainsKey($generationalGroupKey) -or
        $groups[$generationalGroupKey].Nodes.Count -ne 8 -or
        -not $generationalConsumers.SetEquals($groups[$generationalGroupKey].Nodes)) {
        throw "The generational fixture group does not match its exact eight consumers."
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
