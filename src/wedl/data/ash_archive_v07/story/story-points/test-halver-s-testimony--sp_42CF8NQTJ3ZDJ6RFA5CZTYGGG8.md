---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_42CF8NQTJ3ZDJ6RFA5CZTYGGG8
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_7Z1C63ZAP0496Y6AD1B1APQ3FR
    id: spt_3ENZ0Z6MD2VJK7N3Y5HYRGQR2Q
    note: Halver is found in the office.
    state: active
    time:
      order: 20
      tick: 152
      timeline: main
  - causing_event: event_69VN0C6N32DBNXQAN6PCJ5V3Q9
    id: spt_5VTR95JQSDQ1GA9E79Q10XZXHV
    note: His qualified admission is recorded.
    state: resolved
    time:
      order: 10
      tick: 154
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_69VN0C6N32DBNXQAN6PCJ5V3Q9
priority: 80
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Test Halver's Testimony
trigger:
  event:
    event: event_69VN0C6N32DBNXQAN6PCJ5V3Q9
---

# Test Halver's Testimony

Separate what Halver copied, inferred, and altered under threat.
