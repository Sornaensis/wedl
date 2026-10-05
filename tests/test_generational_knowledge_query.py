"""Character assertion reads never borrow canonical genealogy or names."""
from dataclasses import replace
import sqlite3

from test_generational_knowledge_source import _assertion, _knowledge, _time, _world_with_knowledge
from test_generational_knowledge_compiler import _populate, _repository
from wedl.generational_api import execute
from wedl.generational_context import build_generational_context
from wedl.generational_query import PROTOCOL, TrustedViewerScope, discovery_connection, query_connection, query_generational
from wedl.compiler import compile_world
from wedl.ids import id_from_seed
from wedl.model import StoryTime
from wedl.util import canonical_json
from wedl.validation import validate_world


def _scope(world, mapping, tick=2, order=0):
    return TrustedViewerScope(world.revision, "character", "main", StoryTime("main", tick, order),
                              frozenset({"public"}), frozenset({"ordinary"}),
                              frozenset(world.world_record.frontmatter["capabilities"]), mapping["character_child"])


def test_character_parent_learning_is_literal_cited_and_trusted_api_only(tmp_path):
    world, mapping = _world_with_knowledge()
    world.revision = "a" * 40
    belief = _knowledge(world, mapping, learned=_time(2, 1))
    assert validate_world(world) == []
    scope = _scope(world, mapping, 2, 1)
    selector = {"operation": "parents", "subject_id": mapping["character_child"]}
    connection = sqlite3.connect(":memory:")
    _populate(connection, world, tmp_path)
    before = replace(scope, at=StoryTime("main", 2, 0))
    assert query_connection(connection, before, selector) == {"state": "unknown"}
    answer = query_connection(connection, scope, selector)
    assert answer["state"] == "available"
    parent = answer["relations"][0]
    assert parent["targetId"] == mapping["character_alpha"]
    assert parent["labels"] == {mapping["character_alpha"]: "My remembered parent"}
    assert parent["uncertain"] is True
    assert parent["citations"][0]["knowledgeId"] == belief.id
    assert parent["citations"][0]["transitionId"] == belief.frontmatter["transitions"][0]["id"]
    assert parent["citations"][0]["time"] == _time(2, 1)
    repository = _repository(world, tmp_path)
    assert query_generational(repository, scope, selector) == answer
    request = {"protocol": PROTOCOL, "operation": "parents", "revision": world.revision,
               "capabilities": world.world_record.frontmatter["capabilities"], "mode": "character",
               "timeline": "main", "at": {"timeline": "main", "tick": "2", "order": "1"},
               "subject": mapping["character_child"]}
    assert execute(repository, "parents", request)["state"] == "unknown"
    assert execute(repository, "parents", request, trusted_scope=before)["state"] == "invalid"
    request["at"]["order"] = "0"
    assert execute(repository, "parents", request, trusted_scope=before)["state"] == "unknown"
    request["at"]["order"] = "1"
    api = execute(repository, "parents", request, trusted_scope=scope)
    assert api["state"] == "available"
    assert api["relations"][0]["targetId"] == parent["targetId"]
    assert api["relations"][0]["citations"][0]["time"] == request["at"]
    author = replace(scope, mode="author-as-of", character_id=None,
                     audiences=frozenset({"public", "archivist"}), perspectives=frozenset({"ordinary", "council"}))
    author_request = {**request, "mode": "author-as-of"}
    author_api = execute(repository, "parents", author_request)
    assert author_api["state"] == "available"
    assert parent["targetId"] not in {value["targetId"] for value in author_api["relations"]}
    assert {value["targetId"] for value in author_api["relations"]} == {
        value["targetId"] for value in query_generational(repository, author, selector)["relations"]}
    connection.close()


