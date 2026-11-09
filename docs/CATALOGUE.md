# Admitted catalogue

The catalogue reads pages or explicitly requested record labels from one captured
source world. A request pins the exact source fingerprint and selects an author
or character view. That view supplies no new knowledge, scene access or reader
authorization. Source Browse remains the separate full author inventory.

## Interfaces

| Interface | Page | Requested labels |
| --- | --- | --- |
| Native library | `catalogue::read(world, &catalogue::decode(raw)?)` | Same decoder and read function |
| CLI | `wedl-rs --repo WORLD catalogue page --file page.json` | `wedl-rs --repo WORLD catalogue labels --file labels.json` |
| GET API | `/api/catalogue/page?request=JSON` | `/api/catalogue/labels?request=JSON` |
| Browser | Catalogue → Browse pages | Catalogue → Requested record labels |

The CLI reads standard input when `--file` is omitted or is `-`. HTTP percent
encodes the complete JSON as the single `request` parameter. The command or route
must match the JSON `operation`. CLI and HTTP wrap the catalogue result in the
existing `wedl-rust/v1` response envelope.

Take `fingerprint` from the selected source's `status` response. The same native
contract supports working files and an explicitly selected immutable Git
revision. A changed fingerprint refuses the read; refresh the source pin and
start from `cursor: null`. Catalogue reads do not compile or modify source.

## Closed requests

Every shown member is required, including explicit `null` values and empty
arrays. Unknown members, duplicate object keys and duplicate selections are
invalid. Replace `FINGERPRINT_FROM_STATUS` below with the exact 64-character
lowercase hexadecimal source fingerprint.

An all-time author page:

```json
{
  "protocol": "wedl-core-catalogue/v1",
  "operation": "page",
  "fingerprint": "FINGERPRINT_FROM_STATUS",
  "at": null,
  "lens": {"kind": "author-all-time", "timeline": "main"},
  "filters": {"kinds": ["character"], "profiles": [], "eras": [], "name": ""},
  "sort": "name-asc",
  "q": null,
  "searchMode": null,
  "threadIds": [],
  "limit": 25,
  "cursor": null
}
```

Requested labels for an exact character in an exact scene:

```json
{
  "protocol": "wedl-core-catalogue/v1",
  "operation": "labels",
  "fingerprint": "FINGERPRINT_FROM_STATUS",
  "at": {"timeline": "main", "tick": "9007199254740993", "order": "7"},
  "lens": {
    "kind": "character",
    "characterId": "char_00000000000000000000000001",
    "sceneId": "scene_00000000000000000000000001"
  },
  "targets": [
    {"kind": "record", "id": "char_00000000000000000000000001"},
    {"kind": "record", "id": "loc_00000000000000000000000001"}
  ]
}
```

The sample IDs must identify records in the selected world. Label results
preserve the requested order. Each target has `status: "available"` with its
admitted `kind` and `title`, or `status: "unavailable"` with both fields `null`.
Unknown and withheld targets do not reveal distinct reasons. An empty target
array is valid. Labels have no filters, sort, search or continuation members.

### Views and time

| Lens | Required `at` | Admission |
| --- | --- | --- |
| `{"kind":"author-all-time","timeline":"main"}` | `null` | Full-author admission across the selected timeline's story time. |
| `{"kind":"author-as-of"}` | Exact timeline, tick and order | Full-author admission at that inclusive horizon. |
| `{"kind":"character","characterId":"…","sceneId":"…"}` | Exact timeline, tick and order | Native character knowledge plus independently admitted access through actual presence in the selected scene. |

Both IDs in a character lens are exact IDs, not titles or aliases. The character
must actually be present in the selected scene at the requested point. Merely
supplying a scene ID grants nothing.

Time coordinates are canonical decimal **strings**: signed 64-bit `tick` and
signed 32-bit `order`. Leading zeros, `+`, `-0`, numeric JSON values and omitted
orders are invalid. Timelines must be declared. Ticks remain ordinal positions,
not durations.

## Selection and metadata

Admission precedes metadata matching, search, facets and pagination. Hypotheses
and context briefs are excluded. Available rows carry their admitted identity,
current profile ID when applicable, admitted era associations, selected thread
memberships and original-source citations. A visible record's reference to
another record does not automatically admit that record or its title.

Ordinary metadata has explicit temporal boundaries in addition to the native
owner admission rules:

| Owner | Metadata time boundary |
| --- | --- |
| Event | At or after its authored event time. |
| Closed scene or conversation | At or after its authored end. |
| Open scene or conversation | Undated metadata; this does not admit the unfinished aggregate's prose. |
| Environment | Its start through optional end, inclusive. |
| Other ordinary record with timed direct or lifecycle transitions | At or after the earliest transition. |
| Other ordinary record | Timeless metadata. |

An explicit boundary on another timeline excludes the row. Typed concepts,
assertions, existence and learned accounts retain their separate native validity
and admission rules. Era associations come from typed chronology annotation
values on admitted owners, deduplicated per record; provenance strings and
arbitrary extension data do not add era memberships.
All-time views retain finite and expired historical evidence on their selected
timeline. Dated views apply the inclusive boundaries above.

Character typed-belief cards use the native learned statement, labels and
transition citation. Their author heading, aliases and owner prose do not replace
the remembered account. These cards participate in requested labels and explicit
search; ordinary page browsing does not turn them into canonical facts.
Selected-thread filtering applies to the admitted knowledge record's own
memberships.

