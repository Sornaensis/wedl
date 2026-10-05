"""Literal character belief compilation, replay, and cache compatibility."""
from contextlib import closing
from copy import deepcopy
import json
from pathlib import Path
import sqlite3
from types import SimpleNamespace

import pytest

from test_generational_knowledge_source import _assertion, _knowledge, _record, _time, _world_with_knowledge
from wedl.compiler import (_bootstrap_compiled_connection, _populate_full_compiled_connection,
                           cache_readiness, compile_world, fingerprint, require_database)
from wedl.errors import CompileRequired
from wedl.generational import GENERATIONAL_KINDS
from wedl.generational_index import fold_knowledge_assertion
from wedl.ids import id_from_seed
from wedl.model import StoryTime
from wedl.profiles import resolve_profile
from wedl.semantics import current_knowledge
from wedl.validation import validate_world


def _repository(world, root):
    return SimpleNamespace(root=root, source_root="story", _compiled_world_cache={},
                           last_load_stats={"mode": "literal-fixture"},
                           resolve=lambda revision: world.revision,
                           tree_oid=lambda revision: world.tree_oid,
                           load_world=lambda revision, **kwargs: world,
                           is_ancestor=lambda older, newer: False)


def _populate(connection, world, root):
    connection.row_factory = sqlite3.Row
    profile = resolve_profile(world, profile_name="fts")
    _bootstrap_compiled_connection(connection)
    _populate_full_compiled_connection(connection, world, _repository(world, root), profile,
                                      fingerprint(world, profile), "full", {})


