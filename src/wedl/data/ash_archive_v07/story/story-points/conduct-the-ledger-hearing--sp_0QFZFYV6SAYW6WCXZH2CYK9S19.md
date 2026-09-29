---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_0QFZFYV6SAYW6WCXZH2CYK9S19
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_03W0PC6646ZK48PEQXFQ6Q3BGB
    id: spt_5D3WP59C3SY6R5RQKMSJ6YVGHD
    note: The public hearing begins.
    state: active
    time:
      order: 0
      tick: 160
      timeline: main
  - causing_event: event_0B23Z86HZ220VVPZ4BZF7RGDT5
    id: spt_50AM6KDVRBY9X3NQGRWV2N9EFB
    note: Ysabet admits the chain.
    state: resolved
    time:
      order: 0
      tick: 164
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_0B23Z86HZ220VVPZ4BZF7RGDT5
priority: 86
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Conduct the Ledger Hearing
trigger:
  event:
    event: event_2APT0044SZ82863CW6P3KK2T9F
---

# Conduct the Ledger Hearing

Preserve the evidence chain through public challenge without overstating what it proves.
