from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import re
from typing import Any

import yaml

from . import THREAD_SOURCE_SCHEMA, V04_RECOVERY_CONTRACT, V04_SOURCE_SCHEMA
from .errors import ParseError, SupersededSchemaError
from .model import Record
from .ids import spatial_id_path, valid_id, valid_spatial_id
from .util import ENTITY_ID_RE, STORY_LINK_RE, sha256_bytes, slugify


_LoaderBase = getattr(yaml, "CSafeLoader", yaml.SafeLoader)
_DumperBase = getattr(yaml, "CSafeDumper", yaml.SafeDumper)


class StrictLoader(_LoaderBase):
    """LibYAML-backed strict loader when the C extension is available."""

    pass


def _construct_mapping(loader: StrictLoader, node: yaml.Node, deep: bool = False) -> dict[Any, Any]:
    mapping: dict[Any, Any] = {}
    for key_node, value_node in node.value:  # type: ignore[attr-defined]
        key = loader.construct_object(key_node, deep=deep)
        try:
            hash(key)
        except TypeError as exc:
            raise yaml.constructor.ConstructorError(
                "while constructing a mapping",
                node.start_mark,
                "mapping keys must be hashable scalar values",
                key_node.start_mark,
            ) from exc
        if key in mapping:
            raise yaml.constructor.ConstructorError(
                "while constructing a mapping",
                node.start_mark,
                f"duplicate YAML key {key!r}",
                key_node.start_mark,
            )
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


StrictLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_mapping)

KIND_DIR = {
    "world": "",
    "character": "characters",
    "knowledge": "knowledge",
    "event": "events/main",
    "object": "objects",
    "environment": "environments",
    "location": "locations",
    "relationship": "relationships",
    "story-point": "story-points",
    "scene": "scenes",
    "conversation": "conversations",
    "hypothesis": "hypotheses",
    "organization": "organizations",
    "parentage": "kinships",
    "union": "unions",
    "affiliation": "affiliations",
    "legacy": "legacies",
    "tenure": "tenures",
    "claim": "claims",
    "vital-history": "vitals",
    # Latent v0.7 component paths. Generic readers still reject v0.7; these
    # mappings only make component-authoring serialization portable.
    "map": "maps",
    "anchor": "anchors",
    "portal": "portals",
    "route": "routes",
    "overlay": "overlays",
}

