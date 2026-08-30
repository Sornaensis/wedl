"""Public chronology boundary regression coverage.

These checks deliberately exercise the protocol before any private chronology
store is opened: malformed wire documents are transport errors, while a
well-formed value is left for the compiled store to classify semantically.
"""
from __future__ import annotations

import gc
import jsonschema
import pytest
import json
import importlib.util
import subprocess
from copy import deepcopy
from contextlib import closing
from pathlib import Path
from fastapi.testclient import TestClient

from wedl.api_schemas import components
from wedl.chronology_api import PROTOCOL, catalog, convert_date, format_date, search_annotations, source_value_to_public, story_times
from wedl.cli import main
from wedl.errors import UsageError
from wedl.errors import ChronologyUpgradeRequired, CompileRequired, ConfirmationMismatch, ConfirmationRequired, StaleRevision, ValidationFailed
from wedl.authoring import apply_intent, compile_intent, preview_intent
from wedl.changeset import _replace_operation_references
from wedl.repository import Repository
from wedl.query import show_entity
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.compiler import cache_paths, connect
from wedl.ids import id_from_seed
from wedl.validation import validate_world


CALENDAR = "calendar_0123456789ABCDEFGHJKMNPQRS"

_BENCHMARK_SPEC = importlib.util.spec_from_file_location(
    "chronology_api_benchmark", Path(__file__).parents[1] / "tools" / "benchmark_chronology_index.py",
)
assert _BENCHMARK_SPEC and _BENCHMARK_SPEC.loader
_BENCHMARK = importlib.util.module_from_spec(_BENCHMARK_SPEC)
_BENCHMARK_SPEC.loader.exec_module(_BENCHMARK)


def _validate(name: str, value: object) -> None:
    jsonschema.validate(value, {"components": components(), "$ref": f"#/components/schemas/{name}"})


