---
activation_policy: suggest
aliases: []
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
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
priority: 80
schema: wedl/v1
status: canonical
tags:
- letter
- reveal
title: Break the Seal
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
requires: []
when:
  all:
  - state:
      entity: obj_0AZ9FQZCZC8ZR175BPRGDFQHEJ
      key: holder
      equals:
        entity: char_00HBQM4T4CMF11RKMNDBRP92QC
  - knowledge:
      character: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim: letter.origin.deliberate-gift
      states:
      - accepted
      - suspected
      minimum_confidence: 0.5
  - relationship:
      id: rel_0G133RCZ73N2ZT9F6S9SV9BXEQ
      metric: trust
      max: 0.0
---

# Break the Seal

Mara has the letter and sufficient distrust to consider violating archive protocol. Eligibility does not assert that she opens it.

The second act advances this thread at tick 148.
