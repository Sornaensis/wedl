# Typed lore profiles, concepts, links and bound state

Structured lore requires the world capability `lore-core-v1`. A world declares
profile schemas; each `lore-concept` selects one exact profile ID and supplies
typed values. Records may also carry explicit outgoing `concept_links`, and
world `state_profile_bindings` govern explicitly authored state objects.
Concepts and links describe authored facts without granting character knowledge,
traits, membership or authority. Bound state changes only through its authored
initial value and explicit event effects.

Source remains `wedl/v1`. Profile definitions are edited directly and every
dependent concept is checked against the resulting candidate world.

## Profile declarations

Only the world record may declare `profiles`. Omission means an empty registry;
an explicit value must be an array. Each entry contains exactly `id`, `label`
and `fields`:

```yaml
capabilities: [lore-core-v1]
profiles:
  - id: harbor-custom
    label: Harbor custom
    fields:
      type: object
      required:
        description: {type: string}
        amount: {type: number}
        gathering_place: {type: entity, kinds: [location]}
        origin_story: {type: 'null'}
      optional:
        notes: {type: string}
```

Profile IDs are unique ASCII tokens matching `[a-z][a-z0-9-]{0,63}`. Labels are
nonblank strings of at most 256 Unicode characters. `fields` must be an object
type. Profile IDs have no title/alias lookup or implicit fallback.

### Eight closed type forms

Every descriptor contains exactly the fields shown below. Nested descriptors
use the same grammar.

| Type declaration | Accepted source value |
| --- | --- |
| `{type: string}` | An untagged YAML string |
| `{type: boolean}` | A YAML boolean |
| `{type: number}` | A finite YAML integer or floating-point number |
| `{type: 'null'}` | YAML null |
| `{type: entity, kinds: [location, character]}` | Exactly `{kind: record, id: EXACT_RECORD_ID}` |
| `{type: enum, values: [true, 1, "1", null]}` | One listed scalar, compared by its scalar type and value |
| `{type: array, items: {type: string}, max_items: 4}` | An ordered array whose members match `items`, with at most `max_items` members |
| `{type: object, required: {name: {type: string}}, optional: {}}` | A mapping containing every required field, any declared optional fields, and no other fields |

Object field names match `[a-z][a-z0-9_]{0,63}`. Both `required` and `optional`
maps must be supplied, even when empty, and their keys must be disjoint.
`max_items` is an exact integer from 0 to 1024; zero permits only an empty array.
An entity's `kinds` is a nonempty array of unique known record kinds, excluding
`hypothesis`.

Optional means that a field may be absent. An absent field, null, an empty string,
an empty array, false and zero remain distinct. Null is accepted only by the null
type or an enum containing null. There are no default values, coercions, schema
references or expressions. In YAML, quote the discriminator `type: 'null'`;
unquoted `type: null` is a null value and is invalid as a type name.

Enums contain a nonempty array of unique untagged scalar alternatives. Booleans,
numbers, strings and null are separate categories, so `true`, `1` and `"1"` may
coexist. Numeric `1` and `1.0` compare equal, as do `0` and `-0.0`, and therefore
cannot occur together as separate alternatives. Source integers retain the YAML
codec's exact signed i64 or unsigned u64 values. Floating-point values use finite
f64 precision. Enum comparison never rounds an integer through f64 first:
the exact integer `9007199254740993` differs from the rounded floating-point
value parsed from `9007199254740993.0`.

### Typed references and bounds

An entity reference must name an existing record whose actual kind is allowed by
its descriptor, using that kind's exact valid ID. Draft or retired targets still
validate as source references. A title, alias or ID-shaped string is never
resolved as an entity reference. This includes strings inside enums, arrays,
objects and Markdown. References are extracted only at declared entity leaves.

Bounded profile data rejects tagged YAML, non-string mapping keys and nonfinite
numbers before typed processing:

| Bound | Maximum |
| --- | --- |
| Nesting depth, with the bounded root at depth 1 | 16 |
| Expanded tree nodes, counting mapping keys and every value occurrence | 16,384 |
| Keys in any one mapping | 256 |
| Members in any one array | 1024 |

