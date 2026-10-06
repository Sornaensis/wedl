from __future__ import annotations

from adrai_fixtures import current_decision

from importlib import resources
import json
from pathlib import Path
import shutil

from fastapi.testclient import TestClient

from wedl.server import create_app
from wedl.cli import main
from wedl.repository import Repository
from wedl.compiler import compile_world
from wedl.chronology_api import PROTOCOL, convert_date, format_date, search_annotations, story_times
from wedl.validation import validate_world


def test_release_docs_share_the_local_v06_capability_boundary() -> None:
    root = Path(__file__).resolve().parents[1]
    texts = {name: (current_decision("A01M48RX5WAQT5ECH66KTCFVC0T").read_text(encoding="utf-8") if name == "ARCHITECTURE.md" else (root / "docs" / name).read_text(encoding="utf-8")) for name in ("ARCHITECTURE.md", "CHRONOLOGY_ROLLOUT.md", "CHRONOLOGY_MIGRATION_CONTRACT.md", "CHRONOLOGY_API_CONTRACT.md", "CHRONOLOGY_SCHEMA_CONTRACT.md", "CHRONOLOGY_VALIDATION.md", "HTTP_API.md", "MIGRATION_AND_RECOVERY.md")}
    assert all("v0.6" in text for text in texts.values())
    assert "local" in texts["CHRONOLOGY_ROLLOUT.md"] and "no HTTP" in texts["HTTP_API.md"]
    assert "tick gap never" in texts["CHRONOLOGY_ROLLOUT.md"]
    assert all("deferred" not in text.casefold() for text in texts.values())
    stale = ("planned command", "planned implementation", "later implementation", "becomes executable only when", "once the v0.6 validator/compiler are implemented")
    assert all(phrase not in text.casefold() for text in texts.values() for phrase in stale)


def test_packaged_conformance_fixture_is_valid_and_declares_release_coverage(tmp_path: Path) -> None:
    source = resources.files("wedl.data.chronology_conformance").joinpath("story")
    with resources.as_file(source) as packaged:
        shutil.copytree(packaged, tmp_path / "story")
    world = Repository(tmp_path).load_world()
    assert validate_world(world) == []
    chronology = world.world_record.frontmatter["chronology"]
    assert len(chronology["calendars"]) == 3
    assert len(chronology["eras"]) >= 2
    assert world.world_record.frontmatter["x-conformance-years"] == [-249, 250]
    forms = {next(key for key in item["value"] if not key.startswith("x-")) for record in world.records.values() if record.kind != "world" for item in record.frontmatter.get("chronology", [])}
    assert forms == {"civil", "era", "range", "approx", "conflict", "relative", "duration"}
    civil = [item["value"]["civil"] for record in world.records.values() if record.kind != "world" for item in record.frontmatter.get("chronology", []) if "civil" in item["value"]]
    assert {"calendar_0123456789abcdefghjkmnpqrs", "calendar_1123456789abcdefghjkmnpqrs"} <= {item["calendar_id"] for item in civil}
    assert any(item.get("month") == 2 and item.get("day") == 29 for item in civil)
    assert any(item.get("month") == 13 and item.get("day") == 5 for item in civil)
    ranges = [item["value"]["range"] for record in world.records.values() if record.kind != "world" for item in record.frontmatter.get("chronology", []) if "range" in item["value"]]
    assert any(item["lower"] is not None and item["upper"] is not None for item in ranges)


def test_fixture_public_reads_have_source_and_strict_compiled_parity(tmp_path: Path) -> None:
    source = resources.files("wedl.data.chronology_conformance").joinpath("story")
    with resources.as_file(source) as packaged:
        shutil.copytree(packaged, tmp_path / "story")
    repository = Repository(tmp_path)
    value = {"kind": "civil", "calendarId": "calendar_0123456789abcdefghjkmnpqrs", "year": "0", "month": "1", "day": "1"}
    format_request = {"protocol": PROTOCOL, "value": value}
    convert_request = {"protocol": PROTOCOL, "value": value, "target": {"calendarId": "calendar_1123456789abcdefghjkmnpqrs"}}
    search_request = {"protocol": PROTOCOL, "predicate": "overlaps", "value": value, "limit": 100}
    source_results = (format_date(repository, format_request), convert_date(repository, convert_request), search_annotations(repository, search_request))
    assert source_results[0]["outcome"] == source_results[1]["outcome"] == source_results[2]["outcome"] == "ok"
    compile_world(repository, "WORKTREE")
    strict_results = (format_date(repository, format_request, require_compiled=True), convert_date(repository, convert_request, require_compiled=True), search_annotations(repository, search_request, require_compiled=True))
    assert strict_results == source_results


