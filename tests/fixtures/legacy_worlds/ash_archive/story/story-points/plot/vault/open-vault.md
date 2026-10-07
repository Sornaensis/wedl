---
schema: wedl/v0.3
kind: story-point
id: sp_0ACNT6ACRNQF75MN4P7D2Q9XMQ
title: Open the Restricted Vault
domain: plot.vault
status: canonical
tags:
- vault
- discovery
aliases: []
lifecycle:
  initial_state: dormant
  transitions: []
activation_policy: manual
priority: 85
repeat_policy: once
dependencies:
  all:
  - story_point: sp_03D20GFYW8ATVTGHF0BA30P0SJ
    state_in:
    - active
    - resolved
  - story_point: sp_05KWYYN6G5CFVZR0EN1MNCVQGQ
    state_in:
    - resolved
trigger:
  knowledge:
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim_key: vault.watch-sealed
    state_in:
    - accepted
    minimum_confidence: 0.6
on_activate:
  create_draft_scene: false
outcome_events: []
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Open the Restricted Vault

Opening the vault should follow discovery of the catalog route and recovery or replacement of the key; its dependencies are intentionally unmet.
