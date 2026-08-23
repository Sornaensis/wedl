---
schema: wedl/v0.3
kind: story-point
id: sp_4CETXX7BEVPV904RG5FJ0ZPJNX
title: Choose the Custodian of Flood Register N-7B
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_2Z2PY2RSE7NXN4CJH9S2HS0CJT
    time:
      timeline: main
      tick: 139
      order: 30
    state: active
    causing_event: event_3W0QZ7TF8PMZYSQF0M338RTYYP
    note: Ysabet can carry one item.
  - id: spt_6YRPE54YY9CHKX22SCR3PD64N3
    time:
      timeline: main
      tick: 140
      order: 30
    state: resolved
    causing_event: event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
    note: Mara assigns the original register to Ysabet.
activation_policy: manual
priority: 94
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
on_activate:
  create_draft_scene: false
outcome_events:
- event_24R6E5C0DSH5BWPGTQ7FJAJAQ3
---

# Choose the Custodian of Flood Register N-7B

Choose whether the original register remains with Mara or enters a notarial custody chain.
