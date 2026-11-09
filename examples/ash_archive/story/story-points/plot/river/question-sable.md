---
activation_policy: suggest
aliases: []
domain: plot.river
id: sp_013ACJ6297Q206Q3PECAERP90R
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_5HCF55VTB48ADS5XTJAE5S51DX
    id: spt_4TEV3M4ZM5R86PFVPYC534TKET
    note: Mara reaches Sable with the route cord.
    state: active
    time:
      order: 0
      tick: 182
      timeline: main
  - causing_event: event_2H5KG82XHG18KWE5WERDBQVSE8
    id: spt_2D5YWMFMFZHTVJVA021EBCME4N
    note: Sable recovers the map and helps carry the route out of watch custody.
    state: resolved
    time:
      order: 0
      tick: 188
      timeline: main
outcome_events:
- event_58CM80QVNH32PQGP19HCPMGWWZ
- event_2H5KG82XHG18KWE5WERDBQVSE8
priority: 55
schema: wedl/v1
status: canonical
tags:
- sable
- river
title: Question Sable at the River Gate
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
requires: []
when:
  all:
  - knowledge:
      character: char_0PWVJ91SCDP5TFAJRAGNPXDAEB
      claim: letter.source.sable
      states:
      - accepted
      minimum_confidence: 0.8
  - relationship:
      id: rel_0F6WP6V7TC3SADKTFNWD5WJ822
      metric: trust
      min: 0.3
---

# Question Sable at the River Gate

The author knows Oren can lead Mara toward Sable, even though Mara does not yet know Sable handled the letter.
