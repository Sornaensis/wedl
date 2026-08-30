# ADR 0005: First-class generational history and coordinated v0.7 contract

- **Status:** Accepted (ratified by the current project owner for downstream implementation)
- **Owner:** Generational-history ADR owner
- **Ratification:** The current project owner explicitly approved this exact proposal on 2026-08-30 after independent review. The reviewed contract artifact was SHA-256 `F591B44E9BD086F62E78A474E3F8ACC5730856CB2378023B84BB6C2B97881D93` at Git baseline `89c703b8b0ff76ba4679a9d7903effd4fc5fae05`.
- **Decision examples:** [generational-schema-v07.yaml](examples/generational-schema-v07.yaml), [generational-query-v1.yaml](examples/generational-query-v1.yaml), and [generational-migration-v07.yaml](examples/generational-migration-v07.yaml)
- **Depends on:** [ADR 0002](0002-shared-world-concurrent-narrative-threads.md), [ADR 0003](0003-calendar-and-historical-chronology-semantics.md), and accepted [ADR 0004](0004-spatial-domain-query-and-version-contract.md)
- **Approval gate:** satisfied by the current project owner's recorded ratification above; implementation and migration remain limited to their independently assigned task scopes.

## Planning correction and boundary

The former task acceptance cited `d76aacb1-671c-458e-b6a3-ce132c82c83a`
and `1910b6ab-b0ec-4dae-8263-e54f570687df`.  Both identifiers were checked as
an hmem observation, project, and task and returned 404; they contain no
recoverable decision content.  This ADR is a prospective correction, not a
recreation, inference, or overwrite of that missing memory.  Its evidence is
the committed repository: ADR 0002 at `36f3fc9b6c1eae4d10eb719d77b27fc1f1fa9390`,
ADR 0003 and the chronology contracts at
`797688d9cab694e5c9099f48719630fe4fb659b4`, and ADR 0004 at
`89c703b8b0ff76ba4679a9d7903effd4fc5fae05`.

This is a contract and test-vector decision only.  It is not a parser,
runtime, canonical story source, SQLite schema, compiler, query implementation,
API, UI, fixture conversion, or migration execution.  Markdown and Git remain
canonical; a future SQLite index is disposable and fingerprint-rebuilt from
normalized source.  No existing world gains organizations, ancestry, a vital
history, a claim, spatial geometry, or a converted schema by this ADR.

The compatibility baseline is the shared-world source contract in
[`THREAD_SCHEMA_CONTRACT.md`](../THREAD_SCHEMA_CONTRACT.md) and the active
chronology transaction contract in
[`CHRONOLOGY_MIGRATION_CONTRACT.md`](../CHRONOLOGY_MIGRATION_CONTRACT.md).
ADR 0005 neither redefines their v0.5 thread membership nor their v0.6
chronology semantics; the one later `upgrade-v07` preserves both exactly.

## Shared world, time, and identity

There is one shared world, one global signed, exact, ordinal,
timeline-scoped, unitless `StoryTime` tuple `(timeline, tick, order)`, and one
canonical fact set.  It is the ordering, causality, applicability, and horizon
key for every record below.  Threads remain grouping only under ADR 0002:
they do not fork people, organizations, a world state, truth, a clock, or a
corpus.  Calendar labels remain display/evidence under ADR 0003 and never
replace StoryTime applicability.

Source uses signed integer `tick` and `order`.  Public transport uses the
same tuple with canonical signed decimal strings (for example `"-7"` and
`"0"`); numeric JSON, a `+` prefix, and noncanonical leading zeroes are
invalid.  A reference is an opaque stable ID, never title matching, a path
guess, a relationship guess, or a nearest-name result.  Future source records
use lowercase stable IDs and paths:

| Kind | Prefix | Canonical path |
| --- | --- | --- |
| organization | `organization_` | `organizations/<id>.md` |
| parentage | `kinship_` | `kinships/<id>.md` |
| union | `union_` | `unions/<id>.md` |
| affiliation | `affiliation_` | `affiliations/<id>.md` |
| legacy | `legacy_` | `legacies/<id>.md` |
| tenure | `tenure_` | `tenures/<id>.md` |
| claim | `claim_` | `claims/<id>.md` |
| vital history | `vital_` | `vitals/<id>.md` |

Each new ID is `<prefix><26 Crockford Base32 characters>`, immutable after
creation.  Character, location, event, thread, and timeline references retain
their already-declared stable IDs.  IDs and paths are provenance, not display
text; public views resolve a safe name or title and never expose an opaque ID
as the primary label.

