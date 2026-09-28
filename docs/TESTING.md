# Test suites

Run the functional suite on Windows with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test.ps1
```

The script uses the local `.venv` Python when available. Its default execution deadline is 300 seconds; `-TimeoutSeconds` accepts 1–300 for diagnosis. An expired deadline exits 124. Process termination and diagnostic collection can extend the command's total wall time beyond the configured deadline. The runner creates isolated temporary directories, terminates active pytest process trees on failure, and retains failure artifacts at the paths it prints. It removes successful temporary artifacts. For an exited leader, cleanup requires its recorded PID, start time, and actual exit time. It checks one Windows process snapshot, traverses at most 32 levels and 128 descendants, and stops children only when their creation times match live process start times within 50 milliseconds. A child reported after the leader's exit or a reused leader PID makes cleanup stop without killing ambiguous processes. If Windows denies process enumeration or tree termination, cleanup reports that limitation. A direct PID kill can stop the known leader without proving that its descendants stopped.

`pytest.ini` contains an exact, versioned node-ID inventory. Each `U`, `I`, or `P` entry records unit/component, integration, or performance/stress intent. Intent does not select execution. `wedl_active_performance_v1` contains the exact IDs selected for the separate performance suite; its complement is the normal suite. An empty active list runs every registered node in normal. Adding, removing, renaming, or moving a test requires updating the registry, counts, and snapshot rules in `tests/conftest.py`.

The inventory accepts two collection snapshots: the committed 494-ID test set and the 502-ID set with both hash-bound overlay files present. Partial overlays, different file hashes, changed IDs, duplicates, unknown IDs, missing IDs in full collection, and conflicting declared markers fail collection. The runner collects all, normal, and performance IDs and checks exact disjoint union. It then collects normal shards A and B, verifies their exact node-ID multiset against serial normal collection, and checks every executed node has one passing call outcome. Skips, expected failures, unexpected passes, unknown outcomes, and silent deselection fail. A normal test may still be run directly with ordinary `pytest` for local diagnosis; `--wedl-strict` enables gate checks.

The post-functional aggregate command is:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test-performance.ps1
```

Its default hard execution deadline is 1,800 seconds, with a 1,500-second operating target. `-ValidateOnly` checks enrollment without running benchmarks. A complete aggregate requires all eight existing benchmark entry points, the full authored 100,000-place spatial source stage, actual browser navigation against its retained fixture, the 5,000-character/10,000-edge generational stage, and both object-affordance and packaged-example corpus stages. Missing stage commands fail before execution. The compiled spatial index/query projections are separate controls and cannot stand in for authored source, API, or actual browser evidence. The complete command must report exact test outcomes, stage results and hashes; a result above the operating target needs headroom analysis before release use. Running an individual stage for diagnosis does not satisfy the aggregate gate.
