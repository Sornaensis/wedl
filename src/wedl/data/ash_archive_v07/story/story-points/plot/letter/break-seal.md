---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.letter
id: sp_0DZ6TMN98A6B7EGKTD2WRB79NX
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    id: spt_2YS28JA0TQGR44SPWE44VP9A7J
    note: Mara opens the letter under witness.
    state: resolved
    time:
      order: 0
      tick: 148
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
priority: 80
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- letter
- reveal
title: Break the Seal
trigger:
  all:
  - entity_state:
      equals:
        entity: char_00HBQM4T4CMF11RKMNDBRP92QC
      key: holder
      target: obj_0AZ9FQZCZC8ZR175BPRGDFQHEJ
  - knowledge:
      claim_key: letter.origin.deliberate-gift
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      minimum_confidence: 0.5
      state_in:
      - accepted
      - suspected
  - relationship:
      less_than_or_equal: 0.0
      metric: trust
      relationship: rel_0G133RCZ73N2ZT9F6S9SV9BXEQ
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Break the Seal

Mara has the letter and sufficient distrust to consider violating archive protocol. Eligibility does not assert that she opens it.

The second act advances this thread at tick 148.
