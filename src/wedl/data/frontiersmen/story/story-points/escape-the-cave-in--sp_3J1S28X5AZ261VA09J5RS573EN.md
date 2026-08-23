---
schema: wedl/v0.3
kind: story-point
id: sp_3J1S28X5AZ261VA09J5RS573EN
title: Escape the Cave-In
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_541EB5TTRSMV90VZFRWJ3XAFES
    time:
      timeline: main
      tick: 91
      order: 0
    state: active
    causing_event: event_5PE0EKDCDMGT41TT616CBN5F23
    note: The mine collapses between the party and the surface.
  - id: spt_1ZT64DCCGYX323JW61Z3VMFDAV
    time:
      timeline: main
      tick: 101
      order: 0
    state: resolved
    causing_event: event_0PPHBSJACDHS2BDKJE1ZYEP02J
    note: The party finds lower air and a viable route away from the collapse.
activation_policy: suggest
priority: 100
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_0PPHBSJACDHS2BDKJE1ZYEP02J
---

# Escape the Cave-In

Survive the collapse, find an alternate route, and keep the company together underground.
