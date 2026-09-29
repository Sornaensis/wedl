---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ilyra
id: sp_0EWTQTRW748QTCS592XRSDVV80
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_0Z53ASE2D15XFSS8GE47P9Q8BD
    id: spt_000T8WQDPPRM6NBV93139J4ZAZ
    note: The watch treats Ilyra as missing and seals the vault.
    state: active
    time:
      order: 40
      tick: 101
      timeline: main
  - causing_event: event_76BBCME06QATMSNE0SVD7RS0P9
    id: spt_6DEEFDQQXWNYEF5EE87CPT1TJJ
    note: Mara finds Ilyra alive beneath the vault.
    state: resolved
    time:
      order: 20
      tick: 169
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_76BBCME06QATMSNE0SVD7RS0P9
priority: 90
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- ilyra
- active
title: Find Ilyra Sorn
trigger:
  knowledge:
    claim_key: ilyra.status.missing
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    minimum_confidence: 0.6
    state_in:
    - accepted
    - suspected
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Find Ilyra Sorn

The central investigation: determine whether Ilyra fled, was abducted, or is concealed within the Archive.

The second act advances this thread at tick 169.
