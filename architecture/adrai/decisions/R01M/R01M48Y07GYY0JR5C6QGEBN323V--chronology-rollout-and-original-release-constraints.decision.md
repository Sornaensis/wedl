+++
schema = "adrai/decision/v1"
adr = "A01M48Y06MMHSR2E0A4R094QQ15"
record = "R01M48Y07GYY0JR5C6QGEBN323V"
title = "Chronology rollout and original release constraints"
summary = "Migrated CHRONOLOGY_ROLLOUT.md; current implementation boundaries and original contract semantics."
domains = ["chronology"]
+++

## Authority and provenance

Migrated from `docs/CHRONOLOGY_ROLLOUT.md` at Git revision `daf78b54ebfa40a7e9757bb7cdb1179db779b7b3`. This records the existing contract and corrects current implementation descriptions; it does not create a historical approval or replace the authority of the original calendar chronology decision A01M48NNH43QCFRPC68NQQTFT63. Earlier approval-stage wording and immutable measured evidence retain their historical meaning.

# Chronology rollout

Calendar chronology was released at source schema `wedl/v0.6`. Empty/non-example `wedl init` currently creates v0.3 source; explicit packaged examples retain their own source schema. Existing homogeneous v0.3 and v0.5 repositories move through the
local, confirmed `upgrade-v06` migration. The command snapshots the exact
expected Git head, previews a complete source diff, then applies that bound
preview as one forward commit. `upgrade-v06` is a validator-checked no-op for
an already-valid v0.6 repository.

The support matrix, response envelope, backup ref, receipt behavior, and
rollback procedure are specified in the migration contract A01M48XY4R2CJ698Z2VQM9SG4ZZ (read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M48XY4R2CJ698Z2VQM9SG4ZZ --json` in the WEDL development/source checkout).
The migration has no HTTP route: browsers and downstream HTTP clients discover
capability from the chronology catalogue/status response after an operator has
performed the local upgrade.

| source | local `upgrade-v06` | HTTP/UI migration |
|---|---|---|
| v0.3 / v0.5 | preview then confirmed forward commit | unavailable |
| v0.6 | validator-checked no-op | unavailable |
| v0.4, v0.7, mixed, malformed, retired, future (for this mode) | structured rejection | unavailable |

The packaged `chronology_conformance` fixture spans calendar labels -249
through 250, includes two anchored convertible calendars and one deliberately
isolated calendar, eras, leap and intercalary rules, every value form, open
ranges, qualitative uncertainty, conflicts, relative claims, durations, and
`x-*` extension data. It is source-first: SQLite is disposable output rebuilt
from the source snapshot.

The original release constraint requires fixture, validation, and compiler evidence from `uv run python tools/build_chronology_conformance.py`, plus the index benchmark performance gate of 10,000 claims and a 768 MiB per-record RSS target. Preserve that release requirement: a deployment that cannot meet it records its measured environment and follow-up before release. The dated `docs/reports/chronology-index-benchmark.json` records its own environment and result; this documentation migration performs no new benchmark or performance qualification. Practical reproduction and local migration commands are in [the chronology guide](../../../../docs/guides/chronology.md).

Calendar conversion requires an explicit anchor or mapping. StoryTime
`timeline`/`tick`/`order` is a replay-order key only: a tick gap never supplies
an elapsed duration or date conversion.

## Current source and delivery boundary

The generic loader, validator, and compiler accept homogeneous `wedl/v0.7` worlds. Their chronology projection accepts validated v0.6 and v0.7 declarations; legacy v0.3/v0.5 worlds have no chronology projection. This extends compiled read-model coverage without rewriting the v0.6 grammar or the local `upgrade-v06` input policy.

The existing public adapter has a narrower advertised capability: `chronology_capability` enables calendar chronology only for v0.6, and the catalogue returns empty definitions for v0.7. `chronology.replace` also requires v0.6 and rejects v0.7 before making a changeset. In contrast, format, convert, search, and story-times obtain the compiled chronology store without that capability gate, so a valid v0.7 projection can be evaluated by those operations. This asymmetry is the current implementation boundary, not a new uniform public v0.7 admission policy. Source references: `src/wedl/chronology_api.py`, `src/wedl/chronology_index.py`, and `src/wedl/authoring.py`.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiZGJmNjU1MzEyYTJmOGQ2NzJjNDM1NDllYzQ0YTU4MWU0NzAxM2IxMSIsImkiOiJzaGEyNTY6bGZDM1hyQjVhQU9WbW9GOHpVQXlyUXFaNFFDS1BqaGlfMDVMQWJOV3JFcyIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WTA3R1lZMEpSNUM2UUdFQk4zMjNWIiwib3AiOiJPMDFNNDhZMDdHWVkwSlI1QzZRR0VCTjMyM1YiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpUZEJBbmxndTFrcjFpMl9RMzA0RzhTWk5vZmRUU0VRMEE0MTliWGFtdG5vIiwidCI6MTc5MTMwMTI2Mjg3OCwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
