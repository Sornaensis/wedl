# Environmental, material and technology context

These readers assemble explicitly selected lore and dated evidence at one exact
story horizon. Author context includes classified assertions; character context
keeps independently learned words separate from admitted concepts. Habitat, resource, livelihood, trade,
infrastructure, technology and era records remain separate authored accounts.
Quantities, dependencies and dates written in prose do not execute a simulation
or create character knowledge. All project labels remain v1.

## Current profiles and source

The reader recognizes these exact current profile IDs:

| Profile | Role |
| --- | --- |
| `context-habitat` | Habitat |
| `context-resource` | Resource |
| `context-livelihood` | Livelihood |
| `context-trade-practice` | Trade practice |
| `context-infrastructure` | Infrastructure |
| `context-technology` | Technology |
| `context-era` | Authored era |

The [profile fragment](../examples/profiles/environment-material-technology-profiles.json)
contains ordinary current profile declarations, without separate profile
versions, and the two context predicates. Their fields use the existing closed
lore types. The standard source examples use optional typed concept arrays,
typed location arrays and literal text. A quantity is wording such as
`{amount_text: about two dozen, unit_text: baskets, qualification: keeper estimate}`;
the reader does not parse amounts, convert units or calculate totals.

Profile IDs assign roles; they do not impose an additional source schema. Current
declarations still determine which values are typed references. A familiar field
name or a Ref-shaped literal object never establishes a relationship by itself.
Profile replacement validates all dependent concepts through the shared writer.

The existing `lore-core-v1`, `descriptions-v1` and `assertions-v1` capabilities
must all be explicitly enabled. No new capability or source kind is introduced.
Concepts, predicates, assertions and sections use the ordinary reviewed edits;
the reader publishes nothing. For mutually linked new concepts, create valid
endpoints first and then author their explicit links. A source self-link or cycle
is allowed and does not imply a recursive dependency calculation.

## Exact author reads

```sh
wedl-rs --repo my-world environment context --context concept_00000000000000000000000032 --tick 3 --order 0
```

After the complete authoring examples, this reads the trade practice with its
direct resource, infrastructure and livelihood references plus an applicable
habitat concept link. Five records are selected. The technology and era behind
those direct endpoints remain outside this one-hop selection. Two availability
accounts remain competing, and a dependency account remains explicitly unresolved.

Supply one to eight distinct exact concept IDs through repeated `--context`.
Titles and aliases are not selectors. An exact horizon is required: `--tick`
supplies signed i64 ticks, `--order` signed i32 order, and `--timeline` selects a
declared timeline. Order defaults to the end of the tick; timeline defaults to
the world's default. `--audience` and `--view` default to `author` and select
literal presentation labels.

The library operation is
`environment::context(world, roots, at, audience, view)`. The closed GET operation
is `/api/environment/context`, with a JSON array of exact IDs in `contexts`,
required `tick`, and optional `order`, `timeline`, `audience` and `view`.
Unknown parameters are refused. The browser exposes the same author read. These
surfaces use one immutable source snapshot per operation and the existing exact
integer wire codec. The live author server captures current source for each
canonical request; historical sessions keep their selected snapshot. Worktree
and Git reads retain their normal source revision, fingerprint and blob citations.

Missing, hidden, future, noncanonical and wrong-role roots share one unavailable
response. An available root with no admitted assertion or description still
produces its admitted concept and empty native evidence lists. Presentation
labels are not character evidence or an authorization grant.

## Direct links and record selection

Each selected root contributes only its own admitted direct relationships:

- Root arrays named `eras`, `dependencies`, `resources`, `habitats`,
  `technologies` or `infrastructures`, when their declared item type is entity.
- Explicit `concept_links` whose own native validity applies at the selected
  horizon.

Only admitted endpoints with one of the seven recognized roles enter the
selected record set. Locations remain ordinary typed values. Other-profile
references remain in the original concept projection without becoming context
records. Nested entity fields, other root fields, literal Ref-shaped objects and
empty literal arrays are not followed.

Direct endpoints supply their admitted names, values, types and citations. They
do not become additional traversal or evidence roots. Select an endpoint
explicitly to include its own direct links, assertion accounts and sections.
Self-links, cycles and duplicate edges stay authored statements; shared selected
records appear once.

Records and evidence owners are ordered by exact concept ID. Links retain each
root's existing reference order followed by its explicit concept-link order.
Every edge retains the original source field/index and both source and target
citations. Filtering an earlier array member never renumbers a surviving edge.
Unavailable endpoints contribute no placeholder, explanation or selected-record
count. A named era link does not supply an interval or perform calendar mapping.

## Evidence, unknowns and competing accounts

The result carries `protocol`, author perspective/scope, exact `at`, audience,
view, original root selection and selected-record `count`. Its payload contains:

| Field | Meaning |
| --- | --- |
| `records` | Role plus original admitted lore item, including its separate opaque body |
| `links` | Direct admitted typed/concept links with original source positions |
| `canon` | Original assertion result for each explicit root, under `{ownerId, result}` |
| `sections` | Original description result for each explicit root, under `{ownerId, result}` |
| `unknowns` | No admitted context account, or an explicitly unresolved account |
| `competingAccounts` | Different admitted literal values for a named context predicate |
| `hooks` | Exact admitted record/section targets with original current-source citations |

The two summarized predicates are `context-availability` and
`context-dependency-evidence`. A missing admitted cohort yields
`basis: no-admitted-account`, which is not evidence of falsehood. Each unresolved
account additionally yields `basis: explicit-unresolved` with its exact target.
Descriptions retain the native distinction between an unavailable owner and an
available owner with an empty admitted section list; an empty list is not an
inferred missing-description fact.

