from __future__ import annotations

from copy import deepcopy
from pathlib import Path

import pytest

from wedl import authoring, changeset
from wedl.errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, NotFound, StaleRevision, UsageError
from wedl.ids import id_from_seed
from wedl.model import World
from wedl.repository import Repository
from wedl.validation import validate_world


def _world():
    source = Repository(Path("src/wedl/data/ash_archive_v07")).load_world("WORKTREE", cache_write=False)
    return World("a" * 40, source.tree_oid, deepcopy(source.records), source.root, source.source_root)


class ReadRepository:
    def __init__(self, world, root=Path("unused")):
        self.world, self.root, self.loads = world, root, []
    def head(self): return self.world.revision
    def load_world(self, revision, *, cache_write=True):
        self.loads.append((revision, cache_write))
        assert revision == self.head() and cache_write is False
        return self.world


def _time(tick=300): return {"timeline": "main", "tick": str(tick), "order": "0"}


def _event(effects=None):
    return {"type": "event.create", "temporaryId": "tmp:rescue", "title": "Explicit rescue", "time": _time(),
            "effects": effects if effects is not None else [{"target": "Brass Restricted-Vault Key", "key": "condition", "operation": "set", "value": "open"}]}


def _belief(world, label="tmp:belief"):
    return {"type": "knowledge.create", "temporaryId": label, "value": {"frontmatter": {
        "schema": world.schema, "kind": "knowledge", "title": "New belief", "domain": "story.knowledge", "status": "canonical",
        "tags": [], "aliases": [], "knower": "Mara Vale", "claim": {"key": "rescue", "statement": "tmp:rescue"}, "transitions": [],
        "x-literal": {"entity": "tmp:rescue", "time": "tmp:rescue"}}, "bodyMarkdown": "tmp:rescue"}}


def _intent(world, operations):
    return {"action": "consequence.batch", "expectedHead": world.revision, "idempotencyKey": "batch-test", "operations": operations}


def _candidate(world, intent):
    payload = authoring.compile_intent(ReadRepository(world), intent)
    allocated = changeset._allocate(payload, consequence_batch=True)
    records, touched = changeset._apply(payload, world, allocated, consequence_batch=True)
    return World(world.revision, world.tree_oid, records, world.root, world.source_root), allocated, payload, touched


def test_named_temporary_batches_preserve_history_and_cursor():
    world = _world()
    intent = _intent(world, [_event(), _belief(world), {"type": "knowledge.transition.append", "knowledge": "tmp:belief",
        "transition": {"id": "tmp:learned", "time": _time(), "state": "suspected", "causing_event": "tmp:rescue", "note": "tmp:rescue"}}])
    before = deepcopy(intent)
    candidate, allocated, payload, _ = _candidate(world, intent)
    belief = candidate.get(allocated["tmp:belief"])
    assert intent == before
    assert belief.frontmatter["knower"] == world.find("Mara Vale").id
    assert belief.frontmatter["claim"]["statement"] == belief.body == "tmp:rescue"
    assert belief.frontmatter["x-literal"] == before["operations"][1]["value"]["frontmatter"]["x-literal"]
    assert belief.frontmatter["transitions"][0]["causing_event"] == allocated["tmp:rescue"]
    assert belief.frontmatter["transitions"][0]["note"] == "tmp:rescue"
    assert candidate.config.get("current_time") == world.config.get("current_time")
    assert candidate.current_time == world.current_time
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    assert allocated["tmp:learned"] == id_from_seed("knowledge-transition", f"{changeset._request_hash(payload)}:2:transitions:0")