The entire `profiles` array shares these budgets, rather than receiving a fresh
budget for each entry. Concept `values`, label arrays, validity objects and the
entire `concept_links` array and `state_profile_bindings` mapping are also bounded.
An array's declared `max_items`
can impose a smaller limit. The 256-key limit applies independently to each
schema mapping and to the actual value mapping; having separate required and
optional maps does not permit a 512-key value.

## Concepts and validity

A concept uses `kind: lore-concept` and a `concept_` ID followed by 26 uppercase
Crockford characters. It requires `domain`, `profile`, `values`, `valid`,
`audience` and `perspectives`, alongside the common record header. For the
profile above, a concept's domain fields could be:

```yaml
domain: harbor.custom
profile: harbor-custom
values:
  description: A gathering described in the harbor ledger.
  amount: 9007199254740993
  gathering_place: {kind: record, id: location:harbor/landing}
  origin_story: null
valid:
  kind: interval
  timeline: main
  start: {timeline: main, tick: 10, order: 0}
  end: {timeline: main, tick: 20, order: 0}
audience: [author]
perspectives: [author]
provenance: Harbor customs ledger
```

The referenced location and timeline must exist. A domain is an exact dotted
identifier of at most 256 characters with at least two nonempty segments. Each
segment uses alphanumeric characters or hyphens and contains an alphanumeric
character. A concept title is nonblank and at most 256 characters. Status is
`canonical`, `draft` or `retired`, defaulting to canonical; every status validates.
Optional `provenance` is an untagged string. Markdown remains opaque and is
limited to 1 MiB per concept.

Both `audience` and `perspectives` are required, sorted, nonempty arrays of unique
nonblank single-line labels, each at most 256 characters. Labels are exact and
case-sensitive. They select author presentation scopes; their spelling grants
no character access. There are no implicit public, parent or wildcard scopes.

Validity has two closed forms:

| Source | Meaning |
| --- | --- |
| `{kind: timeless}` | Applies on every declared timeline |
| `{kind: interval, timeline: main, start: POINT, end: POINT}` | Applies on the named timeline between inclusive bounds; either bound may be omitted |

Every supplied point contains exactly `timeline`, an exact signed i64 integer
`tick`, and an exact signed i32 integer `order`. It uses the interval's declared
timeline. Bounds compare `(tick, order)` and start must not follow end. Omitted
endpoints are unbounded; explicit null endpoints are invalid.

## Explicit concept links

Any ordinary record, including the world and other concepts, may contain an
ordered `concept_links` array. Hypotheses cannot own these links. Each member
contains exactly `relation`, `concept_id` and `valid`:

```yaml
concept_links:
  - relation: associated-with
    concept_id: concept_00000000000000000000000002
    valid: {kind: timeless}
```

The relation follows the same token grammar as a profile ID. The target must be
an exact existing lore-concept ID; its status does not prevent source validation.
Validity uses the same grammar as concepts. Array order and source positions
are retained. Omission means no links; null is invalid and `[]` clears the array.
Links describe only their authored outgoing relation. They do not imply an
inverse relation, membership, trait inheritance or a transfer of knowledge.

## State profile bindings

The world may map existing state keys to exact current profile IDs:

```yaml
state_keys:
  character:
    species_notes: {type: string}
state_profile_bindings:
  character:
    species_notes: character-description-state
```

The referenced profile must already exist and have its required object root.
The state key still needs a valid ordinary declaration. While bound, the profile
replaces that declaration's leaf value type; its `exclusive_group`, if supplied,
still applies. The example's `type: string` therefore does not accept a string
value while the binding is present. There is no new state type or special
placeholder declaration.

Bindings belong only on the world. Their outer keys are known non-world record
kinds excluding hypotheses, each with an existing `state_keys` mapping; each
nonblank inner key names an existing state declaration. Profile references are
exact strings, without versions or alias resolution. Omission means no bindings;
an explicit mapping, including `{}`, requires `lore-core-v1`. Null is invalid.

