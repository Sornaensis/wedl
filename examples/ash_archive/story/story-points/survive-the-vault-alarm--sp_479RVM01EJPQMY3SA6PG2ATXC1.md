---
activation_policy: manual
aliases: []
domain: plot.ash-archive.second-act
id: sp_479RVM01EJPQMY3SA6PG2ATXC1
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_5P7XRVSGPQ0WJZKKHYZYKFCDZC
    id: spt_767FXVAQAV85TV4C5ZQRWZWSDD
    note: The shutters begin closing.
    state: active
    time:
      order: 30
      tick: 177
      timeline: main
  - causing_event: event_2H5KG82XHG18KWE5WERDBQVSE8
    id: spt_037X3ZNAGCRF641QZYF43KRYTB
    note: The custodians clear the flood route before the shutters and pressure gate close.
    state: resolved
    time:
      order: 0
      tick: 188
      timeline: main
outcome_events:
- event_2H5KG82XHG18KWE5WERDBQVSE8
priority: 100
schema: wedl/v1
status: canonical
tags:
- second-act
title: Survive the Vault Alarm
requires: []
when:
  event: event_5P7XRVSGPQ0WJZKKHYZYKFCDZC
---

# Survive the Vault Alarm

Reach a safe exit before the fire shutters and flood pressure close both routes.
