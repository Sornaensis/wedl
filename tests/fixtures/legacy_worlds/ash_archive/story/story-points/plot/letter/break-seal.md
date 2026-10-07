---
schema: wedl/v0.3
kind: story-point
id: sp_0DZ6TMN98A6B7EGKTD2WRB79NX
title: Break the Seal
domain: plot.letter
status: canonical
tags:
- letter
- reveal
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_2YS28JA0TQGR44SPWE44VP9A7J
    time:
      timeline: main
      tick: 148
      order: 0
    state: resolved
    causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    note: Mara opens the letter under witness.
activation_policy: suggest
priority: 80
repeat_policy: once
dependencies:
  all: []
trigger:
  all:
  - entity_state:
      target: obj_0AZ9FQZCZC8ZR175BPRGDFQHEJ
      key: holder
      equals:
        entity: char_00HBQM4T4CMF11RKMNDBRP92QC
  - knowledge:
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim_key: letter.origin.deliberate-gift
      state_in:
      - accepted
      - suspected
      minimum_confidence: 0.5
  - relationship:
      relationship: rel_0G133RCZ73N2ZT9F6S9SV9BXEQ
      metric: trust
      less_than_or_equal: 0.0
on_activate:
  create_draft_scene: false
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Break the Seal

Mara has the letter and sufficient distrust to consider violating archive protocol. Eligibility does not assert that she opens it.

The second act advances this thread at tick 148.
