# Authored generational benchmark

Run the standalone full corpus outside normal pytest:

```powershell
$env:PYTHONPATH = 'src;tools;.'
.venv\Scripts\python.exe tools/benchmark_generational.py --output docs/generational-benchmark.json --repeats 3
```

The generator reads the accepted v0.7 schema vector and emits one homogeneous
`generational-core-v1` world. The full profile has 101 linked cohorts of 50
characters, two literal directed parentage records per child after the first
cohort, nested dynasty and houses, n-ary unions, role changes, and five office
histories. Reciprocal claims, legal and de-facto holders, interval vacancies,
literal predecessor links, adoptions, signed ticks, and same-tick order
boundaries are source records. Ticks are exact unitless ordinals; the chronicle
labels supply the multi-century scenario, without equating ticks to years.
The bounded 4-by-4 profile uses the same builder and is tested normally.

The script builds twice independently and hashes each sorted canonical path and
serialized source byte stream. It validates source, clones the checked-out
repository to a temporary directory, replaces its `story` contents with the
generated records, commits that fixture in the disposable clone, and compiles
to disposable SQLite. `coldStagesMs` is the production compiler's own source
load, validation, and compile breakdown; `coldCompileMs` wraps the complete
call. `sourceCompiledParity` compares independently projected source evidence
with revision-pinned strict compiled reads, including citations and StoryTime.
The known-answer digest excludes the temporary Git SHA so it is repeatable
across independent runs. SQLite bytes and sampled peak process RSS describe
the cold plus warm process; RSS sampling is every 50 ms and may miss a shorter
peak.

Each warm measurement calls `query_generational(..., require_compiled=True)`:
the production API-neutral repository entry point. One warmup precedes three
timed reads per operation. The JSON records samples, median, interpolated p95,
and worst for early and late bounded lineage, dynasty roster, and office
holder/claim queries. `warmQueryMs` is the maximum reported operation p95 and
must be at most 250 ms. This includes revision/cache checks, compiled database
connection, access filtering, bounded query work, and result construction; it
is not a SQL-only proxy. The final response's platform, Python and SQLite
versions are in `profile`. If the target misses, `bottleneckProfile` contains
the slowest operation's top cumulative Python profile for a follow-up.

The privacy and horizon checks include author-visible sealed parentage, a
public and character closed `unknown`, empty hidden/future search pages with
no cursor, a sealed-free context packet, and an authored future parentage
before and after its activation. Character mode currently has no positive
structural knowledge grant in the source grammar; a repository session token
does not prove one. The fixture uses no generic FTS record for generational
evidence. Normal pytest checks the bounded CLI and authenticated HTTP
confirmed-apply paths, immediate strict compiled reads, exact replay, and
stale/missing-confirmation nonmutation. No normal test asserts time or memory.
After timed reads, the benchmark deliberately drops the private search index,
requires an `incompatible` cache diagnosis, and checks that an ordinary read
rebuilds the disposable cache without changing the authored Git tree.

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
on warm strict reads. The needed follow-up is to preserve the same fingerprint
and corruption checks while making the already-ready, pinned revision's warm
readiness check bounded in cost. The benchmark result keeps `warmTargetPassed:
false`; the release target remains open until that follow-up is measured.
