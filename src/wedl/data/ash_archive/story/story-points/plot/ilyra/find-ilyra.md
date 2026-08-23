---
schema: wedl/v0.3
kind: story-point
id: sp_0EWTQTRW748QTCS592XRSDVV80
title: Find Ilyra Sorn
domain: plot.ilyra
status: canonical
tags:
- ilyra
- active
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_000T8WQDPPRM6NBV93139J4ZAZ
    time:
      timeline: main
      tick: 101
      order: 40
    state: active
    causing_event: event_0Z53ASE2D15XFSS8GE47P9Q8BD
    note: The watch treats Ilyra as missing and seals the vault.
  - id: spt_6DEEFDQQXWNYEF5EE87CPT1TJJ
    time:
      timeline: main
      tick: 169
      order: 20
    state: resolved
    causing_event: event_76BBCME06QATMSNE0SVD7RS0P9
    note: Mara finds Ilyra alive beneath the vault.
activation_policy: manual
priority: 90
repeat_policy: once
dependencies:
  all: []
trigger:
  knowledge:
    knower: char_00HBQM4T4CMF11RKMNDBRP92QC
    claim_key: ilyra.status.missing
    state_in:
    - accepted
    - suspected
    minimum_confidence: 0.6
on_activate:
  create_draft_scene: false
outcome_events:
- event_76BBCME06QATMSNE0SVD7RS0P9
provenance: eyJ2IjoxLCJ0eCI6InR4XzBGVE5HOE5DN1RCNU5ZSjZDNkdLRUdXOUtRIiwiY29tbWFuZCI6ImV4YW1wbGUuc2VlZCIsImFjdG9yIjoiZXhhbXBsZSIsImdlbmVyYXRvciI6IndlZGwvMC4xLjAtZGV2IiwicGFyZW50IjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMCJ9
---

# Find Ilyra Sorn

The central investigation: determine whether Ilyra fled, was abducted, or is concealed within the Archive.

The second act advances this thread at tick 169.