## Canonical record contract

The literal frontmatter vectors are the only canonical source model. Unknown
fields, discriminators, endpoint kinds, causes, timelines, or audiences are
invalid rather than best effort. `audience` and `perspectives` are nonempty,
sorted, duplicate-free opaque-ID lists; `public` and `ordinary` never trust a
caller. A time value is exact `{timeline,tick,order}`. An applicability value
is closed: `static`, `instant` with `point`, or `inclusive-interval` with
inclusive `first` and `last`; `first` cannot follow `last`.

Each literal source record has one `initialization` and an append-only
`transitions` list. Both carry a stable `transition_id`, closed
`transition_kind`, applicability, and an exact payload schema. Initialization
kinds end in `-initialize`; ordinary transitions use the remaining closed kinds.
The schema vector enumerates every payload—including an explicit empty object
where a kind takes no fields—and rejects extra or missing fields. They sort by
point, source ordinal, and transition ID. A correction appends a
transition with `replaces_transition_id` for the same record and exact
applicability value; the
replacement wins the fold but its superseded citation remains visible. No
domain phase, lifecycle, interval, or payload is rewritten in place.

The closed transition vocabulary supplies all domain progression: organization
initialize/rename/reparent/dormant/dissolve; legacy initialize/rename/dormant/dissolve; parentage initialize/confirm/end;
union initialize/form/end/annul/reconcile; affiliation initialize/role/end;
tenure initialize/designate/hold/vacate/transfer/end; claim initialize/dispute/recognize/
withdraw/reject; and vital initialize/birth/death/existence-start/existence-end.
Rename payloads are exactly `{title,aliases}`. A cause is an optional, extant
`event_` endpoint of type `event` on the same timeline and strictly before its
transition point; a replacement is an extant same-record, same-point transition.
Tenure vacancy is a literal `tenure-vacate` inclusive applicability interval,
not a duration derived from instant transitions. Its initialization is an actual
legal or de-facto holder; `tenure-vacate` changes that folded holding to vacancy
only from its inclusive first point through its inclusive last point. Before the
first point the predecessor holding folds normally; after the final point the
vacancy expires and current-holder folds only other literal holding tenures at
the query point. Its inclusive final point is before the literal co-holder
points, so current-holder output cites separate legal and de-facto holding
records. No transition infers legitimacy, inheritance,
genetics, residence, geography, access, or a successor.

At a point strictly after that interval's inclusive final point, the closed
fold operation `tenure-vacate-expire` changes its derived interval state to
`expired`; it is not an authored transition and may not be persisted. The
operation removes the vacancy from the current-holder fold, which then selects
only other literal holding tenures at that query point. The decision vectors
give closed before/inside/after results.

The schema vector is normative for folding: it supplies a separate closed
state/from/to table for organization, parentage, union, affiliation, legacy,
tenure, claim, and vital history. Each record has exactly one kind-matching
initialization; nullable initialization values are permitted only by that
kind's payload schema. All initialization, transitions, and interval endpoints
belong to one record timeline and are monotonic by applicability sort start.
Equal applicability is legal only for an explicit replacement, ordered by
source ordinal and transition ID. A second initialization, cross-timeline or
backward transition, transition from a terminal state, or any unlisted
from/to pair is invalid. A tenure transfer has literal source and target tenure
IDs plus an earlier same-timeline event cause; it leaves the source vacant and
does not infer that the target holds until a literal target `tenure-hold`.

Every generational source file composes this transition payload with the actual
WEDL frontmatter envelope: `schema: wedl/v0.7`, a reserved `kind` discriminator,
stable `id`, human `title`, dotted `domain`, common source `status`, `tags`,
`aliases`, and persisted `threads`. `kind` is exclusively a record discriminator
(`organization`, `parentage`, `union`, `affiliation`, `legacy`, `tenure`,
`claim`, or `vital-history`); organization and legacy classifications use the
separate closed `organization_kind` and `legacy_kind` fields. `status` is the
ordinary WEDL source lifecycle (`canonical`, `draft`, or `retired`) and is not
a domain state. `threads` follows `THREAD_SCHEMA_CONTRACT.md`: it is a sorted,
unique list of declared thread IDs and changes no domain fact or StoryTime.
All applicability points use the chronology-compatible `{timeline,tick,order}`
shape. The schema vector includes literal frontmatter
examples, not a synthetic payload-only format.

