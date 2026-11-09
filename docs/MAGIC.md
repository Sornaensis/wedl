# Magic and metaphysical descriptions

The magic reader composes explicit narrative roles from ordinary finite lore
profiles. The world uses `lore-core-v1` and current profile IDs from
[LORE.md](LORE.md). This reader adds no source kind or capability.

An author chooses a starting concept, profiles for up to four roles, typed link
fields and independent restriction/exception fields. The result preserves those
authored statements, prose and source citations. The reader does not calculate
costs, powers, precedence, success conditions or fictional outcomes.

## Explicit selection

`magic compose CONCEPT_ID` takes these JSON map options:

| CLI option | Map values | Meaning |
| --- | --- | --- |
| `--profiles JSON` | Exact current profile IDs | Required assignment of at least one narrative role |
| `--links JSON` | Ordered arrays of root field names | Optional explicit link fields for each selected role |
| `--restrictions JSON` | Root field names | Optional restriction statement for each selected role |
| `--exceptions JSON` | Root field names | Independent optional exception statement for each selected role |

All map keys come from `tradition`, `practice`, `realm` and `artifact`. Each
selected profile must exist, and one profile may have only one role in a
composition. The profile name, concept title, prose and domain never assign a
role. Profiles can contain any supported finite field types; the role assignment
does not add a schema rule to the source.

The optional maps default to `{}`. Their keys must name selected roles. Each
role can have at most 16 distinct link fields, in the caller's supplied order.
Field names match `[a-z][a-z0-9_]{0,63}`; `values.links` is a path, rather than an
acceptable field selector. Duplicate JSON keys, unknown roles, wrong value
types, unselected-role mappings and individual map inputs over 16 KiB are
rejected. The HTTP layer also bounds the complete query string.

Restrictions and exceptions are selected independently. They may select the same
field or fields of different declared types. Omitting a selection means unknown;
selecting a missing source field also remains unknown. Neither chooses a winning
statement or changes another role's value.

## Query and examples

The root must be an exact admitted lore-concept ID using one of the selected
profiles. Titles, aliases and character IDs cannot select a root.

After authoring examples 80–87:

```sh
wedl-rs --repo my-world magic compose concept_00000000000000000000000009 \
  --profiles '{"tradition":"magic-tradition","practice":"magic-practice","realm":"magic-realm","artifact":"magic-artifact"}' \
  --links '{"tradition":["links"],"practice":["links"],"realm":["links"],"artifact":["links"]}' \
  --restrictions '{"tradition":"restrictions","practice":"restrictions","realm":"restrictions","artifact":"restrictions"}' \
  --exceptions '{"tradition":"exceptions","practice":"exceptions","realm":"exceptions","artifact":"exceptions"}' \
  --tick 5 --order 0
```

This composes Early Lantern Calling with its explicitly linked tradition, realm
and artifact. The practice's object-valued restriction retains its wording,
large integer, decimal value and literal reference-shaped object. The realm's
absent restriction stays unknown, its explicit exception stays null, and an
authored empty exception array stays `[]`.

The middle and late practice roots are
`concept_00000000000000000000000010` and
`concept_00000000000000000000000011`. Read them at tick 15/order 0 and tick
25/order 0 respectively. Each practice keeps its own authored wording and
inclusive validity interval. Example 87 explicitly changes only the tradition's
authored values; subsequent compositions cite that saved statement without
rewriting practice records.

The GET route is `/api/magic/compose`. It accepts `id`, required `profiles`,
optional `links`, `restrictions`, `exceptions`, and normal optional lore `tick`,
`order`, `timeline`, `audience` and `view`. Each map is one JSON query parameter
value. The parameter vocabulary is closed. The Rust library entry point is
`magic::compose(world, root_id, &selection, at, audience, view)`.

Audience and view default to `author`. Omitting the horizon reads matching
authored intervals across all timelines. A supplied tick defaults order to
`2147483647` and timeline to the world's default; order or timeline without a
tick is invalid. Supply explicit order 0 to inspect the examples' endpoint
moments. A concept outside the selected horizon is unavailable as a root and
cannot supply a traversed payload.

The local browser exposes the reader in the Lore tab, with four explicit role
groups, profile selectors, ordered link fields and independent restriction and
exception fields. Live canonical requests capture current source, so saved
edits are available without restarting the server. Historical sessions retain
their selected snapshot.

## Admission and typed links

Composition begins with the shared lore projection. Canonical status, exact
validity, audience and view admission apply before traversal, visible edges,
unknown markers and bounds. Missing, hidden, draft, retired and unselected-profile
roots share one unavailable response. Unrelated concepts do not consume a
catalogue limit before the relevant composition is selected.

