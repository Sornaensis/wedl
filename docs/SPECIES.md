# Common and individual descriptions

The species reader composes explicitly authored common descriptions and
individual variation from ordinary lore concepts. It introduces no source kind,
capability, biological model or automatic membership. The world needs
`lore-core-v1` and the current profiles described in [LORE.md](LORE.md).

Callers select the profiles and fields for each read. Common statements and an
individual's own statements remain separate. Author reads retain source citations;
character reads use the shared independently acquired-record scope. The
reader does not choose a winning trait or derive culture, language, religion,
anatomy, life span, movement or capabilities from another field.

## Explicit selection

Both operations require these selections:

| CLI option | Meaning |
| --- | --- |
| `--common-profile ID` | Exact existing profile ID for common-description concepts |
| `--individual-profile ID` | Exact existing profile ID for individual-variation concepts |
| `--individual-link FIELD` | Root field on an individual concept that explicitly references common concepts |
| `--common-link FIELD` | Optional root field linking a common concept to further common concepts |
| `--common-fields JSON` | Optional role-to-root-field map for every common statement |
| `--individual-fields JSON` | Independent optional role-to-root-field map for the individual statement |

The same profile may be selected for both roles. No profile name or field name
has built-in species meaning. Omitting the common-link selector ends traversal
at each directly linked common description.

The two field maps are JSON objects whose values are exact root field names.
Their closed role vocabulary is `taxonomy`, `life_stage`, `habitat`, `traits`,
`culture`, `language` and `religion`. For example:

```json
{"traits":"traits","life_stage":"stage_note","habitat":"habitats"}
```

Each role may be omitted, and empty maps are the default. Field names match
`[a-z][a-z0-9_]{0,63}`; a path such as `values.traits` is invalid. Duplicate JSON
keys, unknown roles, non-string field names and maps over 16 KiB are rejected.
Omitting a role is different from explicitly selecting a missing source field.

## Query operations

`compose` takes the exact ID of an individual-variation **concept**, rather than
a character ID or a title/alias. It returns that statement and its reachable
common statements. `affected` takes the exact ID of a common-description concept
and lists admitted variation records whose explicit selected links reach it.
The count concerns authored variation records; it is not a character population
or inferred membership count.

After authoring examples 72–79:

```sh
wedl-rs --repo my-world species compose concept_00000000000000000000000005 \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents \
  --common-fields '{"traits":"traits","life_stage":"life_stage","habitat":"habitats","religion":"religion"}' \
  --individual-fields '{"traits":"traits","language":"language"}' \
  --tick 15 --order 0

wedl-rs --repo my-world species affected concept_00000000000000000000000003 \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents \
  --individual-fields '{"traits":"traits"}' --tick 15 --order 0 --limit 100
```

The first result keeps the River Folk statement, its linked Tide Folk statement
and Ana's own variation separate. A missing life-stage field stays unknown;
the Tide Folk's explicit null and empty habitat array remain null and `[]`.
Ana's explicit null language does not replace a common language statement.
The second result confirms the authored path from Ana's variation through River
Folk to Tide Folk. After the variation's inclusive endpoint at tick 20/order 0,
that variation no longer participates in a dated read.

The GET routes are `/api/species/compose` and `/api/species/affected`. Both use
`id` for the exact selected concept and underscore parameter names:
`common_profile`, `individual_profile`, `individual_link`, optional `common_link`,
`common_fields` and `individual_fields`. Encode each field map as a JSON query
parameter value. All parameters are closed; only `affected` accepts `limit`.

Both operations also accept the normal optional lore `tick`, `order`, `timeline`,
`audience` and `view`. Audience/view default to `author`. An omitted horizon reads
matching authored intervals across all timelines. A supplied tick defaults order
to `2147483647` and timeline to the world default; order or timeline without tick
is invalid. Use explicit order 0 to inspect the examples' inclusive endpoints.

The local browser exposes both operations in the Lore tab, with separate common
and individual field controls. It reads the current server snapshot. Restart the
server after source edits to inspect their saved result.

## Admission, typed links and bounds

The reader first obtains one scoped projection of canonical concepts using the
same status, validity, audience and view rules as ordinary lore queries. Hidden,
absent, draft, retired and wrong-profile selected owners all produce the same
unavailable result. Unrelated concepts do not consume an arbitrary catalogue
limit before the chosen profiles are considered.