A bound initial value or `set` effect supplies a complete value conforming to
the profile. Declared nulls, arrays, exact integers and typed references use the
same rules as concept values. `set` replaces the whole object. `clear` removes
the state key and forbids a value; it does not store null. Set-item operations
are invalid for an object-profile-bound key. Initial values and effects of every
event status validate; only canonical events replay, on the exact requested
timeline through the inclusive tick/order horizon.

Bindings describe values explicitly. A field called `location`, `holder` or
`container` inside a bound object supplies no movement or ownership operation.
Likewise, an entity reference to a species concept does not infer traits or
enroll a character in that description. Generic character context/search still
use their separately admitted evidence.

## Author queries

The lore commands have corresponding local GET routes:

| CLI | GET route and operation-specific parameters |
| --- | --- |
| `lore profiles [--limit N]` | `/api/lore/profiles?limit=N` |
| `lore profile ID` | `/api/lore/profile?id=ID` |
| `lore catalog [--domain D] [--limit N]` | `/api/lore/catalog?domain=D&limit=N` |
| `lore concept REFERENCE` | `/api/lore/concept?id=REFERENCE` |
| `lore links REFERENCE [--limit N]` | `/api/lore/links?record=REFERENCE&limit=N` |
| `lore state REFERENCE --tick T` | `/api/lore/state?record=REFERENCE&tick=T` |

`catalog`, `concept` and `links` additionally accept `--audience LABEL`,
`--view LABEL`, `--tick T`, `--order O` and `--timeline NAME`; HTTP uses the same
parameter names without `--`. Audience and view default to `author` and must be
nonblank single-line labels of at most 256 characters. Profile operations reject
scope and time parameters. All route parameter sets are closed.

`state` requires an explicit tick and accepts the same optional order, timeline,
audience and view parameters. It has no all-time mode or result-limit flag.

Supplying order or timeline without tick is an error. With a tick, omitted order
means `2147483647`, the end of that tick, and omitted timeline uses the world's
default. An explicit order selects an exact endpoint. With no tick, the Rust
all-time convention includes matching intervals across **all timelines**; it
does not select a world cursor or an implied current concept. At a horizon,
both concept and link validity must apply on that timeline at the inclusive
`(tick, order)` coordinate.

Before concept lookup, ambiguity checks, counts, references or result limits,
queries select canonical concepts with matching audience, view and validity.
Concept selectors accept exact IDs or unambiguous case-insensitive titles or
aliases within that selection. Hidden concepts do not create ambiguity or
appear through an exact ID. `catalog --domain` is an additional exact domain
filter; it does not remove otherwise visible concept targets from returned
references. Lists are complete or return an error when they exceed `limit`.
The default is 100 and the accepted range is 1–1000; there is no truncation.

Ordinary non-concept records, excluding hypotheses, existence episodes and context briefs, retain the broader author
source boundary: their status and time do not filter them as link owners or
typed-reference targets. For example, a retired location can remain a cited
entity value in an admitted concept. A lore-concept used as an owner or target
must pass the concept filters. Existence episodes require canonical status,
applicable interval, matching own labels and an independently admitted character
endpoint, using the same gate as existence lists and description owners. Hidden
episodes cannot enter concept values, links or profile-bound state. Context
briefs also require their own canonical status, matching labels and applicable
validity before serving as typed targets or link owners. This
distinction does not confer character
evidence or authorize inferred temporal facts about ordinary records.

### Projected values, schemas and citations

A selected concept returns `conceptId`, `title`, `domain`, its exact `profile`,
`valid`, projected `values`, `valueTypes`, `references`, `conceptLinks`, `body`
and a record `citation`, plus `provenance` when authored. Catalogue items have
the same detail shape. The full profile schema appears only in the explicit
`profiles` and `profile` queries.

Projection removes an entity leaf targeting a hidden concept. In an object the
field is omitted; in an array the item is omitted and surviving items retain
their relative order. Empty parent objects and arrays remain. A literal string
that resembles the hidden ID remains literal data. Projected values can therefore
omit a source-required field; source validity is checked before projection and
the response does not fill missing fields with defaults or null.

