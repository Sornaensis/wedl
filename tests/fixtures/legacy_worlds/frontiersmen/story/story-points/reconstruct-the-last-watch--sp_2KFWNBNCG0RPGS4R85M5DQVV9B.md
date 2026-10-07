---
schema: wedl/v0.3
kind: story-point
id: sp_2KFWNBNCG0RPGS4R85M5DQVV9B
title: Reconstruct the Last Watch
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_1QND3MFRJ6JTN3314EFBDBEA43
    time:
      timeline: main
      tick: 53
      order: 10
    state: active
    causing_event: event_5AT02B80HAMCF29H10GY014XJ7
    note: Kellan begins giving the surviving account.
  - id: spt_11AH1G3Y1TTMHY4ZV24QX0HHDR
    time:
      timeline: main
      tick: 64
      order: 10
    state: resolved
    causing_event: event_65NWKCGHET513YTAQPKXVS8KQ5
    note: Kellan's testimony and the repeated trigger reconstruct the attack sequence.
activation_policy: suggest
priority: 88
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_2DZVKF0PP4ZNMHYMPSSHMWN1EX
- event_54PDHHTT83P1MYCD9T1X0S33Z1
---

# Reconstruct the Last Watch

Determine how the watch died inside a barred barracks and what Kellan actually heard.
