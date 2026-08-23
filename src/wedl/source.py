from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import re
from typing import Any

import yaml

from . import SOURCE_SCHEMA
from .errors import ParseError
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
    if not text.startswith("---\n"):
        raise ParseError(f"{path}: missing YAML frontmatter opener")
    end = text.find("\n---\n", 4)
    if end < 0:
        raise ParseError(f"{path}: missing YAML frontmatter terminator")
    yaml_text = text[4:end]
    body = text[end + 5 :].lstrip("\n")
    try:
        value = yaml.load(yaml_text, Loader=StrictLoader)
    except yaml.YAMLError as exc:
        raise ParseError(f"{path}: invalid YAML: {exc}") from exc
    if not isinstance(value, dict):
        raise ParseError(f"{path}: frontmatter must be a mapping")
    return value, body


def parse_record(data: bytes, path: str, *, blob_oid: str | None = None, revision: str | None = None) -> Record:
    frontmatter, body = split_envelope(data, path)
    return Record(frontmatter, body, path, data, blob_oid=blob_oid, revision=revision)


class Dumper(_DumperBase):
    def ignore_aliases(self, data: Any) -> bool:
        return True


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
        for child in value.values():
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
