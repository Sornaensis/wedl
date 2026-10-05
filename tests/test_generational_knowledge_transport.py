"""Confirmed literal belief authoring and local-author POV transport parity."""
from copy import deepcopy
from dataclasses import replace
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

from fastapi.testclient import TestClient
import jsonschema
import pytest

from test_generational_compiler import _world
from test_generational_knowledge_source import _assertion
from test_generational_knowledge_source import _world_with_knowledge
from test_generational_knowledge_compiler import _repository
from test_generational_knowledge_query import _literal
from wedl.authoring import apply_intent, compile_intent, preview_intent
from wedl.api_schemas import components
from wedl.errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, StaleRevision, UsageError
from wedl.generational_api import execute_with_viewpoint
from wedl.ids import id_from_seed
from wedl.model import Record
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.validation import validate_world


def _point(tick, order=0):
    return {"timeline": "main", "tick": str(tick), "order": str(order)}


def _create(head, mapping, *, key="learn-parent"):
    assertion = _assertion(mapping)
    assertion["valid"]["from"] = _point(-10)
    assertion["labels"][mapping["character_child"]] = "Myself"
    return {"action": "generational.knowledge.create", "expectedHead": head, "idempotencyKey": key,
            "title": "AUTHOR wrong father correction", "knower": mapping["character_child"], "assertion": assertion,
            "at": _point(2, 1), "state": "suspected", "confidence": 0.6}


def test_knowledge_intents_compile_closed_history_and_explicit_opt_in():
    world, mapping = _world()
    head = "a" * 40
    repository = SimpleNamespace(head=lambda: head, load_world=lambda *args, **kwargs: world)
    optin = {"action": "generational.knowledge.opt-in", "expectedHead": head, "idempotencyKey": "opt-in"}
    caps = compile_intent(repository, optin)["operations"][0]["frontmatterPatch"]["capabilities"]
    assert caps == ["generational-core-v1", "generational-knowledge-v1"]
    assert world.world_record.frontmatter["capabilities"] == ["generational-core-v1"]
    world.world_record.frontmatter["capabilities"] = caps
    intent = _create(head, mapping)
    payload = compile_intent(repository, intent)
    frontmatter = payload["operations"][0]["value"]["frontmatter"]
    assert payload == compile_intent(repository, deepcopy(intent))
    knowledge = Record(frontmatter, "", f"story/knowledge/{frontmatter['id']}.md", b"")
    world.records[knowledge.id] = knowledge
    assert validate_world(world) == []
    original = deepcopy(frontmatter)
    state = {"action": "generational.knowledge.state", "expectedHead": head, "idempotencyKey": "reject",
             "record": knowledge.id, "at": _point(3), "state": "rejected"}
    history = compile_intent(repository, state)["operations"][0]["frontmatterPatch"]["transitions"]
    assert history[:-1] == original["transitions"] and history[-1]["state"] == "rejected"
    assert knowledge.frontmatter == original
    replacement = {"action": "generational.knowledge.replace", "expectedHead": head, "idempotencyKey": "replace",
        "record": knowledge.id, "at": _point(3), "retireState": "forgotten",
        "replacement": {"title": "New report", "assertion": intent["assertion"], "state": "accepted"}}
    replaced = compile_intent(repository, replacement)["operations"]
    assert [operation["type"] for operation in replaced] == ["entity.update", "entity.create"]
    assert replaced[1]["value"]["frontmatter"]["id"] != knowledge.id
    assert replaced[0]["frontmatterPatch"]["transitions"][:-1] == original["transitions"]
    for mutation in ({"viewer": "forged"}, {"state": []}, {"confidence": 10 ** 400}, {"at": {"timeline": "main", "tick": 2, "order": "1"}}):
        with pytest.raises(UsageError):
            compile_intent(repository, {**_create(head, mapping, key="other"), **mutation})
    with pytest.raises(UsageError):
        compile_intent(repository, {**replacement, "retireState": []})
    with pytest.raises(StaleRevision):
        compile_intent(repository, {**optin, "expectedHead": "b" * 40})


