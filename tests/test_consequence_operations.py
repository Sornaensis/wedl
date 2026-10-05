from __future__ import annotations

from copy import deepcopy
from pathlib import Path

import pytest

from wedl import changeset
from wedl.errors import ConfirmationRequired, ConflictError, StaleRevision, UsageError, ValidationFailed
from wedl.ids import id_from_seed
from wedl.model import Record, StoryTime, World
from wedl.repository import Repository
from wedl.semantics import resolve_state
from wedl.validation import validate_world


def _world():
    return Repository(Path("src/wedl/data/ash_archive")).load_world("WORKTREE", cache_write=False)


def _time(tick=300):
    return {"timeline": "main", "tick": str(tick), "order": "0"}


def _request(world, operations):
    return {"protocol": "wedl-changeset/v1", "expectedHead": world.revision,
            "idempotencyKey": "typed-consequence-check", "summary": "Explicit rescue", "operations": operations}


def _event(world, temporary="tmp:rescue", tick=300):
    return {"type": "event.create", "temporaryId": temporary, "title": "Explicit rescue",
            "time": {"timeline": "main", "tick": tick, "order": 0},
            "effects": [{"target": world.find("Brass Restricted-Vault Key", "object").id,
                         "key": "condition", "operation": "set", "value": "closed"},
                        {"id": "tmp:opened", "target": world.find("Brass Restricted-Vault Key", "object").id,
                         "key": "condition", "operation": "set", "value": "open"}]}


def _apply(world, operations):
    request = _request(world, operations)
    allocated = changeset._allocate(request)
    records, touched = changeset._apply(request, world, allocated)
    return World(world.revision, world.tree_oid, records, world.root, world.source_root), allocated, touched, request


def _knowledge(world, temporary="tmp:belief"):
    return {"type": "knowledge.create", "temporaryId": temporary,
            "value": {"frontmatter": {"schema": world.schema, "kind": "knowledge", "title": "New rescue belief",
                                      "domain": "story.knowledge", "status": "canonical", "tags": [], "aliases": [],
                                      "knower": world.find("Mara Vale", "character").id,
                                      "claim": {"key": "rescue", "statement": "tmp:rescue"}, "transitions": [],
                                      "x-literal": {"entity": "tmp:rescue", "time": "tmp:opened"}},
                      "bodyMarkdown": "The literal words tmp:rescue stay in this body.\n"}}


def test_compound_operations_preserve_history_and_same_event_order():
    world = _world()
    old_relationship = world.find("Mara’s view of Oren", "relationship")
    old_knowledge = world.by_kind("knowledge")[0]
    old_plot = world.find("Open the Restricted Vault", "story-point")
    before = deepcopy({key: record.frontmatter for key, record in world.records.items()})
    operations = [_event(world),
                  {"type": "knowledge.transition.append", "knowledge": old_knowledge.id,
                   "transition": {"time": _time(), "state": "forgotten", "causing_event": "tmp:rescue"}},
                  {"type": "relationship.transition.append", "relationship": old_relationship.id,
                   "transition": {"time": _time(), "metrics": {"trust": 0.8}, "causing_event": "tmp:rescue"}},
                  {"type": "story-point.transition.append", "storyPoint": old_plot.id,
                   "transition": {"time": _time(301), "state": "resolved", "causing_event": "tmp:rescue"}},
                  {"type": "outcome.link", "event": "tmp:rescue", "storyPoints": [old_plot.id], "scenes": []}]
    candidate, allocated, touched, request = _apply(world, operations)
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    assert {key: record.frontmatter for key, record in world.records.items()} == before
    for record in (old_knowledge, old_relationship):
        assert candidate.get(record.id).frontmatter["transitions"][:-1] == before[record.id]["transitions"]
        assert candidate.get(record.id).body == record.body
    assert candidate.get(old_plot.id).frontmatter["lifecycle"]["transitions"][:-1] == before[old_plot.id]["lifecycle"]["transitions"]
    effects = candidate.get(allocated["tmp:rescue"]).frontmatter["effects"]
    assert [effect["value"] for effect in effects] == ["closed", "open"]
    assert effects[0]["id"] == id_from_seed("effect", f"{changeset._request_hash(request)}:0:effects:0")
    assert resolve_state(candidate, effects[0]["target"], StoryTime("main", 300, 0))[0]["condition"] == "open"
    assert set(touched) == {allocated["tmp:rescue"], old_plot.id, old_relationship.id, old_knowledge.id}