COMMON_ORDER = ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "section_audiences"]
SPATIAL_ORDER = {
    "world": ["schema", "kind", "id", "title", "capabilities", "domain", "status", "tags", "aliases", "default_timeline", "timelines", "threads", "section_audiences"],
    "map": ["schema", "kind", "id", "title", "crs", "axis_order", "unit", "bounds", "origin", "scale", "z_policy", "domain", "status", "tags", "aliases", "threads", "section_audiences"],
    "location": ["schema", "kind", "id", "title", "parent_id", "links", "spatial", "domain", "status", "tags", "aliases", "threads", "section_audiences"],
    "anchor": ["schema", "kind", "id", "title", "from", "to", "conversion", "domain", "status", "tags", "aliases", "threads"],
    "portal": ["schema", "kind", "id", "title", "from_location_id", "to", "modes", "domain", "status", "tags", "aliases", "threads"],
    "route": ["schema", "kind", "id", "title", "from_location_id", "to_location_id", "direction", "modes", "route_distance", "travel_cost", "duration", "availability", "uncertainty", "domain", "status", "tags", "aliases", "threads"],
    "overlay": ["schema", "kind", "id", "title", "lifecycle", "membership", "audience", "perspectives", "valid", "domain", "status", "tags", "aliases", "threads"],
}
SPATIAL_NESTED_ORDER = {
    "bounds": ["min", "max"], "origin": ["label", "coordinates"], "spatial": ["map_id", "geometry"],
    "geometry": ["kind", "coordinates"], "from": ["map_id", "coordinates"], "to": ["map_id", "coordinates"],
    "membership": ["location_ids"], "valid": ["start", "end"], "start": ["timeline", "tick", "order"], "end": ["timeline", "tick", "order"],
    "route_distance": ["value", "unit"], "travel_cost": ["value", "unit"], "duration": ["value", "unit"], "scale": ["value", "unit"],
}
GENERATIONAL_NESTED_ORDER = {"initialization": ["transition_id", "transition_kind", "applicability", "payload"], "transitions": ["transition_id", "transition_kind", "applicability", "payload", "cause_event_id", "replaces_transition_id"], "applicability": ["applicability_kind", "point", "first", "last"], "point": ["timeline", "tick", "order"], "first": ["timeline", "tick", "order"], "last": ["timeline", "tick", "order"]}
GENERATIONAL_PAYLOAD_ORDER = {
    "organization-initialize": ["title", "aliases"], "organization-rename": ["title", "aliases"], "organization-reparent": ["parent_id"],
    "parentage-initialize": ["basis"], "parentage-confirm": ["basis"], "union-initialize": ["participant_ids"], "union-form": ["participant_ids"], "union-reconcile": ["participant_ids"],
    "affiliation-initialize": ["role"], "affiliation-role": ["role"], "tenure-initialize": ["holder_id", "basis"], "tenure-designate": ["holder_id", "basis"], "tenure-hold": ["holder_id", "basis"], "tenure-vacate": ["holder_id"], "tenure-transfer": ["from_tenure_id", "to_tenure_id"],
    "claim-initialize": ["competes_with"], "claim-dispute": ["competes_with"],
}
GENERATIONAL_ORDER = {
    "organization": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "organization_kind", "parent_id", "location_id", "initialization", "transitions"],
    "parentage": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "child_id", "parent_id", "initialization", "transitions"],
    "union": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "participant_ids", "initialization", "transitions"],
    "affiliation": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "character_id", "organization_id", "initialization", "transitions"],
    "legacy": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "legacy_kind", "organization_id", "initialization", "transitions"],
    "tenure": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "legacy_id", "predecessor_tenure_id", "successor_tenure_id", "initialization", "transitions"],
    "claim": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "legacy_id", "claimant_id", "initialization", "transitions"],
    "vital-history": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "character_id", "disclosure", "initialization", "transitions"],
}


def split_envelope(data: bytes, path: str) -> tuple[dict[str, Any], str]:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ParseError(f"{path}: source must be UTF-8") from exc
    if text.startswith("---\r\n"):
        newline = "\r\n"
    elif text.startswith("---\n"):
        newline = "\n"
    else:
        raise ParseError(f"{path}: missing YAML frontmatter opener")
    opener = f"---{newline}"
    terminator = f"{newline}---{newline}"
    end = text.find(terminator, len(opener))
    if end < 0:
        raise ParseError(f"{path}: missing YAML frontmatter terminator")
    yaml_text = text[len(opener):end]
    try:
        value = yaml.load(yaml_text, Loader=StrictLoader)
    except yaml.YAMLError as exc:
        raise ParseError(f"{path}: invalid YAML: {exc}") from exc
    if not isinstance(value, dict):
        raise ParseError(f"{path}: frontmatter must be a mapping")
    raw_body = text[end + len(terminator):]
    # This deliberately preserves the historical parser/serializer contract
    # for every legacy schema.  Opaque body preservation is a v0.7 component
    # property only; broadening it changed legacy corpus object IDs.
    if value.get("schema") == "wedl/v0.7":
        if raw_body.startswith("\r\n"):
            raw_body = raw_body[2:]
        elif raw_body.startswith("\n"):
            raw_body = raw_body[1:]
        body = raw_body
    else:
        body = raw_body.lstrip("\r\n")
    return value, body


def parse_record(data: bytes, path: str, *, blob_oid: str | None = None, revision: str | None = None) -> Record:
    frontmatter, body = split_envelope(data, path)
    quarantine_superseded_schema(frontmatter, path)
    return Record(frontmatter, body, path, data, blob_oid=blob_oid, revision=revision)


class Dumper(_DumperBase):
    def ignore_aliases(self, data: Any) -> bool:
        return True


def _member_sort_key(value: Any) -> tuple[str, str, str]:
    """Total deterministic order for malformed YAML member names too."""
    return (type(value).__module__, type(value).__qualname__, repr(value))


