from __future__ import annotations

import json
from copy import deepcopy
import pytest

from wedl.authoring import apply_intent, compile_intent, preview_intent
from wedl.authoring import _detach_from_active_scenes
from wedl.authoring import _intent_hash
from wedl.authoring import _require_not_before_active_cursor
from wedl.authoring import _require_not_before_world_cursor
from wedl.authoring import _scene_characters_at
from wedl.authoring import _stationary_scene_objects
from wedl.authoring import _validate_scene_move_reconciliation
from wedl.api_schemas import SCHEMAS
from wedl.changeset import _apply, preview
from wedl.cli import main
from wedl.ids import id_from_seed
from wedl.model import Record, StoryTime
from wedl.semantics import resolve_state
from wedl.server import create_app


def test_scene_create_resolves_names_and_preserves_independent_continuity(frontiersmen_repo) -> None:
    intent = {
        "action": "scene.create", "title": "A Separate Watch", "location": "Harrowcross",
        "characters": ["Rhea"], "time": {"tick": 210},
    }
    payload = compile_intent(frontiersmen_repo, intent)
    plan = preview(frontiersmen_repo, payload)

    assert plan["valid"] is True
    assert payload["protocol"] == "wedl-changeset/v1"
    event = next(operation for operation in payload["operations"] if operation["type"] == "event.create")
    assert len(event["effects"]) == 1
    assert event["effects"][0]["key"] == "location"
    assert event["effects"][0]["target"].startswith("char_")
    # The new front is placed after the live cursor and the original presence
    # ends before it, rather than creating a hidden overlapping cast.
    created = next(operation for operation in payload["operations"] if operation["type"] == "entity.create")
    assert created["value"]["frontmatter"]["time"]["start"]["order"] == 1


def test_conversation_append_allocates_order_and_resolves_addressee(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "addressee": "Pip", "text": "Move now.", "interruptLast": True,
    })
    plan = preview(frontiersmen_repo, payload)
    turn = next(operation["turn"] for operation in payload["operations"] if operation["type"] == "conversation.turn.append")

    assert plan["valid"] is True
    assert turn["kind"] == "speech"
    assert turn["at"]["order"] == 1
    assert turn["addressee"].startswith("char_")
    assert turn["interrupts"].startswith("turn_")
    assert any(operation["type"] == "entity.update" for operation in payload["operations"])


def test_conversation_create_uses_named_scene_defaults_and_independent_subset(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {
        "action": "conversation.create", "title": "A Narrow Question", "scene": "Southward Cut",
        "characters": ["Rhea", "Pip"],
    })
    created = next(operation for operation in payload["operations"] if operation["type"] == "conversation.create")["value"]

    # Frontiersmen intentionally carries unrelated historical overlap
    # diagnostics while that repair work is pending; this new conversation
    # must not add a conversation diagnostic of its own.
    assert not any(item["code"].startswith("WDL-CONV") for item in preview(frontiersmen_repo, payload)["diagnostics"])
    assert created["status"] == "active"
    assert created["location"] == frontiersmen_repo.load_world().find("Southward Cut", "scene").frontmatter["location"]
    assert created["time"]["start"] == {"timeline": "main", "tick": 210, "order": 1}
    assert len(created["participants"]) == 2
    association = next(operation for operation in payload["operations"] if operation.get("entity") == created["scene"])
    assert association["frontmatterPatch"]["conversations"][-1] == "$author.conversation"
    plan = preview(frontiersmen_repo, payload)
    records, _touched = _apply(payload, frontiersmen_repo.load_world(), plan["generatedIds"])
    created_id = plan["generatedIds"]["$author.conversation"]
    # Scene context/timeline views traverse this reciprocal list, so the
    # generated conversation is immediately discoverable from its scene.
    assert created_id in records[created["scene"]].frontmatter["conversations"]
    assert records[created_id].frontmatter["scene"] == created["scene"]


