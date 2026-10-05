from __future__ import annotations

from copy import deepcopy
import hashlib
from pathlib import Path

import pytest

from wedl import authoring, changeset, event_consequences
from wedl.errors import ConfirmationMismatch, ConflictError, StaleRevision, UsageError
from wedl.ids import id_from_seed
from wedl.model import World
from wedl.repository import Repository


def _world():
    source = Repository(Path("src/wedl/data/ash_archive_v07")).load_world("WORKTREE", cache_write=False)
    records = deepcopy(source.records)
    for record in records.values():
        record.blob_oid = hashlib.sha1(b"blob " + str(len(record.raw_bytes)).encode() + b"\0" + record.raw_bytes).hexdigest()
        record.revision = "a" * 40
    return World("a" * 40, source.tree_oid, records, source.root, source.source_root)


class ReadRepository:
    def __init__(self, world): self.world, self.loads, self.head_reads = world, [], 0
    def head(self): self.head_reads += 1; return self.world.revision
    def load_world(self, revision, *, cache_write=True):
        assert revision == self.world.revision and cache_write is False
        self.loads.append((revision, cache_write))
        return self.world


def _request(world, operations=None, *, tick="300", limit=1000):
    if operations is None:
        operations = [{"type": "event.create", "temporaryId": "tmp:rescue", "title": "Rescue", "time": {"timeline": "main", "tick": 300, "order": 0},
            "effects": [{"target": world.find("Brass Restricted-Vault Key").id, "key": "condition", "operation": "set", "value": "open"}]}]
    return {"protocol": "wedl-changeset/v1", "expectedHead": world.revision, "idempotencyKey": "pure-preview", "summary": "Explicit rescue",
        "operations": operations, "consequenceRequest": {"protocol": "wedl-event-consequence-delta/v1", "at": {"timeline": "main", "tick": tick, "order": "0"}, "limit": limit}}


def _records(report):
    return report["unattributedRecordChanges"] + [value for group in report["eventGroups"] for value in group["recordChanges"]]


def _snapshot(root):
    return {path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in root.rglob("*") if path.is_file()}


def _forbid_writes(monkeypatch):
    def forbidden(*args, **kwargs): raise AssertionError("semantic preview attempted a persistent write or compile")
    for name in ("compile_world", "compile_world_bytes", "atomic_write"):
        monkeypatch.setattr(changeset, name, forbidden)
    import wedl.query as query
    import wedl.compiler as compiler
    monkeypatch.setattr(query, "require_database", forbidden)
    monkeypatch.setattr(compiler, "require_database", forbidden)
    monkeypatch.setattr(Repository, "recover_authoring_transactions", forbidden)


def test_semantic_preview_uses_one_base_final_candidate_and_original_hash(monkeypatch):
    world = _world(); repository = ReadRepository(world); payload = _request(world)
    original = deepcopy(payload); before = deepcopy(world.records); cache = deepcopy(world._cache)
    calls = []; projector = event_consequences.semantic_delta
    def observed(base, candidate, scope, at, identity, **options):
        calls.append((base, candidate, identity, options, scope))
        cache = deepcopy(candidate._cache)
        result = projector(base, candidate, scope, at, identity, **options)
        assert candidate._cache == cache
        return result
    monkeypatch.setattr(event_consequences, "semantic_delta", observed)
    _forbid_writes(monkeypatch)
    plan = changeset.preview(repository, payload)
    delta = plan["semanticDelta"]
    assert plan["valid"] and delta["outcome"] == "ok", delta
    assert repository.head_reads == 1 and repository.loads == [(world.revision, False)]
    assert len(calls) == 1 and calls[0][0].records is world.records and calls[0][1] is not calls[0][0]
    assert calls[0][2].request_hash == plan["requestHash"]
    assert calls[0][4].complete_families == frozenset({"state", "knowledge"})
    assert calls[0][4].record_ids == frozenset(world.records) | frozenset(calls[0][1].records)
    created = plan["generatedIds"]["tmp:rescue"]
    assert created in calls[0][1].records and created not in world.records
    assert calls[0][3]["operation_targets"] == {created: [0]}
    assert delta["candidate"] == {"baseRevision": world.revision, "requestHash": changeset._request_hash(original)}
    assert delta["candidate"]["requestHash"] == plan["requestHash"]
    assert payload == original
    assert world.records == before
    assert world._cache == cache, {"beforeKeys": list(cache), "afterKeys": list(world._cache)}
    assert plan["confirmationToken"] == changeset.confirmation_token(original, request_hash=plan["requestHash"], expected_head=world.revision)


