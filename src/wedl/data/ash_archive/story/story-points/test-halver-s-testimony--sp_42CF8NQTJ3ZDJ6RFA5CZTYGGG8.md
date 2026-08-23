---
schema: wedl/v0.3
kind: story-point
id: sp_42CF8NQTJ3ZDJ6RFA5CZTYGGG8
title: Test Halver's Testimony
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_3ENZ0Z6MD2VJK7N3Y5HYRGQR2Q
    time:
      timeline: main
      tick: 152
      order: 20
    state: active
    causing_event: event_7Z1C63ZAP0496Y6AD1B1APQ3FR
    note: Halver is found in the office.
  - id: spt_5VTR95JQSDQ1GA9E79Q10XZXHV
    time:
      timeline: main
      tick: 154
      order: 10
    state: resolved
    causing_event: event_69VN0C6N32DBNXQAN6PCJ5V3Q9
    note: His qualified admission is recorded.
activation_policy: manual
priority: 80
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_69VN0C6N32DBNXQAN6PCJ5V3Q9
on_activate:
  create_draft_scene: false
outcome_events:
- event_69VN0C6N32DBNXQAN6PCJ5V3Q9
---

# Test Halver's Testimony

Separate what Halver copied, inferred, and altered under threat.
