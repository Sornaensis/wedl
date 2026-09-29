---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_5JW98KWBPBPHFYSA7MMC796ZAJ
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_71BDWMBEY0J1HM0BNVH3EMNPPX
    id: spt_46MZ2A97XST2ZNM7W54MK6640Z
    note: The party wins access to the sealed drift.
    state: active
    time:
      order: 30
      tick: 84
      timeline: main
  - causing_event: event_15JS2KMNQS88FY8GAYXBV1MBPA
    id: spt_5NBQP5MTEWPCQHF0F97BV570GE
    note: The company crosses the green lamp line.
    state: resolved
    time:
      order: 0
      tick: 86
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_15JS2KMNQS88FY8GAYXBV1MBPA
priority: 82
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Enter Saint Orra's Mine
trigger:
  all: []
---

# Enter Saint Orra's Mine

Investigate the sealed drifts and missing lower crew despite Jorund's resistance.