def test_conversation_create_defaults_to_all_present_characters_and_rejects_nonpresent_selection(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {"action": "conversation.create", "title": "All Here"})
    created = next(operation for operation in payload["operations"] if operation["type"] == "conversation.create")["value"]

    assert len(created["participants"]) == 5
    with pytest.raises(Exception, match="present in the selected scene"):
        compile_intent(frontiersmen_repo, {
            "action": "conversation.create", "title": "Not Here", "scene": "Southward Cut",
            "characters": ["Jorund Bale"],
        })


def test_conversation_append_auto_advance_and_author_impact_are_name_only(frontiersmen_repo) -> None:
    intent = {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "text": "Not another word.",
    }
    response = preview_intent(frontiersmen_repo, intent)
    impact = response["authorImpact"]
    updates = [operation for operation in response["changeset"]["operations"] if operation["type"] == "entity.update"]

    assert impact["items"][0]["kind"] == "conversation-turn-appended"
    assert any(item["kind"] == "author-horizon-advanced" for item in impact["items"])
    assert updates[0]["frontmatterPatch"]["current_time"] == impact["items"][0]["at"]
    rendered = json.dumps(impact)
    assert "char_" not in rendered and "conv_" not in rendered and "$author" not in rendered


def test_explicitly_ordered_conversation_append_does_not_advance_horizon(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "text": "At this exact moment.", "time": {"tick": 210, "order": 2},
    })

    assert [operation["type"] for operation in payload["operations"]] == ["conversation.turn.append"]


def _activate_unbound_repair_loft_conversation(ash_repo, monkeypatch):
    world = ash_repo.load_world()
    conversation = world.find("The Repair Loft Warning", "conversation")
    conversation.frontmatter["status"] = "active"
    conversation.frontmatter["time"].pop("end", None)
    monkeypatch.setattr(ash_repo, "load_world", lambda *args, **kwargs: world)
    return world, conversation


def test_unbound_conversation_author_impact_allows_missing_scene_and_location(ash_repo, monkeypatch) -> None:
    _world, _conversation = _activate_unbound_repair_loft_conversation(ash_repo, monkeypatch)
    response = preview_intent(ash_repo, {
        "action": "conversation.append", "conversation": "The Repair Loft Warning", "speaker": "Varo Pell",
        "text": "Leave the ledger where it is.", "time": {"tick": 82, "order": 51},
    })
    item = response["authorImpact"]["items"][0]

    assert item["scene"] is None
    assert item["location"] == "Card-Repair Loft"
    properties = SCHEMAS["AuthorImpact"]["properties"]["items"]["items"]["properties"]
    assert properties["scene"]["type"] == ["string", "null"]
    assert properties["location"]["type"] == ["string", "null"]


def test_author_impact_allows_a_truly_unlocated_conversation(ash_repo, monkeypatch) -> None:
    world, conversation = _activate_unbound_repair_loft_conversation(ash_repo, monkeypatch)
    conversation.frontmatter["location"] = None

    response = preview_intent(ash_repo, {
        "action": "conversation.append", "conversation": "The Repair Loft Warning", "speaker": "Varo Pell",
        "text": "The ledger stays here.", "time": {"tick": 82, "order": 51},
    })

    assert response["authorImpact"]["items"][0]["location"] is None


def test_closed_conversation_append_requires_raw_changeset(ash_repo) -> None:
    with pytest.raises(Exception, match="requires an active conversation; use a raw changeset"):
        compile_intent(ash_repo, {
            "action": "conversation.append", "conversation": "The Repair Loft Warning", "speaker": "Varo Pell",
            "text": "This historical line stays explicit.",
        })


