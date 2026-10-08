---
schema: wedl/v0.7
kind: world
id: world_7XGTHK671HA355KN17X2HRET5G
title: Tideglass
capabilities:
- generational-core-v1
- spatial-core-v1
- geometry-v1
- route-v1
- overlay-v1
domain: tideglass.world
status: canonical
tags:
- tideglass
aliases: []
default_timeline: main
timelines:
- id: main
  label: Tideglass repair sequence
threads: []
section_audiences:
  public:
  - public
compilation_policy:
  default_profile: fts
  incremental_mode: atomic-rebuild
  retain_revisions: 5
  source_cache: true
  vector_cache: true
context_policy:
  conversation_turn_window: 6
  default_budget_chars: 8000
  default_max_items: 24
current_time:
  order: 1
  tick: 40
  timeline: main
relationship_metrics:
  obligation:
    default: 0.0
    max: 1.0
    min: 0.0
  trust:
    default: 0.0
    max: 1.0
    min: -1.0
state_keys:
  character:
    condition:
      type: string
    location:
      exclusive_group: placement
      kind: location
      type: entity
  object:
    condition:
      type: string
    container:
      exclusive_group: placement
      kind: object
      type: entity
    holder:
      exclusive_group: placement
      kind: character
      type: entity
    location:
      exclusive_group: placement
      kind: location
      type: entity
---

# Tideglass

## Public

The storm leaves the beacon dark and the ferry suspended. Mina coordinates a practical repair, Orin checks the crossing, Jessa examines the maintenance record, and Tavi repairs the lens. Cora keeps the beacon office while Sela helps trace the mistaken entry. The investigation finds a copied inspection note, not a completed repair. Repair, evidence and authorization are recorded as separate authored events. Story ticks order these facts and do not measure hours or years.
