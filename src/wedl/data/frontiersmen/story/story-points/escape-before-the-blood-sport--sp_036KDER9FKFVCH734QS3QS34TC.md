---
schema: wedl/v0.3
kind: story-point
id: sp_036KDER9FKFVCH734QS3QS34TC
title: Escape Before the Blood Sport
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_5WJB6S1QY1Y855J9WXEZCQVT5T
    time:
      timeline: main
      tick: 169
      order: 30
    state: active
    causing_event: event_2SZBF08CQFHZKYWN1ZN1MA98D5
    note: The party is condemned to the ring.
  - id: spt_57ZGRP7PB3KGCBH360XTGGR7GR
    time:
      timeline: main
      tick: 184
      order: 20
    state: resolved
    causing_event: event_6V5B2VNCVV5V3V0BKZ2VAQVCKT
    note: The party leaves the pens before the contest begins.
activation_policy: suggest
priority: 100
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_6V5B2VNCVV5V3V0BKZ2VAQVCKT
---

# Escape Before the Blood Sport

Leave the Root Host's pens before the party is used to strengthen masked champions.
