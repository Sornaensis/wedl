+++
schema = "adrai/decision/v1"
adr = "A01M48NJH26YJTG9XWA0SKCPR2Z"
record = "R01M48SFJ2RN244NJ28X5VSGYZG"
title = "Multi-strand chronology and fictional continuities"
summary = "Historical multi-strand continuity design, superseded in full by ADR 0002 and retained only as decision history."
domains = ["narrative-continuity"]
+++

# ADR 0001: Multi-strand chronology and fictional continuities

- **Status:** Superseded
- **Owner:** WEDL maintainers
- **Named approver:** Sornaensis
- **Approval date:** 2026-08-25
- **Decision vectors:** [examples/multi-strand-continuity-vectors.yaml](../../examples/multi-strand-continuity-vectors.yaml)
- **Supersedes:** None
- **Superseded by:** [ADR 0002 — Shared-world concurrent narrative threads] (read with `adrai --repo . show A01M48NHJ5Z6AX617K56WRVFYWT --json`)

> **Historical decision — superseded.** ADR 0002 withdraws this ADR's
> multi-strand, continuity, and v0.4 design direction. This record is retained
> only for decision history; it is not current guidance or a source-schema
> contract.

## Context

`StoryTime` is a unitless ordinal `(timeline, tick, order)` coordinate: `tick`
is signed 64-bit, `order` is signed 32-bit, and negative ticks are valid.
Coordinates compare only within their declared occurrence domain. Spans are
inclusive, and neither duration nor calendar arithmetic is inferred. Current
v0.3 split-party scenes share a single `world.current_time`; generic reads are
ambiguous and conflicting same-coordinate state writes are rejected.

That behavior does not support independently navigated fronts, explicit
fictional forks, or presentation-order flashbacks. Git revisions and branches
describe authored source history, not fictional identity or canon selection;
Markdown/Git remains authoritative and SQLite remains disposable. This ADR
sets product semantics only. It neither changes a source schema nor production
parser, replay, query, compiler, authoring, fixture, or migration behavior.

## Decision

### Vocabulary and identity

A **fictional continuity** is an explicit authored identity and canon boundary.
It has a status (`primary`, `alternate`, `experimental`, or `retired`) and is
not a Git ref, path, commit, or branch. `primary canon` means canon in the
selected primary continuity. `canonical` means settled within the selected
continuity; an alternate can therefore be internally canonical.

An **occurrence timeline/domain** orders fictional occurrences. A continuity
owns exactly one total ordinal occurrence domain in this decision; it is a
separate concept from continuity identity, even when a legacy continuity maps
to its sole `timeline` label. A **strand** (also called a front or lane) is a
named navigation lane in that domain. A **scene** is authored context assigned
to one strand or an explicit multi-strand rendezvous. A **synchronization** or
**rendezvous** is an explicit multi-strand record; equal coordinates alone do
not synchronize, co-locate, or imply a handoff.

An **author horizon** is a selected continuity plus either a scalar coordinate
(which is a shared synchronized frontier), a synchronized frontier, or a
per-strand front-position vector. A **presentation frame/order** is optional reveal/play ordering scoped to a
continuity and named presentation. It has its own identity and never changes
occurrence order. Missing presentation metadata falls back to occurrence order.

### Topology, ancestry, and canon

Keep one total ordinal occurrence axis per continuity. Strands are lanes on
that axis, not independent time domains. Synchronization is explicit. A
continuity has at most one parent, forming a tree. A child declares its parent
and an authored fork coordinate; records at or before that inclusive boundary
are inherited by projection unless the child explicitly replaces or retcons
them. Child-local records begin after the boundary. The parent record remains
owned by the parent; inherited projection does not duplicate or share a
fictional identity. A retcon is a child-local override that identifies the
inherited record it supersedes for that child only.

Ordinary causal edges are legal only within one continuity. A cross-strand
cause is legal only when it is explicitly authored, strictly earlier in the
total occurrence order, and any required handoff/rejoin is an explicit
rendezvous. Exact-coordinate causes are rejected. A continuity ancestry link
is distinct typed provenance, not a causal edge. Ordinary cross-continuity
causes are rejected.

### Horizon, visibility, replay, and state

A scalar horizon is a shared synchronized frontier. Navigation may instead use
a per-strand front-position vector. A strand-local record is visible when its
strand position reaches it; an unstranded/global record is visible only at a
synchronized frontier that reaches it; a multi-strand record is visible only
when every named strand reaches its explicit rendezvous. With unequal strand
positions, a shared mutable-state read must be strand/scene scoped, use an
explicitly synchronized frontier, or reject as ambiguous. It must never invent
aggregate state.

