---
schema: wedl/v0.3
kind: story-point
id: sp_0BAXPMHPXKQQK0VX505E2YRYT9
title: Join the Lantern Pike
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_69P98JRH94165FKKP8N03RWDQ1
    time:
      timeline: main
      tick: 18
      order: 10
    state: resolved
    causing_event: event_63AD4S4G0KGX5JHV8S4RTFDQK9
    note: The five sign and receive guild badges.
activation_policy: suggest
priority: 100
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_63AD4S4G0KGX5JHV8S4RTFDQK9
---

# Join the Lantern Pike

Sign the guild contract and receive the badges that make the party legally responsible for frontier emergencies.
