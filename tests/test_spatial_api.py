"""Public spatial reads use one strict wire contract across codec, CLI and HTTP."""

from __future__ import annotations

import json
from contextlib import closing
from copy import deepcopy
from dataclasses import fields, is_dataclass
from pathlib import Path

import jsonschema
import pytest
import yaml
from fastapi.testclient import TestClient

from wedl.api_schemas import components
from wedl.cli import main
from wedl.compiler import compile_world, connect, require_database
from wedl.errors import UsageError
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import generated_path, serialize_record
from wedl.spatial_api import PROTOCOL, _request, execute, status_code
from wedl.spatial_query import BoundingBox, MapPosition, SpatialStore
from wedl.model import StoryTime


ROOT = Path(__file__).resolve().parents[1]
REVISION = "a" * 40
CAPABILITIES = ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"]


def _base(**fields: object) -> dict[str, object]:
    return {"protocol": PROTOCOL, "revision": REVISION, "capabilities": CAPABILITIES, "limit": 10, "cursor": None, **fields}


REQUESTS = {
    "containment": {"locationId": "location:gate"},
    "children": {"locationId": "loc_00000000000000000000000000"},
    "bbox": {"mapId": "map:town", "bounds": {"min": [-1, -1], "max": [1, 1]}, "relation": "within"},
    "nearby": {"position": {"mapId": "map:town", "coordinates": [0, 0]}, "radius": 2},
    "adjacency": {"locationId": "loc_00000000000000000000000000"},
    "reachability": {"fromLocationId": "loc_00000000000000000000000000"},
    "path": {"fromLocationId": "loc_00000000000000000000000000", "toLocationId": "location:gate", "metric": "routeDistance"},
    "overlay-as-of": {"queryScope": "location", "locationId": "location:gate", "audience": "author", "perspective": "author", "asOf": {"timeline": "main", "tick": "0", "order": "0"}},
}


def _store_wire(value: object) -> object:
    """Independent dataclass-to-wire projection for complete store parity."""
    if isinstance(value, StoryTime):
        return {"timeline": value.timeline, "tick": str(value.tick), "order": str(value.order)}
    if is_dataclass(value):
        result = {}
        for field in fields(value):
            key = "".join(part.capitalize() if index else part for index, part in enumerate(field.name.split("_")))
            if field.name == "cursor":
                key = "nextCursor"
            elif type(value).__name__ == "MetricSummary" and field.name == "value":
                key = "computedTotal"
            item = getattr(value, field.name)
            if field.name == "metric" and item in {"route_distance", "travel_cost"}:
                item = {"route_distance": "routeDistance", "travel_cost": "travelCost"}[item]
            result[key] = _store_wire(item)
        return result
    if isinstance(value, dict):
        result = {}
        for key, item in value.items():
            public = "".join(part.capitalize() if index else part for index, part in enumerate(key.split("_")))
            if key == "horizon" and isinstance(item, dict) and set(item) == {"timeline", "tick", "order"}:
                item = {"timeline": item["timeline"], "tick": str(item["tick"]), "order": str(item["order"])}
            result[public] = _store_wire(item)
        return result
    if isinstance(value, (tuple, list)):
        return [_store_wire(item) for item in value]
    return value


