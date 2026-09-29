---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_35EY7W2N8WH1PG0S9F367HZT4X
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_4ME3BF84VMYJEH9WEGD1ZAR39F
    id: spt_028K3PQ61J865EWB1CD9787H8Z
    note: The reliquary wakes a gallery of remembered beasts.
    state: active
    time:
      order: 10
      tick: 136
      timeline: main
  - causing_event: event_22FQP078VAK3ZC1SVK18VZEC9S
    id: spt_0EAW76HPW40V9XSZ8N1K01BW8Z
    note: The party reaches the surface shaft.
    state: resolved
    time:
      order: 20
      tick: 151
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_22FQP078VAK3ZC1SVK18VZEC9S
priority: 98
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Escape the Wretch Gallery
trigger:
  all: []
---

# Escape the Wretch Gallery

Cross a ruin gallery where wretches can emerge from every carved surface.
