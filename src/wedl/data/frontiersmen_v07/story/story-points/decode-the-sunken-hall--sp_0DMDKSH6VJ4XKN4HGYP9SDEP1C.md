---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_0DMDKSH6VJ4XKN4HGYP9SDEP1C
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_314JMK73N3KMHCBR7RW673BXC5
    id: spt_49ZGVY0DNADZ3PVWX7QW0QHZR1
    note: The party begins reading the Sunken Hall.
    state: active
    time:
      order: 20
      tick: 122
      timeline: main
  - causing_event: event_071AM9366WK3A503C0K3N23HE3
    id: spt_64DRERFV132BDRC5X19QD7JP9K
    note: Veyra translates the central amber warning.
    state: resolved
    time:
      order: 15
      tick: 124
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_071AM9366WK3A503C0K3N23HE3
priority: 89
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Decode the Sunken Hall
trigger:
  all: []
---

# Decode the Sunken Hall

Interpret the old reliefs and tablet describing amber as a vessel for remembered forms.
