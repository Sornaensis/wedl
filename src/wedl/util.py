from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import tempfile
from pathlib import Path
from typing import Any, Iterator

TOKEN_RE = re.compile(r"[\w'-]+", re.UNICODE)
ENTITY_ID_RE = re.compile(r"\b(?:world|char|know|event|obj|env|loc|rel|sp|scene|conv)_[0-9A-HJKMNP-TV-Z]{26}\b")
STORY_LINK_RE = re.compile(r"\]\(story:((?:world|char|know|event|obj|env|loc|rel|sp|scene|conv)_[0-9A-HJKMNP-TV-Z]{26})\)")


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def pretty_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def slugify(value: str, default: str = "record") -> str:
    value = re.sub(r"[^a-z0-9]+", "-", value.casefold()).strip("-")
    return value or default


def tokens(text: str) -> list[str]:
    return [match.group(0).casefold() for match in TOKEN_RE.finditer(text)]


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary_name, path)
    finally:
        try:
            os.unlink(temporary_name)
        except FileNotFoundError:
            pass


def deep_walk(value: Any) -> Iterator[Any]:
    yield value
    if isinstance(value, dict):
        for child in value.values():
            yield from deep_walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from deep_walk(child)


def deep_replace(value: Any, replacements: dict[str, str]) -> Any:
    if isinstance(value, str):
        return replacements.get(value, value)
    if isinstance(value, list):
        return [deep_replace(item, replacements) for item in value]
    if isinstance(value, dict):
        return {key: deep_replace(item, replacements) for key, item in value.items()}
    return value


def b64url_json(value: Any) -> str:
    raw = canonical_json(value).encode()
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")