def test_held_historical_names_context_and_discovery_preserve_exact_learning(tmp_path):
    world, mapping = _world_with_knowledge()
    world.revision = "b" * 40
    child, alpha, beta = (mapping["character_" + key] for key in ("child", "alpha", "beta"))
    data = _assertion(mapping)
    data["valid"]["until"] = _time(-1)
    data["labels"] = {alpha: "Remembered Alpha", child: "Remembered Child"}
    belief = _knowledge(world, mapping, "AUTHOR wrong father secret", learned=_time(5, 1), assertion=data)
    belief.frontmatter.update(aliases=["AUTHOR alias"], tags=["AUTHOR tag"])
    belief.frontmatter["claim"].update(key="AUTHOR correction key", statement="AUTHOR correction prose")
    for tick, state in ((0, "rejected"), (6, "forgotten"), (7, "remembered"), (8, "rejected")):
        belief.frontmatter["transitions"].append({"id": id_from_seed("knowledge-transition", f"historical-{tick}"), "time": _time(tick), "state": state})
    belief.frontmatter["transitions"].sort(key=lambda value: (value["time"]["tick"], value["time"]["order"]))
    prior = _knowledge(world, mapping, "prior evidence", learned=_time(-4))
    prior.frontmatter["claim"] = {"key": "report", "statement": "AUTHOR external prose"}
    belief.frontmatter["claim"]["genealogy"]["evidence"] = [{"kind": "knowledge", "entity_id": prior.id, "transition_id": prior.frontmatter["transitions"][0]["id"]}]
    second = _literal(world, mapping, "another learned name", "parentage", {"child_id": child, "parent_id": beta, "basis": "adoptive"}, labels={beta: "Remembered Beta"}, until=_time(-1), learned=_time(5, 1))
    unicode = _literal(world, mapping, "unicode name", "parentage", {"child_id": child, "parent_id": beta, "basis": "adoptive"}, labels={beta: "𐐀 Memory"}, until=_time(-1), learned=_time(5, 1))
    assert validate_world(world) == []
    connection = sqlite3.connect(":memory:")
    _populate(connection, world, tmp_path)
    scope = _scope(world, mapping, 5, 1)
    parent_selector = {"operation": "parents", "subject_id": child}
    labels = {"operation": "labels", "ids": [alpha, child]}
    before = replace(scope, at=StoryTime("main", 5, 0))
    assert discovery_connection(connection, before, labels) == {"state": "available", "labels": []}
    assert query_connection(connection, scope, parent_selector) == {"state": "unknown"}
    names = discovery_connection(connection, scope, labels)
    assert {value["title"] for value in names["labels"]} == {"Remembered Alpha", "Remembered Child"}
    citation = names["labels"][0]["citations"][0]
    assert citation["learnedAt"] == _time(5, 1)
    assert citation["evidence"] == [{"kind": "knowledge", "entityId": prior.id, "itemId": prior.frontmatter["transitions"][0]["id"], "time": _time(-4)}]
    discover = {"operation": "discover", "kind": "character", "text": "Remembered", "items": 1}
    pages = []
    page = discovery_connection(connection, scope, discover)
    while True:
        pages.extend(page["results"])
        if page["cursor"] is None:
            break
        continuation = {**discover, "cursor": page["cursor"]}
        assert discovery_connection(connection, replace(scope, character_id=alpha), continuation)["state"] == "invalid"
        assert discovery_connection(connection, replace(scope, at=StoryTime("main", 5, 2)), continuation)["state"] == "invalid"
        assert discovery_connection(connection, replace(scope, revision="c" * 40), continuation)["state"] == "invalid"
        page = discovery_connection(connection, scope, continuation)
    assert {value["title"] for value in pages} == {"Remembered Alpha", "Remembered Beta", "Remembered Child"}
    assert discovery_connection(connection, scope, {"operation": "discover", "kind": "character", "text": "𐐨"})["results"][0]["title"] == "𐐀 Memory"
    for tick in (6, 8):
        retired = replace(scope, at=StoryTime("main", tick, 0))
        assert discovery_connection(connection, retired, labels)["labels"] == []
        history = query_connection(connection, retired, {"operation": "knowledge-history", "subject_id": child})
        assert belief.id not in canonical_json(history)
    assert discovery_connection(connection, replace(scope, at=StoryTime("main", 7, 0)), labels)["labels"]
    repository = _repository(world, tmp_path)
    context = build_generational_context(repository, scope, child, max_characters=20000)
    historical = next(item["result"] for item in context["items"] if item["kind"] == "learned-history")
    assertion = next(value for value in historical["assertions"] if value["knowledgeId"] == belief.id)
    assert assertion["applicable"] is False and assertion["labels"] == data["labels"]
    assert all(item["kind"] not in {"parents", "ancestors", "descendants"} for item in context["items"])
    assert "AUTHOR" not in canonical_json(context) and "story/" not in canonical_json(context)
    request = {"protocol": PROTOCOL, "operation": "discover", "revision": world.revision,
               "capabilities": world.world_record.frontmatter["capabilities"], "mode": "character", "timeline": "main",
               "at": {"timeline": "main", "tick": "5", "order": "1"}, "kind": "character", "text": "Remembered", "items": 1}
    api = execute(repository, "discover", request, trusted_scope=scope)
    assert api["state"] == "available" and len(api["results"]) == 1 and api["cursor"]
    request.update(operation="labels", ids=[alpha])
    request.pop("kind")
    request.pop("text")
    assert execute(repository, "labels", request, trusted_scope=scope)["labels"][0]["title"] == "Remembered Alpha"
    request.update(operation="context", subject=child, maxCharacters=20000)
    request.pop("ids")
    request["items"] = 20
    api_context = execute(repository, "context", request, trusted_scope=scope)
    assert api_context["state"] == "available" and len(canonical_json(api_context)) <= 20000
    request["maxCharacters"] = 180
    bounded = execute(repository, "context", request, trusted_scope=scope)
    assert bounded["state"] in {"available", "limit"}
    if bounded["state"] == "available":
        assert bounded["truncated"] and len(canonical_json(bounded)) <= 180
    connection.close()


