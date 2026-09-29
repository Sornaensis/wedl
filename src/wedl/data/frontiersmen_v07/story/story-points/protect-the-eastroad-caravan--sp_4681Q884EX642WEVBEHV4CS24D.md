---
activation_policy: suggest
aliases: []
dependencies:
  all: []
domain: plot.frontiersmen
id: sp_4681Q884EX642WEVBEHV4CS24D
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_622182QP1ANWZNM341YAFYG08F
    id: spt_5SS4ARS01K44SAZK9GTR3SY3RW
    note: The company begins eastroad duty.
    state: active
    time:
      order: 10
      tick: 26
      timeline: main
  - causing_event: event_1175MZGP24P7AXEZE537FBRTTV
    id: spt_1BKRY89TZMC5ZP12YDVWWZ2ZN4
    note: Emergency reassignment ends the caravan contract early.
    state: resolved
    time:
      order: 25
      tick: 57
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_1175MZGP24P7AXEZE537FBRTTV
priority: 80
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- campaign
title: Protect the Eastroad Caravan
trigger:
  all: []
---

# Protect the Eastroad Caravan

Guard Maela Brigg's caravan from Keldmouth toward the eastern settlements.
