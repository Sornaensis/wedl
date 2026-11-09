# Authored governance and collective parties

Governance queries select explicit fields from ordinary finite lore profiles.
Laws, treaties, obligations and authority remain authored statements. A profile
name has no built-in legal behavior. Legal, de-facto, claimed and contested
accounts can coexist; the query never chooses a winner, transfers an office,
establishes sovereignty or infers a rule from containment.

The implemented capability is `collective-parties-v1`. It requires
`generational-core-v1` and `lore-core-v1`. Assertions, descriptions, spatial
history and the other capabilities are optional; source using one of them must
still declare and satisfy its existing prerequisites. Governance introduces no
record kind, source migration, profile version, persisted index or alternate
writer. Current source remains `wedl/v1`, and responses use `wedl-rust/v1`.

## Explicit profile-role binding

The current profile ID and all six role selections are required. Each selection
is an exact root-field token or JSON null. A null selection leaves that role
unbound. Selected field names must be distinct. Neither field names nor profile
names imply a binding.

| Role | Compatible selected type |
| --- | --- |
| `territory` | Entity whose nonempty kind set is a subset of `location`, `overlay` |
| `party` | Entity whose nonempty kind set is a subset of `character`, `organization`, `lore-concept` |
| `basis` | Nonempty enum of strings drawn only from `legal`, `de-facto`, `claimed`, `contested` |
| `subjects` | Array of entities with the same allowed kinds as `party` |
| `obligations` | Any existing finite lore type, including explicitly declared null |
| `supersedes` | Entity whose kind set is exactly `lore-concept` |

For example, an author may define a profile `district-policy` with entity fields
`area` and `issuer`, a basis enum, an array `signatories`, a finite object
`duties`, and an optional lore-concept entity `previous`. Its query binding is:

```json
{
  "profile": "district-policy",
  "roles": {
    "territory": "area",
    "party": "issuer",
    "basis": "basis",
    "subjects": "signatories",
    "obligations": "duties",
    "supersedes": "previous"
  }
}
```

The binding does not define or modify the profile. Profiles use the existing
closed lore type grammar; entity values are exact `{ "kind": "record", "id":
"…" }` references to actual source records. Arbitrary strings, prose and
objects that merely resemble references remain literal. Required fields must
exist, optional fields can be absent, and authored null requires a null type or
an enum that explicitly admits it. Exact integers retain the existing decimal
string wire encoding; used type shapes preserve numeric versus string enum
alternatives.

A missing profile, missing selected field declaration or incompatible role
type produces the same empty author result as an unavailable target. Malformed
bindings, duplicate selections and invalid source produce errors. An optional
field absent from one otherwise compatible policy remains unknown for that
policy and does not invalidate the whole binding.

## Source admission and jurisdiction

Every query validates the captured source bytes, fingerprint, installed
capabilities and composed source before projecting values. The target is an
exact location or overlay ID, with no title or alias lookup. Missing, hidden or
wrong-kind targets return an empty envelope with no source labels. Audience and
view are explicit author presentation labels, independent of character learning.

Eligible policies are canonical lore-concepts of the exact selected profile
whose own validity, audience and view permit the read. Ordinary entity endpoints
use the existing narrative visibility gate: explicit scope labels constrain
them, while event time or an organization's historical lifecycle does not grant
or remove author availability. Lore-concept endpoints additionally require
canonical status and applicable validity. Existence-episode endpoints use their
own canonical status, interval and scope labels together with admission of their
character endpoint; a pointer cannot bypass that episode gate.

Every selected typed role must project completely. If even one entity in a
selected scalar, subjects array or nested obligations object is unavailable,
the entire policy is withheld. Unselected fields do not participate in this
test and are not returned. A literal reference-shaped object is not an entity
edge. This complete-policy rule differs from native assertion nested-value
projection, which may retain the admitted portion of a nested assertion value.

Jurisdiction uses only the selected territory:

- A location matches exactly the requested location ID. Parent/child locations,
  routes, geometry, tenures and spatial history do not expand that match.
- A canonical, scoped static overlay matches itself or one of its directly
  authored member locations, including author history reads.
- A time-bounded overlay matches only at an explicit horizon within its exact
  inclusive start/end bounds on the same timeline. A history query never invents
  a membership horizon, including when its target is the overlay itself.
- An unbound or absent optional territory leaves `jurisdiction: "unknown"`.
  It does not claim that the policy applies to the target.

With a horizon, policies use native inclusive timeline/tick/order validity.
Without a horizon, the query retains admitted author policy and assertion
history across timelines. `current_time` is neither required nor changed.

## Independent output and cited statements

Each item contains its concept ID, title, profile, validity, jurisdiction,
selected role values and projected Party tags. Policies are ordered by exact
concept ID. Source order is retained within subjects and Party projections,
including authored duplicates. No basis takes precedence over another.

