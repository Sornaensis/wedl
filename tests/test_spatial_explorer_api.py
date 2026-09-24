"""HTTP-only spatial explorer projections and closed selected-revision boundaries."""

from __future__ import annotations

import base64
import json
import math
import sqlite3
from pathlib import Path
import shutil

import jsonschema
import pytest
from fastapi.testclient import TestClient

from test_spatial_api import _spatial_seed
from wedl import spatial_explorer_api as explorer_module
from wedl.api_contract import explorer_endpoints
from wedl.api_router import openapi_contract_errors
from wedl.api_schemas import components
from wedl.compiler import compile_world
from wedl.errors import CompileRequired, RepositoryError, UsageError
from wedl.repository import Repository
from wedl.server import create_app
from wedl.spatial_index import install_optional_spatial_index
from wedl.spatial_explorer_api import PROTOCOL, _common, _geometry, _layers, execute, status_code
from test_spatial_query import _entity, _location, _store


def _request(repository: Repository, **fields: object) -> dict[str, object]:
    return {"protocol": PROTOCOL, "revision": repository.head(),
            "capabilities": ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"],
            "limit": 1, "cursor": None, **fields}


def _validate(operation: str, value: dict[str, object]) -> None:
    stem = operation.capitalize()
    schema = {"components": components(), "$ref": f"#/components/schemas/SpatialExplorer{stem}{value['state'].capitalize()}Outcome"}
    jsonschema.validate(value, schema)


def _vm_steps(connection: sqlite3.Connection, action):
    steps = [0]

    def count_steps() -> int:
        steps[0] += 10
        return 0

    connection.set_progress_handler(count_steps, 10)
    try:
        value = action()
    finally:
        connection.set_progress_handler(None, 0)
    return value, steps[0]


def _http_parity(client: TestClient, repository: Repository, operation: str,
                 request: dict[str, object], state: str) -> dict[str, object]:
    direct = execute(repository, operation, {"protocol": PROTOCOL, **request}
                     if operation == "catalog" else request)
    response = (client.get("/api/spatial/explorer/catalog", params=request)
                if operation == "catalog" else
                client.post(f"/api/spatial/explorer/{operation}", json=request))
    assert direct["state"] == state
    assert response.status_code == status_code(direct)
    assert response.json() == direct
    assert str(repository.root) not in json.dumps(direct)
    _validate(operation, direct)
    return direct


@pytest.fixture(scope="module")
def explorer_repository(tmp_path_factory: pytest.TempPathFactory,
                        _spatial_seed: tuple[Path, dict[str, str], str]) -> Repository:
    # The HTTP runtime writes a local session file. Copy the immutable seed
    # once for this module rather than once per read-only test.
    root = tmp_path_factory.mktemp("explorer-repo") / "repository"
    shutil.copytree(_spatial_seed[0], root, copy_function=shutil.copy2)
    return Repository(root)


def test_four_http_only_routes_openapi_and_examples(explorer_repository: Repository) -> None:
    app = create_app(explorer_repository.root)
    schema = app.openapi()
    assert openapi_contract_errors(schema) == ()
    assert [(endpoint.method, endpoint.path) for endpoint in explorer_endpoints()] == [
        ("GET", "/api/spatial/explorer/catalog"),
        ("POST", "/api/spatial/explorer/places"),
        ("POST", "/api/spatial/explorer/viewport"),
        ("POST", "/api/spatial/explorer/layers"),
    ]
    for endpoint in explorer_endpoints():
        descriptor = endpoint.descriptor
        response = descriptor.success_examples[0]["value"]
        _validate(endpoint.path.rsplit("/", 1)[-1], response)
        operation = schema["paths"][endpoint.path][endpoint.method.lower()]
        assert {"200", "400", "403", "409", "422"} <= set(operation["responses"])
        assert operation["x-wedl-examples"] == list(descriptor.examples)
        if endpoint.method == "POST":
            request = descriptor.examples[0]["value"]["body"]
            jsonschema.validate(request, {"components": components(), "$ref": f"#/components/schemas/{descriptor.request_schema}"})
            assert operation["requestBody"]["content"]["application/json"]["examples"]["request"]["value"] == request
        if endpoint.path.endswith("/places"):
            assert {example["value"]["body"]["mode"] for example in descriptor.examples} == {
                "roots", "children", "search", "select"}
            for example in descriptor.examples:
                jsonschema.validate(example["value"]["body"], {"components": components(),
                    "$ref": f"#/components/schemas/{descriptor.request_schema}"})
            for example in descriptor.success_examples:
                _validate("places", example["value"])