def _cli(repository, arguments, request, path, *, expected_code=0):
    path.write_text(json.dumps(request), encoding="utf-8")
    environment = os.environ.copy()
    environment["PYTHONPATH"] = str(repository.root / "src")
    result = subprocess.run([sys.executable, "-m", "wedl.cli", *arguments, str(path), "--repo", str(repository.root)],
                            cwd=repository.root, env=environment, capture_output=True, text=True, timeout=60)
    assert result.returncode == expected_code, result.stderr
    return json.loads(result.stdout if expected_code == 0 else result.stderr)


def test_viewpoint_and_knowledge_openapi_catalogue_is_closed(tmp_path):
    from wedl.api_contract import _generational_intent_examples, route_contracts
    world, mapping = _world_with_knowledge()
    world.revision = "d" * 40
    child, alpha = mapping["character_child"], mapping["character_alpha"]
    organization = next(record.id for record in world if record.kind == "organization")
    legacy = next(record.id for record in world if record.kind == "legacy")
    _literal(world, mapping, "AUTHOR parent", "parentage", {"child_id": child, "parent_id": alpha, "basis": "adoptive"},
             labels={child: "Known child", alpha: "Known parent"})
    union = _literal(world, mapping, "AUTHOR union", "union", {"participant_ids": sorted([child, alpha]), "state": "formed"})
    _literal(world, mapping, "AUTHOR organization", "organization", {"organization_id": organization, "parent_id": None}, labels={organization: "Known house"})
    _literal(world, mapping, "AUTHOR affiliation", "affiliation", {"organization_id": organization, "character_id": child, "role": "Keeper"})
    _literal(world, mapping, "AUTHOR tenure", "tenure", {"legacy_id": legacy, "holder_id": None, "basis": "legal"}, labels={legacy: "Known office"})
    _literal(world, mapping, "AUTHOR claim", "claim", {"legacy_id": legacy, "claimant_id": child, "state": "disputed"})
    _literal(world, mapping, "AUTHOR vital", "vital", {"character_id": child, "state": "living"}, accepted=True)
    assert validate_world(world) == []
    repository = _repository(world, tmp_path)
    schemas = components()["schemas"]
    envelope = {"protocol": "wedl-generational/v1", "revision": world.revision, "capabilities": world.world_record.frontmatter["capabilities"],
                "mode": "character", "timeline": "main", "at": _point(2)}
    selectors = {"parents": {"subject": "Known child"}, "ancestors": {"subject": child},
        "descendants": {"subject": alpha}, "relatives": {"subject": child, "target": alpha},
        "union": {"subject": union.id}, "organization": {"subject": "Known house"}, "legacy": {"subject": "Known office"},
        "vital": {"subject": child}, "search": {"text": "parentage"},
        "discover": {"kind": "character", "text": "Known"}, "labels": {"ids": [child, alpha]},
        "context": {"subject": child, "maxCharacters": 20000}}
    for operation, selector in selectors.items():
        body = {**envelope, "operation": operation, **selector}
        stem = "".join(part.capitalize() for part in operation.split("-"))
        request_validator = jsonschema.Draft202012Validator(schemas[f"Generational{stem}Request"])
        request_validator.validate(body)
        for field in ("viewer", "characterId", "audience", "perspective", "viewpoint"):
            forged = {**body, field: child}
            assert not request_validator.is_valid(forged)
            assert execute_with_viewpoint(repository, operation, forged, viewpoint=child)["state"] == "invalid"
        answer = execute_with_viewpoint(repository, operation, body, viewpoint=child)
        assert answer["state"] == "available", (operation, answer)
        jsonschema.Draft202012Validator(schemas[f"Generational{stem}AvailableOutcome"]).validate(answer)
        assert "AUTHOR" not in json.dumps(answer) and "story/" not in json.dumps(answer)
    for operation, subject in (("character-unions", child), ("organization-legacies", organization)):
        assert execute_with_viewpoint(repository, operation, {**envelope, "operation": operation, "subject": subject}, viewpoint=child)["state"] == "invalid"
    canonical_union = next(record.id for record in world if record.kind == "union")
    assert execute_with_viewpoint(repository, "union", {**envelope, "operation": "union", "subject": canonical_union}, viewpoint=child)["state"] == "unknown"
    request_validator = jsonschema.Draft202012Validator({**schemas["AuthoringRequest"], "components": {"schemas": schemas}})
    examples = _generational_intent_examples("/api/authoring/preview", apply=False)
    assert {example["value"]["body"]["action"] for example in examples if "knowledge" in example["value"]["body"]["action"]} == {
        "generational.knowledge.opt-in", "generational.knowledge.create", "generational.knowledge.state", "generational.knowledge.replace"}
    for example in examples:
        request_validator.validate(example["value"]["body"])
    for contract in route_contracts():
        if len(contract.command) == 2 and contract.command[0] == "generational" and contract.command[1] in selectors:
            argument = next(argument for argument in contract.arguments if argument.dest == "viewpoint")
            assert argument.option_strings == ("--viewpoint",) and argument.transport_name == "viewpoint"
    # Fold before bounding the prefix: these dotted-I/sharp-S labels expand.
    shared = "Learnt " + "ßİ" * 36
    exact_name = shared + " z-target"
    for ordinal in range(100):
        _literal(world, mapping, f"shared prefix {ordinal}", "parentage",
                 {"child_id": child, "parent_id": alpha, "basis": "biological"}, labels={alpha: shared + f" {ordinal:03d}"})
    _literal(world, mapping, "exact last-page name", "parentage",
             {"child_id": child, "parent_id": alpha, "basis": "adoptive"}, labels={child: exact_name})
    long_request = {**envelope, "operation": "parents", "subject": exact_name.swapcase(), "items": 500}
    long_repository = _repository(replace(world, _cache={}), tmp_path / "long-labels")
    answer = execute_with_viewpoint(long_repository, "parents", long_request, viewpoint=child)
    assert answer["state"] == "available" and all(row["targetId"] == alpha for row in answer["relations"])
    _literal(world, mapping, "conflicting exact label", "parentage",
             {"child_id": child, "parent_id": alpha, "basis": "biological"}, labels={alpha: exact_name})
    ambiguous_repository = _repository(replace(world, _cache={}), tmp_path / "ambiguous-labels")
    assert execute_with_viewpoint(ambiguous_repository, "parents", long_request, viewpoint=child)["state"] == "unknown"
    for ordinal in range(200):
        _literal(world, mapping, f"bounded prefix {ordinal}", "parentage",
                 {"child_id": child, "parent_id": alpha, "basis": "biological"},
                 labels={child: shared + f" child{ordinal:03d}", alpha: shared + f" parent{ordinal:03d}"})
    bounded_repository = _repository(replace(world, _cache={}), tmp_path / "bounded-labels")
    assert execute_with_viewpoint(bounded_repository, "parents", long_request, viewpoint=child)["state"] == "limit"