def _git_v06_repository(tmp_path) -> Repository:
    root = tmp_path / "v06-git"
    root.mkdir()
    _BENCHMARK.build_corpus(root, claims=10, years=(-4, 5))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for arguments in (("init", "-q"), ("config", "user.name", "wedl chronology test"), ("config", "user.email", "chronology@test.invalid"), ("add", "story", ".gitignore"), ("commit", "-qm", "seed chronology")):
        subprocess.run(["git", "-C", str(root), *arguments], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def _replacement_intent(repository: Repository, *, key: str, expected_head: str | None = None) -> dict[str, object]:
    public = catalog(repository)
    world = repository.load_world()
    record = next(item for item in world.records.values() if item.kind != "world")
    return {
        "action": "chronology.replace",
        "expectedHead": expected_head or repository.head(),
        "idempotencyKey": key,
        "change": {
            "catalog": {key: [{field: value for field, value in entry.items() if field not in {"basisId", "hasEpoch"}} for entry in public[key]] for key in ("calendars", "eras", "anchors")},
            "records": [{"record": record.title, "annotations": [{"temporaryId": "$chronology.replaced", "provenance": ["public-test"], "value": {"kind": "civil", "calendarId": _BENCHMARK.SHARED_A, "year": "0", "month": "1", "day": "1"}}]}],
        },
    }


def test_entity_detail_exposes_lossless_public_chronology_annotations(tmp_path) -> None:
    repository = _git_v06_repository(tmp_path)
    world = repository.load_world()
    record, reference = [item for item in world.records.values() if item.kind != "world"][:2]
    opaque = {
        "temporaryId": "keep", "calendar_id": "$calendar.keep", "snake_key": 7,
        "large": 9_007_199_254_740_993, "items": [{"era_id": "$era.keep"}, 8],
    }

    def annotation(seed: str, value: dict[str, object]) -> dict[str, object]:
        return {
            "id": id_from_seed("chronology", f"entity-detail-{seed}"),
            "role": "evidence", "display": seed, "provenance": ["entity-detail"],
            "x-annotation": deepcopy(opaque), "value": value,
        }

    def tagged(kind: str, payload: dict[str, object]) -> dict[str, object]:
        """Exercise distinct and colliding payload/tag extension names."""

        return {
            kind: {**payload, "x-collision": {"channel": "payload", "kind": kind}},
            "x-tag": {"kind": kind, **deepcopy(opaque)},
            "x-collision": {"channel": "tag", "kind": kind},
        }

    endpoint = {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 1, "x-endpoint": deepcopy(opaque)}
    annotations = [
        annotation("civil", tagged("civil", {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 1, "x-civil": deepcopy(opaque)})),
        annotation("era", tagged("era", {"era_id": _BENCHMARK.ERA, "year": 0, "month": 1, "day": 1, "x-era": deepcopy(opaque)})),
        annotation("range", tagged("range", {"calendar_id": _BENCHMARK.SHARED_A, "lower": endpoint, "upper": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 2}, "x-range": deepcopy(opaque)})),
        annotation("approximate", tagged("approx", {"display_value": "around epoch", "bounds": {"lower": deepcopy(endpoint), "upper": None, "x-bounds": deepcopy(opaque)}, "x-approx": deepcopy(opaque)})),
        annotation("qualitative", tagged("approx", {"display_value": "unknown date", "bounds": {"lower": None, "upper": None, "x-bounds": deepcopy(opaque)}, "x-approx": deepcopy(opaque)})),
        annotation("conflict", tagged("conflict", {"claims": [tagged("relative", {"relation": "after", "before_id": reference.id, "x-relative": deepcopy(opaque)}), tagged("duration", {"unit": "day", "value": 1, "x-duration": deepcopy(opaque)})], "x-conflict": deepcopy(opaque)})),
        annotation("relative", tagged("relative", {"relation": "after", "before_id": reference.id, "x-relative": deepcopy(opaque)})),
        annotation("duration-min", tagged("duration", {"unit": "year", "value": -(2 ** 63), "x-duration": deepcopy(opaque)})),
        annotation("duration-safe", tagged("duration", {"unit": "month", "value": 9_007_199_254_740_993, "x-duration": deepcopy(opaque)})),
        annotation("duration-max", tagged("duration", {"unit": "day", "value": 2 ** 63 - 1, "x-duration": deepcopy(opaque)})),
    ]
    frontmatter = deepcopy(record.frontmatter)
    frontmatter["chronology"] = annotations
    (repository.root / record.source_path).write_bytes(serialize_record(frontmatter, record.body))
    subprocess.run(["git", "-C", str(repository.root), "add", record.source_path], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    subprocess.run(["git", "-C", str(repository.root), "commit", "-qm", "entity detail chronology"], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    repository = Repository(repository.root)

    direct = show_entity(repository, record.id)
    _validate("EntityResponse", direct)
    assert direct["frontmatter"] == frontmatter
    assert len(direct["chronologyAnnotations"]) == len(annotations)
    by_display = {item["display"]: item for item in direct["chronologyAnnotations"]}
    assert by_display["civil"]["value"]["year"] == "0"
    assert by_display["era"]["value"]["eraId"] == _BENCHMARK.ERA
    assert by_display["range"]["value"]["lower"]["year"] == "0"
    assert by_display["approximate"]["value"]["bounds"]["calendarId"] == _BENCHMARK.SHARED_A
    assert by_display["qualitative"]["value"]["bounds"] == {"lower": None, "upper": None, "x-bounds": opaque}
    assert "calendarId" not in by_display["qualitative"]["value"]["bounds"]
    assert by_display["conflict"]["value"]["claims"][0]["kind"] == "relative"
    assert {by_display[key]["value"]["value"] for key in ("duration-min", "duration-safe", "duration-max")} == {"-9223372036854775808", "9007199254740993", "9223372036854775807"}
    assert by_display["civil"]["x-annotation"] == by_display["civil"]["value"]["x-civil"] == opaque
    assert by_display["civil"]["value"]["x-civil"]["large"] == 9_007_199_254_740_993
    for display, source_kind in (("civil", "civil"), ("era", "era"), ("range", "range"), ("approximate", "approx"), ("qualitative", "approx"), ("conflict", "conflict"), ("relative", "relative"), ("duration-min", "duration"), ("duration-safe", "duration"), ("duration-max", "duration")):
        public = by_display[display]["value"]
        assert public["x-collision"] == {"channel": "payload", "kind": source_kind}
        assert public["tagExtensions"] == {
            "x-tag": {"kind": source_kind, **opaque},
            "x-collision": {"channel": "tag", "kind": source_kind},
        }
    nested_claims = by_display["conflict"]["value"]["claims"]
    assert nested_claims[0]["tagExtensions"]["x-collision"] == {"channel": "tag", "kind": "relative"}
    assert nested_claims[1]["tagExtensions"]["x-collision"] == {"channel": "tag", "kind": "duration"}

    with TestClient(create_app(repository.root)) as client:
        response = client.get(f"/api/entities/{record.id}")
    assert response.status_code == 200
    assert response.json() == direct
    _validate("EntityResponse", response.json())

    intent = {
        "action": "chronology.replace", "expectedHead": repository.head(),
        "change": {"records": [{"record": record.id, "annotations": direct["chronologyAnnotations"]}]},
    }
    preview = preview_intent(repository, intent)
    assert preview["preview"]["valid"] is True
    operation = next(item for item in preview["changeset"]["operations"] if item["entity"] == record.id)
    assert operation["frontmatterPatch"]["chronology"] == annotations
    applied = apply_intent(repository, intent, confirmation_token_value=preview["preview"]["confirmationToken"])
    assert applied["compile"]["status"] in {"compiled", "cache-hit"}
    reloaded = Repository(repository.root)
    assert reloaded.load_world().get(record.id).frontmatter["chronology"] == annotations
    assert show_entity(reloaded, record.id)["chronologyAnnotations"] == direct["chronologyAnnotations"]
    with pytest.raises(jsonschema.ValidationError):
        _validate("EntityResponse", {**direct, "unexpected": True})


def test_entity_detail_exposes_an_empty_array_for_an_empty_v06_record(tmp_path) -> None:
    repository = _git_v06_repository(tmp_path)
    record = next(item for item in repository.load_world().records.values() if item.kind != "world")
    frontmatter = deepcopy(record.frontmatter)
    frontmatter["chronology"] = []
    (repository.root / record.source_path).write_bytes(serialize_record(frontmatter, record.body))
    subprocess.run(["git", "-C", str(repository.root), "add", record.source_path], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    subprocess.run(["git", "-C", str(repository.root), "commit", "-qm", "entity detail empty chronology"], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    repository = Repository(repository.root)
    direct = show_entity(repository, record.id)
    assert direct["chronologyAnnotations"] == []
    _validate("EntityResponse", direct)
    with TestClient(create_app(repository.root)) as client:
        response = client.get(f"/api/entities/{record.id}")
    assert response.status_code == 200
    assert response.json() == direct


def test_entity_response_schema_preserves_legacy_details_and_rejects_unknown_members(ash_repo) -> None:
    record = next(item for item in ash_repo.load_world().records.values() if item.kind != "world")
    detail = show_entity(ash_repo, record.id)
    assert detail["chronologyAnnotations"] == []
    _validate("EntityResponse", detail)
    with pytest.raises(jsonschema.ValidationError):
        _validate("EntityResponse", {**detail, "unexpected": True})


def test_tag_extensions_schema_is_closed_and_opaque() -> None:
    _validate("ChronologyTagExtensions", {"x-meta": {"temporaryId": "keep", "calendar_id": "$calendar.keep", "number": 7}})
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyTagExtensions", {"metadata": {}})
    assert "tagExtensions" not in source_value_to_public({
        "civil": {"calendar_id": CALENDAR, "year": 0},
    })


def test_public_search_request_couples_between_to_upper_and_closes_fields() -> None:
    date = {"kind": "civil", "calendarId": CALENDAR, "year": "0"}
    _validate("ChronologySearchRequest", {"protocol": PROTOCOL, "predicate": "between", "value": date, "upper": date, "limit": 1})
    for invalid in ({"protocol": PROTOCOL, "predicate": "between", "value": date}, {"protocol": PROTOCOL, "predicate": "on_date", "value": date, "upper": date}, {"protocol": PROTOCOL, "predicate": "on_date", "value": date, "limit": 0}):
        with pytest.raises(jsonschema.ValidationError):
            _validate("ChronologySearchRequest", invalid)


def test_public_mapping_cardinality_is_discriminated() -> None:
    time = {"timeline": "main", "tick": "0", "order": "0"}
    _validate("ChronologyStoryTimeMapping", {"mapping": "none", "storyTimes": []})
    _validate("ChronologyStoryTimeMapping", {"mapping": "unique", "storyTimes": [time]})
    _validate("ChronologyStoryTimeMapping", {"mapping": "ambiguous", "storyTimes": [time, time]})
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyStoryTimeMapping", {"mapping": "unique", "storyTimes": []})


@pytest.mark.parametrize(
    ("schema", "value", "valid"),
    [
        ("ChronologyDecimalI64", "-9223372036854775808", True),
        ("ChronologyDecimalI64", "9223372036854775807", True),
        ("ChronologyDecimalI64", "9223372036854775808", False),
        ("ChronologyDecimalI64", "-9223372036854775809", False),
        ("ChronologyDecimalI32", "-2147483648", True),
        ("ChronologyDecimalI32", "2147483647", True),
        ("ChronologyDecimalI32", "2147483648", False),
        ("ChronologyDecimalI32", "-2147483649", False),
    ],
)
def test_public_decimal_schema_enforces_signed_bounds(schema: str, value: str, valid: bool) -> None:
    if valid:
        _validate(schema, value)
    else:
        with pytest.raises(jsonschema.ValidationError):
            _validate(schema, value)


def test_public_civil_endpoint_requires_month_for_day() -> None:
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyCivilEndpoint", {"year": "0", "day": "1"})


def test_authoring_schema_closes_change_and_annotation_identifiers() -> None:
    annotation = {"temporaryId": "$chronology.note", "provenance": ["ledger"], "value": {"kind": "relative", "relation": "after", "beforeId": "$chronology.previous"}}
    change = {"records": [{"record": "Character", "annotations": [annotation]}]}
    _validate("ChronologyChange", change)
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyChange", {})
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyAnnotation", {**annotation, "id": "chronology_0123456789ABCDEFGHJKMNPQRS"})
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyRelativeValue", {"kind": "relative", "relation": "after"})


def test_changeset_resolves_only_core_chronology_temporary_references() -> None:
    opaque = {
        "temporaryId": "keep",
        "calendar_id": "$calendar.keep",
        "snake_key": 7,
        "items": [{"era_id": "$era.keep"}, 8],
    }
    operation = {
        "type": "entity.update",
        "entity": "world_0123456789ABCDEFGHJKMNPQRS",
        "frontmatterPatch": {
            "chronology": {
                "calendars": [{"temporaryId": "$calendar.core", "x-meta": opaque}],
                "eras": [{"temporaryId": "$era.core", "calendar_id": "$calendar.core", "x-meta": opaque}],
                "anchors": [{"temporaryId": "$chronology.core", "x-meta": opaque}],
            },
        },
    }
    replacements = {
        "$calendar.core": "calendar_0123456789ABCDEFGHJKMNPQRS",
        "$era.core": "era_0123456789ABCDEFGHJKMNPQRS",
        "$chronology.core": "chronology_0123456789ABCDEFGHJKMNPQRS",
        "$calendar.keep": "calendar_SHOULD_NOT_REPLACE",
        "$era.keep": "era_SHOULD_NOT_REPLACE",
    }
    resolved = _replace_operation_references(operation, replacements)["frontmatterPatch"]["chronology"]
    assert resolved["calendars"][0]["id"] == replacements["$calendar.core"]
    assert resolved["eras"][0]["id"] == replacements["$era.core"]
    assert resolved["eras"][0]["calendar_id"] == replacements["$calendar.core"]
    assert resolved["anchors"][0]["id"] == replacements["$chronology.core"]
    for collection in ("calendars", "eras", "anchors"):
        assert resolved[collection][0]["x-meta"] == opaque
        assert resolved[collection][0]["x-meta"] is not opaque


def _nested_conflict(depth: int, *, authoring: bool = False) -> dict[str, object]:
    leaf: dict[str, object] = (
        {"kind": "duration", "unit": "day", "value": "1"}
        if authoring else {"kind": "civil", "calendarId": CALENDAR, "year": "0"}
    )
    companion: dict[str, object] = (
        {"kind": "duration", "unit": "month", "value": "2"}
        if authoring else {"kind": "civil", "calendarId": CALENDAR, "year": "1"}
    )
    for _ in range(depth):
        leaf = {"kind": "conflict", "claims": [leaf, companion]}
    return leaf


def test_public_schema_has_honest_operands_nonblank_ids_and_finite_conflict_depth() -> None:
    conflict = _nested_conflict(1)
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyFormatRequest", {"protocol": PROTOCOL, "value": conflict})
    _validate("ChronologySearchRequest", {"protocol": PROTOCOL, "predicate": "overlaps", "value": conflict})
    for name, value in (("ChronologyCivilDate", {"kind": "civil", "calendarId": " \t", "year": "0"}), ("ChronologyEraDate", {"kind": "era", "eraId": "", "year": "0"}), ("ChronologyConversionTarget", {"calendarId": "\n"})):
        with pytest.raises(jsonschema.ValidationError):
            _validate(name, value)
    _validate("ChronologyDateValue", _nested_conflict(64))
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyDateValue", _nested_conflict(65))
    _validate("ChronologyAuthorValue", _nested_conflict(64, authoring=True))
    with pytest.raises(jsonschema.ValidationError):
        _validate("ChronologyAuthorValue", _nested_conflict(65, authoring=True))


def test_malformed_conversion_target_is_rejected_before_repository_access() -> None:
    request = {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": CALENDAR, "year": "0"}, "target": {"calendarId": 0}}
    with pytest.raises(UsageError):
        convert_date(None, request)


def test_ordinal_catalog_has_direct_cli_http_parity_and_strict_cache(ash_repo, capsys) -> None:
    catalog(ash_repo)  # ordinary read builds the disposable cache
    direct = catalog(ash_repo, require_compiled=True)
    assert direct["capability"]["mode"] == "ordinal-only"
    _validate("ChronologyCatalogResponse", direct)
    with TestClient(create_app(ash_repo.root)) as client:
        response = client.get("/api/chronology", params={"requireCompiled": "true"})
    assert response.status_code == 200
    assert response.json() == direct
    assert main(["chronology", "catalog", "--repo", str(ash_repo.root), "--require-compiled"]) == 0
    assert json.loads(capsys.readouterr().out) == direct


def test_nonempty_v06_catalog_schema_is_closed_and_uses_compiled_projection(tmp_path) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=10, years=(-4, 5))
    repository = Repository(tmp_path)
    response = catalog(repository)
    assert repository.is_git is False
    assert response["capability"]["mode"] == "chronology-enabled"
    assert response["calendars"] and response["eras"] and response["anchors"]
    _validate("ChronologyCatalogResponse", response)


def test_public_catalog_obeys_missing_stale_incompatible_and_invalid_cache_states(tmp_path) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=10, years=(-4, 5))
    repository = Repository(tmp_path)
    with pytest.raises(CompileRequired):
        catalog(repository, require_compiled=True)
    ready = catalog(repository)
    database = cache_paths(repository)[1]
    before_invalid = database.read_bytes()
    world_source = tmp_path / "story" / "world.md"
    world_source.write_bytes(world_source.read_bytes() + b"\n")
    with pytest.raises(CompileRequired):
        catalog(repository, require_compiled=True)
    assert catalog(repository)["revision"] == ready["revision"]
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET sqlite_schema = 'incompatible-public-test'")
        connection.commit()
    with pytest.raises(CompileRequired):
        catalog(repository, require_compiled=True)
    assert catalog(repository)["capability"]["mode"] == "chronology-enabled"
    world = repository.load_world()
    world.world_record.frontmatter["schema"] = "wedl/v0.5"
    world_source.write_bytes(serialize_record(world.world_record.frontmatter, world.world_record.body))
    stale_before_invalid = database.read_bytes()
    with pytest.raises(ValidationFailed):
        catalog(repository)
    assert database.read_bytes() == stale_before_invalid != before_invalid


def test_nonempty_v06_direct_cli_http_parity_for_every_read_route(tmp_path, monkeypatch, capsys) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=10, years=(-4, 5))
    repository = Repository(tmp_path)
    calendar = _BENCHMARK.SHARED_A
    requests = {
        "format": {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": calendar, "year": "0", "month": "1", "day": "1"}},
        "convert": {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": calendar, "year": "0", "month": "1", "day": "1"}, "target": {"calendarId": _BENCHMARK.SHARED_B}},
        "search": {"protocol": PROTOCOL, "predicate": "on_date", "value": {"kind": "civil", "calendarId": calendar, "year": "0", "month": "1", "day": "1"}, "limit": 10},
        "story-times": {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": calendar, "year": "0", "month": "1", "day": "1"}},
    }
    direct_functions = {"format": format_date, "convert": convert_date, "search": search_annotations, "story-times": story_times}
    response_schemas = {"format": "ChronologyFormatOutcome", "convert": "ChronologyConvertOutcome", "search": "ChronologySearchOutcome", "story-times": "ChronologyStoryTimesOutcome"}
    direct = {name: function(repository, request) for name, (function, request) in {name: (direct_functions[name], request) for name, request in requests.items()}.items()}
    assert all(response["outcome"] == "ok" for response in direct.values())
    for name, response in direct.items():
        _validate(response_schemas[name], response)
    with TestClient(create_app(repository.root)) as client:
        for name, request in requests.items():
            response = client.post(f"/api/chronology/{name}", json=request)
            assert response.status_code == 200
            assert response.json() == direct[name]
    for name, request in requests.items():
        file = tmp_path / f"{name}.json"
        file.write_text(json.dumps(request), encoding="utf-8")
        assert main(["chronology", name, str(file), "--repo", str(repository.root)]) == 0
        assert json.loads(capsys.readouterr().out) == direct[name]
    unknown = {**requests["convert"], "target": {"calendarId": "calendar_unknown"}}
    assert convert_date(repository, unknown)["outcome"] == "invalid"
    with TestClient(create_app(repository.root)) as client:
        assert client.post("/api/chronology/convert", json=unknown).json()["outcome"] == "invalid"
        malformed = client.post("/api/chronology/convert", json={**unknown, "target": {"calendarId": ""}})
    assert malformed.status_code == 400
    unknown_file = tmp_path / "unknown-target.json"
    unknown_file.write_text(json.dumps(unknown), encoding="utf-8")
    assert main(["chronology", "convert", str(unknown_file), "--repo", str(repository.root)]) == 0
    assert json.loads(capsys.readouterr().out)["outcome"] == "invalid"
    malformed_file = tmp_path / "malformed-target.json"
    malformed_file.write_text(json.dumps({**unknown, "target": {"calendarId": ""}}), encoding="utf-8")
    assert main(["chronology", "convert", str(malformed_file), "--repo", str(repository.root)]) == 2


def test_conflict_depth_and_blank_identifiers_are_structured_direct_and_http(tmp_path) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=10, years=(-4, 5))
    repository = Repository(tmp_path)
    blank = {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": " \t", "year": "0"}}
    conflict = {"protocol": PROTOCOL, "value": _nested_conflict(1)}
    with pytest.raises(UsageError):
        format_date(repository, blank)
    with pytest.raises(UsageError):
        format_date(repository, conflict)
    accepted = {"protocol": PROTOCOL, "predicate": "overlaps", "value": _nested_conflict(64)}
    over_depth = {"protocol": PROTOCOL, "predicate": "overlaps", "value": _nested_conflict(65)}
    with TestClient(create_app(repository.root)) as client:
        assert client.post("/api/chronology/format", json=blank).status_code == 400
        assert client.post("/api/chronology/format", json=conflict).status_code == 400
        assert client.post("/api/chronology/search", json=accepted).status_code == 200
        rejected = client.post("/api/chronology/search", json=over_depth)
        pathological = client.post("/api/chronology/search", json={"protocol": PROTOCOL, "predicate": "overlaps", "value": _nested_conflict(300)})
    assert rejected.status_code == pathological.status_code == 400
    assert rejected.json()["code"] == pathological.json()["code"] == "usage_error"


def test_authoring_conflict_depth_boundary_and_pathological_input_are_usage_errors(tmp_path) -> None:
    repository = _git_v06_repository(tmp_path)
    accepted = _replacement_intent(repository, key="chronology-depth-accepted")
    accepted["change"]["records"][0]["annotations"][0]["value"] = _nested_conflict(64, authoring=True)
    assert compile_intent(repository, accepted)["operations"]
    rejected = _replacement_intent(repository, key="chronology-depth-rejected")
    rejected["change"]["records"][0]["annotations"][0]["value"] = _nested_conflict(65, authoring=True)
    pathological = _replacement_intent(repository, key="chronology-depth-pathological")
    pathological["change"]["records"][0]["annotations"][0]["value"] = _nested_conflict(300, authoring=True)
    with pytest.raises(UsageError, match="conflict nesting exceeds 64"):
        compile_intent(repository, rejected)
    with pytest.raises(UsageError, match="conflict nesting exceeds 64"):
        compile_intent(repository, pathological)
    with TestClient(create_app(repository.root)) as client:
        token = client.get("/api/session").json()["token"]
        http_rejected = client.post("/api/authoring/preview", json=rejected, headers={"X-Wedl-Token": token})
        http_pathological = client.post("/api/authoring/preview", json=pathological, headers={"X-Wedl-Token": token})
    assert http_rejected.status_code == http_pathological.status_code == 400
    assert http_rejected.json()["code"] == http_pathological.json()["code"] == "usage_error"


def test_story_time_mapping_preserves_runtime_none_unique_and_ambiguous_cardinality(tmp_path) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=10, years=(-4, 5))
    initial = Repository(tmp_path)
    world_record = initial.load_world().world_record
    world_record.frontmatter["chronology"]["anchors"].append({
        "id": "chronology_1123456789abcdefghjkmnpqrs", "axis_day": 30,
        "story_time": {"timeline": "main", "tick": 1, "order": 0}, "provenance": ["public-test"],
    })
    (tmp_path / "story" / "world.md").write_bytes(serialize_record(world_record.frontmatter, world_record.body))
    repository = Repository(tmp_path)
    calendar = _BENCHMARK.SHARED_A
    value = lambda year, day=None: {"kind": "civil", "calendarId": calendar, "year": str(year), **({"month": "1", "day": str(day)} if day is not None else {})}
    endpoint = lambda year, day: {"calendarId": calendar, "year": str(year), "month": "1", "day": str(day)}
    unique = story_times(repository, {"protocol": PROTOCOL, "value": value(0, 1)})
    none = story_times(repository, {"protocol": PROTOCOL, "value": value(2, 1)})
    ambiguous = story_times(repository, {"protocol": PROTOCOL, "value": {"kind": "range", "calendarId": calendar, "lower": endpoint(0, 1), "upper": endpoint(1, 1)}})
    assert unique["result"]["mapping"] == "unique"
    assert none["result"]["mapping"] == "none"
    assert ambiguous["result"]["mapping"] == "ambiguous"
    for outcome in (unique, none, ambiguous):
        _validate("ChronologyStoryTimesOutcome", outcome)


def test_public_search_reports_every_runtime_advisory_in_deterministic_order(tmp_path) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=10, years=(-4, 5))
    initial = Repository(tmp_path)
    world = initial.load_world()
    records = sorted((item for item in world.records.values() if item.kind != "world"), key=lambda item: item.id)
    record, referenced = records[:2]
    record.frontmatter["chronology"].extend([
        {"id": id_from_seed("chronology", "public-advisory-exact"), "provenance": ["public-test"], "value": {"civil": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 1}}},
        {"id": id_from_seed("chronology", "public-advisory-approx"), "provenance": ["public-test"], "value": {"approx": {"display_value": "about zero", "bounds": {"lower": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 1}, "upper": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 2}}}}},
        {"id": id_from_seed("chronology", "public-advisory-conflict"), "provenance": ["public-test"], "value": {"conflict": {"claims": [{"civil": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 1}}, {"relative": {"relation": "after", "before_id": referenced.id}}]}}},
    ])
    (tmp_path / record.source_path).write_bytes(serialize_record(record.frontmatter, record.body))
    repository = Repository(tmp_path)
    date = {"kind": "civil", "calendarId": _BENCHMARK.SHARED_A, "year": "0", "month": "1", "day": "1"}
    overlap = search_annotations(repository, {"protocol": PROTOCOL, "predicate": "overlaps", "value": date, "limit": 1})
    before = search_annotations(repository, {"protocol": PROTOCOL, "predicate": "before", "value": {**date, "year": "1"}, "limit": 10})
    assert [item["code"] for item in overlap["advisories"]] == ["approximate-overlap-included", "noncomparable-excluded", "result-limit"]
    assert [item["code"] for item in before["advisories"]] == ["approximate-relation-excluded", "noncomparable-excluded"]
    _validate("ChronologySearchOutcome", overlap)
    _validate("ChronologySearchOutcome", before)