def test_literal_assertions_replay_learning_applicability_and_neutral_evidence(tmp_path):
    world, mapping = _world_with_knowledge()
    knower = mapping["character_child"]
    belief = _knowledge(world, mapping, learned=_time(1, 1))
    belief.frontmatter["transitions"][0]["state"] = "accepted"
    learning_id = belief.frontmatter["transitions"][0]["id"]
    for tick, state in ((0, "rejected"), (2, "rejected"), (3, "forgotten"), (4, "remembered")):
        belief.frontmatter["transitions"].append({"id": id_from_seed("knowledge-transition", f"state-{tick}"),
                                               "time": _time(tick), "state": state})
    belief.frontmatter["transitions"].sort(key=lambda item: (item["time"]["tick"], item["time"]["order"]))
    belief.frontmatter["title"] = "AUTHOR correction: wrong father"
    belief.frontmatter["claim"].update(key="author correction", statement="hidden correction")
    belief.frontmatter["transitions"][0].update(note="author secret", source_entity=mapping["character_secret_parent"])
    prior = _knowledge(world, mapping, "heard report", learned=_time(-2))
    prior.frontmatter["claim"] = {"key": "legacy", "statement": "A report"}
    belief.frontmatter["claim"]["genealogy"]["evidence"] = [{
        "kind": "knowledge", "entity_id": prior.id,
        "transition_id": prior.frontmatter["transitions"][0]["id"]}]
    observation_id = id_from_seed("observation", "cited observation")
    scene = _record("scene", "author-only scene title", status="closed",
                    time={"start": _time(-4), "current": _time(-2), "end": _time(-1)},
                    participants=[{"character": knower}],
                    observations=[{"id": observation_id, "at": _time(-3, 1), "until": _time(-2),
                                   "text": "author-only observation prose", "audience": [knower]}])
    turn_id = id_from_seed("conversation-turn", "cited spoken turn")
    recollection_id = id_from_seed("conversation-recollection", "cited recollection")
    conversation = _record("conversation", "author-only conversation title", status="closed",
                           time={"start": _time(-4), "end": _time(-1)},
                           participants=[{"character": knower}, {"character": mapping["character_alpha"]}],
                           turns=[{"id": turn_id, "speaker": mapping["character_alpha"], "at": _time(-3),
                                   "text": "author-only spoken prose", "audience": [knower]}],
                           recollections=[{"id": recollection_id, "character": knower, "at": _time(0),
                                          "state": "remembered", "exact_turns": [turn_id]}])
    world.records[scene.id] = scene
    world.records[conversation.id] = conversation
    belief.frontmatter["claim"]["genealogy"]["evidence"].extend([
        {"kind": "observation", "entity_id": scene.id, "observation_id": observation_id},
        {"kind": "turn", "entity_id": conversation.id, "turn_id": turn_id},
        {"kind": "recollection", "entity_id": conversation.id, "recollection_id": recollection_id},
    ])
    historical = _assertion(mapping)
    historical["valid"]["until"] = _time(-1)
    history = _knowledge(world, mapping, "learned historical report", learned=_time(5), assertion=historical)
    reverse = _assertion(mapping)
    reverse["payload"].update(child_id=mapping["character_alpha"], parent_id=knower)
    reverse.pop("labels")
    _knowledge(world, mapping, "cyclic wrong report", assertion=reverse)
    corrected = _assertion(mapping)
    corrected["payload"]["parent_id"] = mapping["character_biological"]
    corrected.pop("labels")
    _knowledge(world, mapping, "later correction", learned=_time(2), assertion=corrected)
    organization = next(record.id for record in world if record.kind == "organization")
    legacy = next(record.id for record in world if record.kind == "legacy")
    payloads = {
        "union": {"participant_ids": sorted([knower, mapping["character_alpha"]]), "state": "formed"},
        "organization": {"organization_id": organization, "parent_id": None},
        "affiliation": {"character_id": knower, "organization_id": organization, "role": "Keeper"},
        "tenure": {"legacy_id": legacy, "holder_id": None, "basis": "de-facto"},
        "claim": {"legacy_id": legacy, "claimant_id": knower, "state": "disputed"},
        "vital": {"character_id": knower, "state": "dead"},
    }
    other = [_knowledge(world, mapping, kind, assertion={"kind": kind, "payload": payload,
                                                       "valid": {"from": _time(0)}})
             for kind, payload in payloads.items()]
    other[-1].frontmatter["transitions"][0]["state"] = "uncertain"
    assert validate_world(world) == []
    snapshots = []
    for rebuild in range(2):
        with closing(sqlite3.connect(":memory:")) as connection:
            _populate(connection, world, tmp_path)
            assert connection.execute("PRAGMA foreign_key_check").fetchone() is None
            assert connection.execute("PRAGMA quick_check").fetchone()[0] == "ok"
            assert connection.execute("SELECT COUNT(*) FROM generational_knowledge_assertion WHERE knowledge_id=?", (prior.id,)).fetchone()[0] == 0
            assert connection.execute("SELECT COUNT(*) FROM knowledge_transition WHERE knowledge_id=?", (belief.id,)).fetchone()[0] == 5
            for point in (StoryTime("main", 0, 0), StoryTime("main", 1, 0), StoryTime("main", 1, 1),
                          StoryTime("main", 2, 0), StoryTime("main", 3, 0), StoryTime("main", 4, 0)):
                source = next((item for item in current_knowledge(world, knower, point) if item["knowledgeId"] == belief.id), None)
                compiled = fold_knowledge_assertion(connection, belief.id, knower_id=knower, at=point)
                assert (source is None) == (compiled is None)
                if compiled is not None:
                    assert compiled["state"] == source["state"]
                    assert compiled["payload"] == source["claim"]["genealogy"]["payload"]
                    assert compiled["labels"] == source["claim"]["genealogy"]["labels"]
                    assert compiled["citation"]["transitionId"] == source["transitionId"]
                    assert compiled["citation"]["time"] == source["time"]
                    assert compiled["affirmativeEdge"] == (point.tick != 2)
                    assert "author" not in json.dumps(compiled).casefold() and belief.source_path not in json.dumps(compiled)
            forgotten = fold_knowledge_assertion(connection, belief.id, knower_id=knower,
                                                 at=StoryTime("main", 3, 0), include_forgotten=True)
            assert forgotten["state"] == "forgotten" and not forgotten["held"] and not forgotten["affirmativeEdge"]
            assert fold_knowledge_assertion(connection, belief.id, knower_id=mapping["character_alpha"], at=StoryTime("main", 5, 0)) is None
            assert fold_knowledge_assertion(connection, belief.id, knower_id=knower, at=StoryTime("other", 5, 0)) is None
            assert fold_knowledge_assertion(connection, history.id, knower_id=knower, at=StoryTime("main", 4, 0)) is None
            retained = fold_knowledge_assertion(connection, history.id, knower_id=knower, at=StoryTime("main", 5, 0))
            assert retained["held"] and not retained["applicable"] and not retained["affirmativeEdge"]
            assert retained["labels"] == historical["labels"] and retained["learnedAt"] == _time(5)
            report = fold_knowledge_assertion(connection, belief.id, knower_id=knower, at=StoryTime("main", 4, 0))
            assert report["learnedAt"] == _time(1, 1)
            assert report["learningTransitionId"] == learning_id
            assert report["evidence"] == [
                {"kind": "knowledge", "entityId": prior.id, "itemId": prior.frontmatter["transitions"][0]["id"], "time": _time(-2)},
                {"kind": "observation", "entityId": scene.id, "itemId": observation_id, "time": _time(-3, 1)},
                {"kind": "turn", "entityId": conversation.id, "itemId": turn_id, "time": _time(-3)},
                {"kind": "recollection", "entityId": conversation.id, "itemId": recollection_id, "time": _time(0)},
            ]
            for record in other:
                folded = fold_knowledge_assertion(connection, record.id, knower_id=knower, at=StoryTime("main", 2, 0))
                assert folded["payload"] == record.frontmatter["claim"]["genealogy"]["payload"]
                assert folded["state"] == record.frontmatter["transitions"][0]["state"]
            union = next(record for record in other if record.frontmatter["claim"]["genealogy"]["kind"] == "union")
            assert [row[0] for row in connection.execute("SELECT entity_id FROM generational_knowledge_endpoint WHERE knowledge_id=? ORDER BY ordinal", (union.id,))] == sorted([knower, mapping["character_alpha"]])
            snapshots.append({table: [tuple(row) for row in connection.execute(f"SELECT * FROM {table} ORDER BY 1,2")]
                              for table in ("generational_knowledge_assertion", "generational_knowledge_endpoint", "generational_knowledge_evidence", "knowledge_transition")})
    assert snapshots[0] == snapshots[1]


