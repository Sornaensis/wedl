---
schema: wedl/v0.3
kind: story-point
id: sp_6EW4SA94Y6TT1JAF3QEW1FJRTJ
title: Escape Before Rusk Opens the North Door
domain: plot.expanded
status: canonical
tags:
- choice
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_3HAWDVBADN5TC6S90GTVXXYS9F
    time:
      timeline: main
      tick: 142
      order: 40
    state: resolved
    causing_event: event_0QHKQ1BYF3K36RZ7KH7HBRDFDG
    note: Mara and Nessa take the lower passage before the door opens.
activation_policy: manual
priority: 100
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_3W0QZ7TF8PMZYSQF0M338RTYYP
on_activate:
  create_draft_scene: false
outcome_events:
- event_0QHKQ1BYF3K36RZ7KH7HBRDFDG
---

# Escape Before Rusk Opens the North Door

Leave Flood Gallery N with enough admissible evidence before the search party reaches the door.

The second act advances this thread at tick 142.