### Organizations and affiliation

An `organization` has `id`, `organization_kind`, `title`, optional `parent_id`, optional
`location_id`,
`audience`, `perspectives`, initialization, and authored transitions.
`organization_kind` is one of `house`, `dynasty`, `clan`, `institution`, or
`other`; the applicable organization transition kinds are the closed vector
vocabulary. `parent_id` is a single
organization reference and its containment graph is acyclic.  A nested
organization does not inherit members, roles, claims, location containment,
geometry, access, routes, or visibility.

An `affiliation` has `id`, `character_id`, `organization_id`, initialization,
transitions, audience, and perspectives. Its role is only an
`affiliation-role` payload. It is authored history, not kinship,
residence, employment, rank, allegiance, a title, or evidence of an inherited
right.  Multiple compatible affiliations may overlap only when expressly
authored; duplicate compatible affiliation history is invalid.

### Parentage and unions

A directed `parentage` record has `id`, `child_id`, `parent_id`, initialization,
transitions, audience, and perspectives. `basis` appears only in the closed
initialization/confirmation payload and is exactly `biological` or `adoptive`.
A record cannot point to itself, duplicate the same
canonical child/parent/basis/applicability edge, or create a cycle in the
combined canonical parentage graph.  A child may have multiple authored
parents and more than one basis.  Parentage neither establishes genetics,
legitimacy, inheritance, custody, residence, union membership, a surname, nor
a preferred parent.

A `union` has `id`, a sorted unique `participant_ids` list of two or more
character IDs, initialization, ordered transitions, audience, and perspectives.
A union may have three or more
participants.  Its participants must not include duplicates and its transition
history is monotonic on one timeline.  A union does not infer parentage,
fidelity, co-residence, gender, legality, affiliation, or a future successor.

### Legacies, tenure, and claims

A `legacy` names an authored office, estate, title, or other continuing
identity. It has `id`, `legacy_kind` (`office`, `estate`, `title`, or `other`),
`title`, optional `organization_id`, initialization, transitions, audience,
and perspectives. A legacy is
not itself a holder, a right, or a line of succession.

`tenure` records factual holding history: `id`, `legacy_id`, initialization,
transitions, optional predecessor/successor tenure references, audience, and
perspectives. `holder_id` and basis (`legal` or `de-facto`) occur only in
closed tenure payloads. Vacancy uses `holder_id: null` in a vacate payload and is
explicit.  Co-holding is multiple contemporaneous, expressly authored
`holding` tenures for the same legacy; it is never collapsed into one holder.
Predecessor and successor references are directed, compatible, and
time-ordered; they do not choose an heir or fill a gap.

`claim` records a proposed, disputed, recognized, withdrawn, or rejected
assertion: `id`, `legacy_id`, `claimant_id`, initialization, transitions,
audience, and perspectives. Claims may overlap, compete, and remain unresolved
independently of tenure.  Claims are not a holding and tenure is not a claim.
`competes_with`, when present, is a sorted, unique, non-self claim-ID list;
every cited claim must reciprocally cite this claim and refer to the same
legacy at the same transition point. It supplies no priority, winner, or legitimacy
conclusion.
The contract forbids legitimacy scoring, inheritance execution, genetic
inference, automatic vacancy fill, priority ranking, or successor inference.

### Vital history and privacy

A `vital_history` has `id`, `character_id`, `disclosure`, initialization,
ordered closed transitions, audience, and perspectives. `disclosure` is
`known`, `unknown`, or `withheld`; the vital transition vocabulary is
birth/death/existence-start/existence-end. Histories are monotonic on one
timeline. `unknown` has no affirmative vital transition. `withheld`
does not become caller-provided audience data: a caller cannot select another
audience or perspective to discover it.

Author-as-of filters in this exact order: authenticated viewer identity;
server-derived audience and perspective; protocol capability; selected
timeline; horizon; record audience; record perspective; applicability; then
ordering, pagination, serialization, and derived traversal. Caller-supplied
audience or perspective fields are invalid. Author-all-time is an explicit
separate mode. Character mode admits only authored, accessible knowledge and
applies the same filtering before any relationship, vital, or search result is
derived. It must not turn an absent, future, secret, or inaccessible transition
into a statement that a person lives, exists, died, is related, holds a legacy,
or does not. A positive vital answer requires an accessible authored affirmative
transition chain. `vital-initialize` has an exact empty nonaffirmative payload;
only a later `vital-birth`, `vital-death`, `vital-existence-start`, or
`vital-existence-end` transition has vital meaning. In a character relationship
or vital discovery read, absent, future,
and withheld evidence are observationally equivalent `unknown` results; an
explicit unsupported capability or malformed/cross-timeline input remains
`unavailable` or `invalid` as applicable. Static identity fields and an empty
result therefore do not reveal future death, secret lineage, or inaccessible
relationships.

