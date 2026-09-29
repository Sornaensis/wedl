---
schema: wedl/v0.7
kind: world
id: world_0CQJZAMW59JQXVY75312HDXH8F
title: The Ash Archive
capabilities:
- generational-core-v1
- spatial-core-v1
domain: world
status: canonical
tags:
- example
- mystery
- interactive
aliases: []
default_timeline: main
timelines:
- id: main
  label: Main chronology
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
  fear:
    default: 0.0
    maximum: 1.0
    minimum: 0.0
  obligation:
    default: 0.0
    maximum: 1.0
    minimum: 0.0
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
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# The Ash Archive

Cindervale’s Ash Archive preserves fires, inheritances, civic debts, and the evidence powerful people would rather route elsewhere. Chief Archivist Ilyra Sorn disappeared after discovering that records of illegal river evacuations had been removed through a concealed listening-and-flood network. Mara Vale, Nessa Quill, Notary Ysabet Crane, river custodians, and compromised watch officers reconstructed enough of the chain to expose the abuse without surrendering living descendants to the same centralized index.

## Completed canonical state

At tick 208 the investigation’s principal arc is complete. The descendant ledger has been dismantled into unindexed family packets. Proof, route, and identity are held by independent custodians under an expiring instrument. Caldrin Vey is suspended from Archive authority; Captain Rusk is suspended from Archive access and remains a compelled witness. Mara holds the restricted-vault key as acting chief archivist. The reopened Archive has begun a Ledger of Honest Absences: omissions must state their scope, witnesses, reason, expiration, and review date without retaining a central reconstruction key.

## Fixture purpose

The world now supports a coherent completed mystery across three acts: discovery and pursuit, evidentiary reconstruction, and institutional resolution. It remains useful for temporal state, perspective-safe retrieval, verbatim dialogue provenance, conflicting recollections, distributed custody, historical queries, and sequel threads involving Sable, Caldrin’s political future, and review of the identity packets.
