---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_3J1S28X5AZ261VA09J5RS573EN
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_5PE0EKDCDMGT41TT616CBN5F23
    id: spt_541EB5TTRSMV90VZFRWJ3XAFES
    note: The mine collapses between the party and the surface.
    state: active
    time:
      order: 0
      tick: 91
      timeline: main
  - causing_event: event_0PPHBSJACDHS2BDKJE1ZYEP02J
    id: spt_1ZT64DCCGYX323JW61Z3VMFDAV
    note: The party finds lower air and a viable route away from the collapse.
    state: resolved
    time:
      order: 0
      tick: 101
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_0PPHBSJACDHS2BDKJE1ZYEP02J
priority: 100
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Escape the Cave-In
trigger:
  all: []
---

# Escape the Cave-In

Survive the collapse, find an alternate route, and keep the company together underground.
