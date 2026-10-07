---
schema: wedl/v0.3
kind: environment
id: env_6NF7SVJQ74JF3TFKGNTWH6Y3NM
title: Flood-Stair Surge
domain: environment.second-act
status: canonical
tags:
- second-act
aliases: []
time:
  start:
    timeline: main
    tick: 143
    order: 0
  end:
    timeline: main
    tick: 150
    order: 0
targets:
- loc_40CWSF1X5KAZV4RE9DCTQPT559
- loc_1SZ56FSY6MFJAJBQ7F1VCTZAFZ
conditions:
  water_level: rising
  safe_cycles: 3
sensory:
- Water strikes the lower stair in timed pulses.
- The pressure wheel groans before each surge.
---

# Flood-Stair Surge

The stair is passable only between pressure cycles.