def test_catalog_places_viewport_layers_direct_http_and_selected_cursor(explorer_repository: Repository) -> None:
    spatial_repository = explorer_repository
    with TestClient(create_app(spatial_repository.root)) as client:
        catalog = execute(spatial_repository, "catalog", {"protocol": PROTOCOL, "limit": 1})
        response = client.get("/api/spatial/explorer/catalog?limit=1")
        assert response.status_code == 200 and response.json() == catalog
        assert catalog["result"]["spatialAvailable"] is True
        assert len(catalog["result"]["maps"]) == 1
        assert catalog["result"]["nextCursor"]
        _validate("catalog", catalog)
        later = {"protocol": PROTOCOL, "revision": spatial_repository.head(),
                 "capabilities": catalog["capabilities"], "limit": 1,
                 "cursor": catalog["result"]["nextCursor"]}
        second = execute(spatial_repository, "catalog", later)
        assert second["state"] == "ok" and len(second["result"]["maps"]) == 1
        assert second["result"]["maps"][0]["id"] != catalog["result"]["maps"][0]["id"]
        assert client.get("/api/spatial/explorer/catalog", params={"limit": 1, "revision": later["revision"],
                          "capabilities": later["capabilities"], "cursor": later["cursor"]}).json() == second
        rebound = execute(spatial_repository, "catalog", {**later, "limit": 2})
        assert rebound["state"] == "invalid" and status_code(rebound) == 400
        _validate("catalog", rebound)
        for field in ({"revision": "0" * 40}, {"capabilities": []}):
            with pytest.raises(UsageError):
                execute(spatial_repository, "catalog", {**later, **field})
            rejected = client.get("/api/spatial/explorer/catalog", params={**later, **field})
            assert rejected.status_code == 400 and rejected.json()["code"] == "usage_error"

        requests = {
            "places": _request(spatial_repository, mode="children", parentId="loc_00000000000000000000000000"),
            "viewport": _request(spatial_repository, mapId="map:town", bounds={"min": [-2, -2], "max": [2, 2]}, relation="intersects"),
            "layers": _request(spatial_repository, mapId="map:town", bounds={"min": [-2, -2], "max": [2, 2]}, relation="intersects", asOf={"timeline": "main", "tick": "0", "order": "0"}, audience="author", perspective="author"),
        }
        for operation, request in requests.items():
            direct = execute(spatial_repository, operation, request)
            http = client.post(f"/api/spatial/explorer/{operation}", json=request)
            assert http.status_code == status_code(direct) and http.json() == direct
            assert direct["state"] == "ok" and len(direct["result"][{"places": "places", "viewport": "features", "layers": "layers"}[operation]]) == 1
            assert str(spatial_repository.root) not in json.dumps(direct)
            _validate(operation, direct)
            cursor = direct["result"]["nextCursor"]
            if cursor:
                page = execute(spatial_repository, operation, {**request, "cursor": cursor})
                assert page["state"] == "ok"
                assert client.post(f"/api/spatial/explorer/{operation}", json={**request, "cursor": cursor}).json() == page
                changed = {**request, "cursor": cursor, "limit": 2}
                invalid = execute(spatial_repository, operation, changed)
                assert invalid["state"] == "invalid"
                _validate(operation, invalid)
                changes = {
                    "places": ({"parentId": "location:gate"},),
                    "viewport": ({"mapId": "map:earth"},
                                 {"bounds": {"min": [-1, -1], "max": [2, 2]}},
                                 {"relation": "within"}),
                    "layers": ({"mapId": "map:earth"},
                               {"bounds": {"min": [-1, -1], "max": [2, 2]}},
                               {"relation": "within"},
                               {"asOf": {"timeline": "main", "tick": "1", "order": "0"}},
                               {"audience": "public"}, {"perspective": "visitor"}),
                }[operation]
                for field in changes:
                    _http_parity(client, spatial_repository, operation,
                                 {**request, "cursor": cursor, **field}, "invalid")
                if operation == "places":
                    changed_mode = {key: value for key, value in request.items() if key != "parentId"}
                    _http_parity(client, spatial_repository, operation,
                                 {**changed_mode, "mode": "roots", "cursor": cursor}, "invalid")
                for field in ({"revision": "0" * 40}, {"capabilities": []}):
                    with pytest.raises(UsageError):
                        execute(spatial_repository, operation, {**request, "cursor": cursor, **field})
                    rejected = client.post(f"/api/spatial/explorer/{operation}",
                                           json={**request, "cursor": cursor, **field})
                    assert rejected.status_code == 400 and rejected.json()["code"] == "usage_error"
        stale = client.post("/api/spatial/explorer/places", json={**requests["places"], "revision": "0" * 40})
        assert stale.status_code == 400 and stale.json()["code"] == "usage_error"
        assert client.get("/api/spatial/explorer/places").status_code == 405
        assert client.get("/api/spatial/explorer/catalog?limit=1&unexpected=1").status_code == 400


