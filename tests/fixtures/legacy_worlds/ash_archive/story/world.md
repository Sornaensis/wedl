---
schema: wedl/v0.3
kind: world
id: world_0CQJZAMW59JQXVY75312HDXH8F
title: The Ash Archive
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
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# The Ash Archive

Cindervale’s Ash Archive preserves fires, inheritances, civic debts, and the evidence powerful people would rather route elsewhere. Chief Archivist Ilyra Sorn disappeared after discovering that records of illegal river evacuations had been removed through a concealed listening-and-flood network. Mara Vale, Nessa Quill, Notary Ysabet Crane, river custodians, and compromised watch officers reconstructed enough of the chain to expose the abuse without surrendering living descendants to the same centralized index.

## Completed canonical state

At tick 208 the investigation’s principal arc is complete. The descendant ledger has been dismantled into unindexed family packets. Proof, route, and identity are held by independent custodians under an expiring instrument. Caldrin Vey is suspended from Archive authority; Captain Rusk is suspended from Archive access and remains a compelled witness. Mara holds the restricted-vault key as acting chief archivist. The reopened Archive has begun a Ledger of Honest Absences: omissions must state their scope, witnesses, reason, expiration, and review date without retaining a central reconstruction key.

## Fixture purpose

The world now supports a coherent completed mystery across three acts: discovery and pursuit, evidentiary reconstruction, and institutional resolution. It remains useful for temporal state, perspective-safe retrieval, verbatim dialogue provenance, conflicting recollections, distributed custody, historical queries, and sequel threads involving Sable, Caldrin’s political future, and review of the identity packets.
