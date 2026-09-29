---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_258VWM7VDJ9QD9G6CG6ND5ZVGS
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_545ZXMGNRB16VVV7HGWD7W9J17
    id: spt_4N9HKQ6EZD7RZ1599FWJVPKQQV
    note: Ilyra explains the ledger's danger.
    state: active
    time:
      order: 20
      tick: 171
      timeline: main
  - causing_event: event_3XWNSHT5NFRM0SN2WJDV0Y25ZX
    id: spt_3YBCJJ4AWX5PZFA7MMBKBME02Z
    note: The ledger is dismantled into unindexed identity packets and placed under expiring threefold custody.
    state: resolved
    time:
      order: 35
      tick: 179
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_3XWNSHT5NFRM0SN2WJDV0Y25ZX
priority: 100
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Decide the Fate of the Descendant Ledger
trigger:
  event:
    event: event_545ZXMGNRB16VVV7HGWD7W9J17
---

# Decide the Fate of the Descendant Ledger

Choose destruction, redaction, distributed custody, or another design for a record that is both proof and weapon.
