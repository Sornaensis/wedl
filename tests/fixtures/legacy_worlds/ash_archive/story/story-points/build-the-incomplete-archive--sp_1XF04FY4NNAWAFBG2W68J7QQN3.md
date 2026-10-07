---
schema: wedl/v0.3
kind: story-point
id: sp_1XF04FY4NNAWAFBG2W68J7QQN3
title: Build the Incomplete Archive
domain: plot.ash-archive.ending
status: canonical
tags:
- archive-reform
- epilogue
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_13H6GD8MRXB0147XGCNVX3002K
    time:
      timeline: main
      tick: 205
      order: 0
    state: active
    causing_event: event_1H444APDHD9WCRNJB9MJGJHD56
    note: The Archive can reopen only after defining accountable absence.
  - id: spt_47VNNJH9GCK97S5610FHGW7NZM
    time:
      timeline: main
      tick: 206
      order: 20
    state: resolved
    causing_event: event_65QYK2GWA9WV70TYXSMK8CZG7R
    note: The first absence entry and review rule are recorded.
activation_policy: manual
priority: 90
repeat_policy: once
dependencies:
  all:
  - story_point: sp_258VWM7VDJ9QD9G6CG6ND5ZVGS
    state_in:
    - resolved
trigger:
  event:
    event: event_1H444APDHD9WCRNJB9MJGJHD56
on_activate:
  create_draft_scene: false
outcome_events:
- event_65QYK2GWA9WV70TYXSMK8CZG7R
---

# Build the Incomplete Archive

Create a catalog that records accountable omissions without preserving a central reconstruction key.
