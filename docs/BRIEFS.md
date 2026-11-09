# Authored context briefs

A `context-brief` stores explicitly authored, scoped Markdown and ordered required
references. Selecting a brief or a reference grants no knowledge. Composition
either returns every required admitted item or returns one closed unknown result.
It generates no summary, source edit, event or state change.

## Source

Declare `briefs-v1` explicitly. It works independently of lore; referenced
descriptions, assertions and other domains retain their own capability requirements.
The source schema remains `wedl/v1`.

```yaml
kind: context-brief
id: brief_00000000000000000000000001
title: Harbor setting
domain: story.setting
status: canonical
scope: world
valid: {kind: timeless}
audience: [author, public]
perspectives: [author, ordinary]
required_refs:
  - kind: section
    owner_id: loc_00000000000000000000000001
    id: description_00000000000000000000000001
```

The record's Markdown body supplies its exact brief prose. A blank body is allowed;
the maximum is 1 MiB of UTF-8 bytes. Title is a nonblank single-line label of at
most 256 characters; domain is the existing dotted identifier. Status is
canonical by default, with draft and retired retained as valid source.
Audience and perspectives are required, sorted, unique, nonempty label arrays.
Validity uses the same timeless or inclusive interval grammar as descriptions.

The common Rust metadata, chronology, description and opaque extension conventions
remain available. They do not supply inferred pins or applicability. Brief-owned
description sections and typed endpoint references additionally respect the
brief's canonical status and own validity. This is the Rust common-field boundary;
the pinned upstream brief source uses a narrower common envelope.

| Scope | `scope_ref` |
| --- | --- |
| `world` | Forbidden |
| `region` | Required exact record reference to a location or overlay |
| `era` | Required exact section reference to an era in the world's chronology |

An era scope cannot name a calendar, anchor or another owner's section. Scopes
describe authored applicability; they do not infer presence in a region, overlay
membership, calendar conversions or character knowledge. The referenced scope
must itself be admitted during composition. It is not emitted unless also pinned.

`required_refs` is required and contains at most 1024 unique references in authored
order. An explicit empty array is valid; omission and null are invalid. References
have exactly one of these source shapes:

```yaml
{kind: record, id: RECORD_ID}
{kind: section, owner_id: RECORD_ID, id: MEMBER_ID}
{kind: transition, owner_id: RECORD_ID, id: MEMBER_ID}
```

The owner must be an exact current canonical-kind source record; hypotheses are
excluded. Member identity includes its owner, and the member must resolve exactly
once. Sections are explicit descriptions or world chronology calendar/era/anchor
members. Transitions belong to knowledge/relationship `transitions`, story-point
`lifecycle.transitions`, or the generational initialization/transition grammar.
Story-point pins resolve only the nested lifecycle member, and citations retain
its original `lifecycle.transitions[i]` source position. Historical assertion citations
are not live brief references, and ordinary prose or ID-looking metadata stays
literal. Draft/retired referenced source still validates; read admission decides
whether it can contribute.

## Required closure and bounds

Composition visits the selected brief, then its required references in authored
depth-first order. A nested brief recursively contributes its own required items.
Cycles terminate through exact reference identity; they do not create repeated
items or imply inferred relationships. Distinct owner/member pins with identical
text both remain. Emitted learned assertion items are deduplicated by their actual
knowledge transition target, including when another pin selects that transition.

The shared context collector accepts at most 100 selected brief IDs. Repeated
selected IDs deduplicate stably; source `required_refs` duplicates are invalid.
The traversal refuses more than 16384 distinct references. All public list limits
are 1–1000 and are complete or error. Required admission precedes the result
limit: an inaccessible late pin closes the whole result even when earlier
admitted items already exceed the caller's limit. No partial list, hidden label,
missing-pin identity or hidden count is returned.

Source validation covers the captured bytes, parsed records and fingerprint
before composition. Valid private changes that leave admitted evidence unchanged
cannot change a character packet or an unknown result. This does not claim an
authenticated multiuser boundary or bounded total source-validation work.

## Author and character projections

Author composition uses explicit audience/view labels and either an exact horizon
or all-time history. Ordinary record pins retain their source identity and title;
their body follows the same dated prose gate as author search. Event bodies start
at the event time. Scene and conversation summaries require an explicit end and
start at that end. Environment bodies apply inside their inclusive interval.
Other ordinary bodies start at their first explicit direct or lifecycle
transition, if present. Dated pins include `bodyIncluded`; withheld bodies are
empty strings, while the identity remains available. All-time pins preserve the
complete original body. Typed lore, assertions and existence episodes delegate
to their admitted native projections. Generational record pins use the applicable historical fold;
transition pins use the exact admitted history member. Their current summary
title and body never replace historical labels. A generational description pin
also requires an initialized applicable owner. Chronology member pins retain their
exact source declarations. Section pins preserve only the section's own fields,
not an owner's current summary envelope.