def test_places_modes_lens_and_closed_validation(explorer_repository: Repository) -> None:
    spatial_repository = explorer_repository
    roots = execute(spatial_repository, "places", _request(spatial_repository, mode="roots", limit=10))
    assert roots["state"] == "ok"
    assert roots["result"]["places"][0]["parentId"] is None
    selected = execute(spatial_repository, "places", _request(spatial_repository, mode="select", ids=["location:gate", "location:far"], limit=10))
    assert {place["id"] for place in selected["result"]["places"]} == {"location:gate", "location:far"}
    search = execute(spatial_repository, "places", _request(spatial_repository, mode="search", query="Gate", limit=10))
    assert search["state"] == "ok" and any(place["id"] == "location:gate" for place in search["result"]["places"])
    assert execute(spatial_repository, "places", _request(spatial_repository, mode="search", query='" OR *', limit=10))["state"] == "ok"

    layer = _request(spatial_repository, mapId="map:town", bounds={"min": [-2, -2], "max": [2, 2]}, relation="intersects", asOf={"timeline": "main", "tick": "0", "order": "0"}, audience="public", perspective="author", limit=10)
    hidden = execute(spatial_repository, "layers", layer)
    assert hidden["state"] == "ok" and hidden["result"]["layers"] == [] and hidden["result"]["nextCursor"] is None
    forbidden = execute(spatial_repository, "layers", {**layer, "overlayId": "overlay:ward"})
    missing = execute(spatial_repository, "layers", {**layer, "overlayId": "overlay:missing"})
    assert forbidden == missing and forbidden["state"] == "forbidden"
    _validate("layers", forbidden)
    for changed in ({"audience": "author"}, {"perspective": "another"}, {"asOf": {"timeline": "main", "tick": "1", "order": "0"}}, {"bounds": {"min": [-1, -1], "max": [2, 2]}}):
        base = {**_request(spatial_repository, mapId="map:town", bounds={"min": [-2, -2], "max": [2, 2]}, relation="intersects", asOf={"timeline": "main", "tick": "0", "order": "0"}, audience="author", perspective="author"), "cursor": "bad"}
        result = execute(spatial_repository, "layers", {**base, **changed})
        assert result["state"] == "invalid"
    with pytest.raises(UsageError):
        _common(_request(spatial_repository, mode="roots", bogus=True), "places")
    with pytest.raises(UsageError):
        _common(_request(spatial_repository, mapId="map:town", bounds={"min": [0, 0], "max": [1, 1]}, relation="intersects", asOf={"timeline": "main", "tick": 0, "order": "0"}, audience="author", perspective="author"), "layers")


