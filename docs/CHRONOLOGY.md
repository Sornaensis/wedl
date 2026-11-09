# Calendar chronology

The current v1 implementation supports exact calendar days, partial civil/era
formatting, exact conversion, authored date-to-story anchors, record annotations
and date-search predicates with explicit uncertainty. These are **author/all-time**
source reads. Calendar dates do not change temporal replay, character knowledge,
the world cursor or the meaning of story ticks.

## Declarations

The existing world record may contain `chronology` with all three arrays:
`calendars`, `eras` and `anchors`. Empty arrays are valid; absence of the whole
field means no catalogue. No new capability is needed. These mappings are closed
except for `x-*` extension metadata. The bundled
`examples/chronology_conformance/story/world.md` supplies cycle, table, isolated
calendar and bounded-era examples; its source bytes are unchanged this iteration.

A calendar declares an exact `id`, `label`, ascending unique `months`, a `rule`
and an optional `epoch`. Each month has `number`, `days` and optional `label`.

```yaml
chronology:
  calendars:
    - id: calendar_00000000000000000000000001
      label: Harbor calendar
      months:
        - {number: 1, days: 30, label: First}
        - {number: 2, days: 30, label: Second}
      rule:
        kind: cycle
        period: 4
        overrides:
          - {residue: 0, target_month: 2, delta_days: 1}
      epoch: {civil: {year: 0, month: 1, day: 1}, axis_day: 0}
  eras: []
  anchors: []
```

Calendar, era and chronology anchor IDs have their respective prefix plus 26
Crockford characters. Authored casing is retained and queries use exact IDs;
case-only declaration duplicates are rejected. Labels and era aliases do not
resolve query selectors.

| Rule | Meaning |
| --- | --- |
| Cycle | `period` is 1–500; a change applies when the year modulo the period equals its `residue`, including negative years |
| Table | `years` contains ascending unique `{year, overrides}` rows; only those years are available |
| Month delta | `{target_month, delta_days}` changes an existing base month's length |
| Intercalary month | `{intercalary_month: {number, days, label?}}` inserts a new numbered month; cycle changes also include `residue` |
| Epoch | `{civil: {year, month, day}, axis_day}` identifies one exact day on the shared axis |

Base months contain 1–64 rows; month numbers are 1–64, and effective lengths are
1–4096 days. Rules reject duplicate applicable changes and excessive mechanics.
Table years are bounded to 500 rows, with at most 500 base/rule/change entries
per calendar. Each declaration array is bounded to 500 entries. Table rows are
concatenated in their declared year order: omitted year labels contribute no
invented days and querying such a label returns `table_gap`.

Years and axis days are signed i64 integers. Source coordinates must be YAML
integers, while public JSON coordinates are decimal strings. Calculations use
checked i128 intermediates and never iterate across a year gap. An axis result
outside i64 is an error for conversion/anchor mapping. Local date formatting
still succeeds with `axisDay:null` and `axisReason:"overflow"`. Omitting an epoch,
or declaring `epoch:null`, permits local formatting with `axisReason:"no_epoch"`.
No epoch is inferred from the calendar name or another calendar.

## Eras and anchors

An era has `id`, exact `calendar_id`, `label`, `aliases`, `display_year_zero`,
`display_epoch: {display_year, machine_year}` and nonempty `provenance`. Optional
`bounds` contain inclusive exact `lower` and `upper` civil days. An era can skip
display year zero; its epoch and queries must respect that rule and its bounds.
The `date` command takes an exact civil date and optionally renders its year in
an era. `format` accepts a civil or era input, including partial years/months.
`convert-date` converts an exact civil or era input to a calendar or era target.
The concise `convert` command takes two calendars and one exact civil day.

An anchor has `id`, signed `axis_day`, explicit
`story_time: {timeline, tick, order}` and nonempty `provenance`. Within one
timeline, days and story positions must increase together. One day cannot name
different positions and one position cannot name different days. Different IDs
for the identical pair coalesce, retaining sorted anchor IDs and unique
provenance. Different timelines are independent; the same day may have an
explicit anchor in each. This follows Rust's existing incomparable-timeline
rule rather than imposing an order on timeline names.

An anchor query matches exactly, including `order`. Unanchored dates or story
positions return `found:false, reason:"no_match"`; no interpolation,
nearest-match search or extrapolation occurs. `from-story` defaults order to
**0**, not the end-of-tick horizon used by temporal replay commands.

## Exact day queries

