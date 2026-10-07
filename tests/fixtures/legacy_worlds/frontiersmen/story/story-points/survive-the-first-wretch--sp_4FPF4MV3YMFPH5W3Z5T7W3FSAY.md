---
schema: wedl/v0.3
kind: story-point
id: sp_4FPF4MV3YMFPH5W3Z5T7W3FSAY
title: Survive the First Wretch
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_5MNJ1WKQWATPSYYFHECSCQBHN1
    time:
      timeline: main
      tick: 63
      order: 20
    state: active
    causing_event: event_7SAZQYAV7G22NMSESPM4196DSQ
    note: A manifestation attacks the investigators.
  - id: spt_5DXYTS736DBEQX48NFP5MGR2EM
    time:
      timeline: main
      tick: 64
      order: 10
    state: resolved
    causing_event: event_65NWKCGHET513YTAQPKXVS8KQ5
    note: The party weakens and disperses the manifestation.
activation_policy: suggest
priority: 96
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_65NWKCGHET513YTAQPKXVS8KQ5
---

# Survive the First Wretch

Protect Harrowcross when an ethereal animal-wretch manifests through the barracks walls.
