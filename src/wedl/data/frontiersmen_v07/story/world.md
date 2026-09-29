---
schema: wedl/v0.7
kind: world
id: world_01M0HH0YXDMTCEX34NPQ7S2AXQ
title: The Frontiersmen
capabilities:
- generational-core-v1
- spatial-core-v1
domain: world
status: canonical
tags:
- example
- frontier-fantasy
- monster-hunters
- interactive
aliases: []
default_timeline: main
timelines:
- id: main
  label: Main chronology
section_audiences:
  Author truth:
  - author
  Current canonical state:
  - author
  Public premise:
  - public
  - author
compilation_policy:
  atomic_publish: true
  default_profile: hybrid
  retained_revisions: 5
  retrieval:
    fts_candidate_limit: 120
    hybrid_fts_weight: 1.0
    hybrid_rrf_k: 60.0
    hybrid_vector_weight: 1.0
    vector_candidate_limit: 120
  source_parse_cache: true
  vector_cache: true
context_policy:
  conversation_turn_window: 6
  default_budget_characters: 8000
  default_max_items: 24
embedding_policy:
  dimensions: 192
  max_features: 8192
  model: wedl-lsa-v1
  provider: lsa
relationship_metrics:
  affinity:
    default: 0.0
    maximum: 1.0
    minimum: -1.0
  dependence:
    default: 0.0
    maximum: 1.0
    minimum: 0.0
  fear:
    default: 0.0
    maximum: 1.0
    minimum: 0.0
  obligation:
    default: 0.0
    maximum: 1.0
    minimum: 0.0
  respect:
    default: 0.0
    maximum: 1.0
    minimum: -1.0
  trust:
    default: 0.0
    maximum: 1.0
    minimum: -1.0
state_keys:
  character:
    condition:
      type: string
    location:
      entity_kind: location
      exclusive: true
      type: entity
  object:
    condition:
      type: string
    container:
      entity_kind: object
      exclusive_group: placement
      type: entity
    holder:
      entity_kind: character
      exclusive_group: placement
      type: entity
    location:
      entity_kind: location
      exclusive_group: placement
      type: entity
---

# The Frontiersmen

## Public premise

The Frontier is a remote boreal peninsula reached through Keldmouth, a port at the mouth of the River Keld. Monster-hunting guilds protect caravans and settlements while gnome dowsers locate near-surface pockets of strange liquid-gold amber. New hires Rhea Marrow, Sylvi Ashdown, Brother Garran Holt, Veyra Kest, and Pip Fenlock arrive expecting paid caravan work and discover a frontier economy whose currency, monsters, and institutions are entangled.

## Current canonical state

At tick 210 the five hunters are moving south along an abandoned warden road after the Drowned Waymark split the immediate Root Host pursuit. Rhea carries a narrowly corroborating Blackroot counterfoil; Veyra's reliquary remains closed and quiet under charcoal; Pip still holds Moth's bone key with its hooked notch unused. The road is not confirmed to reach Harrowcross, and neither Moth's nor Jorund's fate is known.

## Author truth

Amber is not ordinary mineral. It is hardened memory-sap from the buried Heartwood Below, a vast ancient living network beneath the peninsula. Fresh blood, concentrated fear, and abrupt fracture can make raw amber 'quick,' releasing stored predator and victim impressions as ethereal animal-wretches that move through walls and ground. The Lantern Pike guild documented this at Blackroot and concealed the correlation while accepting amber for protection services. Former field-master Aldren Veyl rejected the concealment, survived catastrophic exposure, and became the Tree King. He correctly learned to call and strengthen manifestations with blooded amber, then mistook access to accumulated memory for divine kingship. Secret marker: FRONTIER-SECRET-AMBER-ROOT-MEMORY-7K4M.