def test_all_place_modes_and_map_features_match_authored_source(explorer_repository: Repository) -> None:
    repository = explorer_repository
    world = repository.load_world()
    with TestClient(create_app(repository.root)) as client:
        catalog = _http_parity(client, repository, "catalog", {"limit": 10}, "ok")
        for descriptor in catalog["result"]["maps"]:
            source = world.records[descriptor["id"]]
            assert descriptor == {"id": source.id, "label": source.title,
                                  "crs": source.frontmatter["crs"],
                                  "axes": source.frontmatter["axis_order"],
                                  "unit": source.frontmatter["unit"],
                                  "bounds": source.frontmatter["bounds"]}
        requests = (
            ("roots", {}, "authored-parent-id"),
            ("children", {"parentId": "loc_00000000000000000000000000"}, "authored-parent-id"),
            ("search", {"query": "Gate"}, "compiled-title-search"),
            ("select", {"ids": ["location:gate", "location:far"]}, "authored-location-id"),
        )
        for mode, fields, basis in requests:
            outcome = _http_parity(client, repository, "places",
                                   _request(repository, mode=mode, limit=10, **fields), "ok")
            assert outcome["result"]["basis"] == basis
            assert outcome["result"]["places"]
            for card in outcome["result"]["places"]:
                source = world.records[card["id"]]
                spatial = source.frontmatter.get("spatial") or {}
                assert (card["label"], card["parentId"], card["mapId"], card["geometryAvailable"]) == (
                    source.title, source.frontmatter.get("parent_id"), spatial.get("map_id"), bool(spatial))
        viewport = _http_parity(client, repository, "viewport", _request(
            repository, mapId="map:town", bounds={"min": [-2, -2], "max": [2, 2]},
            relation="intersects", limit=10), "ok")
        assert viewport["result"]["crs"] == world.records["map:town"].frontmatter["crs"]
        assert viewport["result"]["unit"] == world.records["map:town"].frontmatter["unit"]
        assert {feature["id"] for feature in viewport["result"]["features"]} == {"location:gate", "location:side"}
        for feature in viewport["result"]["features"]:
            source = world.records[feature["id"]]
            assert feature["geometry"] == source.frontmatter["spatial"]["geometry"]
        layers = _http_parity(client, repository, "layers", _request(
            repository, mapId="map:town", bounds={"min": [-2, -2], "max": [2, 2]},
            relation="intersects", asOf={"timeline": "main", "tick": "0", "order": "0"},
            audience="author", perspective="author", limit=10), "ok")
        for layer in layers["result"]["layers"]:
            source = world.records[layer["overlayId"]]
            assert layer["label"] == source.title
            assert layer["locationId"] in source.frontmatter["membership"]["location_ids"]
            assert layer["geometry"] == world.records[layer["locationId"]].frontmatter["spatial"]["geometry"]


def test_forged_cursor_ordinals_and_surrogates_close_as_invalid(explorer_repository: Repository) -> None:
    repository = explorer_repository
    catalog = {key: value for key, value in _request(repository).items() if key != "protocol"}
    cases = (
        ("catalog", catalog, (2**63, "map:town")),
        ("places", _request(repository, mode="search", query="Gate"), (2**63, "location:gate")),
        ("places", _request(repository, mode="select", ids=["location:gate"]), (2**63, "location:gate")),
        ("places", _request(repository, mode="roots"), (0, "\ud800")),
        ("places", _request(repository, mode="children", parentId="location:gate"), (0, "\udfff")),
    )
    with TestClient(create_app(repository.root)) as client:
        for operation, request, last in cases:
            normalized = {"protocol": PROTOCOL, **request} if operation == "catalog" else request
            binding, _ = explorer_module._page(normalized, repository.head())
            payload = json.dumps({"binding": binding, "last": list(last)}, ensure_ascii=True).encode("ascii")
            forged = base64.urlsafe_b64encode(payload).decode("ascii").rstrip("=")
            _http_parity(client, repository, operation, {**request, "cursor": forged}, "invalid")


def test_non_ok_state_matrix_and_guard_parity(explorer_repository: Repository,
                                              monkeypatch: pytest.MonkeyPatch) -> None:
    repository = explorer_repository
    bounds = {"min": [-2, -2], "max": [2, 2]}
    viewport = _request(repository, mapId="map:town", bounds=bounds, relation="intersects")
    layers = {**viewport, "asOf": {"timeline": "main", "tick": "0", "order": "0"},
              "audience": "author", "perspective": "author"}
    with TestClient(create_app(repository.root)) as client:
        catalog = _http_parity(client, repository, "catalog", {"limit": 1}, "ok")
        _http_parity(client, repository, "catalog", {"limit": 1, "revision": repository.head(),
                     "capabilities": catalog["capabilities"], "cursor": "bad"}, "invalid")
        _http_parity(client, repository, "places", _request(repository, mode="children",
                     parentId="location:missing"), "unavailable")
        _http_parity(client, repository, "places", _request(repository, mode="roots", cursor="bad"), "invalid")
        _http_parity(client, repository, "viewport", {**viewport, "mapId": "map:missing"}, "unavailable")
        _http_parity(client, repository, "viewport", {**viewport, "bounds": {"min": [0, 0, 0], "max": [1, 1, 1]}}, "unavailable")
        _http_parity(client, repository, "viewport", {**viewport, "cursor": "bad"}, "invalid")
        _http_parity(client, repository, "layers", {**layers, "mapId": "map:missing"}, "unavailable")
        hidden = {**layers, "audience": "public", "overlayId": "overlay:ward"}
        assert _http_parity(client, repository, "layers", hidden, "forbidden") == _http_parity(
            client, repository, "layers", {**hidden, "overlayId": "overlay:missing"}, "forbidden")
        _http_parity(client, repository, "layers", {**layers, "cursor": "bad"}, "invalid")
        monkeypatch.setattr(explorer_module, "_MAX_SEARCH_CANDIDATES", 0)
        _http_parity(client, repository, "places", _request(repository, mode="search", query="Gate"), "limit")
        monkeypatch.setattr(explorer_module, "MAX_GEOMETRY_CANDIDATES", 1)
        _http_parity(client, repository, "viewport", viewport, "limit")
        monkeypatch.setattr(explorer_module, "MAX_OVERLAY_CANDIDATES", 0)
        _http_parity(client, repository, "layers", layers, "limit")
        for operation, request in (("places", _request(repository, mode="roots")),
                                   ("viewport", viewport), ("layers", layers)):
            stale = {**request, "revision": "0" * 40}
            response = client.post(f"/api/spatial/explorer/{operation}", json=stale)
            assert response.status_code == 400 and response.json()["code"] == "usage_error"
            assert str(repository.root) not in json.dumps(response.json())
            wrong_capabilities = {**request, "capabilities": []}
            response = client.post(f"/api/spatial/explorer/{operation}", json=wrong_capabilities)
            assert response.status_code == 400 and response.json()["code"] == "usage_error"


