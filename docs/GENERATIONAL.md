# Generational histories and beliefs

The v1 implementation covers all eight canonical generational record kinds:
organizations, parentage, unions, affiliations, legacies, tenures, claims and
vital histories. Canonical reads are author views at an exact story horizon.
Separately authored learned assertions, labels, discovery, search and context
use their own admission gate; see [the learned contract](GENERATIONAL_KNOWLEDGE.md).
Neither path infers relationships, office holders, ages or deaths.

## Source declarations

The world must declare `generational-core-v1` before these records are saved.
Capabilities retain their established registry order; when used with the spatial
examples, the declaration is:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1]
```

All eight record kinds use the common `schema: wedl/v1`, `id`, `kind`, `title`, status
and Markdown envelope. Status defaults to `canonical`; explicit statuses are
`canonical`, `draft` and `retired`. Every status validates its history and exact
canonical references. Only canonical histories supply current facts.
Common metadata, top-level chronology annotations and existing opaque extensions
remain separate from executable history objects.

| Kind | Required domain fields |
| --- | --- |
| `parentage` | `child_id`, `parent_id`, `initialization`, `transitions` |
| `union` | `participant_ids`, `initialization`, `transitions` |
| `organization` | `organization_kind`, `initialization`, `transitions`; optional `parent_id`, `location_id` |
| `affiliation` | `character_id`, `organization_id`, `initialization`, `transitions` |
| `legacy` | `legacy_kind`, `initialization`, `transitions`; optional `organization_id` |
| `tenure` | `legacy_id`, `initialization`, `transitions`; optional `predecessor_tenure_id`, `successor_tenure_id` |
| `claim` | `legacy_id`, `claimant_id`, `initialization`, `transitions` |
| `vital-history` | `character_id`, `disclosure`, `initialization`, `transitions` |

Parentage uses a `kinship_` ID; unions use a `union_` ID. The suffix is 26 uppercase
Crockford characters, as for the other portable record IDs. Endpoints are exact
existing canonical character IDs, not titles or aliases. Parentage cannot name
the same child and parent. Union rosters contain 2–500 sorted unique character
IDs. The header roster is literal source metadata; the latest visible payload
roster determines membership and can differ from that header.

The other IDs use their kind as prefix, except `vital-history` uses `vital_`.
Organization kinds are `house`, `dynasty`, `clan`, `institution`, `other`; legacy
kinds are `office`, `estate`, `title`, `other`. Optional source references can be
absent/null; present IDs must name canonical records of the declared kind.

### History objects

Each initialization/transition requires exactly `transition_id`,
`transition_kind`, `applicability` and `payload`. Later transitions optionally
include `cause_event_id` and `replaces_transition_id`; initialization forbids
both. `transition_` IDs have the same 26-character suffix and are globally
unique across the eight generational record kinds, including noncanonical
records. A history has at most 1,000 entries including initialization.

Initialization and ordinary transitions use exact instant applicability:

```yaml
applicability:
  applicability_kind: instant
  point: {timeline: main, tick: 2, order: 0}
