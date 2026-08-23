---
schema: wedl/v0.3
kind: story-point
id: sp_78TJVX2YDYAP1XW5PME0EVDZ09
title: Decide Whether to Trust Ysabet
domain: plot.expanded
status: canonical
tags:
- choice
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_37GXZXRX2PAX9YC3WN43W6JWGR
    time:
      timeline: main
      tick: 140
      order: 30
    state: resolved
    causing_event: event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
    note: Mara entrusts Ysabet with the original register.
activation_policy: manual
priority: 90
repeat_policy: once
dependencies:
  all: []
trigger:
  knowledge:
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim_key: ysabet.created-hour
    state_in:
    - suspected
    - accepted
    minimum_confidence: 0.6
on_activate:
  create_draft_scene: false
outcome_events:
- event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
---

# Decide Whether to Trust Ysabet

Mara must decide whether procedural help is enough to entrust evidence to Ysabet.

The second act advances this thread at tick 140.
