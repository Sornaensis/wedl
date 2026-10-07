---
schema: wedl/v0.3
kind: story-point
id: sp_03D20GFYW8ATVTGHF0BA30P0SJ
title: Search the Catalog Gap
domain: plot.catalog
status: canonical
tags:
- catalog
- investigation
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_0QC5ZF6QWQXS3ERTV6ZM3SZG5Y
    time:
      timeline: main
      tick: 126
      order: 10
    state: resolved
    causing_event: event_5RBQTGPVEZ6TGXR0CM7A1SGZ4M
    note: Token 7B opens the missing-card rail and turns the gap into a physical route.
activation_policy: suggest
priority: 70
repeat_policy: once
dependencies:
  all:
  - story_point: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
    state_in:
    - resolved
trigger:
  all:
  - knowledge:
      knower: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim_key: catalog.gap.exists
      state_in:
      - accepted
      minimum_confidence: 0.6
  - entity_state:
      target: obj_07BE1N7XS519W9GWNPR7PB248P
      key: holder
      equals:
        entity: char_0T8KVZX33X5FQJFYPMH70FADD9
on_activate:
  create_draft_scene: false
outcome_events:
- event_5RBQTGPVEZ6TGXR0CM7A1SGZ4M
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Search the Catalog Gap

Mara and Nessa can use Token 7B and the burned coordinate to search the missing catalog run.


The search is resolved when Token 7B opens the seventh-drawer rail at tick 126.
