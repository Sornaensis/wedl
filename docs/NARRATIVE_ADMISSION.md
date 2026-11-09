# Shared character narrative admission

The character narrative readers select explicit dated source through one
internally derived character scope. Knowing a record and believing a statement
remain separate: an entity acquisition can make that record's scoped
descriptions or typed lore available, while a learned assertion continues to
return the character's own authored words. The current canonical assertion is
never substituted for those words.

This slice keeps the source schema `wedl/v1`, protocol `wedl-rust/v1` and existing
capability names. It adds no source migration, caller-supplied grant, manuscript
reader scope or inferred learning.

## Deriving the scope

Every operation requires an exact current character ID and an exact story
horizon. Native tick and order retain signed i64/i32 values; JSON carries their
exact decimal strings. CLI and HTTP require a tick, default the timeline to the
world default and default order to `2147483647`.

The character's fixed presentation labels are:

| Selector | Admitted labels |
| --- | --- |
| Audience | `public`, exact character ID |
| Perspective | `ordinary`, exact character ID |

An optional root label array adds a constraint only when present. An empty
array admits nothing. Knowledge labels are unique nonblank string arrays without
an ordering requirement; malformed labels fail closed. A caller cannot supply
another audience, view or known-record set.

The known set starts with the character itself. A canonical knowledge record
must belong to that character and pass both learning-label constraints before
it can add anything. Its current transition is the latest same-timeline
transition at or before the requested horizon.

| Knowledge source | Own knowledge ID becomes known when | Additional canonical record grant |
| --- | --- | --- |
| Ordinary claim | A current transition exists and is not `forgotten`, including an ordinary held rejection | The exact existing `claim.object.entity`, when supplied |
| Linked assertion claim | Existing linked-claim admission finds an affirmative acquisition and a current state other than `forgotten` | None |
| Typed genealogy claim | Existing learned genealogy admission finds an affirmative held state and a currently applicable learned interval | None |

Linked claims reuse `learned_assertions::admitted`; genealogy reuses
`generational::knowledge::admitted` with current applicability. In particular,
an initial rejection does not acquire a linked assertion claim, a rejection
after affirmative learning may remain held, and a rejected genealogy record is
not current learned genealogy. These are the existing domain rules, not a new
shared interpretation of belief states.

Ordinary `claim.subject`, literal words, learned genealogy endpoints, assertion
subjects, transition source/causing-event metadata, and concept links grant no
additional canonical records. Discovering a link or a typed reference does not
recursively acquire its target. There is no scene selector in these operations,
so scene presence supplies no new known roots.

### Record availability

A known ordinary record must pass its own optional root labels. Its independent
dated sections do not inherit the owner's event time or infer a canonical-only
status restriction. This preserves the ordinary description contract: an
explicitly available forecast can have a different applicability from its event.

A lore concept or context brief additionally needs canonical status, matching
own labels and its own applicable validity. Hypotheses are unavailable. Canonical assertions
and existence episodes are unavailable as character narrative source even when
an ordinary entity pointer names them. Their current canonical values, bodies
and sections cannot be acquired by following a learned link.

## Native operations

All six functions are in `narrative` and accept `&World`, exact IDs and
`&StoryTime`:

| Function after the world argument | Result |
| --- | --- |
| `descriptions(viewer, owner, at, limit)` | All admitted dated sections on one known owner |
| `section(viewer, owner, section_id, at)` | One admitted section identified by owner and local section ID |
| `lore_catalog(viewer, domain, at, limit)` | Admitted typed concepts, optionally filtered by one exact domain |
| `lore_concept(viewer, target, at)` | One exact admitted concept |
| `lore_links(viewer, owner, at, limit)` | Applicable links with independently admitted owners and targets |
| `spatial_history(viewer, owner, at, limit)` | Applicable admitted members on one canonical location or route |

Exact IDs are required; titles, aliases, headings and profile labels are not
resolution fallbacks. Unknown, withheld and unavailable owners share the same
unavailable error. An available owner with no matching members returns an empty
list. A suppressed or unavailable exact section uses the same unavailable error
as a missing section.

List limits are 1–1,000 and apply after admission, applicability, corrections and
optional domain filtering. Overflow refuses the complete operation. No partial
page or hidden-row count is returned.

Each public read validates the complete captured source, including parsed-field
agreement with exact source bytes and the snapshot fingerprint. Invalid source
returns a generic character narrative validation error. Detailed author
diagnostics remain available through author validation.

### Dated descriptions

The existing `descriptions-v1` parser and correction rules are reused.
Descriptions remain independently installable without lore. Each member must
pass its own audience, perspective and explicit validity before it can
participate in corrections. Only eligible corrections suppress eligible earlier
members. Independent accounts and correction forks remain separate.

The character projection contains `ownerId`, `sectionId`, exact `heading` and
`markdown`, `valid`, a stable section target, and `replaces` only when its target
is also eligible. It contains no owner title or source position. The opaque
record body is not converted to a timeless section.

Generic character context and search consume these same admitted description
rows. Admission and correction suppression precede context budgets and search
indexing. Search result kind `description` identifies this material. When no
description rows are admitted, the existing empty context shape stays intact.
The generic context does not automatically import typed canonical lore values.

### Typed lore and direct links

The existing profile parser, typed projection and exact numeric serializer are
reused. A concept returns its ID, domain, profile ID, validity, admitted values,
used value-type shapes, safe references, direct admitted links and record
target. It does not expose the full profile registry or author profile labels.

