---
schema: wedl/v0.3
kind: story-point
id: sp_6F4KPDTHR877JMR17WQ362W4J5
title: Pass Pressure Gate Seven
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_2VA7CWPMTGXNJWEE5HVE3JREN1
    time:
      timeline: main
      tick: 142
      order: 40
    state: active
    causing_event: event_0QHKQ1BYF3K36RZ7KH7HBRDFDG
    note: The flood stair becomes the escape route.
  - id: spt_6HYHC8SKSVAMM56AA6X25C7CC2
    time:
      timeline: main
      tick: 145
      order: 10
    state: resolved
    causing_event: event_58QVWKM8WN61NYC9GDQ28WZEYE
    note: Ansel opens the gate.
activation_policy: manual
priority: 90
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_0QHKQ1BYF3K36RZ7KH7HBRDFDG
on_activate:
  create_draft_scene: false
outcome_events:
- event_58QVWKM8WN61NYC9GDQ28WZEYE
---

# Pass Pressure Gate Seven

Reach and safely pass Gate Seven before Rusk finds the lower route.