def test_every_event_auxiliary_form_preserves_literal_values_and_supplied_ids():
    world = _world()
    effect = {"target": "Brass Restricted-Vault Key", "key": "condition", "operation": "set", "value": "tmp:rescue"}
    supplied = id_from_seed("effect", "explicit-effect")
    forms = [None, [], [effect], [effect | {"id": supplied}], [effect | {"id": "tmp:effect"}],
             [effect | {"id": supplied}, effect | {"id": "tmp:effect"}, effect]]
    for effects, supplied_event in [(effects, explicit) for effects in forms for explicit in (False, True)]:
        event = _event(effects or []) | {"bodyMarkdown": "tmp:rescue", "title": "tmp:rescue"}
        if effects is None: event.pop("effects")
        if supplied_event: event["id"] = id_from_seed("event", "explicit-event")
        candidate, allocated, payload, _ = _candidate(world, _intent(world, [event]))
        written = candidate.get(allocated["tmp:rescue"])
        expected_event = event["id"] if supplied_event else id_from_seed("event", f"{changeset._request_hash(payload)}:0:tmp:rescue")
        assert written.id == expected_event and written.frontmatter["schema"] == world.schema
        assert written.title == written.body == "tmp:rescue"
        for index, item in enumerate(written.frontmatter["effects"]):
            assert item["value"] == "tmp:rescue"
            expected = supplied if (effects[index].get("id") == supplied) else id_from_seed("effect", f"{changeset._request_hash(payload)}:0:effects:{index}")
            assert item["id"] == expected
        raw = deepcopy(payload)
        _, raw_ids = candidate, changeset._allocate(raw)
        if effects is None:
            raw_records, _ = changeset._apply(raw, world, raw_ids)
            assert raw_records[raw_ids["tmp:rescue"]].body == raw_ids["tmp:rescue"], "raw legacy recursive substitution remains unchanged"


def test_generic_operations_and_nested_state_resolve_only_declared_leaves():
    world = _world()
    world.config["state_keys"]["object"]["typed-data"] = {"type": "object", "properties": {"owner": {"type": "entity", "entity_kind": "character"}, "members": {"type": "array", "items": {"type": "entity", "entity_kind": "character"}}}}
    world.config["state_keys"]["object"]["literal-data"] = {"type": "object"}
    event = _event([{ "target": "Brass Restricted-Vault Key", "key": "typed-data", "operation": "set",
        "value": {"owner": {"entity": "Mara Vale", "note": "tmp:rescue"}, "members": [{"entity": "Oren Thane"}], "extension": {"entity": "tmp:rescue"}}},
        {"target": "Brass Restricted-Vault Key", "key": "literal-data", "operation": "set", "value": {"entity": "tmp:rescue"}}])
    update = {"type": "entity.update", "entity": "Brass Restricted-Vault Key", "frontmatterPatch": {"x-literal": {"entity": "tmp:rescue"}}, "bodyMarkdown": "tmp:rescue"}
    create = {"type": "entity.create", "temporaryId": "tmp:item", "value": {"frontmatter": {"schema": world.schema, "kind": "object", "title": "Literal item", "domain": "story.objects", "status": "canonical", "x-literal": "tmp:rescue"}, "bodyMarkdown": "tmp:rescue"}}
    candidate, allocated, _, _ = _candidate(world, _intent(world, [event, update, create]))
    effects = candidate.get(allocated["tmp:rescue"]).frontmatter["effects"]
    assert effects[0]["value"]["owner"]["entity"] == world.find("Mara Vale").id
    assert effects[0]["value"]["members"][0]["entity"] == world.find("Oren Thane").id
    assert effects[0]["value"]["owner"]["note"] == "tmp:rescue"
    assert effects[0]["value"]["extension"] == effects[1]["value"] == {"entity": "tmp:rescue"}
    assert candidate.get(world.find("Brass Restricted-Vault Key").id).body == candidate.get(allocated["tmp:item"]).body == "tmp:rescue"
    assert candidate.get(allocated["tmp:item"]).frontmatter["x-literal"] == "tmp:rescue"
    scene = world.by_kind("scene")[0]
    cleared, _, _, _ = _candidate(world, _intent(world, [{"type": "entity.update", "entity": scene.id,
        "frontmatterPatch": {"outcome_events": None}}]))
    assert "outcome_events" not in cleared.get(scene.id).frontmatter
    _spatial_generic_compatibility(world)
    _conversation_generic_compatibility(world)


def _spatial_generic_compatibility(world):
    location = deepcopy(world.by_kind("location")[0].frontmatter)
    location.update(id="location:review-place", title="Review place")
    create = {"type": "entity.create", "temporaryId": "tmp:place", "value": {"frontmatter": location, "bodyMarkdown": "tmp:place"}}
    belief = _belief(world)
    belief["value"]["frontmatter"]["transitions"] = [{"time": {"timeline": "main", "tick": 300, "order": 0}, "state": "suspected", "source_entity": "tmp:place"}]
    candidate, allocated, _, _ = _candidate(world, _intent(world, [create, belief]))
    assert allocated["tmp:place"] == "location:review-place"
    assert candidate.get("location:review-place").body == "tmp:place"
    assert candidate.get(allocated["tmp:belief"]).frontmatter["transitions"][0]["source_entity"] == "location:review-place"
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    for kind in ("location", "map", "anchor", "portal", "route", "overlay"):
        for explicit_temp in (False, True):
            operation = {"type": "entity.create", "value": {"schema": world.schema, "kind": kind, "id": f"{kind}:review-place", "title": "Review place"}}
            if explicit_temp: operation["temporaryId"] = "tmp:spatial"
            payload = authoring.compile_intent(ReadRepository(world), _intent(world, [operation]))
            assert payload["operations"][0]["value"]["id"] == f"{kind}:review-place"
            if explicit_temp: assert changeset._allocate(payload, consequence_batch=True)["tmp:spatial"] == f"{kind}:review-place"