def _active_ash_conversation_world(ash_repo, *, concurrent: bool):
    """Make a valid, future-dated authoring fixture without changing sources."""

    world = ash_repo.load_world()
    point = {"timeline": "main", "tick": 300, "order": 0}
    scene = world.find("Flood Gallery N", "scene")
    conversation = world.find("Whispers in Flood Gallery N", "conversation")
    mara = world.find("Mara Vale", "character")
    mara_location = resolve_state(world, mara.id, StoryTime.from_value(point))[0]["location"]["entity"]
    scene.frontmatter.update({
        "status": "active", "time": {"start": point, "current": point, "end": None},
        "location": mara_location, "participants": [{"character": mara.id, "from": point}],
        "objects": [], "environments": [], "story_points": [], "observations": [], "conversations": [conversation.id],
    })
    conversation.frontmatter.update({
        "status": "active", "time": {"start": point}, "location": mara_location,
        "participants": [{"character": mara.id, "from": point}],
        "turns": [], "recollections": [],
    })
    world.world_record.frontmatter["current_time"] = deepcopy(point)
    second = None
    if concurrent:
        second = deepcopy(scene)
        second_id = id_from_seed("scene", "authoring-concurrent-second-front")
        ilyra = world.find("Ilyra Sorn", "character")
        ilyra_location = resolve_state(world, ilyra.id, StoryTime.from_value(point))[0]["location"]["entity"]
        second.frontmatter.update({
            "id": second_id, "title": "Archive Roof Watch", "status": "active",
            "location": ilyra_location, "participants": [{"character": ilyra.id, "from": point}],
            "conversations": [],
        })
        second.source_path = "story/scenes/archive-roof-watch--scene_authoring.md"
        world.records[second_id] = second
        world._cache.clear()
    return world, scene, conversation, second


def test_implicit_append_advances_every_concurrent_active_front(ash_repo, monkeypatch) -> None:
    world, scene, conversation, second = _active_ash_conversation_world(ash_repo, concurrent=True)
    monkeypatch.setattr(ash_repo, "load_world", lambda *args, **kwargs: world)

    payload = compile_intent(ash_repo, {
        "action": "conversation.append", "conversation": conversation.title, "speaker": "Mara Vale", "text": "Keep below the waterline.",
    })
    plan = preview(ash_repo, payload)
    turn = next(operation["turn"] for operation in payload["operations"] if operation["type"] == "conversation.turn.append")
    updates = [operation for operation in payload["operations"] if operation["type"] == "entity.update"]

    assert plan["valid"] is True, plan["diagnostics"]
    assert turn["at"] == {"timeline": "main", "tick": 300, "order": 1}
    assert {operation["entity"] for operation in updates} == {world.world_record.id, scene.id, second.id}
    assert updates[0]["frontmatterPatch"]["current_time"] == turn["at"]
    assert all(operation["frontmatterPatch"].get("time", {}).get("current") == turn["at"] for operation in updates[1:])


def test_implicit_append_migrates_legacy_singleton_cursor(ash_repo, monkeypatch) -> None:
    world, scene, conversation, _second = _active_ash_conversation_world(ash_repo, concurrent=False)
    world.world_record.frontmatter.pop("current_time")
    monkeypatch.setattr(ash_repo, "load_world", lambda *args, **kwargs: world)

    payload = compile_intent(ash_repo, {
        "action": "conversation.append", "conversation": conversation.title, "speaker": "Mara Vale", "text": "I can hear it now.",
    })
    plan = preview(ash_repo, payload)
    turn = next(operation["turn"] for operation in payload["operations"] if operation["type"] == "conversation.turn.append")
    updates = [operation for operation in payload["operations"] if operation["type"] == "entity.update"]

    assert plan["valid"] is True, plan["diagnostics"]
    assert {operation["entity"] for operation in updates} == {world.world_record.id, scene.id}
    assert updates[0]["frontmatterPatch"]["current_time"] == turn["at"]
    assert updates[1]["frontmatterPatch"]["time"]["current"] == turn["at"]