def test_compiled_read_errors_redact_host_paths(explorer_repository: Repository,
                                               monkeypatch: pytest.MonkeyPatch) -> None:
    repository = explorer_repository
    request = _request(repository, mode="roots")
    with TestClient(create_app(repository.root)) as client:
        original_require_database = explorer_module.require_database
        for error, status, code in ((CompileRequired, 400, "compile_required"),
                                    (RepositoryError, 400, "repository_error")):
            def fail(*_args: object, **_kwargs: object) -> None:
                raise error(f"private cache at {repository.root}",
                            details={"path": str(repository.root)})
            monkeypatch.setattr(explorer_module, "require_database", fail)
            with pytest.raises(error) as direct:
                execute(repository, "places", request)
            assert str(repository.root) not in json.dumps(direct.value.as_dict())
            response = client.post("/api/spatial/explorer/places", json=request)
            assert response.status_code == status and response.json()["code"] == code
            assert str(repository.root) not in json.dumps(response.json())
        monkeypatch.setattr(explorer_module, "require_database", original_require_database)
        def fail_connect(*_args: object, **_kwargs: object) -> None:
            raise sqlite3.OperationalError(f"private cache at {repository.root}")
        monkeypatch.setattr(explorer_module, "connect", fail_connect)
        with pytest.raises(RepositoryError) as direct:
            execute(repository, "places", request)
        assert str(repository.root) not in json.dumps(direct.value.as_dict())
        response = client.post("/api/spatial/explorer/places", json=request)
        assert response.status_code == 400 and response.json()["code"] == "repository_error"
        assert str(repository.root) not in json.dumps(response.json())


def test_signed_large_bounds_and_unsafe_geometry_close(explorer_repository: Repository) -> None:
    repository = explorer_repository
    maximum = 2**53 - 1
    request = _request(repository, mapId="map:town",
                       bounds={"min": [-maximum, -maximum], "max": [maximum, maximum]},
                       relation="intersects", limit=10)
    with TestClient(create_app(repository.root)) as client:
        outcome = _http_parity(client, repository, "viewport", request, "ok")
        assert {feature["id"] for feature in outcome["result"]["features"]} == {
            "location:gate", "location:side"}
    with pytest.raises(UsageError):
        _common({**request, "bounds": {"min": [-(2**53), 0], "max": [0, 1]}}, "viewport")
    with pytest.raises(UsageError):
        _common({**request, "bounds": {"min": [0, 0], "max": [math.inf, 1]}}, "viewport")
    with pytest.raises(ValueError, match="unsafe compiled geometry"):
        _geometry("point", "[NaN,0]")


def test_legacy_catalog_is_useful_and_non_map_reads_close(task91_repo: Repository) -> None:
    assert compile_world(task91_repo, "HEAD")["status"] == "compiled"
    catalog = execute(task91_repo, "catalog", {"protocol": PROTOCOL, "limit": 10})
    assert catalog["state"] == "ok"
    assert catalog["result"] == {"spatialAvailable": False, "maps": [], "nextCursor": None}
    _validate("catalog", catalog)
    places = execute(task91_repo, "places", {"protocol": PROTOCOL, "revision": task91_repo.head(), "capabilities": [], "limit": 10, "cursor": None, "mode": "roots"})
    assert places["state"] == "unavailable"
    _validate("places", places)


