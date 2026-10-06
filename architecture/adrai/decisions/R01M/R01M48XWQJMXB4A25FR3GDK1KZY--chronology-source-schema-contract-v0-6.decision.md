+++
schema = "adrai/decision/v1"
adr = "A01M48XWQD0809VQ3RD9NBYPSKB"
record = "R01M48XWQJMXB4A25FR3GDK1KZY"
title = "Chronology source schema contract (v0.6)"
summary = "Migrated CHRONOLOGY_SCHEMA_CONTRACT.md; current implementation boundaries and original contract semantics."
domains = ["chronology"]
+++

## Authority and provenance

Migrated from `docs/CHRONOLOGY_SCHEMA_CONTRACT.md` at Git revision `daf78b54ebfa40a7e9757bb7cdb1179db779b7b3`. This records the existing contract and corrects current implementation descriptions; it does not create a historical approval or replace the authority of the original calendar chronology decision A01M48NNH43QCFRPC68NQQTFT63. Earlier approval-stage wording and immutable measured evidence retain their historical meaning.

# Chronology schema contract (`wedl/v0.6`)

The v0.6 schema is active. Older v0.3/v0.5 source remains pinned until a local
confirmed `upgrade-v06`; mixed, retired, malformed, and future labels are never
normalized record-by-record.

This is the active homogeneous source grammar. v0.6 extends v0.5's one
shared world/global `(timeline,tick,order)` StoryTime and grouping-only threads.
Validated v0.6 worlds load and compile into disposable SQLite indexes for
internal typed reads. The public chronology CLI/HTTP boundary and
`chronology.replace` authoring intent are active; `upgrade-v06` is the local
confirmed migration route and UI clients never migrate source.
v0.3 and v0.5 remain unchanged; v0.4 remains quarantined.

Existing source IDs are preserved byte-for-byte and are never renamed. New
`calendar_`, `era_`, and `chronology_` IDs use uppercase Crockford
`prefix_[0-9ABCDEFGHJKMNPQRSTVWXYZ]{26}`; ordinary character records retain
the runtime `char_` convention.
All integers below are signed i64 (not booleans) and checked; every object and
tag payload admits only its declared keys plus `x-*` extensions. Such extensions
may be preserved but cannot change meaning. Arrays
have their stated order; maps never infer, convert, rank, or select a winner.

## Closed world declaration grammar

Only `kind: world` may have `chronology`, exactly this map:

| object | required keys / exact shape | optional; cardinality; order |
|---|---|---|
| `chronology` | `calendars: [calendar]`, `eras: [era]`, `anchors: [anchor]` | each 0..500; each list stable-ID ascending |
| `calendar` | `{id: calendar_ID,label: string,rule: rule,months: [month],epoch: null|epoch}` | months 1..64 by ascending `number`; base month rows + rule year rows + all overrides total <=500 |
| `rule` | cycle `{kind:cycle,period:i64,overrides:[cycle_override]}` or table `{kind:table,years:[year_row]}` | period 1..500; no expression language; the calendar-wide mechanics cap applies |
| `cycle_override` | `{residue:i64,target_month:i64,delta_days:i64}` **or** `{residue:i64,intercalary_month:{number:i64,label?:string,days:i64}}` | residue 0..period-1; ordered by `(residue,effective_month_number)` |
| `year_row` | `{year:i64,overrides:[table_override]}` | table rules contain 1..500 signed-i64 year rows, ascending unique; only listed years are available and gaps never recur |
| `table_override` | `{target_month:i64,delta_days:i64}` **or** `{intercalary_month:{number:i64,label?:string,days:i64}}` | no residue field; ordered by `effective_month_number` |
| `month` | `{number:i64,label?:string,days:i64}` | number 1..64 ascending unique; days 1..4096 |
| `epoch` | `{civil:{year:i64,month:i64,day:i64},axis_day:i64}` | or `null`, meaning valid isolated/unconvertible; the closed civil object has no `calendar_id` and its month/day are validated against this owning calendar's selected cycle/table effective-month result for its signed year |
| `era` | `{id:era_ID,calendar_id:calendar_ID,label:string,aliases:[string],display_year_zero:bool,display_epoch:{display_year:i64,machine_year:i64},provenance:[string]}` | `bounds?:{lower:{year:i64,month:i64,day:i64},upper:{year:i64,month:i64,day:i64}}` inclusive owning-calendar civil endpoints; referenced calendar exists; aliases and provenance are author ordered; zero display disallowed when policy false |
| `anchor` | `{id:chronology_ID,axis_day:i64,story_time:{timeline:existing_timeline_ID,tick:i64,order:i32},provenance:[string]}` | provenance author order; StoryTime names an existing global tuple |