def test_metadata_retains_every_noop_index_and_reciprocal_outcome_target():
    world = _world(); plot = world.find("Open the Restricted Vault", "story-point")
    scene = deepcopy(world.by_kind("scene")[0].frontmatter)
    scene.update(id=id_from_seed("scene", "preview-scene"), title="Preview scene", status="closed", participants=[], objects=[], environments=[], conversations=[], story_points=[], outcome_events=[], observations=[])
    scene["time"] = {"start": {"timeline": "main", "tick": 0, "order": 0}, "current": {"timeline": "main", "tick": 300, "order": 0}, "end": {"timeline": "main", "tick": 400, "order": 0}}
    operations = _request(world)["operations"] + [{"type": "entity.create", "value": {"frontmatter": scene}},
        {"type": "outcome.link", "event": "tmp:rescue", "storyPoints": [plot.id], "scenes": [scene["id"]]},
        {"type": "outcome.link", "event": "tmp:rescue", "storyPoints": [plot.id], "scenes": [scene["id"]]},
        {"type": "entity.update", "entity": world.find("Brass Restricted-Vault Key").id, "frontmatterPatch": {}},
        {"type": "entity.update", "entity": world.find("Brass Restricted-Vault Key").id, "frontmatterPatch": {}}]
    plan = changeset.preview(ReadRepository(world), _request(world, operations))
    assert plan["valid"] and plan["semanticDelta"]["outcome"] == "ok", plan["diagnostics"]
    indexes = {item["recordId"]: item["operationIndexes"] for item in _records(plan["semanticDelta"])}
    event = plan["generatedIds"]["tmp:rescue"]
    assert indexes[event] == [0, 2, 3] and indexes[plot.id] == [2, 3] and indexes[scene["id"]] == [1, 2, 3]
    key = world.find("Brass Restricted-Vault Key").id
    assert indexes[key] == [4, 5]
    noop = next(item for item in _records(plan["semanticDelta"]) if item["recordId"] == key)
    assert noop["change"] == "noop" and noop["sections"] == []


def test_closed_request_failures_and_invalid_candidate_never_fold(monkeypatch):
    world = _world(); _forbid_writes(monkeypatch)
    def forbidden(*args, **kwargs): raise AssertionError("invalid semantic input was folded")
    monkeypatch.setattr(event_consequences, "Projection", forbidden)
    import wedl.consequence_delta as delta
    monkeypatch.setattr(delta, "Projection", forbidden)
    request = _request(world)
    mutations = [lambda value: value.update(mode="author-as-of"), lambda value: value.update(scope={}),
        lambda value: value.update(operationTargets={}), lambda value: value.update(focusEvents=[]),
        lambda value: value.pop("at"), lambda value: value.update(limit=True), lambda value: value.update(limit=0),
        lambda value: value.update(limit=1001), lambda value: value.update(protocol="other"),
        lambda value: value["at"].update(tick=300), lambda value: value["at"].update(tick="-0"),
        lambda value: value["at"].update(order=str(2**31)), lambda value: value["at"].update(timeline="unknown")]
    for mutate in mutations:
        payload = deepcopy(request); mutate(payload["consequenceRequest"])
        plan = changeset.preview(ReadRepository(world), payload)
        assert plan["valid"] and plan["semanticDelta"] == {"protocol": "wedl-event-consequence-delta/v1", "outcome": "invalid", "code": "CONSEQUENCE-REQUEST-001", "message": "Invalid consequence request."}
    invalid = _request(world, [{"type": "entity.update", "entity": world.find("Brass Restricted-Vault Key").id, "frontmatterPatch": {"kind": "unsupported"}}])
    plan = changeset.preview(ReadRepository(world), invalid)
    assert not plan["valid"] and any(item["severity"] == "error" for item in plan["diagnostics"])
    assert plan["semanticDelta"]["outcome"] == "unavailable"


