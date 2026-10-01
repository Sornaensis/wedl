"""The v0.7 upgrade preserves authored legacy object affordances exactly."""

from __future__ import annotations

from copy import deepcopy
import gc
import hashlib
import json

import pytest

from wedl import migration
from wedl.compiler import compile_world
from wedl.model import Record, World
from wedl.query import show_entity
from wedl.repository import Repository
from wedl.source import generated_path, serialize_record, split_envelope
from wedl.validation import validate_world


def _request(repository: Repository, mode: str, key: str) -> dict:
    return {"protocol": migration.PROTOCOL, "mode": mode,
            "expectedHead": repository.head(), "idempotencyKey": key}


def _apply(repository: Repository, mode: str, key: str) -> tuple[dict, dict]:
    request = _request(repository, mode, key)
    plan = migration.preview(repository, request)
    assert plan["valid"], plan["diagnostics"]
    assert not plan["noOp"]
    bound = {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]}
    result = migration.apply(repository, bound, confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "committed"
    replay = migration.apply(repository, bound, confirmation_token_value=plan["confirmationToken"])
    assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
    return plan, result


def _add_records(repository: Repository, records: list[dict]) -> None:
    changes = {
        generated_path("story", item["kind"], item["title"], item["id"], item):
        serialize_record(item, f"# {item['title']}\n")
        for item in records
    }
    repository.commit_files(expected_head=repository.head(), files=changes, message="add affordance vectors")


def _record(schema: str, identifier: str, **fields: object) -> dict:
    return {"schema": schema, "kind": "object", "id": identifier,
            "title": identifier, "domain": "test", "status": "canonical", **fields}


def _with_frontmatter(world: World, identifier: str, **fields: object) -> World:
    records = dict(world.records)
    original = records[identifier]
    frontmatter = deepcopy(original.frontmatter)
    frontmatter.update(fields)
    records[identifier] = Record(frontmatter, original.body, original.source_path,
                                 original.raw_bytes, original.blob_oid, original.revision)
    return World(world.revision, world.tree_oid, records, world.root, world.source_root, world.is_worktree)


def test_legacy_corpus_affordances_survive_candidate_conversion(
    ash_repo: Repository, frontiersmen_repo: Repository, request: pytest.FixtureRequest,
) -> None:
    pytest_request = request
    totals = []
    tokens: set[str] = set()
    authored_rows = []
    mapped_rows = []
    package_evidence = []
    for package, repository, expected_count in (("ash", ash_repo, 24), ("frontiersmen", frontiersmen_repo, 29)):
        snapshot = repository.snapshot("HEAD")
        before = snapshot.files
        raw = migration._raw_records(snapshot)
        candidate, diagnostics, no_op = migration._v07_candidate(repository, snapshot, raw)
        assert not diagnostics and not no_op
        assert not [item for item in validate_world(
            migration._world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, candidate)
        ) if item["severity"] == "error"]
        original = {path: (frontmatter, body) for path, frontmatter, body, _data in raw}
        converted = {path: (frontmatter, body) for path, frontmatter, body in candidate}
        assert set(original) == set(converted)
        count = 0
        for path, (old, old_body) in original.items():
            new, new_body = converted[path]
            assert new_body == old_body
            assert new["schema"] == "wedl/v0.7"
            if old["kind"] == "world":
                assert new["capabilities"] == ["generational-core-v1", "spatial-core-v1"]
                continue
            old_facts = {key: value for key, value in old.items() if key != "schema"}
            new_facts = {key: value for key, value in new.items() if key != "schema"}
            if old["kind"] == "object" and "capabilities" in old_facts:
                authored = old_facts.pop("capabilities")
                assert new_facts.pop("object_affordances") == authored
                # A second source parse must preserve [] and token order too.
                round_trip, body = split_envelope(serialize_record(new, new_body), path)
                assert body == new_body and round_trip["object_affordances"] == authored
                count += 1
                tokens.update(authored)
                authored_rows.append({"package": package, "path": path.removeprefix("story/"),
                                      "capabilities": authored, "sourceSchema": old["schema"]})
            elif old["kind"] == "object":
                assert "object_affordances" not in new_facts
            assert new_facts == old_facts
        assert count == expected_count
        totals.append(count)

        request = _request(repository, "upgrade-v07", f"corpus-{package}")
        plan = migration.preview(repository, request)
        assert plan["valid"] and not plan["noOp"], plan["diagnostics"]
        assert repository.snapshot("HEAD").files == before
        bound = {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]}
        result = migration.apply(repository, bound, confirmation_token_value=plan["confirmationToken"])
        assert result["status"] == "committed" and repository.ref(plan["backupRef"]) == snapshot.revision
        replay = migration.apply(repository, bound, confirmation_token_value=plan["confirmationToken"])
        assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
        after = repository.snapshot("HEAD").files
        assert set(after) == set(before)
        world = repository.load_world()
        assert not [item for item in validate_world(world) if item["severity"] == "error"]
        for path, old_bytes in before.items():
            old, old_body = split_envelope(old_bytes, path)
            new, new_body = split_envelope(after[path], path)
            assert new_body == old_body and new["schema"] == "wedl/v0.7"
            if old["kind"] != "object":
                continue
            if "capabilities" not in old:
                assert "object_affordances" not in new
                continue
            authored = old["capabilities"]
            assert new["object_affordances"] == authored and "capabilities" not in new
            source = world.records[old["id"]].frontmatter
            assert source["object_affordances"] == authored
            detail = show_entity(repository, old["id"], require_compiled=True)["frontmatter"]
            assert detail["object_affordances"] == authored and detail == source
            mapped_rows.append({"package": package, "path": path.removeprefix("story/"),
                                "object_affordances": authored, "sourceSchema": new["schema"]})
        no_op = migration.preview(repository, _request(repository, "upgrade-v07", f"noop-{package}"))
        assert no_op["valid"] and no_op["noOp"]
        gc.collect()
        rollback = {**_request(repository, "rollback", f"reverse-{package}"),
                    "rollbackBackupRef": plan["backupRef"]}
        rollback_plan = migration.preview(repository, rollback)
        assert rollback_plan["valid"] and not rollback_plan["noOp"]
        reversed_result = migration.apply(
            repository, {**rollback, "sourceSnapshotHash": rollback_plan["sourceSnapshotHash"]},
            confirmation_token_value=rollback_plan["confirmationToken"],
        )
        assert reversed_result["status"] == "committed"
        assert repository.snapshot("HEAD").files == before
        source_material = bytearray()
        for path, data in sorted(before.items()):
            source_material.extend(path.encode("utf-8") + b"\0" + hashlib.sha256(data).digest() + b"\n")
        package_evidence.append({
            "name": package, "recordCount": len(before), "fieldCount": count,
            "sourceSha256": hashlib.sha256(source_material).hexdigest(),
            "sourceFiles": {path: hashlib.sha256(data).hexdigest() for path, data in sorted(before.items())},
            "lifecycle": {name: True for name in (
                "candidate", "preview", "apply", "replay", "sourceReload", "compiledDetail", "noOp", "rollback")},
        })
    assert sum(totals) == 53
    assert len(tokens) == 19
    assert len(mapped_rows) == 53
    digest = lambda rows: hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(",", ":"),
                                                    ensure_ascii=False).encode("utf-8")).hexdigest()
    assert digest(authored_rows) == "924318fb126571a159be87a66ff8febd76fc19c262756beda312aa3eab69b7a5"
    assert digest(mapped_rows) == "728193728c7561d41a13e20c3fc5cef0cb16eca255d46fcfc26f4920fe04ce97"
    assert sum(not row["capabilities"] for row in authored_rows) == 25
    assert sum(bool(row["capabilities"]) for row in authored_rows) == 28
    # The runner consumes this proof only after pytest reports this exact node passed.
    import os
    from pathlib import Path
    import sys

    context_path = os.environ.get("WEDL_PERFORMANCE_CONTEXT")
    if context_path:
        root = Path(__file__).resolve().parents[1]
        sys.path.insert(0, str(root / "tools"))
        from benchmark_spatial_browser import read_context, write_manifest

        context = read_context(Path(context_path))
        assert pytest_request.node.nodeid == context["affordanceNode"]
        assert hashlib.sha256(Path(__file__).read_bytes()).hexdigest() == context["codeHashes"]["tests/test_object_affordances.py"]
        proof = {
            "kind": "object-affordance-corpus", "nodeId": pytest_request.node.nodeid,
            "fieldCount": 53, "emptyFieldCount": 25, "nonemptyFieldCount": 28,
            "tokenCount": len(tokens), "tokens": sorted(tokens), "packages": package_evidence,
            "authoredRows": authored_rows, "mappedRows": mapped_rows,
            "authoredSha256": digest(authored_rows), "mappedSha256": digest(mapped_rows),
        }
        write_manifest(Path(context["results"]["object-affordance-corpus"]), proof, context)


