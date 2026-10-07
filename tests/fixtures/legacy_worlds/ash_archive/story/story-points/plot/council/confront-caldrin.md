---
schema: wedl/v0.3
kind: story-point
id: sp_03D8BERZXNN1C6NDJM27CR45P5
title: Confront Caldrin Vey
domain: plot.council
status: canonical
tags:
- caldrin
- confrontation
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_1ZYYTS170A95AC8K5JWW7B5YS4
    time:
      timeline: main
      tick: 160
      order: 0
    state: active
    causing_event: event_03W0PC6646ZK48PEQXFQ6Q3BGB
    note: The ledger hearing puts Caldrin's account under public challenge.
  - id: spt_4F5HBEW2D09V9CTAAK3WMRGR7M
    time:
      timeline: main
      tick: 197
      order: 20
    state: resolved
    causing_event: event_1H444APDHD9WCRNJB9MJGJHD56
    note: Caldrin is confronted in Ember Hall and suspended from Archive authority while the inquiry continues.
activation_policy: manual
priority: 40
repeat_policy: once
dependencies:
  all: []
trigger:
  all:
  - knowledge:
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim_key: caldrin.involvement.suspected
      state_in:
      - suspected
      - accepted
      minimum_confidence: 0.5
  - relationship:
      relationship: rel_0150APAYS9V1YWQNESA1GVPZ4R
      metric: trust
      less_than_or_equal: 0.0
on_activate:
  create_draft_scene: false
outcome_events:
- event_1H444APDHD9WCRNJB9MJGJHD56
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Confront Caldrin Vey

Mara has enough suspicion to confront Caldrin, but not enough evidence for the confrontation to be safe or decisive.


The confrontation is active after the hearing, but final responsibility remains contested.
