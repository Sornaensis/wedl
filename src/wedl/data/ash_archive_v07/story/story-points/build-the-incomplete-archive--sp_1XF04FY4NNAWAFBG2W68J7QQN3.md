---
activation_policy: manual
aliases: []
dependencies:
  all:
  - state_in:
    - resolved
    story_point: sp_258VWM7VDJ9QD9G6CG6ND5ZVGS
domain: plot.ash-archive.ending
id: sp_1XF04FY4NNAWAFBG2W68J7QQN3
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_1H444APDHD9WCRNJB9MJGJHD56
    id: spt_13H6GD8MRXB0147XGCNVX3002K
    note: The Archive can reopen only after defining accountable absence.
    state: active
    time:
      order: 0
      tick: 205
      timeline: main
  - causing_event: event_65QYK2GWA9WV70TYXSMK8CZG7R
    id: spt_47VNNJH9GCK97S5610FHGW7NZM
    note: The first absence entry and review rule are recorded.
    state: resolved
    time:
      order: 20
      tick: 206
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_65QYK2GWA9WV70TYXSMK8CZG7R
priority: 90
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- archive-reform
- epilogue
title: Build the Incomplete Archive
trigger:
  event:
    event: event_1H444APDHD9WCRNJB9MJGJHD56
---

# Build the Incomplete Archive

Create a catalog that records accountable omissions without preserving a central reconstruction key.
