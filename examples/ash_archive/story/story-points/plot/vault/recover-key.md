---
activation_policy: suggest
aliases: []
domain: plot.vault
id: sp_05KWYYN6G5CFVZR0EN1MNCVQGQ
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_1H444APDHD9WCRNJB9MJGJHD56
    id: spt_1MXR60Z0VPGHK8F661WYFD9QNX
    note: Ilyra can return the key once the Archive’s authority is redefined.
    state: active
    time:
      order: 0
      tick: 205
      timeline: main
  - causing_event: event_65QYK2GWA9WV70TYXSMK8CZG7R
    id: spt_7R83D4C93WW7CWYSKA0XG0ZHWS
    note: Ilyra transfers the restricted-vault key to Mara under the new review rule.
    state: resolved
    time:
      order: 20
      tick: 206
      timeline: main
outcome_events:
- event_65QYK2GWA9WV70TYXSMK8CZG7R
priority: 75
schema: wedl/v1
status: canonical
tags:
- key
- vault
title: Recover the Restricted-Vault Key
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
requires:
- id: sp_0EWTQTRW748QTCS592XRSDVV80
  states:
  - active
  - resolved
when:
  all:
  - knowledge:
      character: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim: ilyra.status.missing
      states:
      - accepted
      minimum_confidence: 0.6
  - state:
      entity: obj_0KC1YJH9MFDT31AWC1M7HKD36X
      key: holder
      equals:
        entity: char_0JS5X2Q20CFGQTHD59XZGW684E
---

# Recover the Restricted-Vault Key

The vault cannot be opened lawfully while Ilyra and her key are both missing.
