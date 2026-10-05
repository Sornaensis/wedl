"""Literal character assertions and explicit historical learning provenance."""
from copy import deepcopy

import pytest

from wedl.generational_knowledge import GenealogyAssertion, validate_genealogy_knowledge
from wedl.ids import id_from_seed
from wedl.model import Record, StoryTime
from wedl.semantics import current_knowledge
from wedl.source import parse_record, serialize_record
from wedl.validation import validate_world
from wedl.v07 import canonical_capabilities

from test_generational_compiler import _world


def _time(tick, order=0):
    return {"timeline": "main", "tick": tick, "order": order}


def _assertion(mapping):
    return {"kind": "parentage", "payload": {
        "child_id": mapping["character_child"], "parent_id": mapping["character_alpha"],
        "basis": "biological"}, "valid": {"from": _time(-10)},
        "labels": {mapping["character_alpha"]: "My remembered parent"}}


def _record(kind, name, **fields):
    identifier = id_from_seed(kind, name)
    data = {"schema": "wedl/v0.7", "kind": kind, "id": identifier,
            "title": name, "domain": "knowledge.genealogy", "status": "canonical",
            "tags": [], "aliases": [], **fields}
    return Record(data, "authored body\n", f"story/{kind}/{identifier}.md", b"")


def _knowledge(world, mapping, name="belief", *, learned=None, assertion=None):
    record = _record("knowledge", name, knower=mapping["character_child"],
                     claim={"key": name, "statement": "An explicitly authored belief",
                            "genealogy": deepcopy(assertion or _assertion(mapping))},
                     transitions=[{"id": id_from_seed("knowledge-transition", name),
                                   "time": learned or _time(2), "state": "suspected",
                                   "confidence": 0.6, "acquisition": "learned"}])
    world.records[record.id] = record
    return record


def _world_with_knowledge():
    world, mapping = _world()
    world.world_record.frontmatter["capabilities"].append("generational-knowledge-v1")
    return world, mapping


def test_typed_assertion_literals_are_closed_immutable_and_roundtrip():
    world, mapping = _world_with_knowledge()
    character = mapping["character_child"]
    other = mapping["character_alpha"]
    organization = next(r.id for r in world if r.kind == "organization")
    legacy = next(r.id for r in world if r.kind == "legacy")
    payloads = {
        "parentage": {"child_id": character, "parent_id": other, "basis": "adoptive"},
        "union": {"participant_ids": sorted([character, other]), "state": "formed"},
        "organization": {"organization_id": organization, "parent_id": None},
        "affiliation": {"character_id": character, "organization_id": organization, "role": "Keeper"},
        "tenure": {"legacy_id": legacy, "holder_id": None, "basis": "legal"},
        "claim": {"legacy_id": legacy, "claimant_id": other, "state": "disputed"},
        "vital": {"character_id": character, "state": "dead"},
    }
    for kind, payload in payloads.items():
        data = {"kind": kind, "payload": payload, "valid": {"from": _time(-5), "until": _time(3, 1)}}
        assertion = GenealogyAssertion.from_value(data)
        with pytest.raises(TypeError):
            assertion.payload["extra"] = True
        data["payload"] = {**payload, "canonical_record_id": other}
        with pytest.raises(ValueError):
            GenealogyAssertion.from_value(data)
    belief = _knowledge(world, mapping)
    wire = serialize_record(belief.frontmatter, belief.body)
    parsed = parse_record(wire, belief.source_path)
    assert parsed.frontmatter == belief.frontmatter and parsed.body == belief.body
    assert serialize_record(parsed.frontmatter, parsed.body) == wire
    for mutate in (
        lambda a: a.update(kind=[]),
        lambda a: a["payload"].update(basis=[]),
        lambda a: a["valid"].update(until=_time(-11)),
        lambda a: a["valid"]["from"].update(tick=True),
        lambda a: a["valid"]["from"].update(tick=2**63),
        lambda a: a["valid"]["from"].update(tick="-10"),
        lambda a: a.update(labels={other: ""}),
        lambda a: a.update(labels={other: "x" * 201}),
        lambda a: a.update(labels={mapping["character_gamma"]: "Unlearned name"}),
        lambda a: a.update(evidence=[{"kind": "source_entity", "entity_id": other}]),
        lambda a: a.update(evidence=[None]),
        lambda a: a.update(evidence=[None] * 33),
    ):
        malformed = _assertion(mapping)
        mutate(malformed)
        with pytest.raises(ValueError):
            GenealogyAssertion.from_value(malformed)
    too_many = {"kind": "union", "payload": {
        "participant_ids": sorted(id_from_seed("character", f"participant-{i}") for i in range(33)),
        "state": "formed"}, "valid": {"from": _time(0)}}
    with pytest.raises(ValueError):
        GenealogyAssertion.from_value(too_many)


