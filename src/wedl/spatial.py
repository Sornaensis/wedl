"""Immutable, authored-only accessors for latent v0.7 spatial components.

These are value readers, not a spatial engine: they never infer a reverse
route, containment, proximity, map conversion, duration, or StoryTime.
"""
from __future__ import annotations

from dataclasses import dataclass
import math
from typing import Any

from .ids import valid_id, valid_spatial_id
from .model import StoryTime


def _number(value: Any) -> float | int | None:
    return value if type(value) in (int, float) and math.isfinite(value) else None


def _point(value: Any) -> tuple[float | int, ...] | None:
    if not isinstance(value, list) or len(value) not in (2, 3):
        return None
    result = tuple(_number(item) for item in value)
    return result if all(item is not None for item in result) else None


def _policy_dimension(policy: Any, value: Any) -> int | None:
    """Return an authored ordinate count permitted by one map z policy."""
    point = _point(value)
    if point is None:
        return None
    if policy == "forbidden" and len(point) == 2:
        return 2
    if policy == "optional-level" and len(point) in (2, 3):
        return len(point)
    if policy == "required" and len(point) == 3:
        return 3
    return None


def _ordered_strings(value: Any, *, nonempty: bool = False) -> bool:
    return isinstance(value, list) and (bool(value) or not nonempty) and all(isinstance(item, str) and item for item in value) and value == sorted(set(value))


def _story_time(value: Any) -> StoryTime | None:
    if not isinstance(value, dict) or not {"timeline", "tick", "order"} <= set(value) or any(not isinstance(key, str) or (key not in {"timeline", "tick", "order"} and not key.startswith("x-")) for key in value):
        return None
    try:
        return StoryTime(value["timeline"], value["tick"], value["order"])
    except ValueError:
        return None


@dataclass(frozen=True, slots=True)
class SpatialReference:
    value: str

    @property
    def kind(self) -> str:
        return "location" if valid_id(self.value, "location") else self.value.partition(":")[0]

    @classmethod
    def from_value(cls, value: Any, expected_kind: str | None = None) -> "SpatialReference | None":
        if expected_kind == "location":
            valid = valid_id(value, "location") or valid_spatial_id(value, "location")
        else:
            valid = valid_spatial_id(value, expected_kind)
        return cls(value) if valid and isinstance(value, str) else None


@dataclass(frozen=True, slots=True)
class Metric:
    value: float | int
    unit: str

    @classmethod
    def from_value(cls, value: Any) -> "Metric | None":
        number = value.get("value") if isinstance(value, dict) else None
        unit = value.get("unit") if isinstance(value, dict) else None
        return cls(number, unit) if _number(number) is not None and number >= 0 and isinstance(unit, str) and unit.strip() else None


@dataclass(frozen=True, slots=True)
class MapPosition:
    map_id: SpatialReference
    coordinates: tuple[float | int, ...]

    @classmethod
    def from_value(cls, value: Any) -> "MapPosition | None":
        if not isinstance(value, dict):
            return None
        ident = SpatialReference.from_value(value.get("map_id"), "map")
        coords = _point(value.get("coordinates"))
        return cls(ident, coords) if ident is not None and coords is not None else None


@dataclass(frozen=True, slots=True)
class Geometry:
    kind: str
    coordinates: tuple[Any, ...]

    @classmethod
    def from_value(cls, value: Any) -> "Geometry | None":
        if not isinstance(value, dict) or value.get("kind") not in {"point", "line", "polygon"}:
            return None
        kind, raw = value["kind"], value.get("coordinates")
        if kind == "point":
            point = _point(raw)
            return cls(kind, point) if point is not None else None
        if not isinstance(raw, list): return None
        points = tuple(_point(item) for item in raw)
        if not all(item is not None for item in points):
            return None
        typed = tuple(points)
        if len(typed) > 10_000 or len(typed) < (2 if kind == "line" else 4):
            return None
        if len({len(point) for point in typed}) != 1:
            return None
        if kind == "polygon" and typed[0] != typed[-1]:
            return None
        return cls(kind, typed)


@dataclass(frozen=True, slots=True)
class LocationSpatial:
    map_id: SpatialReference
    geometry: Geometry

    @classmethod
    def from_value(cls, value: Any) -> "LocationSpatial | None":
        if not isinstance(value, dict): return None
        map_id, geometry = SpatialReference.from_value(value.get("map_id"), "map"), Geometry.from_value(value.get("geometry"))
        return cls(map_id, geometry) if map_id and geometry else None


@dataclass(frozen=True, slots=True)
class MapDefinition:
    id: SpatialReference
    crs: str
    axis_order: tuple[str, str]
    unit: str
    bounds: tuple[tuple[float | int, ...], tuple[float | int, ...]]
    origin: MapPosition | None = None
    scale: Metric | None = None
    z_policy: str = "forbidden"

    @classmethod
    def from_value(cls, value: Any) -> "MapDefinition | None":
        if not isinstance(value, dict) or value.get("kind") != "map" or not isinstance(value.get("crs"), str) or not isinstance(value.get("unit"), str): return None
        ident, axes, bounds = SpatialReference.from_value(value.get("id"), "map"), value.get("axis_order"), value.get("bounds")
        if not ident or not isinstance(axes, list) or len(axes) != 2 or not all(isinstance(x, str) for x in axes) or not isinstance(bounds, dict): return None
        policy = value.get("z_policy", "forbidden")
        minimum, maximum = _point(bounds.get("min")), _point(bounds.get("max"))
        if policy not in {"forbidden", "optional-level", "required"} or minimum is None or maximum is None or len(minimum) != len(maximum) or _policy_dimension(policy, list(minimum)) != len(minimum) or any(low >= high for low, high in zip(minimum, maximum)): return None
        origin = value.get("origin"); parsed_origin = None
        if origin is not None:
            if not isinstance(origin, dict) or not isinstance(origin.get("label"), str): return None
            coords = _point(origin.get("coordinates"))
            if coords is None or len(coords) != len(minimum): return None
            parsed_origin = MapPosition(ident, coords)
        scale = Metric.from_value(value["scale"]) if "scale" in value else None
        if "scale" in value and scale is None: return None
        return cls(ident, value["crs"], tuple(axes), value["unit"], (minimum, maximum), parsed_origin, scale, policy)


