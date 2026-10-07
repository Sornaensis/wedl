---
schema: wedl/v0.3
kind: story-point
id: sp_5JW98KWBPBPHFYSA7MMC796ZAJ
title: Enter Saint Orra's Mine
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_46MZ2A97XST2ZNM7W54MK6640Z
    time:
      timeline: main
      tick: 84
      order: 30
    state: active
    causing_event: event_71BDWMBEY0J1HM0BNVH3EMNPPX
    note: The party wins access to the sealed drift.
  - id: spt_5NBQP5MTEWPCQHF0F97BV570GE
    time:
      timeline: main
      tick: 86
      order: 0
    state: resolved
    causing_event: event_15JS2KMNQS88FY8GAYXBV1MBPA
    note: The company crosses the green lamp line.
activation_policy: suggest
priority: 82
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_15JS2KMNQS88FY8GAYXBV1MBPA
---

# Enter Saint Orra's Mine

Investigate the sealed drifts and missing lower crew despite Jorund's resistance.