`valueTypes` describes only the returned values. Scalar leaves have
`{"type":"string"}`, `{"type":"number"}`, `{"type":"boolean"}`,
`{"type":"null"}` or `{"type":"entity"}`. An object has
`{"type":"object","fields":{...}}` for its returned fields, and an array has
`{"type":"array","items":[...]}` with one shape per surviving value. An enum
leaf has `{"type":"enum","scalarType":"number"}` or scalar type `string`,
`boolean` or `null`; it contains no unused enum alternatives.

Authored integers use exact decimal strings in JSON, while floating-point values
remain JSON numbers. This applies to values, profile schemas and authored
validity coordinates. Thus source enum alternatives `1` and `"1"` both encode
as JSON `"1"`. Explicit profile queries disambiguate them by adding the
output-only `enumValueTypes` array beside each enum's `values`, in the same
source order:

```json
{"type":"enum","values":["1","1",true,null],
 "enumValueTypes":["number","string","boolean","null"]}
```

Concept `scalarType` provides the same distinction for its selected enum value.
These annotations are response metadata, not accepted source descriptor fields.
Derived counts and limits remain JSON integers.

Each surviving typed reference reports `sourceField`, `entityId`, actual `kind`,
`title`, `citation` and `targetCitation`. Source fields such as
`values.symbols[2].id` retain their original array positions even when earlier
items were omitted. Link items report `ownerId`, `conceptId`, target `title`,
`relation`, authored `valid`, and both citations. Link citations likewise retain
`concept_links[N]` source positions. Citations identify the record, source path,
blob OID, revision, fingerprint and source section.

The service response retains `protocol`, `fingerprint`, `revision` and `result`.
Lore results identify `perspective: author`, their `basis`, and either
`scope: author-all-time-lore` with `at: null` or `scope: author-as-of-lore` with
the selected point. Scoped operations echo `audience` and `view`; catalogues
also echo `domain`. List results include `items`, `count` and `limit`. Profile
items include the complete current declaration and a world-source citation.
The local browser exposes these same operations.

### Scoped bound-state reads

Ordinary author `state` and `context` omit bound keys and their citations.
Read them explicitly with `lore state` at a story horizon. Source browsing
remains a full author read of raw source; ordinary FTS still indexes titles and
bodies rather than bound values.

The scoped state result uses `basis: authored-bound-state` and returns the owner
`entityId`, `kind` and `title`, plus `state`, `valueTypes`, `profiles`, `citations`
and `references`. `state` contains only present bound keys; `profiles` maps each
present key to its exact profile ID. Values and used shapes share concept
projection: hidden concept, existence-episode and context-brief references disappear, ordinary
authored targets retain their existing availability, integers use decimal strings and enum leaves retain
`scalarType`. Owner resolution applies the same concept visibility before ID,
title or alias lookup.

Each key cites its last initial/set value, or the explicit clear that made it
absent. Initial citations use `initial_state.KEY`; set citations use
`effects[N].value` and retain effect ID/time. Clear citations use `effects[N]`
without inventing a value, shape or profile entry. Surviving references carry
`stateKey`, original `sourceField`, target identity/title/kind and both source
and target citations. Filtering array members does not renumber those source
paths.

The browser adds the same exact-horizon state inspector. The
[common/individual description reader](SPECIES.md) composes separately selected
concepts; a state binding does not choose those profiles or fields automatically.

## Editing and examples

Typed `record create` and `record update` support lore concepts. All 27
non-world kinds other than hypothesis, and world updates, accept the common `concept_links` field.
World updates also accept `profiles`, `predicates`, `state_keys` and `state_profile_bindings`.
Supplied arrays and `values` mappings
replace the complete field; they are not partial merges. `profiles: []` clears
the registry only when the resulting world has no dependent concepts. Null is
invalid for `profiles`, `values` or `concept_links`, while null inside typed
values or enum declarations remains valid where explicitly declared.