def test_confirmed_knowledge_authoring_and_cli_http_viewpoints(task91_repo, tmp_path, monkeypatch):
    repository = task91_repo
    world, mapping = _world()
    files = {path.relative_to(repository.root).as_posix(): None for path in (repository.root / "story").rglob("*.md")}
    files.update({("story/world.md" if record.kind == "world" else record.source_path): serialize_record(record.frontmatter, "# Authored fixture\n") for record in world})
    repository.commit_files(expected_head=repository.head(), files=files, message="seed literal knowledge transport")
    app = create_app(repository.root)
    with TestClient(app) as client:
        token = client.get("/api/session").json()["token"]
        headers = {"X-Wedl-Token": token}
        seed_head = repository.head()
        optin = {"action": "generational.knowledge.opt-in", "expectedHead": seed_head, "idempotencyKey": "knowledge-opt-in-" + seed_head}
        preview = client.post("/api/authoring/preview", headers=headers, json=optin)
        assert preview.status_code == 200 and preview.json()["preview"]["valid"] is True
        confirmation = preview.json()["preview"]["confirmationToken"]
        assert client.post("/api/authoring/apply", headers=headers, json=optin).status_code == 400
        applied = client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": confirmation}, json=optin)
        assert applied.status_code == 200, applied.text
        assert client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": confirmation}, json=optin).json()["idempotentReplay"] is True
        intent = _create(repository.head(), mapping, key="learn-parent-" + seed_head)
        preview = _cli(repository, ["author", "request", "preview"], intent, tmp_path / "create.json")
        assert preview["preview"]["valid"] is True
        knowledge_id = preview["changeset"]["operations"][0]["value"]["frontmatter"]["id"]
        confirmation = preview["preview"]["confirmationToken"]
        applied = client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": confirmation}, json=intent)
        assert applied.status_code == 200, applied.text
        result = applied.json()
        assert result["newHead"] == repository.head()
        assert client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": confirmation}, json=intent).json()["idempotentReplay"] is True
        with pytest.raises(ConfirmationMismatch):
            apply_intent(repository, intent, confirmation_token_value="wrong")
        with pytest.raises(ConflictError):
            apply_intent(repository, {**intent, "confidence": 0.1}, confirmation_token_value=confirmation)
        caps = repository.load_world().world_record.frontmatter["capabilities"]
        body = {"protocol": "wedl-generational/v1", "operation": "parents", "revision": repository.head(), "capabilities": caps,
                "mode": "character", "timeline": "main", "at": _point(2, 1), "subject": "Myself"}
        pov = mapping["character_child"]
        direct = execute_with_viewpoint(repository, "parents", body, viewpoint=pov)
        http = client.post("/api/generational/parents", params={"viewpoint": pov}, headers=headers, json=body)
        assert http.status_code == 200 and http.json() == direct
        cli = _cli(repository, ["generational", "parents", "--viewpoint", pov], body, tmp_path / "parents.json")
        assert cli == direct and direct["state"] == "available"
        assert direct["relations"][0]["targetId"] == mapping["character_alpha"]
        assert direct["relations"][0]["citations"][0]["knowledgeId"] == knowledge_id
        jsonschema.Draft202012Validator(components()["schemas"]["GenerationalParentsAvailableOutcome"]).validate(direct)
        assert "AUTHOR" not in json.dumps(direct) and "story/" not in json.dumps(direct)
        assert client.post("/api/generational/parents", headers=headers, json=body).json()["state"] == "unknown"
        assert client.post("/api/generational/parents", params={"viewpoint": pov}, json=body).status_code == 401
        assert client.post("/api/generational/parents", params={"viewpoint": pov}, headers=headers, json={**body, "characterId": pov}).status_code == 400
        before = {**body, "at": _point(2, 0)}
        assert execute_with_viewpoint(repository, "parents", before, viewpoint=pov)["state"] == "unknown"
        assert execute_with_viewpoint(repository, "parents", body, viewpoint=mapping["character_alpha"])["state"] == "unknown"
        assert execute_with_viewpoint(repository, "parents", {**body, "subject": "character_child"}, viewpoint=pov)["state"] == "unknown"
        original = deepcopy(repository.load_world().get(knowledge_id).frontmatter)
        reject = {"action": "generational.knowledge.state", "expectedHead": repository.head(),
                  "idempotencyKey": "reject-" + seed_head, "record": knowledge_id, "at": _point(3), "state": "rejected"}
        preview = client.post("/api/authoring/preview", headers=headers, json=reject).json()
        applied = client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": preview["preview"]["confirmationToken"]}, json=reject)
        assert applied.status_code == 200, applied.text
        assert execute_with_viewpoint(repository, "parents", {**body, "revision": repository.head(), "at": _point(3)}, viewpoint=pov)["state"] == "unknown"
        assert repository.load_world().get(knowledge_id).frontmatter["transitions"][:-1] == original["transitions"]
        replacement_assertion = deepcopy(intent["assertion"])
        replacement_assertion["payload"]["basis"] = "adoptive"
        replacement = {"action": "generational.knowledge.replace", "expectedHead": repository.head(),
                       "idempotencyKey": "replace-" + seed_head, "record": knowledge_id, "at": _point(4), "retireState": "forgotten",
                       "replacement": {"title": "AUTHOR revised report", "assertion": replacement_assertion, "state": "accepted"}}
        preview = client.post("/api/authoring/preview", headers=headers, json=replacement).json()
        replacement_id = preview["changeset"]["operations"][1]["value"]["frontmatter"]["id"]
        confirmation = preview["preview"]["confirmationToken"]
        applied = client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": confirmation}, json=replacement)
        assert applied.status_code == 200, applied.text
        assert client.post("/api/authoring/apply", headers={**headers, "X-Wedl-Confirmation": confirmation}, json=replacement).json()["idempotentReplay"] is True
        renewed = execute_with_viewpoint(repository, "parents", {**body, "revision": repository.head(), "at": _point(4)}, viewpoint=pov)
        assert renewed["relations"][0]["label"] == "adoptive-parent"
        assert renewed["relations"][0]["citations"][0]["knowledgeId"] == replacement_id
        old = repository.load_world().get(knowledge_id).frontmatter
        assert old["claim"] == original["claim"] and old["transitions"][-1]["state"] == "forgotten"
    # Stop the server watcher before independent CLI/direct writes to this copy.
    _assert_cli_state_and_atomic_rollback(repository, replacement_id, seed_head, replacement_assertion, tmp_path, monkeypatch)


