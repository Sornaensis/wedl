# Historical authored generational benchmark

This report retains the measured result and its limitations from the former `docs/GENERATIONAL_BENCHMARK.md`. No new measurement or qualification was performed for the documentation migration; the original report does not name a measurement date or input Git revision, so neither is inferred. The measured builder and `TrustedViewerScope` negotiate only `generational-core-v1`. Its no-positive-character-grant check describes that input, not the current optional typed-knowledge capability.

## Measured Windows result

The checked-in [result](generational-benchmark.json) was measured on Windows
11, Python 3.13.15, SQLite 3.50.4, and the default hybrid search profile.
The 5,063-character, 10,004-edge, 101-generation, 504-transition source
validated with no diagnostics. Its two independent source hashes match;
source and strict compiled answers, privacy, and horizon checks passed.
Cold compilation took 21,473.804 ms; the full stage, including the cache
corruption and rebuild check, took 176,065.531 ms. SQLite occupied 482,467,840
bytes and sampled peak RSS was 777,297,920 bytes. The deliberately damaged
index was diagnosed as incompatible and rebuilt without source changes.

The warm production-path target **missed**: the maximum operation p95 was
2,626.816 ms versus 250 ms. The six operation p95 values are retained with
all individual samples in JSON. A cumulative profile of the slowest query
shows 1.451 seconds of a 1.473-second call inside `require_database`; the
earlier run located about 0.900 seconds in `_compiled_database_issues` on
195 SQLite executions. This points to repeated full cache-compatibility work
on warm strict reads. The bounded-readiness follow-up remains in the ADRAI projection contract. The benchmark result keeps `warmTargetPassed:
false`; the release target remains open until that follow-up is measured.

## Methodology and fresh runs

The current methodology and unresolved target are durable in ADRAI: `adrai --repo WEDL_SOURCE_CHECKOUT show A01M491Y1RVN98VZDF318XJ1ARW --json`. The checked-in JSON is immutable historical evidence; a fresh run produces different evidence and must use a fresh explicitly owned output path with a finite retention disposition. Do not overwrite this report or its JSON.

```powershell
$env:PYTHONPATH = 'src;tools;.'
.venv\Scripts\python.exe tools/benchmark_generational.py --output output/OWNED_FRESH_RUN/generational-benchmark.json --repeats 3
```

The caller creates a new contained `OWNED_FRESH_RUN` directory and specifies its owner and expiry before invocation. This recipe is for a separately authorized benchmark run, not a required documentation check. Original pipeline, privacy/horizon checks, RSS sampling limitations and failure analysis remain in the projection record; individual measured samples and `warmTargetPassed: false` remain byte-for-byte in [the result](generational-benchmark.json).
