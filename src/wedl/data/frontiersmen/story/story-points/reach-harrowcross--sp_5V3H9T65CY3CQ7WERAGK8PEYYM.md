---
schema: wedl/v0.3
kind: story-point
id: sp_5V3H9T65CY3CQ7WERAGK8PEYYM
title: Reach Harrowcross
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_7A24XTT906K1Y4SGEMCRWQAQ2C
    time:
      timeline: main
      tick: 50
      order: 20
    state: resolved
    causing_event: event_5AT02B80HAMCF29H10GY014XJ7
    note: The caravan reaches Harrowcross.
activation_policy: suggest
priority: 78
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_5AT02B80HAMCF29H10GY014XJ7
---

# Reach Harrowcross

Bring the caravan to the crossroads town where the coast road meets the mine track.
