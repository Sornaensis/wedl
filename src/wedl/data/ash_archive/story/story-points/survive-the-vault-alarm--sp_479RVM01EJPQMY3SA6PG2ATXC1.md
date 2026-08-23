---
schema: wedl/v0.3
kind: story-point
id: sp_479RVM01EJPQMY3SA6PG2ATXC1
title: Survive the Vault Alarm
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_767FXVAQAV85TV4C5ZQRWZWSDD
    time:
      timeline: main
      tick: 177
      order: 30
    state: active
    causing_event: event_5P7XRVSGPQ0WJZKKHYZYKFCDZC
    note: The shutters begin closing.
  - id: spt_037X3ZNAGCRF641QZYF43KRYTB
    time:
      timeline: main
      tick: 188
      order: 0
    state: resolved
    causing_event: event_2H5KG82XHG18KWE5WERDBQVSE8
    note: The custodians clear the flood route before the shutters and pressure gate close.
activation_policy: manual
priority: 100
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_5P7XRVSGPQ0WJZKKHYZYKFCDZC
on_activate:
  create_draft_scene: false
outcome_events:
- event_2H5KG82XHG18KWE5WERDBQVSE8
---

# Survive the Vault Alarm

Reach a safe exit before the fire shutters and flood pressure close both routes.