def test_character_scope_search_and_unlearned_mutations_stay_closed(tmp_path):
    world, mapping = _world_with_knowledge()
    world.revision = "d" * 40
    child, alpha = mapping["character_child"], mapping["character_alpha"]
    belief = _knowledge(world, mapping, "AUTHOR secret correction", learned=_time(3))
    scope = _scope(world, mapping, 2)
    selectors = [{"operation": operation, "subject_id": child} for operation in ("parents", "ancestors", "descendants", "vital", "knowledge-history")]
    selectors += [{"operation": "search", "text": "AUTHOR"}, {"operation": "relatives", "subject_id": child, "target_id": alpha}]
    connection = sqlite3.connect(":memory:")
    _populate(connection, world, tmp_path)
    observed = []
    connection.set_trace_callback(observed.append)
    baseline = [query_connection(connection, scope, selector) for selector in selectors]
    assert all(value["state"] in {"unknown", "available"} for value in baseline)
    assert all(not value.get("results") for value in baseline)
    assert not any("generational_record" in sql or "generational_discovery_name" in sql for sql in observed)
    other = _knowledge(world, mapping, "unheard poisoned title", learned=_time(-3))
    other.frontmatter["knower"] = alpha
    connection2 = sqlite3.connect(":memory:")
    _populate(connection2, world, tmp_path)
    assert [query_connection(connection2, scope, selector) for selector in selectors] == baseline
    learned = replace(scope, at=StoryTime("main", 3, 0))
    assert query_connection(connection2, learned, {"operation": "search", "text": "Remembered"})["results"][0]["knowledgeId"] == belief.id
    assert query_connection(connection2, learned, {"operation": "search", "text": "AUTHOR"})["results"] == []
    repository = _repository(world, tmp_path)
    request = {"protocol": PROTOCOL, "operation": "parents", "revision": world.revision,
               "capabilities": world.world_record.frontmatter["capabilities"], "mode": "character", "timeline": "main",
               "at": {"timeline": "main", "tick": "3", "order": "0"}, "subject": child}
    for changes in ({"revision": "e" * 40}, {"timeline": "other"}, {"character_id": None}, {"mode": "author-all-time"}, {"capabilities": frozenset({"generational-core-v1"})}):
        assert execute(repository, "parents", request, trusted_scope=replace(learned, **changes))["state"] == "invalid"
    assert execute(repository, "parents", {**request, "characterId": child}, trusted_scope=learned)["state"] == "invalid"
    assert execute(repository, "parents", request)["state"] == "unknown"
    assert execute(repository, "parents", {**request, "subject": "My remembered parent"}, trusted_scope=learned)["state"] == "invalid"
    assert query_connection(connection2, replace(learned, mode="author-all-time", at=None, character_id=None), {"operation": "knowledge-history", "subject_id": child})["state"] == "invalid"
    assert query_generational(repository, replace(learned, revision="e" * 40), selectors[0])["state"] == "invalid"
    connection.close()
    connection2.close()