def test_explicit_horizon_limits_and_optional_request_bind_source_identity():
    world = _world(); request = _request(world)
    plan = changeset.preview(ReadRepository(world), request)
    earlier = deepcopy(request); earlier["consequenceRequest"]["at"]["tick"] = "299"
    excluded = changeset.preview(ReadRepository(world), earlier)
    assert excluded["semanticDelta"]["outcome"] == "ok" and excluded["semanticDelta"]["eventGroups"] == [], excluded["semanticDelta"]
    assert plan["requestHash"] != excluded["requestHash"] and plan["confirmationToken"] != excluded["confirmationToken"]
    limited = changeset.preview(ReadRepository(world), _request(world, limit=1))
    assert limited["valid"] and limited["semanticDelta"]["outcome"] == "limit"
    assert set(limited["semanticDelta"]) == {"protocol", "outcome", "code", "message"}
    extrema_world = deepcopy(world)
    # This fixture's plots explicitly depend on future evidence at the lower
    # extremum. Keep the honest unavailable result and verify exact codec use.
    for tick in (str(-(2**63)), str(2**63 - 1)):
        extreme = changeset.preview(ReadRepository(extrema_world), _request(extrema_world, tick=tick))
        if tick.startswith("-"):
            assert extreme["semanticDelta"]["outcome"] == "unavailable"
        else:
            assert extreme["semanticDelta"]["outcome"] == "ok" and extreme["semanticDelta"]["at"]["tick"] == tick, extreme["semanticDelta"]
    legacy = deepcopy(request); legacy.pop("consequenceRequest")
    class LegacyRepository(ReadRepository):
        def load_world(self, revision, *, cache_write=True):
            assert cache_write is True
            return self.world
    assert "semanticDelta" not in changeset.preview(LegacyRepository(world), legacy)


def test_named_batch_and_raw_preview_share_ids_delta_but_keep_intent_confirmation():
    world = _world(); raw = _request(world)
    batch = {"action": "consequence.batch", "expectedHead": world.revision, "idempotencyKey": raw["idempotencyKey"], "summary": raw["summary"],
        "operations": deepcopy(raw["operations"]), "consequenceRequest": deepcopy(raw["consequenceRequest"])}
    batch["operations"][0]["time"]["tick"] = "300"; batch["operations"][0]["time"]["order"] = "0"
    batch["operations"][0]["effects"][0]["target"] = "Brass Restricted-Vault Key"
    repository = ReadRepository(world)
    before_cache = deepcopy(world._cache)
    authored = authoring.preview_intent(repository, batch)
    direct = changeset.preview(ReadRepository(world), authored["changeset"])
    assert repository.loads == [(world.revision, False), (world.revision, False)]
    assert world._cache == before_cache
    for field in ("requestHash", "generatedIds", "semanticDelta", "diagnostics", "diff"):
        assert authored["preview"][field] == direct[field]
    assert authored["preview"]["confirmationToken"] != direct["confirmationToken"]
    assert "_changes" not in authored["preview"] and authored["authorImpact"]["items"] == []
    changed = deepcopy(batch); changed["operations"][0]["effects"][0]["target"] = world.find("Brass Restricted-Vault Key").id
    equivalent = authoring.preview_intent(ReadRepository(world), changed)
    assert equivalent["changeset"] == authored["changeset"]
    assert equivalent["preview"]["semanticDelta"] == authored["preview"]["semanticDelta"]
    assert equivalent["preview"]["confirmationToken"] != authored["preview"]["confirmationToken"]
    assert world._cache == before_cache
    invalid = deepcopy(batch); invalid["operations"][0]["effects"][0]["target"] = "tmp:missing"
    with pytest.raises(UsageError): authoring.preview_intent(ReadRepository(world), invalid)
    assert world._cache == before_cache