def _assert_cli_state_and_atomic_rollback(repository, replacement_id, seed_head, replacement_assertion, tmp_path, monkeypatch):
    state = {"action": "generational.knowledge.state", "expectedHead": repository.head(),
             "idempotencyKey": "remember-" + seed_head, "record": replacement_id, "at": _point(5), "state": "remembered"}
    preview = _cli(repository, ["author", "request", "preview"], state, tmp_path / "state.json")
    confirmation = preview["preview"]["confirmationToken"]
    bypass = _cli(repository, ["author", "request", "apply", "--yes"], state, tmp_path / "bypass.json", expected_code=2)
    assert bypass["code"] == "usage_error" and "--yes cannot bypass preview confirmation" in bypass["message"]
    applied = _cli(repository, ["author", "request", "apply", "--confirm", confirmation], state, tmp_path / "state.json")
    assert applied["newHead"] == repository.head()
    assert _cli(repository, ["author", "request", "apply", "--confirm", confirmation], state, tmp_path / "state.json")["idempotentReplay"] is True
    with pytest.raises(StaleRevision):
        preview_intent(repository, {**state, "idempotencyKey": "stale-" + seed_head})
    _assert_atomic_rollback(repository, replacement_id, seed_head, replacement_assertion, monkeypatch, at=_point(5, 1))


