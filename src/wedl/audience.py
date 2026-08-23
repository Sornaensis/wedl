from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Any, Iterable

from .model import Record

HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
AUTHOR_RE = re.compile(r"^(author(?:'s)?(?:[- ]only)?\s+(?:notes?|constraints?|secrets?|truth)|gm\s+notes?|hidden(?:\s+(?:truth|notes?|secrets?))?|spoilers?|internal\s+notes?)$", re.I)


@dataclass(frozen=True, slots=True)
class Section:
    heading: str | None
    level: int
    parent: str | None
    text: str


def heading_key(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.casefold()).strip("-")


def sections(body: str) -> list[Section]:
    stack: list[tuple[int, str]] = []
    result: list[Section] = []
    current: list[str] = []
    heading: str | None = None
    level = 0
    parent: str | None = None

    def flush() -> None:
        nonlocal current
        text = "".join(current).strip()
        if text:
            result.append(Section(heading, level, parent, text))
        current = []

    for line in body.replace("\r\n", "\n").replace("\r", "\n").splitlines(keepends=True):
        match = HEADING_RE.match(line)
        if not match:
            current.append(line)
            continue
        flush()
        level = len(match.group(1))
        heading = match.group(2).strip()
        while stack and stack[-1][0] >= level:
            stack.pop()
        parent = stack[-1][1] if stack else None
        stack.append((level, heading))
        current.append(line)
    flush()
    return result


def _owner(record: Record) -> str | None:
    if record.kind == "character":
        return record.id
    if record.kind == "knowledge":
        return str(record.frontmatter.get("knower") or "") or None
    return None


def _participants(record: Record) -> set[str]:
    return {
        str(item.get("character"))
        for item in record.frontmatter.get("participants") or []
        if isinstance(item, dict) and item.get("character")
    }


def default_audience(record: Record, heading: str | None) -> tuple[str, ...]:
    key = heading_key(heading or "")
    if heading and AUTHOR_RE.fullmatch(heading):
        return ("author",)
    if record.kind == "character":
        if key in {"summary", "appearance", "voice", "manner", "mannerisms", "public-dossier"}:
            return ("public", "self", "author")
        return ("self", "author")
    if record.kind in {"object", "location", "environment"}:
        return ("public", "author")
    if record.kind == "knowledge":
        return ("self", "author")
    return ("author",)


def section_audience(record: Record, section: Section) -> tuple[str, ...]:
    mapping = record.frontmatter.get("section_audiences") or {}
    if isinstance(mapping, dict):
        for candidate in (section.heading, section.parent):
            if not candidate:
                continue
            value = mapping.get(candidate)
            if value is None:
                value = mapping.get(heading_key(candidate))
            if isinstance(value, str):
                return (value,)
            if isinstance(value, list) and all(isinstance(item, str) for item in value):
                return tuple(value)
    return default_audience(record, section.heading or section.parent)


def allows(record: Record, audience: Iterable[str], perspective: str, character_id: str | None) -> bool:
    values = set(audience)
    if perspective == "author":
        return True
    if perspective != "character" or not character_id:
        return False
    if "public" in values or character_id in values or f"char:{character_id}" in values:
        return True
    if "self" in values and _owner(record) == character_id:
        return True
    if "participants" in values and character_id in _participants(record):
        return True
    return False


def visible_sections(record: Record, perspective: str, character_id: str | None = None) -> list[Section]:
    return [
        section
        for section in sections(record.body)
        if allows(record, section_audience(record, section), perspective, character_id)
    ]


def visible_body(record: Record, perspective: str, character_id: str | None = None) -> str:
    return "\n\n".join(section.text for section in visible_sections(record, perspective, character_id)).strip()
