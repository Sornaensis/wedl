---
activation_policy: suggest
aliases: []
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
outcome_events:
- event_5RBQTGPVEZ6TGXR0CM7A1SGZ4M
priority: 70
schema: wedl/v1
status: canonical
tags:
- catalog
- investigation
title: Search the Catalog Gap
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
requires:
- id: sp_022QNDXCYPD5EWJHJCY3VKP9Q3
  states:
  - resolved
when:
  all:
  - knowledge:
      character: char_00HBQM4T4CMF11RKMNDBRP92QC
      claim: catalog.gap.exists
      states:
      - accepted
      minimum_confidence: 0.6
  - state:
      entity: obj_07BE1N7XS519W9GWNPR7PB248P
      key: holder
      equals:
        entity: char_0T8KVZX33X5FQJFYPMH70FADD9
---

# Search the Catalog Gap

Mara and Nessa can use Token 7B and the burned coordinate to search the missing catalog run.


The search is resolved when Token 7B opens the seventh-drawer rail at tick 126.
