+++
schema = "adrai/decision/v1"
adr = "A01M49735W4D3CZ2PJ129HTJCGG"
record = "R01M49736S0YYJVNTMPRDZNHJRG"
title = "Historical report custody and fresh benchmark output"
summary = "Preserve historical evidence and its authority boundaries during report relocation; no new release qualification."
domains = ["development-workflow", "performance"]
+++

# Historical report custody and fresh benchmark output

## Decision

Historical report measurements are immutable provenance, not current release or
performance certification. Transfer the six report JSON files byte-for-byte and
keep every unique measured outcome, query/rank, timing, version/count, failure and
limitation in explicitly historical reports. Their owner is the repository
maintainers; retention continues until separately authorized replacement or
retirement with a reconciliation preserving original outcomes. Age, an ignore
rule, a new passing diagnostic or a rerun is not deletion authority.

Architectural/policy/algorithm rationale, including old proposals, is retained
through ADRAI A01M4971W13BPKM0G1FPZ86MA9Z with nonnormative stage framing. Practical reports are
not a second policy registry. The exact-path reconciliation under docs/reports
records concrete scope, reason, owner and retention before old paths are retired.
Existing domain-owned chronology/spatial/generational evidence remains unchanged,
including the unmet historical generational warm-query target (2626.816 ms versus
250 ms). No current qualification is inferred from these archived results.

## Protected custody and limits

The entire existing `output/repository-cleanup-20261002/` is protected by
A01M3Y3S5QMJWMBMQT42HDQPDJS and this task grants no cleanup, restoration or
reconstruction there. Historical cleanup statements remain recorded history.
At source revision 4d81ccc3c3cfc52381396c24458866cf1e6fb2ad, the directory
exists with p4b5f, recovery-candidate-34dc9556-3279-4d13-8086-976161574fa5 and
test-run-4b5f0b38-26e1-46e4-b417-c95aec3a4537 entries; named root-receipts.zip
and legacy-retirement/diagnostics.zip are absent in this checkout. This is an
inherited custody limitation, not evidence of this task deleting or revalidating
archives. Preserve unrelated user work, active inputs and registered worktrees.

## Fresh chronology output interface

Default full/smoke benchmark invocation keeps stdout behavior. `--write` requires
an explicit `--output PATH`; `--write --smoke` remains invalid and `--output`
without `--write` is invalid. Missing, existing or uncreatable destinations fail
before run_benchmark. Reserve the fresh path with exclusive file creation before
benchmark work, so even an explicitly supplied archived receipt cannot be
overwritten. Do not create its parent implicitly. The caller identifies a
contained owned destination, reason and finite lifetime under the applicable
custody policy; no default points into archived docs/reports.

On benchmark failure the newly reserved output may be empty; the caller keeps
only bounded unresolved diagnostic evidence and resolves its disposition within
that ownership. The historical docs/reports/chronology-index-benchmark.json stays
byte-identical with its original environment, input and implementation fingerprint.
Changing the generator creates a different future implementation fingerprint;
fresh output is new evidence, never a rewrite or reenactment of that old receipt.
No benchmark algorithm, scale, budget, runner or runtime semantics changes here.

## Original limits preserved

VALIDATION 0.5.0/0.5.1 retain their 35/37-test historical results. Frontiersmen
294 records/17 conversations/224 turns/main195 belong to the measured pursuit
snapshot, while maintained packages have 309/19/248/main210. Inherited 308/307
rebuild claims remain historical provenance; this documentation transfer does
not assert a fresh rebuild. Existing generators copy current packages, so fresh
runs do not reproduce an absent old input snapshot. Do not invent missing dates,
source revisions, hardware guarantees or performance success.

The [concrete retention reconciliation](../../../../docs/reports/retention-and-dispositions.md)
and [historical measurements](../../../../docs/reports/) contain evidence, while
the scratch policy and configured verification contract retain policy/gate authority.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiYzhkZDZkODhkODE0OTRjMGQwMWVkNDA5ZDY4M2VmMTU4NmQ2YmY0NyIsImkiOiJzaGEyNTY6NDE1bkNRZ1h5cmsyYUcwOF8tREhZS0ozcDI2WDgxSDUwc0JBMUF0cjF5TSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5NzM2UzBZWUpWTlRNUFJEWk5ISlJHIiwib3AiOiJPMDFNNDk3MzZTMFlZSlZOVE1QUkRaTkhKUkciLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpabTJiSE9KMlNZUFNOOXdxZTJJcVpmMXpjYzhBQVZCUkdDUmV0UU9vZUF3IiwidCI6MTc5MTMxMDc5NzYwMCwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
