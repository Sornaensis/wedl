---
schema: wedl/v0.3
kind: story-point
id: sp_56BDX8H6ZW2K20S7Z9EV5RBRTR
title: Understand the Tree King
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_5PAVS0Y3GBCVNKR13NYK6NBRXB
    time:
      timeline: main
      tick: 160
      order: 0
    state: active
    causing_event: event_6K8VCQVV5Q04PRF6NHWHJNHT4N
    note: The party approaches the only visible camp.
  - id: spt_5FECFYGEM5EG8FMJQMJ6KKGWE0
    time:
      timeline: main
      tick: 170
      order: 20
    state: resolved
    causing_event: event_7TH2BAQE1Q7J6MABMC316AD0ZB
    note: Aldren reveals his guild past, amber practice, and blood-sport purpose.
activation_policy: suggest
priority: 95
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_7TH2BAQE1Q7J6MABMC316AD0ZB
- event_2SZBF08CQFHZKYWN1ZN1MA98D5
---

# Understand the Tree King

Learn why the masked king can harness amber and whether his communion with the Frontier is real.
