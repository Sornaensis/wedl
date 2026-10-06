+++
schema = "adrai/decision/v1"
adr = "A01M491Y1RVN98VZDF318XJ1ARW"
record = "R01M491Y2TKQ2TF99JSG9SBRM7E"
title = "Generational compiled projection and readiness contract"
summary = "Preserve the existing compilation contract with current v0.7 behavior and original approval provenance."
domains = ["compilation", "generational"]
+++

# Generational compiled projection and readiness contract

This record migrates the existing `docs/GENERATIONAL_COMPILATION.md` contract into ADRAI. The original ADR 0005 (`A01M48NQZ50KH9V094Q5A3138R0`) remains the authority for its 2026-08-30 approved proposal and historical five-token decision vectors. Its existing 2026-10-05 typed-knowledge extension remains separately identified; this migration does not invent a new approval or rewrite the original approval stage. Current implementation statements below distinguish that stage from the implemented v0.7 component.

The v0.7 source grammar and validation are defined by ADR 0005 and
the ADRAI source record `adrai --repo WEDL_SOURCE_CHECKOUT show A01M491X2B6KF0PZFZ4HJMBFSSR --json`. Markdown in Git remains canonical. The
compiler creates a disposable SQLite evidence model from validated source;
deleting the cache does not alter authored history.

## Stored evidence

`generational_record` carries kind, source ordinal/path, status, world-only
capability, timeline, audience, and perspectives. Typed tables retain organization containment,
directed parentage, n-ary union participants per literal transition,
affiliations and roles, legacies,
separate tenures and claims, and optional vital history. No ancestry closure,
successor, holder from a claim, or spatial topology is inferred or stored.
The union participant posting is per literal transition and may retain former
members. Reverse reads deduplicate candidates and fold current membership and
state at the requested horizon. The `(organization_id,id)` legacy index seeks
only authored organization links; it does not imply a current holder or
succession.

`generational_transition` stores every literal initialization and transition,
including its ID, kind, exact signed tick/order, inclusive interval end when
present, source ordinal, payload, cause, replacement target, and authored
citation. Rows sort by `(timeline, tick, order, source ordinal, transition ID)`.
The citation identifies the source path, blob, revision, and exact transition
section. Cause events retain their own source citation. A winning replacement
retains citations to its superseded chain.

`generational_current` records a deterministic fold at the selected world
cursor. `generational_candidate` retains private per-transition structural
fields with their timeline, applicability bounds, audience, perspectives, and
citation. Record-level labels are omitted there because they can summarize a
later transition.
Generic metadata, body, titles, aliases, tags, and transitions from these eight
kinds do not enter `search_document`, FTS5, or vector training/retrieval.

`generational_discovery_name` is a private, disposable title/alias posting
table for the read-only generational interface. A canonical authored fact
admits a linked character or cause event only at its first applicable
StoryTime, under that fact's audience and perspective. Organization and legacy
names come from their literal initialization or rename payloads, not from a
later source summary title. The small `generational_discovery_lens` table lets
the local author adapter derive its trusted lanes without scanning source
records. `generational_discovery_time` ranks exact `(tick, order)` instants
separately in each audience/perspective lane;
`generational_discovery_segment` stores private time-prefix postings for
character/event names and horizon interval postings for organization/legacy
names, so each name range is sought only among rows eligible at the requested
horizon. Interval lookup has a fixed depth independent of later source instants.
An overlong literal title closes that organization or legacy while
it is current, without suppressing its earlier bounded title. Static titles
longer than 256 UTF-8 bytes close their entities; overlong aliases are omitted.
Ready-cache discovery reads use these compiled rows without building
a whole-world `World`. If the schema or named index shape is stale, the cache
is rejected and rebuilt from the unchanged Git source.

## Internal replay

`fold_record` takes an explicit StoryTime and an already visibility-filtered set
of candidate record IDs. It applies only authored transitions on that timeline
up to the exact `(tick, order)` boundary. The inclusive vacancy operation
returns `vacant-interval` through its last instant and a nonpersisted `expired`
state after it. A transfer vacates its source tenure; it never creates an
implicit hold on its target. Legal and de-facto literal holds remain distinct,
and claims never count as tenure.