def test_head_drift_does_not_substitute_loaded_revision(monkeypatch):
    world = _world(); repository = ReadRepository(world)
    original = repository.load_world
    def moved(revision, **options):
        result = original(revision, **options)
        repository.head = lambda: "b" * 40
        return result
    repository.load_world = moved
    plan = changeset.preview(repository, _request(world))
    assert plan["expectedHead"] == plan["semanticDelta"]["baseRevision"] == world.revision
    with pytest.raises(StaleRevision): changeset.preview(repository, _request(world))
    assert len(repository.loads) == 1


def test_semantic_success_failure_preserves_files_and_parser_cache(tmp_path, monkeypatch):
    root = tmp_path / "snapshot"; root.mkdir()
    for name in ("source.md", "index.db", "idempotency.json", "pending-transaction"):
        (root / name).write_text("unchanged", encoding="utf-8")
    world = _world(); world._cache["sentinel"] = {"literal": [1, 2]}
    before, cache = _snapshot(root), deepcopy(world._cache)
    repository = ReadRepository(world); _forbid_writes(monkeypatch)
    for request in (_request(world), _request(world, limit=1), _request(world, tick="-0")):
        changeset.preview(repository, request)
        assert _snapshot(root) == before and world._cache == cache


def test_actual_repository_semantic_preview_is_pure_and_apply_rechecks_identity(ash_repo, monkeypatch):
    world = ash_repo.load_world(cache_write=False)
    payload = _request(world)
    before = _snapshot(ash_repo.root)
    with monkeypatch.context() as patch:
        _forbid_writes(patch)
        plan = changeset.preview(ash_repo, payload)
        limited = changeset.preview(ash_repo, _request(world, limit=1))
    assert plan["valid"] and plan["semanticDelta"]["outcome"] == "ok"
    candidate_identity = {"baseRevision": world.revision, "requestHash": changeset._request_hash(payload)}
    assert plan["semanticDelta"]["candidate"] == candidate_identity
    provenances = []
    def inspect(value):
        if isinstance(value, dict):
            if "provenance" in value and "recordId" in value:
                provenance = value["provenance"]; provenances.append(provenance["kind"])
                if provenance["kind"] == "source":
                    assert provenance == {"kind": "source", "revision": world.revision, "blobOid": world.get(value["recordId"]).blob_oid}
                    assert len(provenance["blobOid"]) == 40
                else:
                    assert provenance == {"kind": "candidate", **candidate_identity}
            for member in value.values(): inspect(member)
        elif isinstance(value, list):
            for member in value: inspect(member)
    inspect(plan["semanticDelta"])
    assert "source" in provenances and "candidate" in provenances
    assert limited["semanticDelta"]["outcome"] == "limit" and _snapshot(ash_repo.root) == before
    changed = deepcopy(payload); changed["consequenceRequest"]["at"]["tick"] = "301"
    with pytest.raises(ConfirmationMismatch): changeset.apply(ash_repo, changed, confirmation_token_value=plan["confirmationToken"])
    assert _snapshot(ash_repo.root) == before
    result = changeset.apply(ash_repo, payload, confirmation_token_value=plan["confirmationToken"])
    def forbidden(*args, **kwargs): raise AssertionError("receipt replay resolved a new HEAD")
    monkeypatch.setattr(ash_repo, "load_world", forbidden)
    replay = changeset.apply(ash_repo, payload, confirmation_token_value=plan["confirmationToken"])
    assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
    with pytest.raises(ConflictError): changeset.apply(ash_repo, changed, confirmation_token_value=plan["confirmationToken"])
    stale = deepcopy(payload); stale["idempotencyKey"] = "fresh-stale-key"
    with pytest.raises(StaleRevision): changeset.apply(ash_repo, stale, confirmation_token_value=plan["confirmationToken"])