```sh
wedl-rs --repo examples/chronology_conformance chronology calendars
wedl-rs --repo examples/chronology_conformance chronology date calendar_0123456789abcdefghjkmnpqrs --year -4 --month 2 --day 29
wedl-rs --repo examples/chronology_conformance chronology convert calendar_0123456789abcdefghjkmnpqrs calendar_1123456789abcdefghjkmnpqrs --year 0 --month 2 --day 4
wedl-rs --repo examples/chronology_conformance chronology to-story calendar_0123456789abcdefghjkmnpqrs --year 0 --month 1 --day 1
wedl-rs --repo examples/chronology_conformance chronology from-story calendar_0123456789abcdefghjkmnpqrs --tick 0 --order 0
```

`date` also accepts `--era ERA_ID`; both anchor operations accept `--timeline`.
Calendar listing accepts `--limit` in 1–1000 and fails instead of truncating a
larger catalogue. Query integers use canonical decimal spelling: no `+1`, `01`,
`-0`, floating-point spelling or whitespace. The browser keeps them as strings.
Canonical formatted dates include the exact calendar or era ID and coordinates;
this is not a localized date renderer.

Conversion returns `found:false` for `no_epoch`, `table_gap` or
`disconnected_table` when no target day is available. Unknown IDs, impossible
month/day combinations and out-of-range arithmetic fail explicitly. Successful
reads include source citations and scope `author-all-time-chronology-source`.
The CLI, GET API and browser Chronology tab use the same library operations.

## Record annotations

Any non-world record may declare a `chronology` array. Each annotation requires
an `id`, nonempty `provenance` array and a `value` containing exactly one tag.
Optional `role` and `display` are nonblank strings. Annotation IDs are unique
within their owner, including case-only duplicates; different records may reuse
the same annotation ID. Record status is shown in author results and does not
silently suppress annotated source.

```yaml
chronology:
  - id: chronology_00000000000000000000000001
    role: arrival
    value:
      civil: {calendar_id: calendar_00000000000000000000000001, year: 0, month: 2}
    provenance: [harbor-ledger]
```

| Tag | Payload and meaning |
| --- | --- |
| `civil` | Exact `calendar_id`, signed `year`, optional `month` and `day`; day requires month |
| `era` | Exact `era_id`, signed display `year`, optional `month` and `day`; the entire represented interval must fit the era's bounds |
| `range` | Exact `calendar_id`, `lower` and `upper`; each endpoint is a civil value or null for an open end |
| `approx` | `display_value` and `bounds: {lower, upper}`; endpoints are civil values or null |
| `conflict` | `claims` containing 2–64 tagged values, including nested conflicts; every claim validates but no winner is selected |
| `relative` | A nonblank `relation` plus `before_id`, `after_id` or both, naming exact existing record IDs; no target dates are inferred |
| `duration` | Signed i64 `value` and `unit` of `year`, `month` or `day`; display-only, with no automatic calendar arithmetic |

Partial civil/era dates expand to the first and last valid days of that year or
month. Range lower endpoints use their first possible day and upper endpoints
their last possible day. Endpoints are inclusive. An open range uses null as an
unbounded end; it is not a missing-value default. Bounds must share the same
calendar and cannot be reversed. Finite approximations retain their uncertainty;
one-sided or entirely unbounded approximations remain noncomparable.

Calendars with epochs use the shared `axis` basis. An isolated calendar uses
`calendar:CALENDAR_ID` and can be compared only with that same isolated calendar.
Normalized bounds must fit signed i64, including local isolated-calendar
ordinals. Unlike a display-only `date` command, a source annotation that cannot
normalize because of overflow or a missing table year is invalid.

There are at most 500 annotations per record, 64 nested conflict levels and
4096 tagged value nodes per record. Executable shapes are closed except for
opaque `x-*` metadata. Extensions do not supply references or appear in query
results. Chronology belongs on the source record. Known structural objects such
as story positions, effects, transitions, turns and initialization payloads
cannot contain it; validation reports `WDL-CHRON-003` at the actual nested field.
State-key namespaces, metric names, scalar environment conditions, lore profile
field names, typed lore `values` and opaque `x-*` metadata may use the word
`chronology` as ordinary data. A profile-declared `values.chronology` field is
validated by its value type and does not become an annotation. Checks follow
declared paths, not arbitrary recursive key matching. New domain schemas must
extend their structural validation as they are implemented.

## Listing and comparing annotations

