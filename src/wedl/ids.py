from __future__ import annotations

import hashlib
import os
import re
import time

CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
KIND_PREFIX = {
    "world": "world",
    "character": "char",
    "knowledge": "know",
    "event": "event",
    "object": "obj",
    "environment": "env",
    "location": "loc",
    "relationship": "rel",
    "story-point": "sp",
    "scene": "scene",
    "conversation": "conv",
    "hypothesis": "hyp",
}
AUX_PREFIX = {
    "thread": "thread",
    "effect": "effect",
    "knowledge-transition": "kt",
    "relationship-transition": "rt",
    "story-point-transition": "spt",
    "transaction": "tx",
    "conversation-turn": "turn",
    "conversation-recollection": "recol",
    "observation": "obs",
}
ID_RE = re.compile(r"^(?P<prefix>[a-z][a-z0-9-]*)_(?P<body>[0-9A-HJKMNP-TV-Z]{26})$")
# A component suffix is a portable relative path, not a filename.  Keeping
# this codec here makes the source writer and validator agree on the exact
# Windows-safe spelling rather than each doing a slightly different check.
SPATIAL_ID_RE = re.compile(r"^(?P<prefix>map|location|overlay|route|anchor|portal):(?P<path>[a-z0-9][a-z0-9-]{0,127}(?:/[a-z0-9][a-z0-9-]{0,127})*)$")
_WINDOWS_RESERVED = frozenset({"con", "prn", "aux", "nul", *(f"com{i}" for i in range(1, 10)), *(f"lpt{i}" for i in range(1, 10))})


def _encode(value: int) -> str:
    chars = ["0"] * 26
    for index in range(25, -1, -1):
        chars[index] = CROCKFORD[value & 31]
        value >>= 5
    return "".join(chars)


def new_id(kind_or_prefix: str) -> str:
    prefix = KIND_PREFIX.get(kind_or_prefix, AUX_PREFIX.get(kind_or_prefix, kind_or_prefix))
    stamp = int(time.time() * 1000) & ((1 << 48) - 1)
    random_bits = int.from_bytes(os.urandom(10), "big")
    return f"{prefix}_{_encode((stamp << 80) | random_bits)}"


def id_from_seed(kind_or_prefix: str, seed: str) -> str:
    prefix = KIND_PREFIX.get(kind_or_prefix, AUX_PREFIX.get(kind_or_prefix, kind_or_prefix))
    value = int.from_bytes(hashlib.sha256(f"{prefix}:{seed}".encode()).digest()[:16], "big")
    return f"{prefix}_{_encode(value)}"


def valid_id(value: object, expected_kind: str | None = None) -> bool:
    if not isinstance(value, str):
        return False
    match = ID_RE.fullmatch(value)
    if match is None:
        return False
    if expected_kind is None:
        return True
    expected = KIND_PREFIX.get(expected_kind, AUX_PREFIX.get(expected_kind, expected_kind))
    return match.group("prefix") == expected


def valid_spatial_id(value: object, expected_kind: str | None = None) -> bool:
    """Validate a v0.7 opaque spatial path without changing legacy ID rules."""
    if not isinstance(value, str):
        return False
    match = SPATIAL_ID_RE.fullmatch(value)
    if match is None or (expected_kind is not None and match.group("prefix") != expected_kind):
        return False
    # Windows reserves these names even with an extension.  Rejecting them at
    # the ID boundary keeps generated Markdown paths portable everywhere.
    return not any(segment.casefold() in _WINDOWS_RESERVED for segment in match.group("path").split("/"))


def spatial_id_path(value: object, expected_kind: str | None = None) -> str | None:
    """Return the portable path suffix for a validated colon ID."""
    if not valid_spatial_id(value, expected_kind):
        return None
    assert isinstance(value, str)
    return value.partition(":")[2]