def test_closed_inputs_auxiliary_identity_and_literal_extensions():
    world = _world()
    create = _knowledge(world)
    operations = [_event(world), create,
                  {"type": "knowledge.transition.append", "knowledge": "tmp:belief",
                   "transition": {"id": "tmp:learned", "time": _time(), "state": "accepted",
                                  "causing_event": "tmp:rescue", "note": "tmp:rescue"}}]
    candidate, allocated, _, request = _apply(world, operations)
    assert changeset._allocate(request) == allocated
    belief = candidate.get(allocated["tmp:belief"])
    assert belief.frontmatter["claim"]["statement"] == "tmp:rescue"
    assert belief.frontmatter["x-literal"] == create["value"]["frontmatter"]["x-literal"]
    assert belief.body == create["value"]["bodyMarkdown"]
    assert belief.frontmatter["transitions"][0]["note"] == "tmp:rescue"
    assert allocated["tmp:learned"] == id_from_seed("knowledge-transition", f"{changeset._request_hash(request)}:2:transitions:0")
    supplied = deepcopy(create)
    supplied["value"]["frontmatter"]["id"] = id_from_seed("knowledge", "supplied")
    _, supplied_ids, _, _ = _apply(world, [_event(world), supplied])
    assert supplied_ids["tmp:belief"] == supplied["value"]["frontmatter"]["id"]
    literal_effect = _event(world)
    literal_effect["effects"][0]["value"] = {"entity": "tmp:opened"}
    candidate, identifiers, _, _ = _apply(world, [literal_effect])
    assert candidate.get(identifiers["tmp:rescue"]).frontmatter["effects"][0]["value"] == {"entity": "tmp:opened"}
    wrong_entity = _event(world)
    wrong_entity["effects"][0].update(key="holder", value={"entity": "tmp:belief"})
    with pytest.raises(UsageError):
        _apply(world, [wrong_entity, create])
    for mutation in (lambda ops: ops[2]["transition"].update(extra=True),
                     lambda ops: ops[2]["transition"].update(id=None),
                     lambda ops: ops[2]["transition"].update(state=[]),
                     lambda ops: ops[2]["transition"]["time"].update(tick=300),
                     lambda ops: ops[2]["transition"]["time"].update(tick="9" * 5000),
                     lambda ops: ops[0]["effects"][0].update(operation=[]),
                     lambda ops: ops[2]["transition"].update(id="tmp:opened"),
                     lambda ops: ops[2]["transition"].update(causing_event="tmp:learned")):
        altered = deepcopy(operations)
        mutation(altered)
        with pytest.raises(UsageError):
            _apply(world, altered)