def test_layer_pages_bind_lens_horizon_and_member_order() -> None:
    store = _store()
    _location(store, "location:side", 100, parent="loc_00000000000000000000000000", map_id="map:town", point=(1, 1))
    map_key = store.connection.execute("SELECT rowid FROM spatial_map WHERE id='map:town'").fetchone()[0]
    store.connection.execute("INSERT INTO spatial_location_rtree VALUES (?,?,?,?,?,?,?)", (100, 1, 1, 1, 1, map_key, map_key))
    _entity(store, "overlay:second", "overlay", 101)
    store.connection.execute("INSERT INTO spatial_overlay VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                             ("overlay:second", 101, "static", '["author"]', '["author"]', None, None, None, None, None, "{}"))
    store.connection.executemany("INSERT INTO spatial_overlay_location VALUES (?,?,?)",
                                 [("overlay:second", "location:gate", 0), ("overlay:second", "location:side", 1)])
    store.connection.execute("INSERT INTO spatial_overlay_audience VALUES (?,?,?)", ("overlay:second", "author", 0))
    store.connection.execute("INSERT INTO spatial_overlay_perspective VALUES (?,?,?)", ("overlay:second", "author", 0))
    store.connection.executemany("INSERT INTO spatial_overlay_lens_location VALUES (?,?,?,?,?,?,?,?,?,?)", [
        ("location:gate", "author", "author", "overlay:second", "static", None, None, None, None, None),
        ("location:side", "author", "author", "overlay:second", "static", None, None, None, None, None),
    ])
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 1, "cursor": None,
               "mapId": "map:town", "bounds": {"min": [-2, -2], "max": [2, 2]},
               "relation": "intersects", "asOf": {"timeline": "main", "tick": "0", "order": "0"},
               "audience": "author", "perspective": "author"}
    seen: list[tuple[str, str]] = []
    for _ in range(4):
        state, result, _, _ = _layers(store.connection, request, store.revision)
        assert state == "ok"
        seen.extend((row["overlayId"], row["locationId"]) for row in result["layers"])
        cursor = result["nextCursor"]
        if cursor is None:
            break
        for change in ({"audience": "public"}, {"perspective": "visitor"},
                       {"asOf": {"timeline": "main", "tick": "1", "order": "0"}},
                       {"bounds": {"min": [-1, -1], "max": [2, 2]}},
                       {"mapId": "map:other"}, {"relation": "within"},
                       {"limit": 2}, {"overlayId": "overlay:second"}):
            rebound, _, _, _ = _layers(store.connection, {**request, **change, "cursor": cursor}, store.revision)
            assert rebound == "invalid"
        revision_rebound, _, _, _ = _layers(store.connection, {**request, "cursor": cursor}, "revision-b")
        assert revision_rebound == "invalid"
        request = {**request, "cursor": cursor}
    store.connection.close()
    assert len(seen) == 3 and len(set(seen)) == 3
    assert {ident for ident, _ in seen} == {"overlay:ward", "overlay:second"}


def test_layers_budget_counts_only_authorized_viewport_memberships() -> None:
    store = _store()
    rows = [(f"overlay:outside:{index:04d}", index + 1000) for index in range(2001)]
    store.connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
        (ident, "overlay", ident, "world", "canonical", f"story/{ident}.md", None, "", "{}")
        for ident, _ in rows))
    store.connection.executemany("INSERT INTO spatial_overlay VALUES (?,?,?,?,?,?,?,?,?,?,?)", (
        (ident, ordinal, "static", '["author"]', '["author"]', None, None, None, None, None, "{}")
        for ident, ordinal in rows))
    store.connection.executemany("INSERT INTO spatial_overlay_location VALUES (?,?,?)", (
        (ident, "location:gate", 0) for ident, _ in rows))
    store.connection.executemany("INSERT INTO spatial_overlay_audience VALUES (?,?,?)", (
        (ident, "author", 0) for ident, _ in rows))
    store.connection.executemany("INSERT INTO spatial_overlay_perspective VALUES (?,?,?)", (
        (ident, "author", 0) for ident, _ in rows))
    store.connection.executemany("INSERT INTO spatial_overlay_lens_location VALUES (?,?,?,?,?,?,?,?,?,?)", (
        ("location:gate", "author", "author", ident, "static", None, None, None, None, None)
        for ident, _ in rows))
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 10, "cursor": None,
               "mapId": "map:town", "bounds": {"min": [50, 50], "max": [51, 51]},
               "relation": "intersects", "asOf": {"timeline": "main", "tick": "0", "order": "0"},
               "audience": "author", "perspective": "author"}
    state, result, _, _ = _layers(store.connection, request, store.revision)
    assert state == "ok" and result["layers"] == [] and result["nextCursor"] is None
    state, result, code, _ = _layers(store.connection, {**request,
        "bounds": {"min": [-1, -1], "max": [1, 1]}}, store.revision)
    assert state == "limit" and result is None and code == "SPATIAL-LIMIT-001"
    store.connection.close()