A link field must be declared as an entity or an array of entities. Only
surviving projected typed references supply edges. Strings, ordinary
reference-shaped objects, prose and `concept_links` relations are not followed.
The declared type is checked even when a projected array is empty: an empty
string or object array does not become known linkage. A declared entity array
whose admitted projection is `[]` is known-empty, including when inaccessible
targets have been removed.

A missing or withheld scalar field is one generic unknown. Withheld array
members produce no placeholder, target ID, extra unknown or traversal visit.
Surviving references retain their original source indexes; projection can turn
the second source member into the first returned value while its citation still
names `values.links[1].id`.

Traversal follows the caller's link-field order and each source array's order,
depth first. Shared records appear once. Every traversed edge remains in the
`links` array, including repeated and shared references. An admitted target
outside the selected profiles retains its visible edge citation and becomes an
explicit unknown instead of contributing a record payload.

| Bound | Behavior |
| --- | --- |
| Selected link fields | At most 16 distinct root fields per role |
| Depth | At most 16; the root is depth 0 |
| Distinct visited records | At most 64, including the root and admitted targets outside the selected profiles |
| Visits | At most 1,024, including the root and every repeated or shared target visit |

An active-path cycle or an exhausted bound refuses the whole result. No partial
composition is returned for these failures. Unknown authored inputs are instead
represented explicitly in a completed traversal. Bounds are correctness guards,
not a claim of indexed or bounded-total-work performance.

## Result meaning and source evidence

The result keeps the author lore envelope: `protocol`, `perspective`, `scope`,
exact `at`, audience/view, explicit `selection` and
`basis: explicitly-linked-magic-descriptions`. It adds `rootId`, ordered
`records`, ordered `links`, `unknowns` and `linkageComplete`.

Every record contains its assigned `role`, concept ID, title, current profile,
`fields.restrictions`, `fields.exceptions`, `body`, source citation and any
authored provenance string. The body is labeled
`bodyKind: undated-authored-prose`; it is not a dated narrative section.

Each selected statement has one of these meanings:

| Statement | Result |
| --- | --- |
| No field selected | `known: false`, `field: null`, `reason: unselected` |
| Selected field absent from the admitted projection | `known: false`, selected `field`, `reason: not-supplied-or-visible` |
| Authored admitted field present | `known: true`, selected `field`, `value`, used `valueType`, field citation and surviving `references` |

Known null and empty values remain present and known. Integer values use the
shared exact decimal-string wire representation, with their used type retained;
numeric and string enum alternatives remain distinguishable. Object and array
values remain structured, and ordinary reference-shaped values remain literal.

Source citations retain entity ID, source path, blob OID, revision, current
source fingerprint and section. Each `links` entry contains `ownerId`, original
`sourceField`, target `entityId`, kind, admitted title, source citation and target
citation. Link unknowns carry `ownerId`, `sourceField` and reason
`absent-or-unavailable`, `not-typed-reference` or `target-profile-unavailable`.
They do not distinguish inaccessible payloads from absence.

`linkageComplete` concerns only the explicitly selected links. It can be true
when a restriction or exception field is unknown. It does not declare that the
fiction is consistent, that a practice is permitted or that any outcome follows.

Author composition also returns `canon` and `sections`, each an ordered array of
`{ownerId, result}` native evidence streams for exactly the returned records.
Assertions retain classification, attribution, literal value, applicability and
correction history. Descriptions retain their exact authored Markdown, independent
validity and same-owner correction rules. Competing accounts remain separate;
neither stream selects a winning explanation or changes a restriction field.
Source citations keep their original owner/member identity and indexes.

These streams are optional: `assertions-v1` enables assertions and
`descriptions-v1` enables dated sections. An undeclared stream is `[]`, so existing
lore-only author compositions need no additional capability. No horizon retains
all admitted native history across timelines; an exact horizon applies inclusive
scope/time rules and correction suppression. Their combined bound is 512 admitted
items; overflow refuses the whole result. The record body remains separately
labeled `undated-authored-prose` and is never used to fill a missing dated account.

## Character learned explanations

Character magic reads explicit learned assertion or knowledge targets. It does
not compose current canonical roles, choose a profile catalogue, or associate
learned wording with a current canonical subject. The native operation is:

```rust
magic::character(world, viewer, belief_targets, at)
```

Supply an exact character ID, exact story horizon and one to eight distinct exact
assertion or knowledge IDs. The world needs the existing `lore-core-v1` and
`assertions-v1` capabilities. Each target uses the viewer's own scoped canonical
linked knowledge, a prior affirmative acquisition and the latest held state.
A later rejection remains a literal held account. Forgetting withholds it;
remembering can make it available again. Ordinary acquisition of a concept or
another person's knowledge ID does not become a learned explanation.

