---
schema: wedl/v0.3
kind: story-point
id: sp_4PDSD15X3XG4JKJBRKZH9421FH
title: Expose the Listening Office
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_16KNSCA0KJ639233MJS2CD5RHD
    time:
      timeline: main
      tick: 150
      order: 0
    state: active
    causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    note: Ilyra's warning makes the listening route actionable.
  - id: spt_5XE6DF9Q1YRZNJ4J8JMNJ18NRR
    time:
      timeline: main
      tick: 156
      order: 30
    state: resolved
    causing_event: event_2APT0044SZ82863CW6P3KK2T9F
    note: The active ledger is numbered as evidence.
activation_policy: manual
priority: 92
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_651JGHAPM9JY5GWASRC097DFZ3
on_activate:
  create_draft_scene: false
outcome_events:
- event_2APT0044SZ82863CW6P3KK2T9F
---

# Expose the Listening Office

Prove that the supposedly decommissioned office continued listening to the Archive.
