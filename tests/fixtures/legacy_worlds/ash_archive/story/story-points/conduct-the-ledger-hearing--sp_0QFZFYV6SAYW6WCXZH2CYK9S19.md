---
schema: wedl/v0.3
kind: story-point
id: sp_0QFZFYV6SAYW6WCXZH2CYK9S19
title: Conduct the Ledger Hearing
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_5D3WP59C3SY6R5RQKMSJ6YVGHD
    time:
      timeline: main
      tick: 160
      order: 0
    state: active
    causing_event: event_03W0PC6646ZK48PEQXFQ6Q3BGB
    note: The public hearing begins.
  - id: spt_50AM6KDVRBY9X3NQGRWV2N9EFB
    time:
      timeline: main
      tick: 164
      order: 0
    state: resolved
    causing_event: event_0B23Z86HZ220VVPZ4BZF7RGDT5
    note: Ysabet admits the chain.
activation_policy: manual
priority: 86
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_2APT0044SZ82863CW6P3KK2T9F
on_activate:
  create_draft_scene: false
outcome_events:
- event_0B23Z86HZ220VVPZ4BZF7RGDT5
---

# Conduct the Ledger Hearing

Preserve the evidence chain through public challenge without overstating what it proves.