```sh
wedl-rs --repo examples/chronology_conformance chronology annotations "Yearly Annals"
wedl-rs --repo examples/chronology_conformance chronology annotations --calendar calendar_0123456789abcdefghjkmnpqrs
wedl-rs --repo examples/chronology_conformance chronology compare "Yearly Annals" chronology_C123456789abcdefghjkmnpqrs "Yearly Annals" chronology_D123456789abcdefghjkmnpqrs
```

The list accepts an optional record selector, `--calendar`, `--era` and `--limit`
(1–1000). Record selectors accept IDs, titles or aliases; calendar, era and
annotation selectors are exact IDs. Calendar/era filters match declared
references anywhere in the value, including conflict branches and an era's
calendar. They do not follow relative record references. Combined filters all
apply. Unknown selectors fail; a selected non-world record without annotations
returns an empty list. Select the world through `calendars`, not `annotations`.

Results are ordered by record ID, then annotation ID. Each result includes
record status, provenance, the authored value, normalized bounds/precision,
comparability and source citation. `sourceOrdinal` preserves the original array
index even when display order differs. Limits apply after filters and fail
instead of truncating. Dates, durations, bounds and ordinals remain decimal
strings in JSON.

Pair comparison describes the relation between the represented intervals:

| Result | Requirement |
| --- | --- |
| `before` | Left finite upper bound is strictly below right finite lower bound |
| `after` | Left finite lower bound is strictly above right finite upper bound |
| `equal` | Both nonapproximate values represent the same single day, including a singleton range |
| `overlaps` | Inclusive intervals intersect; identical wider ranges also return overlaps |
| `unknown` | No common basis or the authored value cannot establish that relation |

Finite approximations can establish overlap only. Even when disjoint they
return `unknown` with `approximate_only`, and never return before, after or
equal. Conflicts remain unknown with `conflicting_claims`, even when all claims
happen to agree. Relative references and durations remain unknown with their
respective reasons. A comparison never resolves an uncertainty into a fact.

The matching routes are GET `/api/chronology/annotations` with optional
`record`, `calendar`, `era`, `limit`, and `/api/chronology/compare` with required
`left_record`, `left_id`, `right_record`, `right_id`. Extra/horizon/character
parameters are rejected. The browser supplies list/filter and comparison forms.

## Civil and era query inputs

Four commands share one typed date-input contract: `format`, `convert-date`,
`search` and `story-times`. Select exactly one of `--calendar CALENDAR_ID` or
`--era ERA_ID`, then supply `--year` and optional `--month` and `--day`. A day
requires a month. Omitted components retain year/month precision; they never
silently become day 1. All selectors use exact authored IDs.

```sh
wedl-rs --repo examples/chronology_conformance chronology format --era era_0123456789abcdefghjkmnpqrs --year 0 --month 2
wedl-rs --repo examples/chronology_conformance chronology convert-date --era era_0123456789abcdefghjkmnpqrs --year 0 --month 2 --day 4 --target-calendar calendar_1123456789abcdefghjkmnpqrs
wedl-rs --repo examples/chronology_conformance chronology convert-date --calendar calendar_1123456789abcdefghjkmnpqrs --year 0 --month 13 --day 5 --target-era era_0123456789abcdefghjkmnpqrs
```

Era input years use the declared display epoch and skip zero when configured.
The whole represented year/month/day must fit the era's inclusive bounds.
`format` returns the authored precision, corresponding civil interval and
canonical display string. Local formatting remains available without an epoch,
or when the shared day axis overflows: `axisInterval` and `axisDay` are null
with the appropriate `axisReason`. A partial date has no single `axisDay`.

`convert-date` requires exactly one `--target-calendar` or `--target-era`.
Conversion requires an explicitly complete input day and declared epochs, even
when a partial year or month happens to contain only one day. Partial input
returns `found:false, reason:"conversion_exactness"`. Successful target-era
conversion also checks the target's bounds and zero-skipping year convention.
Table gaps and absent epochs remain explicit unavailable results. Invalid
dates, unknown IDs, out-of-bounds era dates and overflow fail.

### Advanced typed values

Use `--value JSON` in place of all ordinary input flags. The same JSON is
accepted in the URL-encoded HTTP `value` parameter. Objects are closed, duplicate
fields are rejected and every numeric coordinate is a canonical **decimal
string**. Numeric JSON values are refused, including small years and months.
Supported tags are `civil`, `era`, `range`, `approx` and `conflict`:

```json
{"civil":{"calendar_id":"calendar_0123456789abcdefghjkmnpqrs","year":"0","month":"2"}}
```

```json
{"era":{"era_id":"era_0123456789abcdefghjkmnpqrs","year":"0"}}
```

