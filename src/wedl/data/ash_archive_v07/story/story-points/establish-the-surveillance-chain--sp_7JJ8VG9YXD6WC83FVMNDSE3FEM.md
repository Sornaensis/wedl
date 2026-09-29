---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_7JJ8VG9YXD6WC83FVMNDSE3FEM
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_69VN0C6N32DBNXQAN6PCJ5V3Q9
    id: spt_3780J9TNYB9MT303WS0CX40HJW
    note: The testimony identifies distinct actors.
    state: active
    time:
      order: 10
      tick: 154
      timeline: main
  - causing_event: event_0B23Z86HZ220VVPZ4BZF7RGDT5
    id: spt_4H9RMA835EBJHVVZK4TMYK463R
    note: The chain is admitted while responsibility stays contested.
    state: resolved
    time:
      order: 0
      tick: 164
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_0B23Z86HZ220VVPZ4BZF7RGDT5
priority: 93
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Establish the Surveillance Chain
trigger:
  event:
    event: event_2QYWWCR1CMMDKYKKFM951DZV8K
---

# Establish the Surveillance Chain

Connect Rusk's requisition, Caldrin's receiver, Halver's dates, and the live listening ledger.
