+++
schema = "adrai/decision/v1"
adr = "A01M48NHJ5Z6AX617K56WRVFYWT"
record = "R01M4DMT38C94FHBZ7EXV9T13WY"
title = "Shared-world concurrent narrative threads"
summary = "One shared world and one global StoryTime; narrative threads group work without separate canon, clocks, or search corpora."
domains = ["shared-world"]
+++

# ADR 0002: Shared-world concurrent narrative threads

- **Status:** Accepted
- **Owner:** WEDL maintainers
- **Named approver:** Sornaensis
- **Approval date:** 2026-08-26
- **Decision vectors:** [examples/shared-world-thread-vectors.yaml](../../../../tests/fixtures/architecture/shared-world-thread-vectors.yaml)
- **Supersedes:** [ADR 0001] (read with `adrai --repo . show A01M48NJH26YJTG9XWA0SKCPR2Z --json`)

## Context

WEDL needs to describe concurrent narrative activity without turning a
collaborative shared world into several competing fictional worlds. The
previous ADR reserved a multi-strand continuity and v0.4 direction. That
direction is withdrawn: it adds fictional identity, clock, corpus, and replay
boundaries that are inappropriate for one shared setting and cannot be safely
inferred from ordinary concurrent work.

This ADR is governance and product semantics only. It does not define source
fields, identifiers, cardinality, a migration, a parser, API, protocol,
compiler, query route, or runtime behavior. Those details are explicitly
deferred to follow-up B.

## Decision

### One world and one clock

WEDL has one shared fictional world and one global `StoryTime` ordering for
that world. Ordinary time-bearing facts use that shared ordering. A narrative
thread is a grouping or coordination label for related scenes and work; it is
not a second world, a canon boundary, or a time coordinate system.

Threads share the same characters, locations, and mutable world state. They
also share one interpretation of ticks and any calendar meaning attached to
the global ordering. Threads may be concurrent or overlap in time; that
overlap does not duplicate a character, location, state, clock, or canon.

The following are forbidden by this decision: alternate canon, fictional
forks, continuity/occurrence domains, replay projections, per-thread horizons,
and per-thread or projection-specific search corpora. A thread never selects
different truth, inherits a branch, or changes which facts are canonical.

### Causality, search, and presentation

Cross-thread causality is ordinary causality. A cause must be strictly earlier
than its effect in the single global `StoryTime` ordering; thread membership
does not need a handoff, rendezvous, conversion, or special causal rule.

Search uses one corpus and the ordinary global filters only. Thread grouping
may be used as a filter or presentation aid when a later, separately approved
schema defines it, but it must not create a separate ranking corpus, model, or
canon-visible result set. A presentation order, reveal sequence, or display
grouping has no effect on canon, chronology, replay, causality, or search
membership.

### Compatibility and recovery

`wedl/v0.3` is the only supported source schema. The separately owned
`wedl/v0.5` thread contract is a reservation, not an implementation. This ADR
introduces no v0.4 schema, continuity protocol, runtime upgrade path, or
compatibility alias. Existing v0.3 material remains the recovery baseline:
authors and implementers must keep or return to the single shared-world model
rather than translate sources into the withdrawn continuity design.

ADR 0001 and its continuity schema/vector material are superseded historical
artifacts. Their tombstones are nonnormative withdrawals, not specifications
that can be revived piecemeal. Recovery requires a new approved decision and
the deferred schema work in B; it must not infer a migration from archived
examples or introduce partial alternate-canon support.

## Consequences

- Concurrent work is represented as threads within one world, not as forks.
- Characters, locations, mutable state, and tick/calendar interpretation stay
  shared even when threads overlap.
- Every causal check and time comparison remains on one global ordering.
- Search remains one corpus with one ordinary filtering model, preventing
  thread-local material from changing separate ranking statistics.
- Presentation remains display-only and cannot rewrite shared-world truth.
- Field names, cardinality, validation diagnostics, serialization, and any
  runtime implementation are deferred to B.

## Rejected alternatives

1. **Alternate canon or forks for threads:** creates competing truth where the
   product requires one shared world.
2. **Per-thread domains, horizons, or replay projections:** creates partial
   clocks and ambiguous aggregate state instead of using the global order.
3. **Per-thread corpora:** permits one thread's excluded material to affect a
   separate ranking system and fragments ordinary search.
4. **Presentation as a canon mechanism:** confuses reveal order with facts.

## Supersession

This decision supersedes ADR 0001 in full. It deliberately leaves that ADR's
historical rationale available, but no part of its v0.4 multi-strand contract
is normative after this ADR's approval.

<!-- @adrai:eyJhIjp7ImkiOiJhcmNoaXZlLWV4YW1wbGVzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiZWY2MmUzODcxMTIyZmJmNDUyMGMwMjM5YTU3YTQzN2YxZWM3NDY1YyIsImkiOiJzaGEyNTY6X205OHF4MTZ4QmNlSzFwcTRWeGdsRXNNS0JvVGNuWDF4THdUV3R3TWFvRSIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNERNVDM4Qzk0RkhCWjdFWFY5VDEzV1kiLCJvcCI6Ik8wMU00RE1UMzhDOTRGSEJaN0VYVjlUMTNXWSIsInAiOlsiUjAxTTQ4UzkyTlAxUVkwMTJLOVNLQlBHRFZGIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OkY0RWNwci1VVWNFYndzQnlHVng4eVBGZXpXUUNNa2piX25fY1RNcm13ZE0iLCJ0IjoxNzkxNDU5Mzk2ODc2LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