A selected link field must be declared as an entity or an array of entities.
Only surviving projected typed references are followed. An ordinary object that
resembles a reference, a string, prose or a `concept_links` relation supplies no
ancestry. The declared descriptor is checked even for an empty array, so an empty
string/object array does not qualify as known linkage. A declared entity array
is known-empty when its projected result is `[]`, including after hidden targets
have been withheld. No raw hidden edge is consulted to report incompleteness.

Common traversal follows authored link order, depth first. Shared ancestors
appear once; an active-path cycle refuses the entire result. Missing/unavailable
fields, unsuitable declared field types and surviving references outside the
selected common profile become explicit unknowns. A confirmed path can still
appear in `affected` when another branch is unknown; the item retains that
incompleteness rather than inventing the missing description.

| Bound | Behavior |
| --- | --- |
| Common depth | At most 16, with directly linked common records at depth 1 |
| Visited records | At most 64 including the selected individual and visited target records; repeated targets count once |
| Link visits | At most 65,536 per composition, including repeated links |
| Affected result limit | 1–1000, default 100; complete result or an error |

A cycle or exhausted traversal bound encountered while checking affected
individuals refuses the whole query. Bounds are correctness guards for complete
answers, not a claim of indexed or bounded-total-work performance.

## Result meaning and citations

Results retain the usual author lore envelope, exact `at`, audience/view and the
explicit `selection`. Composition returns `individual`, a `common` array,
`linkageComplete` and `unknowns`. Affected results return the selected `common`
statement, `items`, `count` and `limit`; each individual item also carries its
own `linkageComplete` and `unknowns`.

Each statement contains concept ID, title, profile, all seven display roles,
source citation and `body` labeled `bodyKind: undated-authored-prose`. A known
role contains its selected `field`, exact projected `value`, used `valueType`,
field citation and surviving reference citations. Unknown roles have
`known: false` and reason `unselected` or `not-supplied-or-visible`, with no
invented value. An authored null still has `known: true` and a present null value.

Link unknowns identify `ownerId`, original `sourceField` and one of
`absent-or-unavailable`, `not-typed-reference` or `target-profile-unavailable`.
They do not distinguish inaccessible source from absence. `linkageComplete`
describes the selected linkage only: it does not assert that every display role
has a known value. Surviving array-reference citations retain original source
indexes even when projection removes earlier items.

Both author operations additionally return `canon` and `sections`, grouped as
`{ownerId, result}` around native assertion and description results. Composition
uses the selected individual followed by each returned common owner. Affected
reads use the selected common owner and returned matching individuals; a common
ancestor that is not itself returned does not become an extra evidence owner.
The native streams retain their own labels, exact applicability, classifications,
correction rules and original citations. An undated read preserves all admitted
history across timelines. The body stays separately labeled undated prose.

Descriptions are included when `descriptions-v1` is declared and assertions when
`assertions-v1` is declared. An undeclared optional stream is an empty array;
ordinary lore-only compositions continue to work. The combined admitted evidence
limit is 512 items. Exhaustion refuses the result, without clipping either stream.

## Character composition and affected records

The native entry points are:

```rust
species::character_compose(world, viewer, individual_id, selection, belief_targets, at)
species::character_affected(world, viewer, common_id, selection, at, limit)
```

They require an exact character ID and story horizon. The individual and every
common concept must independently pass the acquired-record scope, canonical
status, own labels and validity. Typed endpoints retain their own independent
scope and domain applicability. Selecting a profile, a root or a link
does not acquire it. Current ordinary knowledge can acquire a concept through its
explicit entity pointer; linked assertions remain independent learned accounts.
The callers supply no known-ID set or alternative author labels.

The same typed traversal and depth/record/link limits apply after character
projection. Inaccessible array members contribute no edge, unknown placeholder,
count or visit. Surviving members retain their order. A missing or withheld scalar
link is one generic unknown. An admitted cycle refuses the complete result;
an unlearned cycle contributes no edges. `character_affected` counts only known
variation records with an admitted explicit path to the known selected common
record, with its usual 1–1000 complete-result limit.

Character statements retain `conceptId`, exact `profile`, selected fields and a
safe record `target`. Known values retain structured nulls, arrays, exact integer
strings and used type metadata. References contain safe entity/kind/target
identities. Unknown links contain the selected root `field`, owner ID and generic
reason. Root titles, aliases, untimed bodies, provenance, source positions, source
citations, paths, hashes and global revisions are absent. Inserting hidden source
array members cannot change visible reference positions or a complete packet.

