---
schema: wedl/v0.3
kind: story-point
id: sp_0DMDKSH6VJ4XKN4HGYP9SDEP1C
title: Decode the Sunken Hall
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_49ZGVY0DNADZ3PVWX7QW0QHZR1
    time:
      timeline: main
      tick: 122
      order: 20
    state: active
    causing_event: event_314JMK73N3KMHCBR7RW673BXC5
    note: The party begins reading the Sunken Hall.
  - id: spt_64DRERFV132BDRC5X19QD7JP9K
    time:
      timeline: main
      tick: 124
      order: 15
    state: resolved
    causing_event: event_071AM9366WK3A503C0K3N23HE3
    note: Veyra translates the central amber warning.
activation_policy: suggest
priority: 89
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_071AM9366WK3A503C0K3N23HE3
---

# Decode the Sunken Hall

Interpret the old reliefs and tablet describing amber as a vessel for remembered forms.
