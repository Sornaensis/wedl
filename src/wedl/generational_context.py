"""Compact generational context assembled only from authorized query results."""
from __future__ import annotations

from typing import Any

from .generational_query import TrustedViewerScope, query_generational
from .generational_knowledge import CAPABILITY as KNOWLEDGE_CAPABILITY
from .repository import Repository
from .util import canonical_json


def build_generational_context(
    repository: Repository, scope: TrustedViewerScope, subject_id: str, *,
    max_characters: int = 4096, max_items: int = 20, max_depth: int = 4,
    require_compiled: bool = False,
) -> dict[str, Any]:
    """Return deterministic cited relations within the final serialized budget."""
    if (type(max_characters) is not int or type(max_items) is not int
            or type(max_depth) is not int or max_characters < 80
            or not 1 <= max_items <= 100 or not 0 <= max_depth <= 16
            or not isinstance(subject_id, str) or not subject_id):
        return {"state": "invalid", "code": "GEN-REQUEST-001"}
    requests = (
        ("parents", {"operation": "parents", "subject_id": subject_id, "items": max_items}),
        ("ancestors", {"operation": "ancestors", "subject_id": subject_id,
                       "depth": max_depth, "items": max_items}),
        ("descendants", {"operation": "descendants", "subject_id": subject_id,
                         "depth": max_depth, "items": max_items}),
        ("vital", {"operation": "vital", "subject_id": subject_id, "items": max_items}),
    )
    if scope.mode == "character" and KNOWLEDGE_CAPABILITY in scope.capabilities:
        requests += (("learned-history", {"operation": "knowledge-history", "subject_id": subject_id,
                                         "items": max_items}),)
    results = []
    for name, request in requests:
        value = query_generational(repository, scope, request, require_compiled=require_compiled)
        if value["state"] in {"invalid", "unavailable", "limit"}:
            return value
        if value["state"] == "available":
            results.append({"kind": name, "result": value})
    packet: dict[str, Any] = {"state": "available" if results else "unknown",
                              "items": [], "truncated": False}
    for value in results[:max_items]:
        candidate = {**packet, "items": [*packet["items"], value],
                     "truncated": len(results) > len(packet["items"]) + 1}
        if len(canonical_json(candidate)) > max_characters:
            packet["truncated"] = True
            break
        packet = candidate
    if len(packet["items"]) < len(results):
        packet["truncated"] = True
    if len(canonical_json(packet)) > max_characters:
        return {"state": "limit", "code": "GEN-LIMIT-001"}
    return packet