## Derived reads and closed outcomes

The public read protocol is `wedl-generational/v1`. Requests name a revision,
capabilities, explicit mode (`author-as-of`, `author-all-time`, or
`character`), bounded depth/items, and cursor where applicable. `author-as-of`
and `character` require a canonical StoryTime `at`; `author-all-time` instead
requires `timeline` and forbids `at`. Its exact filter order is authenticated
viewer, server-derived audience, server-derived perspective, capability,
timeline, record audience, record perspective, applicability, ordering,
pagination, serialization, and derived traversal. It exposes all authored,
applicable facts on that timeline through the selected revision, including facts
after any ordinary as-of horizon; it does not make character reads future-aware.
Every derived relationship returns ordered authored
edge citations (`record_id`, source path, and applicability) and is one of:

- `available`: a bounded, deterministic result backed by citations;
- `unknown`: source is valid but lacks an accessible affirmative fact needed
  for the result;
- `unavailable`: a negotiated capability, explicit non-discovery target, or
  required source support is unavailable, without a secret-revealing detail;
- `invalid`: malformed request, endpoint, timeline, variant, or causal shape;
- `limit`: the requested bound would be exceeded, with no partial inference.

Every citation in every derived response is exactly
`{record_id,path,applicability}`. `applicability` is the closed source union:
`{applicability_kind: static}`, `{applicability_kind: instant,point}`, or
`{applicability_kind: inclusive-interval,first,last}`; the interval endpoints
are inclusive StoryTime values. Derived relatives are labeled
`biological-parent`, `adoptive-parent`,
`ancestor`, `descendant`, or `relative-path` only when a deterministic chain
of canonical parentage edges supports that exact label.  They create no source
fact, cannot traverse a cycle, never select a biological over adoptive path,
and return `unknown` rather than guess when no cited relationship exists.
Default result order is `(applicability StoryTime, source_ordinal, stable_id)`;
traversal order is breadth-first by depth and then that order.  Depth and item
budgets are mandatory and an exhausted budget returns `GEN-LIMIT-001`.

## v0.7 capability, compatibility, and conversion

`wedl/v0.7` is one homogeneous, capability-negotiated source envelope shared
with ADR 0004. Capabilities occur exactly once on the authoritative world
frontmatter envelope, never on a generational, spatial, or other non-world
record; records inherit no separate capability list. Its complete registry is the core tokens
`generational-core-v1` and `spatial-core-v1`, plus ADR 0004's optional
`geometry-v1`, `route-v1`, and `overlay-v1`; each optional token requires
`spatial-core-v1`. A valid v0.7 source may be generational-only, spatial-only,
or combined. Neither core capability implies the other, and an unknown token
is rejected with `GEN-CAPABILITY-001` before records are read. A reader rejects
every declared capability it cannot negotiate, and a v0.7 world with omitted,
unknown, unsorted, duplicate, or prerequisite-missing world capabilities is
invalid rather than partially interpreted. A non-world `capabilities` field is
also invalid. Organizations may reference a location ID, but never
own or infer geometry, topology, containment, access, routes, a common metric,
or a spatial visibility rule.

That v0.7 world envelope inherits the validated v0.6 chronology grammar: its
`timelines` value is a nonempty array of closed `{id,label[,origin]}` mappings,
where `origin` is exactly `{tick,label}`, and `default_timeline` names a listed
ID. Its world-only `chronology` is the closed `{calendars,eras,anchors}`
declaration. These retain the shared world’s chronology semantics unchanged;
calendar labels remain evidence/display and never replace StoryTime.

Capability ordering is exact and is never locale-, case-, or Unicode-normalized:
first reject any token outside the registry or whose prerequisite is absent;
then require the list to equal the registry-rank projection
`[generational-core-v1, spatial-core-v1, geometry-v1, route-v1, overlay-v1]`
after omitting undeclared tokens. Duplicate or any non-rank order is invalid;
readers do not silently sort source. The parser/compiler/SQLite fingerprint
uses UTF-8 canonical JSON with the literal `schema` followed by that exact
ordered token array. Migration previews serialize and compare the same bytes,
so an equivalent set in a different order is rejected instead of receiving a
different fingerprint.

