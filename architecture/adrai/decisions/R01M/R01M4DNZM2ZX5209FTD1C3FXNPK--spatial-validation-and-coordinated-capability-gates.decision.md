+++
schema = "adrai/decision/v1"
adr = "A01M48ZT8WHG5XAVFBVXSZDGGMK"
record = "R01M4DNZM2ZX5209FTD1C3FXNPK"
title = "Spatial validation and coordinated capability gates"
summary = "Migrated SPATIAL_VALIDATION.md; preserved exact spatial semantics and current delivery boundaries."
domains = ["spatial"]
+++

## Authority and provenance

Migrated from `docs/SPATIAL_VALIDATION.md` at Git revision `aac36745258e6f93487481564dc1c40daeb327ef`. Existing approval, aliases, historical editions and semantic authority remain intact; this records current implementation boundaries without inventing ratification or redesigning the product.

# Spatial validation diagnostics

The component emits deterministic field-addressed errors only:
`GEN-CAPABILITY-001`, `SPATIAL-REQUEST-001`, and the `-001` diagnostics for
ID, reference, map, geometry, metric, hierarchy, anchor, portal, route,
overlay, time, and limit.  Author extensions (`x-*`) are inert metadata; an
unknown operative field or unapproved faction/institution/calendar boundary is
`SPATIAL-REQUEST-001` at that field's exact leaf. Map-unit failures use the
`unit` leaf (rather than CRS or axis order).

Validation never adds reverse routes, proximity adjacency, containment,
coordinate conversion, visibility, metric, duration, or calendar semantics.
Legacy v0.3/v0.5/v0.6 validation is untouched.

Diagnostics sort by source path, entity, field, and code. Invalid YAML
collection members—including booleans, non-finite numbers, and malformed
lists—are ordinary diagnostics rather than validator exceptions. Repeated
record IDs, self-portals, duplicate directed route edges, invalid bounds or
geometry, hierarchy cycles, and cross-timeline overlays are rejected.

Coordinated capability declarations must be non-empty and include at least one of `generational-core-v1` or `spatial-core-v1`. Coordinate-free generational-only records are valid; every operative spatial feature remains gated by `spatial-core-v1`. The six-token registry order ends with optional `generational-knowledge-v1`, requiring `generational-core-v1`; geometry/route/overlay tokens require spatial core.
Map z policy is checked consistently across bounds, origin, geometry, and
map-position endpoints: forbidden maps are 2D, required maps are 3D, and
optional-level maps select one consistent 2D or 3D shape. Portal and route
mode lists are non-empty, sorted, and duplicate-free. Anchor `conversion`
values are text when authored. A time-bounded overlay requires a valid
inclusive same-timeline interval; a static overlay has no `valid` interval.
Detailed location-link prose uses the established legacy validation and is
reported at its exact `links[n].description`, `.label`, or `.summary` leaf.

## Current delivery and authority boundary

Homogeneous `wedl/v0.7` is accepted by the generic source loader, validator, and compiler. The current SQLite schema is `wedl-sqlite/v15` and the spatial compiler fingerprint is `wedl-spatial-index/v5`. Markdown/Git remains canonical and all spatial SQLite structures are disposable projections.

The eight-operation `wedl-spatial/v1` codec is implemented in `src/wedl/spatial_api.py`: containment, children, bbox, nearby, adjacency, reachability, path, and overlay-as-of. The separate `wedl-spatial-explorer/v1` adapter implements catalog, places, viewport, layers, and routes. Browser explorer/editor and ID-only `spatial.<kind>.create|update` authoring intents are also present. Those intents compile to ordinary confirmed changesets and require v0.7, exact expected HEAD, idempotency identity, closed payloads, and the established confirmation flow. They edit authored source; they do not authorize inferred topology, geometry, metrics, horizons, or identity.

An explorer author lens selects audience/perspective for presentation. It does not confer trusted character identity or confidentiality. Exact authored visibility and horizon filters continue to apply before counts and paging. The original spatial ADR A01M48NPY19KENPF8EWB8A4W95W retains its 2026-08-30 approval-stage wording and vectors; this migration does not silently rewrite that approval as a later deployment claim.

Executable component fixture: [spatial-source-component-v07.yaml](../../../../tests/fixtures/architecture/spatial-source-component-v07.yaml). Its semantic cases and original stage comments are preserved unchanged; those comments do not state current generic runtime acceptance.

<!-- @adrai:eyJhIjp7ImkiOiJhcmNoaXZlLWV4YW1wbGVzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiOTM4Yzg3NDk1ODYzOWZkMDBjZWNkMTI2ZWFmMTU4NzY2MjAyNTA4OSIsImkiOiJzaGEyNTY6RmE3WmtvMHZQQzRUYnM0bW9HWnB1VUdieTFkQlIwWmpWZkJ4STRvMW5fZyIsImsiOiJkZWNpc2lvbi5hbWVuZCIsIm8iOiJSMDFNNEROWk0yWlg1MjA5RlREMUMzRlhOUEsiLCJvcCI6Ik8wMU00RE5aTTJaWDUyMDlGVEQxQzNGWE5QSyIsInAiOlsiUjAxTTQ4WlQ5U0hTSEdCQzZHRlRYRlZCUVBIIl0sInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OmlTRXUtQmVvcEoyaFZmQ2daOVNqX0tDdzJrRkJCS0Q4UlFCRXlPamt1bjgiLCJ0IjoxNzkxNDYwNjI2NTI3LCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