@dataclass(frozen=True, slots=True)
class Anchor:
    id: SpatialReference
    from_position: MapPosition
    to_position: MapPosition
    conversion: str | None = None

    @classmethod
    def from_value(cls, value: Any) -> "Anchor | None":
        if not isinstance(value, dict): return None
        ident, source, target = SpatialReference.from_value(value.get("id"), "anchor"), MapPosition.from_value(value.get("from")), MapPosition.from_value(value.get("to"))
        conversion = value.get("conversion")
        return cls(ident, source, target, conversion) if ident and source and target and (conversion is None or isinstance(conversion, str)) else None


@dataclass(frozen=True, slots=True)
class Portal:
    id: SpatialReference
    from_location_id: SpatialReference
    to: SpatialReference | MapPosition
    modes: tuple[str, ...]

    @classmethod
    def from_value(cls, value: Any) -> "Portal | None":
        if not isinstance(value, dict) or not _ordered_strings(value.get("modes"), nonempty=True): return None
        ident = SpatialReference.from_value(value.get("id"), "portal")
        source = SpatialReference.from_value(value.get("from_location_id"), "location")
        target = SpatialReference.from_value(value.get("to"), "location") or MapPosition.from_value(value.get("to"))
        return cls(ident, source, target, tuple(value["modes"])) if ident and source and target else None


@dataclass(frozen=True, slots=True)
class Route:
    id: SpatialReference
    from_location_id: SpatialReference
    to_location_id: SpatialReference
    direction: str
    modes: tuple[str, ...]
    route_distance: Metric | None = None
    travel_cost: Metric | None = None
    duration: Metric | None = None
    availability: str = "open"
    uncertainty: str = "exact"

    @classmethod
    def from_value(cls, value: Any) -> "Route | None":
        if not isinstance(value, dict) or value.get("direction") not in {"one-way", "two-way"} or not _ordered_strings(value.get("modes"), nonempty=True): return None
        ident, source, target = SpatialReference.from_value(value.get("id"), "route"), SpatialReference.from_value(value.get("from_location_id"), "location"), SpatialReference.from_value(value.get("to_location_id"), "location")
        metrics = [Metric.from_value(value[field]) if field in value else None for field in ("route_distance", "travel_cost", "duration")]
        if not ident or not source or not target or any(field in value and metric is None for field, metric in zip(("route_distance", "travel_cost", "duration"), metrics)): return None
        availability, uncertainty = value.get("availability", "open"), value.get("uncertainty", "exact")
        return cls(ident, source, target, value["direction"], tuple(value["modes"]), *metrics, availability, uncertainty) if availability in {"open", "closed", "restricted", "unknown"} and uncertainty in {"exact", "estimated", "unknown"} else None


@dataclass(frozen=True, slots=True)
class Overlay:
    id: SpatialReference
    lifecycle: str
    location_ids: tuple[SpatialReference, ...]
    audience: tuple[str, ...]
    perspectives: tuple[str, ...]
    valid: tuple[StoryTime, StoryTime] | None

    @classmethod
    def from_value(cls, value: Any) -> "Overlay | None":
        if not isinstance(value, dict) or value.get("lifecycle") not in {"static", "time-bounded"} or not isinstance(value.get("membership"), dict): return None
        raw, audience, perspectives = value["membership"].get("location_ids"), value.get("audience"), value.get("perspectives")
        if not _ordered_strings(raw) or not _ordered_strings(audience) or not _ordered_strings(perspectives): return None
        refs, ident = tuple(SpatialReference.from_value(item, "location") for item in raw), SpatialReference.from_value(value.get("id"), "overlay")
        if not ident or not all(refs): return None
        valid = value.get("valid")
        if value["lifecycle"] == "static":
            return cls(ident, value["lifecycle"], refs, tuple(audience), tuple(perspectives), None) if valid is None else None
        if valid is None or not isinstance(valid, dict) or not {"start", "end"} <= set(valid) or any(not isinstance(key, str) or (key not in {"start", "end"} and not key.startswith("x-")) for key in valid): return None
        interval = (_story_time(valid["start"]), _story_time(valid["end"]))
        if not all(interval): return None
        if interval[0].timeline != interval[1].timeline or (interval[0].tick, interval[0].order) > (interval[1].tick, interval[1].order): return None
        return cls(ident, value["lifecycle"], refs, tuple(audience), tuple(perspectives), interval)


def spatial_frontmatter(record: Any) -> LocationSpatial | None:
    """Return an immutable location spatial value, never raw mutable YAML."""
    return LocationSpatial.from_value(getattr(record, "frontmatter", {}).get("spatial"))