Full local-author composition admits the union of audiences `author`/`public`
and perspectives `author`/`ordinary` within one required closure. The brief,
nested briefs, owner, section, typed endpoints and existence character may each
have different admitted labels. This uses one scope for typed projection and
correction suppression; it does not combine several partially successful
single-label compositions. Any required withheld pin closes the whole result
before limits. Source member ordinals, literal ID-looking strings and authored
reference order are preserved. Explicit single-label composition remains
available for narrower requests.

Writing author margins and revision source review use this full union. Character
briefs still require their own complete independent closure; author availability
never substitutes for character evidence.

Character composition uses the shared acquired-record scope. A brief must be
independently known, canonical, applicable and scoped for that viewer before its
explicitly dated body/title can enter the packet. Required targets receive their
own independent admission; selecting them never expands the known set. Bare
ordinary record pins expose safe identity only. Their untimed title/body remains
excluded; use an explicit description-section pin for dated prose.

Known lore concepts retain the existing safe typed projection. Description pins
reuse member scope, validity and correction rules. Canonical assertion record
pins expand into the viewer's admitted literal learned accounts, regardless of
the assertion's current canonical wording or applicability. A canonical existence
episode has no typed character payload and closes a required character pin.
Canonical generational record/transition payloads do not become learned content
through an ordinary source pointer; explicitly learned genealogy belongs to its
own knowledge record. Known-owner description sections retain the existing
independent character section contract.

Character knowledge pins require the viewer's own scoped canonical held knowledge.
A selected historical transition must be at/before the horizon and have an
affirmative learning transition at/before that selected member. Latest forgetting
withholds the whole knowledge record. A historical member reports its own exact
time/state/confidence, while linked and genealogy claims retain their established
safe learned payload. Acquisition notes, untimed bodies, source paths, source
indices, blob IDs and global fingerprints/revisions stay out. Knowing someone
else's knowledge ID does not expose their claim or transitions.

## Interfaces

```sh
wedl-rs --repo my-world briefs compose BRIEF_ID --tick 12 --order 0 --audience author --view author --limit 100
wedl-rs --repo my-world briefs compose BRIEF_ID
wedl-rs --repo my-world character brief VIEWER_ID BRIEF_ID --tick 12 --order 0 --limit 100
```

The author GET route is `/api/briefs/compose`, with `brief`, optional `tick`, `order`,
`timeline`, `audience`, `view` and `limit`. The character route is
`/api/character/brief`, with exact `character`, `brief`, required `tick`, optional
`order`, `timeline` and `limit`. Character requests accept no author scope labels,
known-record grants or policy bindings. The Narrative browser offers both reads.
Native entry points are `briefs::compose` and `briefs::character`. Full local-author
composition is `briefs::compose_full_author`; its envelope exposes `audiences`
and `views` arrays. Existing single-label CLI/GET reads retain their explicit
labels. The Writing CLI/GET/browser author margin uses full-author composition
when briefs are explicitly selected.

Results contain `briefId`, `outcome: available|unknown`, `count`, `items`, `limit`
and `complete: true`, alongside their author or character envelope. An unknown
result has no items. Items contain `target`, `label`, `value` and `citations`.
Wire member targets use `ownerId` with `id`; source references use `owner_id`.
Author items retain source citations. Character items have empty outer citations
and only separately admitted safe evidence inside domain values.

The ordinary reviewed `record create` and `record update` writer supports briefs.
Submitted arrays replace whole arrays; omitted fields and body bytes are retained.
Preview validates the complete candidate, and apply binds to its exact hash and
unchanged source fingerprint. Changing a region/era brief to world scope requires
removing the optional `scope_ref` through a direct source edit under the existing
top-level removal convention. Brief edits share the ordinary single-record
writer; explicitly authored multi-record plans can use the separate
[saved-outcome publication contract](OUTCOMES.md).

Exact writing frames consume this same collector. Required brief material is
included only through explicit selections and counts toward the complete packet
budget. Ordinary context without brief selections retains its existing behavior.
See [WRITING_FRAMES.md](WRITING_FRAMES.md), [NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md)
and [AUTHORING.md](AUTHORING.md).

## Pinned evidence

The reference commit is `f0e885ccebc279eff65a5db2984d01ebc6647b2b`:

- [briefs.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/briefs.py)
  defines the three scopes and exact source member references.
- [brief_runtime.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/brief_runtime.py)
  admits required closure before limits and keeps learned material separate from
  canonical source.
- [Source tests](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_briefs.py),
  [runtime tests](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_brief_runtime.py)
  and publication tests cover literal preservation, exact pin identity,
  generational applicability, private-change invariance and required budgets.

Rust preserves its current snapshot, exact integer codec, all-timeline history,
common metadata and reviewed single-record writer. It does not recreate upstream
legacy body adapters, versioned profiles, cursor protocols or confirmation receipts.
