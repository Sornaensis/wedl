# Authored propositions

Assertions record independently authored propositions over declared finite value
types. Their classification is explicit: `author-fact`, `attributed-claim` or
`unresolved`. Alternatives remain separate; the reader does not derive truth,
choose a winning account or teach a character. All project labels remain v1.

## Predicate declarations

The world can declare current predicates when `lore-core-v1` is enabled:

```yaml
predicates:
  - id: has-account
    subject_kinds: [character, lore-concept]
    value_type:
      type: string
```

Each declaration contains exactly `id`, `subject_kinds` and `value_type`. IDs are
unique lowercase tokens. Subject kinds are nonempty, unique known canonical
kinds; hypotheses are excluded. The value type uses the eight closed forms in
[LORE.md](LORE.md), including scalar, entity, array and object roots. It has no
version, default or executable expression. Full predicate replacement validates
all dependent assertion values, including retired, draft, historical and future
source. Removing a declaration cannot leave a dependent assertion unresolved.

A world `record update` replaces the complete `predicates` array while preserving
omitted world fields. Predicate declarations require lore even when the supplied
array is empty. Using an assertion additionally requires `assertions-v1`; ordinary
description sections remain an independent optional capability.

## Source records

An assertion has the ordinary `wedl/v1` record identity and common metadata:

```yaml
schema: wedl/v1
kind: assertion
id: assertion_00000000000000000000000001
title: An authored account
domain: harbor.practice
status: canonical
subject: {kind: record, id: char_00000000000000000000000001}
predicate: has-account
value: The harbor account describes a spoken invitation.
classification: author-fact
valid: {kind: timeless}
evidence: []
audience: [author]
perspectives: [author]
```

The referenced subject must exist and match the predicate's declared kinds.
`value` must be present and match its declared type; a declared null is a real
value. Only declared entity leaves become references. Ref-shaped ordinary
objects, strings and enum alternatives remain literal.

| Field | Rule |
| --- | --- |
| `domain` | Dotted identifier, at most 256 characters |
| `status` | Canonical by default; `canonical`, `draft` or `retired` |
| `classification` | Exactly `author-fact`, `attributed-claim` or `unresolved` |
| `attributed_to` | Required exactly for `attributed-claim`; a record reference to a character, organization or lore concept |
| `valid` | Required closed timeless or interval value, using exact declared story coordinates |
| `audience`, `perspectives` | Required nonempty, sorted, unique arrays of scope labels |
| `evidence` | Required array of closed historical citation syntax |
| `replaces` | Optional exact existing assertion ID with compatible subject, predicate and validity |
| `provenance` | Optional authored string |

Top-level chronology and the project's common metadata conventions remain
available. The record body stays opaque source prose. Optional explicit dated
sections use [DESCRIPTIONS.md](DESCRIPTIONS.md); ordinary body text is not
converted into dated evidence.

### Historical citation syntax

Each evidence item has exactly `revision`, `path`, `blob_oid` and `target`.
Revision/blob IDs are 40 or 64 lowercase hexadecimal characters. Paths are safe
repository-relative paths without empty, `.` or `..` components, backslashes,
colons or NUL. Targets are closed historical references:

- `{kind: record, id: NONBLANK_ID}`
- `{kind: section, owner_id: NONBLANK_ID, id: NONBLANK_MEMBER}`
- `{kind: transition, owner_id: NONBLANK_ID, id: NONBLANK_MEMBER}`

Citations must be unique. Historical targets need not exist in current source.
Validation checks syntax without accessing historical Git objects or files;
evidence is excluded from the live reference graph. Author query output therefore
uses `evidence: []` and cites the actual current assertion source instead of
claiming that old blobs or their disclosure have been authenticated. Character
learned claims use their own safe projection, described below.

### Corrections and independent alternatives

