---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_036KDER9FKFVCH734QS3QS34TC
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_2SZBF08CQFHZKYWN1ZN1MA98D5
    id: spt_5WJB6S1QY1Y855J9WXEZCQVT5T
    note: The party is condemned to the ring.
    state: active
    time:
      order: 30
      tick: 169
      timeline: main
  - causing_event: event_6V5B2VNCVV5V3V0BKZ2VAQVCKT
    id: spt_57ZGRP7PB3KGCBH360XTGGR7GR
    note: The party leaves the pens before the contest begins.
    state: resolved
    time:
      order: 20
      tick: 184
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_6V5B2VNCVV5V3V0BKZ2VAQVCKT
priority: 100
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Escape Before the Blood Sport
trigger:
  all: []
---

# Escape Before the Blood Sport

Leave the Root Host's pens before the party is used to strengthen masked champions.
