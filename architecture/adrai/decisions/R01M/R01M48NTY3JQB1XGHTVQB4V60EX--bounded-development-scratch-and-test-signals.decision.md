+++
schema = "adrai/decision/v1"
adr = "A01M3Y3S5QMJWMBMQT42HDQPDJS"
record = "R01M48NTY3JQB1XGHTVQB4V60EX"
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
This policy prevents new accumulation by making routine reporting compact and scratch ownership/expiry explicit. Existing tools/test.ps1 behavior still retains failure artifacts; docs/TESTING.md describes that behavior. Runner changes and disposition of existing output are separate work. docs/WORKSPACE_CLEANUP.md historical receipts remain valid and protected. ADRs are read and written through the ADRAI CLI; migrated records retain their historical ADR 0001–0006 aliases, approvals, and supersession history. This managed ADRAI record remains identified by its returned ADR ID.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleCIsImsiOiJsbG0iLCJtIjoiZ3B0LTYuMS1zb2wifSwiYiI6IjhiZTFlMjhmNDIyNjFkZjJkMjhiNDNkNGI4NTZlNGNhYTI2YWU4OTkiLCJpIjoic2hhMjU2OjJsOHVTOV80alJtbF82YzZZSTJKeWpsOFdiVzM3TjFXOGVncUZLN0VPSGciLCJrIjoiZGVjaXNpb24uYW1lbmQiLCJvIjoiUjAxTTQ4TlRZM0pRQjFYR0hUVlFCNFY2MEVYIiwib3AiOiJPMDFNNDhOVFkzSlFCMVhHSFRWUUI0VjYwRVgiLCJwIjpbIlIwMU0zWTNTNVhaMzNZN1NEMkszRTIwRVE2USJdLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpSOUp1T0g0RUNSWWZXZjNZZFRIQkIzQ1R2WVRfVjV2OTBTQk5MMlRMY0JNIiwidCI6MTc5MTI5MjcwMDc4NiwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
