+++
schema = "adrai/decision/v1"
adr = "A01M4B11D5PBJTPFGFJYS3EJFX0"
record = "R01M4DMCJ78XM1A803M8SVEBACZ"
title = "Retirement custody and authorized archive disposal"
summary = "Remove the seven repository archive files without backup under explicit owner direction; preserve historical retirement provenance without current full-recovery promises."
domains = ["recovery", "source"]
+++

# Retirement custody and authorized archive disposal

## Current decision, 2026-10-08

Sornaensis directed removal of `archives/` from the WEDL source repository.
After being asked about the disposition of the exact seven files, including
the two private full archives, Sornaensis explicitly chose: "Remove all seven
files without a backup".

This authorization replaces the earlier requirement to retain these recovery
artifacts indefinitely. Remove exactly the following files from
`archives/world-retirement/2026-10-07`, followed by their empty contained parent
directories:

- `.gitignore`
- `frontiersmen.core.zip`
- `frontiersmen.full.zip`
- `frontiersmen.manifest.json`
- `harry-potts.core.zip`
- `harry-potts.full.zip`
- `harry-potts.manifest.json`

Do not make an external copy or backup. The rejected proposal to transfer them
outside the checkout is not authorized and must not be executed.

Before removal, verify the exact contained paths, current seven-file census,
and absence of links, reparse points, embedded Git repositories, and worktree
overlap. Preserve all other worlds, active worktrees, user and agent work,
failure evidence, and historical reports. This narrow disposition does not
authorize general archive, cache, or scratch cleanup.

The ignored full archives may contain local credentials and must never be
printed or committed. Their disposal intentionally ends the verified complete
private recovery copy, including ignored local data. No full backup, external
custody, or complete recovery guarantee remains after removal. The tracked
core archives and manifests remain retrievable from earlier main-repository
commits; this change removes them from the current tree without rewriting Git
history. No push or history purge is authorized.

## Historical admission and retirement

The original 2026-10-07 edition of this decision governed preservation before
retiring the embedded `frontiersmen` and `harry-potts` world copies. It required
complete protected local full archives and independently reviewed,
credential-free tracked recovery cores and manifests before source retirement.
That edition, its admission checks, ownership, approval, and provenance remain
available through ADRAI history. They describe the completed retirement and
its former retention obligations, rather than current custody promises.

The historical Frontiersmen tip was
`845b1d0c47783278c9f658a7df4ddba327d6c399`, with 309 source records and one
local-only commit. The Harry tip was
`bb3b92c0d201e9783b02031a7c4588adcd5ab6e6`, with 110 unique source records
and seven local-only commits. Harry's preserved idempotency receipt was
30,178 bytes with SHA-256
`CD86D44691867155CAB70751768E18CBC38DA9F923CB9C466A0194C314DAFA22`.
These are historical observations, not a claim of current full recoverability.

This amendment changes only archive disposition. It introduces no runtime
semantics, implicit world upgrade, Git initialization, or general deletion
authority.

<!-- @adrai:eyJhIjp7ImkiOiJhcmNoaXZlLWV4YW1wbGVzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiY2QyZGZkYjAwNmJiNmI2YTg4MWMwZTMwMWU1MTBjYzBhZjYzODg0YSIsImkiOiJzaGEyNTY6cEFKdmVSMXdwTEgyWWxpczdiVlRHRkIzd3RLSWZ2aG9IeUYycjhRTlV0ZyIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNERNQ0o3OFhNMUE4MDNNOFNWRUJBQ1oiLCJvcCI6Ik8wMU00RE1DSjc4WE0xQTgwM004U1ZFQkFDWiIsInAiOlsiUjAxTTRCMTFEQlZDVEpSTlNKTThRSlBWTUVWIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OnpFbTdqOE1oTU8xRlVibGF2NTFBNHljclhvWGQwQzBENkpJT1FhTHh2MFUiLCJ0IjoxNzkxNDU4OTUzNDQ4LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
