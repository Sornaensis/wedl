---
schema: wedl/v0.7
kind: world
id: world_0123456789ABCDEFGHJKMNPQRS
title: Five Hundred Years
capabilities:
- generational-core-v1
- spatial-core-v1
domain: world
status: canonical
tags:
- conformance
- chronology
aliases: []
default_timeline: main
timelines:
- id: main
  label: Shared world chronology
threads: []
chronology:
  anchors:
  - axis_day: 0
    id: chronology_0123456789abcdefghjkmnpqrs
    story_time:
      order: 0
      tick: 0
      timeline: main
    provenance:
    - annals
  - axis_day: 365
    id: chronology_1123456789abcdefghjkmnpqrs
    story_time:
      order: 0
      tick: 1
      timeline: main
    provenance:
    - annals
  calendars:
  - epoch:
      axis_day: 0
      civil:
        day: 1
        month: 1
        year: 0
    id: calendar_0123456789abcdefghjkmnpqrs
    label: Solar
    months:
    - days: 31
      label: First
      number: 1
    - days: 28
      label: Second
      number: 2
    rule:
      kind: cycle
      overrides:
      - delta_days: 1
        residue: 0
        target_month: 2
      period: 4
    x-conformance: convertible
  - epoch:
      axis_day: 0
      civil:
        day: 1
        month: 1
        year: 0
    id: calendar_1123456789abcdefghjkmnpqrs
    label: Regnal
    months:
    - days: 30
      number: 1
    rule:
      kind: table
      years:
      - overrides:
        - intercalary_month:
            days: 5
            label: Feast
            number: 13
        year: 0
    x-conformance: convertible-intercalary
  - epoch: null
    id: calendar_2123456789abcdefghjkmnpqrs
    label: Isolated
    months:
    - days: 30
      number: 1
    rule:
      kind: cycle
      overrides: []
      period: 1
    x-conformance: intentionally-unconvertible
  eras:
  - aliases:
    - C
    bounds:
      lower:
        day: 1
        month: 1
        year: -249
      upper:
        day: 28
        month: 2
        year: 250
    calendar_id: calendar_0123456789abcdefghjkmnpqrs
    display_epoch:
      display_year: 0
      machine_year: 0
    display_year_zero: true
    id: era_0123456789abcdefghjkmnpqrs
    label: Common
    provenance:
    - annals
  - aliases:
    - F
    bounds:
      lower:
        day: 1
        month: 1
        year: 0
      upper:
        day: 5
        month: 13
        year: 0
    calendar_id: calendar_1123456789abcdefghjkmnpqrs
    display_epoch:
      display_year: 0
      machine_year: 0
    display_year_zero: true
    id: era_1123456789abcdefghjkmnpqrs
    label: Feast Era
    provenance:
    - regnal-ledger
x-conformance-years:
- -249
- 250
---

# Five Hundred Years

This fixture covers a shared world across 500 inclusive calendar labels. Story
ticks order replay only; no tick gap states or implies elapsed calendar time.