For an era with `display_year_zero: false`, display year `0` is invalid and
the zero-skipping display ordinal is `ordinal(y)=y` for `y<0` and `y-1` for
`y>0`; thus display `-1` and `1` are adjacent ordinals. `display_epoch` is
validated by the same rule. Its machine year is
`checked(machine_epoch + (ordinal(display)-ordinal(display_epoch)))`, with
checked signed-i64 subtraction and addition. When `display_year_zero: true`,
the ordinal is simply `y` and the same checked affine calculation applies.
Era bounds are inclusive exact owning-calendar civil endpoints. An `era`
annotation is converted to a machine civil interval: a year expands from that
year's first through last effective day, a month from its first through last
effective day, and a day is a singleton. Its complete interval must lie inside
the era bounds; a partial annotation that crosses either bound is invalid.

Identical `(axis_day,story_time)` anchors coalesce as provenance aliases.
Different StoryTime at one day, one StoryTime at different days, and crossings
after stable coordinate order are invalid. Finite mechanics reuse the accepted
semantic preparation accounting (52/66/2); the 10,000-record fixture is batch
evidence, not a repository limit.

For each applicable rule row, base months remain ascending; a target must name
one existing base month, intercalary numbers are unique insertions, collisions
reject, and the resulting effective month list remains numbers 1..64 and days
1..4096.

## Closed non-world annotation grammar

Every non-world record, including a hypothesis as evidence, may have
`chronology: [annotation]` (0..500; authored annotation ordering is retained). This is
a list, not the world declaration map. It is forbidden under effects, transitions,
observations, conversation turns, or any nested operational object.

`annotation` is exactly `{id:chronology_ID,role?:string,display?:string,
provenance:[string],value:annotation_value}`. Provenance is author ordered;
exactly one value tag follows:

| tag | exact payload |
|---|---|
| `civil` | `{calendar_id:calendar_ID,year:i64,month?:i64,day?:i64}`; day requires month, month requires year, references exist |
| `era` | `{era_id:era_ID,year:i64,month?:i64,day?:i64}`; one display-date form only; the authored display year is interpreted through that era's `display_epoch` |
| `range` | `{calendar_id:calendar_ID,lower:null|civil,upper:null|civil}`; inclusive endpoints and each non-null endpoint has that calendar; non-null lower is not after non-null upper |
| `approx` | `{display_value:string,bounds:{lower:null|civil,upper:null|civil}}` including null/null; comparable non-null bounds share a display calendar and are ordered lower through upper |
| `conflict` | `{claims:[annotation_value]}` 2..64 in author order; no canonical winner |
| `relative` | `{relation:string,before_id?:stable_ID,after_id?:stable_ID}`; at least one reference to a declared source-record ID only, never an annotation ID |
| `duration` | `{unit:year|month|day,value:i64}`; display-only, no month/year addition and never a CivilDay/AxisDay/StoryTime rewrite |

Annotation value fields never establish truth/canon, causality, horizon, auth,
visibility, state, replay, StoryTime conversion, search rank/corpus/vector, or
thread scope. They cannot resolve conflicts or alter a shared world.

## Current source and delivery boundary

The generic loader, validator, and compiler accept homogeneous `wedl/v0.7` worlds. Their chronology projection accepts validated v0.6 and v0.7 declarations; legacy v0.3/v0.5 worlds have no chronology projection. This extends compiled read-model coverage without rewriting the v0.6 grammar or the local `upgrade-v06` input policy.

The existing public adapter has a narrower advertised capability: `chronology_capability` enables calendar chronology only for v0.6, and the catalogue returns empty definitions for v0.7. `chronology.replace` also requires v0.6 and rejects v0.7 before making a changeset. In contrast, format, convert, search, and story-times obtain the compiled chronology store without that capability gate, so a valid v0.7 projection can be evaluated by those operations. This asymmetry is the current implementation boundary, not a new uniform public v0.7 admission policy. Source references: `src/wedl/chronology_api.py`, `src/wedl/chronology_index.py`, and `src/wedl/authoring.py`.

Executable contract vectors: [chronology-schema-v06.yaml](../../examples/chronology-schema-v06.yaml). Their semantic cases are preserved unchanged.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiZGFmNzhiNTRlYmZhNDBhN2U5NzU3YmI3Y2RiMTE3OWRiNzc5YjdiMyIsImkiOiJzaGEyNTY6SVg2a1ZHTEs5THgyMjNWMXBOSTZPV2hqR3ZSdGtvRll1ZnJRUGRjbkkybyIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4WFdRSk1YQjRBMjVGUjNHREsxS1pZIiwib3AiOiJPMDFNNDhYV1FKTVhCNEEyNUZSM0dESzFLWlkiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpTWmJkbU11NmhlQjZaU0kycnpHVEUzeFRFWl9TUmtGN1RMMFpqekhFMDlVIiwidCI6MTc5MTMwMTE0ODI0NCwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