def test_chronology_replace_preview_confirmation_apply_compile_stale_and_replay(tmp_path, capsys) -> None:
    repository = _git_v06_repository(tmp_path)
    intent = _replacement_intent(repository, key="chronology-public-replace-v1")
    opaque = {
        "temporaryId": "keep",
        "calendar_id": "$calendar.keep",
        "snake_key": 7,
        "items": [{"era_id": "$era.keep"}, 8],
    }
    calendar = next(item for item in intent["change"]["catalog"]["calendars"] if item["id"] == _BENCHMARK.SHARED_B)
    calendar["temporaryId"] = "$calendar.keep"
    del calendar["id"]
    era = intent["change"]["catalog"]["eras"][0]
    era["temporaryId"] = "$era.keep"
    del era["id"]
    annotation_value = intent["change"]["records"][0]["annotations"][0]["value"]
    intent["change"]["catalog"]["calendars"][0]["x-calendar"] = {"year": "opaque"}
    intent["change"]["catalog"]["eras"][0]["x-era"] = {"month": "opaque"}
    intent["change"]["catalog"]["anchors"][0]["x-anchor"] = {"day": "opaque"}
    intent["change"]["records"][0]["annotations"][0]["x-annotation"] = {"year": "opaque"}
    intent["change"]["records"][0]["annotations"][0]["value"]["x-value"] = {"month": "opaque"}
    calendar["x-meta"] = opaque
    era["x-meta"] = opaque
    intent["change"]["catalog"]["anchors"][0]["x-meta"] = opaque
    intent["change"]["records"][0]["annotations"][0]["x-meta"] = opaque
    annotation_value["x-meta"] = opaque
    _validate("ChronologyAuthoringRequest", intent)
    with pytest.raises(UsageError):
        compile_intent(repository, {**intent, "unexpected": True})
    change_file = tmp_path / "replace.json"
    change_file.write_text(json.dumps(intent["change"]), encoding="utf-8")
    from wedl.cli import main
    from fastapi.testclient import TestClient
    with TestClient(create_app(repository.root)) as client:
        token = client.get("/api/session").json()["token"]
        http_preview = client.post("/api/authoring/preview", json=intent, headers={"X-Wedl-Token": token})
        explicit_null = client.post("/api/authoring/preview", json={**intent, "summary": None}, headers={"X-Wedl-Token": token})
    assert http_preview.status_code == 200
    assert explicit_null.status_code == 400
    _validate("AuthoringPreviewResponse", http_preview.json())
    assert http_preview.json()["preview"]["valid"] is True
    assert {operation["type"] for operation in http_preview.json()["changeset"]["operations"]} == {"entity.update"}
    assert main(["author", "chronology", "replace", str(change_file), "--repo", str(repository.root), "--expected-head", str(intent["expectedHead"]), "--idempotency-key", str(intent["idempotencyKey"])]) == 0
    cli_preview = json.loads(capsys.readouterr().out)
    _validate("AuthoringPreviewResponse", cli_preview)
    assert "summary" not in cli_preview["intent"]
    preview = preview_intent(repository, intent)
    assert preview["preview"]["valid"] is True
    assert {operation["type"] for operation in preview["changeset"]["operations"]} == {"entity.update"}
    assert "$chronology.replaced" in preview["preview"]["generatedIds"]
    calendar_id = preview["preview"]["generatedIds"]["$calendar.keep"]
    era_id = preview["preview"]["generatedIds"]["$era.keep"]
    world_operation = next(operation for operation in preview["changeset"]["operations"] if operation["entity"] == repository.load_world().world_record.id)
    preview_catalog = world_operation["frontmatterPatch"]["chronology"]
    preview_calendar = next(item for item in preview_catalog["calendars"] if item.get("temporaryId") == "$calendar.keep")
    preview_era = next(item for item in preview_catalog["eras"] if item.get("temporaryId") == "$era.keep")
    assert preview_calendar["x-meta"] == opaque
    assert preview_era["x-meta"] == opaque
    assert preview_era["x-meta"]["calendar_id"] == "$calendar.keep"
    token = preview["preview"]["confirmationToken"]
    head = repository.head()
    with pytest.raises(ConfirmationRequired):
        apply_intent(repository, intent)
    assert repository.head() == head
    with pytest.raises(ConfirmationMismatch):
        apply_intent(repository, intent, confirmation_token_value="wedl-confirmation/v1:wrong")
    assert repository.head() == head
    applied = apply_intent(repository, intent, confirmation_token_value=token)
    assert applied["idempotentReplay"] is False
    assert applied["compile"]["status"] in {"compiled", "cache-hit"}
    public_catalog = catalog(repository, require_compiled=True)
    assert public_catalog["capability"]["mode"] == "chronology-enabled"
    _validate("ChronologyCatalogResponse", public_catalog)
    assert next(item for item in public_catalog["calendars"] if item["id"] == calendar_id)["x-meta"] == opaque
    assert next(item for item in public_catalog["eras"] if item["id"] == era_id)["x-meta"] == opaque
    assert public_catalog["anchors"][0]["x-meta"] == opaque
    stored = repository.load_world()
    assert stored.world_record.frontmatter["chronology"]["calendars"][0]["x-calendar"] == {"year": "opaque"}
    assert stored.world_record.frontmatter["chronology"]["eras"][0]["x-era"] == {"month": "opaque"}
    assert stored.world_record.frontmatter["chronology"]["anchors"][0]["x-anchor"] == {"day": "opaque"}
    updated_record = stored.get(next(operation["entity"] for operation in preview["changeset"]["operations"] if operation["entity"] != stored.world_record.id))
    assert updated_record.frontmatter["chronology"][0]["x-annotation"] == {"year": "opaque"}
    assert updated_record.frontmatter["chronology"][0]["value"]["civil"]["x-value"] == {"month": "opaque"}
    assert next(item for item in stored.world_record.frontmatter["chronology"]["calendars"] if item["id"] == calendar_id)["x-meta"] == opaque
    assert stored.world_record.frontmatter["chronology"]["eras"][0]["x-meta"] == opaque
    assert stored.world_record.frontmatter["chronology"]["anchors"][0]["x-meta"] == opaque
    assert updated_record.frontmatter["chronology"][0]["x-meta"] == opaque
    assert updated_record.frontmatter["chronology"][0]["value"]["civil"]["x-meta"] == opaque
    public_search = search_annotations(repository, {"protocol": PROTOCOL, "predicate": "on_date", "value": {"kind": "civil", "calendarId": _BENCHMARK.SHARED_A, "year": "0", "month": "1", "day": "1"}, "limit": 100})
    _validate("ChronologySearchOutcome", public_search)
    matching = next(item for item in public_search["result"]["matches"] if item["annotationId"] == preview["preview"]["generatedIds"]["$chronology.replaced"])
    assert matching["value"]["x-meta"] == opaque
    replay = apply_intent(repository, intent, confirmation_token_value=token)
    assert replay == {**applied, "idempotentReplay": True}
    stale = _replacement_intent(repository, key="chronology-public-stale-v1", expected_head=head)
    with pytest.raises(StaleRevision):
        apply_intent(repository, stale, allow_unconfirmed=True)