The result uses the metadata-free character envelope and
`basis: explicitly-learned-magic-explanations`. It retains the ordered
`beliefTargets`, grouped `beliefs: [{targetId, result}]`, aggregate `count`,
`outcome: available|unknown` and `complete: true`. Each grouped result follows the
native learned-assertion list: literal statement and claim, selected knowledge and
transition identity, held state, numeric confidence, exact learning time and safe
target. Outer evidence citations are empty. A missing or unlearned target remains
an empty result; the overall outcome is unknown when all selected targets are
empty. An unknown target never triggers a canonical fallback.

The current assertion's subject, classification, predicate, value, title, validity
and corrections do not supply character content or decide its association with a
magic role. Canonical profile edits cannot rewrite retained learned explanations.
Untimed bodies, source paths, hashes, revisions, source indexes and private
learning notes remain absent. Full captured-source validation still applies before
projection, with generic character validation failure.

Only selected admitted rows count toward the shared 512-item evidence bound.
Compact result JSON and its final newline must fit 262144 UTF-8 bytes. A bound
failure refuses the whole result. Unselected accounts and private learning do not
consume those output limits or alter a complete character packet.

```sh
wedl-rs --repo my-world character magic VIEWER_ID \
  --belief-target ASSERTION_ID --belief-target KNOWLEDGE_ID --tick 15 --order 0
```

The closed GET route is `/api/character/magic`, requiring `character`, `tick` and
`beliefTargets` as a JSON string array, with optional `order` and `timeline`.
It accepts no author profile/field selection, author labels or arbitrary record
grants. The browser's Character magic control uses the same learned-target read.

Exact writing-frame family selections keep the two paths explicit:

```json
{
  "magic": {
    "author": {"rootId": "CONCEPT_ID", "selection": {"profiles": {"practice": "magic-practice"}}},
    "beliefTargets": ["ASSERTION_OR_KNOWLEDGE_ID"]
  }
}
```

The example uses placeholder IDs. Author and dramatic-irony lenses require the
`author` selection; character and dramatic-irony lenses require nonempty learned
targets. A character lens never reads the optional author root or role payload.
The two results remain in their independently admitted frame components. The
captured source and exact time come from the existing six-field frame rather
than an ambient cursor. A complete context read retains explicit unknown links
or empty learned targets. It does not certify an actionable capture or infer
that the selected evidence is sufficient. See [WRITING_FRAMES.md](WRITING_FRAMES.md) for the complete
request and packet contract.

## Editing and remaining boundaries

Current profiles and concepts use the existing reviewed source writer. Reading
these descriptions changes no source file, world time, state value, action,
approval or character evidence. See [AUTHORING.md](AUTHORING.md).

Author composition includes native [dated descriptions](DESCRIPTIONS.md) and
[classified assertions](ASSERTIONS.md). Character magic retains explicit learned
words through the shared [character contract](NARRATIVE_ADMISSION.md). This
division follows the pinned magic reader: known canonical role values are not
silently substituted for learned explanations.

Exact [writing frames](WRITING_FRAMES.md), scene-derived grants and authored
[briefs](BRIEFS.md) are available, including explicit magic family selection in
the frame's separate components. Saved-action review and exact literal
requirement comparisons are available through [ACTIONS.md](ACTIONS.md), using
a complete admitted capture, explicit operands, source classifications and
exact decimal comparison. A match never supplies outcome approval. Separate
[reviewed publication](OUTCOMES.md) applies only explicit saved plans.
Explicit reveal plans and the separate reader service now have their own admission contract in
[REVEALS.md](REVEALS.md) and [READER.md](READER.md). Work-bound Writing context remains unfinished.
Current local-author sessions support [reviewed browser publication](BROWSER_PUBLICATION.md)
through the existing writer. Historical sessions remain fixed and GET-only.
All existing v1 labels and current profile IDs remain unchanged.

## Pinned upstream evidence

The behavior is adapted from commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`:

- [Magic profiles guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/magic-profiles.md)
  defines the four roles, independent statements, author/character separation
  and later capture/comparison workflow.
- [Magic profiles reader](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/static/magic_profiles.mjs)
  supplies explicit profile selection, ordered typed-link traversal, cycle
  handling and the 16/64/1,024 bounds.
- [Magic source tests](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_magic_profiles.py)
  exercise three eras, exact authored values, admission and distinct learned
  explanations.
- [Magic transport tests](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_magic_profiles_transport.py)
  cover source-pure reads, exact authoring replay and writing/action captures.

The Rust character operation specifically follows the pinned explicit learned
target path. Its source validation, numeric codec, current profile IDs and native
complete-result bounds remain unchanged; there is no profile-version adapter or
legacy body projection.

Rust uses the project's current profile IDs, shared lore admission and current
source citations. It deliberately checks the declared entity element type for
empty link arrays, preventing an empty literal array from gaining link authority.
The local contract is exercised by `tests/magic.rs`,
`tests/magic_authoring.rs` and the matching interface tests.
