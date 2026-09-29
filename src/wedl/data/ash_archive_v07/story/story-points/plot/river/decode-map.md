---
activation_policy: suggest
aliases: []
dependencies:
  all:
  - state_in:
    - active
    - resolved
    story_point: sp_013ACJ6297Q206Q3PECAERP90R
domain: plot.river
id: sp_0C5M4JA1VK0J4QFSVNQ522Q7XW
kind: story-point
lifecycle:
  initial_state: dormant
  transitions: []
on_activate:
  create_draft_scene: false
outcome_events: []
priority: 60
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- map
- undercroft
title: Decode the Ash-Glass Map
trigger:
  entity_state:
    equals:
      entity: char_00HBQM4T4CMF11RKMNDBRP92QC
    key: holder
    target: obj_0FC34G7EBBC03HMGB9Z20WWPDB
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Decode the Ash-Glass Map

Mara must first obtain the confiscated map tile. Its route is an author secret at the active scene.
