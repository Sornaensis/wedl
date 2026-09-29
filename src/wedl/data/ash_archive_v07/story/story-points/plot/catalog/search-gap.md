---
activation_policy: suggest
aliases: []
dependencies:
  all:
  - state_in:
    - resolved
    story_point: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
domain: plot.catalog
id: sp_03D20GFYW8ATVTGHF0BA30P0SJ
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_5RBQTGPVEZ6TGXR0CM7A1SGZ4M
    id: spt_0QC5ZF6QWQXS3ERTV6ZM3SZG5Y
    note: Token 7B opens the missing-card rail and turns the gap into a physical route.
    state: resolved
    time:
      order: 10
      tick: 126
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_5RBQTGPVEZ6TGXR0CM7A1SGZ4M
priority: 70
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- catalog
- investigation
title: Search the Catalog Gap
trigger:
  all:
  - knowledge:
      claim_key: catalog.gap.exists
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      minimum_confidence: 0.6
      state_in:
      - accepted
  - entity_state:
      equals:
        entity: char_0T8KVZX33X5FQJFYPMH70FADD9
      key: holder
      target: obj_07BE1N7XS519W9GWNPR7PB248P
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Search the Catalog Gap

Mara and Nessa can use Token 7B and the burned coordinate to search the missing catalog run.


The search is resolved when Token 7B opens the seventh-drawer rail at tick 126.
