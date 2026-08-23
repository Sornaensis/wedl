from __future__ import annotations

from wedl.query import conversation_view


def test_late_arriving_character_hears_only_later_turns(ash_repo) -> None:
    world = ash_repo.load_world()
    conversation = world.find("Whispers in Flood Gallery N", "conversation")
    mara = world.find("Mara Vale", "character")
    ysabet = world.find("Ysabet Crane", "character")
    mara_view = conversation_view(ash_repo, conversation.id, perspective="character", character_id=mara.id)
    ysabet_view = conversation_view(ash_repo, conversation.id, perspective="character", character_id=ysabet.id)
    assert len(mara_view["heardVerbatimTurns"]) == 10
    assert len(ysabet_view["heardVerbatimTurns"]) == 7
    assert mara_view["conversationTime"]["end"] == {"timeline": "main", "tick": 142, "order": 40}
    assert mara_view["effectiveTime"] == {"timeline": "main", "tick": 142, "order": 40}
    assert ysabet_view["heardVerbatimTurns"]
    assert all(
        (turn["at"]["tick"], turn["at"]["order"]) >= (139, 20)
        for turn in ysabet_view["heardVerbatimTurns"]
    )
    assert "These entries are not accessions" not in " ".join(
        turn["text"] for turn in ysabet_view["heardVerbatimTurns"]
    )


def test_recollections_are_separate_from_transcript(ash_repo) -> None:
    world = ash_repo.load_world()
    conversation = world.find("The Interrupted Question", "conversation")
    mara = world.find("Mara Vale", "character")
    nessa = world.find("Nessa Quill", "character")
    author = conversation_view(ash_repo, conversation.id, perspective="author")
    mara_view = conversation_view(ash_repo, conversation.id, perspective="character", character_id=mara.id, tick=208)
    nessa_view = conversation_view(ash_repo, conversation.id, perspective="character", character_id=nessa.id, tick=208)
    assert len(author["verbatimTurns"]) == 6
    assert mara_view["subjectiveRecollection"]["summary"] != nessa_view["subjectiveRecollection"]["summary"]
    assert all(turn["text"] in [item["text"] for item in author["verbatimTurns"]] for turn in mara_view["heardVerbatimTurns"])


def test_author_conversation_is_time_sliced_unless_all_time_is_explicit(ash_repo) -> None:
    world = ash_repo.load_world()
    conversation = world.find("Whispers in Flood Gallery N", "conversation")

    bounded = conversation_view(
        ash_repo,
        conversation.id,
        perspective="author",
        tick=139,
        order=10,
    )
    full = conversation_view(ash_repo, conversation.id, perspective="author", all_time=True)

    assert bounded["protocol"] == "wedl-conversation/v2"
    assert bounded["timeScope"] == {
        "mode": "as-of",
        "at": {"timeline": "main", "tick": 139, "order": 10},
    }
    assert len(bounded["verbatimTurns"]) == 3
    assert len(bounded["recollections"]) == 2
    assert all(turn["at"]["tick"] <= 139 for turn in bounded["verbatimTurns"])
    assert "Open this door" not in " ".join(turn["text"] for turn in bounded["verbatimTurns"])
    assert full["timeScope"] == {"mode": "all-time"}
    assert len(full["verbatimTurns"]) > len(bounded["verbatimTurns"])
