---
schema: wedl/v0.3
kind: story-point
id: sp_7HSK6TK6C1QBG4GMM7ZY3KYFWP
title: Open the Letter Under Witness
domain: plot.ash-archive.second-act
status: canonical
tags:
- second-act
aliases: []
lifecycle:
  initial_state: dormant
  transitions:
  - id: spt_7RYS7NQP99YPSD9FFBA4TB666Y
    time:
      timeline: main
      tick: 145
      order: 10
    state: active
    causing_event: event_58QVWKM8WN61NYC9GDQ28WZEYE
    note: The group reaches a defensible place to open the letter.
  - id: spt_3ZHDR8Q3F7YTMD3TA6NZ9FJDZ5
    time:
      timeline: main
      tick: 148
      order: 0
    state: resolved
    causing_event: event_651JGHAPM9JY5GWASRC097DFZ3
    note: Mara opens and reads the letter.
activation_policy: manual
priority: 88
repeat_policy: once
dependencies:
  all: []
trigger:
  event:
    event: event_58QVWKM8WN61NYC9GDQ28WZEYE
on_activate:
  create_draft_scene: false
outcome_events:
- event_651JGHAPM9JY5GWASRC097DFZ3
---

# Open the Letter Under Witness

Break the heron seal with an independent witness and preserve its contents as provenance.