Typed entity leaves require independently available endpoints. Withheld entity
leaves are pruned before values, used type shapes and reference lists are
returned. Literal string/object fields that resemble references remain literal;
the profile type determines whether a value is an entity reference. Exact
numbers, explicit nulls and enum scalar types retain their authored meaning.

Character references carry only the admitted entity ID, kind and stable target.
Character links carry only owner ID, concept ID, relation, validity and stable
target. Neither carries `sourceField` or a source-array index: inserting a
withheld reference or link before an admitted one must not change the complete
character result. Root bodies, canonical titles, provenance and global citations
are absent.

### Spatial history

The owner must be an independently known canonical location or route. Applicable
placement/geometry members additionally require their map endpoint; parentage
members require a non-null parent endpoint. An explicit null parent and literal
route metrics introduce no endpoint grant.

Endpoint admission occurs before correction suppression. A correction whose map
is unlearned cannot suppress an older geometry on a learned map. Each returned
member carries its ID, aspect, validity, literal value, stable section target
and an eligible replacement ID where applicable. It carries no owner title,
source index or citation.

## CLI, HTTP and transport

The separate character command family includes:

```sh
wedl-rs --repo my-world character descriptions CHARACTER_ID OWNER_ID --tick 5 --order 0
wedl-rs --repo my-world character section CHARACTER_ID OWNER_ID SECTION_ID --tick 5
wedl-rs --repo my-world character lore CHARACTER_ID --domain example.lore --tick 5
wedl-rs --repo my-world character concept CHARACTER_ID CONCEPT_ID --tick 5
wedl-rs --repo my-world character links CHARACTER_ID OWNER_ID --tick 5
wedl-rs --repo my-world character spatial-history CHARACTER_ID OWNER_ID --tick 5
```

The closed GET endpoints are `/api/character/descriptions`, `/section`, `/lore`,
`/concept`, `/links` and `/spatial-history`. All require `character` and `tick`;
owner reads use `owner`, concept reads use `target`, and section reads additionally
use `section`. List operations accept `limit`; all accept optional `order` and
`timeline`; the catalog additionally accepts `domain`. Unknown parameters,
including audience/view or extra known-record grants, are refused.

Native results use `perspective: character`, `scope: explicit-record-admission`,
the character ID, exact effective time and an operation-specific basis. Character
transport envelopes omit global source fingerprint, revision, paths, blob IDs
and diagnostics. Author operations continue to use their existing authored
source projections and author metadata.

## Existence, governance and the remaining context boundary

The same module re-exports `existence::character` as `narrative::existence` and
`governance::learned` as `narrative::governance`.

Character existence returns an explicit empty typed episode result. Current
knowledge contains no learned typed existence snapshot, so a known episode ID
does not establish episode kind, current embodiment, validity or evidence.

Character governance requires an independently admitted location/overlay
target. It presents the character's admitted linked assertion words with
jurisdiction `unknown`, basis null, parties empty and applicability explicitly
unassigned. It does not join those words to canonical governance profiles or
infer jurisdiction from prose, profile bindings, titles or entity references.
The linked claims remain separate even when they disagree.

Explicit [context briefs](BRIEFS.md) now collect a complete required closure
through this gate. A pin never expands the known-record set. The brief's own
prose is explicitly scoped and valid; ordinary record bodies remain excluded.
Brief-owned descriptions require the same canonical, scope and applicability
gate as the brief owner.

Exact [writing frames](WRITING_FRAMES.md) can select a scene that the viewer is
actually present in at the supplied horizon and whose labels admit that viewer.
This adds the scene, location, current participants, objects and environments,
with each endpoint's own scope still enforced. Scoped current relationship
endpoints and ordinary claim subjects also participate only in that explicit
scene scope. Typed assertion/genealogy pointers and transitive links still grant
nothing. Existing character reads use their unchanged scope without an implicit
scene selection.

Species, social and environmental character compositions now use this gate
before discovering typed links or charging work bounds. Exact belief targets
select separate literal learned accounts, without canonical owner/subject joins;
character magic uses only those targets. Optional writing-family selections use
the frame's same captured scope and keep character/author components separate.
Semantic/hybrid [retrieval](RETRIEVAL.md) now ranks the same admitted search
targets using a separate training-only public prior; it adds no scene or family
grant. Bound-state character reads, action captures and manuscript/reader
knowledge remain unfinished. These shared gates do not certify
complete upstream entitlement or narrative conformance.

## Source evidence and verification

The admission choices were checked against the pinned WEDL source at
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`:

- [`brief_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/brief_runtime.py):
  learning labels before known roots and no typed-link canonical grants.
- [`description_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/description_runtime.py):
  known owners, member labels/applicability and corrections after admission.
- [`lore_query.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/lore_query.py):
  scoped typed values and independently admitted links and entity leaves.
- [`spatial_history_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/spatial_history_runtime.py):
  endpoint admission before correction and empty character source citations.
- [`existence_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/existence_runtime.py)
  and [`governance_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/governance_runtime.py):
  unavailable typed existence knowledge and unassigned learned governance claims.

Rust intentionally maps its existing ordinary `claim.object.entity` acquisition
to the pinned runtime's legacy `claim.entity` acquisition. It does not add the
pinned scene-dependent subject/object grants, new knowledge grammar, cursor
protocol or revision-pin request format. Typed knowledge admission reuses Rust's
already implemented domain rules.

`tests/narrative_admission.rs` exercises exact learning/applicability boundaries,
fixed lenses, forgotten and rejected states, typed knowledge isolation, direct
links, endpoint-first corrections, exact native values, whole-source coherence,
independent capabilities, safe context/search integration and full-response
invariance under withheld source edits and insertions.
