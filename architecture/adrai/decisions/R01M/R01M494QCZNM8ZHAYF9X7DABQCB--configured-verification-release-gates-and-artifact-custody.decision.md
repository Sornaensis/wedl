+++
schema = "adrai/decision/v1"
adr = "A01M494QCT41GF34E92FKKHQ8AS"
record = "R01M494QCZNM8ZHAYF9X7DABQCB"
title = "Configured verification, release gates and artifact custody"
summary = "Preserve the implemented operational contract and correct documented interface limits; practical usage remains in guides."
domains = ["release", "testing"]
+++

# Configured verification, release gates and artifact custody

Run the functional suite on Windows with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test.ps1
```

The script uses the local `.venv` Python when available. Its default execution deadline is 800 seconds; `-TimeoutSeconds` accepts 1–800 for diagnosis. An expired deadline exits 124. Process termination and diagnostic collection can extend the command's total wall time beyond the configured deadline. The runner creates isolated temporary directories, terminates active pytest process trees on failure, and retains failure artifacts at the paths it prints. It removes successful temporary artifacts. For an exited leader, cleanup requires its recorded PID, start time, and actual exit time. It checks one Windows process snapshot, traverses at most 32 levels and 128 descendants, and stops children only when their creation times match live process start times within 50 milliseconds. A child reported after the leader's exit or a reused leader PID makes cleanup stop without killing ambiguous processes. If Windows denies process enumeration or tree termination, cleanup reports that limitation. A direct PID kill can stop the known leader without proving that its descendants stopped.

`pytest.ini` contains an exact, versioned node-ID inventory. Each `U`, `I`, or `P` entry records unit/component, integration, or performance/stress intent. Intent does not select execution. `wedl_active_performance_v2` contains the exact IDs selected for the separate performance suite; its complement is the normal suite. The version 2 registry verifies that this active list matches the declared performance nodes. Adding, removing, renaming, or moving a test requires updating the registry, counts, and snapshot rules in `tests/conftest.py`.

The version 2 inventory records the baseline digest, reviewed additions, intent totals, active performance count, and any hash-bound overlays. Collection verifies the current snapshot against those records; partial overlays, different file hashes, changed IDs, duplicates, unknown IDs, missing IDs in full collection, and conflicting declared markers fail collection. The runner collects all, normal, and performance IDs and checks their exact disjoint union. It then collects five normal shards A through E, verifies their exact node-ID multiset against serial normal collection, and checks every executed node has one passing call outcome. Successful runs report live collection and execution counts rather than assuming historical totals. Skips, expected failures, unexpected passes, unknown outcomes, and silent deselection fail. A normal test may still be run directly with ordinary `pytest` for local diagnosis; `--wedl-strict` enables gate checks.

The post-functional aggregate command is:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test-performance.ps1
```

Its default hard execution deadline is 1,800 seconds, with a 1,500-second operating target. `-ValidateOnly` checks enrollment without running benchmarks. A complete aggregate requires all eight existing benchmark entry points, the full authored 100,000-place spatial source stage, actual browser navigation against its retained fixture, the 5,000-character/10,000-edge generational stage, and both object-affordance and packaged-example corpus stages. Missing stage commands fail before execution. The compiled spatial index/query projections are separate controls and cannot stand in for authored source, API, or actual browser evidence. The complete command must report exact test outcomes, stage results and hashes; a result above the operating target needs headroom analysis before release use. Running an individual stage for diagnosis does not satisfy the aggregate gate.

## Current enrollment, deadlines and retention limits

The configured registry is version 2; report actual collection counts rather
than freezing a historical U/I/P total. Shards are deterministic weighted
groups A–E, with spatial API modules kept together. Collection, execution and
aggregation run under the normal execution deadline; teardown can extend wall
time. A direct focused pytest invocation is local diagnosis, not proof that
the full runner met its deadline or exact shard/matrix gate.

Performance `-TimeoutSeconds` accepts 1–1800. The 1500-second operating target
and 1800-second hard deadline are distinct. The eight baseline stages and four
mandatory adjunct stages remain required. `-ValidateOnly` still checks the
pinned interpreter, Node/browser runtime, Playwright, input hashes and
browser/package preflight; it skips performance execution, not enrollment or
preflight. A stage run or synthetic compiled projection does not certify the
authored/API/browser or complete aggregate boundary.

Current normal-run failure roots and temporary artifacts are retained
indefinitely by the runner. The performance runner has no removal path and
retains scratch/context/evidence on success and on `-ValidateOnly` as well.
These are implementation limitations, not a new permission to erase required
release/failure evidence or to ignore finite custody for ordinary task-owned
diagnostics. Required existing release/benchmark/recovery evidence remains
protected under its original scope and owner until separately authorized
retirement. New routine task scratch has a named owner and finite lifetime;
successful temporary task diagnostics can be removed only within that verified
ownership. Do not delete preexisting runner outputs or registered worktrees.

Historical failed deadline or benchmark receipts retain their original result.
Correcting or relocating this documentation does not rerun or qualify a release,
and does not convert an individual passing diagnostic into aggregate acceptance.


## Provenance and authority

Transferred from `docs/TESTING.md` at source revision `1f724998bb35bb523fee70da9b300860c4c7f16d` during documentation maintenance. This record preserves the implemented contract and its unique rationale; it does not invent a new historical approval. Existing domain decisions retain their authority. Current interface limitations are distinguished from approved canonical source semantics. Practical invocation is in the corresponding [usage guides](../../../../docs/guides/).

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiMDlmZDBmNmM4MzcwOWE3YjQwZWY3YjcyZWJhZGExZGRlY2EyMzU5MCIsImkiOiJzaGEyNTY6QXRFUzJ4bXBhM0RtS1pOOGRDUGhqa3VNTlhyRnFYOWZJS2ZHdHgzVHNXOCIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5NFFDWk5NOFpIQVlGOVg3REFCUUNCIiwib3AiOiJPMDFNNDk0UUNaTk04WkhBWUY5WDdEQUJRQ0IiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpGQ1RUajhVQmsxREMxOGVfUHE3b3JrdUdrVWhKb2Uzem1nZWtkOFNBMWJrIiwidCI6MTc5MTMwODMxMzU4OSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
