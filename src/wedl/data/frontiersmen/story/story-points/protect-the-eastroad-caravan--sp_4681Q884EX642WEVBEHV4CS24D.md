---
schema: wedl/v0.3
kind: story-point
id: sp_4681Q884EX642WEVBEHV4CS24D
title: Protect the Eastroad Caravan
domain: plot.frontiersmen
status: canonical
tags:
- campaign
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_5SS4ARS01K44SAZK9GTR3SY3RW
    time:
      timeline: main
      tick: 26
      order: 10
    state: active
    causing_event: event_622182QP1ANWZNM341YAFYG08F
    note: The company begins eastroad duty.
  - id: spt_1BKRY89TZMC5ZP12YDVWWZ2ZN4
    time:
      timeline: main
      tick: 57
      order: 25
    state: resolved
    causing_event: event_1175MZGP24P7AXEZE537FBRTTV
    note: Emergency reassignment ends the caravan contract early.
activation_policy: suggest
priority: 80
repeat_policy: once
dependencies:
  all: []
trigger:
  all: []
on_activate:
  create_draft_scene: false
outcome_events:
- event_1175MZGP24P7AXEZE537FBRTTV
---

# Protect the Eastroad Caravan

Guard Maela Brigg's caravan from Keldmouth toward the eastern settlements.
