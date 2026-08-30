"""Shared latent v0.7 component-envelope primitives.

This is intentionally not a generic source-schema registration.  Callers opt
in through a component validator until the coordinated migration task owns
runtime acceptance.
"""
from __future__ import annotations

from typing import Any


SOURCE_SCHEMA = "wedl/v0.7"
CAPABILITY_ORDER = ("generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1")
CAPABILITY_REQUIRES = {"geometry-v1": "spatial-core-v1", "route-v1": "spatial-core-v1", "overlay-v1": "spatial-core-v1"}


def canonical_capabilities(value: Any) -> tuple[str, ...] | None:
    """Return the declared closed capability tuple, never inferred features."""
    if not isinstance(value, list) or not value or not all(isinstance(item, str) and item in CAPABILITY_ORDER for item in value):
        return None
    if len(set(value)) != len(value) or list(value) != sorted(value, key=CAPABILITY_ORDER.index):
        return None
    # A v0.7 world activates at least one coordinated domain.  Individual
    # component validators gate records and fields belonging to the domains
    # that are not declared by the envelope.
    if not ({"generational-core-v1", "spatial-core-v1"} & set(value)):
        return None
    if any(required not in value for item, required in CAPABILITY_REQUIRES.items() if item in value):
        return None
    return tuple(value)
