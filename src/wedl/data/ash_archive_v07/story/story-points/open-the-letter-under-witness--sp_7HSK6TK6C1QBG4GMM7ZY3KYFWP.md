---
activation_policy: manual
aliases: []
dependencies:
  all: []
domain: plot.ash-archive.second-act
id: sp_7HSK6TK6C1QBG4GMM7ZY3KYFWP
kind: story-point
lifecycle:
  initial_state: dormant
  transitions:
  - causing_event: event_58QVWKM8WN61NYC9GDQ28WZEYE
    id: spt_7RYS7NQP99YPSD9FFBA4TB666Y
    note: The group reaches a defensible place to open the letter.
    state: active
    time:
      order: 10
      tick: 145
      timeline: main
  - causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    id: spt_3ZHDR8Q3F7YTMD3TA6NZ9FJDZ5
    note: Mara opens and reads the letter.
    state: resolved
    time:
      order: 0
      tick: 148
      timeline: main
on_activate:
  create_draft_scene: false
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
priority: 88
repeat_policy: once
schema: wedl/v0.7
status: canonical
tags:
- second-act
title: Open the Letter Under Witness
trigger:
  event:
    event: event_58QVWKM8WN61NYC9GDQ28WZEYE
---

# Open the Letter Under Witness

Break the heron seal with an independent witness and preserve its contents as provenance.