| Source version | Read policy | Generational capability | Mixed-world policy | Upgrade policy |
| --- | --- | --- | --- | --- |
| `wedl/v0.3` | pinned legacy | unavailable | reject with v0.7 | only coordinated preview/apply |
| `wedl/v0.5` | pinned thread legacy | unavailable | reject with v0.7 | preserve thread grouping exactly |
| `wedl/v0.6` | pinned chronology legacy | unavailable | reject with v0.7 | preserve chronology exactly |
| `wedl/v0.7` | capability-negotiated | declared capabilities only | one version per world | idempotent no-op when already homogeneous |

There is exactly one future local migration mode:
`wedl migrate preview|apply --mode upgrade-v07`, using the existing
`wedl-migration/v1` request/confirmation envelope.  It is the single
coordinated v0.7 conversion for spatial and generational work, not competing
`upgrade-spatial` or `upgrade-generational` commands.  Preview is read-only;
apply requires expected HEAD, a clean managed source/index, the exact snapshot
hash, deterministic confirmation token, and idempotency key. The request's
`targetCapabilities` is a closed world-preview-plan list drawn only from the
complete registry, must already be in registry-rank order, and is part of the existing normalized
`wedl-migration/v1` request: derive `baseRequestHash` from canonical JSON before
`backupRef`, append that ref, then derive `requestHash` from the complete
normalized request. `requestHash` is server-derived in the preview response,
not a CLI argument. Apply preserves existing v1 transport: it receives the
snapshot hash and the exact preview token through `--confirm`, then rechecks
the complete request, server-derived request hash, and confirmation binding.
For legacy v0.3, v0.5, and v0.6 sources, the deterministic target is the fixed
ordered default `[generational-core-v1,spatial-core-v1]`; a homogeneous v0.7
no-op instead uses its exact existing world capability list. The preview derives
its token from the normalized request and request hash; the token is not an
input to that binding and apply only passes it through `--confirm`. Changing a
target capability requires a
new preview and confirmation. It creates
one atomic forward Git commit, preserves thread grouping and chronology byte
semantics, adds neither spatial nor generational facts, and disposes/rebuilds
SQLite from the new source fingerprint. Legacy sources need not create empty
spatial or generational records merely to convert.

Parser, compiler, and SQLite compatibility fingerprints must include the source
version plus the already registry-ordered capability tokens. Until their implementation, a v0.7
source is rejected by existing readers; no reader silently normalizes legacy
records, accepts a mixed version, or pretends it understands a capability.
Rollback is `git revert` (or a forward restoration of the expected source
revision) followed by disposable index disposal/rebuild.  It never rolls back
SQLite as authoritative state, resets Git history, or claims to reverse
un-authored facts.

## Ownership, rollout, and non-goals

| Deferred decision or delivery | Owner | Gate |
| --- | --- | --- |
| v0.7 parser grammar, source validation, diagnostics | generational source task | approval of this ADR |
| normalized tables, fingerprints, replay, bounded query algorithms | compiler task | source validation evidence |
| CLI/HTTP/OpenAPI transport and exact status mapping | API task | query parity evidence |
| authoring/changesets and atomic source writes | authoring task | source and API contracts |
| character knowledge, horizon policy, and leak testing | security/query task | compiled read evidence |
| accessible read-only compendium | UI task | stable API evidence |
| scale fixture, corpus conversion, performance | fixture task | parser/compiler evidence |
| coordinated `upgrade-v07` execution | spatial migration task `3901c303-351a-456a-b438-ac8cc2a3588f` | this ADR and spatial ADR approval |

Rollout order is: validator/source round-trip; normalized compiler and
source/index parity; bounded author/character reads; CLI/HTTP; accessible UI;
then the one explicit coordinated conversion.  Every stage keeps legacy worlds
readable under their pinned policy and rolls back by retracting its capability
advertisement and reverting source, never by inferred repair.  This ADR does
not authorize parser code, database tables, source data, runtime migration,
fixture conversion, endpoint implementation, UI, external identity matching,
privacy inference, or implementation authority beyond the assigned downstream
tasks.  Those deliveries remain governed by their own scoped acceptance and
review gates.