def test_knowledge_only_cache_rejects_old_missing_stale_and_damaged_projection(tmp_path):
    world, mapping = _world_with_knowledge()
    world.records = {key: record for key, record in world.records.items() if record.kind not in GENERATIONAL_KINDS}
    belief = _knowledge(world, mapping)
    assert validate_world(world) == []
    repository = _repository(world, tmp_path)
    report = compile_world(repository, profile_name="fts")
    assert report["status"] == "compiled"
    database = Path(report["database"])
    assert cache_readiness(repository)["state"] == "ready"
    world_from_cache, _ = require_database(repository, require_compiled=True)
    assert world_from_cache.get(belief.id).frontmatter == belief.frontmatter
    with closing(sqlite3.connect(database)) as connection:
        assert connection.execute("SELECT COUNT(*) FROM generational_record").fetchone()[0] == 0
        assert connection.execute("SELECT COUNT(*) FROM generational_knowledge_assertion").fetchone()[0] == 1
    for defect, expected in (("old", "compilerFingerprint"),
                             ("missing", "missingTable:generational_knowledge_evidence"),
                             ("shape", "tableShape:generational_knowledge_assertion"),
                             ("index", "indexShape:generational_knowledge_endpoint_idx")):
        with closing(sqlite3.connect(database)) as connection:
            if defect == "old":
                connection.execute("UPDATE revision SET compiler_fingerprint=replace(compiler_fingerprint,?,?)",
                                   ("wedl-generational-index/v9", "wedl-generational-index/v8"))
            elif defect == "missing":
                connection.execute("DROP TABLE generational_knowledge_evidence")
            elif defect == "shape":
                connection.execute("ALTER TABLE generational_knowledge_assertion RENAME COLUMN labels_json TO author_labels")
            else:
                connection.executescript("DROP INDEX generational_knowledge_endpoint_idx; CREATE INDEX generational_knowledge_endpoint_idx ON generational_knowledge_endpoint(knowledge_id,entity_id);")
            connection.commit()
        readiness = cache_readiness(repository)
        assert readiness["state"] == "incompatible" and expected in readiness["incompatibleFields"]
        with pytest.raises(CompileRequired):
            require_database(repository, require_compiled=True)
        assert compile_world(repository)["status"] == "compiled"
        assert cache_readiness(repository)["state"] == "ready"
        with closing(sqlite3.connect(database)) as connection:
            connection.row_factory = sqlite3.Row
            assert fold_knowledge_assertion(connection, belief.id, knower_id=mapping["character_child"], at=StoryTime("main", 2, 0))["payload"] == belief.frontmatter["claim"]["genealogy"]["payload"]
    with closing(sqlite3.connect(database)) as connection:
        connection.execute("UPDATE revision SET head_commit='old-revision'")
        connection.commit()
    assert cache_readiness(repository)["state"] == "stale"
    with pytest.raises(CompileRequired):
        require_database(repository, require_compiled=True)
    assert compile_world(repository)["status"] == "compiled"
    assert cache_readiness(repository)["state"] == "ready"
    with closing(sqlite3.connect(database)) as connection:
        connection.execute("PRAGMA foreign_keys=OFF")
        connection.execute("UPDATE generational_knowledge_endpoint SET entity_id='missing-character'")
        connection.commit()
    assert "foreignKeyIntegrity" in cache_readiness(repository)["incompatibleFields"]
    with pytest.raises(CompileRequired):
        require_database(repository, require_compiled=True)
    assert compile_world(repository)["status"] == "compiled"
    database.unlink()
    assert cache_readiness(repository)["state"] == "missing"
    with pytest.raises(CompileRequired):
        require_database(repository, require_compiled=True)
    assert compile_world(repository)["status"] == "compiled"
    assert cache_readiness(repository)["state"] == "ready"