def _canonical_legacy_origins(frontmatter: dict[str, Any]) -> dict[str, Any]:
    """Canonicalize only descriptive origin maps without changing their meaning."""
    result = dict(frontmatter)
    timelines = result.get("timelines")
    if not isinstance(timelines, list):
        return result
    normalized: list[Any] = []
    for timeline in timelines:
        if not isinstance(timeline, dict) or not isinstance(timeline.get("origin"), dict):
            normalized.append(timeline)
            continue
        origin = timeline["origin"]
        ordered_origin: dict[Any, Any] = {}
        for key in ("tick", "label"):
            if key in origin:
                ordered_origin[key] = origin[key]
        for key in sorted((key for key in origin if key not in ordered_origin), key=_member_sort_key):
            ordered_origin[key] = origin[key]
        normalized.append({**timeline, "origin": ordered_origin})
    result["timelines"] = normalized
    return result


def _canonical_thread_declaration(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    result: dict[Any, Any] = {}
    for key in ("id", "label"):
        if key in value:
            result[key] = value[key]
    for key in sorted((key for key in value if key not in result), key=_member_sort_key):
        result[key] = value[key]
    return result


def _canonical_threads(frontmatter: dict[str, Any]) -> dict[str, Any]:
    if frontmatter.get("schema") != THREAD_SOURCE_SCHEMA:
        return frontmatter
    values = frontmatter.get("threads")
    if not isinstance(values, list):
        return frontmatter
    result = dict(frontmatter)
    if frontmatter.get("kind") == "world":
        declarations = [_canonical_thread_declaration(value) for value in values]
        result["threads"] = sorted(
            declarations,
            key=lambda value: _member_sort_key(value.get("id")) if isinstance(value, dict) else _member_sort_key(value),
        )
    else:
        result["threads"] = sorted(values, key=_member_sort_key)
    return result


def _canonical_spatial_parent(frontmatter: dict[str, Any]) -> dict[str, Any]:
    """Emit the ADR 0004 ``parent_id`` spelling for a v0.7 location."""
    if frontmatter.get("schema") != "wedl/v0.7" or frontmatter.get("kind") != "location" or "parent" not in frontmatter:
        return frontmatter
    result = dict(frontmatter)
    legacy_parent = result.pop("parent")
    # Never resolve contradictory containment input by silently choosing one.
    if "parent_id" in result:
        raise ParseError("v0.7 location cannot contain both parent and parent_id")
    result["parent_id"] = legacy_parent
    return result


def quarantine_superseded_schema(frontmatter: dict[str, Any], path: str | None = None) -> None:
    if frontmatter.get("schema") != V04_SOURCE_SCHEMA:
        return
    location = f"{path}: " if path else ""
    raise SupersededSchemaError(
        f"{location}{V04_SOURCE_SCHEMA} is superseded; see {V04_RECOVERY_CONTRACT}",
        details={"schema": V04_SOURCE_SCHEMA, "recoveryContract": V04_RECOVERY_CONTRACT},
    )


def _ordered(frontmatter: dict[str, Any]) -> dict[str, Any]:
    if frontmatter.get("schema") == "wedl/v0.7":
        return _ordered_spatial(frontmatter)
    result: dict[str, Any] = {}
    for key in COMMON_ORDER:
        if key in frontmatter:
            result[key] = frontmatter[key]
    for key, value in frontmatter.items():
        if key not in result and key != "provenance":
            result[key] = value
    if "provenance" in frontmatter:
        result["provenance"] = frontmatter["provenance"]
    return result


def _ordered_spatial(value: Any, context: str | None = None) -> Any:
    """Canonical component ordering without changing opaque x-* extension data."""
    if isinstance(value, list):
        # A transition array's members use the transition order, not the
        # alphabetic fallback used for arbitrary list-member mappings.
        return [_ordered_spatial(item, context) for item in value]
    if not isinstance(value, dict):
        return value
    order = (GENERATIONAL_ORDER.get(str(value.get("kind"))) or SPATIAL_ORDER.get(str(value.get("kind")))) if context is None else (GENERATIONAL_PAYLOAD_ORDER.get(context.removeprefix("payload:")) if context.startswith("payload:") else (GENERATIONAL_NESTED_ORDER.get(context) or SPATIAL_NESTED_ORDER.get(context)))
    result: dict[Any, Any] = {}
    for key in order or []:
        if key in value:
            child_context = f"payload:{value.get('transition_kind')}" if context in {"initialization", "transitions"} and key == "payload" else key
            result[key] = _ordered_spatial(value[key], child_context)
    for key in sorted((key for key in value if key not in result and key != "provenance"), key=_member_sort_key):
        child_context = f"payload:{value.get('transition_kind')}" if context in {"initialization", "transitions"} and key == "payload" else str(key)
        result[key] = _ordered_spatial(value[key], child_context)
    if "provenance" in value:
        result["provenance"] = _ordered_spatial(value["provenance"], "provenance")
    return result


def serialize_record(frontmatter: dict[str, Any], body: str) -> bytes:
    frontmatter = _canonical_legacy_origins(frontmatter)
    quarantine_superseded_schema(frontmatter)
    frontmatter = _canonical_threads(frontmatter)
    frontmatter = _canonical_spatial_parent(frontmatter)
    if "importance" in frontmatter:
        raise ParseError("importance is calculated output and cannot be serialized into canonical source")
    payload = yaml.dump(
        _ordered(frontmatter),
        Dumper=Dumper,
        sort_keys=False,
        allow_unicode=True,
        width=1000,
        default_flow_style=False,
    ).rstrip()
    if frontmatter.get("schema") == "wedl/v0.7":
        # Component bodies are opaque authored bytes once decoded as UTF-8.
        return f"---\n{payload}\n---\n\n{body}".encode("utf-8")
    # Keep the exact legacy canonical serializer behavior.  Existing source
    # object IDs and corpus tests depend on this byte-level normalization.
    return f"---\n{payload}\n---\n\n{body.strip()}\n".encode("utf-8")


def generated_path(source_root: str, kind: str, title: str, entity_id: str, frontmatter: dict[str, Any]) -> str:
    if kind == "world":
        return f"{source_root}/world.md"
    base = KIND_DIR.get(kind)
    if base is None:
        raise ParseError(f"unsupported entity kind {kind}")
    if kind == "knowledge":
        knower = str(frontmatter.get("knower") or "unknown")
        base = f"knowledge/{slugify(knower)}"
    if kind == "event":
        time_value = frontmatter.get("time")
        timeline = str(time_value.get("timeline", "main")) if isinstance(time_value, dict) else "main"
        base = f"events/{slugify(timeline)}"
    if frontmatter.get("schema") == "wedl/v0.7" and kind in GENERATIONAL_ORDER:
        if not valid_id(entity_id, kind):
            raise ParseError(f"invalid generational entity ID {entity_id}")
        return f"{source_root}/{base}/{entity_id}.md"
    if frontmatter.get("schema") == "wedl/v0.7" and kind in {"map", "location", "anchor", "portal", "route", "overlay"}:
        # v0.7 retains existing ``loc_`` identifiers verbatim; only new
        # colon IDs use the portable path codec.
        if kind == "location" and valid_id(entity_id, "location"):
            return f"{source_root}/{base}/{slugify(title)}--{entity_id}.md"
        if not valid_spatial_id(entity_id, kind):
            raise ParseError(f"invalid spatial entity ID {entity_id}")
        suffix = spatial_id_path(entity_id, kind)
        if suffix is None:
            raise ParseError(f"invalid spatial entity ID {entity_id}")
        return f"{source_root}/{base}/{suffix}.md"
    filename = f"{slugify(title)}--{entity_id}.md"
    return f"{source_root}/{base}/{filename}"


def extract_entity_refs(value: Any) -> set[str]:
    result: set[str] = set()
    if isinstance(value, str):
        if ENTITY_ID_RE.fullmatch(value):
            result.add(value)
    elif isinstance(value, dict):
        for key, child in value.items():
            if key == "threads":
                continue
            result.update(extract_entity_refs(child))
    elif isinstance(value, list):
        for child in value:
            result.update(extract_entity_refs(child))
    return result


def markdown_entity_links(body: str) -> set[str]:
    return set(STORY_LINK_RE.findall(body))


def record_hash(record: Record) -> str:
    return sha256_bytes(record.raw_bytes)


def clone_record(record: Record) -> Record:
    frontmatter = deepcopy(record.frontmatter)
    body = record.body
    raw = serialize_record(frontmatter, body)
    return Record(frontmatter, body, record.source_path, raw, record.blob_oid, record.revision)