def test_empty_late_catalog_page_seeks_unique_map_ordinal() -> None:
    store = _store()
    connection = store.connection
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 10, "cursor": None}
    binding, _ = explorer_module._page(request, store.revision)

    def insert_maps(start: int, stop: int) -> None:
        connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
            (f"map:late:{index:05d}", "map", f"Map {index}", "world", "canonical",
             f"story/map-{index:05d}.md", None, "", "{}") for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_map VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
            (f"map:late:{index:05d}", 1000 + index, "local", "x", "y", "pace", "forbidden",
             -1, -1, None, 1, 1, None, "{}") for index in range(start, stop)))

    def probe(last_index: int) -> int:
        cursor = explorer_module._cursor(binding, (1000 + last_index, f"map:late:{last_index:05d}"))
        outcome, steps = _vm_steps(connection, lambda: explorer_module._catalog(
            connection, {**request, "cursor": cursor}, store.revision))
        assert outcome[0] == "ok" and outcome[1]["maps"] == []
        return steps

    insert_maps(0, 1000)
    small = probe(999)
    insert_maps(1000, 10000)
    large = probe(9999)
    assert large <= small * 2 + 100, (small, large)
    connection.close()


def test_place_search_caps_nonplace_fts_matches_before_join() -> None:
    store = _store()
    connection = store.connection
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 10, "cursor": None,
               "mode": "search", "query": "Ghost"}

    def insert_nonplaces(start: int, stop: int) -> None:
        connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
            (f"character:ghost:{index:05d}", "character", "Ghost", "world", "canonical",
             f"story/ghost-{index:05d}.md", None, "", "{}") for index in range(start, stop)))
        connection.executemany("INSERT INTO search_document(rowid,document_id,entity_id,document_kind,heading,audience_kind,audience_character_id,scene_id,timeline,from_tick,from_order,until_tick,until_order,text,metadata_json,chunk_hash) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
            (index + 1, f"ghost-doc:{index:05d}", f"character:ghost:{index:05d}", "record",
             None, "public", None, None, None, None, None, None, None, "", "{}", "hash")
            for index in range(start, stop)))
        connection.executemany("INSERT INTO search_fts(rowid,title,aliases,heading,text,domain,tags) VALUES (?,?,?,?,?,?,?)", (
            (index + 1, "Ghost", "", "", "", "world", "") for index in range(start, stop)))

    insert_nonplaces(0, 1000)
    small_result, small = _vm_steps(connection, lambda: explorer_module._places(connection, request, store.revision))
    assert small_result[0] == "ok" and small_result[1]["places"] == []
    insert_nonplaces(1000, 10000)
    large_result, large = _vm_steps(connection, lambda: explorer_module._places(connection, request, store.revision))
    assert large_result[0] == "limit" and large_result[2] == "SPATIAL-LIMIT-001"
    assert large <= small * 4 + 500, (small, large)
    connection.close()