@pytest.fixture()
def spatial_repository(ash_repo: Repository) -> Repository:
    fixture = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text(encoding="utf-8"))
    authored_route = deepcopy(fixture["route"])
    authored_route["route_distance"] = {"value": 1, "unit": "pace"}
    records = [fixture["world"], *fixture["maps"], *fixture["locations"], fixture["anchor"], fixture["portal"], authored_route, fixture["overlay"],
        {"schema": "wedl/v0.7", "kind": "location", "id": "location:side", "title": "Side", "parent_id": "loc_00000000000000000000000000", "spatial": {"map_id": "map:town", "geometry": {"kind": "point", "coordinates": [1, 1]}}},
        {"schema": "wedl/v0.7", "kind": "location", "id": "location:far", "title": "Far", "parent_id": "loc_00000000000000000000000000"},
        {"schema": "wedl/v0.7", "kind": "location", "id": "location:earth", "title": "Earth place", "parent_id": "loc_00000000000000000000000000", "spatial": {"map_id": "map:earth", "geometry": {"kind": "point", "coordinates": [12, 55]}}},
        {"schema": "wedl/v0.7", "kind": "route", "id": "route:gate-side", "title": "Gate to side", "from_location_id": "location:gate", "to_location_id": "location:side", "direction": "one-way", "modes": ["foot"], "route_distance": {"value": 2, "unit": "pace"}},
        {"schema": "wedl/v0.7", "kind": "route", "id": "route:side-far", "title": "Side to far", "from_location_id": "location:side", "to_location_id": "location:far", "direction": "one-way", "modes": ["foot"], "availability": "closed", "route_distance": {"value": 3, "unit": "pace"}},
    ]
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    for record in records:
        path = generated_path("story", record["kind"], record["title"], record["id"], record)
        changes[path] = serialize_record(record, f"# {record['title']}\n")
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed spatial transport fixture")
    repository = Repository(ash_repo.root)
    assert compile_world(repository, "HEAD")["status"] == "compiled"
    return repository


def _revision(repository: Repository) -> str:
    return repository.load_world().revision


