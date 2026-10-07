+++
schema = "adrai/decision/v1"
adr = "A01M4B11D5PBJTPFGFJYS3EJFX0"
record = "R01M4B11DBVCTJRNSJM8QJPVMEV"
title = "Recoverable custody before retiring embedded world copies"
summary = "Preserve complete protected local archives and independently verified credential-free tracked recovery cores before either embedded world is retired."
domains = ["recovery", "source"]
+++

# Recoverable custody before retiring embedded world copies

## Authority and scope

Sornaensis authorized completion of the example consolidation and retirement
project on 2026-10-07, including concrete protected custody inside the WEDL
repository. This decision governs the root `frontiersmen` and `harry-potts`
Git links and contained local world trees, and their preservation under
`archives/world-retirement/2026-10-07`. It does not authorize deletion of other
worlds, historical evidence, active worktrees or unrelated user/agent work.
It supplements the bounded-development-scratch policy without weakening it.

## Protected custody

Repository maintainers own both preservation tiers with no expiry. They are
required recovery evidence, not scratch, and must not be removed by task or
runner cleanup. Each world has these distinct artifacts:

- `{world}.full.zip`: a complete byte-exact whole-world archive, including
  hidden `.git` metadata and complete history, authored source, ignored and
  untracked files, empty directories, and all `.wedl` configuration, caches
  and durable receipts. It can contain local credentials. Keep it protected
  locally and ignore only `frontiersmen.full.zip` and `harry-potts.full.zip`
  through the archive directory's narrow `.gitignore`. Never print its tokens
  or stage the full archive.
- `{world}.core.zip`: a tracked credential-free archive containing exact
  authored source, complete safe Git metadata/history, and durable receipts.
  It excludes only explicitly inventoried credential-bearing or disposable
  cache/session-auth configuration. Exact excluded originals remain in the
  protected full tier; redaction does not replace original required data.
- A tracked per-world manifest records artifact SHA-256 values, every member
  path/type/size/hash, full/core membership and exact omissions with reasons,
  source inventory, Git tip/history, required receipt fingerprints, validation
  outcomes, owner and retention. Do not include credential values.

A clone can recover the verified core tier, not the ignored full tier. No
remote or external full backup has been verified, and no external-backup or
clone-complete-custody claim is permitted.

## Admission before retirement

Freeze and re-audit each exact contained tree before capture: tip, refs and
history, source, dirty/untracked/ignored content, active owners, worktrees and
links. Preserve any drift or unrelated work. Do not steal locks or remove an
active worktree. The initial planning inputs are Frontiersmen tip
`845b1d0c47783278c9f658a7df4ddba327d6c399` with 309 source records and one
local-only commit, and Harry tip `bb3b92c0d201e9783b02031a7c4588adcd5ab6e6`
with 110 unique source records and seven local-only commits. Those observations
are provenance, not permission to ignore later changes.

Before tracking the core tier, inspect source, Git history/object payloads,
Git configuration and durable receipts for actual credentials. Compare known
local auth-token values in memory and classify credential-shaped fields;
never print values. A no-match scan alone is not a general credential guarantee.
Keep credential-bearing originals exact in the full tier and record the
specific exclusion. If required authored source, complete history or durable
receipt data cannot be safely preserved in the tracked core, stop retirement
for scoped reconciliation rather than weakening custody or silently redacting.

Each tier is bounded to 64 MiB for Frontiersmen and 32 MiB for Harry. Capture
all full-tier bytes and empty directories; do not silently truncate to fit.
Validate member CRCs, SHA-256 inventories, unique contained paths and safe
extraction with no link/absolute/parent traversal. Recover both tiers into
one task-owned bounded scratch area and compare members with their manifests.
Verify the preserved Git tip/history and source and durable receipt inventory
without initializing a new Git repository. Hash the captured source again
before retirement and prove that it still matches the frozen input.

Harry's required `.wedl/idempotency.json` receipt is 30,178 bytes with SHA-256
`CD86D44691867155CAB70751768E18CBC38DA9F923CB9C466A0194C314DAFA22` at the
planning snapshot. Inspect its credential safety and preserve its exact bytes;
re-audit any drift rather than treating all `.wedl` content as cache.
Frontiersmen's complete local history is also preserved, even though its
source duplicates the legacy compatibility fixture.

## Retirement ordering

Independent review must accept the actual archive content, safe core
classification, manifest and recovery proof before retirement. Preserve the
tracked core and manifest in the main repository before removing either root
Git link or contained local directory. Treat tracked-link retirement and
physical directory removal as separate reviewed operations with exact paths.
Frontiersmen additionally requires the concurrent-scene tests to pass without
the root world, using the retained legacy fixture and existing HEAD semantics.
Harry requires the verified unique source/history/receipt custody above.

Contain all cleanup to verified current-task-owned scratch and the explicitly
authorized retired local world path, with finite limits and a reported failure
disposition. Preserve both protected archive tiers, historical reports,
existing outputs and unrelated evidence. No Git initialization, history rewrite,
implicit world upgrade or blanket cache/scratch deletion is authorized.

<!-- @adrai:eyJhIjp7ImkiOiJ3ZWRsLWV4YW1wbGUtY29uc29saWRhdGlvbi1kZXZlbG9wZXIiLCJrIjoibGxtIiwibSI6ImdwdC02LjEtc29sIn0sImIiOiIyMjQwNzNlYzIxNDRkOTM2YWZlNmUzMWFjMDM0ZTM2MGViZGMwOTk0IiwiaSI6InNoYTI1NjowaFlWcWhCZVU2c1ZGd2puM3UtN0xVOVhHWXJRdEZFV05jVVZYazE0cVVJIiwiayI6ImRlY2lzaW9uLmNyZWF0ZSIsIm8iOiJSMDFNNEIxMURCVkNUSlJOU0pNOFFKUFZNRVYiLCJvcCI6Ik8wMU00QjExREJWQ1RKUk5TSk04UUpQVk1FViIsInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OjlRQW02TC1JZ00xeWJqdzJ4dFlsNUowcS15WlpnZTZXUmdGRGZab2V5ZGMiLCJ0IjoxNzkxMzcxNTU2MjE5LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
