---
schema: wedl/v0.3
kind: story-point
id: sp_0C5M4JA1VK0J4QFSVNQ522Q7XW
title: Decode the Ash-Glass Map
domain: plot.river
status: canonical
tags:
- map
- undercroft
aliases: []
lifecycle:
  initial_state: dormant
  transitions: []
activation_policy: suggest
priority: 60
repeat_policy: once
dependencies:
  all:
  - story_point: sp_013ACJ6297Q206Q3PECAERP90R
    state_in:
    - active
    - resolved
trigger:
  entity_state:
    target: obj_0FC34G7EBBC03HMGB9Z20WWPDB
    key: holder
    equals:
      entity: char_00HBQM4T4CMF11RKMNDBRP92QC
on_activate:
  create_draft_scene: false
outcome_events: []
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Decode the Ash-Glass Map

Mara must first obtain the confiscated map tile. Its route is an author secret at the active scene.
