---
activation_policy: manual
aliases: []
dependencies:
  all: []
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
on_activate:
  create_draft_scene: false
outcome_events:
- event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
priority: 90
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- choice
title: Decide Whether to Trust Ysabet
trigger:
  knowledge:
    claim_key: ysabet.created-hour
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    minimum_confidence: 0.6
    state_in:
    - suspected
    - accepted
---

# Decide Whether to Trust Ysabet

Mara must decide whether procedural help is enough to entrust evidence to Ysabet.

The second act advances this thread at tick 140.