def test_rejects_causes_order_kinds_and_final_conflicts():
    world = _world()
    knowledge = world.by_kind("knowledge")[0]
    transition = {"type": "knowledge.transition.append", "knowledge": knowledge.id,
                  "transition": {"time": _time(), "state": "accepted", "causing_event": "tmp:rescue"}}
    for mutate in (lambda op: op["transition"].pop("causing_event"),
                   lambda op: op.update(knowledge=world.by_kind("relationship")[0].id),
                   lambda op: op["transition"].update(time=_time(-1)),
                   lambda op: op["transition"].update(id=knowledge.frontmatter["transitions"][0]["id"])):
        altered = deepcopy(transition)
        mutate(altered)
        with pytest.raises(UsageError):
            _apply(world, [_event(world), altered])
    with pytest.raises(UsageError):
        _apply(world, [transition | {"knowledge": "tmp:belief"}, _knowledge(world), _event(world)])
    for identifier in ("this-record-does-not-exist", id_from_seed("character", "absent"),
                       *[id_from_seed(kind, "auxiliary") for kind in ("effect", "knowledge-transition", "relationship-transition",
                           "story-point-transition", "observation", "conversation-turn", "conversation-recollection",
                           "generational-transition", "thread", "transaction")]):
        altered = deepcopy(transition)
        altered["transition"]["source_entity"] = identifier
        with pytest.raises(UsageError):
            _apply(world, [_event(world), altered])
    forward = deepcopy(transition)
    forward["transition"]["source_entity"] = "tmp:source"
    auxiliary_forward = deepcopy(transition)
    auxiliary_forward["transition"]["source_entity"] = "tmp:opened"
    with pytest.raises(UsageError):
        _apply(world, [auxiliary_forward, _event(world)])
    candidate, allocated, _, _ = _apply(world, [_event(world), forward, _knowledge(world, "tmp:source")])
    assert candidate.get(knowledge.id).frontmatter["transitions"][-1]["source_entity"] == allocated["tmp:source"]
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    with pytest.raises(UsageError):
        _apply(world, [_event(world), forward, _knowledge(world, "tmp:source"),
                       {"type": "entity.delete", "entity": "tmp:source"}])
    # A declared entity ID cannot be admitted as a malformed auxiliary record.
    wrong = id_from_seed("knowledge", "auxiliary-record")
    world.records[wrong] = Record({"id": wrong, "kind": "observation"}, "", "unused.md", b"")
    altered = deepcopy(transition)
    altered["transition"]["source_entity"] = wrong
    with pytest.raises(UsageError):
        _apply(world, [_event(world), altered])
    world.records.pop(wrong)
    create = _knowledge(world)
    create["value"]["frontmatter"]["knower"] = id_from_seed("character", "missing-knower")
    with pytest.raises(UsageError):
        _apply(world, [create])
    create["value"]["frontmatter"]["knower"] = "tmp:source"
    with pytest.raises(UsageError):
        _apply(world, [create, _knowledge(world, "tmp:source")])
    event = _event(world)
    event["effects"][0].update(key="holder", value={"entity": id_from_seed("character", "missing-holder")})
    with pytest.raises(UsageError):
        _apply(world, [event])
    candidate, _, _, _ = _apply(world, [_event(world, tick=301), transition])
    assert any(item["code"] == "WDL-CAUSE-004" for item in validate_world(candidate))
    candidate, _, _, _ = _apply(world, [_event(world) | {"status": "draft"}, transition])
    assert any(item["code"] == "WDL-CAUSE-002" for item in validate_world(candidate))
    conflicting = _event(world, "tmp:other")
    conflicting["effects"] = conflicting["effects"][:1]
    candidate, _, _, _ = _apply(world, [_event(world), conflicting])
    assert any(item["code"] == "WDL-STATE-020" for item in validate_world(candidate))


def test_outcome_links_are_explicit_ordered_and_idempotent():
    world = _world()
    plot = world.find("Open the Restricted Vault", "story-point")
    event = _event(world)
    link = {"type": "outcome.link", "event": "tmp:rescue", "storyPoints": [plot.id], "scenes": []}
    candidate, allocated, _, _ = _apply(world, [link, event, link])
    event_id = allocated["tmp:rescue"]
    assert candidate.get(plot.id).frontmatter["outcome_events"] == world.get(plot.id).frontmatter["outcome_events"] + [event_id]
    assert candidate.get(event_id).frontmatter["related_story_points"] == [plot.id]
    assert len(candidate.records) == len(world.records) + 1
    with pytest.raises(UsageError):
        _apply(world, [event, link | {"storyPoints": [plot.id, plot.id]}])
    with pytest.raises(UsageError):
        _apply(world, [event, link | {"storyPoints": [], "scenes": []}])
    old_event = _event(world, "tmp:older", 299)
    old_link = link | {"event": "tmp:older"}
    with pytest.raises(UsageError):
        _apply(world, [event, old_event, link, old_link])
    scene = world.find("Under the Vault", "scene")
    scene_event = _event(world, tick=174) | {"effects": []}
    scene_link = link | {"storyPoints": [], "scenes": [scene.id]}
    candidate, allocated, _, _ = _apply(world, [scene_event, scene_link, scene_link])
    assert candidate.get(scene.id).frontmatter["outcome_events"][-1] == allocated["tmp:rescue"]
    assert candidate.get(scene.id).frontmatter["outcome_events"].count(allocated["tmp:rescue"]) == 1
    assert candidate.get(allocated["tmp:rescue"]).frontmatter["related_story_points"] == []
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    candidate, _, _, _ = _apply(world, [scene_event | {"time": {"timeline": "main", "tick": 175, "order": 0}}, scene_link])
    assert any(item["severity"] == "error" and item["field"].startswith("outcome_events") for item in validate_world(candidate))


