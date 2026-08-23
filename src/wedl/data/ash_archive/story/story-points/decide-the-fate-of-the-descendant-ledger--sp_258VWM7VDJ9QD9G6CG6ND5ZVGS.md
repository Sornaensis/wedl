---
schema: wedl/v0.3
kind: story-point
id: sp_258VWM7VDJ9QD9G6CG6ND5ZVGS
title: Decide the Fate of the Descendant Ledger
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_4N9HKQ6EZD7RZ1599FWJVPKQQV
    time:
      timeline: main
      tick: 171
      order: 20
    state: active
    causing_event: event_545ZXMGNRB16VVV7HGWD7W9J17
    note: Ilyra explains the ledger's danger.
  - id: spt_3YBCJJ4AWX5PZFA7MMBKBME02Z
    time:
      timeline: main
      tick: 179
      order: 35
    state: resolved
    causing_event: event_3XWNSHT5NFRM0SN2WJDV0Y25ZX
    note: The ledger is dismantled into unindexed identity packets and placed under expiring threefold custody.
activation_policy: manual
priority: 100
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_545ZXMGNRB16VVV7HGWD7W9J17
on_activate:
  create_draft_scene: false
outcome_events:
- event_3XWNSHT5NFRM0SN2WJDV0Y25ZX
---

# Decide the Fate of the Descendant Ledger

Choose destruction, redaction, distributed custody, or another design for a record that is both proof and weapon.