Both character operations include independently admitted dated `sections` for
their returned evidence owners. Scope, time and corrections apply before counting.
They require no descriptions capability when that optional source is absent.
Composition additionally accepts zero to eight distinct exact assertion or
knowledge IDs as `belief_targets`. These explicit targets produce separate
`beliefs` entries, each `{targetId, result}` with the native learned-assertion list.
An unlearned target has an empty result. Selecting a canonical assertion does not
read its subject, predicate, classification, value or current applicability to
associate it with a species statement. The author's attribution and the viewer's
literal learned account remain separate; held rejection can remain visible while
forgetting withholds the account. Nonempty belief selections require the existing
lore and assertion capabilities. Affected lists take no belief selectors.

Every character result has `complete: true`; the shared 512-item evidence bound
counts admitted sections and selected learned rows together. Compact JSON plus
its terminating newline must fit 262144 UTF-8 bytes. Oversize results fail instead
of ranking, clipping or returning a partial composition. Full captured-source
validation precedes reads, and valid private edits that preserve admitted material
leave complete character packets unchanged.

```sh
wedl-rs --repo my-world character species VIEWER_ID INDIVIDUAL_CONCEPT_ID \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents \
  --common-fields '{"traits":"traits"}' --individual-fields '{"traits":"traits"}' \
  --belief-target ASSERTION_OR_KNOWLEDGE_ID --tick 15 --order 0

wedl-rs --repo my-world character species-affected VIEWER_ID COMMON_CONCEPT_ID \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents --tick 15 --order 0 --limit 100
```

`--belief-target` is optional and repeatable for composition. The closed GET routes
are `/api/character/species` and `/api/character/species-affected`. Both require
`character`, exact concept `id`, the existing underscore-named profile/field
selectors and `tick`; optional `order` and `timeline` keep the normal exact-time
rules. Composition alone accepts optional `beliefTargets` as a JSON string array;
affected alone accepts `limit`. The browser's Character group uses these same
operations and omits author labels.

An optional species selection in an exact writing frame uses `individualId`,
the unchanged `selection` object and optional `beliefTargets`. Its character
component uses that frame's admitted scene scope; standalone character species
reads grant no scene roots. The author's canonical evidence and character's
learned evidence remain in their separate frame components. A complete report
can retain `linkageComplete: false` and explicit unknown branches; this reading
does not assert that an action has sufficient evidence. Required roots must
resolve, and exhausted bounds still refuse the whole context. See
[WRITING_FRAMES.md](WRITING_FRAMES.md) for complete selection and packet rules.

## Editing and remaining boundaries

Profiles and concepts use the existing reviewed writer. Changing a common
description changes a subsequent saved-source composition; it does not rewrite
individual values or create inherited state. Examples 72–79 include three
explicit common/individual concepts and separate bound state with initial,
set and clear values, reusing the ordinary editable kinds and reviewed writer.

State bindings select the validation type of an authored state key. They do not
automatically choose the species reader's profiles, fields or starting concept.
Reading or linking a description grants no character evidence. Separate author
[dated sections](DESCRIPTIONS.md), [assertions](ASSERTIONS.md) and
[magic composition](MAGIC.md) are available. The shared
[character readers](NARRATIVE_ADMISSION.md) now admit explicitly acquired lore,
known-owner dated sections and independently learned assertion words.

Author composition now attaches native sections and assertions, while character
composition uses only its independently admitted values, sections and explicitly
selected learned accounts. Exact [writing frames](WRITING_FRAMES.md) select this
specialized context alongside [briefs](BRIEFS.md) without widening standalone
character admission. Action outcomes and manuscript/reader workflows remain
separate planned work.
See [LORE.md](LORE.md) for the shared typed source and
[AUTHORING.md](AUTHORING.md) for publication.

## Pinned evidence

The reference remains commit `f0e885ccebc279eff65a5db2984d01ebc6647b2b`:

- [Species reader](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/static/species_traits.mjs)
  composes common and individual statements from admitted typed values and
  attaches independently admitted descriptions.
- [Species guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/species-traits.md)
  distinguishes individual variation, explicit profile selection and character
  knowledge from inferred membership or traits.
- [Species source tests](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_species_traits.py)
  and the native description/assertion/learning contracts supply the source-backed
  boundaries for dated evidence and independently retained accounts.

Rust uses the existing current profiles and shared character gate. It retains no
legacy body adapter or author-citation fallback for character statements, and
does not join canonical assertion ownership into learned evidence.
