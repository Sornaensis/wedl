---
schema: wedl/v0.3
kind: story-point
id: sp_013ACJ6297Q206Q3PECAERP90R
title: Question Sable at the River Gate
domain: plot.river
status: canonical
tags:
- sable
- river
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_4TEV3M4ZM5R86PFVPYC534TKET
    time:
      timeline: main
      tick: 182
      order: 0
    state: active
    causing_event: event_5HCF55VTB48ADS5XTJAE5S51DX
    note: Mara reaches Sable with the route cord.
  - id: spt_2D5YWMFMFZHTVJVA021EBCME4N
    time:
      timeline: main
      tick: 188
      order: 0
    state: resolved
    causing_event: event_2H5KG82XHG18KWE5WERDBQVSE8
    note: Sable recovers the map and helps carry the route out of watch custody.
activation_policy: suggest
priority: 55
repeat_policy: once
dependencies:
  all: []
trigger:
  all:
  - knowledge:
      knower: char_0PWVJ91SCDP5TFAJRAGNPXDAEB
      claim_key: letter.source.sable
      state_in:
      - accepted
      minimum_confidence: 0.8
  - relationship:
      relationship: rel_0F6WP6V7TC3SADKTFNWD5WJ822
      metric: trust
      greater_than_or_equal: 0.3
on_activate:
  create_draft_scene: false
outcome_events:
- event_58CM80QVNH32PQGP19HCPMGWWZ
- event_2H5KG82XHG18KWE5WERDBQVSE8
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Question Sable at the River Gate

The author knows Oren can lead Mara toward Sable, even though Mara does not yet know Sable handled the letter.