def _literal(world, mapping, name, kind, payload, *, labels=None, learned=None, until=None, accepted=False):
    data = {"kind": kind, "payload": payload, "valid": {"from": _time(-10)}, "labels": labels or {}}
    if until is not None:
        data["valid"]["until"] = until
    record = _knowledge(world, mapping, name, assertion=data, learned=learned)
    if accepted:
        record.frontmatter["transitions"][0].update(state="accepted", confidence=1)
    return record


def test_character_paths_are_complete_literal_uncertain_and_cycle_bounded(tmp_path):
    world, mapping = _world_with_knowledge()
    child, alpha, beta, gamma, secret = (mapping["character_" + key] for key in ("child", "alpha", "beta", "gamma", "secret_parent"))
    first = _literal(world, mapping, "wrong father", "parentage", {"child_id": child, "parent_id": alpha, "basis": "biological"})
    second = _literal(world, mapping, "next parent", "parentage", {"child_id": alpha, "parent_id": beta, "basis": "adoptive"}, accepted=True)
    _literal(world, mapping, "cycle", "parentage", {"child_id": beta, "parent_id": child, "basis": "biological"})
    _literal(world, mapping, "conflicting report", "parentage", {"child_id": child, "parent_id": gamma, "basis": "adoptive"})
    future = _literal(world, mapping, "future bridge", "parentage", {"child_id": gamma, "parent_id": secret, "basis": "biological"}, learned=_time(5))
    other = _literal(world, mapping, "unheard bridge", "parentage", {"child_id": beta, "parent_id": secret, "basis": "biological"})
    other.frontmatter["knower"] = alpha
    assert validate_world(world) == []
    connection = sqlite3.connect(":memory:")
    _populate(connection, world, tmp_path)
    scope = _scope(world, mapping)
    def query(operation, subject=child, **fields):
        return query_connection(connection, scope, {"operation": operation, "subject_id": subject, **fields})
    answer = query("ancestors")
    assert {value["targetId"] for value in answer["relations"]} == {alpha, beta, gamma}
    path = next(value for value in answer["relations"] if value["targetId"] == beta)
    assert [(edge["from"], edge["to"], edge["recordId"]) for edge in path["edges"]] == [(child, alpha, first.id), (alpha, beta, second.id)]
    assert [edge["basis"] for edge in path["edges"]] == ["biological", "adoptive"]
    assert path["uncertain"] and path["edges"][0]["uncertain"] and not path["edges"][1]["uncertain"]
    assert query("relatives", target_id=secret) == {"state": "unknown"}
    relatives = query("relatives", subject=beta, target_id=alpha)
    assert relatives["state"] == "available"
    assert relatives["relations"][0]["edges"][0]["from"] == beta
    assert relatives["relations"][0]["edges"][-1]["to"] == alpha
    assert query("descendants", subject=alpha)["state"] == "available"
    for fields in ({"depth": 1}, {"depth": 0}, {"items": 2}):
        assert query("ancestors", **fields) == {"state": "limit", "code": "GEN-LIMIT-001"}
    assert query("parents", items=1) == {"state": "limit", "code": "GEN-LIMIT-001"}
    assert future.id not in canonical_json(answer) and other.id not in canonical_json(answer)
    repository = _repository(world, tmp_path)
    selector = {"operation": "ancestors", "subject_id": child}
    assert query_generational(repository, scope, selector) == answer
    compile_world(repository, world.revision, force=True)
    assert query_generational(repository, scope, selector, require_compiled=True) == answer
    connection.close()


