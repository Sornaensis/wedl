---
activation_policy: manual
aliases: []
dependencies:
  all:
  - state_in:
    - active
    - resolved
    story_point: sp_03D20GFYW8ATVTGHF0BA30P0SJ
  - state_in:
    - resolved
    story_point: sp_05KWYYN6G5CFVZR0EN1MNCVQGQ
domain: plot.vault
id: sp_0ACNT6ACRNQF75MN4P7D2Q9XMQ
kind: story-point
lifecycle:
  initial_state: dormant
  transitions: []
on_activate:
  create_draft_scene: false
outcome_events: []
priority: 85
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- vault
- discovery
title: Open the Restricted Vault
trigger:
  knowledge:
    claim_key: vault.watch-sealed
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    minimum_confidence: 0.6
    state_in:
    - accepted
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Open the Restricted Vault

Opening the vault should follow discovery of the catalog route and recovery or replacement of the key; its dependencies are intentionally unmet.
