---
schema: wedl/v0.3
kind: story-point
id: sp_76YKCXX41HQC1N17A7S3ZKDRW4
title: Learn What Counts as Pay
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_473WXDX2VKAGW5XRTR2PWBJ75S
    time:
      timeline: main
      tick: 24
      order: 55
    state: resolved
    causing_event: event_52DCBYZ42SGE56QS71Z2917G4N
    note: The party receives and can spend amber wages.
activation_policy: suggest
priority: 72
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_52DCBYZ42SGE56QS71Z2917G4N
---

# Learn What Counts as Pay

Understand that guild wages and settlement protection fees are paid in amber rather than ordinary coin.