`replaces` retains the exact authored `subject`, `predicate` and `valid` values.
It can change the proposition value or classification, with attribution still
obeying its classification. The replacement graph must be acyclic across all
source statuses. Identifiers and titles do not establish edit order. Multiple
corrections of one source remain independent alternatives.

A changed time interval is a separate applicable proposition, not a compatible
correction. Corrections preserve old source; they do not rewrite an existing
record's body, state, dated description or character evidence.

## Author reads and projection

After applying the worked authoring examples:

```sh
wedl-rs --repo my-world assertions predicates
wedl-rs --repo my-world assertions list concept_00000000000000000000000009 --tick 5 --order 0
wedl-rs --repo my-world assertions list concept_00000000000000000000000009
```

Requests 88–97 supply three predicates; the 125-request checkpoint has six,
and the complete 207-request sequence has nine. All 207 requests leave 144 source
records, including the world, and 25 profiles. The practice read at 5/0 returns
four admitted accounts, including two separate author-fact corrections, an
attributed claim and an unresolved account. The all-time read retains all five
accounts, including the original proposition. These are explicit forks, not a
ranking of which correction should win.

The list target is an exact assertion ID or exact subject ID. Titles and aliases
are not resolved. An absent or inaccessible result is an empty list, meaning no
admitted account was supplied, never a false proposition. A direct read of a
superseded assertion at a horizon does not redirect to its correction.

A supplied tick selects exact inclusive applicability. Order defaults to the end
of the tick; timeline defaults to the world's default. Order/timeline without a
tick is invalid. With no horizon, all admitted authored timelines and replacement
history remain available. Audience and view default to `author`.

Admission proceeds before correction suppression and limits:

1. Assertions must be canonical, applicable and match their own audience/view.
2. Ordinary subject, value and attribution endpoints honor optional scope labels.
   Ordinary status or event time does not supply an extra endpoint interval.
   Lore concepts additionally retain their canonical status and explicit validity;
   assertion endpoints must be eligible assertions. Existence episodes require
   canonical status, applicable episode validity, matching own labels and an
   independently admitted character endpoint.
3. Typed projection removes unavailable entity leaves. An unavailable subject,
   attribution or root entity value withholds the whole assertion. Objects retain
   admitted fields and arrays retain admitted members, including empty `{}` or
   `[]`. Such a remaining object/array is an admitted proposition value and can
   supply a correction; projection does not require every raw reference visible.
4. At a horizon, only those admitted proposition payloads suppress their explicit
   predecessors. Without a horizon, both predecessor and correction remain.
5. Exact target matching and the complete-result limit apply to the remaining rows.

A missing/withheld predecessor is not exposed through a `replaces` field or its
reference citation. Other surviving references retain their original source
indexes, such as `value.links[1].id`, even when prior members were filtered.
Hypotheses are never canonical endpoints.

The result contains the author scope, exact horizon, caller target, count and
stable assertion-ID-ordered items. Each item retains subject, predicate,
classification, projected `value`, used `valueType`, validity, source citation,
live references and any admitted attribution/replacement or authored provenance.
Numbers use the existing exact integer-string wire format; enum used types and
predicate `enumValueTypes` distinguish numeric and string alternatives. Known
null and empty values remain distinct from a missing item.

Both predicate and assertion lists accept limits from 1 through 1000. Exceeding a
limit refuses the whole result. The finite source value bounds from lore also
apply. These are correctness guards, not a bounded-total-work performance claim.

| Surface | Operation |
| --- | --- |
| Library | `assertion::predicates(world, limit)`; `assertion::as_of(world, target, at, audience, view, limit)` |
| GET | `/api/assertions/predicates`; `/api/assertions/list?target=...` |
| Browser | Narrative tab, with author scope and optional exact horizon |
| Editing | Existing reviewed predicate replacement and assertion create/update requests |

## Explicitly learned claims

The existing reviewed knowledge writer accepts a separately authored claim with
exactly `key`, `statement` and `assertion_id`, under `lore-core-v1` and
`assertions-v1`:

