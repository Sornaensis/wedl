from __future__ import annotations

from copy import deepcopy
import gc
from pathlib import Path
import subprocess

import pytest

from wedl.authoring import apply_intent, compile_intent, preview_intent
from wedl.api_schemas import components
from wedl.command_parser import parser
from wedl.changeset import preview
from wedl.compiler import compile_world
from wedl.errors import UsageError
from wedl.ids import id_from_seed
from wedl.model import Record, World
from wedl.query import causality, hypotheses, list_entities, search_world, show_entity, timeline, whereabouts
from wedl.repository import Repository
from wedl.search import hypothesis_source_search
from wedl.source import serialize_record
from wedl.validation import is_adoptable_canonical_record, validate_world


def _simple_repo(root: Path) -> Repository:
    story = root / "story"; story.mkdir(parents=True)
    ids = {key: id_from_seed(key, "hypothesis-test") for key in ("world", "character", "location", "scene", "event")}
    records = [
        ("world.md", {"schema": "wedl/v0.3", "kind": "world", "id": ids["world"], "title": "Test world", "domain": "world", "status": "canonical", "tags": [], "aliases": [], "default_timeline": "main", "timelines": [{"id": "main", "label": "Main"}], "state_keys": {"character": {"location": {"type": "entity", "entity_kind": "location"}}}, "relationship_metrics": {}, "embedding_policy": {"provider": "lsa", "model": "wedl-lsa-v1", "dimensions": 8, "max_features": 64}, "compilation_policy": {"default_profile": "hybrid"}}, "# Test world\n"),
        ("characters/mara.md", {"schema": "wedl/v0.3", "kind": "character", "id": ids["character"], "title": "Mara Vale", "domain": "cast", "status": "canonical", "tags": [], "aliases": [], "initial_state": {"location": {"entity": ids["location"]}}}, "# Mara Vale\n"),
        ("locations/gate.md", {"schema": "wedl/v0.3", "kind": "location", "id": ids["location"], "title": "River Gate", "domain": "places", "status": "canonical", "tags": [], "aliases": []}, "# River Gate\n"),
        ("scenes/gate.md", {"schema": "wedl/v0.3", "kind": "scene", "id": ids["scene"], "title": "Flood Gallery N", "domain": "scenes", "status": "closed", "tags": [], "aliases": [], "location": ids["location"], "time": {"start": {"tick": 1}, "current": {"tick": 1}, "end": {"tick": 1}}, "participants": [{"character": ids["character"], "from": {"tick": 1}}], "objects": [], "environments": [], "story_points": [], "observations": [], "conversations": []}, "# Flood Gallery N\n"),
        ("events/main/gate.md", {"schema": "wedl/v0.3", "kind": "event", "id": ids["event"], "title": "The Route Passes the River Gate", "domain": "plot", "status": "canonical", "tags": [], "aliases": [], "time": {"tick": 1}, "effects": []}, "# The Route Passes the River Gate\n"),
    ]
    for relative, frontmatter, body in records:
        target = story / relative; target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(serialize_record(frontmatter, body))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for command in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *command], check=True, capture_output=True)
    return Repository(root)


def _intent() -> dict[str, object]:
    return {
        "action": "hypothesis.create", "title": "The missing route",
        "statement": "The river gate may have been opened from the north shelf.",
        "subjects": ["Mara Vale"], "alternatives": ["A hidden clerk opened it.", "The gate failed on its own."],
        "context": "An author note while the route evidence remains incomplete.",
        "scene": "Flood Gallery N",
    }


def _assert_no_identifier_fields(value: object) -> None:
    if isinstance(value, dict):
        assert "id" not in value
        for child in value.values(): _assert_no_identifier_fields(child)
    elif isinstance(value, list):
        for child in value: _assert_no_identifier_fields(child)


