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

Capability declarations must be non-empty and include `spatial-core-v1`.
Map z policy is checked consistently across bounds, origin, geometry, and
map-position endpoints: forbidden maps are 2D, required maps are 3D, and
optional-level maps select one consistent 2D or 3D shape. Portal and route
mode lists are non-empty, sorted, and duplicate-free. Anchor `conversion`
values are text when authored. A time-bounded overlay requires a valid
inclusive same-timeline interval; a static overlay has no `valid` interval.
Detailed location-link prose uses the established legacy validation and is
reported at its exact `links[n].description`, `.label`, or `.summary` leaf.
