---
schema: wedl/v0.6
kind: world
id: world_0123456789ABCDEFGHJKMNPQRS
title: Five Hundred Years
domain: world
status: canonical
tags: [conformance, chronology]
aliases: []
default_timeline: main
timelines: [{id: main, label: Shared world chronology}]
threads: []
chronology:
  calendars:
    - id: calendar_0123456789abcdefghjkmnpqrs
      label: Solar
      x-conformance: convertible
      rule: {kind: cycle, period: 4, overrides: [{residue: 0, target_month: 2, delta_days: 1}]}
      months: [{number: 1, label: First, days: 31}, {number: 2, label: Second, days: 28}]
      epoch: {civil: {year: 0, month: 1, day: 1}, axis_day: 0}
    - id: calendar_1123456789abcdefghjkmnpqrs
      label: Regnal
      x-conformance: convertible-intercalary
      rule: {kind: table, years: [{year: 0, overrides: [{intercalary_month: {number: 13, label: Feast, days: 5}}]}]}
      months: [{number: 1, days: 30}]
      epoch: {civil: {year: 0, month: 1, day: 1}, axis_day: 0}
    - id: calendar_2123456789abcdefghjkmnpqrs
      label: Isolated
      x-conformance: intentionally-unconvertible
      rule: {kind: cycle, period: 1, overrides: []}
      months: [{number: 1, days: 30}]
      epoch: null
  eras:
    - id: era_0123456789abcdefghjkmnpqrs
      calendar_id: calendar_0123456789abcdefghjkmnpqrs
      label: Common
      aliases: [C]
      display_year_zero: true
      display_epoch: {display_year: 0, machine_year: 0}
      bounds: {lower: {year: -249, month: 1, day: 1}, upper: {year: 250, month: 2, day: 28}}
      provenance: [annals]
    - id: era_1123456789abcdefghjkmnpqrs
      calendar_id: calendar_1123456789abcdefghjkmnpqrs
      label: Feast Era
      aliases: [F]
      display_year_zero: true
      display_epoch: {display_year: 0, machine_year: 0}
      bounds: {lower: {year: 0, month: 1, day: 1}, upper: {year: 0, month: 13, day: 5}}
      provenance: [regnal-ledger]
  anchors:
    - {id: chronology_0123456789abcdefghjkmnpqrs, axis_day: 0, story_time: {timeline: main, tick: 0, order: 0}, provenance: [annals]}
    - {id: chronology_1123456789abcdefghjkmnpqrs, axis_day: 365, story_time: {timeline: main, tick: 1, order: 0}, provenance: [annals]}
x-conformance-years: [-249, 250]
---

# Five Hundred Years

This fixture covers a shared world across 500 inclusive calendar labels. Story
ticks order replay only; no tick gap states or implies elapsed calendar time.