def _conversation_generic_compatibility(world):
    conversation = world.find("The Interrupted Question", "conversation")
    speaker = conversation.frontmatter["participants"][0]["character"]
    first_time = deepcopy(conversation.frontmatter["time"]["start"])
    first_time["order"] += 1
    second_time = first_time | {"order": first_time["order"] + 1}
    first = {"type": "conversation.turn.append", "temporaryId": "tmp:first", "conversation": conversation.title,
        "turn": {"kind": "speech", "at": first_time, "speaker": world.get(speaker).title, "audience": ["participants"], "text": "tmp:first"}}
    second = {"type": "conversation.turn.append", "temporaryId": "tmp:second", "conversation": conversation.title,
        "turn": {"kind": "speech", "at": second_time, "speaker": world.get(speaker).title, "addressee": "char:" + speaker,
            "audience": ["char:" + speaker], "interrupts": "tmp:first", "text": "tmp:first"}}
    recollection = {"type": "conversation.recollection.record", "temporaryId": "tmp:memory", "conversation": conversation.title,
        "recollection": {"character": world.get(speaker).title, "at": deepcopy(second_time), "state": "remembered", "exact_turns": ["tmp:first"],
            "summary": "tmp:first", "remembered_quotes": [{"source_turn": "tmp:first", "speaker": world.get(speaker).title, "text": "tmp:first"}]}}
    operations = [{"type": "entity.update", "entity": conversation.title, "frontmatterPatch": {"turns": [], "recollections": []}},
        recollection, first, second]
    candidate, allocated, _, _ = _candidate(world, _intent(world, operations))
    source = candidate.get(conversation.id).frontmatter
    assert source["turns"][1]["interrupts"] == source["turns"][0]["id"] == allocated["tmp:first"]
    assert source["turns"][1]["text"] == source["recollections"][0]["summary"] == "tmp:first"
    assert source["recollections"][0]["exact_turns"] == [allocated["tmp:first"]]
    assert source["recollections"][0]["remembered_quotes"][0] == {"source_turn": allocated["tmp:first"], "speaker": speaker, "text": "tmp:first"}
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]
    supplied = deepcopy(operations)
    supplied[2]["turn"]["id"] = id_from_seed("conversation-turn", "supplied-first")
    supplied[1]["recollection"]["id"] = id_from_seed("conversation-recollection", "supplied-memory")
    explicit, mapped, _, _ = _candidate(world, _intent(world, supplied))
    assert mapped["tmp:first"] == supplied[2]["turn"]["id"]
    assert mapped["tmp:memory"] == supplied[1]["recollection"]["id"]
    assert explicit.get(conversation.id).frontmatter["turns"][1]["interrupts"] == mapped["tmp:first"]
    assert not [item for item in validate_world(explicit) if item["severity"] == "error"]
    for wrong in ("tmp:missing", "tmp:memory"):
        bad = deepcopy(operations); bad[-1]["turn"]["interrupts"] = wrong
        with pytest.raises(UsageError): authoring.compile_intent(ReadRepository(world), _intent(world, bad))
    backwards = deepcopy(operations); backwards[2]["turn"]["interrupts"] = "tmp:second"; backwards[-1]["turn"].pop("interrupts")
    invalid, _, _, _ = _candidate(world, _intent(world, backwards))
    assert any(item["code"] == "WDL-CONV-021" for item in validate_world(invalid))
    created_source = deepcopy(source)
    created_source.update(id=id_from_seed("conversation", "created-conversation"), title="New conversation")
    created_source["turns"][0]["speaker"] = world.get(speaker).title
    created_source["recollections"][0]["character"] = world.get(speaker).title
    create = {"type": "entity.create", "value": {"frontmatter": created_source, "bodyMarkdown": "tmp:first"}}
    created, _, _, _ = _candidate(world, _intent(world, [create]))
    assert not [item for item in validate_world(created) if item["severity"] == "error"]
    assert created.get(created_source["id"]).body == "tmp:first"
    wrapped = {"type": "conversation.create", "temporaryId": "tmp:conversation", "value": created_source}
    compiled = authoring.compile_intent(ReadRepository(world), _intent(world, [wrapped]))
    assert compiled["operations"][0]["value"]["turns"][0]["speaker"] == speaker
    assert compiled["operations"][0]["value"]["recollections"][0]["character"] == speaker