```json
{"range":{"calendar_id":"calendar_0123456789abcdefghjkmnpqrs","lower":null,"upper":{"calendar_id":"calendar_0123456789abcdefghjkmnpqrs","year":"0"}}}
```

Range endpoints are untagged civil objects or null and must use the range's
calendar. An approximation is `{"approx":{"display_value":"about then",
"bounds":RANGE_PAYLOAD}}`, where `RANGE_PAYLOAD` includes `calendar_id`, `lower`
and `upper` as above. Query approximation bounds explicitly identify their
calendar even if both ends are open. A conflict is
`{"conflict":{"claims":[VALUE,VALUE]}}`, with 2–64 independently validated
tagged values. No claim is selected as the answer. Relative references and
durations are authored annotations, not query date operands.

`format` and `convert-date` accept only civil or era values. Search and interval
anchor queries accept all five tags, then retain the unavailable outcomes for
values that cannot support the requested relation. Input text is bounded to
1 MiB; typed inputs enforce at most 64 nested levels and 4096 value nodes. The
standard JSON decoder additionally has a 128-container recursion ceiling:
objects and arrays each count, so deeply wrapped conflicts reach this transport
limit sooner than the typed limit. The GET API also caps the complete encoded
query string at 16,384 bytes.

## Searching annotations by date

```sh
wedl-rs --repo examples/chronology_conformance chronology search on_date --calendar calendar_0123456789abcdefghjkmnpqrs --year 0 --month 2 --day 29
wedl-rs --repo examples/chronology_conformance chronology search before --era era_0123456789abcdefghjkmnpqrs --year 0
wedl-rs --repo examples/chronology_conformance chronology search between --calendar calendar_0123456789abcdefghjkmnpqrs --year 2 --upper-year 3
wedl-rs --repo examples/chronology_conformance chronology search overlaps --calendar calendar_0123456789abcdefghjkmnpqrs --year 2 --era-filter era_0123456789abcdefghjkmnpqrs --era-mode overlaps_bounds
```

All date bounds are inclusive and must share a comparable basis. Epoch
calendars share `axis`; the same isolated calendar can be searched on its local
ordinal basis. Different isolated calendars never acquire an implied alignment.

| Predicate | Match rule |
| --- | --- |
| `on_date` | Annotation and query intervals intersect; relation `exact` means identical interval bounds, otherwise `overlaps` |
| `overlaps` | Inclusive intervals intersect; finite approximate claims may participate |
| `before` | Annotation has a finite upper bound strictly below the query's finite lower bound |
| `after` | Annotation has a finite lower bound strictly above the query's finite upper bound |
| `between` | Annotation intersects the inclusive span from the first operand's lower bound through the second operand's upper bound |

The `exact` label in an on-date search describes equal represented intervals;
it does not promote a partial month/year into an exact historical day. Stored
pair comparison retains its stricter `equal` requirement of certain singleton
days. Open ranges are searchable, but before/after require the relevant query
boundary. Between requires two finite, nonapproximate operands on one basis and
rejects reversed bounds.

For `between`, supply `--upper-year` with optional `--upper-month` and
`--upper-day`. The upper operand inherits the input calendar or era unless
`--upper-calendar` or `--upper-era` explicitly selects another. Advanced upper
values use `--upper-value JSON`. A complex first operand requires an explicit
calendar/era if the upper operand uses ordinary flags. Other search predicates
forbid an upper operand.

Optional filters are `--record`, `--calendar-filter`, `--era-filter`,
`--era-mode` and `--limit` (1–1000). Owner selection accepts an ID/title/alias;
calendar and era selectors are exact IDs. Calendar filters match explicit
references. Era mode defaults to `authored`, selecting explicitly referenced
era claims. `overlaps_bounds` instead requires an independent overlap with the
era's calendar interval, on the same basis. It does not clip the date predicate
to the era: a sufficiently wide range can overlap disjoint era and query
windows. Unbounded eras still require a shared basis. Explicit `--era-mode`/HTTP
`era_mode` requires an era filter; without a filter the default authored mode
adds no selection. All selectors validate before scanning annotations.

Finite approximate annotations are included only by `overlaps`, with an
`approximate-overlap-included` advisory. They are excluded from other predicates
even if their possible bounds intersect. An approximate query operand likewise
supports only `overlaps`. Open approximations, conflicts, relative references
and durations supply no confirmed matches. Advisories count excluded uncertain
claims in the selected scope without following record references or choosing
conflict branches. An unavailable query returns `available:false`, an explicit
reason and `complete:false`, rather than a successful empty search. Missing
table years return `table_gap`.