def test_layers_seek_viewport_and_lens_before_offscreen_overlay_work() -> None:
    store = _store()
    connection = store.connection
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 10, "cursor": None,
               "mapId": "map:town", "bounds": {"min": [50, 50], "max": [51, 51]},
               "relation": "intersects", "asOf": {"timeline": "main", "tick": "0", "order": "0"},
               "audience": "author", "perspective": "author"}

    def insert_offscreen(start: int, stop: int) -> None:
        connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
            (f"overlay:far:{index:05d}", "overlay", f"Far {index}", "world", "canonical",
             f"story/far-{index:05d}.md", None, "", "{}") for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay VALUES (?,?,?,?,?,?,?,?,?,?,?)", (
            (f"overlay:far:{index:05d}", 1000 + index, "static", '["author"]', '["author"]',
             None, None, None, None, None, "{}") for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_location VALUES (?,?,?)", (
            (f"overlay:far:{index:05d}", "location:gate", 0) for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_audience VALUES (?,?,?)", (
            (f"overlay:far:{index:05d}", "author", 0) for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_perspective VALUES (?,?,?)", (
            (f"overlay:far:{index:05d}", "author", 0) for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_lens_location VALUES (?,?,?,?,?,?,?,?,?,?)", (
            ("location:gate", "author", "author", f"overlay:far:{index:05d}",
             "static", None, None, None, None, None)
            for index in range(start, stop)))

    insert_offscreen(0, 1000)
    small_result, small = _vm_steps(connection, lambda: _layers(connection, request, store.revision))
    assert small_result[0] == "ok" and small_result[1]["layers"] == []
    insert_offscreen(1000, 10000)
    large_result, large = _vm_steps(connection, lambda: _layers(connection, request, store.revision))
    assert large_result[0] == "ok" and large_result[1]["layers"] == []
    assert large <= small * 2 + 100, (small, large)
    connection.close()


def test_layers_seek_exact_as_of_before_future_or_expired_candidates() -> None:
    store = _store()
    connection = store.connection
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 10, "cursor": None,
               "mapId": "map:town", "bounds": {"min": [-1, -1], "max": [1, 1]},
               "relation": "intersects", "asOf": {"timeline": "main", "tick": "0", "order": "0"},
               "audience": "author", "perspective": "author"}

    def insert_future(start: int, stop: int) -> None:
        connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
            (f"overlay:future:{index:05d}", "overlay", f"Future {index}", "world", "canonical",
             f"story/future-{index:05d}.md", None, "", "{}") for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay VALUES (?,?,?,?,?,?,?,?,?,?,?)", (
            (f"overlay:future:{index:05d}", 1000 + index, "changing", '["author"]', '["author"]',
             "main", 100, 0, 200, 0, "{}") for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_location VALUES (?,?,?)", (
            (f"overlay:future:{index:05d}", "location:gate", 0) for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_audience VALUES (?,?,?)", (
            (f"overlay:future:{index:05d}", "author", 0) for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_perspective VALUES (?,?,?)", (
            (f"overlay:future:{index:05d}", "author", 0) for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_overlay_lens_location VALUES (?,?,?,?,?,?,?,?,?,?)", (
            ("location:gate", "author", "author", f"overlay:future:{index:05d}",
             "changing", "main", 100, 0, 200, 0) for index in range(start, stop)))
        connection.execute("DROP TABLE spatial_overlay_time_rtree")
        connection.execute("DROP TABLE spatial_location_rtree")
        assert install_optional_spatial_index(connection) == "rtree"

    def probe(tick: str, order: str = "0") -> int:
        at = {**request, "asOf": {"timeline": "main", "tick": tick, "order": order}}
        outcome, steps = _vm_steps(connection, lambda: _layers(connection, at, store.revision))
        assert outcome[0] == "ok" and len(outcome[1]["layers"]) == (0 if tick != "0" else 1)
        return steps

    insert_future(0, 1000)
    small_future, small_expired, small_order = probe("0"), probe("300"), probe("100", "-1")
    insert_future(1000, 4000)
    large_future, large_expired, large_order = probe("0"), probe("300"), probe("100", "-1")
    assert large_future <= small_future * 2 + 100, (small_future, large_future)
    assert large_expired <= small_expired * 2 + 100, (small_expired, large_expired)
    assert large_order <= small_order * 2 + 100, (small_order, large_order)
    connection.close()


def test_compiled_same_tick_overlay_respects_both_order_boundaries() -> None:
    store = _store(overlay_valid={"start": {"timeline": "main", "tick": 5, "order": 0},
                                  "end": {"timeline": "main", "tick": 5, "order": 1}})
    request = {"protocol": PROTOCOL, "revision": store.revision,
               "capabilities": list(store.capabilities), "limit": 10, "cursor": None,
               "mapId": "map:town", "bounds": {"min": [-1, -1], "max": [1, 1]},
               "relation": "intersects", "audience": "author", "perspective": "author"}
    for order, count in (("-1", 0), ("0", 1), ("1", 1), ("2", 0)):
        state, result, _, _ = _layers(store.connection,
            {**request, "asOf": {"timeline": "main", "tick": "5", "order": order}}, store.revision)
        assert state == "ok" and len(result["layers"]) == count
    store.connection.close()
