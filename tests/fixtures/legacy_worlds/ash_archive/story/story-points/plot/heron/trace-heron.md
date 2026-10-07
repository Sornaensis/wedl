---
schema: wedl/v0.3
kind: story-point
id: sp_0JS6FPRR8F23GGR5V9W47J7MK5
title: Trace the Erased-Wing Heron
domain: plot.heron
status: canonical
tags:
- heron
- history
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_25GYNC0G2SV92KBCFJW91BKVZW
    time:
      timeline: main
      tick: 148
      order: 0
    state: active
    causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    note: Opening Ilyra's letter confirms that the mark belongs to a live contingency network.
  - id: spt_33NXW2CMEHV04F7E5DXYQJHPYZ
    time:
      timeline: main
      tick: 171
      order: 20
    state: resolved
    causing_event: event_545ZXMGNRB16VVV7HGWD7W9J17
    note: Ilyra explains how she reactivated the network and why its records were divided.
activation_policy: suggest
priority: 65
repeat_policy: once
dependencies:
  all:
  - story_point: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
    state_in:
    - resolved
trigger:
  knowledge:
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim_key: heron.mark.courier-network
    state_in:
    - accepted
    minimum_confidence: 0.6
on_activate:
  create_draft_scene: false
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
- event_545ZXMGNRB16VVV7HGWD7W9J17
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Trace the Erased-Wing Heron

Investigate who revived the obsolete erased-wing courier sign and how it connects to Ilyra.


The thread resolves when Ilyra accounts for the reactivated network at tick 171.
