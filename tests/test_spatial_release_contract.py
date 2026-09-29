"""Cross-seam source, transport, and migration compatibility at the v0.7 boundary."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
import yaml
from fastapi.testclient import TestClient

from test_spatial_api import CAPABILITIES, _spatial_seed, spatial_repository
from wedl import migration
from wedl.cli import main
from wedl.compiler import compile_world, require_database
from wedl.errors import StaleRevision
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import serialize_record, split_envelope
from wedl.source import generated_path
from wedl.spatial_api import PROTOCOL, execute, status_code
from wedl.validation import validate_world


FIXTURE = Path(__file__).parent / "fixtures" / "spatial_release" / "matrix.yaml"
GRAPH = FIXTURE.with_name("graph.yaml")


def _matrix() -> dict:
    return yaml.safe_load(FIXTURE.read_text(encoding="utf-8"))


def _source_facts(repository: Repository) -> dict[str, tuple]:
    world = repository.load_world()
    return {
        record.id: (record.frontmatter.get("kind"), record.frontmatter.get("parent_id"),
                    record.frontmatter.get("spatial"), record.body)
        for record in world.records.values()
    }


def _migration_request(repository: Repository, mode: str, key: str) -> dict:
    return {"protocol": migration.PROTOCOL, "mode": mode,
            "expectedHead": repository.head(), "idempotencyKey": key}


def _upgrade(repository: Repository, mode: str, key: str) -> tuple[dict, dict]:
    request = _migration_request(repository, mode, key)
    before = repository.snapshot("HEAD").files
    head = repository.head()
    plan = migration.preview(repository, request)
    assert repository.head() == head and repository.snapshot("HEAD").files == before
    assert plan["valid"] and plan["expectedHead"] == head and plan["sourceSnapshotHash"]
    bound = {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]}
    result = migration.apply(repository, bound, confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "committed" and result["previousHead"] == head
    assert repository.ref(plan["backupRef"]) == head
    replay = migration.apply(repository, bound, confirmation_token_value=plan["confirmationToken"])
    assert replay["newHead"] == result["newHead"] and replay["idempotentReplay"] is True
    return plan, result


def test_authored_facts_match_compiled_cli_and_http(
    spatial_repository: Repository, tmp_path: Path, capsys: pytest.CaptureFixture[str],
) -> None:
    matrix = _matrix()
    world = spatial_repository.load_world()
    assert world.revision == spatial_repository.head()
    assert {map_id: {key: world.records[map_id].frontmatter[key] for key in ("crs", "unit")}
            for map_id in matrix["source"]["maps"]} == matrix["source"]["maps"]
    for identifier in matrix["source"]["coordinate_free"]:
        assert world.records[identifier].frontmatter.get("spatial") is None
    relation = matrix["source"]["authored_parent"]
    assert world.records[relation["child"]].frontmatter["parent_id"] == relation["parent"]
    compiled, database = require_database(spatial_repository, require_compiled=True)
    assert compiled.revision == world.revision and database.is_file()
    source_before = _source_facts(spatial_repository)
    observed = []
    with TestClient(create_app(spatial_repository.root)) as client:
        for index, case in enumerate(matrix["transport"]):
            operation = case["operation"]
            request = {"protocol": PROTOCOL, "revision": world.revision,
                       "capabilities": CAPABILITIES, "limit": 10, "cursor": None,
                       **case["fields"]}
            direct = execute(spatial_repository, operation, request, require_compiled=True)
            assert direct["state"] == case["state"]
            assert execute(spatial_repository, operation, request, require_compiled=True) == direct
            assert direct["revision"] == world.revision
            assert direct["cache"]["sourceSchema"] == "wedl/v0.7"
            assert direct["cache"]["treeOid"] == world.tree_oid
            assert str(spatial_repository.root) not in json.dumps(direct)
            observed.append((operation, request, direct))
            path = tmp_path / f"request-{index}.json"
            path.write_text(json.dumps(request), encoding="utf-8")
            exit_code = main(["--compact", "spatial", operation, str(path),
                              "--repo", str(spatial_repository.root), "--require-compiled"])
            captured = capsys.readouterr()
            assert exit_code == (0 if direct["state"] == "ok" else 2)
            assert json.loads(captured.out if exit_code == 0 else captured.err) == direct
            response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
            assert response.status_code == status_code(direct) and response.json() == direct
            if operation == "children" and request["limit"] == 1:
                cursor = direct["result"]["nextCursor"]
                assert cursor
                page_request = {**request, "cursor": cursor}
                next_page = execute(spatial_repository, operation, page_request, require_compiled=True)
                path.write_text(json.dumps(page_request), encoding="utf-8")
                assert main(["--compact", "spatial", operation, str(path),
                             "--repo", str(spatial_repository.root), "--require-compiled"]) == 0
                assert json.loads(capsys.readouterr().out) == next_page
                page_response = client.post(f"/api/spatial/{operation}?requireCompiled=true",
                                            json=page_request)
                assert page_response.status_code == status_code(next_page)
                assert page_response.json() == next_page
            if operation == "overlay-as-of" and request["audience"] == "public":
                assert direct["result"]["ids"] == []
                control = execute(spatial_repository, operation,
                                  {**request, "locationId": relation["parent"]},
                                  require_compiled=True)
                assert control["state"] == "ok" and direct == control
            if operation == "overlay-as-of" and request["audience"] == "author" and request["asOf"]["tick"] == "0":
                authored = world.records["overlay:ward"].frontmatter
                assert request["locationId"] in authored["membership"]["location_ids"]
                assert "author" in authored["audience"]
                assert authored["valid"]["start"]["tick"] <= 0 < authored["valid"]["end"]["tick"]
                assert direct["result"]["ids"] == ["overlay:ward"]
            if operation == "overlay-as-of" and request["asOf"]["tick"] == "2":
                assert direct["result"]["ids"] == []
            if operation == "path" and direct["state"] == "ok":
                assert direct["result"]["routeIds"] == ["route:place-gate"]
                assert direct["result"]["metric"]["computedTotal"] >= 0
            if operation == "adjacency" and request["locationId"] == "location:gate":
                assert "portal:gate-place" in direct["result"]["portalIds"]
            if operation == "bbox" and direct["state"] == "ok":
                expected_map = request["mapId"]
                minimum, maximum = request["bounds"]["min"], request["bounds"]["max"]
                authored_ids = {
                    record.id for record in world.records.values()
                    if record.kind == "location" and (spatial := record.frontmatter.get("spatial"))
                    and spatial["map_id"] == expected_map
                    and spatial["geometry"]["kind"] == "point"
                    and all(low <= coordinate <= high for low, coordinate, high in
                            zip(minimum, spatial["geometry"]["coordinates"], maximum))
                }
                assert set(direct["result"]["ids"]) == authored_ids
                assert not set(matrix["source"]["coordinate_free"]).intersection(direct["result"]["ids"])
            if operation == "children" and request["limit"] == 10 and direct["state"] == "ok":
                authored_children = {record.id for record in world.records.values()
                                     if record.kind == "location"
                                     and record.frontmatter.get("parent_id") == request["locationId"]}
                assert set(direct["result"]["ids"]) == authored_children
    database.unlink()
    assert compile_world(spatial_repository)["status"] == "compiled"
    rebuilt_world, rebuilt_database = require_database(spatial_repository, require_compiled=True)
    assert (rebuilt_world.revision, rebuilt_world.tree_oid) == (world.revision, world.tree_oid)
    assert rebuilt_database == database
    with TestClient(create_app(spatial_repository.root)) as client:
        for index, (operation, request, original) in enumerate(observed):
            rebuilt = execute(spatial_repository, operation, request, require_compiled=True)
            assert rebuilt == original
            path = tmp_path / f"rebuilt-{index}.json"
            path.write_text(json.dumps(request), encoding="utf-8")
            exit_code = main(["--compact", "spatial", operation, str(path),
                              "--repo", str(spatial_repository.root), "--require-compiled"])
            captured = capsys.readouterr()
            assert exit_code == (0 if original["state"] == "ok" else 2)
            assert json.loads(captured.out if exit_code == 0 else captured.err) == original
            response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
            assert response.status_code == status_code(original) and response.json() == original
    assert _source_facts(spatial_repository) == source_before


@pytest.mark.parametrize("schema", ["wedl/v0.3", "wedl/v0.5", "wedl/v0.6"])
def test_legacy_upgrade_is_bound_lossless_and_reversible(
    task91_repo: Repository, schema: str,
) -> None:
    repository = task91_repo
    if schema in {"wedl/v0.5", "wedl/v0.6"}:
        _upgrade(repository, "upgrade-v03", "prepare-v05")
    if schema == "wedl/v0.6":
        _upgrade(repository, "upgrade-v06", "prepare-v06")
    root_id = "loc_00000000000000000000000098"
    child_id = "loc_00000000000000000000000099"
    legacy_records = [
        {"schema": schema, "kind": "location", "id": root_id, "title": "Legacy root",
         "domain": "setting.release", "status": "canonical", "links": [child_id]},
        {"schema": schema, "kind": "location", "id": child_id, "title": "Legacy child",
         "domain": "setting.release", "status": "canonical", "parent": root_id, "links": []},
    ]
    source_changes = {
        generated_path("story", record["kind"], record["title"], record["id"], record):
        serialize_record(record, f"# {record['title']}\nNo authored coordinates.\n")
        for record in legacy_records
    }
    repository.commit_files(expected_head=repository.head(), files=source_changes,
                            message=f"add coordinate-free {schema} release places")
    before = repository.snapshot("HEAD").files
    assert {yaml.safe_load(data.decode("utf-8").split("---", 2)[1])["schema"]
            for data in before.values()} == {schema}
    source_head = repository.head()
    plan, result = _upgrade(repository, "upgrade-v07", f"release-{schema}")
    after = repository.snapshot("HEAD").files
    assert set(after) == set(before)
    for path in before:
        old, old_body = split_envelope(before[path], path)
        new, new_body = split_envelope(after[path], path)
        assert new_body == old_body and new["schema"] == "wedl/v0.7"
        old_facts = {key: value for key, value in old.items() if key != "schema" and (old["kind"] != "world" or key != "capabilities")}
        new_facts = {key: value for key, value in new.items() if key != "schema" and (new["kind"] != "world" or key != "capabilities")}
        if old["kind"] == "object" and "capabilities" in old_facts:
            assert new_facts.pop("object_affordances") == old_facts.pop("capabilities")
        if "parent" in old_facts:
            assert new_facts.pop("parent_id") == old_facts.pop("parent")
        assert new_facts == old_facts
    migrated = repository.load_world()
    assert migrated.records[root_id].frontmatter["links"] == [child_id]
    assert migrated.records[child_id].frontmatter["parent_id"] == root_id
    assert all(migrated.records[record_id].frontmatter.get("spatial") is None
               for record_id in (root_id, child_id))
    assert result["targetCapabilities"] == ["generational-core-v1", "spatial-core-v1"]
    assert compile_world(repository)["status"] == "cache-hit"
    child_request = {"protocol": PROTOCOL, "revision": migrated.revision,
                     "capabilities": result["targetCapabilities"], "limit": 10, "cursor": None,
                     "locationId": root_id}
    authored_children = {record.id for record in migrated.records.values()
                         if record.kind == "location"
                         and record.frontmatter.get("parent_id") == root_id}
    before_rebuild = execute(repository, "children", child_request, require_compiled=True)
    assert before_rebuild["state"] == "ok"
    assert before_rebuild["cache"]["treeOid"] == migrated.tree_oid
    assert set(before_rebuild["result"]["ids"]) == authored_children == {child_id}
    cache = repository.root / ".wedl" / "world.sqlite"
    cache.unlink()
    assert compile_world(repository)["status"] == "compiled"
    assert execute(repository, "children", child_request, require_compiled=True) == before_rebuild
    no_op = migration.preview(repository, _migration_request(repository, "upgrade-v07", "noop"))
    assert no_op["valid"] and no_op["noOp"]
    with pytest.raises(StaleRevision):
        migration.preview(repository, {**_migration_request(repository, "upgrade-v07", "stale"),
                                       "expectedHead": source_head})
    rollback = _migration_request(repository, "rollback", "reverse")
    rollback["rollbackBackupRef"] = plan["backupRef"]
    rollback_plan = migration.preview(repository, rollback)
    assert rollback_plan["valid"]
    reversed_result = migration.apply(
        repository, {**rollback, "sourceSnapshotHash": rollback_plan["sourceSnapshotHash"]},
        confirmation_token_value=rollback_plan["confirmationToken"],
    )
    assert reversed_result["status"] == "committed"
    assert repository.snapshot("HEAD").files == before
    assert repository.head() != source_head


def test_mixed_legacy_source_is_rejected_without_side_effects(task91_repo: Repository) -> None:
    repository = task91_repo
    foreign = {"schema": "wedl/v0.5", "kind": "location", "id": "location:mixed",
               "title": "Mixed place"}
    repository.commit_files(expected_head=repository.head(),
                            files={"story/locations/mixed.md": serialize_record(foreign, "# Mixed place\n")},
                            message="mixed source fixture")
    before = repository.snapshot("HEAD").files
    head = repository.head()
    plan = migration.preview(repository, _migration_request(repository, "upgrade-v07", "mixed"))
    assert plan["valid"] is False and plan["files"] == []
    assert any(item["code"] == "GEN-VERSION-001" for item in plan["diagnostics"])
    assert repository.head() == head and repository.snapshot("HEAD").files == before


def test_authored_deep_wide_cycle_and_unknown_costs(spatial_repository: Repository) -> None:
    graph = yaml.safe_load(GRAPH.read_text(encoding="utf-8"))
    records = []
    for item in graph["deep_parent_chain"]:
        records.append({"schema": "wedl/v0.7", "kind": "location", "id": item["id"],
                        "title": item["id"], "parent_id": item["parent"]})
    for index in range(graph["wide_siblings"]):
        records.append({"schema": "wedl/v0.7", "kind": "location",
                        "id": f"location:release-wide-{index}",
                        "title": graph["hostile_title"] if index == 0 else f"Wide {index}",
                        "parent_id": "loc_00000000000000000000000000"})
    for route in graph["routes"]:
        record = {"schema": "wedl/v0.7", "kind": "route", "id": route["id"],
                  "title": route["id"], "from_location_id": route["from"],
                  "to_location_id": route["to"], "direction": route["direction"],
                  "modes": route["modes"]}
        if "distance" in route:
            record["route_distance"] = {"value": route["distance"], "unit": "pace"}
        records.append(record)
    changes = {}
    for record in records:
        path = generated_path("story", record["kind"], record["title"], record["id"], record)
        changes[path] = serialize_record(record, f"# {record['title']}\nHostile <img src='https://invalid.example/track'> stays prose.\n")
    spatial_repository.commit_files(expected_head=spatial_repository.head(), files=changes,
                                    message="authored release graph fixture")
    assert not [issue for issue in validate_world(spatial_repository.load_world())
                if issue["severity"] == "error"]
    assert compile_world(spatial_repository)["status"] == "compiled"
    world = spatial_repository.load_world()
    assert all(world.records[record["id"]].frontmatter.get("spatial") is None
               for record in records if record["kind"] == "location")
    request = {"protocol": PROTOCOL, "revision": world.revision,
               "capabilities": CAPABILITIES, "limit": 100, "cursor": None}
    def query(operation: str, **fields: object) -> dict:
        return execute(spatial_repository, operation, {**request, **fields}, require_compiled=True)
    leaf = graph["deep_parent_chain"][-1]["id"]
    chain = query("containment", locationId=leaf)
    assert chain["state"] == "ok" and chain["result"]["ids"][-4:] == [
        item["id"] for item in graph["deep_parent_chain"]]
    children = query("children", locationId="loc_00000000000000000000000000")
    assert children["state"] == "ok"
    assert {f"location:release-wide-{index}" for index in range(graph["wide_siblings"])}.issubset(
        children["result"]["ids"])
    reciprocal = query("adjacency", locationId="loc_00000000000000000000000000")
    assert reciprocal["state"] == "ok"
    assert "location:side" in reciprocal["result"]["targetLocationIds"]
    competing = query("path", fromLocationId="location:gate", toLocationId="location:side", metric="routeDistance")
    assert competing["state"] == "ok"
    assert competing["result"]["metric"]["computedTotal"] == 2
    assert query("path", fromLocationId="location:gate", toLocationId="location:side", metric="routeDistance") == competing
    cycle = query("path", fromLocationId="location:gate", toLocationId="loc_00000000000000000000000000", metric="routeDistance")
    assert cycle["state"] == "ok" and cycle["result"]["metric"]["computedTotal"] >= 0
    assert query("path", fromLocationId="location:gate", toLocationId="loc_00000000000000000000000000", metric="routeDistance") == cycle
    unknown = query("path", fromLocationId=leaf, toLocationId="location:side", metric="routeDistance")
    assert unknown["state"] in {"ok", "unavailable"}
    if unknown["state"] == "ok":
        assert unknown["result"]["metric"]["complete"] is False
    no_travel_cost = query("path", fromLocationId="loc_00000000000000000000000000",
                           toLocationId="location:gate", metric="travelCost")
    assert no_travel_cost["state"] in {"ok", "unavailable"}
    if no_travel_cost["state"] == "ok":
        assert no_travel_cost["result"]["metric"]["complete"] is False
    with TestClient(create_app(spatial_repository.root)) as client:
        response = client.post("/api/spatial/children?requireCompiled=true", json={
            **request, "locationId": "loc_00000000000000000000000000"})
        assert response.status_code == status_code(children) and response.json() == children
        assert "Hostile <img" not in json.dumps(children)