def test_wrong_conflicting_and_cyclic_beliefs_opt_in_without_canon_matching():
    world, mapping = _world_with_knowledge()
    belief = _knowledge(world, mapping)
    belief.frontmatter["transitions"][0]["state"] = "accepted"
    reverse = _assertion(mapping)
    reverse["payload"].update(child_id=mapping["character_alpha"], parent_id=mapping["character_child"])
    reverse.pop("labels")
    _knowledge(world, mapping, "reverse wrong belief", assertion=reverse)
    conflict = _assertion(mapping)
    conflict["payload"]["parent_id"] = mapping["character_beta"]
    conflict.pop("labels")
    _knowledge(world, mapping, "conflicting wrong belief", assertion=conflict)
    assert validate_world(world) == []
    assert canonical_capabilities(["generational-knowledge-v1"]) is None
    assert canonical_capabilities(["spatial-core-v1", "generational-knowledge-v1"]) is None
    assert canonical_capabilities(["generational-knowledge-v1", "generational-core-v1"]) is None
    missing = _assertion(mapping)
    missing["payload"]["parent_id"] = id_from_seed("character", "missing endpoint")
    missing.pop("labels")
    missing_record = _knowledge(world, mapping, "missing endpoint belief", assertion=missing)
    assert any(d["code"] == "GEN-KNOWLEDGE-003" for d in validate_genealogy_knowledge(world, missing_record))
    world.records.pop(missing_record.id)
    original = deepcopy(belief.frontmatter)
    for mutate, code in (
        (lambda data: data["claim"].update(canonical_correction="author only"), "GEN-KNOWLEDGE-002"),
        (lambda data: data["transitions"].clear(), "GEN-KNOWLEDGE-005"),
        (lambda data: data["transitions"][0].pop("id"), "GEN-KNOWLEDGE-004"),
        (lambda data: data["transitions"][0].update(confidence="author correction"), "GEN-KNOWLEDGE-002"),
        (lambda data: data["transitions"][0].update(state=[]), "GEN-KNOWLEDGE-005"),
        (lambda data: data["claim"]["genealogy"]["valid"]["from"].update(timeline="undeclared"), "GEN-KNOWLEDGE-004"),
    ):
        belief.frontmatter = deepcopy(original)
        mutate(belief.frontmatter)
        assert any(d["code"] == code for d in validate_genealogy_knowledge(world, belief))
    belief.frontmatter = deepcopy(original)
    belief.frontmatter["transitions"][0]["confidence"] = 10 ** 400
    assert any(d["code"] == "GEN-KNOWLEDGE-002" for d in validate_world(world))
    belief.frontmatter = deepcopy(original)
    world.world_record.frontmatter["capabilities"].remove("generational-knowledge-v1")
    assert any(d["code"] == "GEN-KNOWLEDGE-001" for d in validate_world(world))
    world.world_record.frontmatter["capabilities"].append("generational-knowledge-v1")
    belief.frontmatter["schema"] = "wedl/v0.3"
    assert any(d["code"] == "GEN-KNOWLEDGE-001" for d in validate_genealogy_knowledge(world, belief))
    legacy_claim = {"key": "legacy", "statement": "free prose", "arbitrary": {"legacy": True}}
    belief.frontmatter["claim"] = legacy_claim
    before = serialize_record(belief.frontmatter, belief.body)
    assert validate_genealogy_knowledge(world, belief) == []
    assert parse_record(before, belief.source_path).frontmatter["claim"] == legacy_claim
    assert serialize_record(belief.frontmatter, belief.body) == before