def test_hypothesis_preview_is_nonoperative_and_valid(tmp_path: Path) -> None:
    plan = preview_intent(_simple_repo(tmp_path / "world"), _intent())
    assert plan["preview"]["valid"], plan["preview"]["diagnostics"]
    operation = plan["changeset"]["operations"][0]
    assert operation["value"]["frontmatter"]["kind"] == "hypothesis"
    assert operation["value"]["frontmatter"]["status"] == "open"
    assert "time" not in operation["value"]["frontmatter"]
    assert plan["authorImpact"]["items"] == [{"kind": "hypothesis-recorded", "hypothesis": "The missing route", "nonCanonical": True}]


def test_hypothesis_confirmed_apply_replays_idempotently(tmp_path: Path) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    intent = {**_intent(), "idempotencyKey": "hypothesis-create-once"}
    first = apply_intent(ash_repo, intent, allow_unconfirmed=True)
    replay = apply_intent(ash_repo, intent, allow_unconfirmed=True)
    assert first["idempotentReplay"] is False
    assert replay["idempotentReplay"] is True
    assert {key: value for key, value in replay.items() if key != "idempotentReplay"} == {key: value for key, value in first.items() if key != "idempotentReplay"}
    assert [item["title"] for item in hypotheses(ash_repo, require_compiled=True)["hypotheses"]] == ["The missing route"]


@pytest.mark.parametrize("field,value", [
    ("time", {"tick": 1}), ("tick", 1), ("order", 0), ("at", {"tick": 1}),
    ("start", {"tick": 1}), ("end", {"tick": 1}), ("duration", "one day"),
    ("timeline", "main"), ("scene", "some scene"), ("event", "some event"),
    ("turns", []), ("observations", []), ("effects", []), ("state", {}),
])
def test_hypothesis_structural_validation_rejects_operational_fields(tmp_path: Path, field: str, value: object) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    world = ash_repo.load_world("HEAD")
    payload = compile_intent(ash_repo, _intent())
    candidate = preview(ash_repo, payload)
    assert candidate["valid"], candidate["diagnostics"]
    operation = payload["operations"][0]
    frontmatter = deepcopy(operation["value"]["frontmatter"])
    frontmatter["id"] = "hyp_01HYPOTHESIS00000000000000"
    frontmatter[field] = value
    record = Record(frontmatter, operation["value"]["bodyMarkdown"], "story/hypotheses/test.md", serialize_record(frontmatter, operation["value"]["bodyMarkdown"]))
    altered = World(world.revision, world.tree_oid, {**world.records, record.id: record}, world.root, world.source_root)
    assert any(item["code"] == "WDL-HYP-010" and item["field"] == field for item in validate_world(altered))


def test_hypotheses_are_explicit_in_reads_and_search_only(tmp_path: Path) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    applied = apply_intent(ash_repo, _intent(), allow_unconfirmed=True)
    assert applied["authorImpact"]["items"][0]["nonCanonical"] is True
    listed = hypotheses(ash_repo, require_compiled=True)
    assert [item["title"] for item in listed["hypotheses"]] == ["The missing route"]
    assert listed["nonCanonical"] is True
    assert "id" not in listed["hypotheses"][0]
    assert all("id" not in reference for reference in listed["hypotheses"][0]["subjects"])
    _assert_no_identifier_fields(listed["hypotheses"][0]["placement"])
    with pytest.raises(UsageError): list_entities(ash_repo, "hypothesis", require_compiled=True)
    with pytest.raises(UsageError): show_entity(ash_repo, "The missing route", require_compiled=True)
    assert all(reference["kind"] != "hypothesis" for reference in show_entity(ash_repo, "Mara Vale", require_compiled=True)["inboundReferences"])
    normal = search_world(ash_repo, "north shelf", perspective="author", all_time=True, mode="fts", require_compiled=True)
    assert all(item["documentKind"] != "hypothesis" for item in normal["results"])
    opted_in = search_world(ash_repo, "north shelf", perspective="author", all_time=True, mode="fts", include_hypotheses=True, require_compiled=True)
    assert any(item["documentKind"] == "hypothesis" for item in opted_in["results"])
    assert hypothesis_source_search(ash_repo.load_world("HEAD"), "shelf", limit=10)
    assert hypothesis_source_search(ash_repo.load_world("HEAD"), "hel", limit=10) == []
    with pytest.raises(UsageError):
        search_world(ash_repo, "north shelf", perspective="character", character_id="Mara Vale", scene_id="Whispers in Flood Gallery N", mode="fts", include_hypotheses=True, require_compiled=True)
    # The canonical location projection remains unaffected by a possibility.
    before = whereabouts(ash_repo, "Mara Vale", 121, "main", 0, require_compiled=True)
    assert before["characters"][0]["character"]["title"] == "Mara Vale"