def test_codec_cli_http_share_one_exact_spatial_outcome(spatial_repository: Repository, tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    compiled_world, database = require_database(spatial_repository, require_compiled=True)
    with closing(connect(database, True)) as connection:
        store = SpatialStore(connection, compiled_world.revision)
        direct_reads = {
            "containment": store.containment("location:gate"),
            "children": store.children("loc_00000000000000000000000000"),
            "bbox": store.bbox("map:town", BoundingBox((-1, -1), (1, 1)), relation="within"),
            "nearby": store.nearby(MapPosition("map:town", (0, 0)), radius=2),
            "adjacency": store.adjacency("loc_00000000000000000000000000"),
            "reachability": store.reachability("loc_00000000000000000000000000"),
            "path": store.path("loc_00000000000000000000000000", "location:gate", metric="route_distance"),
            "overlay-as-of": store.overlay_as_of("location:gate", StoryTime("main", 0, 0), audience="author", perspective="author"),
        }
    with TestClient(create_app(spatial_repository.root)) as client:
        for operation, fields in REQUESTS.items():
            request = _base(**fields)
            request["revision"] = _revision(spatial_repository)
            direct = execute(spatial_repository, operation, request, require_compiled=True)
            assert direct["state"] == direct_reads[operation].kind.value
            if direct["state"] == "ok":
                assert direct["result"] == _store_wire(direct_reads[operation].value)
            assert direct["operation"] == operation and direct["state"] in {"ok", "invalid", "unavailable", "forbidden", "limit"}
            assert set(direct["cache"]) == {"state", "revision", "treeOid", "sourceSchema", "fingerprint"}
            assert str(spatial_repository.root) not in json.dumps(direct)
            stem = "OverlayAsOf" if operation == "overlay-as-of" else operation.capitalize()
            jsonschema.validate(direct, {"components": components(), "$ref": f"#/components/schemas/Spatial{stem}{direct['state'].capitalize()}Outcome"})

            request_file = tmp_path / f"{operation}.json"
            request_file.write_text(json.dumps(request), encoding="utf-8")
            exit_code = main(["--compact", "spatial", operation, str(request_file), "--repo", str(spatial_repository.root), "--require-compiled"])
            captured = capsys.readouterr()
            assert exit_code == (0 if direct["state"] == "ok" else 2)
            assert json.loads(captured.out if exit_code == 0 else captured.err) == direct

            response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
            assert response.status_code == status_code(direct)
            assert response.json() == direct

        semantic_states = (
            ("children", {**REQUESTS["children"], "cursor": "malformed"}, "invalid"),
            ("path", {"fromLocationId": "location:gate", "toLocationId": "loc_00000000000000000000000000", "metric": "routeDistance"}, "unavailable"),
            ("overlay-as-of", {**REQUESTS["overlay-as-of"], "queryScope": "overlay", "overlayId": "overlay:ward", "audience": "public"}, "forbidden"),
            ("reachability", {**REQUESTS["reachability"], "limit": 1}, "limit"),
        )
        for operation, fields, state in semantic_states:
            request = _base(**fields)
            request["revision"] = _revision(spatial_repository)
            outcome = execute(spatial_repository, operation, request, require_compiled=True)
            assert outcome["state"] == state and "result" not in outcome and outcome["code"].startswith("SPATIAL-")
            response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
            assert response.status_code == status_code(outcome) and response.json() == outcome
            request_file = tmp_path / f"{operation}-{state}.json"
            request_file.write_text(json.dumps(request), encoding="utf-8")
            assert main(["--compact", "spatial", operation, str(request_file), "--repo", str(spatial_repository.root), "--require-compiled"]) == 2
            captured = capsys.readouterr()
            assert not captured.out and json.loads(captured.err) == outcome


def test_spatial_edge_cases_and_cursor_pages_match_direct_store_and_both_public_transports(
    spatial_repository: Repository, tmp_path: Path, capsys: pytest.CaptureFixture[str],
) -> None:
    revision = _revision(spatial_repository)
    cases = [
        ("containment", {"locationId": "location:far"}, lambda s: s.containment("location:far")),
        ("bbox", {"mapId": "map:town", "bounds": {"min": [0, 0], "max": [1, 1]}, "relation": "intersects"}, lambda s: s.bbox("map:town", BoundingBox((0, 0), (1, 1)), relation="intersects")),
        ("bbox", {"mapId": "map:town", "bounds": {"min": [0, 0], "max": [1, 1]}, "relation": "within"}, lambda s: s.bbox("map:town", BoundingBox((0, 0), (1, 1)), relation="within")),
        ("nearby", {"position": {"mapId": "map:town", "coordinates": [0, 0]}, "radius": 2}, lambda s: s.nearby(MapPosition("map:town", (0, 0)), radius=2)),
        ("adjacency", {"locationId": "location:gate", "modes": ["horse"]}, lambda s: s.adjacency("location:gate", modes=["horse"])),
        ("adjacency", {"locationId": "location:gate", "modes": ["foot"]}, lambda s: s.adjacency("location:gate", modes=["foot"])),
        ("reachability", {"fromLocationId": "location:gate", "modes": ["foot"]}, lambda s: s.reachability("location:gate", modes=["foot"])),
        ("path", {"fromLocationId": "location:gate", "toLocationId": "location:side", "metric": "routeDistance", "unit": "pace"}, lambda s: s.path("location:gate", "location:side", metric="route_distance", unit="pace")),
        ("path", {"fromLocationId": "location:gate", "toLocationId": "location:side", "metric": "duration"}, lambda s: s.path("location:gate", "location:side", metric="duration")),
        ("path", {"fromLocationId": "location:gate", "toLocationId": "location:side", "metric": "routeDistance", "unit": "mile"}, lambda s: s.path("location:gate", "location:side", metric="route_distance", unit="mile")),
        ("path", {"fromLocationId": "location:gate", "toLocationId": "location:far", "metric": "routeDistance"}, lambda s: s.path("location:gate", "location:far", metric="route_distance")),
        ("path", {"fromLocationId": "location:side", "toLocationId": "location:gate", "metric": "routeDistance"}, lambda s: s.path("location:side", "location:gate", metric="route_distance")),
        ("path", {"fromLocationId": "location:gate", "toLocationId": "location:earth", "metric": "routeDistance"}, lambda s: s.path("location:gate", "location:earth", metric="route_distance")),
        ("overlay-as-of", {**REQUESTS["overlay-as-of"], "asOf": {"timeline": "main", "tick": "2", "order": "0"}}, lambda s: s.overlay_as_of("location:gate", StoryTime("main", 2, 0), audience="author", perspective="author")),
        ("overlay-as-of", {**REQUESTS["overlay-as-of"], "queryScope": "overlay", "overlayId": "overlay:ward", "audience": "public"}, lambda s: s.overlay_as_of("location:gate", StoryTime("main", 0, 0), audience="public", perspective="author", overlay_id="overlay:ward")),
        ("overlay-as-of", {**REQUESTS["overlay-as-of"], "audience": "public"}, lambda s: s.overlay_as_of("location:gate", StoryTime("main", 0, 0), audience="public", perspective="author")),
        ("reachability", {**REQUESTS["reachability"], "limit": 1}, lambda s: s.reachability("loc_00000000000000000000000000", limit=1)),
    ]
    for tick, order in ((-(2**63), -(2**31)), (2**63 - 1, 2**31 - 1), (0, -1), (0, 1)):
        at = StoryTime("main", tick, order)
        fields = {**REQUESTS["overlay-as-of"], "asOf": {"timeline": "main", "tick": str(tick), "order": str(order)}}
        cases.append(("overlay-as-of", fields, lambda s, value=at: s.overlay_as_of("location:gate", value, audience="author", perspective="author")))
    compiled_world, database = require_database(spatial_repository, require_compiled=True)
    with closing(connect(database, True)) as connection:
        store = SpatialStore(connection, compiled_world.revision)
        # Catalogue cursors are opaque and bound to normalized operands, limit,
        # operation, and revision. Compare both pages through every boundary.
        for operation in ("children", "bbox", "nearby"):
            fields = deepcopy(REQUESTS[operation]); fields["limit"] = 1
            if operation == "children":
                direct = store.children(fields["locationId"], limit=1)
                page = lambda cursor: store.children(fields["locationId"], limit=1, cursor=cursor)
            elif operation == "bbox":
                bounds = BoundingBox(tuple(fields["bounds"]["min"]), tuple(fields["bounds"]["max"]))
                direct = store.bbox(fields["mapId"], bounds, relation=fields["relation"], limit=1)
                page = lambda cursor: store.bbox(fields["mapId"], bounds, relation=fields["relation"], limit=1, cursor=cursor)
            else:
                pos = MapPosition(fields["position"]["mapId"], tuple(fields["position"]["coordinates"]))
                direct = store.nearby(pos, radius=fields["radius"], limit=1)
                page = lambda cursor: store.nearby(pos, radius=fields["radius"], limit=1, cursor=cursor)
            assert direct.kind.value == "ok" and direct.value.cursor
            cases.append((operation, fields, lambda _store, value=direct: value))
            cursor = direct.value.cursor
            cases.append((operation, {**fields, "cursor": cursor}, lambda _store, value=page(cursor): value))
            rebound = page("malformed")
            cases.append((operation, {**fields, "cursor": "malformed"}, lambda _store, value=rebound: value))
            if operation == "children":
                changed = {**fields, "locationId": "location:gate", "cursor": cursor}
                bound = store.children("location:gate", limit=1, cursor=cursor)
            elif operation == "bbox":
                changed = {**fields, "relation": "intersects", "cursor": cursor}
                bound = store.bbox(fields["mapId"], bounds, relation="intersects", limit=1, cursor=cursor)
            else:
                changed = {**fields, "radius": 3, "cursor": cursor}
                bound = store.nearby(pos, radius=3, limit=1, cursor=cursor)
            assert bound.kind.value == "invalid"
            cases.append((operation, changed, lambda _store, value=bound: value))
        with TestClient(create_app(spatial_repository.root)) as client:
            for index, (operation, fields, read) in enumerate(cases):
                expected = read(store)
                request = {**_base(**fields), "revision": revision}
                outcome = execute(spatial_repository, operation, request, require_compiled=True)
                assert outcome["state"] == expected.kind.value, (operation, fields, outcome)
                if outcome["state"] == "ok":
                    assert outcome["result"] == _store_wire(expected.value), (operation, fields)
                else:
                    assert "result" not in outcome and outcome["code"].startswith("SPATIAL-")
                response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
                assert response.status_code == status_code(outcome)
                assert response.json() == outcome
                request_file = tmp_path / f"edge-{index}.json"
                request_file.write_text(json.dumps(request), encoding="utf-8")
                exit_code = main(["--compact", "spatial", operation, str(request_file), "--repo", str(spatial_repository.root), "--require-compiled"])
                captured = capsys.readouterr()
                assert exit_code == (0 if outcome["state"] == "ok" else 2)
                assert json.loads(captured.out if exit_code == 0 else captured.err) == outcome


@pytest.mark.parametrize(("operation", "field"), (("bbox", "relation"), ("path", "metric"), ("overlay-as-of", "queryScope")))
def test_unhashable_read_enums_are_ordinary_wedl_http_errors(
    spatial_repository: Repository, operation: str, field: str,
) -> None:
    request = {**_base(**REQUESTS[operation]), "revision": _revision(spatial_repository), field: ["invalid"]}
    with pytest.raises(UsageError):
        execute(None, operation, request, require_compiled=True)
    with TestClient(create_app(spatial_repository.root)) as client:
        response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
    assert response.status_code == 400
    assert response.json()["code"] == "usage_error"
    assert "state" not in response.json()


@pytest.mark.parametrize("operation", tuple(REQUESTS))
def test_request_shape_is_closed_before_database_access(operation: str) -> None:
    request = _base(**REQUESTS[operation])
    assert _request(request, operation) == request
    with pytest.raises(UsageError):
        execute(None, operation, {**request, "unexpected": True})
    with pytest.raises(UsageError):
        execute(None, operation, {**request, "revision": "not-a-sha"})


@pytest.mark.parametrize("bad", [0, True, 101, 1.0])
def test_limit_rejects_nonintegers_and_out_of_range_values(bad: object) -> None:
    with pytest.raises(UsageError):
        execute(None, "children", {**_base(**REQUESTS["children"]), "limit": bad})


@pytest.mark.parametrize("bad", [0, True, "-0", "+1", "01", "9223372036854775808"])
def test_story_time_rejects_noncanonical_or_out_of_range_values_before_lookup(bad: object) -> None:
    request = _base(**REQUESTS["overlay-as-of"])
    request["asOf"] = {"timeline": "main", "tick": bad, "order": "0"}
    with pytest.raises(UsageError):
        execute(None, "overlay-as-of", request)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), True, 2**53])