def test_exact_evidence_is_historical_admitted_and_same_character():
    world, mapping = _world_with_knowledge()
    knower, other = mapping["character_child"], mapping["character_alpha"]
    belief = _knowledge(world, mapping)
    source_knowledge = _knowledge(world, mapping, "prior knowledge", learned=_time(-2))
    source_knowledge.frontmatter["claim"] = {"key": "heard", "statement": "A fallible report"}
    observation_id = id_from_seed("observation", "explicit report")
    scene = _record("scene", "report scene", status="closed",
                    time={"start": _time(-3), "end": _time(0)},
                    participants=[{"character": knower}],
                    observations=[{"id": observation_id, "at": _time(-2), "until": _time(-1),
                                   "text": "A fallible observation", "audience": [knower]}])
    turn_id, recollection_id = id_from_seed("conversation-turn", "heard report"), id_from_seed("conversation-recollection", "fallible memory")
    conversation = _record("conversation", "report conversation", status="closed",
                           time={"start": _time(-3), "end": _time(0)},
                           participants=[{"character": knower}, {"character": other}],
                           turns=[{"id": turn_id, "speaker": other, "at": _time(-2),
                                   "text": "A mistaken spoken report", "audience": ["participants"]}],
                           recollections=[{"id": recollection_id, "character": knower,
                                          "at": _time(1), "state": "remembered", "exact_turns": [turn_id]}])
    for record in (scene, conversation):
        world.records[record.id] = record
    evidence = [
        {"kind": "knowledge", "entity_id": source_knowledge.id, "transition_id": source_knowledge.frontmatter["transitions"][0]["id"]},
        {"kind": "observation", "entity_id": scene.id, "observation_id": observation_id},
        {"kind": "turn", "entity_id": conversation.id, "turn_id": turn_id},
        {"kind": "recollection", "entity_id": conversation.id, "recollection_id": recollection_id},
    ]
    belief.frontmatter["claim"]["genealogy"]["evidence"] = evidence
    assert validate_genealogy_knowledge(world, belief) == []
    # The observation has expired and the conversation ended by learning:
    # admissibility is checked at the authored evidence time, not the read time.
    scene.frontmatter["observations"][0]["salience"] = 10 ** 400
    belief.frontmatter["claim"]["genealogy"]["evidence"] = [evidence[1]]
    assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
    scene.frontmatter["observations"][0].pop("salience")
    for item in evidence:
        field = next(k for k in item if k not in {"kind", "entity_id"})
        bad = deepcopy(item)
        bad[field] = id_from_seed({"knowledge": "knowledge-transition", "observation": "observation", "turn": "conversation-turn", "recollection": "conversation-recollection"}[item["kind"]], "missing")
        belief.frontmatter["claim"]["genealogy"]["evidence"] = [bad]
        assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
    belief.frontmatter["claim"]["genealogy"]["evidence"] = [evidence[2]]
    conversation.frontmatter["turns"][0]["audience"] = [other]
    assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
    conversation.frontmatter["turns"][0]["audience"] = ["participants"]
    conversation.frontmatter["turns"][0]["kind"] = "action"
    assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
    conversation.frontmatter["turns"][0].pop("kind")
    for item, source, field in (
        (evidence[0], source_knowledge.frontmatter["transitions"][0], "time"),
        (evidence[1], scene.frontmatter["observations"][0], "at"),
        (evidence[2], conversation.frontmatter["turns"][0], "at"),
        (evidence[3], conversation.frontmatter["recollections"][0], "at"),
    ):
        original = source[field]
        source[field] = _time(2, 1)
        belief.frontmatter["claim"]["genealogy"]["evidence"] = [item]
        assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
        source[field] = original
    belief.frontmatter["claim"]["genealogy"]["evidence"] = [evidence[3]]
    conversation.frontmatter["recollections"][0]["character"] = other
    assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
    belief.frontmatter["claim"]["genealogy"]["evidence"] = [evidence[0]]
    source_knowledge.frontmatter["knower"] = other
    assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))
    source_knowledge.frontmatter["knower"] = knower
    source_knowledge.frontmatter["transitions"][0]["time"] = _time(2)
    assert validate_genealogy_knowledge(world, belief) == []
    source_knowledge.frontmatter["transitions"][0]["state"] = "forgotten"
    assert any(d["code"] == "GEN-KNOWLEDGE-006" for d in validate_genealogy_knowledge(world, belief))