Three parallel role maps keep unknown and null distinct:

- `roleKnown` is true only when the selected field exists in that policy.
- `roleTypes` records the used type shape for that field; an absent or unbound
  role has no used type.
- `roleCitations` cites the original `values.FIELD` source location. Typed
  `references` retain the original field and array index, role, endpoint identity
  and actual endpoint citation.

Thus a present null obligation has `roles.obligations: null`,
`roleKnown.obligations: true` and its declared used null/enum shape. An absent or
unbound obligation has the same JSON null value but false knownness and no
type/citation. The query does not return unselected profile values or policy
prose.

`statements` contains original native admitted assertion items linked by either
an exact subject reference to the policy or a root **entity-typed** value
reference to it. Nested references, reference-shaped literal objects and
ID-looking words do not create links. Native assertion admission, classification,
attribution, value projection, correction chains and forks run before this join.
An admitted replacement can suppress its predecessor at a horizon; author
history retains the corrections. Governance does not promote an
`attributed-claim` to an author fact. Native historical evidence remains subject
to the assertion contract; governance does not authenticate evidence pointers.

The `supersedes` role is an ordinary admitted source pointer with a
`supersedesCitation`. It never suppresses the referenced policy or arbitrates
between policies. Even a cycle of otherwise valid concept pointers is simply
reported as authored data.

The policy limit is from 1 to 1000, applied after admission and territory
matching. There is also a complete packet bound of 1000 linked statement
occurrences. An assertion linked to two returned policies counts twice because
it appears twice in the output. Unrelated or withheld assertions consume none
of that bound. Exceeding either bound refuses an incomplete result; there is no
cursor or silent truncation.

## Public Party grammar

A Party has exactly two fields, `kind` and `id`:

| Public tag | Required actual source kind |
| --- | --- |
| `character` | `character` |
| `organization` | `organization` |
| `concept` | `lore-concept` |

`governance::validate_party` checks this closed shape, required capabilities and
exact endpoint identity. `party_from_reference` and `party_reference` also
validate the captured source, then convert between a typed record Ref and the
Party shape. These are pure source/display helpers. They grant no endpoint
presentation authority, teach no character and retain no reusable admission
context. Governance calls the conversion only after its endpoint admission.

A Party is never stored as a lore entity value. Collective profile fields also
leave existing rules intact: tenure holders, claim claimants and parentage/union
endpoints remain characters; organization parents remain organizations.
Independent legal/de-facto coholding and authored succession histories keep
their existing meaning.

## Native, command-line and HTTP reads

The native entry point is:

```rust,ignore
governance::query(world, target_id, &binding, at, "author", "author", 100)
```

`at` is `Option<&StoryTime>`; `None` selects author history. The equivalent CLI
read uses a JSON role map:

```sh
wedl-rs governance query loc_00000000000000000000000001 \
  --profile district-policy \
  --roles '{"territory":"area","party":"issuer","basis":"basis","subjects":"signatories","obligations":"duties","supersedes":"previous"}' \
  --tick 5 --order 0 --timeline main --audience author --view author --limit 100
```

Omit the time options for author history. `GET /api/governance/query` accepts
`target`, `profile`, URL-encoded JSON `roles`, optional `tick`/`order`/`timeline`,
`audience`, `view` and `limit`. Both adapters reject unknown fields and duplicate
role JSON keys and call the same native query. The author browser panel captures
these explicit choices and displays the cited results. Existing guarded record
authoring is used for source edits.

## Learned character governance

`character governance VIEWER TARGET --tick T` and GET
`/api/character/governance?character=VIEWER&target=TARGET&tick=T` expose the
separate learned read. Native callers use
`governance::learned(world, viewer, target, at, limit)`, also exported through
`narrative::governance`. It requires collective-parties, generational core and
lore capabilities, a valid source snapshot and an exact horizon.

The target must be an independently known, admitted location or overlay.
Missing, wrong-kind and unavailable targets produce the same empty result.
For an admitted target, the result retains all of the viewer's held linked
assertion claims under [the learned contract](LEARNED_ASSERTIONS.md). There is
no canonical subject, predicate or profile join that decides which learned words
are about this jurisdiction. Every item reports `jurisdiction: unknown`,
`basis: null`, `parties: []` and `presentation: learned-claim; applicability unassigned`.

The character request takes no policy profile, role bindings, author audience,
view or history mode. Limits of 1–1000 apply after target and learning admission;
overflow refuses the complete result. Canonical policies, typed party references
and saved historical evidence never supplement a learned statement. Correcting
or retiring a canonical policy or assertion therefore leaves held character
words unchanged. See [NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md).