Every preview validates the complete candidate world, including existing draft
and retired concepts, initial state and past/future/draft event effects. A changed
or removed profile, declaration or binding must leave all dependents valid.
Removing a binding restores its ordinary state type, so existing profile objects
can prevent that edit from publishing. World maps replace the entire field,
preserving unrelated declarations only when they are included in the request.
A candidate with semantic validation errors remains reviewable and cannot
be applied. Applying requires the matching candidate hash and unchanged source
fingerprint under the ordinary writer workflow in [AUTHORING.md](AUTHORING.md).
Capability declarations remain explicit. An authored field named `chronology` inside a
profile's values is ordinary typed data, not a nested calendar declaration.

Authoring examples 64–71 cover profile creation, two concepts with nested values
and entity references, dated outgoing links, complete value replacement, direct
profile extension, concept retirement and link clearing. Examples 72–79 add
common/individual profiles and descriptions, one state binding, initial/set/clear
values and an explicit common-description revision. Examples 80–87 add four
magic role profiles, three practice intervals and an explicit tradition edit.
Examples 88–97 add predicates, dated descriptions and classified assertions.
Examples 98–125 add social and environmental contexts with native sections and
assertions. Examples 126–142 add independent existence episodes and explicit
governance policies with native assertions. Examples 143–158 add scoped concepts,
dated sections, linked learning and independent acquisition/forgetting boundaries.
Examples 159–175 add required context briefs and exact writing selections;
176–203 add explicit family acquisitions and independent dated accounts.
Requests 204–207 add a native public retrieval prior and independent learned evidence.
The complete sequence has 207 requests and leaves 144 records, 25 profiles and
nine predicates. See
[the executable sequence](../examples/authoring/README.md).

The [species reader](SPECIES.md) now supports explicit author common/individual
composition and affected variation records. The [magic reader](MAGIC.md) supports
explicit tradition/practice/realm/artifact composition with independent cited
restrictions and exceptions. The [social reader](SOCIAL.md) selects literal
languages/senses/forms and explicit cultural roots. The [environmental reader](ENVIRONMENT.md)
composes habitat/material/technology roots and direct links. Both retain native
[dated descriptions](DESCRIPTIONS.md) and [classified assertions](ASSERTIONS.md)
for explicit evidence owners. The [governance reader](GOVERNANCE.md) selects
an explicit current profile and six role bindings, preserves independent policies
and links admitted assertions by typed identity. Collective Party tags are
projections; stored entity values retain the ordinary Ref grammar.

## Character lore admission

`character lore VIEWER --tick T`, `character concept VIEWER TARGET --tick T`
and `character links VIEWER OWNER --tick T` use a separate
[shared character gate](NARRATIVE_ADMISSION.md). GET routes use the same names
under `/api/character/`, with `character`, required `tick` and exact `target`
or `owner` when needed. Catalogues accept an optional exact domain; catalogue
and link limits are 1–1000. The character cannot supply author labels or a list
of known IDs.

Concepts must be independently known from the viewer's own admitted ordinary
`claim.object.entity` acquisition, canonical, applicable and scoped for that
viewer. Knowing a concept does not teach its outgoing references. Entity leaves
and concept links survive only when their target is independently known and
admitted; retained array values keep relative order without exposing hidden
source positions. Literal values and their used types remain exact. Typed
genealogy/assertion pointers never grant the canonical target's content.

Character concepts omit root titles, untimed bodies, provenance and author
citations. References and links omit source indices and hidden endpoint labels.
This projection runs before counts and complete-result bounds. Generic context
and search include admitted dated description sections, rather than indexing
every value or untimed body of a known concept.

Explicit [writing frames](WRITING_FRAMES.md) now grant admitted roots from a
selected present scene, and [context briefs](BRIEFS.md) collect required admitted
records/members without teaching them. Specialized species, social and
environmental character composition now uses the same admitted typed values,
with no original source positions in links or statements. Character magic
selects explicit learned reports without reading canonical role profiles.
Each family can carry separately selected assertion/knowledge belief targets;
no canonical subject or classification join assigns those words to an owner.
Optional writing family selections retain separate character and author packets.
Bound-state character reads and action capture remain later work.