```

All three point fields are required. Timeline must be declared; tick is an exact
signed i64 integer and order a signed i32 integer. Strings, floating values,
tagged YAML and unknown fields are rejected. The sole interval transition is
`tenure-vacate`, with `applicability_kind: inclusive-interval`, exact `first` and
`last` points on the same timeline, and first not after last. Histories
increase on one timeline, with equal instants allowed only for explicit
corrections. Chronology annotations cannot appear inside initialization,
transitions, applicability, points or payloads.

| Transition kind | Prior lifecycle | Payload | Result |
| --- | --- | --- | --- |
| `parentage-initialize` | No prior entry | `{basis: biological}` or `{basis: adoptive}` | `asserted` |
| `parentage-confirm` | `asserted` | Same two permitted basis values | `confirmed` |
| `parentage-end` | `asserted` or `confirmed` | `{}` | `ended` |
| `union-initialize` | No prior entry | `{participant_ids: [...]}` | `declared` |
| `union-form` | `declared` | `{participant_ids: [...]}` | `formed` |
| `union-reconcile` | `formed` | `{participant_ids: [...]}` | `formed` |
| `union-end` | `declared` or `formed` | `{}` | `ended` |
| `union-annul` | `declared` or `formed` | `{}` | `annulled` |

Each roster payload obeys the same sorted unique 2–500 rule. Confirmation may
change parentage basis; reconciliation replaces the whole current union roster.
Ended and annulled states are terminal. Corrections amend a previous assertion;
there is no reopening transition.

A cause names an existing canonical event strictly earlier on the same timeline.
Its event time must contain the complete exact timeline/tick/order point. An
event at the same tick but earlier order qualifies; the same exact point does
not. A cause is evidence for the authored transition, not a generated event.

### Organizations, affiliations and succession

| Kind | Lifecycle and payload rules |
| --- | --- |
| Organization | Initialize active with `{title, aliases}`; rename while active/dormant; reparent with `{parent_id: ID-or-null}`; active may become dormant; either may dissolve terminally |
| Affiliation | Initialize active with `{role: string-or-null}`; change to a nonblank role while active; end terminally with `{}` |
| Legacy | Initialize active with `{title, aliases}`; rename while active/dormant; active may become dormant; either may dissolve terminally |
| Tenure | Initialize with `{holder_id: ID-or-null, basis: legal-or-de-facto}`; null means vacant, otherwise holding; vacant may designate a holder; vacant/designated may hold; designated/holding may vacate; holding may transfer; vacant/designated/holding may end |
| Claim | Initialize proposed with `{competes_with: [...]}`; proposed/disputed may dispute, recognize, withdraw or reject; the latter three outcomes are terminal |
| Vital history | Initialize unknown with `{}`; birth leads to living then death to dead, or existence-start leads to existing then existence-end to ended; no branch switching or revival |

Organization/legacy rename payloads require a nonblank title and sorted unique
nonblank aliases, at most 500. Empty alias arrays are valid. Header titles and
aliases remain current source selectors; historical names are returned in
`value.title`/`value.aliases`. A history rename does not rewrite its source header.
Dormancy permits rename/reparent but has no implicit reactivation. Dissolution
does not automatically end independently authored affiliations or tenures.

Tenure designation/holding requires an exact canonical character and an explicit
legal/de-facto basis. Multiple authored holdings remain separate; no claim or
priority rule selects a winner. Vacancy requires exactly `{holder_id: null}`.
It holds through the inclusive last point, then queries report `expired`; no
expiry transition is persisted and no later holding is inferred. Vacancy-interval
and expired states cannot acquire a later transition under this lifecycle table.

A transfer requires `{from_tenure_id, to_tenure_id}` and an earlier cause event.
The source must name itself and be holding. The canonical target must already
be vacant/designated strictly before the transfer, on the same timeline and for
the same legacy. A target initialized at that exact point does not qualify.
Transfer clears only the source holder; the target needs its own authored hold.
Predecessor/successor header links require the same legacy and strictly ordered
initialization times on one timeline. These links need not be reciprocal and do
not create transfers or appointments.

Competing claim lists are sorted unique canonical claim IDs, at most 500, and
cannot include the record itself. Each authored competition requires reciprocal
effective evidence for the same legacy at identical applicability. Superseded
source competitions remain subject to this consistency check. Recognition does
not appoint a holder or suppress other claims.

Vital `disclosure` is `known`, `unknown` or `withheld`. Unknown source permits
initialization only; withheld source may retain affirmative transitions but
typed vital/history reads return no affirmative state or history. Known records
before an affirmative transition remain unknown. Multiple known histories are
returned with their evidence: differing states set `ambiguous:true` and
`vital:null`, instead of selecting an arbitrary record. No civil date, duration,
age or death follows from these ordinal story points.

### Corrections and graph integrity

`replaces_transition_id` must target an earlier entry in the same record at the
identical applicability. Validation checks the correction against the target's
pre-transition lifecycle. Visible replacements supersede their targets; the
remaining entries fold in source order. Replacement chains and multiple leaves
that target one prior transition retain their literal evidence. All visible
entries retain their source ordinals and citations, with explicit superseded
flags and links. Future replacements do not alter an earlier horizon.

The canonical authored parentage graph must be acyclic across all histories,
including ended edges and independent timelines. Duplicate rejection uses the
exact child, parent, initial basis and initialization point. Different bases or
initialization points are not automatically duplicates. There is no two-parent
limit or inferred biological exclusivity. Draft/retired records validate but do
not enter this canonical graph.

Canonical organization header parents must be acyclic. Reparented containment
is evaluated at the query horizon; a cycle at that horizon fails the query.
All historical links are not combined into one graph, because successive detach
and reparent operations need not coexist. Canonical affiliations for the same
character and organization cannot overlap, irrespective of role. Their authored
intervals include initialization and effective ending, so touching endpoints
also conflict; corrections determine the effective ending.

## Queries

```sh
wedl-rs --repo my-world generational family "Ana Reed" --tick 2 --order 0
wedl-rs --repo my-world generational ancestors "Ana Reed" --tick 2 --max-depth 8
wedl-rs --repo my-world generational descendants "Rowan Reed" --tick 2
wedl-rs --repo my-world generational path "Ana Reed" "Rowan Reed" --tick 2 --max-hops 8
wedl-rs --repo my-world generational history kinship_00000000000000000000000001 --tick 10
wedl-rs --repo my-world generational organization "Harbor Guild" --tick 15 --include-former
wedl-rs --repo my-world generational organization-legacies "Harbor Guild" --tick 2
wedl-rs --repo my-world generational legacy "Harbor Keeper" --tick 12 --order 1
wedl-rs --repo my-world generational vital "Ana Reed" --tick 2
```

Selectors accept exact IDs or unambiguous titles/aliases. Family, lineage and
path/vital endpoints must resolve to canonical characters; history accepts all
eight kinds. Every command requires `--tick`; `--timeline` defaults to the
world's default, and omitted `--order` is i32 maximum, meaning the end of that
tick. Supply order explicitly to inspect an exact same-tick boundary. Public
ticks, orders and path/generation distances are decimal strings.

| Query | Returned meaning |
| --- | --- |
| `family` | Active direct parent and child records plus unions whose current roster includes the character |
| `ancestors` | One shortest cited path to every reachable parent/ancestor |
| `descendants` | One shortest cited path to every reachable child/descendant |
| `path` | A shortest continuous path over active parentage edges in either direction |
| `history` | Visible entry projections, corrections, citations and the folded lifecycle/value |
| `organization` | Historical identity, cited parent paths and active affiliations; `--include-former` adds ended affiliations |
| `organization-legacies` | Initialized canonical legacies explicitly linked to the organization |
| `legacy` | Historical identity, all visible tenures/claims, current holders and explicit succession links |
| `vital` | All known affirmative histories for a character, with explicit uncertainty if their states differ |

Asserted/confirmed parentage and declared/formed unions are active, inclusively
at their authored instant. An ending removes a record from current family
results at that same instant but remains available in history. Different
timelines do not compare. History before initialization, on another timeline or
for a noncanonical record reports `available:false` with a specific reason and
no visible entries. Unknown/withheld vital history reports `undisclosed`.
Dissolved organizations/legacies and ended tenures/claims remain historical
results; their lifecycle does not erase independently authored records.

Union memberships never supply path edges. A path from a character to itself
has distance `"0"`. Equal-length parentage paths choose the lexicographically
first record-ID sequence; lineage items are ordered by target ID. Source
citations retain record IDs, source paths, revisions/fingerprints, exact field
positions, transition IDs and replacement/cause links. The query envelope uses
`perspective:author`, `scope:author-as-of-generational-source` and the selected
`effectiveTime`. Its `complete:true` describes that result, not whole-project
validation coverage.

Folded values and visible history payloads omit references to generational
targets that are noncanonical, on another timeline or not yet initialized.
Competition arrays are filtered by the same rule before query limits. A hidden
reference is omitted, not changed to an authored null. Source bytes and source
citations remain intact; `recordTitle` and selector names are current author
metadata, while the historical value is explicitly dated.

### Bounds

`--limit` defaults to 100 and accepts 1–1,000. It bounds the combined family row
count, lineage discoveries, visible entries/citations for each returned history
or edge, and each returned union roster. Path traversal budgets newly discovered
characters excluding the start, so its `visitedCount` may be `limit + 1`.

Organization containment uses `--max-depth` with the same complete-result rule.
Its combined current/former affiliation rows and parent-path collection each
have the query limit. Legacy tenure, claim and succession collections each have
the limit; its `count` combines tenures and claims and may therefore exceed it.
Reverse-legacy and vital reads bound their admitted results. Nested aliases,
competition arrays and returned history arrays are bounded individually. Filtered
noncanonical, future or undisclosed rows do not consume the returned-result limit.

Depth and hop bounds default to 8 and accept 0–32. They are completeness guards,
not truncation requests. A leaf lineage can succeed with depth zero and a self
path with zero hops. If undiscovered reachable nodes remain beyond a bound,
the query fails instead of reporting a partial lineage or false no-match.
A found path is already shortest and needs no search beyond that result.

These bounds control returned collections and traversal. The current
implementation validates authored histories and builds the active graph before
traversal; it does not claim indexed or bounded-total-work performance parity.

### HTTP and browser

The same operations are GET `/api/generational/family`, `/ancestors`,
`/descendants`, `/path`, `/history`, `/organization`, `/organization-legacies`,
`/legacy` and `/vital` under the `/api/generational` prefix.
Family/lineage use `character`, path uses `from` and `to`, and history uses
`record`. All accept `tick`, optional `order`, `timeline` and `limit`;
lineage additionally accepts `max_depth`, and path `max_hops`. Organization uses
`organization`, `max_depth` and optional `include_former:true|false`; its reverse
legacy read uses `organization`; legacy uses `legacy`; vital uses `character`.
Unknown, duplicate or incompatible parameters fail. The browser Generations tab
uses existing exact-string horizon controls and shortcuts from Browse results.

## Editing and remaining scope

`record create` and `record update` accept all eight kinds through the existing
fingerprint/hash-bound preview and atomic one-file save. New histories default
to canonical status and an empty `transitions` array. Updates preserve omitted
fields/prose; supplied initialization and transitions replace their whole
values. Semantic errors produce reviewable `canApply:false` candidates. The
writer never enables capabilities automatically or supplies a missing parent,
partner, cause or belief.

Nulls are supported at their declared history payload positions, including an
unspecified initial role, vacant holder and explicit reparent-to-root. Top-level
null is still not a deletion operation. Reciprocal claim edits can require
coordinated changes to two source records; the one-record writer refuses either
invalid half. Such edits remain direct coordinated source changes until the
planned reviewed batch workflow is implemented.

[Requests 21–35](../examples/authoring/README.md) add characters, all eight kinds,
endings, a historical rename and an inclusive vacancy. [AUTHORING.md](AUTHORING.md) specifies
the writer contract. Restart the read-only snapshot server after edits.

Separately learned generational assertions, labels and admitted discovery,
search and context are implemented through the existing knowledge record kind.
Author labels, discovery, structural search and character-subject context are
also available; [GENERATIONAL_KNOWLEDGE.md](GENERATIONAL_KNOWLEDGE.md) specifies
these surfaces, exact admission rules and their query bounds. Canonical queries
never substitute for learned evidence. The upstream comparison
is pinned to `Sornaensis/wedl@f0e885ccebc279eff65a5db2984d01ebc6647b2b`; Rust uses
one shortest lineage path per target and flat cited histories, with no promise
of exact upstream wire, diagnostics, path multiplicity or SQL-index behavior.
Vital query ambiguity is explicit instead of taking the first matching state.
