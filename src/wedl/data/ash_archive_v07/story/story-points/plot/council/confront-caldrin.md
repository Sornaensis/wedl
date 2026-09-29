---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.council
id: sp_03D8BERZXNN1C6NDJM27CR45P5
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_03W0PC6646ZK48PEQXFQ6Q3BGB
    id: spt_1ZYYTS170A95AC8K5JWW7B5YS4
    note: The ledger hearing puts Caldrin's account under public challenge.
    state: active
    time:
      order: 0
      tick: 160
      timeline: main
  - causing_event: event_1H444APDHD9WCRNJB9MJGJHD56
    id: spt_4F5HBEW2D09V9CTAAK3WMRGR7M
    note: Caldrin is confronted in Ember Hall and suspended from Archive authority while the inquiry continues.
    state: resolved
    time:
      order: 20
      tick: 197
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_1H444APDHD9WCRNJB9MJGJHD56
priority: 40
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- caldrin
- confrontation
title: Confront Caldrin Vey
trigger:
  all:
  - knowledge:
      claim_key: caldrin.involvement.suspected
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      minimum_confidence: 0.5
      state_in:
      - suspected
      - accepted
  - relationship:
      less_than_or_equal: 0.0
      metric: trust
      relationship: rel_0150APAYS9V1YWQNESA1GVPZ4R
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Confront Caldrin Vey

Mara has enough suspicion to confront Caldrin, but not enough evidence for the confrontation to be safe or decisive.


The confrontation is active after the hearing, but final responsibility remains contested.
