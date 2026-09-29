---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_6F4KPDTHR877JMR17WQ362W4J5
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_0QHKQ1BYF3K36RZ7KH7HBRDFDG
    id: spt_2VA7CWPMTGXNJWEE5HVE3JREN1
    note: The flood stair becomes the escape route.
    state: active
    time:
      order: 40
      tick: 142
      timeline: main
  - causing_event: event_58QVWKM8WN61NYC9GDQ28WZEYE
    id: spt_6HYHC8SKSVAMM56AA6X25C7CC2
    note: Ansel opens the gate.
    state: resolved
    time:
      order: 10
      tick: 145
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_58QVWKM8WN61NYC9GDQ28WZEYE
priority: 90
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Pass Pressure Gate Seven
trigger:
  event:
    event: event_0QHKQ1BYF3K36RZ7KH7HBRDFDG
---

# Pass Pressure Gate Seven

Reach and safely pass Gate Seven before Rusk finds the lower route.
