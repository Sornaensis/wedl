---
schema: wedl/v0.3
kind: story-point
id: sp_7C05N0E5311W4YY72D8X3PHB6P
title: Reach the Surface
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_06MX24601VFB0PT5QR1C5A0DJC
    time:
      timeline: main
      tick: 151
      order: 20
    state: active
    causing_event: event_22FQP078VAK3ZC1SVK18VZEC9S
    note: A cold shaft offers the first surface route.
  - id: spt_2JFQ7V269A03HSZ11YKJKXZ62X
    time:
      timeline: main
      tick: 158
      order: 0
    state: resolved
    causing_event: event_5QXAWZGMJXAY3MA8W1C3BNSV1V
    note: The party emerges onto the isolated hilltop.
activation_policy: suggest
priority: 92
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_5QXAWZGMJXAY3MA8W1C3BNSV1V
---

# Reach the Surface

Find an exit from the cavern system and establish where the party has emerged.