Different projected values for one subject/predicate form a
`different-literal-accounts` group. Comparison includes the used type shape, so
an enum number and string with identical wire text stay distinct. Nulls, empty
containers and customized finite predicate schemas remain safe literal values.
Identical values do not become competing merely because classification or
attribution differs. No truth winner or prose contradiction is inferred.

All classifications, attribution and unrelated predicates remain in native
`canon`. Only the two summarized predicates contribute assertion hooks. Explicit
assertion and description corrections use the existing admitted suppression
rules; separate forks remain separate. Dated descriptions retain their own
inclusive bounds and original Markdown. Neither body prose nor era/quantity
wording manufactures dates, events or historical cascades.

Historical assertion evidence remains syntax-checked source. Its old targets
and blobs are not authenticated or promoted to hooks; native author output has
`evidence: []` and cites the actual current assertion. See
[ASSERTIONS.md](ASSERTIONS.md) and [DESCRIPTIONS.md](DESCRIPTIONS.md).

Hooks identify source to inspect before a later reviewed edit. A clipped reader
projection is not a replacement source record. Hooks neither confirm edits nor
predict consequences. Explicit [retcon impact](RETCON_IMPACT.md) and
[saved review](RETCON_REVIEWS.md) remain separate author workflows.

## Completion bounds and remaining work

The reader refuses the whole result if completion exceeds eight explicit roots,
32 distinct selected records, 512 aggregate admitted assertion/description items,
or 262,144 bytes for compact UTF-8 JSON including one newline. Pretty CLI
formatting can occupy more bytes than that compact representation. Scope and
native correction filtering precede evidence and selected-record counts.
Unrelated catalogue records do not consume the selected-record bound. These
limits do not claim bounded total source-loading or validation work.

## Character environment context

```sh
wedl-rs --repo my-world character environment CHARACTER_ID --context CONTEXT_ID --belief-target ASSERTION_ID --tick 12 --order 0
```

The native entry point is
`environment::character_context(world, viewer, roots, belief_targets, at)`.
The closed GET route `/api/character/environment` requires `character`, `contexts`
and `tick`; it also accepts `beliefTargets`, `order` and `timeline`. Both lists
are JSON arrays. CLI repeats `--context` and `--belief-target`. Character mode
accepts no author audience/view or request-supplied known identities. The same
three capabilities remain required. The browser provides explicit character
controls separately from its author context read.

The reader computes the existing [character scope](NARRATIVE_ADMISSION.md) from
one captured world, viewer and exact horizon. Root selection never grants
knowledge. Every root, typed array endpoint and concept-link endpoint must
already be independently known and pass its own current scope, canonical status
and applicability. Admission precedes reference traversal, roles, counts and
limits. One-hop selection and explicit evidence owners remain exactly as
described above: direct endpoints never acquire their own traversal or section
streams unless requested as roots.

Character `records` contain admitted native values, used types, validity and
safe target identities without current source titles, untimed bodies,
provenance or citations. `links` contain only owner, root field, target and, for
concept links, the admitted relation and validity. They expose neither original
array positions nor source paths. Internal traversal positions are rebuilt from
projected values and removed before publication. Inserting an inaccessible
reference cannot change a surviving visible edge, count or result budget.
Literal Ref-shaped objects retain their literal strings without creating links.

`sections` contains native dated section results for the exact roots. `hooks`
contains admitted record/section targets without author citations. The character
result carries `perspective: character`, `scope: character-as-of-environment-context`,
`characterId`, exact `effectiveTime`, selection, records, links, sections, beliefs,
hooks and `complete: true`. It excludes the author `canon`, `unknowns` and
`competingAccounts` summaries: canonical predicates, classification, subject and
correction metadata cannot classify a character's independently held words.

Optional `beliefTargets` permits up to eight distinct exact assertion or
knowledge IDs. Each produces `{targetId,result}` in `beliefs`, with the original
native learned item shape: literal claim, selected transition, belief state,
numeric confidence, exact story time and empty citations. The selected targets
are independent of the environment roots. The reader never finds beliefs by
joining a canonical assertion's subject to a root, and never imports an
assertion's canonical value as its learned statement. Unknown, unlearned or
forgotten targets produce empty native item lists; this is not evidence that
the corresponding world assertion is false. An omitted target list adds no
belief stream and does not attach the viewer's whole learned history.

The 512-item evidence bound counts admitted dated sections and emitted selected
learned rows together. Eight roots, 32 selected records and the compact
262,144-byte report bound remain complete-result guards. A limit failure returns
no partial family result. Private/future source changes that leave admitted
evidence unchanged leave the complete character report unchanged, including its
ordering, identities and limits.

Exact [writing frames](WRITING_FRAMES.md) can explicitly add an environment family
using the same selections and the selected scene's admitted scope. Generic
character context/search retains its prior behavior and does not automatically
import typed family concepts. Explicit [semantic/hybrid retrieval](RETRIEVAL.md)
uses that same search corpus. Explicit action/proposal capture and
[retcon review](RETCON_REVIEWS.md) are separate native workflows; manuscript/reader
admission remains planned. Current local-author sessions support
[reviewed browser publication](BROWSER_PUBLICATION.md) through the existing writer.

## Pinned evidence

The implementation follows the author and explicit character-selection behavior in
[`narrative_context.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/narrative_context.py),
the [context guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/narrative-context.md)
and the [seven-profile data example](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/examples/environment-material-technology-profiles.json)
at the pinned comparison commit. Rust retains current profile IDs, complete
source-backed results, original reference indexes and existing snapshot APIs.
It does not reproduce upstream pagination or profile-version machinery.
`tests/environment.rs` covers author composition, evidence and bounds.
`tests/family_character.rs` and `tests/writing_family_context.rs` cover character
scope, separate learned targets, private-change invariance and complete frame
composition.