`cited_ancestors` and `cited_containment` use indexed direct edges and bounded,
cycle-safe breadth-first traversal. Ancestry sorts each whole depth by authored
applicability StoryTime, source ordinal, and stable edge ID. Distinct authored
edges to one parent retain separate citations while that parent expands once.
Callers must provide depth and item bounds;
if either is exhausted, the result is a closed `limit` outcome with no partial
path. A detected containment cycle has the same closed outcome. Returned edges
cite authored records. These primitives do not select a
viewer, authorize source, implement complete author-as-of or character
visibility, or expose public query responses. That policy belongs to the
downstream generational query layer, which must filter before traversal or
ranking.

## Cache compatibility

The disposable SQLite schema identifier is `wedl-sqlite/v15` and the
generational compiler generation token is `wedl-generational-index/v9` for
the reverse legacy index. Both direct and in-memory authoring-byte
compilation use the same projection insertion path. A schema/token mismatch,
missing or malformed required table or index, failed SQLite integrity check, or failed
foreign-key check rejects the cache; a complete validated candidate replaces
it atomically. Full and fast-forward rebuild modes create the same generational
rows from the same source.

## Historical generation and current knowledge projection

The earlier v8 documentation described its then-current reverse legacy index. It is historical provenance, not the current cache fingerprint: `wedl.compiler.GENERATIONAL_INDEX_GENERATION_TOKEN` is `wedl-generational-index/v9`, and the SQLite schema is v15. Existing v8 caches require the ordinary disposable-cache rebuild; canonical Git source remains unchanged. The v15 typed belief projection uses `generational_knowledge_assertion`, `generational_knowledge_endpoint` and `generational_knowledge_evidence`, with knower, endpoint and state indexes. These represent authored assertions and admitted learning evidence, not canonical relationships. Ready-cache compatibility still checks the exact required shapes and revision fingerprint.

## Retained benchmark methodology and unresolved release target

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
before and after its activation. In this historical measured core-only fixture and its `TrustedViewerScope`, character mode has no positive
structural knowledge grant; the fixture does not negotiate `generational-knowledge-v1`. The current typed-knowledge source extension is separate; a repository session token
does not prove one. The fixture uses no generic FTS record for generational
evidence. Normal pytest checks the bounded CLI and authenticated HTTP
confirmed-apply paths, immediate strict compiled reads, exact replay, and
stale/missing-confirmation nonmutation. No normal test asserts time or memory.
After timed reads, the benchmark deliberately drops the private search index,
requires an `incompatible` cache diagnosis, and checks that an ordinary read
rebuilds the disposable cache without changing the authored Git tree.

The historical measured result did not meet the 250 ms warm production-path target. Preserve that failure and the strict bounded-readiness/complexity requirement: a future optimization must preserve the same fingerprint, corruption diagnosis, scope/privacy/horizon admission and strict compiled-read semantics while making the already-ready pinned revision readiness check bounded in cost. Neither this documentation migration nor ordinary focused checks constitute new performance qualification. Historical numeric results and raw samples remain in [the immutable report](../../../../docs/reports/generational-benchmark.md).

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiOTRjMGEyMjJmMjI1MDFjODIxN2NhNGNkNDkxN2Y2YTYyZjBjYjVjZCIsImkiOiJzaGEyNTY6YW0wN0FTeTNmTWpBd0RfVXMwZ1I2a2pRbExVNVhsSTNSQVppcERXTzltWSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5MVkyVEtRMlRGOTlKU0c5U0JSTTdFIiwib3AiOiJPMDFNNDkxWTJUS1EyVEY5OUpTRzlTQlJNN0UiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1Njpnd3oyUVJpLTdQRVF2aHBpcTNWTlp6Mzk5c3BDb3UtdURMcmtzQi10aUhBIiwidCI6MTc5MTMwNTM4NjgzNSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
