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
