# Learned generational assertions

The v1 implementation keeps a character's explicitly authored generational
beliefs separate from canonical histories. Beliefs may be mistaken, contradictory
or cyclic. Validation checks their literal shape, references, learning and
evidence; it never changes a belief to agree with canonical parentage, membership,
office holding or vital status. Canonical histories never grant knowledge.

## Source contract

Use the existing `knowledge` record kind and explicitly declare both capabilities
in the world. With the numbered spatial examples, registry order is:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1, generational-knowledge-v1]
```

A typed claim contains exactly `key`, `statement` and `genealogy`. The first two
are nonblank author-facing strings. They do not enter learned results or character
search. A genealogy object requires `kind`, `payload` and `valid`, with optional
`labels` and `evidence`. Its nested objects are closed to undeclared fields.

```yaml
kind: knowledge
id: know_00000000000000000000000002
title: Ana suspects Jules is a parent
knower: char_00000000000000000000000001
claim:
  key: genealogy.parentage
  statement: Ana suspects Jules is a parent.
  genealogy:
    kind: parentage
    payload:
      child_id: char_00000000000000000000000001
      parent_id: char_00000000000000000000000003
      basis: biological
    valid:
      from: {timeline: main, tick: -40, order: 0}
      until: {timeline: main, tick: 8, order: 0}
    labels:
      char_00000000000000000000000001: Annie
      char_00000000000000000000000003: Jules the Elder
transitions:
  - id: learned-parentage
    time: {timeline: main, tick: 4, order: 0}
    state: suspected
    acquisition: told
    confidence: 0.6