def test_adoption_requires_existing_canonical_records_and_changes_no_canon(tmp_path: Path) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    apply_intent(ash_repo, _intent(), allow_unconfirmed=True)
    adoption = {"action": "hypothesis.adopt", "hypothesis": "The missing route", "canonicalRecords": ["The Route Passes the River Gate"]}
    previewed = preview_intent(ash_repo, adoption)
    assert previewed["preview"]["valid"]
    assert "Canon is unchanged" in previewed["authorImpact"]["summary"]
    applied = apply_intent(ash_repo, adoption, allow_unconfirmed=True)
    assert applied["authorImpact"]["items"] == [{"kind": "hypothesis-status", "hypothesis": "The missing route", "status": "adopted", "nonCanonical": True}]
    projection = hypotheses(ash_repo, "The missing route", require_compiled=True)["hypothesis"]
    assert projection["resolution"]["canonicalRecords"] == [{"kind": "event", "title": "The Route Passes the River Gate"}]
    _assert_no_identifier_fields(projection)
    with pytest.raises(UsageError):
        compile_intent(ash_repo, {"action": "hypothesis.adopt", "hypothesis": "The missing route", "canonicalRecords": ["The missing route"]})
    with pytest.raises(UsageError):
        compile_intent(ash_repo, {"action": "hypothesis.reject", "hypothesis": "The missing route"})


def test_rejection_projection_uses_neutral_resolution_with_retained_note(tmp_path: Path) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    apply_intent(ash_repo, _intent(), allow_unconfirmed=True)
    apply_intent(ash_repo, {"action": "hypothesis.reject", "hypothesis": "The missing route", "note": "The gate ledger disproves this reading."}, allow_unconfirmed=True)
    projection = hypotheses(ash_repo, "The missing route", require_compiled=True)["hypothesis"]
    assert projection["status"] == "rejected"
    assert projection["resolution"] == {"note": "The gate ledger disproves this reading."}
    assert "adoption" not in projection
    _assert_no_identifier_fields(projection)


@pytest.mark.parametrize("field,value", [("timeline", []), ("timeline", {"name": "main"}), ("scene", []), ("location", {"id": "x"})])
def test_hypothesis_placement_requires_scalar_named_references(tmp_path: Path, field: str, value: object) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    payload = compile_intent(ash_repo, _intent()); frontmatter = deepcopy(payload["operations"][0]["value"]["frontmatter"])
    frontmatter["id"] = "hyp_01HYPOTHESIS00000000000000"; frontmatter["placement"][field] = value
    world = ash_repo.load_world("HEAD")
    record = Record(frontmatter, "", "story/hypotheses/test.md", serialize_record(frontmatter, ""))
    altered = World(world.revision, world.tree_oid, {**world.records, record.id: record}, world.root, world.source_root)
    assert any(item["code"] in {"WDL-HYP-007", "WDL-HYP-008"} and item["field"] == f"placement.{field}" for item in validate_world(altered))


def test_hypothesis_resolution_status_contract_and_adoptable_records(tmp_path: Path) -> None:
    ash_repo = _simple_repo(tmp_path / "world"); world = ash_repo.load_world("HEAD")
    assert is_adoptable_canonical_record(world.find("Flood Gallery N", "scene"))
    assert not is_adoptable_canonical_record(Record({**world.find("The Route Passes the River Gate", "event").frontmatter, "status": "draft"}, "", "draft.md", b""))
    payload = compile_intent(ash_repo, _intent()); base = deepcopy(payload["operations"][0]["value"]["frontmatter"]); base["id"] = "hyp_01HYPOTHESIS00000000000000"
    for status, resolution, code in (("open", {"note": "not allowed"}, "WDL-HYP-011"), ("adopted", {}, "WDL-HYP-011"), ("adopted", {"canonical_entities": [world.find("The Route Passes the River Gate", "event").id], "note": []}, "WDL-HYP-013"), ("rejected", {}, "WDL-HYP-013"), ("rejected", {"note": "No", "time": {"tick": 1}}, "WDL-HYP-014")):
        frontmatter = {**base, "status": status, "resolution": resolution}
        record = Record(frontmatter, "", "story/hypotheses/test.md", serialize_record(frontmatter, ""))
        altered = World(world.revision, world.tree_oid, {**world.records, record.id: record}, world.root, world.source_root)
        assert any(item["code"] == code for item in validate_world(altered))


