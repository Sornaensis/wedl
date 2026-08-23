from __future__ import annotations

from wedl.model import StoryTime
from wedl.semantics import current_knowledge, evaluate_all_story_points, resolve_state


def test_temporal_object_holder(ash_repo) -> None:
    world = ash_repo.load_world()
    letter = world.find("Sealed Heron Letter", "object")
    mara = world.find("Mara Vale", "character")
    before, _ = resolve_state(world, letter.id, StoryTime("main", 119, 99))
    after, _ = resolve_state(world, letter.id, StoryTime("main", 120, 99))
    assert before.get("holder") != {"entity": mara.id}
    assert after["holder"] == {"entity": mara.id}


def test_story_point_history_is_not_rewritten(ash_repo) -> None:
    world = ash_repo.load_world()
    at_zero = {item["title"]: item["derivedState"] for item in evaluate_all_story_points(world, StoryTime("main", 0, 99))}
    at_56 = {item["title"]: item["derivedState"] for item in evaluate_all_story_points(world, StoryTime("main", 56, 99))}
    assert at_zero["Audit the Missing Cards"] == "dormant"
    assert at_56["Audit the Missing Cards"] == "resolved"


def test_expanded_knowledge_appears_at_correct_time(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    before = {item["claimKey"] for item in current_knowledge(world, mara.id, StoryTime("main", 132, 50))}
    after = {item["claimKey"] for item in current_knowledge(world, mara.id, StoryTime("main", 133, 0))}
    assert "ysabet.created-hour" not in before
    assert "ysabet.created-hour" in after
