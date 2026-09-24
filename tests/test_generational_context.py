"""Context never expands beyond authorized reads or the final JSON budget."""
from __future__ import annotations

from wedl.generational_context import build_generational_context
from wedl.generational_query import TrustedViewerScope
from wedl.model import StoryTime
from wedl.util import canonical_json


def _scope() -> TrustedViewerScope:
    return TrustedViewerScope("revision", "character", "main", StoryTime("main", 0, 0),
                              frozenset({"public"}), frozenset({"ordinary"}),
                              frozenset({"generational-core-v1"}), "character_1")


def test_context_uses_only_authorized_results(monkeypatch) -> None:
    from wedl import generational_context

    def query(_repository, _scope, request, **_kwargs):
        if request["operation"] == "parents":
            return {"state": "available", "relations": [{"targetId": "visible",
                "citations": [{"record_id": "edge", "path": "story/edge.md",
                               "applicability": {"applicability_kind": "static"}}]}]}
        return {"state": "unknown"}

    monkeypatch.setattr(generational_context, "query_generational", query)
    packet = build_generational_context(object(), _scope(), "subject", max_characters=1000)
    assert packet["state"] == "available"
    assert len(packet["items"]) == 1
    assert "visible" in canonical_json(packet)
    assert len(canonical_json(packet)) <= 1000


def test_final_serialized_size_and_truncation(monkeypatch) -> None:
    from wedl import generational_context

    def query(_repository, _scope, request, **_kwargs):
        return {"state": "available", "relations": [{"targetId": request["operation"] + "x" * 60}]}

    monkeypatch.setattr(generational_context, "query_generational", query)
    packet = build_generational_context(object(), _scope(), "subject", max_characters=180,
                                        max_items=4)
    assert packet["truncated"] is True
    assert len(packet["items"]) < 4
    assert len(canonical_json(packet)) <= 180


def test_closed_character_reads_make_no_context(monkeypatch) -> None:
    from wedl import generational_context

    monkeypatch.setattr(generational_context, "query_generational",
                        lambda *_args, **_kwargs: {"state": "unknown"})
    assert build_generational_context(object(), _scope(), "subject") == {
        "state": "unknown", "items": [], "truncated": False}


def test_subquery_traversal_limit_closes_entire_packet(monkeypatch) -> None:
    from wedl import generational_context

    def query(_repository, _scope, request, **_kwargs):
        if request["operation"] == "parents":
            return {"state": "available", "relations": [{"targetId": "visible"}]}
        return {"state": "limit", "code": "GEN-LIMIT-001"}

    monkeypatch.setattr(generational_context, "query_generational", query)
    assert build_generational_context(object(), _scope(), "subject") == {
        "state": "limit", "code": "GEN-LIMIT-001"}