def test_retrospective_assertion_learning_and_new_correction_preserve_boundaries():
    world, mapping = _world_with_knowledge()
    belief = _knowledge(world, mapping, learned=_time(1, 1))
    belief.frontmatter["transitions"].insert(0, {
        "id": id_from_seed("knowledge-transition", "rejected before learning"),
        "time": _time(-1), "state": "rejected"})
    before = serialize_record(belief.frontmatter, belief.body)
    knower = mapping["character_child"]
    assert validate_world(world) == []
    assert current_knowledge(world, knower, StoryTime("main", -1, 0), include_forgotten=True) == []
    assert current_knowledge(world, knower, StoryTime("main", 1, 0)) == []
    learned = current_knowledge(world, knower, StoryTime("main", 1, 1))
    assert learned[0]["state"] == "suspected" and learned[0]["claim"]["genealogy"]["valid"]["from"] == _time(-10)
    belief.frontmatter["transitions"].extend([
        {"id": id_from_seed("knowledge-transition", "reject"), "time": _time(2), "state": "rejected"},
        {"id": id_from_seed("knowledge-transition", "forget"), "time": _time(3), "state": "forgotten"},
    ])
    assert current_knowledge(world, knower, StoryTime("main", 2, 0))[0]["state"] == "rejected"
    assert current_knowledge(world, knower, StoryTime("main", 3, 0)) == []
    corrected = _assertion(mapping)
    corrected["payload"]["parent_id"] = mapping["character_biological"]
    corrected.pop("labels")
    new = _knowledge(world, mapping, "corrected assertion", learned=_time(2), assertion=corrected)
    world._cache.clear()
    assert new.id != belief.id and validate_world(world) == []
    assert current_knowledge(world, knower, StoryTime("main", 1, 1))[0]["knowledgeId"] == belief.id
    assert belief.frontmatter["claim"]["genealogy"] == parse_record(before, belief.source_path).frontmatter["claim"]["genealogy"]
    historical = _assertion(mapping)
    historical["valid"]["until"] = _time(-1)
    history = _knowledge(world, mapping, "learned history", learned=_time(5), assertion=historical)
    world._cache.clear()
    assert validate_world(world) == []
    assert history.id not in {item["knowledgeId"] for item in current_knowledge(world, knower, StoryTime("main", 4, 0))}
    retained = next(item for item in current_knowledge(world, knower, StoryTime("main", 5, 0)) if item["knowledgeId"] == history.id)
    assert retained["claim"]["genealogy"]["valid"]["until"] == _time(-1)
    assert retained["claim"]["genealogy"]["labels"] == historical["labels"]
    assert GenealogyAssertion.from_value(history.frontmatter["claim"]["genealogy"]).valid_until == StoryTime("main", -1, 0)


