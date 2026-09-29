---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_2KFWNBNCG0RPGS4R85M5DQVV9B
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_5AT02B80HAMCF29H10GY014XJ7
    id: spt_1QND3MFRJ6JTN3314EFBDBEA43
    note: Kellan begins giving the surviving account.
    state: active
    time:
      order: 10
      tick: 53
      timeline: main
  - causing_event: event_65NWKCGHET513YTAQPKXVS8KQ5
    id: spt_11AH1G3Y1TTMHY4ZV24QX0HHDR
    note: Kellan's testimony and the repeated trigger reconstruct the attack sequence.
    state: resolved
    time:
      order: 10
      tick: 64
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_2DZVKF0PP4ZNMHYMPSSHMWN1EX
- event_54PDHHTT83P1MYCD9T1X0S33Z1
priority: 88
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Reconstruct the Last Watch
trigger:
  all: []
---

# Reconstruct the Last Watch

Determine how the watch died inside a barred barracks and what Kellan actually heard.