def test_character_union_roles_holders_claims_and_vital_remain_literal(tmp_path):
    world, mapping = _world_with_knowledge()
    child, alpha, beta = (mapping["character_" + key] for key in ("child", "alpha", "beta"))
    organization, parent = [record.id for record in world if record.kind == "organization"][:2]
    legacy = next(record.id for record in world if record.kind == "legacy")
    union = _literal(world, mapping, "author-only wrong union", "union", {"participant_ids": sorted([child, alpha, beta]), "state": "formed"})
    org = _literal(world, mapping, "author-only wrong containment", "organization", {"organization_id": organization, "parent_id": parent})
    _literal(world, mapping, "root containment", "organization", {"organization_id": parent, "parent_id": None})
    role = _literal(world, mapping, "author-only affiliation", "affiliation", {"organization_id": organization, "character_id": child, "role": "Keeper"})
    for name, holder, basis in (("legal holder one", child, "legal"), ("legal holder two", alpha, "legal"), ("vacant defacto", None, "de-facto")):
        _literal(world, mapping, name, "tenure", {"legacy_id": legacy, "holder_id": holder, "basis": basis})
    claim = _literal(world, mapping, "author-only claim", "claim", {"legacy_id": legacy, "claimant_id": beta, "state": "disputed"})
    _literal(world, mapping, "living belief", "vital", {"character_id": child, "state": "living"}, accepted=True)
    _literal(world, mapping, "death belief", "vital", {"character_id": child, "state": "dead"})
    assert validate_world(world) == []
    connection = sqlite3.connect(":memory:")
    _populate(connection, world, tmp_path)
    scope = _scope(world, mapping)
    def query(operation, subject, **fields):
        return query_connection(connection, scope, {"operation": operation, "subject_id": subject, **fields})
    union_answer = query("union", union.id)
    assert union_answer["participants"] == sorted([child, alpha, beta]) and union_answer["unionState"] == "formed"
    assert query("union", union.id, items=2) == {"state": "limit", "code": "GEN-LIMIT-001"}
    canonical_union = next(record.id for record in world if record.kind == "union")
    assert query("union", canonical_union)["state"] == "invalid"
    org_answer = query("organization", organization)
    assert org_answer["assertions"][0]["knowledgeId"] == org.id
    assert org_answer["roles"][0]["value"]["role"] == "Keeper"
    assert org_answer["roles"][0]["knowledgeId"] == role.id
    assert org_answer["parentPath"][0]["targetId"] == parent
    legacy_answer = query("legacy", legacy)
    assert {row["value"]["holder_id"] for row in legacy_answer["holders"]} == {child, alpha}
    assert legacy_answer["vacancies"][0]["value"] == {"legacy_id": legacy, "holder_id": None, "basis": "de-facto"}
    assert legacy_answer["claims"][0]["knowledgeId"] == claim.id
    assert legacy_answer["claims"][0]["value"]["claimant_id"] == beta
    assert legacy_answer["succession"] == []
    assert query("legacy", legacy, items=3)["state"] == "limit"
    vital = query("vital", child)
    assert vital["vital"] is None and vital["conflicting"] and vital["uncertain"]
    assert {row["value"]["state"] for row in vital["assertions"]} == {"living", "dead"}
    assert query("character-unions", child)["state"] == "invalid"
    assert query("organization-legacies", organization)["state"] == "invalid"
    encoded = canonical_json([union_answer, org_answer, legacy_answer, vital])
    assert "author-only" not in encoded and "story/" not in encoded and "sourcePath" not in encoded
    connection.close()