def test_hypothesis_author_impact_schema_has_no_fake_story_time_requirement() -> None:
    item = components()["schemas"]["AuthorImpact"]["properties"]["items"]["items"]
    assert item["required"] == ["kind"]
    assert {"hypothesis-recorded", "hypothesis-status"}.issubset(item["properties"]["kind"]["enum"])
    assert item["properties"]["at"]["anyOf"][1] == {"type": "null"}


def test_hypothesis_rejection_note_is_required_by_cli_and_api_contract() -> None:
    with pytest.raises(UsageError):
        parser().parse_args(["author", "hypothesis", "reject", "The missing route"])
    accepted = parser().parse_args(["author", "hypothesis", "reject", "The missing route", "--note", "Evidence disproved it"])
    assert accepted.note == "Evidence disproved it"
    branches = components()["schemas"]["AuthoringRequest"]["oneOf"]
    rejected = next(branch for branch in branches if branch["properties"]["action"] == {"const": "hypothesis.reject"})
    assert {"action", "hypothesis", "note"}.issubset(rejected["required"])
    assert rejected["properties"]["note"] == {"type": "string", "minLength": 1}
    assert "hypothesis" not in components()["schemas"]["WedlEntity"]["properties"]["kind"]["enum"]


def test_hypothesis_cannot_perturb_canonical_compiled_retrieval_or_story_reads(tmp_path: Path) -> None:
    ash_repo = _simple_repo(tmp_path / "world")
    before_compile = compile_world(ash_repo, force=True)
    before_models = [(model["scope"], model["modelId"], model["corpusHash"]) for model in before_compile["vectorModels"]]
    before_search = {
        mode: [(item["documentId"], item["score"]) for item in search_world(ash_repo, "river gate", perspective="author", all_time=True, mode=mode, include_text=True, require_compiled=True)["results"]]
        for mode in ("fts", "vector", "hybrid")
    }
    before_timeline = {key: value for key, value in timeline(ash_repo, require_compiled=True).items() if key != "revision"}
    before_whereabouts = whereabouts(ash_repo, "Mara Vale", 1, "main", 0, require_compiled=True)["characters"]
    before_causality = causality(ash_repo, "The Route Passes the River Gate", require_compiled=True)
    # Query helpers intentionally use short-lived SQLite reads; collect those
    # handles before an atomic Windows cache replacement during apply.
    gc.collect()
    apply_intent(ash_repo, _intent(), allow_unconfirmed=True)
    after_compile = compile_world(ash_repo, force=True)
    assert [(model["scope"], model["modelId"], model["corpusHash"]) for model in after_compile["vectorModels"]] == before_models
    after_search = {
        mode: [(item["documentId"], item["score"]) for item in search_world(ash_repo, "river gate", perspective="author", all_time=True, mode=mode, include_text=True, require_compiled=True)["results"]]
        for mode in ("fts", "vector", "hybrid")
    }
    assert after_search == before_search
    assert {key: value for key, value in timeline(ash_repo, require_compiled=True).items() if key != "revision"} == before_timeline
    assert whereabouts(ash_repo, "Mara Vale", 1, "main", 0, require_compiled=True)["characters"] == before_whereabouts
    after_causality = causality(ash_repo, "The Route Passes the River Gate", require_compiled=True)
    assert {key: value for key, value in after_causality.items() if key != "revision"} == {key: value for key, value in before_causality.items() if key != "revision"}
