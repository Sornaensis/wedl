---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_7VDVQCDD7VGSGHMD95RT3SJ0TN
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_34VWF280Y3BEFV6PYEAH17EJGT
    id: spt_78MD9XGBATSJDZ573CX7QFFYCK
    note: The Root Host raises the camp-wide pursuit alarm.
    state: active
    time:
      order: 10
      tick: 189
      timeline: main
  - causing_event: event_27J1R8D9M32H638DCMQC1G22ZH
    id: spt_4AXR2WKGV8DNBBQAN4PFJDA9BK
    note: Running water divides the reliquary signal and the immediate hunting lines pass away from the party.
    state: resolved
    time:
      order: 20
      tick: 202
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_27J1R8D9M32H638DCMQC1G22ZH
priority: 100
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Flee the Root Host
trigger:
  all: []
---

# Flee the Root Host

Stay ahead of the masked host through the deep northern woods.