def test_typed_knowledge_exports_do_not_grant_author_only_provenance(tmp_path, monkeypatch):
    from wedl.context import accessible_entities, _knowledge as context_knowledge
    from wedl.compiler import fingerprint
    from wedl.profiles import resolve_profile

    world, mapping = _world_with_knowledge()
    belief = _knowledge(world, mapping, learned=_time(2, 1))
    secret_source = mapping["character_secret_parent"]
    cause = mapping["event_appointment"]
    transition = belief.frontmatter["transitions"][0]
    belief.frontmatter["transitions"].insert(0, {
        "id": id_from_seed("knowledge-transition", "rejected unseen report"),
        "time": _time(-1), "state": "rejected"})
    poison = "AUTHOR ONLY: this is the wrong father; correction is hidden"
    belief.frontmatter.update(title=poison)
    belief.source_path = "story/knowledge/hidden-correction-wrong-father.md"
    belief.frontmatter["claim"].update(key=poison, statement=poison)
    transition.update(source_entity=secret_source, causing_event=cause, note=poison, acquisition=poison)
    knower = mapping["character_child"]
    point = StoryTime("main", 2, 1)
    assert validate_world(world) == []
    typed = current_knowledge(world, knower, point)[0]
    projection = {key: value for key, value in typed.items() if key != "record"}
    assert projection["sourceEntityId"] is None and projection["causingEventId"] is None
    assert projection["note"] is None and projection["acquisition"] == "authored"
    assert projection["statement"] == "An explicitly authored genealogy assertion."
    assert projection["claimKey"] == "genealogy"
    assert poison not in str(projection) and belief.source_path not in str(projection)
    assert projection["transitionId"] == transition["id"] and projection["time"] == transition["time"]
    assert transition["source_entity"] == secret_source and transition["causing_event"] == cause
    scene = _record("scene", "viewpoint scene", status="closed",
                    time={"start": _time(0), "current": _time(2), "end": _time(3)}, participants=[{"character": knower}])
    assert secret_source not in accessible_entities(world, knower, scene, point)
    assert cause not in accessible_entities(world, knower, scene, point)
    atoms, total = context_knowledge(world, world.get(knower), point, set(), set(), set())
    assert total == 1 and len(atoms) == 1
    assert all(ref.entity_id == belief.id for ref in atoms[0].refs)
    assert poison not in atoms[0].text and belief.source_path not in atoms[0].text
    # Exercise the actual compiler/search lanes and final public citation,
    # substituting only repository storage for this literal source fixture.
    import sqlite3
    from types import SimpleNamespace
    from wedl import query
    from wedl.compiler import (_bootstrap_compiled_connection, _populate_full_compiled_connection,
                               cache_readiness, compile_world, require_database)
    from wedl.errors import CompileRequired
    from wedl.search import build_documents

    belief.frontmatter.update(aliases=["poisonalias"], domain="poisondomain", tags=["poisontag"])
    world.get(knower).body = "## Summary\nA remembered parent is a public concept.\n"
    world.get(mapping["character_alpha"]).body = "## Summary\nRemembered parent genealogy assertion.\n"
    world.get(mapping["character_beta"]).body = "## Summary\nUnrelated river forest landscape.\n"
    world.records[scene.id] = scene
    legacy_record = _knowledge(world, mapping, "Legacy report")
    legacy_record.frontmatter["claim"] = {"key": "legacyreport", "statement": "legacysearchword"}
    world._cache.clear()
    documents = build_documents(world)
    character_document = next(d for d in documents if d.entity_id == belief.id and d.audience_kind == "knowledge")
    assert character_document.valid_from == point
    assert "My remembered parent" in character_document.vector_text
    assert poison not in character_document.vector_text
    assert all(token not in character_document.vector_text for token in ("poisonalias", "poisondomain", "poisontag"))
    repository = SimpleNamespace(root=tmp_path, source_root="story", _compiled_world_cache={},
                                 last_load_stats={"mode": "literal-fixture"},
                                 resolve=lambda revision: world.revision,
                                 tree_oid=lambda revision: world.tree_oid,
                                 load_world=lambda revision, **kwargs: world,
                                 is_ancestor=lambda older, newer: False)
    database = tmp_path / ".wedl" / "world.sqlite"
    database.parent.mkdir()
    profile = resolve_profile(world, profile_name="hybrid", vector_dimensions=16)
    with sqlite3.connect(database) as connection:
        connection.row_factory = sqlite3.Row
        _bootstrap_compiled_connection(connection)
        _populate_full_compiled_connection(connection, world, repository, profile,
                                          fingerprint(world, profile), "full", {})
    connection.close()
    monkeypatch.setattr(query, "require_database", lambda *args, **kwargs: (world, database))
    arguments = {"perspective": "character", "character_id": knower, "scene_id": scene.id,
                 "tick": 2, "order": 1, "include_text": True, "limit": 100}
    for token in ("correction", "poisonalias", "poisondomain", "poisontag"):
        assert query.search_world(object(), token, mode="fts", **arguments)["results"] == []
    for mode in ("fts", "vector", "hybrid"):
        hits = query.search_world(object(), "remembered parent", mode=mode, **arguments)["results"]
        assert any(item["entityId"] == belief.id for item in hits), (mode, hits)
        hit = next(item for item in hits if item["entityId"] == belief.id)
        assert hit["citation"] == {"knowledgeId": belief.id, "transitionId": transition["id"],
                                   "time": transition["time"], "state": "suspected"}
        assert poison not in str(hits) and belief.source_path not in str(hits)
        assert all(token not in str(hits) for token in ("poisonalias", "poisondomain", "poisontag"))
    assert not any(item["entityId"] == belief.id for item in query.search_world(
        object(), "remembered parent", mode="fts", **{**arguments, "tick": 1})["results"])
    assert not any(item["entityId"] == belief.id for item in query.search_world(
        object(), "remembered parent", mode="fts", **{**arguments, "order": 0})["results"])
    assert not any(item["knowledgeId"] == belief.id for item in current_knowledge(world, knower, StoryTime("main", 2, 0)))
    author = query.search_world(object(), "correction", mode="fts", tick=2, include_text=True)["results"]
    assert any(item["entityId"] == belief.id and item["citation"]["sourcePath"] == belief.source_path for item in author)
    legacy_hit = query.search_world(object(), "legacysearchword", mode="fts", **arguments)["results"][0]
    assert legacy_hit["entityId"] == legacy_record.id and legacy_hit["citation"]["sourcePath"] == legacy_record.source_path
    assert cache_readiness(repository)["state"] == "ready"
    with sqlite3.connect(database) as connection:
        connection.execute("UPDATE revision SET compiler_fingerprint=replace(compiler_fingerprint,?,?)",
                           ("wedl-document-generation/v5", "wedl-document-generation/v4"))
    connection.close()
    readiness = cache_readiness(repository)
    assert readiness["state"] == "incompatible" and "compilerFingerprint" in readiness["incompatibleFields"]
    with pytest.raises(CompileRequired):
        require_database(repository, require_compiled=True)
    assert compile_world(repository)["status"] == "compiled"
    assert cache_readiness(repository)["state"] == "ready"
    assert query.search_world(object(), "correction", mode="fts", **arguments)["results"] == []
    world.records.pop(legacy_record.id)
    world._cache.clear()
    profile = resolve_profile(world)
    baseline = fingerprint(world, profile)
    world.world_record.frontmatter["capabilities"].remove("generational-knowledge-v1")
    assert fingerprint(world, profile) != baseline
    belief.frontmatter["claim"].pop("genealogy")
    legacy = current_knowledge(world, knower, point)[0]
    assert legacy["sourceEntityId"] == secret_source and legacy["causingEventId"] == cause
    assert legacy["claimKey"] == poison and legacy["statement"] == poison and legacy["note"] == poison