def test_geometry_rejects_nonfinite_boolean_and_unsafe_numbers_before_lookup(bad: object) -> None:
    request = _base(**REQUESTS["nearby"])
    request["position"] = {"mapId": "map:town", "coordinates": [bad, 0]}
    with pytest.raises(UsageError):
        execute(None, "nearby", request)


def test_capability_order_and_operation_fields_are_closed_before_lookup() -> None:
    request = _base(**REQUESTS["adjacency"])
    for bad in (["route-v1", "spatial-core-v1"], ["spatial-core-v1", "spatial-core-v1"], ["future-v1"]):
        with pytest.raises(UsageError):
            execute(None, "adjacency", {**request, "capabilities": bad})
    assert _request({**request, "capabilities": []}, "adjacency")["capabilities"] == []
    with pytest.raises(UsageError):
        execute(None, "adjacency", {**request, "fromLocationId": "location:gate"})
    with pytest.raises(UsageError):
        execute(None, "path", {**_base(**REQUESTS["path"]), "cursor": "opaque"})
    with pytest.raises(UsageError):
        execute(None, "children", {**_base(**REQUESTS["children"]), "locationId": "x" * 257})


def test_hidden_overlay_and_no_overlay_control_have_identical_public_result(spatial_repository: Repository) -> None:
    request = _base(**REQUESTS["overlay-as-of"])
    request["revision"] = _revision(spatial_repository)
    request["audience"] = "public"
    hidden = execute(spatial_repository, "overlay-as-of", request, require_compiled=True)
    assert hidden["state"] == "ok" and hidden["result"]["ids"] == []
    request["locationId"] = "loc_00000000000000000000000000"
    control = execute(spatial_repository, "overlay-as-of", request, require_compiled=True)
    assert hidden == control


