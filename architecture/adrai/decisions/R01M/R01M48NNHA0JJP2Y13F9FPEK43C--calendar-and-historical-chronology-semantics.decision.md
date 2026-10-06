+++
schema = "adrai/decision/v1"
adr = "A01M48NNH43QCFRPC68NQQTFT63"
record = "R01M48NNHA0JJP2Y13F9FPEK43C"
title = "Calendar and historical chronology semantics"
summary = "Finite calendar and historical-date evidence semantics preserve global ordinal StoryTime, exact bounds, and explicit mappings without inference."
domains = ["chronology"]
+++

# ADR 0003: Calendar and historical chronology semantics

- **Status:** Accepted
- **Approved:** 2026-08-27 by Project owner (user)
- **Scope of approval:** The conservative decisions listed in this ADR only.
- **Decision vectors:** [examples/calendar-chronology-semantics-v1.yaml](../../examples/calendar-chronology-semantics-v1.yaml)

## Context and scope

This is an accepted semantic contract only. It adds no source schema, parser,
API, model, compiler, query, UI, plugin, remote service, executable calendar
code, or migration. Git commit/wallclock time is provenance, never fiction.

There remains one shared world and one global `StoryTime` `(timeline,tick,order)`.
It is the sole replay, state, causality, ordering, and horizon key. Civil dates,
era/regnal labels, duration claims, and historical claims are displays/evidence,
not clocks. Every thread shares the same mappings and interpretation; membership
changes no time, canon, rank, corpus, vector, state, or causality.

## Accepted decision

The machine civil axis uses a signed proleptic year (including zero) and signed
day. `CivilDay` is a day number, `CivilMonth` a `(year,month)` label, `Year` a
signed proleptic year, `AxisDay` the shared signed fixed-day coordinate, and
`EraDate` a display label. Fixed-day difference is checked signed-i64 subtraction
of two exact `AxisDay` values; an overflowing difference is invalid. A civil day
is the minimum unit: no time-of-day, timezone,
locale, or Gregorian privilege. Display eras may omit zero or overlap; they are views, not
clocks. Year and month precision mean inclusive full intervals; explicit
endpoints are inclusive and omission is unbounded. Approximation is independent.
Qualitative/unbounded approximate claims may display but cannot order exactly,
gain invented tolerance, or become bounded. Explicit bounds may overlap.

Conflicting claims retain ordered provenance; no averaging or canonical winner.
Relative claims are evidence only: no prose/transitive ordering or StoryTime
rewrite. Fixed-day differences are exact only on the shared axis. Calendar-relative
durations are recorded/displayed, never arithmetic or ticks.

Calendars cross-convert only through an explicit shared axis/epoch; a valid
unconvertible calendar is unavailable. Calendar-to-StoryTime mappings require
explicit anchors: no interpolation, nearest match, inference, or thread rule.
Closed outcomes are invalid, unavailable/no-match, one unique global StoryTime,
or ambiguous ordered global StoryTime matches. Exact anchors are monotone and
one-to-one: duplicate-axis/different-StoryTime anchors are invalid. Ambiguity
comes only from an already-valid mapping query's candidate matches, which sort
by the global StoryTime tuple. Mappings are display-only.

Calendar mechanics are finite month/leap/intercalation tables, cycles, or
residue rules only. All axes, years, and caps are signed i64 with checked
arithmetic and floor division for negative years. Cycle conversion is O(1);
there is no month/year addition. Anchors are strict: exact duplicate aliases
coalesce, incompatible ties reject, and ambiguity sorts global StoryTime tuples
lexicographically. Approximate claims establish overlap only. The vector's
reproducible semantic-only corpus spans -249..250 (500 years), 10,000 generated
events, 20 yearly claims, and 100 anchors plus aliases across two linked
calendars and one isolated calendar. It sets the review caps: 500-year
tables/cycles, 10,000 claims per batch, and O(1) cycles; any larger design
requires a new approved ADR. Stable semantic outcomes are `CAL-SEM-*` IDs and
are checked only by pure test-local conformance oracles.

The vector separates finite definition preparation from conversion work. A
calendar prepares one prefix row per cycle residue and one month-prefix cell
per valid residue/month; its stated solar, regnal, and isolated definitions
require 52, 66, and 2 cells respectively (120 total). Thereafter each civil
date normalization performs one floor quotient/remainder, one cycle-prefix
lookup, one month-prefix lookup, and one month-length validation. Mapping an
epoch-normalized date performs that bounded normalization for the subject and
the explicit epoch; it never scans years or months. These are accepted review
work bounds, not an implementation requirement.

## Consequences

Implementation requires separate approved schema/runtime work. This decision
does not establish historical truth or implementation support.

### Semantic conformance identifiers

`CAL-SEM-001`: CivilDay and AxisDay are signed i64; prepared finite-cycle prefixes make conversion one floor quotient/remainder plus bounded lookups and checked arithmetic; fixed-day difference is checked i64 subtraction of two exact AxisDays only.
`CAL-SEM-002`: Year/month precision and explicit ranges are inclusive; omitted endpoints are unbounded; approximation is overlap-only and qualitative approximation has no exact order.
`CAL-SEM-003`: Anchors require exact monotone one-to-one pairs; exact aliases coalesce and ties/crossings reject; ambiguity comes from valid mapping candidates sorted tuple-lexicographically.
`CAL-SEM-004`: Calendar conversion requires an explicit shared axis and epoch; mapping outcomes are invalid, unavailable, unique, or ordered ambiguous.

<!-- @adrai:eyJhIjp7ImkiOiJjb2RleCIsImsiOiJsbG0iLCJtIjoiZ3B0LTYuMS1zb2wifSwiYiI6IjlhODA5ZWNhMDM4NzdmNzFmMDcwYTU0MDE5NTE4YmRlZGQxYjM2N2QiLCJpIjoic2hhMjU2OjNudHNhZzFoQnA0d3ZtU1prSG9EbUhGMEhUUk5xRk5kRnpoaVk1cWR5UTgiLCJrIjoiZGVjaXNpb24uY3JlYXRlIiwibyI6IlIwMU00OE5OSEEwSkpQMlkxM0Y5RlBFSzQzQyIsIm9wIjoiTzAxTTQ4Tk5IQTBKSlAyWTEzRjlGUEVLNDNDIiwiciI6Im1hc3RlciIsInMiOiJzaGEyNTY6NTZEc0xmb1k3TnVsOG1PMm1KUFdjNDFZRFJBaWs2SVN3ZnJHeFlKMl8xRSIsInQiOjE3OTEyOTI1MjM4NDAsInYiOjEsIngiOiJhZHJhaS8xLjAuMCJ9 -->
