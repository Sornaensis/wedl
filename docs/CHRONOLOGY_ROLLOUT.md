# Chronology rollout

Chronology is released at source schema `wedl/v0.6`. New repositories use the
v0.6 grammar; existing homogeneous v0.3 and v0.5 repositories move through the
local, confirmed `upgrade-v06` migration. The command snapshots the exact
expected Git head, previews a complete source diff, then applies that bound
preview as one forward commit. `upgrade-v06` is a validator-checked no-op for
an already-valid v0.6 repository.

The support matrix, response envelope, backup ref, receipt behavior, and
rollback procedure are specified in [the migration contract](CHRONOLOGY_MIGRATION_CONTRACT.md).
The migration has no HTTP route: browsers and downstream HTTP clients discover
capability from the chronology catalogue/status response after an operator has
performed the local upgrade.

| source | local `upgrade-v06` | HTTP/UI migration |
|---|---|---|
| v0.3 / v0.5 | preview then confirmed forward commit | unavailable |
| v0.6 | validator-checked no-op | unavailable |
| v0.4, mixed, malformed, retired, future | structured rejection | unavailable |

The packaged `chronology_conformance` fixture spans calendar labels -249
through 250, includes two anchored convertible calendars and one deliberately
isolated calendar, eras, leap and intercalary rules, every value form, open
ranges, qualitative uncertainty, conflicts, relative claims, durations, and
`x-*` extension data. It is source-first: SQLite is disposable output rebuilt
from the source snapshot.

Run `uv run python tools/build_chronology_conformance.py` for fixture,
validation, and compiler evidence. The index benchmark remains the performance
gate: 10,000 claims and a 768 MiB per-record RSS target. If a deployment cannot
meet that target, record its measured environment and follow-up before release.

Calendar conversion requires an explicit anchor or mapping. StoryTime
`timeline`/`tick`/`order` is a replay-order key only: a tick gap never supplies
an elapsed duration or date conversion.