def test_legacy_compiled_capability_absence_is_a_typed_public_unavailable(
    ash_repo: Repository, tmp_path: Path, capsys: pytest.CaptureFixture[str],
) -> None:
    assert compile_world(ash_repo, "HEAD")["status"] == "compiled"
    with TestClient(create_app(ash_repo.root)) as client:
        for operation, fields in REQUESTS.items():
            request = {**_base(**fields), "revision": ash_repo.head(), "capabilities": []}
            outcome = execute(ash_repo, operation, request, require_compiled=True)
            assert outcome["state"] == "unavailable" and outcome["capabilities"] == []
            assert outcome["code"] == "SPATIAL-REQUEST-001" and "result" not in outcome
            stem = "OverlayAsOf" if operation == "overlay-as-of" else operation.capitalize()
            jsonschema.validate(outcome, {"components": components(), "$ref": f"#/components/schemas/Spatial{stem}UnavailableOutcome"})
            path = tmp_path / f"legacy-{operation}.json"
            path.write_text(json.dumps(request), encoding="utf-8")
            assert main(["--compact", "spatial", operation, str(path), "--repo", str(ash_repo.root), "--require-compiled"]) == 2
            captured = capsys.readouterr()
            assert not captured.out and json.loads(captured.err) == outcome
            response = client.post(f"/api/spatial/{operation}?requireCompiled=true", json=request)
            assert response.status_code == 409 and response.json() == outcome


