+++
schema = "adrai/decision/v1"
adr = "A01M3Y3S5QMJWMBMQT42HDQPDJS"
record = "R01M4973ZSWVCHYX5CVARF8HRDT"
title = "Bounded development scratch and test signals"
summary = "Routine tests provide compact pass/fail evidence; owned scratch is bounded and cleaned without weakening required verification."
domains = ["development-workflow"]
+++

## Status and scope
Accepted by Sornaensis on 2026-10-02. This prospective repository-wide policy applies to developer, reviewer, analyzer, orchestrator, and alternate worker workflows. It governs how future work creates and retains scratch and routine test output.

## Decision
1. Tests are a pass/fail signal for developers and reviewers. Run the relevant supported checks and preserve real assertions and explicit local or parent release gates. Report the exact command, outcome (including exit code/counts where available), relevant input revision or hashes, and material limitations in a compact handoff. Routine successful runs do not require raw stdout/stderr, XML reports, copied fixtures, or a file-backed run journal.
2. Use one documented contained scratch area per active task under the repository's documented ignored scratch location or an OS temporary location. Identify its task owner and finite lifetime before creation; reuse that area across iterations, rather than accumulating numbered retry directories. Additional isolation needs a concrete reason and the same ownership and expiry. Reuse compatible assets and checks when relevant input provenance still matches. Keep active scratch size bounded to the task's needs. Clean only scratch created and owned by the current task when no longer needed, after verifying exact contained paths, excluding links/worktrees and coordinating any live owner. Run cleanup with finite limits and report a failure instead of repeatedly creating replacement directories.
3. On failure, retain only the targeted diagnostics needed to explain or reproduce the unresolved defect. Bound their size and number; state their owner and disposal condition. Dispose of owned diagnostics at issue resolution or task closure unless an explicitly named requirement sets a finite later expiry. Preserve the actual failure and relevant phase timings in the compact handoff without routinely archiving every process stream or fixture. A compact message can be the evidence journal; use a file only when persistence or a task-specific gate needs it.
4. Preserve canonical source, authored fixtures, user work, other agents' active inputs, registered worktrees, and explicitly required existing evidence. This decision neither authorizes deleting existing files nor retroactively changes historical acceptance. An existing task's explicit proof/retention requirement needs an authorized scoped reconciliation before it can be relaxed. Future required release, benchmark, recovery, or other retained evidence is an explicit bounded exception: state its scope, reason, owner, and retention disposition; do not use it to retain unrelated routine output.

## Consequences and current limits
This policy prevents new accumulation by making routine reporting compact and scratch ownership/expiry explicit. Existing tools/test.ps1 behavior still retains failure artifacts; docs/guides/testing.md describes that behavior. Runner changes and disposition of existing output are separate work. docs/reports/workspace-cleanup-2026-10-02.md historical receipts remain valid and protected. ADRs are read and written through the ADRAI CLI; migrated records retain their historical ADR 0001–0006 aliases, approvals, and supersession history. This managed ADRAI record remains identified by its returned ADR ID.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNmMyNjc3NzJiNmRhNmM4NTRmM2Y2NzZiZGMxMTA1OTQyYjU2N2YzYiIsImkiOiJzaGEyNTY6ODBrT3hyQTcwSmRnQURYVmpvNS1abE1oNG1xYzIxX3hrTm1UZFl0Wk1hMCIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNDk3M1pTV1ZDSFlYNUNWQVJGOEhSRFQiLCJvcCI6Ik8wMU00OTczWlNXVkNIWVg1Q1ZBUkY4SFJEVCIsInAiOlsiUjAxTTQ4TlRZM0pRQjFYR0hUVlFCNFY2MEVYIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OjhqREIwS0lCQ19MdFRjRjg4MV8tMmFDQ2xSWkJQZEZhZUxLcUVfNHRqZlEiLCJ0IjoxNzkxMzEwODIzMjI4LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