def test_explicit_tick_without_order_uses_next_safe_coordinate(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "text": "Keep going.", "time": {"tick": 210},
    })

    turn = next(operation["turn"] for operation in payload["operations"] if operation["type"] == "conversation.turn.append")
    assert turn["at"]["order"] > 0
    assert any(operation["type"] == "entity.update" for operation in payload["operations"])


def test_current_time_set_keeps_the_exact_requested_coordinate(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {"action": "current-time.set", "time": {"tick": 210}})

    assert payload["operations"][0]["frontmatterPatch"]["current_time"] == {"timeline": "main", "tick": 210, "order": 0}


def test_authoring_time_rejects_bool_and_fractional_coordinates(frontiersmen_repo) -> None:
    with pytest.raises(Exception, match="story time"):
        compile_intent(frontiersmen_repo, {"action": "current-time.set", "time": {"tick": 210.9}})
    with pytest.raises(Exception, match="story time"):
        compile_intent(frontiersmen_repo, {"action": "current-time.set", "time": {"tick": 210, "order": True}})


def test_authoring_runtime_rejects_untyped_http_like_fields(frontiersmen_repo) -> None:
    with pytest.raises(Exception, match="must be a string"):
        compile_intent(frontiersmen_repo, {"action": "character.move", "characters": ["Rhea"], "location": 9})
    with pytest.raises(Exception, match="array of strings"):
        compile_intent(frontiersmen_repo, {"action": "character.move", "characters": "Rhea", "location": "Harrowcross"})
    with pytest.raises(Exception, match="must be a boolean"):
        compile_intent(frontiersmen_repo, {"action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea", "text": "Stop.", "interruptLast": "yes"})
    with pytest.raises(Exception, match="requires an action"):
        apply_intent(frontiersmen_repo, {"action": 7})


def test_singleton_scene_advance_can_omit_its_scene_and_moves_present_characters(frontiersmen_repo) -> None:
    payload = compile_intent(frontiersmen_repo, {
        "action": "scene.advance", "location": "Harrowcross", "time": {"tick": 211},
    })
    event = next(operation for operation in payload["operations"] if operation["type"] == "event.create")

    assert len(event["effects"]) == 5
    assert all(effect["key"] == "location" for effect in event["effects"])
    assert preview(frontiersmen_repo, payload)["valid"] is True


def test_advance_rejects_closed_scene_and_invalid_beat_field_combinations(frontiersmen_repo) -> None:
    with pytest.raises(Exception, match="active scene"):
        compile_intent(frontiersmen_repo, {"action": "scene.advance", "scene": "Drowned Waymark", "time": {"tick": 211}})
    with pytest.raises(Exception, match="cannot move an active scene before"):
        compile_intent(frontiersmen_repo, {"action": "scene.advance", "scene": "Southward Cut", "time": {"tick": 209}})
    with pytest.raises(Exception, match="action beats cannot"):
        compile_intent(frontiersmen_repo, {"action": "conversation.append", "conversation": "The Southward Cut", "kind": "action", "speaker": "Rhea", "actors": ["Rhea"], "text": "Rhea runs."})
    with pytest.raises(Exception, match="distinct"):
        compile_intent(frontiersmen_repo, {"action": "conversation.append", "conversation": "The Southward Cut", "kind": "action", "actors": ["Rhea", "Rhea"], "text": "Rhea runs."})


def test_scene_close_rejects_historical_and_non_monotonic_coordinates(frontiersmen_repo) -> None:
    with pytest.raises(Exception, match="explicit nonblank scene reference"):
        compile_intent(frontiersmen_repo, {"action": "scene.close", "scene": "", "time": {"tick": 211}})
    with pytest.raises(Exception, match="requires an active scene"):
        compile_intent(frontiersmen_repo, {"action": "scene.close", "scene": "Drowned Waymark", "time": {"tick": 211}})
    with pytest.raises(Exception, match="cannot move an active scene before"):
        compile_intent(frontiersmen_repo, {"action": "scene.close", "scene": "Southward Cut", "time": {"tick": 209}})

    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    with pytest.raises(Exception, match="another timeline"):
        _require_not_before_active_cursor(world, scene, {"timeline": "parallel", "tick": 211, "order": 0}, "scene.close")
    with pytest.raises(Exception, match="must match the current scene location"):
        compile_intent(frontiersmen_repo, {"action": "scene.close", "scene": "Southward Cut", "location": "Harrowcross", "time": {"tick": 211}})


