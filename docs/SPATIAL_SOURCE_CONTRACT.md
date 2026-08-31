# Latent spatial v0.7 source contract

Spatial source uses the single coordinated `wedl/v0.7` envelope.  Capabilities
are declared only by the world record in this exact order:
`generational-core-v1`, `spatial-core-v1`, `geometry-v1`, `route-v1`, and
`overlay-v1`; the latter three require `spatial-core-v1`.  A component never
infers an undeclared capability.

The declaration is never empty and always includes `spatial-core-v1`.

Every authored map, placement, anchor, portal, route, or overlay requires
`spatial-core-v1`; geometry and coordinate anchors additionally require
`geometry-v1`; portals and routes require `route-v1`; overlays require
`overlay-v1`.  Retained `loc_…` location IDs remain legal endpoints alongside
new `location:` IDs. The new colon IDs use lower-case opaque slug paths;
every segment is portable (including on Windows, where reserved device names
are forbidden) and maps to `story/maps/`, `locations/`, `anchors/`,
`portals/`, `routes/`, and `overlays/` without placing a colon in a filename.

The component vocabulary is opaque `map:`, `location:`, `anchor:`, `portal:`,
`route:`, and `overlay:` references.  Maps declare CRS, axis order, unit, and
finite bounds. Locations may remain coordinate-free. Geometry is authored
point/line/polygon data. Location containment uses canonical `parent_id`, and
directional prose links retain the ordinary `links` grammar; they are authored
facts, not route declarations. `parent` is accepted only as an explicit legacy
compatibility input and serializes as `parent_id`. Parents are structural and acyclic, and anchors or
portals do not create a general coordinate conversion.  Routes are directed
and typed.  Overlays are static or inclusive, same-timeline bounded intervals.
Ticks are signed integers, not duration or calendar conversion.

Only `local-planar:<name>` maps with `[east, north]` axes and an authored unit,
or `EPSG:4326` maps with `[longitude, latitude]` axes and `degree` units, are
canonical. Coordinates are finite, dimensionality-checked, and within their
authored bounds. Unknown operative fields are rejected; `x-*` fields are
preserved as inert extension data. Canonical v0.7 serialization places
provenance last and preserves the Markdown body byte-for-byte after UTF-8
decoding. Legacy schemas retain their established body normalization and
therefore their historical canonical bytes.

`z_policy: forbidden` requires two ordinates everywhere on that map;
`optional-level` permits either two or three, but the map bounds, origin, and
every geometry/position on that map must use the one authored dimensionality;
`required` requires three. Routes and portals have a non-empty, sorted,
duplicate-free `modes` list. Detailed location links keep the established
`description`, `label`, and `summary` rule: when present, each is non-empty
text.

The v0.7 world envelope is homogeneous and world-only: it has the exact
ordered capability list and closed `timelines` declarations of
`{id,label[,origin:{tick,label}]}`, a listed `default_timeline`, and closed
`chronology: {calendars,eras,anchors}` when chronology is declared. The
component runs the complete v0.6
closed chronology grammar, including calendar/era/anchor IDs, ordering,
bounds, and references. It inherits chronology semantics; it
does not reinterpret calendar labels as spatial time or route duration.

The local-only `upgrade-v07` migration activates generic source validation and
compiled-cache projection. It does not add a public spatial query, API, UI, or
ChangeSet mutation route.
