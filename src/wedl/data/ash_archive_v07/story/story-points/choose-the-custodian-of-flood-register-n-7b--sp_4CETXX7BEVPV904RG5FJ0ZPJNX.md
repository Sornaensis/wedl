---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_4CETXX7BEVPV904RG5FJ0ZPJNX
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_3W0QZ7TF8PMZYSQF0M338RTYYP
    id: spt_2Z2PY2RSE7NXN4CJH9S2HS0CJT
    note: Ysabet can carry one item.
    state: active
    time:
      order: 30
      tick: 139
      timeline: main
  - causing_event: event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
    id: spt_6YRPE54YY9CHKX22SCR3PD64N3
    note: Mara assigns the original register to Ysabet.
    state: resolved
    time:
      order: 30
      tick: 140
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
priority: 94
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Choose the Custodian of Flood Register N-7B
trigger:
  event:
    event: event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
---

# Choose the Custodian of Flood Register N-7B

Choose whether the original register remains with Mara or enters a notarial custody chain.