```yaml
claim:
  key: ferry-report
  statement: Residents told me the ferry runs all night.
  assertion_id: assertion_00000000000000000000000017
```

The link must name a current assertion of the correct kind. It cannot be mixed
with ordinary `subject`, `predicate`, `object`, `truth` or typed `genealogy`
fields. Its `statement` is the character's literal learned wording. The reader
does not copy the canonical assertion's value, title, applicability,
classification, evidence, provenance, body or correction into that wording.

Optional root `audience` and `perspectives` arrays constrain all knowledge forms,
including linked claims. They contain unique nonblank strings without an ordering
requirement. Omission adds no constraint; an empty array admits nothing. Character
admission requires an audience of `public` or the exact viewer ID and a perspective
of `ordinary` or that ID, whenever the corresponding array is supplied. Callers
cannot enlarge those fixed label sets.

At a required exact horizon, linked-claim admission selects the viewer's own
canonical, scoped knowledge with an affirmative transition at or before that
horizon and a latest state other than `forgotten`. Rejection without earlier
affirmative learning supplies no claim. Rejection after learning retains the
literal claim and its explicit rejected state. Canonical expiry, retirement,
labels and replacement do not change what the character learned. Saving an
otherwise valid rejection-only record remains possible; it produces no learned
result. These rules are separate from the current affirmative-state and
applicability rules for [typed generational beliefs](GENERATIONAL_KNOWLEDGE.md).

```sh
wedl-rs --repo my-world character assertions char_00000000000000000000000001 assertion_00000000000000000000000017 --tick 12 --order 0
```

The native reader is `learned_assertions::list(world, viewer, target, at, limit)`;
GET uses `/api/character/assertions` with `character`, `target`, required `tick`
and optional `order`, `timeline` and `limit`. An exact assertion link or knowledge
ID selects learned rows; canonical subjects, titles and aliases are not resolved.
Unavailable and unlearned targets return empty items. The result contains literal
claim words, belief state, confidence and exact knowledge/transition identities
and time, with no canonical source metadata, acquisition notes or historical
citations. Admission and target filtering precede the complete-result limit.

Generic character context and search reuse this learned projection. Search
indexes its literal statement. Edits to the linked canonical assertion alone
cannot change the admitted words, rankings, counts or packet budget on valid
snapshots. Learning an assertion ID does not grant its canonical descriptions or
related concepts.
Known-owner description and lore reads follow the separate
[shared narrative admission rules](NARRATIVE_ADMISSION.md).

See [LEARNED_ASSERTIONS.md](LEARNED_ASSERTIONS.md) for the full source, held-state,
wire and error contracts. Requests 143–158 exercise these reads through the same
reviewed writer, including private and unlearned claims, forgetting, independent
canonical corrections and dated sections.

## Remaining boundaries and evidence

Historical Git citation authentication and reader/manuscript entitlement remain
later work. Current local-author sessions support
[reviewed browser publication](BROWSER_PUBLICATION.md). Explicit multi-record replacements can use
[reviewed outcome publication](OUTCOMES.md), subject to the existing typed
candidate-construction boundary; no belief change is inferred. Author
assertion reads and author knowledge reads retain their respective source-facing
metadata; selecting a character uses the separate learned projection.

The pinned implementation at `f0e885ccebc279eff65a5db2984d01ebc6647b2b` supplies
these contracts in
[assertions.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/assertions.py),
[assertion_runtime.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/assertion_runtime.py),
[the assertions guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/assertions.md)
and its source/runtime/entitlement/authoring tests. Rust keeps the current v1
source/wire conventions and reviewed single-record writer. `tests/assertion.rs`
covers author source and projection rules; `tests/learned_assertions.rs` covers
the separate learned contract. Narrative transport and authoring tests exercise
the shared interfaces, while `tests/admission_authoring.rs` checks the 16 new
requests, exact preview/save bytes and unchanged character results after private
or canonical-only edits.
