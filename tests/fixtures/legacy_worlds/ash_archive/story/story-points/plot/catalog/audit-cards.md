---
schema: wedl/v0.3
kind: story-point
id: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
title: Audit the Missing Cards
domain: plot.catalog
status: canonical
tags:
- catalog
- resolved
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_0Y1JQM5ZM5EYAQEAFZHVGHMHTV
    time:
      timeline: main
      tick: 56
      order: 30
    state: resolved
    causing_event: event_0A9MEJXH75KDVP5BHB0X4RPS62
    note: Mara has confirmation of the catalog gap and a physical token.
activation_policy: manual
priority: 20
repeat_policy: once
dependencies:
  all: []
trigger:
  knowledge:
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim_key: catalog.gap.exists
    state_in:
    - accepted
    minimum_confidence: 0.7
on_activate:
  create_draft_scene: false
outcome_events:
- event_0ASC5J9C99QD256GDTGBDA8NKR
- event_0A9MEJXH75KDVP5BHB0X4RPS62
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Audit the Missing Cards

Mara and Nessa establish that the missing accession cards are deliberate rather than a clerical accident.
