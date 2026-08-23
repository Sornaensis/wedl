---
schema: wedl/v0.3
kind: story-point
id: sp_05KWYYN6G5CFVZR0EN1MNCVQGQ
title: Recover the Restricted-Vault Key
domain: plot.vault
status: canonical
tags:
- key
- vault
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_1MXR60Z0VPGHK8F661WYFD9QNX
    time:
      timeline: main
      tick: 205
      order: 0
    state: active
    causing_event: event_1H444APDHD9WCRNJB9MJGJHD56
    note: Ilyra can return the key once the Archive’s authority is redefined.
  - id: spt_7R83D4C93WW7CWYSKA0XG0ZHWS
    time:
      timeline: main
      tick: 206
      order: 20
    state: resolved
    causing_event: event_65QYK2GWA9WV70TYXSMK8CZG7R
    note: Ilyra transfers the restricted-vault key to Mara under the new review rule.
activation_policy: suggest
priority: 75
repeat_policy: once
dependencies:
  all:
  - story_point: sp_0EWTQTRW748QTCS592XRSDVV80
    state_in:
    - active
    - resolved
trigger:
  all:
  - knowledge:
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim_key: ilyra.status.missing
      state_in:
      - accepted
      minimum_confidence: 0.6
  - entity_state:
      target: obj_0KC1YJH9MFDT31AWC1M7HKD36X
      key: holder
      equals:
        entity: char_0JS5X2Q20CFGQTHD59XZGW684E
on_activate:
  create_draft_scene: false
outcome_events:
- event_65QYK2GWA9WV70TYXSMK8CZG7R
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Recover the Restricted-Vault Key

The vault cannot be opened lawfully while Ilyra and her key are both missing.