def _assert_atomic_rollback(repository, replacement_id, seed_head, replacement_assertion, monkeypatch, *, at):
    from wedl import transaction_recovery as recovery
    source_before = deepcopy(repository.load_world().get(replacement_id).frontmatter)
    rollback = {"action": "generational.knowledge.replace", "expectedHead": repository.head(),
                "idempotencyKey": "rollback-" + seed_head, "record": replacement_id, "at": at, "retireState": "rejected",
                "replacement": {"title": "Rollback report", "assertion": replacement_assertion, "state": "suspected"}}
    confirmation = preview_intent(repository, rollback)["preview"]["confirmationToken"]
    before_head = repository.head()
    paths = [*(repository.root / "story").rglob("*.md"), repository.root / ".git/index",
             repository.root / ".wedl/world.sqlite", repository.root / ".wedl/vector-cache-v2.sqlite", repository.root / ".wedl/idempotency.json"]
    before_bytes = {path: path.read_bytes() for path in paths}
    publications = []
    def publication_failure(name):
        if name == "surface-publish-create":
            publications.append(name)
        if name == "surface-publish-create" and len(publications) == 2:
            assert repository.head() != before_head
            assert {path: path.read_bytes() for path in (repository.root / "story").rglob("*.md")} != {
                path: before for path, before in before_bytes.items() if path.suffix == ".md"}
            raise RuntimeError("owned knowledge publication failure")
    with monkeypatch.context() as patch:
        patch.setattr(recovery, "_mutation_hook", publication_failure)
        with pytest.raises(RuntimeError, match="owned knowledge publication failure"):
            apply_intent(repository, rollback, confirmation_token_value=confirmation)
    assert repository.head() == before_head
    assert len(publications) == 2
    assert set((repository.root / "story").rglob("*.md")) == {path for path in paths if path.suffix == ".md"}
    assert {path: path.read_bytes() for path in paths} == before_bytes
    assert repository.load_world().get(replacement_id).frontmatter == source_before
