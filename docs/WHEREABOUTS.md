# Character whereabouts

`navigation::whereabouts::whereabouts(world, at, characters, limit)` is a
read-only bulk author view. `characters` is absent for the whole cohort or an
array of 1–32 distinct exact character IDs. Unknown, unavailable and wrong-kind
selections are refused; names and aliases are not selectors. Every coordinate
uses the existing declared timeline and exact signed `tick`/`order` values.

## Location and presence

The cohort contains all canonical and retired characters admitted by the full
local-author audience/view scope. A location is supported by an ordinary,
unbound scalar entity state key named `location`, or by that key's explicitly
bound object profile. In the latter case the profile's root `entity` field must
be an entity type admitting locations and its value must be the exact
`{kind: record, id: LOCATION_ID}` reference. Either form requires an independently
admitted location record. Literal strings, Ref-shaped literal objects, sibling
or nested profile fields, prose, scene presence and place links never establish
placement or infer travel.

Initial state and admitted canonical event effects produce complete journeys in
exact time, event-ID and authored effect order:

| Journey entry | Meaning |
| --- | --- |
| `initial` | Explicit admitted initial location; no dated event is invented. |
| `move` | A `set` changes the operative location, including to unknown when an optional leaf is absent. |
| `reaffirmation` | A `set` repeats the operative location, including an already absent leaf. |
| `clear` | An explicit `clear` removes the current location. |

Each entry retains its source citation, including the original effect ordinal.
`lastKnownLocation` is returned only while current location is absent. A hidden
location operand is withheld before replay; it neither becomes a location label
nor changes the projected journey. Future and other-timeline effects are omitted.
An optional operative profile leaf can be absent. An explicit set to a valid
profile value without that leaf replaces the prior state and makes the current
location unknown while retaining the last-known location. A sibling destination
never fills the gap. Generic state reads continue to omit bound profile values
and citations; this navigation consumer does not change scene-control writers.

`activeScenes` means active **at the requested horizon**. Active and closed
scenes use the existing inclusive scene and participant intervals; an open
active scene ends at its authored cursor for this purpose. Planned/abandoned
scenes confer no presence. An unlocated character stays `unlocated` even when
scene presence is explicit; otherwise presence is `active-scene` or `offstage`.
All directly authored scene memberships remain in the scene groups, while a
character's single `activeScene` uses deterministic title/ID order.

Character entries, current-location groups, active-scene groups and the separate
offstage/unlocated lists are ordered by title then ID. A selected read contains
only selected characters in every group and omits empty groups. Every named link
is admitted independently; an unavailable scene location is null.

## Relative prominence

Prominence is a calculated navigation aid, not authored canon. Compute it for
the **complete admitted canonical/retired cohort before selection**. Selecting
one character must not change that character's score or normalization maxima.

| Signal | Evidence through the horizon | Weight |
| --- | --- | ---: |
| Scenes | Active/closed scene appearances from the authored entrance onward; exit does not erase earlier evidence | 40 |
| Point of view | The explicitly marked subset of those appearances | 25 |
| Events | Canonical event participation or admitted effect targeting, counted once per character per event | 20 |
| Relationship neighbors | Distinct admitted endpoints joined by canonical relationship history; both directions count as one neighbor | 15 |

Each signal is normalized as `log1p(raw) / log1p(cohort maximum)`; a zero maximum
contributes zero. Weighted contributions are rounded to two decimal places, then
summed and rounded. Normalized values are reported to six decimals. Relationships
and all other signals remain local to the selected timeline. Knowledge, body
prose, tags and inferred co-presence contribute nothing. An unbound scalar entity
effect with an unavailable or future endpoint contributes no effect evidence.

## Bounds and preservation

`limit` must be 1–1,000. It caps returned characters, the **total** returned journey
entries, location groups and scene groups separately. Results are complete or
refused; nothing is silently truncated. The full unfiltered admitted cohort is
capped at 1,000, admitted event capture and total contributing evidence each at
10,000. These safety bounds are not evaded by selecting one character. The final
response is limited to 4 MiB of encoded JSON, checked by streaming serialization
before allocating its JSON value.

The query constructs no cache, private store, source write, state transition,
belief or new event. Existing source bytes and modification times are untouched.
The interface retains the native `wedl-rust/v1` envelope and decimal-string time
encoding. It follows the confirmed whereabouts and importance behavior in pinned
Python `query.py` at `f0e885ccebc279eff65a5db2984d01ebc6647b2b`, using the existing
native scalar placement, exact operative profile leaf and admission rules.
