+++
schema = "adrai/decision/v1"
adr = "A01M48XYVYK43DWXQTC541WSFZK"
record = "R01M48XYW4AKSASDM3M34P2H660"
title = "Chronology disposable index and typed query contract"
summary = "Migrated CHRONOLOGY_INDEX_QUERY_CONTRACT.md; current implementation boundaries and original contract semantics."
domains = ["chronology"]
+++

## Authority and provenance

Migrated from `docs/CHRONOLOGY_INDEX_QUERY_CONTRACT.md` at Git revision `daf78b54ebfa40a7e9757bb7cdb1179db779b7b3`. This records the existing contract and corrects current implementation descriptions; it does not create a historical approval or replace the authority of the original calendar chronology decision A01M48NNH43QCFRPC68NQQTFT63. Earlier approval-stage wording and immutable measured evidence retain their historical meaning.

# Chronology index and query contract

Chronology indexes are a disposable `wedl-sqlite/v15` read model. Markdown/Git
remains canonical: a source/schema/compiler fingerprint mismatch rebuilds a
complete temporary SQLite database and atomically replaces the old snapshot;
there is no SQL migration or public SQL interface.

Validated `wedl/v0.6` and `wedl/v0.7` worlds with chronology declarations populate the chronology tables. They retain
author order, provenance and exact JSON for calendars, eras, anchors and
annotations. A persisted capability sentinel distinguishes a legacy empty
projection from a valid v0.6/v0.7 empty catalog, so v0.3/v0.5 source and SQLite
reads return the same typed no-chronology outcome.

The source and compiled stores use one typed evaluator and return the same
outcomes, hit shapes, advisory codes, and author-order tie breaks. Source IDs
remain in every returned value; the private canonical IDs needed by the kernel
are never a query surface. The evaluator materializes comparable endpoints as
either shared `axis` days or local `calendar:<source-id>` ordinals. Thus a
calendar without an epoch supports inclusive exact/range overlap within itself,
but cannot compare to another calendar or map to StoryTime.

`axis` is the only cross-calendar comparison basis. A calendar without an
explicit epoch uses `calendar:<id>` and can only be compared with claims in the
same isolated calendar. It cannot be converted to StoryTime. Conflicts,
relatives and qualitative unbounded approximations are non-comparable.

The source and SQLite stores expose typed internal Python calls. The active CLI/HTTP adapter is specified separately by the chronology API contract; it consumes the compiled store. Exact and
interval comparisons use inclusive endpoints; `on_date` evaluates the complete
normalized operand interval rather than just its lower endpoint. Approximate claims are eligible
only for overlap operations and emit an advisory; ordering never invents a
global order. Results otherwise retain deterministic source order.

An era filter is resolved before either store reads candidate annotations. It
is either `authored` (the annotation explicitly used that known era) or
`overlaps_bounds` (the annotation interval independently overlaps that era's
inclusive local or axis bounds). In `overlaps_bounds` mode, a row must satisfy
both the original request predicate and the era predicate; their windows are
never intersected into a replacement request window, so a wide range may
satisfy both even when the two windows are disjoint. An unknown era is an
`invalid/era` outcome in both modes.
Era-date operands delegate display-year conversion and inclusive bounds to the
kernel; epoch calendars use kernel axis normalization and isolated calendars
use the prepared local ordinal only after that conversion. Eras without bounds
cover their calendar basis. A valid cross-basis claim is simply outside an era
bounds selector; it is never converted or globally ordered.

Advisories are aggregate and deterministic: at most one each, in this order,
for `approximate-overlap-included`, `approximate-relation-excluded`,
`noncomparable-excluded`, and `result-limit`. Each has a positive count and a
constant message. A finite approximate claim is included only for an actual
`overlaps` relation; if it would otherwise satisfy an ordering predicate it is
counted as excluded instead. Conflicting, relative, duration, and qualitative
unbounded approximation claims are retained for provenance but cannot create a
comparable hit. The disposable row projection persists their closed exclusion
classification, so source and SQLite advisory counts are not inferred from a
row missing from a candidate query.

Exclusion scope is explicit projection metadata: every non-comparable row has
zero or more known comparison bases, zero or more directly or recursively
authored era memberships, and an unknown-basis flag. A query counts it only
when its known basis contains the query basis or its basis is unknown; an
`authored` era filter additionally requires the explicit era membership. This
metadata never makes the row comparable or creates an endpoint/order.

Candidate access is stream-only. SQLite candidate selection uses an immutable
shared plan, is parameterized and conservative, and is indexed by comparison
basis/endpoints and authored-era/basis order. Non-comparable advisory counts
use their own indexed exclusion path. It fetches at most 256 candidates per
complete keyset page and records first and continuation plans. Source and
SQLite use the same ascending author-order key:
`(record_ordinal, source_ordinal, record_id, annotation_id)`, where records
are ordered by `(source_path.casefold(), record.id)` and `source_ordinal` is
the authored chronology-array position. Evaluation consumes the complete
stream for exact advisory and count totals, while retaining only the first
requested number of matching rows in that order. The result-limit advisory is
the total relation count minus the retained limit, so neither store needs
unbounded hit retention or an early SQL limit.
Ordinary
internal reads may rebuild the disposable cache; strict reads require an
already-current compiled cache. The benchmark uses a temporary non-Git source
root, production serialization, validation, state-profile compilation, and
the production source and SQLite chronology stores. Its full result records
cache lifecycle, compiler/inserter stats, bounded candidate/query-plan
evidence, conversion parity, and machine-specific timings; --smoke is the
smaller production-path CI mode. The reproducible 500-year/10,000-claim runner
is `uv run python tools/benchmark_chronology_index.py`; its output metadata and
budget are recorded in `docs/reports/chronology-index-benchmark.json`.

The current compiler fingerprint includes `wedl-chronology-index/v3`; the historical index contract originally named SQLite v6. The relocated JSON is dated measured evidence from its recorded input and environment, not a fresh qualification of the current compiler.

## Current source and delivery boundary

The generic loader, validator, and compiler accept homogeneous `wedl/v0.7` worlds. Their chronology projection accepts validated v0.6 and v0.7 declarations; legacy v0.3/v0.5 worlds have no chronology projection. This extends compiled read-model coverage without rewriting the v0.6 grammar or the local `upgrade-v06` input policy.

The existing public adapter has a narrower advertised capability: `chronology_capability` enables calendar chronology only for v0.6, and the catalogue returns empty definitions for v0.7. `chronology.replace` also requires v0.6 and rejects v0.7 before making a changeset. In contrast, format, convert, search, and story-times obtain the compiled chronology store without that capability gate, so a valid v0.7 projection can be evaluated by those operations. This asymmetry is the current implementation boundary, not a new uniform public v0.7 admission policy. Source references: `src/wedl/chronology_api.py`, `src/wedl/chronology_index.py`, and `src/wedl/authoring.py`.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNGQwNjRlOWE3YWU5NmEzN2I4ZGQwYWUzYjVhNjI0ODY2YTkwZmE5MiIsImkiOiJzaGEyNTY6aVZfRjRaQmFZUXFvalRhOTBNUVJQaDlIY2Q0WmlIMmpPeUdUaEZGaGVMYyIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WFlXNEFLU0FTRE0zTTM0UDJINjYwIiwib3AiOiJPMDFNNDhYWVc0QUtTQVNETTNNMzRQMkg2NjAiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1Njp4dXlrMjRZaWk4SHA2ZG0ycS1VOUhSeHdDdVFQTjNFeVNxaWRyeVhzbUtnIiwidCI6MTc5MTMwMTIxODQ0MiwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