def test_upgrade_preview_apply_replay_detail_and_rollback(task91_repo: Repository) -> None:
    repository = task91_repo
    identifiers = [f"obj_{number:026d}" for number in (91, 92, 93)]
    _add_records(repository, [
        _record("wedl/v0.3", identifiers[0]),
        _record("wedl/v0.3", identifiers[1], capabilities=[]),
        _record("wedl/v0.3", identifiers[2], capabilities=["can-be-opened", "reveals-water-marks", "can-be-opened"]),
    ])
    before = repository.snapshot("HEAD").files
    old_head = repository.head()
    request = _request(repository, "upgrade-v07", "preserve-objects")
    plan = migration.preview(repository, request)
    assert plan["valid"] and not plan["noOp"]
    assert repository.head() == old_head and repository.snapshot("HEAD").files == before
    result = migration.apply(repository, {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]},
                             confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "committed" and repository.ref(plan["backupRef"]) == old_head
    replay = migration.apply(repository, {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]},
                             confirmation_token_value=plan["confirmationToken"])
    assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
    world = repository.load_world()
    assert "object_affordances" not in world.records[identifiers[0]].frontmatter
    assert world.records[identifiers[1]].frontmatter["object_affordances"] == []
    authored = ["can-be-opened", "reveals-water-marks", "can-be-opened"]
    assert world.records[identifiers[2]].frontmatter["object_affordances"] == authored
    assert all("capabilities" not in world.records[identifier].frontmatter for identifier in identifiers)
    assert compile_world(repository)["status"] in {"compiled", "cache-hit"}
    for identifier in identifiers:
        source = world.records[identifier].frontmatter
        detail = show_entity(repository, identifier, require_compiled=True)["frontmatter"]
        assert detail == source
    gc.collect()  # Release the read-only SQLite detail handles before Windows replaces the cache.
    assert migration.preview(repository, _request(repository, "upgrade-v07", "noop"))["noOp"]
    rollback = {**_request(repository, "rollback", "reverse"), "rollbackBackupRef": plan["backupRef"]}
    rollback_plan = migration.preview(repository, rollback)
    assert rollback_plan["valid"]
    reversed_result = migration.apply(repository, {**rollback, "sourceSnapshotHash": rollback_plan["sourceSnapshotHash"]},
                                      confirmation_token_value=rollback_plan["confirmationToken"])
    assert reversed_result["status"] == "committed"
    assert repository.snapshot("HEAD").files == before