def test_typed_creates_reuse_genealogy_and_homogeneous_source_validation():
    world = _world()
    create = _knowledge(world)
    create["value"]["frontmatter"]["claim"]["genealogy"] = {
        "kind": "parentage", "payload": {"child_id": world.find("Mara Vale", "character").id,
                                           "parent_id": world.find("Oren Thane", "character").id, "basis": "biological"},
        "valid": {"from": {"timeline": "main", "tick": 0, "order": 0}}}
    candidate, _, _, _ = _apply(world, [_event(world), create])
    assert any(item["code"] == "GEN-KNOWLEDGE-001" for item in validate_world(candidate))
    create = _knowledge(world)
    create["value"]["frontmatter"]["schema"] = "wedl/v0.7"
    with pytest.raises(UsageError):
        _apply(world, [create])
    create["value"]["frontmatter"]["schema"] = world.schema
    create["value"]["frontmatter"]["id"] = world.by_kind("knowledge")[0].id
    with pytest.raises(UsageError):
        _apply(world, [create])
    assert changeset.CHANGESET_OPERATION_TYPES.count("expectation.check") == 1
    assert "expectation.unknown" not in changeset.CHANGESET_OPERATION_TYPES
    with pytest.raises(changeset.ProtocolError, match="unsupported operation 'expectation.unknown'"):
        _apply(world, [{"type": "expectation.unknown"}])
    existing = world.by_kind("relationship")[0]
    frontmatter = deepcopy(existing.frontmatter)
    frontmatter.pop("id")
    frontmatter.pop("inverse", None)
    frontmatter["transitions"] = [{"time": {"timeline": "main", "tick": 0, "order": 0}, "metrics": {"trust": 0.1}}]
    relationship = {"type": "relationship.create", "temporaryId": "tmp:relationship",
                    "value": {"frontmatter": frontmatter, "bodyMarkdown": "Authored relationship body.\n"}}
    candidate, allocated, _, request = _apply(world, [_event(world), relationship,
        {"type": "relationship.transition.append", "relationship": "tmp:relationship",
         "transition": {"time": _time(), "causing_event": "tmp:rescue", "facets": ["rescuer"]}}])
    created = candidate.get(allocated["tmp:relationship"])
    assert created.body == relationship["value"]["bodyMarkdown"]
    assert created.frontmatter["transitions"][0]["id"] == id_from_seed("relationship-transition", f"{changeset._request_hash(request)}:1:transitions:0")
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    relationship["value"]["frontmatter"]["transitions"][0]["metrics"]["trust"] = 2
    with pytest.raises(UsageError):
        _apply(world, [relationship])
    # Old generic event inputs retain their shape, even in an additive batch.
    legacy = {"type": "event.create", "temporaryId": "tmp:legacy-event", "title": "Legacy",
              "time": {"tick": 300}, "effects": [], "bodyMarkdown": "tmp:legacy-event"}
    candidate, allocated, _, _ = _apply(world, [legacy, _knowledge(world)])
    assert candidate.get(allocated["tmp:legacy-event"]).body == allocated["tmp:legacy-event"]
    assert candidate.get(allocated["tmp:legacy-event"]).frontmatter["time"] == {"tick": 300}
    candidate, allocated, _, _ = _apply(world, [legacy])
    assert candidate.get(allocated["tmp:legacy-event"]).frontmatter["time"] == {"tick": 300}
    auxiliary = deepcopy(legacy)
    auxiliary["effects"] = [{"id": "tmp:raw-effect", "target": world.by_kind("object")[0].id,
        "key": "condition", "operation": "set", "value": "open"}]
    candidate, allocated, _, _ = _apply(world, [auxiliary])
    assert candidate.get(allocated["tmp:legacy-event"]).frontmatter["time"] == {"tick": 300}
    assert candidate.get(allocated["tmp:legacy-event"]).frontmatter["effects"][0]["id"] == allocated["tmp:raw-effect"]
    modern = Repository(Path("src/wedl/data/ash_archive_v07")).load_world("WORKTREE", cache_write=False)
    candidate, allocated, _, _ = _apply(modern, [_event(modern)])
    assert candidate.get(allocated["tmp:rescue"]).frontmatter["schema"] == modern.schema
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    evidence = _knowledge(modern, "tmp:evidence-owner")
    evidence["value"]["frontmatter"]["transitions"] = [{"id": "tmp:evidence-member",
        "time": {"timeline": "main", "tick": 299, "order": 0}, "state": "accepted"}]
    belief = _knowledge(modern)
    belief["value"]["frontmatter"]["claim"]["genealogy"] = {
        "kind": "parentage", "payload": {"child_id": modern.find("Mara Vale", "character").id,
            "parent_id": modern.find("Oren Thane", "character").id, "basis": "biological"},
        "valid": {"from": {"timeline": "main", "tick": 0, "order": 0}},
        "evidence": [{"kind": "knowledge", "entity_id": "tmp:evidence-owner", "transition_id": "tmp:evidence-member"}]}
    operations = [{"type": "entity.update", "entity": modern.world_record.id,
                   "frontmatterPatch": {"capabilities": modern.config["capabilities"] + ["generational-knowledge-v1"]}},
                  belief, evidence, _event(modern), {"type": "knowledge.transition.append", "knowledge": "tmp:belief",
                  "transition": {"time": _time(), "state": "accepted", "causing_event": "tmp:rescue"}}]
    candidate, allocated, _, _ = _apply(modern, operations)
    authored = candidate.get(allocated["tmp:belief"]).frontmatter["claim"]["genealogy"]["evidence"][0]
    assert authored == {"kind": "knowledge", "entity_id": allocated["tmp:evidence-owner"],
                        "transition_id": allocated["tmp:evidence-member"]}
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]