Each nonempty filter array matches any of its entries; the four filter fields
combine by intersection. `kinds`, `profiles` and `eras` are exact names or IDs.
Profiles use the current native profile ID string. `name` matches normalized,
case-folded ID, admitted title or admitted aliases, including their slug form.
An empty filter leaves that dimension unrestricted within the admitted view.

`threadIds` is a sorted, unique array of declared thread IDs. A nonempty selection
keeps records in any selected thread and returns only those selected memberships.
Thread grouping is applied after search ranking; it does not alter the admitted
search corpus or scores and cannot grant character access.

### Sorts

| Sort | Ordering |
| --- | --- |
| `kind-name` | Kind, then normalized name, original normalized title and stable ID. |
| `name-asc` | Normalized name, normalized title and stable ID, ascending. |
| `name-desc` | The complete name key in descending order. |
| `prominence-high` / `prominence-low` | Character prominence in the requested direction; unscored records last, with ascending name ties. Author views only. |
| `relevance` | Search score descending, then stable document identity. Requires a search query. |

The `nfc-casefold-scalar-v1` collation uses Unicode NFC normalization and full
case folding; original titles are preserved in returned rows. Prominence uses
the complete admitted character cohort before page filters. Dated author reads
use their explicit horizon. All-time author reads use the world's shared current
point, reported as `context.prominenceAt` and `context.prominenceOrigin`.
Scoring reuses [whereabouts](WHEREABOUTS.md) prominence without constructing
location journeys; the catalogue's complete cohort bound is 4,096 characters.

## Search and continuation

Set both `q` and `searchMode`, or set both to `null`. Search modes are native
`lexical`, `semantic` and `hybrid`; see [RETRIEVAL.md](RETRIEVAL.md) for the shared
lexical, local LSA and reciprocal-rank-fusion semantics. Search operates on
admitted material selected by the catalogue filters. Each returned record has
one best matching document, including its optional heading, snippet, score, retrieval
lanes and exact citation. `searchState` reports the native request-local search
provenance. Search coverage is `complete-admitted-search`; ordinary browsing has
coverage `selection`.

Character descriptions cite actual section targets. Observations, speech,
actions and recollections cite their owner record plus an exact `field`, such as
`observations[1]`, `turns[2]` or `recollections[1]`. These indexes refer to the
original source arrays, before admission or sorting; they are not description
section IDs. Learned beliefs keep their learned citations. This source-field
metadata does not enter ordinary character context, transcript or search output.

Author search combines admitted identity/prose, descriptions and applicable
native typed or structured evidence. A future or hidden body, section, value or
effect cannot enter through an admitted owner's title. Open scene and conversation
aggregate bodies remain excluded even in all-time author search. Character prose
comes from its admitted learned, observed and described evidence. Character
semantic and hybrid reads require the existing explicit public, timeless-description
prior; they do not fit from private author prose or fall back to an author model.

`total` and the kind/profile/era `facets` describe the complete selected result
after search and thread grouping, before pagination. `limit` controls records
per page. It does not truncate the total or weaken a complete-selection bound.

Continue by copying `nextCursor` unchanged into the same request's `cursor`.
The opaque continuation binds the source pin, lens, exact horizon, filters,
search, sort, selected threads, page size, admitted results and effective search
state. Changing any part requires a new first-page request. A cursor neither
grants admission nor chooses a source. `nextCursor: null` means that no further
page exists; repeated independent first-page reads do not advance one another.

## Browser behavior

Catalogue controls use explicit GET reads. All-time mode uses the shared timeline
control and disables tick/order; dated author and character modes use all three
coordinates exactly. Character and scene fields accept exact IDs without loading
an author-title chooser. Advanced filters accept one entry per line, reject
duplicates, and sort selected threads before submission.

Read first page, Next page and Reset continuation are separate actions. The
result shows page counts and the complete returned JSON. Requested labels retain
their own operation and order. Changing any selector, horizon or tab clears the
result and continuation. Source refresh also clears them while retaining entered
selectors. Delayed responses cannot restore stale results or clear a newer
request's pending state. Late bootstrap retains an already edited or used story
horizon.

## Bounds and native adaptations

| Bound | Maximum |
| --- | ---: |
| Complete decoded request | 16 KiB |
| Encoded GET query | 49,160 bytes |
| Each text selector | 256 Unicode scalar values and 1,024 UTF-8 bytes |
| Search query | 64 whitespace-separated words |
| Kinds, profiles or eras per filter | 8 each |
| Selected threads | 32 |
| Requested label targets | 64 |
| Records per page | 1–100 |
| Complete metadata selection | 4,096 records |
| Complete prominence cohort | 4,096 characters |
| Admitted events and contributing prominence evidence | 10,000 each |
| Accumulated selected metadata | 8 MiB |
| Complete streamed cursor proof | 8 MiB |
| Retrieval inputs, including training texts | 1,024 |
| Counted search text, including query and training | 512 KiB |
| Distinct facets | 64 per kind/profile/era dimension |
| Cursor | 4 KiB |
| Complete response | 256 KiB |

Exceeding a complete bound refuses the read without partial items or a usable
continuation. The ordinary native error envelope carries a `CAT_*` diagnostic;
an unavailable scope does not include hidden source labels.

Search charges candidate keys, titles, text and separators before ranking.
Semantic modes also retain the shared [retrieval bounds](RETRIEVAL.md), including
training, vocabulary and numerical work limits. Limits apply after admission;
page size and selected-thread grouping cannot reduce the corpus used for ranking.

The functional reference is the pinned
[core catalogue implementation](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/core_catalogue.py).
The Rust contract pins captured source fingerprints and uses native profile IDs,
source/learned citations and local retrieval. It does not require a compiled
Python catalogue or replace existing source, character or reader contracts.
