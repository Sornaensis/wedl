---
schema: wedl/v0.3
kind: story-point
id: sp_05AYKKC34KZ155A2YBVTZJF4VN
title: Cross the Lower Caverns
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_3XRNBDZKRRQ0Z2SFN2SBJK5VNG
    time:
      timeline: main
      tick: 101
      order: 0
    state: active
    causing_event: event_0PPHBSJACDHS2BDKJE1ZYEP02J
    note: The party enters unmapped lower caverns.
  - id: spt_4K3WE2ZEZGW1NJNDZDWM5S6S3M
    time:
      timeline: main
      tick: 122
      order: 20
    state: resolved
    causing_event: event_314JMK73N3KMHCBR7RW673BXC5
    note: The lower route reaches constructed ruins and a new navigational frame.
activation_policy: suggest
priority: 90
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_0WRPYJT2TS2ZCXGR05HDF6TYRY
- event_5ZY58D2JG3TCJNX8F7NR0TKG40
- event_314JMK73N3KMHCBR7RW673BXC5
---

# Cross the Lower Caverns

Travel through two days of unmapped cavern systems with dwindling food and lamps.