def test_closed_batch_shape_exact_times_and_reference_kinds():
    world = _world()
    intent = _intent(world, [_event()])
    mutations = [lambda value: value.update(consequence_batch=False), lambda value: value.update(authoringIntentHash="a" * 64),
        lambda value: value.pop("expectedHead"), lambda value: value.update(expectedHead="HEAD"), lambda value: value.update(idempotencyKey=" "),
        lambda value: value["operations"][0].update(extra=True), lambda value: value["operations"][0]["time"].update(tick=300),
        lambda value: value["operations"][0]["time"].update(tick="-0"), lambda value: value["operations"][0]["time"].update(tick="0300"),
        lambda value: value["operations"][0]["time"].update(order=str(2**31)), lambda value: value["operations"][0]["effects"][0].update(operation=[]),
        lambda value: value["operations"][0]["effects"][0].update(operation="clear"), lambda value: value["operations"][0].update(relatedStoryPoints=[], related_story_points=[]),
        lambda value: value["operations"].append(_event()), lambda value: value["operations"][0]["effects"][0].update(target="tmp:unknown")]
    for mutate in mutations:
        altered = deepcopy(intent); mutate(altered)
        with pytest.raises(UsageError): authoring.compile_intent(ReadRepository(world), altered)
    bad = _intent(world, [_event(), _belief(world), {"type": "knowledge.transition.append", "knowledge": "tmp:belief", "transition": {"time": _time(), "state": "accepted", "causing_event": "tmp:belief"}}])
    with pytest.raises(UsageError): authoring.compile_intent(ReadRepository(world), bad)
    malformed = _intent(world, [{"type": "entity.create", "temporaryId": "tmp:item", "value": {"frontmatter": []}}])
    with pytest.raises(UsageError, match="frontmatter must be an object"):
        authoring.compile_intent(ReadRepository(world), malformed)


def test_ambiguous_unknown_and_stale_plans_reuse_resolver_without_writes():
    world = _world(); repo = ReadRepository(world)
    intent = _intent(world, [_event()])
    original = world.find("Brass Restricted-Vault Key").title
    world.by_kind("object")[1].frontmatter["aliases"] = [original]
    with pytest.raises(NotFound, match="ambiguous"): authoring.compile_intent(repo, intent)
    intent["operations"][0]["effects"][0]["target"] = "No such entity"
    with pytest.raises(NotFound, match="could not resolve"): authoring.compile_intent(repo, intent)
    repo.loads.clear(); intent["expectedHead"] = "b" * 40
    with pytest.raises(StaleRevision): authoring.compile_intent(repo, intent)
    assert repo.loads == []


def test_batch_bounds_precede_reads_and_valid_preview_is_pure(monkeypatch):
    world = _world(); repo = ReadRepository(world)
    for operations in ([{**_event(), "temporaryId": f"tmp:event-{index}"} for index in range(101)],
                       [{"type": "entity.delete", "entity": world.by_kind("object")[0].id}] * 1001,
                       [_event([deepcopy(_event()["effects"][0]) for _ in range(1000)])]):
        with pytest.raises(UsageError, match="limit|1000"): authoring.compile_intent(repo, _intent(world, operations))
    assert repo.loads == []
    def forbidden(*args, **kwargs): raise AssertionError("preview mutated or compiled")
    monkeypatch.setattr(changeset, "compile_world", forbidden)
    monkeypatch.setattr(changeset, "compile_world_bytes", forbidden)
    result = authoring.preview_intent(repo, _intent(world, [_event()]))
    assert result["preview"]["valid"] and "_changes" not in result["preview"]
    assert repo.loads == [(world.revision, False), (world.revision, False)]


