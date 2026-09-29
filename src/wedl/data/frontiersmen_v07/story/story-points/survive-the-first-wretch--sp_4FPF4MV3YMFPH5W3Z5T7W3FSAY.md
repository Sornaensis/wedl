---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_4FPF4MV3YMFPH5W3Z5T7W3FSAY
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_7SAZQYAV7G22NMSESPM4196DSQ
    id: spt_5MNJ1WKQWATPSYYFHECSCQBHN1
    note: A manifestation attacks the investigators.
    state: active
    time:
      order: 20
      tick: 63
      timeline: main
  - causing_event: event_65NWKCGHET513YTAQPKXVS8KQ5
    id: spt_5DXYTS736DBEQX48NFP5MGR2EM
    note: The party weakens and disperses the manifestation.
    state: resolved
    time:
      order: 10
      tick: 64
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_65NWKCGHET513YTAQPKXVS8KQ5
priority: 96
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Survive the First Wretch
trigger:
  all: []
---

# Survive the First Wretch

Protect Harrowcross when an ethereal animal-wretch manifests through the barracks walls.