```

The full request in [example 36](../examples/authoring/36-learned-parentage.yaml)
passes through the existing reviewed writer. It does not alter the canonical
Rowan/Ana parentage in example 23.

| Assertion kind | Exact payload fields | Literals and nullable fields |
| --- | --- | --- |
| `parentage` | `child_id`, `parent_id`, `basis` | Character endpoints; `biological` or `adoptive` |
| `union` | `participant_ids`, `state` | 2–32 sorted unique character IDs; `formed`, `ended`, `annulled` |
| `organization` | `organization_id`, `parent_id` | Organization endpoints; parent may be null |
| `affiliation` | `character_id`, `organization_id`, `role` | Role is null or nonblank text of at most 200 Unicode scalar values |
| `tenure` | `legacy_id`, `holder_id`, `basis` | Holder is a character ID or null; `legal` or `de-facto` |
| `claim` | `legacy_id`, `claimant_id`, `state` | Character claimant; `proposed`, `disputed`, `recognized`, `withdrawn`, `rejected` |
| `vital` | `character_id`, `state` | `living`, `dead`, `existing`, `ended` |

Every nonnull endpoint is an exact existing ID of the declared kind. Endpoint
existence is a source-validation requirement; querying a learned endpoint does
not inspect that endpoint's canonical facts or titles. A typed claim cannot also
carry plain-claim subject/predicate/object/truth fields or a linked
`assertion_id`. Labels map at most 32 asserted endpoint IDs to nonblank literal
names, each at most 200 Unicode scalar values. They cannot label an unasserted
entity. [Linked learned assertions](LEARNED_ASSERTIONS.md) use a separate closed
claim and their own admission rules.

`valid.from` and optional `valid.until` contain exactly timeline, i64 tick and
i32 order. Both bounds are inclusive and share a declared timeline; until cannot
precede from. Omit until for an open end; null is not a validity point. The three
declared payload nulls are data, not deletion commands.

### Knowledge scope labels

Every knowledge form, including typed genealogy, may supply optional root
`audience` and `perspectives` arrays. These scope labels are separate from the
literal names in `claim.genealogy.labels`. Each array contains unique nonblank
strings without an ordering requirement. Omission adds no constraint; an empty
array withholds the record. Null, scalar, duplicate, blank and nonstring entries
are invalid source and cannot pass the reviewed writer.

For a selected character, a supplied audience must intersect `public` or the
exact character ID, and a supplied perspective must intersect `ordinary` or that
ID. Both constraints apply before learning, names, evidence, counts, traversal,
search or packet budgets enter a character result. Request labels cannot grant a
wider view. Author knowledge reads retain their source-facing behavior.

## Learning and applicability

Knowledge transitions keep the existing Rust grammar: unique nonblank member
IDs; strictly increasing points on one timeline; a supported state, acquisition
and finite confidence in [0,1]. A learning point may omit timeline/order, using
the world's default timeline and order zero. The acquisition vocabulary remains
`authored`, `experienced`, `heard`, `inferred`, `observed`, `read`, `remembered`,
`tested`, `told`. An optional causing event must be canonical and no later than
the transition on that timeline.

A typed record must have an affirmative transition: `accepted`, `suspected`,
`uncertain` or `remembered`. Its earliest affirmative transition is `learnedAt`.
Learning and applicability use the same timeline but need not start together.
In the example, Ana learns at tick 4 an assertion about the interval beginning
at tick −40. Nothing is admitted before tick 4/order 0.

At a query horizon, admission selects only the viewer's canonical knowledge
that passes its optional scope labels, with visible first learning and a latest
visible affirmative state. Rejection or forgetting removes the assertion from
current learned views. Normal relation queries additionally require the horizon
to be inside the inclusive validity interval. Held-history reads ignore that
interval while retaining the same scope, learning and latest-state requirements.
They are history currently held by the character, not a dump of rejected or
forgotten records. Linked `assertion_id` claims may retain a rejection after
affirmative learning; that separate rule does not alter generational admission.

An admitted row preserves its literal value, labels, validity, earliest learning,
latest visible transition ID/time, belief state and confidence. `uncertain` is
true for a state other than accepted or confidence below one. Contradictory
assertions remain separate rows; a vital result reports its distinct states and
explicit conflict rather than choosing one as truth.

## Evidence references

Evidence is an optional list of at most 32 unique exact references. Each item
contains only `kind`, `entity_id` and the corresponding member field:

| Kind | Entity | Member field | Admission at first learning |
| --- | --- | --- | --- |
| `knowledge` | Own canonical knowledge admitted by its root scope labels | `transition_id` | Referenced affirmative transition is already visible and latest state is held |
| `observation` | Active/closed scene | `observation_id` | Observation was visible to the knower at its origin, no later than learning |
| `turn` | Active/closed conversation | `turn_id` | Speech was audible to the knower at its origin, no later than learning |
| `recollection` | Active/closed conversation | `recollection_id` | Exact latest own remembered recollection at learning |

Member IDs retain the existing nonblank Rust knowledge/scene grammar. An expired
observation can remain provenance for a later belief because its original
visibility is checked. Evidence does not infer a belief, verify the literal's
truth or copy source prose. Returned evidence contains only kind, exact entity
and member IDs, and the admitted time. It never follows an evidence reference
to expose otherwise unavailable transcript or canonical source metadata.

A knowledge proof must pass the same audience/perspective checks for the knower
as the learned record itself. Historical transition selection does not bypass
those root labels. Changing a used proof to an author-only or empty scope makes
the dependent typed source invalid; the reviewed writer refuses that candidate
until the dependency is repaired. This validation does not copy the proof's
statement or grant access to any canonical endpoint.

## Learned query surfaces

The CLI namespace is `generational learned VIEWER_ID OPERATION`; GET routes use
`/api/generational/learned/OPERATION?viewer=VIEWER_ID`. The viewer is always an
exact character ID. Relation subjects accept exact typed IDs or an unambiguous
held learned label through the service; direct relation library functions accept
IDs. Canonical titles and aliases never participate in learned name resolution.
An unknown or ambiguous learned label fails without listing hidden candidates.

| Operation | Result |
| --- | --- |
| `assertions` | Applicable assertions, optionally filtered by kind or endpoint |
| `history` | Currently held assertions involving the selected endpoint, including inapplicable intervals |
| `family` | Explicit parent/child assertions and asserted union memberships |
| `ancestors`, `descendants` | One deterministic shortest witnessed path per reachable character |
| `path` | One deterministic shortest undirected parentage witness |
| `organization` | Asserted parents, affiliations and witnessed parent paths |
| `legacy` | Asserted tenures and separate claims; no inferred succession |
| `vital` | All admitted states and explicit conflict/uncertainty |
| `labels`, `discover` | Explicit held names with their belief provenance |
| `search` | Literal assertion-kind and learned-name word-prefix matches |
| `context` | Complete character-subject family, lineage, vital and held-history packet with relevant learned labels |

```sh
wedl-rs --repo my-world generational learned char_00000000000000000000000001 family Annie --tick 4 --order 0
wedl-rs --repo my-world generational learned char_00000000000000000000000001 history Annie --tick 8 --order 1
wedl-rs --repo my-world generational learned char_00000000000000000000000001 search Jules --tick 8 --order 1
```

The Generations browser view explicitly selects author or character perspective
and reuses the exact-string tick/order controls. Learned transport envelopes
contain protocol and result only. They omit global fingerprints, revisions,
paths, blob hashes, source indexes and canonical titles. Filtering happens before
limits, counts, ranking and packet serialization. The unchanged-response
guarantee applies to valid snapshots with the same admitted evidence and horizon;
it is an author-selected projection, not authentication or a timing guarantee.

### Learned names and bounds

An assertion's labels remain available in labels/discovery/search after its
applicability expires, while the assertion remains held. Rejection or forgetting
removes those labels unless another held assertion also supplies them. Multiple
names and conflicting assignments retain separate provenance. A shared label
for different entities is ambiguous; several labels for one entity are allowed.
There is no fallback to canonical names and no global pre-admission candidate cap.

Result limits are 1–1000, default 100; depth/hop limits are 0–32, default 8.
Queries fail when their complete result or traversal exceeds a bound. Paths use
knowledge-ID ordering for deterministic equal-length witnesses and can traverse
only applicable asserted edges. They never bridge through canonical relations.
Cycles in beliefs are allowed; traversal visits each endpoint once.

Discovery uses a lowercase-normalized prefix of at most 64 Unicode scalar values and
a declared name kind (character, organization, legacy or event). The current
seven assertion payloads supply character/organization/legacy labels. Search
uses 1–8 literal word prefixes of at most 32 Unicode scalar values each, with at
most 512 input characters. All prefixes must match; results sort by knowledge ID.
No ordinary source prose, author statement, global FTS index or opaque metadata
enters this corpus. Generational context budgets are 80–1000000 Unicode scalar
values over compact inner JSON, default 12000 through the interfaces. Matching
uses Unicode lowercase conversion, without full Unicode case folding. A packet
overflow fails instead of dropping evidence.

## Existing knowledge and character views

Ordinary author `knowledge` reads retain typed claims and their full author
metadata, with exact decimal-string applicability coordinates. Generic author
context therefore continues to work when typed claims are present.

Generic `character context` and `character search` also admit applicable typed
assertions through the shared gate. They never treat typed `claim.statement` as
ordinary learned prose. Search indexes only assertion kind and explicit labels,
using the existing admitted-only FTS behavior; hits identify
`kind: generational-belief`. These generic operations require current applicability.
Dedicated generational history/name/search reads retain held expired assertions
as described above. Ordinary plain beliefs also honor their optional knowledge
scope labels; their existing latest-state rules and conversation behavior remain.

The [shared narrative scope](NARRATIVE_ADMISSION.md) can admit a typed knowledge
record's own explicitly dated sections while that genealogy claim is currently
held and applicable. Its payload endpoint IDs and learned labels do not acquire
canonical records or their sections. The same scope handles ordinary entity
acquisitions and [linked assertion claims](LEARNED_ASSERTIONS.md), keeping each
form's learning rules separate. Generic context/search can therefore combine
admitted genealogy, linked learned words and dated sections without joining
canonical genealogy or proposition values.

## Canonical discovery and context

Author `generational labels`, `discover`, `search` and `context` complement the
existing nine canonical reads. Their names are backed by visible canonical
histories at the horizon. Organization/legacy labels use historical titles and
visible effective names; character/event names require a visible fact involving
that entity. Future history and unknown/withheld vital evidence supply no names.
Structural search matches only visible history literals with citations. The
character-subject context composes family, ancestor, descendant and disclosed
vital results with relevant labels. These remain author views with source
metadata; selecting a horizon does not make an author view private.

## Editing and comparison boundary

Requests [36–44](../examples/authoring/README.md) create all seven assertion kinds,
reject the first parentage and create a replacement belief. They share the
fingerprint/hash-bound, validated single-record writer. A typed request requires
its affirmative transition; an empty default list yields a reviewable invalid
candidate. Updates replace supplied claim/transitions fields and preserve omitted
source metadata, annotations and prose. Reject an old belief and create a new
record to preserve what was believed earlier. There is no automatic multi-file
replacement transaction.

Requests 143–158 extend that writer with scoped linked claims and shared
narrative admission. Requests 159–175 add required briefs and exact writing
examples; 176–203 add family acquisitions and independently learned reports.
Requests 204–207 add a public retrieval prior and separate learned evidence.
The complete 207-request sequence leaves 144 source records,
including the world, with 25 profiles and nine predicates. The original
generational requests retain their rejection, held-history and applicability
behavior. `tests/admission_authoring.rs` also verifies ordinary and typed
knowledge scope edits, preservation of omitted source, and refusal to hide a
still-used historical knowledge proof.

The comparison remains pinned to
[`Sornaensis/wedl@f0e885ccebc279eff65a5db2984d01ebc6647b2b`](https://github.com/Sornaensis/wedl/tree/f0e885ccebc279eff65a5db2984d01ebc6647b2b).
Rust retains its current knowledge transition grammar, shortest-path convention,
complete-or-error bounds and metadata-free character responses. It does not
reproduce upstream source versions, paginated SQL projections, every simple path,
wire shapes or diagnostics. Overall validation remains `complete:false`. Linked
learned assertions, typed lore reads and shared narrative admission are
implemented as documented above; richer character context composition,
reader/manuscript entitlement and reviewed batch workflows remain in the plan.
