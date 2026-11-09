---
activation_policy: manual
aliases: []
domain: plot.catalog
id: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_0A9MEJXH75KDVP5BHB0X4RPS62
    id: spt_0Y1JQM5ZM5EYAQEAFZHVGHMHTV
    note: Mara has confirmation of the catalog gap and a physical token.
    state: resolved
    time:
      order: 30
      tick: 56
      timeline: main
outcome_events:
- event_0ASC5J9C99QD256GDTGBDA8NKR
- event_0A9MEJXH75KDVP5BHB0X4RPS62
priority: 20
schema: wedl/v1
status: canonical
tags:
- catalog
- resolved
title: Audit the Missing Cards
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
requires: []
when:
  knowledge:
    character: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim: catalog.gap.exists
    states:
    - accepted
    minimum_confidence: 0.7
---

# Audit the Missing Cards

Mara and Nessa establish that the missing accession cards are deliberate rather than a clerical accident.