def test_confirmed_atomic_writes_refusal_and_receipt_replay(ash_repo):
    world = ash_repo.load_world(cache_write=False)
    request = _request(world, [_event(world), _knowledge(world),
                             {"type": "knowledge.transition.append", "knowledge": "tmp:belief",
                              "transition": {"time": _time(), "state": "accepted", "causing_event": "tmp:rescue"}}])
    before_head = ash_repo.head()
    preview = changeset.preview(ash_repo, request, cache_write=False)
    assert preview["valid"]
    with pytest.raises(ConfirmationRequired):
        changeset.apply(ash_repo, request)
    assert ash_repo.head() == before_head
    bad = deepcopy(request)
    bad["operations"][2]["transition"]["time"] = _time(299)
    with pytest.raises(ValidationFailed):
        changeset.apply(ash_repo, bad)
    assert ash_repo.head() == before_head
    committed = changeset.apply(ash_repo, request, confirmation_token_value=preview["confirmationToken"])
    assert committed["newHead"] != before_head
    written = ash_repo.load_world(cache_write=False).get(preview["generatedIds"]["tmp:belief"])
    assert written.frontmatter["transitions"][0]["causing_event"] == preview["generatedIds"]["tmp:rescue"]
    replay = changeset.apply(ash_repo, request, confirmation_token_value=preview["confirmationToken"])
    assert replay["idempotentReplay"] and replay["newHead"] == committed["newHead"]
    with pytest.raises(ConflictError):
        changeset.apply(ash_repo, request | {"summary": "Changed"}, confirmation_token_value=preview["confirmationToken"])
    with pytest.raises(StaleRevision):
        changeset.preview(ash_repo, request, cache_write=False)