Within the selected continuity, canonical records visible at the horizon
contribute to replay; drafts never contribute; retconned and cancelled records
do not contribute. `author selected-continuity` returns the selected
continuity’s applicable author material with status labels. `author-all` is a
labelled union across requested continuities, not merged truth. A character
view is selected-continuity, horizon-, authorization-, and knowledge-safe.

### Presentation and compatibility

Occurrence order remains canonical. A flashback is an earlier occurrence shown
later by presentation metadata; it never rewrites the fact’s occurrence time.

For v0.3, the declared timeline maps conceptually to the continuity’s sole
occurrence domain, `world.current_time` maps to a scalar/synchronized horizon,
and unstranded records map to global records. Existing split-party sources map
to lanes constrained to that shared scalar horizon. `wedl-timeline/v1` remains
a single-domain occurrence projection. Exact fields, protocol versions, source
schema, and upgrade mechanics are owned by downstream task
`08e5cd72-fe2f-4794-9193-4f412b619489`; this ADR promises no implicit or
destructive migration.

## Rejected alternatives

1. **Separate timeline per split front:** makes ordinary party splits distinct
   occurrence domains and loses direct rendezvous/replay semantics.
2. **Git branch as fictional continuity:** conflates source revision history
   with authored fictional identity and default canon.
3. **True partial-order clock:** increases replay, query, validation, and
   migration cost without a demonstrated need beyond lanes plus rendezvous.
4. **Silent aggregate state at unequal fronts:** invents shared state without
   an authored synchronization boundary.
5. **Occurrence-time rewriting for flashbacks:** corrupts chronology to model
   reveal order that presentation metadata represents directly.

## Consequences and follow-up ownership

Downstream implementation must make `World.active_scene()` singleton ambiguity,
scalar replay/query/compiler assumptions, authoring, causality validation,
search/context, and `wedl-timeline/v1` conform to this ADR. No inference from prose, co-presence, location, or equal coordinate is
permitted. No duration inference, unrelated-domain comparison, calendar
conversion, or hypothesis promotion is added.

The schema-contract task `08e5cd72-fe2f-4794-9193-4f412b619489` owns exact
field names, IDs, versions, and upgrades. The eight downstream canonical tasks
must be reconciled to this terminology after named approval, without changing
their IDs, priorities, or dependencies. The archived
ARCHITECTURE.md (read with `adrai --repo . show A01M48S4ZSCWHKWP8ZXFZ3HQJB2 --json`) and current
SEMANTICS.md (read with `adrai --repo . show A01M48RYDXG6N84N3XSTH44WS81 --json`) retain their existing scalar/split-party
contracts until their owning implementation/documentation follow-up; this ADR
does not claim they are already implemented.

## Approval and supersession

Review this ADR and its vectors together. Sornaensis accepted it on
2026-08-25. The eight downstream task descriptions and their bindings must be
reconciled before implementation begins. A later decision supersedes this one
by naming ADR 0001, preserving its rationale and migration consequences.

## Historical continuity schema withdrawal

# Withdrawn continuity schema contract

**Status:** Historical, nonnormative withdrawal.

This file formerly reserved a multi-strand continuity and `wedl/v0.4` source
schema direction. It is withdrawn and superseded by
[ADR 0002] (read with `adrai --repo . show A01M48NHJ5Z6AX617K56WRVFYWT --json`).

It defines no active source schema, identifier, field, cardinality,
diagnostic, protocol, upgrade, migration, parser, compiler, API, or runtime
behavior. Do not implement or recover the withdrawn design from prior
revisions, examples, or tests. `wedl/v0.3` is the only supported schema under
ADR 0002; any future thread representation requires the separately approved
follow-up B.

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiODE2NjNjNGJkOWI0NDE4MzQzM2U4NzdjZDQwOTc1NWNjNWVkMzg1MiIsImkiOiJzaGEyNTY6czduaE01eFkxS1Q5VlJjSVExblZlVHB1LUFwV2dBdUZEQVNYTEQzS0k3VSIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNDhTRkoyUk4yNDROSjI4WDVWU0dZWkciLCJvcCI6Ik8wMU00OFNGSjJSTjI0NE5KMjhYNVZTR1laRyIsInAiOlsiUjAxTTQ4TkpIWlRHQ1lSVEJCSlE5RDdYRzlNIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OkNWV3VrNlBHUU1wYUFWV202a2hDaWM4Q194aGZ5eGFYUEtONVR5QkY2djgiLCJ0IjoxNzkxMjk2NTIyMzI4LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
