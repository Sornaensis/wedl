---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_4PDSD15X3XG4JKJBRKZH9421FH
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    id: spt_16KNSCA0KJ639233MJS2CD5RHD
    note: Ilyra's warning makes the listening route actionable.
    state: active
    time:
      order: 0
      tick: 150
      timeline: main
  - causing_event: event_2APT0044SZ82863CW6P3KK2T9F
    id: spt_5XE6DF9Q1YRZNJ4J8JMNJ18NRR
    note: The active ledger is numbered as evidence.
    state: resolved
    time:
      order: 30
      tick: 156
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_2APT0044SZ82863CW6P3KK2T9F
priority: 92
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Expose the Listening Office
trigger:
  event:
    event: event_651JGHAPM9JY5GWASRC097DFZ3
---

# Expose the Listening Office

Prove that the supposedly decommissioned office continued listening to the Archive.
