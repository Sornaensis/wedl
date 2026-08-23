---
schema: wedl/v0.3
kind: world
id: world_01M0HH0YXDMTCEX34NPQ7S2AXQ
title: The Frontiersmen
domain: world
status: canonical
tags:
- example
- frontier-fantasy
- monster-hunters
- interactive
aliases: []
section_audiences:
  Public premise:
  - public
  - author
  Author truth:
  - author
  Current canonical state:
  - author
default_timeline: main
timelines:
- id: main
  label: Main chronology
state_keys:
  character:
    location:
      type: entity
      entity_kind: location
      exclusive: true
    condition:
      type: string
  object:
    holder:
      type: entity
      entity_kind: character
      exclusive_group: placement
    location:
      type: entity
      entity_kind: location
      exclusive_group: placement
    container:
      type: entity
      entity_kind: object
      exclusive_group: placement
    condition:
      type: string
relationship_metrics:
  trust:
    minimum: -1.0
    maximum: 1.0
    default: 0.0
  affinity:
    minimum: -1.0
    maximum: 1.0
    default: 0.0
  fear:
    minimum: 0.0
    maximum: 1.0
    default: 0.0
  obligation:
    minimum: 0.0
    maximum: 1.0
    default: 0.0
  respect:
    minimum: -1.0
    maximum: 1.0
    default: 0.0
  dependence:
    minimum: 0.0
    maximum: 1.0
    default: 0.0
embedding_policy:
  provider: lsa
  model: wedl-lsa-v1
  dimensions: 192
  max_features: 8192
context_policy:
  default_budget_characters: 8000
  default_max_items: 24
  conversation_turn_window: 6
compilation_policy:
  default_profile: hybrid
  retained_revisions: 5
  vector_cache: true
  source_parse_cache: true
  atomic_publish: true
  retrieval:
    fts_candidate_limit: 120
    vector_candidate_limit: 120
    hybrid_fts_weight: 1.0
    hybrid_vector_weight: 1.0
    hybrid_rrf_k: 60.0
---

# The Frontiersmen

## Public premise

The Frontier is a remote boreal peninsula reached through Keldmouth, a port at the mouth of the River Keld. Monster-hunting guilds protect caravans and settlements while gnome dowsers locate near-surface pockets of strange liquid-gold amber. New hires Rhea Marrow, Sylvi Ashdown, Brother Garran Holt, Veyra Kest, and Pip Fenlock arrive expecting paid caravan work and discover a frontier economy whose currency, monsters, and institutions are entangled.

## Current canonical state

At tick 210 the five hunters are moving south along an abandoned warden road after the Drowned Waymark split the immediate Root Host pursuit. Rhea carries a narrowly corroborating Blackroot counterfoil; Veyra's reliquary remains closed and quiet under charcoal; Pip still holds Moth's bone key with its hooked notch unused. The road is not confirmed to reach Harrowcross, and neither Moth's nor Jorund's fate is known.

## Author truth

Amber is not ordinary mineral. It is hardened memory-sap from the buried Heartwood Below, a vast ancient living network beneath the peninsula. Fresh blood, concentrated fear, and abrupt fracture can make raw amber 'quick,' releasing stored predator and victim impressions as ethereal animal-wretches that move through walls and ground. The Lantern Pike guild documented this at Blackroot and concealed the correlation while accepting amber for protection services. Former field-master Aldren Veyl rejected the concealment, survived catastrophic exposure, and became the Tree King. He correctly learned to call and strengthen manifestations with blooded amber, then mistook access to accumulated memory for divine kingship. Secret marker: FRONTIER-SECRET-AMBER-ROOT-MEMORY-7K4M.
