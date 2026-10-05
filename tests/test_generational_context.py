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


def test_writing_context_uses_literal_learned_genealogy_and_history(tmp_path):
    from wedl.context import build_context
    from wedl.ids import id_from_seed
    from wedl.validation import validate_world
    from test_generational_knowledge_source import _world_with_knowledge, _record, _time
    from test_generational_knowledge_query import _literal
    from test_generational_knowledge_compiler import _repository

    world, mapping = _world_with_knowledge()
    world.revision = "e" * 40
    child, alpha = mapping["character_child"], mapping["character_alpha"]
    organization = next(record.id for record in world if record.kind == "organization")
    legacy = next(record.id for record in world if record.kind == "legacy")
    labels = {child: "Remembered self", alpha: "Remembered parent", organization: "Remembered house", legacy: "Remembered office"}
    scene = _record("scene", "Writing room", status="closed", time={"start": _time(-20), "current": _time(2), "end": _time(10)},
                    participants=[{"character": child}, {"character": alpha}])
    world.records[scene.id] = scene
    literals = {
        "parentage": {"child_id": child, "parent_id": alpha, "basis": "adoptive"},
        "union": {"participant_ids": sorted([child, alpha]), "state": "formed"},
        "organization": {"organization_id": organization, "parent_id": None},
        "affiliation": {"character_id": child, "organization_id": organization, "role": "Keeper"},
        "tenure": {"legacy_id": legacy, "holder_id": None, "basis": "legal"},
        "claim": {"legacy_id": legacy, "claimant_id": alpha, "state": "disputed"},
        "vital": {"character_id": alpha, "state": "dead"},
    }
    beliefs = []
    for kind, payload in literals.items():
        endpoints = {value for value in payload.values() if isinstance(value, str)} | set(payload.get("participant_ids", []))
        belief = _literal(world, mapping, "AUTHOR correction " + kind, kind, payload, labels={key: value for key, value in labels.items() if key in endpoints},
                          until=_time(-1) if kind == "parentage" else None)
        belief.frontmatter.update(aliases=["AUTHOR secret alias"], tags=["AUTHOR hidden tag"])
        belief.frontmatter["transitions"][0]["note"] = "AUTHOR truth correction"
        belief.frontmatter["transitions"].append({"id": id_from_seed("knowledge-transition", "reject " + kind),
                                                "time": _time(3), "state": "rejected"})
        beliefs.append(belief)
    assert validate_world(world) == []
    repository = _repository(world, tmp_path)
    def packet(character, tick, order=0):
        return build_context(repository, character_id=character, scene_id=scene.id, revision=world.revision,
                             tick=tick, order=order, max_characters=20000, search_mode="fts")
    before = packet(child, 2, -1)["promptText"]
    assert "Remembered parent" not in before
    after = packet(child, 2)["promptText"]
    for text in ["suspects", "adoptive parent", "union of", "parent organization", "affiliated with", "legal holder none / vacancy", "disputed claim", "Remembered parent is dead", "historical knowledge, not a current edge"]:
        assert text in after, (text, after)
    for belief in beliefs:
        assert belief.id in after and belief.frontmatter["transitions"][0]["id"] in after
    assert "time:main 2:0" in after
    assert "AUTHOR" not in after and "story/" not in after
    assert "An explicitly authored genealogy assertion" not in after
    assert "Remembered parent" not in packet(child, 3)["promptText"]
    assert "Remembered parent" not in packet(alpha, 2)["promptText"]
    assert all(belief.frontmatter["claim"]["genealogy"]["payload"] == literals[belief.frontmatter["claim"]["genealogy"]["kind"]] for belief in beliefs)
    from dataclasses import replace
    conflicting = replace(world, records={key: value for key, value in world.records.items()
                                          if value.kind != "knowledge" or value is beliefs[0]}, _cache={})
    other = _literal(conflicting, mapping, "AUTHOR contradictory parent", "parentage",
                     {"child_id": child, "parent_id": alpha, "basis": "biological"},
                     labels={child: labels[child], alpha: labels[alpha]}, until=_time(-1))
    conflict_packet = build_context(_repository(conflicting, tmp_path / "conflicts"), character_id=child,
                                   scene_id=scene.id, revision=world.revision, tick=2, order=0,
                                   max_characters=20000, search_mode="fts")["promptText"]
    assert "adoptive parent" in conflict_packet and "biological parent" in conflict_packet
    assert beliefs[0].id in conflict_packet and other.id in conflict_packet
    # Valid long literals must never lose a participant or their expired
    # applicability qualification through the per-atom text limit.
    long_world = replace(world, records={key: value for key, value in world.records.items()
                                        if value.kind != "knowledge"}, _cache={})
    participants = sorted(record.id for record in long_world if record.kind == "character")
    long_labels = {identifier: f"Long learned participant {index} " + "x" * 170 for index, identifier in enumerate(participants)}
    long_union = _literal(long_world, mapping, "AUTHOR long expired union", "union",
                          {"participant_ids": participants, "state": "formed"}, labels=long_labels, until=_time(-1))
    long_role = "Long complete role " + "r" * 175
    long_affiliation = _literal(long_world, mapping, "AUTHOR long expired affiliation", "affiliation",
                                {"character_id": child, "organization_id": organization, "role": long_role},
                                labels={child: long_labels[child], organization: "Long learned organization " + "o" * 170}, until=_time(-1))
    retained = _literal(long_world, mapping, "AUTHOR short expired report", "vital",
                        {"character_id": child, "state": "living"}, labels={child: "Learned self"}, until=_time(-1))
    assert validate_world(long_world) == []
    bounded = build_context(_repository(long_world, tmp_path / "long-literals"), character_id=child,
                            scene_id=scene.id, revision=world.revision, tick=2, order=0,
                            max_characters=20000, search_mode="fts")
    assert "historical knowledge, not a current edge" in bounded["promptText"]
    assert "Learned self is living" in bounded["promptText"] and retained.id in bounded["promptText"]
    assert "Long learned" not in bounded["promptText"] and "Long complete role" not in bounded["promptText"]
    assert long_union.id not in bounded["promptText"] and long_affiliation.id not in bounded["promptText"]
    assert bounded["selection"]["omitted"] >= 2
