---
schema: wedl/v0.3
kind: story-point
id: sp_7VDVQCDD7VGSGHMD95RT3SJ0TN
title: Flee the Root Host
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_78MD9XGBATSJDZ573CX7QFFYCK
    time:
      timeline: main
      tick: 189
      order: 10
    state: active
    causing_event: event_34VWF280Y3BEFV6PYEAH17EJGT
    note: The Root Host raises the camp-wide pursuit alarm.
  - id: spt_4AXR2WKGV8DNBBQAN4PFJDA9BK
    time:
      timeline: main
      tick: 202
      order: 20
    state: resolved
    causing_event: event_27J1R8D9M32H638DCMQC1G22ZH
    note: Running water divides the reliquary signal and the immediate hunting lines pass away from the party.
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
- event_27J1R8D9M32H638DCMQC1G22ZH
---

# Flee the Root Host

Stay ahead of the masked host through the deep northern woods.
