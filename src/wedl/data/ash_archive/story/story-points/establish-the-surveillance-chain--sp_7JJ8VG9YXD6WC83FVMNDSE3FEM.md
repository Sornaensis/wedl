---
schema: wedl/v0.3
kind: story-point
id: sp_7JJ8VG9YXD6WC83FVMNDSE3FEM
title: Establish the Surveillance Chain
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_3780J9TNYB9MT303WS0CX40HJW
    time:
      timeline: main
      tick: 154
      order: 10
    state: active
    causing_event: event_69VN0C6N32DBNXQAN6PCJ5V3Q9
    note: The testimony identifies distinct actors.
  - id: spt_4H9RMA835EBJHVVZK4TMYK463R
    time:
      timeline: main
      tick: 164
      order: 0
    state: resolved
    causing_event: event_0B23Z86HZ220VVPZ4BZF7RGDT5
    note: The chain is admitted while responsibility stays contested.
activation_policy: manual
priority: 93
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_2QYWWCR1CMMDKYKKFM951DZV8K
on_activate:
  create_draft_scene: false
outcome_events:
- event_0B23Z86HZ220VVPZ4BZF7RGDT5
---

# Establish the Surveillance Chain

Connect Rusk's requisition, Caldrin's receiver, Halver's dates, and the live listening ledger.