Successful results have `available:true, complete:true`, the normalized query,
stable record-ID/annotation-ID ordering, source ordinals, provenance and
citations. Limits apply after predicates and filters; exceeding one fails the
operation instead of returning a partial result. Queries read the current source
snapshot without an additional chronology cache.

## Interval-to-story anchors

```sh
wedl-rs --repo examples/chronology_conformance chronology story-times --calendar calendar_0123456789abcdefghjkmnpqrs --year 0
wedl-rs --repo examples/chronology_conformance chronology story-times --calendar calendar_0123456789abcdefghjkmnpqrs --year 0 --upper-year 7 --timeline main
```

`story-times` returns only authored anchors whose axis days are inside the
inclusive interval. It accepts partial civil/era dates, open ranges or two
finite endpoints using the same upper flags as `between`. Optional `--timeline`
selects one declared timeline. With no timeline selected it returns all authored
timeline matches; the exact-day `to-story` command retains its default-timeline
behavior. Display sorting by timeline/tick/order establishes no chronology
between separate timelines.

Available results report `mapping:"none"`, `"unique"` or `"ambiguous"` from the
number of distinct anchor positions, plus `count`, `complete:true` and the
matching axis days, story positions, coalesced anchor IDs, provenance and source
citations. Zero matches return `found:false, reason:"no_match"`. Multiple
positions are all shown; no nearest anchor is chosen. The optional limit is
1–1000 and fails if complete matches exceed it.

Approximations never establish a story anchor, even when their finite bounds
contain one. They return `approximate_only`; conflicts return
`conflicting_claims`. Missing epochs and table years remain unavailable. These
results have `available:false, complete:false, mapping:null`, distinguishing an
unresolved date from an available interval with no explicit anchor. No query
interpolates, creates an event or changes a timeline's cursor.

## HTTP and browser date queries

GET `/api/chronology/format`, `/convert-date`, `/search` and `/story-times` use
the same inputs and outcomes. Ordinary input fields are `calendar` or `era`,
`year`, optional `month`, `day`; advanced input is `value`. Conversion adds
`target_calendar` or `target_era`. Search adds required `predicate` and optional
`record`, `calendar_filter`, `era_filter`, `era_mode`, `limit`. Search/anchor
upper fields are `upper_year`, `upper_month`, `upper_day`, `upper_calendar`,
`upper_era`, or `upper_value`; interval anchors also accept `timeline` and
`limit`. All four full paths begin `/api/chronology/`. Unknown or duplicate
parameters, mixed input forms and character/horizon parameters fail explicitly.

The browser Chronology tab includes a folded date-query form for these four
operations. It offers civil/era or advanced JSON input, optional upper endpoint,
conversion target, search filters and timeline selection. Date coordinates stay
strings throughout the browser and API. All responses retain author/all-time
scope; none is a character knowledge view.

## Editing and coverage

`record update WORLD --file REQUEST --expected-fingerprint HASH` accepts
`kind: world` plus a complete replacement `chronology` mapping. Preview and
apply use the existing single-record writer and candidate hash. Omitted world
metadata and prose remain intact; semantically invalid declarations appear as
`canApply:false` previews and cannot save. Clear declarations with all three
empty arrays. `chronology:null` is not a deletion operation. See runnable
[request 19](../examples/authoring/19-world-chronology.yaml).

All twenty-seven non-world record kinds other than hypothesis, including lore concepts,
existence episodes and context briefs, accept
a replacement annotation array in `record create` and `record update`.
Omission retains it; `chronology: []`
clears it, while `chronology:null` is refused. Nulls within supported open bounds
remain data. The same candidate validation prevents world calendar edits from
orphaning or invalidating existing annotations. Unchanged requests retain source
bytes and omitted prose remains intact. [Request 20](../examples/authoring/20-character-chronology.yaml)
demonstrates all seven tags through the existing writer.
Request 133 adds a calendar annotation and independent dated description to an
existence episode. Its operational validity, evidence pointers and historical
targets cannot own annotation arrays; only the episode's root may do so.

Annotations remain author-only. Valid annotation edits leave complete character
context, search and transcript responses unchanged. The removed annotation
coverage warning does not make overall validation complete: it still reports
`complete:false` for the wider project. The planned chronology date-query slice
is implemented, while chronology admission into character/reader views awaits
those domains' explicit evidence rules. Domain-specific namespace checks must
remain aligned with new schemas. Existing core replay is unchanged.