def test_complete_intent_confirmation_binds_aliases_and_internal_guard(monkeypatch):
    world = _world(); repo = ReadRepository(world)
    key = world.find("Brass Restricted-Vault Key")
    key.frontmatter["aliases"].append("Vault key alias")
    intent = _intent(world, [_event()]); alias = deepcopy(intent)
    alias["operations"][0]["effects"][0]["target"] = "Vault key alias"
    first = authoring.preview_intent(repo, intent); second = authoring.preview_intent(repo, alias)
    assert first["changeset"] == second["changeset"]
    assert first["preview"]["requestHash"] == second["preview"]["requestHash"]
    assert first["preview"]["confirmationToken"] != second["preview"]["confirmationToken"]
    with pytest.raises(ConfirmationRequired): authoring.apply_intent(repo, intent, allow_unconfirmed=True)
    for bad_hash in (None, "", "G" * 64):
        with pytest.raises(UsageError): changeset.preview(repo, first["changeset"], consequence_batch=True, authoring_intent_hash=bad_hash)
    with pytest.raises(ConfirmationRequired): changeset.apply(repo, first["changeset"], consequence_batch=True, authoring_intent_hash=None)


def test_typed_genealogy_beliefs_reuse_existing_evidence_admission():
    world = _world()
    event = _event()
    donor = _belief(world, "tmp:donor")
    donor["value"]["frontmatter"]["transitions"] = [{"id": "tmp:evidence", "time": {"timeline": "main", "tick": 299, "order": 0}, "state": "accepted"}]
    belief = _belief(world)
    belief["value"]["frontmatter"]["claim"]["genealogy"] = {"kind": "parentage", "payload": {"child_id": "Mara Vale", "parent_id": "Oren Thane", "basis": "biological"}, "valid": {"from": {"timeline": "main", "tick": 0, "order": 0}}, "evidence": [{"kind": "knowledge", "entity_id": "tmp:donor", "transition_id": "tmp:evidence"}]}
    operations = [{"type": "entity.update", "entity": world.world_record.title, "frontmatterPatch": {"capabilities": world.config["capabilities"] + ["generational-knowledge-v1"]}}, belief, donor, event,
        {"type": "knowledge.transition.append", "knowledge": "tmp:belief", "transition": {"time": _time(), "state": "suspected", "causing_event": "tmp:rescue"}}]
    candidate, allocated, _, _ = _candidate(world, _intent(world, operations))
    evidence = candidate.get(allocated["tmp:belief"]).frontmatter["claim"]["genealogy"]["evidence"][0]
    assert evidence == {"kind": "knowledge", "entity_id": allocated["tmp:donor"], "transition_id": allocated["tmp:evidence"]}
    assert not [item for item in validate_world(candidate) if item["severity"] == "error"]


def test_confirmed_batch_writer_refuses_changed_intent_and_replays_before_resolution(ash_repo, monkeypatch):
    world = ash_repo.load_world(cache_write=False)
    key = world.find("Brass Restricted-Vault Key")
    intent = _intent(world, [_event(), _belief(world), {"type": "knowledge.transition.append", "knowledge": "tmp:belief", "transition": {"time": _time(), "state": "accepted", "causing_event": "tmp:rescue"}}])
    plan = authoring.preview_intent(ash_repo, intent)
    before = ash_repo.head()
    with pytest.raises(ConfirmationRequired): authoring.apply_intent(ash_repo, intent)
    assert ash_repo.head() == before
    changed = deepcopy(intent); changed["operations"][0]["effects"][0]["target"] = key.id
    assert authoring.compile_intent(ash_repo, changed) == plan["changeset"]
    with pytest.raises(ConfirmationMismatch): authoring.apply_intent(ash_repo, changed, confirmation_token_value=plan["preview"]["confirmationToken"])
    assert ash_repo.head() == before
    result = authoring.apply_intent(ash_repo, intent, confirmation_token_value=plan["preview"]["confirmationToken"])
    assert result["newHead"] != before
    written = ash_repo.load_world(cache_write=False).get(plan["preview"]["generatedIds"]["tmp:belief"])
    assert written.frontmatter["transitions"][0]["causing_event"] == plan["preview"]["generatedIds"]["tmp:rescue"]
    def forbidden(*args, **kwargs): raise AssertionError("replay resolved against later HEAD")
    monkeypatch.setattr(ash_repo, "load_world", forbidden)
    replay = authoring.apply_intent(ash_repo, intent, confirmation_token_value=plan["preview"]["confirmationToken"])
    assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
    with pytest.raises(ConflictError): authoring.apply_intent(ash_repo, changed, confirmation_token_value=plan["preview"]["confirmationToken"])
    with pytest.raises(ConfirmationRequired): authoring.apply_intent(ash_repo, intent, allow_unconfirmed=True)
