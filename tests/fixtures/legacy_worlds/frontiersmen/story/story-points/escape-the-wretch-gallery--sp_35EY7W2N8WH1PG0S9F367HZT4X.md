---
schema: wedl/v0.3
kind: story-point
id: sp_35EY7W2N8WH1PG0S9F367HZT4X
title: Escape the Wretch Gallery
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_028K3PQ61J865EWB1CD9787H8Z
    time:
      timeline: main
      tick: 136
      order: 10
    state: active
    causing_event: event_4ME3BF84VMYJEH9WEGD1ZAR39F
    note: The reliquary wakes a gallery of remembered beasts.
  - id: spt_0EAW76HPW40V9XSZ8N1K01BW8Z
    time:
      timeline: main
      tick: 151
      order: 20
    state: resolved
    causing_event: event_22FQP078VAK3ZC1SVK18VZEC9S
    note: The party reaches the surface shaft.
activation_policy: suggest
priority: 98
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_22FQP078VAK3ZC1SVK18VZEC9S
---

# Escape the Wretch Gallery

Cross a ruin gallery where wretches can emerge from every carved surface.
