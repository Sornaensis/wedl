+++
schema = "adrai/decision/v1"
adr = "A01M491X2B6KF0PZFZ4HJMBFSSR"
record = "R01M491X2MDMSKTASAQM4Z0M6Q3"
title = "Generational source and authored belief contract"
summary = "Preserve the existing source contract with current v0.7 behavior and original approval provenance."
domains = ["generational", "source"]
+++

# Generational source and authored belief contract

This record migrates the existing `docs/GENERATIONAL_SOURCE_CONTRACT.md` contract into ADRAI. The original ADR 0005 (`A01M48NQZ50KH9V094Q5A3138R0`) remains the authority for its 2026-08-30 approved proposal and historical five-token decision vectors. Its existing 2026-10-05 typed-knowledge extension remains separately identified; this migration does not invent a new approval or rewrite the original approval stage. Current implementation statements below distinguish that stage from the implemented v0.7 component.

## Character assertions

An existing `knowledge` record may opt into `claim.genealogy` on `wedl/v0.7`.
The world must declare `generational-knowledge-v1`, after the existing
capabilities in their canonical order, and also `generational-core-v1`.
Legacy free-form claims and `upgrade-v07` defaults are unchanged.

A typed claim contains only `key`, `statement`, and `genealogy`. The genealogy
mapping requires `kind`, `payload`, and `valid`; optional `labels` and `evidence`
are explicit learned material. The closed payloads are:

| Kind | Payload |
| --- | --- |
| `parentage` | `child_id`, `parent_id`, `basis` (`biological` or `adoptive`) |
| `union` | sorted unique `participant_ids`, `state` (`formed`, `ended`, or `annulled`) |
| `organization` | `organization_id`, nullable `parent_id` |
| `affiliation` | `character_id`, `organization_id`, nullable literal `role` |
| `tenure` | `legacy_id`, nullable `holder_id`, `basis` (`legal` or `de-facto`) |
| `claim` | `legacy_id`, `claimant_id`, `state` (`proposed`, `disputed`, `recognized`, `withdrawn`, or `rejected`) |
| `vital` | `character_id`, affirmative `state` (`living`, `dead`, `existing`, or `ended`) |

All endpoints must be existing stable IDs of the stated kind. They need not
match canonical relationships: mistaken, conflicting, or cyclic beliefs are
valid source. Claims do not establish holders, legitimacy, or inheritance.

`valid` has exact signed `{timeline,tick,order}` values in `from` and optional
inclusive `until`. This asserted applicability is separate from when the
knower learns it. Existing timed knowledge states carry acceptance, suspicion,
uncertainty, rejection and forgetting. At least one explicit affirmative
learning transition is required; retrospective applicability does not move
learning earlier. Correction creates a new knowledge record and rejects or
forgets the old assertion, retaining its earlier history.
Earlier rejected states do not reveal the typed assertion or learned labels:
character projections and search begin at the first affirmative learning time.
Learning may occur after `valid.until`: interval expiry does not forget the
authored historical assertion or its learned labels, or establish a current edge.

Learned labels map only asserted endpoint IDs to literal nonempty text of at
most 200 characters. Union participants, labels, and evidence lists each have
a limit of 32. Evidence is optional: the own knowledge transition is always
the intrinsic citation. Supplied evidence mappings contain exactly `kind`,
`entity_id`, and the corresponding `transition_id` (`knowledge`),
`observation_id` (`observation`), `turn_id` (`turn`), or `recollection_id`
(`recollection`). They must reference exact admitted evidence for this knower
no later than the first affirmative learning transition. Historical
observations and heard speech are checked at their authored time; another
character's knowledge/recollection, inaudible speech, action beats and future
evidence are not learning provenance. Presence, prose and `source_entity`
alone never grant a typed assertion.

Authored `source_entity` and `causing_event` provenance is retained in source,
but their identifiers are null in typed character knowledge projections and
cannot authorize retrieval of canonical material. Typed projections use a
neutral statement and claim key, omit external evidence pointers and author
notes, and cite the actual knowledge/transition IDs and exact learning time.
Source titles, paths, corrections and acquisition prose remain private; no
canonical title or correction replaces an explicitly learned label.
Character search indexes only the neutral typed statement and learned labels,
before matching and ranking, and cites knowledge/transition IDs and learning
time instead of author source paths. Author and legacy knowledge search retain
their existing documents and citations.

## Canonical generational records

`wedl/v0.7` generational facts are literal Markdown frontmatter records.  The
supported record kinds are `organization`, `parentage`, `union`,
`affiliation`, `legacy`, `tenure`, `claim`, and `vital-history`.

Each requires `generational-core-v1` in the canonical, world-only capabilities
array.  Its ID prefix and canonical generated directory are respectively:
`organization_`/`organizations`, `kinship_`/`kinships`, `union_`/`unions`,
`affiliation_`/`affiliations`, `legacy_`/`legacies`, `tenure_`/`tenures`,
`claim_`/`claims`, and `vital_`/`vitals`.

Records use an exact common envelope plus the kind's fields from ADR 0005.
`initialization` is one instant `*-initialize` transition and `transitions` is
an append-only list of closed transition payloads. Applicability is only
`static`, `instant`, or `inclusive-interval`; all timed values are exact
`{timeline,tick,order}` values. This source contract does not derive lineage,
inheritance, legitimacy, residence, or a successor.

The optional `ash-archive-v07` and `frontiersmen-v07` bootstrap choices carry
this source envelope, but add no organization, kinship, union, affiliation,
legacy, tenure, claim, or vital-history records. They preserve the pinned
worlds' authored history for explicit future authoring. See
[the practical generational guide](../../../../docs/guides/generational.md#packaged-examples) for
packaged-example commands and compatibility guidance.

## Coordinated current capability envelope

The current world-only capability order is `[generational-core-v1, spatial-core-v1, geometry-v1, route-v1, overlay-v1, generational-knowledge-v1]`. Each world declares only the capabilities it uses, in that order. The optional final knowledge capability requires `generational-core-v1`; geometry, route and overlay require `spatial-core-v1`. At least one core component is required. A generational-only world can have coordinate-free ordinary locations, while operative spatial fields require spatial core. The original five-token ADR 0005 vectors describe their approved stage and remain unchanged; migration defaults do not opt into typed knowledge.

Inherited thread, StoryTime and chronology contracts remain authoritative: thread contract `A01M48RX5WAQT5ECH66KTCFVC0T`, chronology schema `A01M48XWQD0809VQ3RD9NBYPSKB`, validation `A01M48XXBREJY497SNXZVCW85EC`, migration `A01M48XY4R2CJ698Z2VQM9SG4ZZ`, and spatial source `A01M48ZRCS8CP9EC5H5YH2MRVQ4`. These components do not infer genealogy or convert literal beliefs into canonical truth.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNDFlYWZmY2RhOGNhZjBlZTlhYTVhYjllZWNjOWVjMDYwYWM5NTgxYSIsImkiOiJzaGEyNTY6WmtQcVBDdUhHOXVSRWxOc1NBTE5wblZNX3RiRjRjX2swekVxSjBhbURyQSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ5MVgyTURNU0tUQVNBUU00WjBNNlEzIiwib3AiOiJPMDFNNDkxWDJNRE1TS1RBU0FRTTRaME02UTMiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpDQkpJWlgwbXFVZHFTbVZNSUM5OGJub3dHVDRkWDMyamg1TUFTNWh1QkF3IiwidCI6MTc5MTMwNTM1Mzg2OSwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
