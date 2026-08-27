from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import re
from typing import Any

import yaml

from . import THREAD_SOURCE_SCHEMA, V04_RECOVERY_CONTRACT, V04_SOURCE_SCHEMA
from .errors import ParseError, SupersededSchemaError
from .model import Record
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
}

COMMON_ORDER = ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "section_audiences"]


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
    body = text[end + len(terminator):].lstrip("\r\n")
    try:
        value = yaml.load(yaml_text, Loader=StrictLoader)
    except yaml.YAMLError as exc:
        raise ParseError(f"{path}: invalid YAML: {exc}") from exc
    if not isinstance(value, dict):
        raise ParseError(f"{path}: frontmatter must be a mapping")
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


def quarantine_superseded_schema(frontmatter: dict[str, Any], path: str | None = None) -> None:
    if frontmatter.get("schema") != V04_SOURCE_SCHEMA:
        return
    location = f"{path}: " if path else ""
    raise SupersededSchemaError(
        f"{location}{V04_SOURCE_SCHEMA} is superseded; see {V04_RECOVERY_CONTRACT}",
        details={"schema": V04_SOURCE_SCHEMA, "recoveryContract": V04_RECOVERY_CONTRACT},
    )


def _ordered(frontmatter: dict[str, Any]) -> dict[str, Any]:
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


def serialize_record(frontmatter: dict[str, Any], body: str) -> bytes:
    frontmatter = _canonical_legacy_origins(frontmatter)
    quarantine_superseded_schema(frontmatter)
    frontmatter = _canonical_threads(frontmatter)
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