@pytest.mark.parametrize("fields,kind,field", [
    ({"object_affordances": ["legacy-mixed"], "capabilities": ["can-be-read"]}, "object", "object_affordances"),
    ({"object_affordances": ["legacy-only"]}, "object", "object_affordances"),
    ({"capabilities": "can-be-read"}, "object", "capabilities"),
    ({"capabilities": ["can-be-read"]}, "character", "capabilities"),
])
def test_legacy_upgrade_rejects_invalid_affordance_placement(
    task91_repo: Repository, fields: dict, kind: str, field: str,
) -> None:
    repository = task91_repo
    identifier = ("obj_" if kind == "object" else "char_") + "0" * 25 + "9"
    record = {**_record("wedl/v0.3", identifier), "kind": kind, **fields}
    _add_records(repository, [record])
    before = repository.snapshot("HEAD").files
    head = repository.head()
    plan = migration.preview(repository, _request(repository, "upgrade-v07", "reject"))
    assert not plan["valid"] and not plan["noOp"]
    assert any(item["code"] == "GEN-AFFORDANCE-001" and item["field"] == field for item in plan["diagnostics"])
    assert repository.head() == head and repository.snapshot("HEAD").files == before


def test_v07_validation_rejects_mixed_non_object_and_bad_values(task91_repo: Repository) -> None:
    repository = task91_repo
    identifier = "obj_" + "0" * 25 + "8"
    _add_records(repository, [_record("wedl/v0.3", identifier, capabilities=["can-be-read"])])
    _apply(repository, "upgrade-v07", "validate-shape")
    world = repository.load_world()
    assert not validate_world(world)
    for value, leaf in ((None, "object_affordances"), (["good", 7], "object_affordances[1]")):
        errors = validate_world(_with_frontmatter(world, identifier, object_affordances=value))
        assert any(item["code"] == "GEN-AFFORDANCE-001" and item["field"] == leaf for item in errors)
    errors = validate_world(_with_frontmatter(world, identifier, capabilities=["can-be-read"]))
    assert any(item["code"] == "GEN-AFFORDANCE-001" and item["field"] == "capabilities" for item in errors)
    world_id = world.world_record.id
    errors = validate_world(_with_frontmatter(world, world_id, object_affordances=["can-be-read"]))
    assert any(item["code"] == "GEN-AFFORDANCE-001" and item["field"] == "object_affordances" for item in errors)
