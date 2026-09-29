---
activation_policy: suggest
aliases: []
dependencies:
  all:
  - state_in:
    - resolved
    story_point: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
domain: plot.heron
id: sp_0JS6FPRR8F23GGR5V9W47J7MK5
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    id: spt_25GYNC0G2SV92KBCFJW91BKVZW
    note: Opening Ilyra's letter confirms that the mark belongs to a live contingency network.
    state: active
    time:
      order: 0
      tick: 148
      timeline: main
  - causing_event: event_545ZXMGNRB16VVV7HGWD7W9J17
    id: spt_33NXW2CMEHV04F7E5DXYQJHPYZ
    note: Ilyra explains how she reactivated the network and why its records were divided.
    state: resolved
    time:
      order: 20
      tick: 171
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
- event_545ZXMGNRB16VVV7HGWD7W9J17
priority: 65
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- heron
- history
title: Trace the Erased-Wing Heron
trigger:
  knowledge:
    claim_key: heron.mark.courier-network
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    minimum_confidence: 0.6
    state_in:
    - accepted
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Trace the Erased-Wing Heron

Investigate who revived the obsolete erased-wing courier sign and how it connects to Ilyra.


The thread resolves when Ilyra accounts for the reactivated network at tick 171.
