---
schema: wedl/v0.3
kind: story-point
id: sp_5Q8BN1G91QHYSCRSDSF9QWDK09
title: Trace the Attacks to Saint Orra's Mine
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_01ZCHVC44DHAVRWZZV2G3HKS7K
    time:
      timeline: main
      tick: 72
      order: 45
    state: active
    causing_event: event_3X3WCH3S4HA27KAGTQTDP6FPCM
    note: The dowsing fork and wretch route point to Saint Orra.
  - id: spt_1Y93E1WR7N63D941YN73J9KPNB
    time:
      timeline: main
      tick: 80
      order: 20
    state: resolved
    causing_event: event_71BDWMBEY0J1HM0BNVH3EMNPPX
    note: The party reaches the sealed drift that draws the fork.
activation_policy: suggest
priority: 86
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_71BDWMBEY0J1HM0BNVH3EMNPPX
---

# Trace the Attacks to Saint Orra's Mine

Follow the orchard frost, mine road, and dowsing evidence toward the amber workings.
