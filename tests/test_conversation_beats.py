from __future__ import annotations

from wedl.changeset import apply, preview
from wedl.context import build_context
from wedl.query import conversation_view
from wedl.validation import validate_world


def _beat_payload(ash_repo):
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    nessa = world.find("Nessa Quill", "character")
    scene = world.find("Flood Gallery N", "scene")
    return {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "conversation-beats-v1",
        "summary": "record an interrupted exchange with choreography",
        "operations": [{
            "type": "conversation.create",
            "temporaryId": "$tmp.beat-conversation",
            "value": {
                "title": "A Moving Exchange", "status": "closed", "scene": scene.id,
                "location": scene.frontmatter["location"],
                "time": {"start": {"timeline": "main", "tick": 140, "order": 0}, "end": {"timeline": "main", "tick": 140, "order": 40}},
                "participants": [
                    {"character": mara.id, "from": {"timeline": "main", "tick": 140, "order": 0}, "to": {"timeline": "main", "tick": 140, "order": 40}},
                    {"character": nessa.id, "from": {"timeline": "main", "tick": 140, "order": 0}, "to": {"timeline": "main", "tick": 140, "order": 40}},
                ],
                "turns": [
                    {"id": "turn_00000000000000000000000100", "at": {"timeline": "main", "tick": 140, "order": 10}, "speaker": mara.id, "addressee": nessa.id, "text": "Keep your hand off the latch.", "audience": ["participants"]},
                    {"id": "turn_00000000000000000000000101", "kind": "action", "at": {"timeline": "main", "tick": 140, "order": 20}, "actors": [nessa.id], "text": "Nessa steps between Mara and the flooded stair.", "audience": ["participants"]},
                    {"id": "turn_00000000000000000000000102", "at": {"timeline": "main", "tick": 140, "order": 30}, "speaker": nessa.id, "interrupts": "turn_00000000000000000000000100", "text": "Too late.", "audience": ["participants"]},
                ],
                "recollections": [
                    {"id": "recol_00000000000000000000000100", "character": mara.id, "at": {"timeline": "main", "tick": 140, "order": 40}, "state": "remembered", "summary": "Nessa moved before answering.", "confidence": 0.8, "exact_turns": ["turn_00000000000000000000000102"], "remembered_quotes": []},
                ],
            },
        }, {
            "type": "entity.update",
            "entity": scene.id,
            "frontmatterPatch": {
                "conversations": [*(scene.frontmatter.get("conversations") or []), "$tmp.beat-conversation"],
            },
        }],
    }


def test_conversation_beats_are_ordered_while_legacy_transcripts_remain_speech_only(ash_repo) -> None:
    payload = _beat_payload(ash_repo)
    assert preview(ash_repo, payload)["valid"] is True
    receipt = apply(ash_repo, payload, allow_unconfirmed=True)
    conversation_id = receipt["generatedIds"]["$tmp.beat-conversation"]
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")

    author = conversation_view(ash_repo, conversation_id, perspective="author", all_time=True)
    character = conversation_view(ash_repo, conversation_id, perspective="character", character_id=mara.id, tick=140, order=40)

    assert [beat["kind"] for beat in author["beats"]] == ["speech", "action", "speech"]
    assert author["beats"][0]["addressee"] == "Nessa Quill"
    assert author["beats"][1]["actors"] == ["Nessa Quill"]
    assert author["beats"][2]["interrupts"] == "turn_00000000000000000000000100"
    assert len(author["verbatimTurns"]) == 2
    assert len(character["perceivedBeats"]) == 3
    assert len(character["heardVerbatimTurns"]) == 2


def test_conversation_beat_validation_rejects_unsafe_actions_and_non_speech_exact_quotes(ash_repo) -> None:
    payload = _beat_payload(ash_repo)
    receipt = apply(ash_repo, payload, allow_unconfirmed=True)
    world = ash_repo.load_world()
    conversation = world.get(receipt["generatedIds"]["$tmp.beat-conversation"])
    action = conversation.frontmatter["turns"][1]
    action["actors"] = []
    action["effects"] = [{"target": "not-allowed"}]
    action["speaker"] = conversation.frontmatter["participants"][0]["character"]
    conversation.frontmatter["turns"][0]["actors"] = [conversation.frontmatter["participants"][0]["character"]]
    conversation.frontmatter["turns"][2]["interrupts"] = action["id"]
    conversation.frontmatter["recollections"][0]["exact_turns"] = [action["id"]]

    codes = {item["code"] for item in validate_world(world)}
    assert {"WDL-CONV-021", "WDL-CONV-022", "WDL-CONV-026", "WDL-CONV-027", "WDL-CONV-028"} <= codes


def test_author_context_marks_action_beats_without_turning_them_into_quotes(ash_repo) -> None:
    apply(ash_repo, _beat_payload(ash_repo), allow_unconfirmed=True)
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("Flood Gallery N", "scene")

    packet = build_context(
        ash_repo,
        character_id=mara.id,
        scene_id=scene.id,
        perspective="author",
        tick=140,
        order=40,
        max_characters=20_000,
    )

    prompt = packet["promptText"]
    assert "Action — Nessa Quill:" in prompt
    assert "Nessa steps between Mara and the flooded stair." in prompt
    assert "“Nessa steps between Mara and the flooded stair.”" not in prompt


def test_character_context_includes_only_perceivable_action_beats_unquoted(ash_repo) -> None:
    apply(ash_repo, _beat_payload(ash_repo), allow_unconfirmed=True)
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("Flood Gallery N", "scene")

    packet = build_context(
        ash_repo,
        character_id=mara.id,
        scene_id=scene.id,
        perspective="character",
        tick=140,
        order=40,
        max_characters=20_000,
    )

    prompt = packet["promptText"]
    assert "Action — Nessa Quill:" in prompt
    assert "Nessa steps between Mara and the flooded stair." in prompt
    assert "“Nessa steps between Mara and the flooded stair.”" not in prompt