def test_fixture_mapping_ambiguity_and_query_advisory_are_explicit_source_and_strict(tmp_path: Path) -> None:
    source = resources.files("wedl.data.chronology_conformance").joinpath("story")
    with resources.as_file(source) as packaged:
        shutil.copytree(packaged, tmp_path / "story")
    repository = Repository(tmp_path)
    calendar = "calendar_0123456789abcdefghjkmnpqrs"
    lower = {"calendarId": calendar, "year": "0", "month": "1", "day": "1"}
    upper = {"calendarId": calendar, "year": "250", "month": "1", "day": "1"}
    mapping_request = {"protocol": PROTOCOL, "value": {"kind": "range", "calendarId": calendar, "lower": lower, "upper": upper}}
    query_request = {"protocol": PROTOCOL, "predicate": "overlaps", "value": {"kind": "civil", **lower}, "limit": 100}
    source_mapping, source_query = story_times(repository, mapping_request), search_annotations(repository, query_request)
    assert source_mapping["result"]["mapping"] == "ambiguous"
    assert source_query["advisories"], "relative/conflict claims must remain an explicit query advisory"
    compile_world(repository, "WORKTREE")
    assert story_times(repository, mapping_request, require_compiled=True) == source_mapping
    assert search_annotations(repository, query_request, require_compiled=True) == source_query


def test_fixture_public_outcome_and_advisory_matrix_is_exact_source_and_strict(tmp_path: Path) -> None:
    source = resources.files("wedl.data.chronology_conformance").joinpath("story")
    with resources.as_file(source) as packaged:
        shutil.copytree(packaged, tmp_path / "story")
    repository = Repository(tmp_path)
    solar = "calendar_0123456789abcdefghjkmnpqrs"
    isolated = "calendar_2123456789abcdefghjkmnpqrs"
    broad = {"kind": "range", "calendarId": solar, "lower": {"calendarId": solar, "year": "-249"}, "upper": {"calendarId": solar, "year": "250"}}
    cases = (
        ("invalid", lambda strict: format_date(repository, {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": "calendar_missing", "year": "0"}}, require_compiled=strict), "invalid", "date", ()),
        ("unavailable", lambda strict: convert_date(repository, {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": isolated, "year": "0", "month": "1", "day": "1"}, "target": {"calendarId": solar}}, require_compiled=strict), "unavailable", "no_epoch", ()),
        ("limit-and-advisories", lambda strict: search_annotations(repository, {"protocol": PROTOCOL, "predicate": "overlaps", "value": broad, "limit": 1}, require_compiled=strict), "ok", None, (("approximate-overlap-included", 1), ("noncomparable-excluded", 4), ("result-limit", 8))),
    )
    source_results = []
    for _name, call, outcome, reason, advisories in cases:
        result = call(False)
        assert result["outcome"] == outcome and result.get("reason") == reason
        assert tuple((item["code"], item["count"]) for item in result["advisories"]) == advisories
        source_results.append(result)
    compile_world(repository, "WORKTREE")
    assert tuple(call(True) for _name, call, _outcome, _reason, _advisories in cases) == tuple(source_results)


def test_fixture_validation_mutations_and_cli_http_strict_read_parity(tmp_path: Path, capsys) -> None:
    source = resources.files("wedl.data.chronology_conformance").joinpath("story")
    with resources.as_file(source) as packaged:
        shutil.copytree(packaged, tmp_path / "story")
    repository = Repository(tmp_path)
    world = repository.load_world()
    world.world_record.frontmatter["chronology"]["calendars"][0]["months"] = []
    diagnostics = validate_world(world)
    assert any(item["severity"] == "error" and item["code"].startswith("WDL-CAL-") for item in diagnostics)

    # Fresh source reads, then strict cache reads via both supported clients.
    repository = Repository(tmp_path)
    assert compile_world(repository, "WORKTREE")["status"] == "compiled"
    expected = format_date(repository, {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": "calendar_0123456789abcdefghjkmnpqrs", "year": "0", "month": "1", "day": "1"}}, require_compiled=True)
    request_file = tmp_path / "format.json"
    request_file.write_text(json.dumps({"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": "calendar_0123456789abcdefghjkmnpqrs", "year": "0", "month": "1", "day": "1"}}), encoding="utf-8")
    assert main(["--compact", "chronology", "format", str(request_file), "--repo", str(tmp_path), "--require-compiled"]) == 0
    assert json.loads(capsys.readouterr().out) == expected
    with TestClient(create_app(tmp_path)) as client:
        response = client.post("/api/chronology/format?requireCompiled=true", json=json.loads(request_file.read_text(encoding="utf-8")))
    assert response.status_code == 200 and response.json() == expected