def test_chronology_replace_approximate_bounds_compile_preview_apply_and_reload(tmp_path) -> None:
    repository = _git_v06_repository(tmp_path)
    world = repository.load_world()
    record = next(item for item in world.records.values() if item.kind != "world")
    opaque = {"temporaryId": "keep", "calendar_id": "$calendar.keep", "snake_key": 7, "items": [{"era_id": "$era.keep"}, 8]}

    def approximate(name, lower, upper):
        bounds = {"lower": lower, "upper": upper}
        if lower is not None or upper is not None:
            bounds["calendarId"] = _BENCHMARK.SHARED_A
        return {
            "temporaryId": f"$chronology.{name}", "provenance": ["approximate-shape"],
            "value": {
                "kind": "approximate", "displayValue": name, "x-meta": opaque,
                "bounds": bounds,
            },
        }

    lower = {"year": "0", "month": "1", "day": "1", "x-endpoint": {"calendar_id": "opaque"}}
    upper = {"year": "1", "month": "1", "day": "1", "x-endpoint": {"era_id": "opaque"}}
    annotations = [
        approximate("bounded", lower, upper),
        approximate("lower-only", lower, None),
        approximate("upper-only", None, upper),
        approximate("qualitative", None, None),
        {
            "temporaryId": "$chronology.nested", "provenance": ["approximate-shape"],
            "value": {"kind": "conflict", "x-meta": opaque, "claims": [
                approximate("nested", lower, None)["value"],
                {"kind": "duration", "unit": "day", "value": "1"},
            ]},
        },
    ]
    intent = {
        "action": "chronology.replace", "expectedHead": repository.head(), "idempotencyKey": "approximate-source-shapes",
        "change": {"records": [{"record": record.title, "annotations": annotations}]},
    }
    compiled = compile_intent(repository, intent)
    compiled_annotations = next(operation for operation in compiled["operations"] if operation["entity"] == record.id)["frontmatterPatch"]["chronology"]
    preview = preview_intent(repository, intent)
    assert preview["preview"]["valid"] is True
    assert compiled_annotations
    confirmation = preview["preview"]["confirmationToken"]
    applied = apply_intent(repository, intent, confirmation_token_value=confirmation)
    assert applied["compile"]["status"] in {"compiled", "cache-hit"}
    reloaded = repository.load_world()
    assert validate_world(reloaded) == []
    for annotation in reloaded.get(record.id).frontmatter["chronology"]:
        value = annotation["value"]
        approximate_value = value.get("approx") or value["conflict"]["claims"][0]["approx"]
        assert approximate_value["x-meta"] == opaque
        assert set(approximate_value["bounds"]) == {"lower", "upper"}
        assert "calendar_id" not in approximate_value["bounds"]
        for endpoint in approximate_value["bounds"].values():
            if endpoint is not None:
                assert endpoint["calendar_id"] == _BENCHMARK.SHARED_A