def test_scene_close_api_request_requires_a_nonblank_explicit_scene(frontiersmen_repo) -> None:
    close_schema = SCHEMAS["AuthoringRequest"]["oneOf"][3]
    assert close_schema["required"] == ["action", "scene"]
    assert close_schema["properties"]["scene"]["minLength"] == 1
    with pytest.raises(Exception, match="explicit nonblank scene reference"):
        compile_intent(frontiersmen_repo, {"action": "scene.close", "scene": "   ", "time": {"tick": 211}})


def test_scene_create_and_reconciled_move_cannot_rollback_shared_cursor(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    world.world_record.frontmatter["current_time"] = {"timeline": "main", "tick": 210, "order": 0}
    scene = world.active_scenes()[0]
    character = scene.frontmatter["participants"][0]["character"]

    with pytest.raises(Exception, match="shared current cursor backward"):
        _require_not_before_world_cursor(world, {"timeline": "main", "tick": 209, "order": 0}, "scene.create")
    with pytest.raises(Exception, match="another timeline"):
        _require_not_before_world_cursor(world, {"timeline": "parallel", "tick": 211, "order": 0}, "scene.create")
    with pytest.raises(Exception, match="cannot move an active scene before"):
        _validate_scene_move_reconciliation(world, scene, scene.frontmatter["location"], [character], {"timeline": "main", "tick": 209, "order": 0})


def test_reconciled_move_requires_existing_scene_location_and_no_reentry(frontiersmen_repo) -> None:
    with pytest.raises(Exception, match="destination must match"):
        compile_intent(frontiersmen_repo, {
            "action": "character.move", "characters": ["Rhea"], "location": "Harrowcross",
            "scene": "Southward Cut", "time": {"tick": 211},
        })

    payload = compile_intent(frontiersmen_repo, {
        "action": "character.move", "characters": ["Rhea"], "location": "Abandoned Warden Road",
        "scene": "Southward Cut", "time": {"tick": 211},
    })
    scene_patch = next(operation for operation in payload["operations"] if operation.get("entity") and operation["entity"].startswith("scene_"))
    assert "location" not in scene_patch["frontmatterPatch"]

    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    character = scene.frontmatter["participants"][0]["character"]
    scene.frontmatter["participants"][0]["to"] = {"timeline": "main", "tick": 210, "order": 0}
    with pytest.raises(Exception, match="cannot re-enter"):
        _validate_scene_move_reconciliation(
            world, scene, scene.frontmatter["location"], [character],
            {"timeline": "main", "tick": 211, "order": 0},
        )

    scene.frontmatter["participants"][0]["to"] = {"timeline": "main", "tick": 211, "order": 0}
    with pytest.raises(Exception, match="cannot re-enter"):
        _validate_scene_move_reconciliation(
            world, scene, scene.frontmatter["location"], [character],
            {"timeline": "main", "tick": 211, "order": 1},
        )
    scene.frontmatter["participants"][0].pop("to")
    scene.frontmatter["participants"][0]["from"] = {"timeline": "main", "tick": 211, "order": 1}
    _validate_scene_move_reconciliation(
        world, scene, scene.frontmatter["location"], [character],
        {"timeline": "main", "tick": 211, "order": 1},
    )


def test_reconciled_move_never_reopens_a_closed_participant_interval(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    character = scene.frontmatter["participants"][0]["character"]
    scene.frontmatter["participants"][0]["to"] = {"timeline": "main", "tick": 209, "order": 20}

    operations = _detach_from_active_scenes(
        world, [character], {"timeline": "main", "tick": 210, "order": 1},
    )

    assert operations == []
    assert scene.frontmatter["participants"][0]["to"] == {"timeline": "main", "tick": 209, "order": 20}


def test_reconciled_move_shortens_a_future_bounded_presence(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    character = scene.frontmatter["participants"][0]["character"]
    scene.frontmatter["participants"][0]["to"] = {"timeline": "main", "tick": 211, "order": 0}

    operations = _detach_from_active_scenes(
        world, [character], {"timeline": "main", "tick": 210, "order": 1},
    )

    participants = operations[0]["frontmatterPatch"]["participants"]
    assert participants[0]["to"] == {"timeline": "main", "tick": 210, "order": 0}


def test_scene_advance_uses_cast_present_at_the_requested_time(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    departed = scene.frontmatter["participants"][0]["character"]
    scene.frontmatter["participants"][0]["to"] = {"timeline": "main", "tick": 210, "order": 0}

    moving = _scene_characters_at(world, scene, StoryTime("main", 211, 0))

    assert departed not in moving
    assert len(moving) == len(scene.frontmatter["participants"]) - 1


def test_scene_advance_reports_stationary_old_location_props(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    object_id = id_from_seed("object", "stationary-authoring-prop")
    prop = deepcopy(world.by_kind("object")[0])
    prop.frontmatter["id"] = object_id
    prop.frontmatter["title"] = "Dropped Lantern"
    prop.frontmatter["initial_state"] = {"location": {"entity": scene.frontmatter["location"]}}
    world.records[object_id] = prop
    scene.frontmatter["objects"].append(object_id)
    at = StoryTime.from_value(scene.frontmatter["time"]["current"], world.default_timeline)
    present = [item["character"] for item in scene.frontmatter["participants"]]

    assert _stationary_scene_objects(world, scene, present, at) == ["Dropped Lantern"]


def test_scene_location_advance_uses_future_handoff_and_drop_state(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.active_scenes()[0]
    holder = scene.frontmatter["participants"][0]["character"]
    location = scene.frontmatter["location"]
    held_id = id_from_seed("object", "authoring-future-held-prop")
    dropped_id = id_from_seed("object", "authoring-future-dropped-prop")
    template = world.by_kind("object")[0]
    for object_id, title, initial_state in (
        (held_id, "Future Held Lantern", {"location": {"entity": location}}),
        (dropped_id, "Future Dropped Lantern", {"holder": {"entity": holder}}),
    ):
        prop = deepcopy(template)
        prop.frontmatter["id"] = object_id
        prop.frontmatter["title"] = title
        prop.frontmatter["initial_state"] = initial_state
        world.records[object_id] = prop
        scene.frontmatter["objects"].append(object_id)
    world.records[id_from_seed("event", "authoring-future-object-state")] = Record(
        frontmatter={
            "id": id_from_seed("event", "authoring-future-object-state"), "kind": "event",
            "title": "Future object handling", "domain": "events.authoring", "status": "canonical",
            "time": {"timeline": "main", "tick": 210, "order": 1},
            "effects": [
                {"id": id_from_seed("effect", "authoring-future-hold"), "target": held_id, "key": "holder", "operation": "set", "value": {"entity": holder}},
                {"id": id_from_seed("effect", "authoring-future-drop-location"), "target": dropped_id, "key": "location", "operation": "set", "value": {"entity": location}},
                {"id": id_from_seed("effect", "authoring-future-drop-holder"), "target": dropped_id, "key": "holder", "operation": "clear"},
            ],
        }, body="", source_path="", raw_bytes=b"",
    )
    cast = _scene_characters_at(world, scene, StoryTime("main", 211, 0))

    at_cursor = _stationary_scene_objects(world, scene, cast, StoryTime("main", 210, 0))
    before_move = _stationary_scene_objects(world, scene, cast, StoryTime("main", 211, -1))

    assert "Future Held Lantern" in at_cursor
    assert "Future Held Lantern" not in before_move
    assert "Future Dropped Lantern" not in at_cursor
    assert "Future Dropped Lantern" in before_move


def test_exact_semantic_retry_replays_after_head_has_advanced(frontiersmen_repo) -> None:
    intent = {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "text": "Keep east.", "idempotencyKey": "semantic-retry-test-v1",
    }
    plan = preview(frontiersmen_repo, compile_intent(frontiersmen_repo, intent))
    assert "authoringIntentHash" not in compile_intent(frontiersmen_repo, intent)
    first = apply_intent(frontiersmen_repo, intent, confirmation_token_value=plan["confirmationToken"])
    replay = apply_intent(frontiersmen_repo, intent, confirmation_token_value=plan["confirmationToken"])

    assert first["idempotentReplay"] is False
    assert replay == {**first, "idempotentReplay": True}


def test_legacy_authoring_receipt_replay_includes_safe_impact_fallback(frontiersmen_repo) -> None:
    intent = {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "text": "Keep east.", "idempotencyKey": "legacy-authoring-receipt-v1",
    }
    receipt_path = frontiersmen_repo.root / ".wedl" / "idempotency.json"
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(json.dumps({
        intent["idempotencyKey"]: {
            "authoringIntentHash": _intent_hash(intent),
            "confirmationToken": "wedl-confirmation/v1:legacy",
            "result": {"protocol": "wedl-command-result/v1", "status": "committed"},
        },
    }), encoding="utf-8")

    replay = apply_intent(frontiersmen_repo, intent, allow_unconfirmed=True)

    assert replay["idempotentReplay"] is True
    assert replay["authorImpact"] == {"summary": "Replayed authoring: conversation.append.", "items": []}
    assert "char_" not in json.dumps(replay["authorImpact"])


def test_cli_authoring_defaults_to_preview(frontiersmen_repo, capsys) -> None:
    code = main([
        "author", "move", "--repo", str(frontiersmen_repo.root), "--character", "Rhea",
        "--location", "Harrowcross", "--tick", "211",
    ])
    output = json.loads(capsys.readouterr().out)

    assert code == 0
    assert output["protocol"] == "wedl-author-preview/v1"
    assert output["preview"]["confirmationToken"].startswith("wedl-confirmation/v1:")


def test_authoring_http_preview_and_confirmed_apply(frontiersmen_repo) -> None:
    try:
        from fastapi.testclient import TestClient
    except RuntimeError as exc:  # environment dependency diagnosis, not product behavior
        pytest.skip(str(exc))
    intent = {
        "action": "conversation.append", "conversation": "The Southward Cut", "speaker": "Rhea",
        "text": "Hold the road.", "time": {"tick": 210, "order": 2},
        "idempotencyKey": "authoring-http-conversation-v1",
    }
    with TestClient(create_app(frontiersmen_repo.root)) as client:
        token = client.get("/api/session").json()["token"]
        headers = {"X-Wedl-Token": token}
        plan = client.post("/api/authoring/preview", json=intent, headers=headers)
        assert plan.status_code == 200
        assert plan.json()["preview"]["valid"] is True
        assert plan.json()["changeset"]["protocol"] == "wedl-changeset/v1"
        assert plan.json()["authorImpact"]["items"][0]["kind"] == "conversation-turn-appended"
        applied = client.post(
            "/api/authoring/apply", json=intent,
            headers={**headers, "X-Wedl-Confirmation": plan.json()["preview"]["confirmationToken"]},
        )
        assert applied.status_code == 200
        assert applied.json()["status"] == "committed"
        assert applied.json()["authorImpact"]["items"][0]["conversation"] == "The Southward Cut"
