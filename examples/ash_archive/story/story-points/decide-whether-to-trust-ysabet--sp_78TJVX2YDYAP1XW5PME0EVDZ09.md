---
activation_policy: manual
aliases: []
domain: plot.expanded
id: sp_78TJVX2YDYAP1XW5PME0EVDZ09
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
    id: spt_37GXZXRX2PAX9YC3WN43W6JWGR
    note: Mara entrusts Ysabet with the original register.
    state: resolved
    time:
      order: 30
      tick: 140
      timeline: main
outcome_events:
- event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
priority: 90
schema: wedl/v1
status: canonical
tags:
- choice
title: Decide Whether to Trust Ysabet
requires: []
when:
  knowledge:
    character: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim: ysabet.created-hour
    states:
    - suspected
    - accepted
    minimum_confidence: 0.6
---

# Decide Whether to Trust Ysabet

Mara must decide whether procedural help is enough to entrust evidence to Ysabet.

The second act advances this thread at tick 140.