def test_approximate_bound_extensions_never_establish_the_public_calendar(tmp_path) -> None:
    repository = _git_v06_repository(tmp_path)
    record = next(item for item in repository.load_world().records.values() if item.kind != "world")
    calendar_looking = {
        "calendar_id": _BENCHMARK.SHARED_B,
        "nested": {"calendar_id": _BENCHMARK.ISOLATED, "temporaryId": "keep"},
    }
    lower = {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 1}
    upper = {"calendar_id": _BENCHMARK.SHARED_A, "year": 1, "month": 1, "day": 1}

    def annotation(name, bounds):
        return {
            "id": id_from_seed("chronology", f"bound-extension-{name}"),
            "display": name, "provenance": ["bound-extension"],
            "value": {"approx": {"display_value": name, "bounds": bounds}},
        }

    # Both extensions deliberately precede core endpoints in source insertion
    # order.  The lower-only case would previously index the scalar; upper-only
    # could select the misleading calendar-looking extension instead.
    annotations = [
        annotation("lower-only", {
            "x-scalar": 7, "x-calendar-looking": deepcopy(calendar_looking),
            "lower": lower, "upper": None,
        }),
        annotation("upper-only", {
            "x-calendar-looking": deepcopy(calendar_looking), "x-scalar": [7, {"era_id": "keep"}],
            "lower": None, "upper": upper,
        }),
        annotation("qualitative", {
            "x-scalar": 7, "x-calendar-looking": deepcopy(calendar_looking),
            "lower": None, "upper": None,
        }),
    ]
    source = repository.root / record.source_path
    frontmatter = deepcopy(record.frontmatter)
    frontmatter["chronology"] = annotations
    source.write_bytes(serialize_record(frontmatter, record.body))
    subprocess.run(["git", "-C", str(repository.root), "add", record.source_path], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    subprocess.run(["git", "-C", str(repository.root), "commit", "-qm", "approximate bound extensions"], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    repository = Repository(repository.root)

    direct = show_entity(repository, record.id)
    public = {item["display"]: item["value"] for item in direct["chronologyAnnotations"]}
    for name, endpoint in (("lower-only", "lower"), ("upper-only", "upper")):
        assert public[name]["bounds"]["calendarId"] == _BENCHMARK.SHARED_A
        assert public[name]["bounds"][endpoint]["calendarId"] == _BENCHMARK.SHARED_A
        assert public[name]["bounds"]["x-calendar-looking"] == calendar_looking
    assert public["lower-only"]["bounds"]["x-scalar"] == 7
    assert public["upper-only"]["bounds"]["x-scalar"] == [7, {"era_id": "keep"}]
    assert public["qualitative"]["bounds"] == {
        "lower": None, "upper": None, "x-scalar": 7,
        "x-calendar-looking": calendar_looking,
    }
    assert "calendarId" not in public["qualitative"]["bounds"]

    intent = {
        "action": "chronology.replace", "expectedHead": repository.head(),
        "change": {"records": [{"record": record.id, "annotations": direct["chronologyAnnotations"]}]},
    }
    preview = preview_intent(repository, intent)
    assert preview["preview"]["valid"] is True
    operation = next(item for item in preview["changeset"]["operations"] if item["entity"] == record.id)
    assert operation["frontmatterPatch"]["chronology"] == annotations
    # ``sqlite3.Connection`` context managers commit but do not close on
    # Windows.  Force disposal of transient read/preview connections before
    # the confirmed apply atomically replaces the compiled cache file.
    gc.collect()
    apply_intent(repository, intent, confirmation_token_value=preview["preview"]["confirmationToken"])
    reloaded = Repository(repository.root)
    assert reloaded.load_world().get(record.id).frontmatter["chronology"] == annotations
    reloaded_detail = show_entity(reloaded, record.id)
    assert reloaded_detail["chronologyAnnotations"] == direct["chronologyAnnotations"]
    with TestClient(create_app(reloaded.root)) as client:
        response = client.get(f"/api/entities/{record.id}")
    assert response.status_code == 200
    assert response.json()["chronologyAnnotations"] == reloaded_detail["chronologyAnnotations"]


def test_chronology_replace_invalid_and_legacy_fail_before_writes(tmp_path, ash_repo) -> None:
    repository = _git_v06_repository(tmp_path)
    invalid = _replacement_intent(repository, key="chronology-public-invalid-v1")
    invalid["change"] = {"catalog": {"calendars": [], "eras": [], "anchors": []}}
    world_source = repository.root / "story" / "world.md"
    before = world_source.read_bytes()
    with pytest.raises(Exception):
        apply_intent(repository, invalid, allow_unconfirmed=True)
    assert world_source.read_bytes() == before
    legacy = {"action": "chronology.replace", "expectedHead": ash_repo.head(), "change": {"catalog": {"calendars": [], "eras": [], "anchors": []}}}
    with pytest.raises(ChronologyUpgradeRequired) as failure:
        compile_intent(ash_repo, legacy)
    assert failure.value.as_dict()["details"] == {"code": "WDL-MIG-V06-004", "sourceSchema": ash_repo.load_world().schema, "upgradeAvailable": False}
    with TestClient(create_app(ash_repo.root)) as client:
        token = client.get("/api/session").json()["token"]
        preview = client.post("/api/authoring/preview", json=legacy, headers={"X-Wedl-Token": token})
        apply = client.post("/api/authoring/apply", json=legacy, headers={"X-Wedl-Token": token, "X-Wedl-Confirmation": "wedl-confirmation/v1:wrong"})
        openapi = client.get("/openapi.json").json()
    assert preview.status_code == apply.status_code == 400
    assert preview.json()["code"] == apply.json()["code"] == "upgrade_required"
    for operation in (openapi["paths"]["/api/authoring/preview"]["post"], openapi["paths"]["/api/authoring/apply"]["post"]):
        assert "upgrade_required" in operation["responses"]["400"]["x-wedl-error-codes"]
