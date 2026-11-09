---
activation_policy: manual
aliases: []
domain: plot.vault
id: sp_0ACNT6ACRNQF75MN4P7D2Q9XMQ
kind: story-point
lifecycle:
  initial_state: dormant
  transitions: []
outcome_events: []
priority: 85
schema: wedl/v1
status: canonical
tags:
- vault
- discovery
title: Open the Restricted Vault
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
requires:
- id: sp_03D20GFYW8ATVTGHF0BA30P0SJ
  states:
  - active
  - resolved
- id: sp_05KWYYN6G5CFVZR0EN1MNCVQGQ
  states:
  - resolved
when:
  knowledge:
    character: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim: vault.watch-sealed
    states:
    - accepted
    minimum_confidence: 0.6
---

# Open the Restricted Vault

Opening the vault should follow discovery of the catalog route and recovery or replacement of the key; its dependencies are intentionally unmet.
