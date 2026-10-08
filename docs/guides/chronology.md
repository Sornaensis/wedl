# Working with calendar chronology

Run these commands in a WEDL world repository, passing its path with `--repo` when needed. Check the catalogue before editing calendar annotations:

```sh
wedl chronology catalog --repo PATH
wedl chronology catalog --repo PATH --require-compiled
```

The second form requires a current compiled cache. Read commands accept a raw `wedl-chronology/v1` JSON file, or `-` for stdin:

```sh
wedl chronology format request.json --repo PATH
wedl chronology convert request.json --repo PATH
wedl chronology search request.json --repo PATH
wedl chronology story-times request.json --repo PATH
```

For example, a format request is:

```json
{"protocol":"wedl-chronology/v1","value":{"kind":"civil","calendarId":"calendar_0123456789abcdefghjkmnpqrs","year":"1","month":"1","day":"1"}}
```

Use an existing calendar ID from the catalogue. Coordinates are decimal strings. An `invalid` or `unavailable` semantic outcome is returned in JSON; inspect it before using the result. StoryTime tick gaps do not supply elapsed durations.

To upgrade a homogeneous v0.3 or v0.5 world locally, supply the exact audited Git HEAD and a stable request key:

```sh
wedl migrate preview --repo PATH --mode upgrade-v06 --expected-head HEAD --idempotency-key KEY
wedl migrate apply --repo PATH --mode upgrade-v06 --expected-head HEAD --idempotency-key KEY --source-snapshot-hash HASH --confirm TOKEN
```

Inspect the complete preview diff and diagnostics. Use its exact `sourceSnapshotHash` and `confirmationToken` in apply. Keep the same HEAD and key. Migration is local; HTTP and browser clients do not perform it. v0.4 needs its separate recovery route first; v0.7 is outside `upgrade-v06`, even though the current generic loader and compiler support it. Empty initialization creates v0.3; public packaged examples use v0.7. The source checkout retains the v0.6 chronology conformance input in `tests/fixtures/legacy_worlds/chronology_conformance/story` for catalogue and replacement coverage.

For a v0.6 world, preview a complete catalogue and/or complete annotation-array replacement with:

```sh
wedl author chronology replace replacement.json --repo PATH --expected-head HEAD
```

Inspect the preview and use the normal confirmed changeset workflow. The catalogue capability and replacement authoring currently enable calendar chronology only for v0.6. A v0.7 world can compile chronology, and format/convert/search/story-times can evaluate that projection, while its catalogue returns empty definitions and replacement authoring rejects it. Check this distinction when selecting a workflow.

Read the architectural contracts with the ADRAI CLI in the WEDL development/source checkout:

```sh
adrai --repo WEDL_SOURCE_CHECKOUT search --mode fts "Chronology" --json
```

The current schema, validation, migration, index, API, and rollout decisions are A01M48XWQD0809VQ3RD9NBYPSKB, A01M48XXBREJY497SNXZVCW85EC, A01M48XY4R2CJ698Z2VQM9SG4ZZ, A01M48XYVYK43DWXQTC541WSFZK, A01M48XZJ6V1X1QSM4QEZJZS81M, and A01M48Y06MMHSR2E0A4R094QQ15. Show a returned stable ID with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json`. The [small request examples](../examples/chronology-api-v1.yaml) provide complete positive requests to adapt to your world.

For local reproduction in the development/source checkout:

```sh
uv run python tools/build_chronology_conformance.py
uv run python tools/benchmark_chronology_index.py --smoke
```

The [chronology benchmark JSON](../reports/chronology-index-benchmark.json) retains its original measured environment, input, timings, and budget result. It is historical evidence. The full runner uses 500 years and 10,000 claims; the original release constraint is retained in the rollout decision. Reproduction creates new evidence for the environment where it runs.

To save a new full run, supply an absent filename within your existing contained,
task-owned run directory (with a named owner and finite lifetime):

```text
uv run python tools/benchmark_chronology_index.py --write --output output/CALLER_OWNED_RUN/chronology.json
```

The parent must already exist. Omitting --output or naming an existing file fails before the
benchmark starts; archived report JSON cannot be overwritten, even when named
explicitly. Default/full/smoke calls still print stdout. A failed write-mode run
may leave its newly reserved file empty; resolve that owned artifact with the
failure diagnostics. A new run uses current implementation/environment inputs,
not the archived receipt's old fingerprint or measurements.