def test_rounded_two_edge_metric_is_typed_unavailable_across_public_transports(
    ash_repo: Repository, tmp_path: Path, capsys: pytest.CaptureFixture[str],
) -> None:
    fixture = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text(encoding="utf-8"))
    first = deepcopy(fixture["route"])
    first["route_distance"] = {"value": 2**53 - 1, "unit": "pace"}
    second = {"schema": "wedl/v0.7", "kind": "route", "id": "route:gate-side", "title": "Gate to side",
              "from_location_id": "location:gate", "to_location_id": "location:side", "direction": "one-way",
              "modes": ["foot"], "route_distance": {"value": 0.5, "unit": "pace"}}
    side = {"schema": "wedl/v0.7", "kind": "location", "id": "location:side", "title": "Side",
            "parent_id": "loc_00000000000000000000000000"}
    records = [fixture["world"], *fixture["maps"], *fixture["locations"], side,
               fixture["anchor"], fixture["portal"], first, second, fixture["overlay"]]
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    for record in records:
        path = generated_path("story", record["kind"], record["title"], record["id"], record)
        changes[path] = serialize_record(record, f"# {record['title']}\n")
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed unsafe spatial metric result")
    assert compile_world(ash_repo, "HEAD")["status"] == "compiled"
    revision = ash_repo.head()
    with closing(connect(require_database(ash_repo, require_compiled=True)[1], True)) as connection:
        direct = SpatialStore(connection, revision).path(
            "loc_00000000000000000000000000", "location:side", metric="route_distance",
        )
    assert direct.kind.value == "ok"
    assert direct.value.metric.value == float(2**53)
    request = {**_base(fromLocationId="loc_00000000000000000000000000", toLocationId="location:side", metric="routeDistance"),
               "revision": revision}
    outcome = execute(ash_repo, "path", request, require_compiled=True)
    assert outcome["state"] == "unavailable" and outcome["code"] == "SPATIAL-METRIC-001"
    assert "result" not in outcome
    jsonschema.validate(outcome, {"components": components(), "$ref": "#/components/schemas/SpatialPathUnavailableOutcome"})
    request_file = tmp_path / "unsafe-metric.json"
    request_file.write_text(json.dumps(request), encoding="utf-8")
    assert main(["--compact", "spatial", "path", str(request_file), "--repo", str(ash_repo.root), "--require-compiled"]) == 2
    captured = capsys.readouterr()
    assert not captured.out and json.loads(captured.err) == outcome
    with TestClient(create_app(ash_repo.root)) as client:
        response = client.post("/api/spatial/path?requireCompiled=true", json=request)
    assert response.status_code == 409 and response.json() == outcome
