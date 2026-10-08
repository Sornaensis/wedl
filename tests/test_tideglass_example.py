"""Native Tideglass examples exercise bounded, current production operations."""
from __future__ import annotations

from collections import Counter
from contextlib import closing
from importlib import resources
from pathlib import Path
import json
import stat

from fastapi.testclient import TestClient
import pytest

from wedl import changeset
from wedl.cli import initialize, main
from wedl.compiler import cache_readiness, compile_world, connect, world_from_database
from wedl.context import build_context
from wedl.errors import CompileRequired, ConfirmationMismatch, StaleRevision, UsageError
from wedl.generational_api import execute as generational_execute
from wedl.ids import id_from_seed
from wedl.model import StoryTime
from wedl.query import conversation_view, entity_state, knowledge, search_world
from wedl.repository import Repository
from wedl.semantics import current_relationship, resolve_state
from wedl.server import create_app
from wedl.source import parse_record
from wedl.spatial_api import execute as spatial_execute
from wedl.spatial_query import SQLiteSpatialStore
from wedl.validation import validate_world


CAPABILITIES = ["generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"]
# Frozen content-contract identities and portable paths, independent of loading.
ROSTER = [
    ('affiliation', 'affiliation_3163J7T1VFYEY88H175PSMKJR5', 'story/affiliations/affiliation_3163J7T1VFYEY88H175PSMKJR5.md'),
    ('affiliation', 'affiliation_6X7F5RXMG784DJX9VCVJR72HZD', 'story/affiliations/affiliation_6X7F5RXMG784DJX9VCVJR72HZD.md'),
    ('character', 'char_46VTBJZ9TNMTF7G14N5GKYQ9C1', 'story/characters/cora-finch.md'),
    ('character', 'char_6D246WN3J75HBRJJXGQ5H6QAD4', 'story/characters/jessa-quill.md'),
    ('character', 'char_4JVNDN6QW0BAFFV379AJJSMEPZ', 'story/characters/mina-vale.md'),
    ('character', 'char_62PCSFAX4YQE48JK476XKGG6B8', 'story/characters/orin-reed.md'),
    ('character', 'char_39E9EWWC017E40519033S4RW4X', 'story/characters/sela-vale.md'),
    ('character', 'char_0PYVCG3ETXZ640BWJ5JMT6XXFZ', 'story/characters/tavi-moss.md'),
    ('claim', 'claim_289DFQ0S7Z2YDEFBJXY2H5YPXH', 'story/claims/claim_289DFQ0S7Z2YDEFBJXY2H5YPXH.md'),
    ('conversation', 'conv_3G4YJGZW7NHJ2NB5PESMV0A6S5', 'story/conversations/quay-log-review.md'),
    ('conversation', 'conv_4YFR36RGTKN7VE0R5XQFBKH6QT', 'story/conversations/workshop-repair-decision.md'),
    ('environment', 'env_28ET043JHFN0CMZEYH1S9Y0B3K', 'story/environments/quay-flood.md'),
    ('environment', 'env_07N1ZF3XBMMCJGVBB4N21QG93Y', 'story/environments/storm-weather.md'),
    ('event', 'event_0AP9C85TAREDZ7F84QNM9SNQZ5', 'story/events/main/beacon-lit.md'),
    ('event', 'event_6FTMM1W0HX7G8HNWA313JBD2XE', 'story/events/main/ferry-resumed.md'),
    ('event', 'event_2WRNKX74A1ZC8WJZ6GYXVFSXFR', 'story/events/main/lens-repaired.md'),
    ('event', 'event_22CNFT9Q1FNTR7039D5A324D4X', 'story/events/main/log-discovery.md'),
    ('event', 'event_3M1TJPZ4845X2J6V5QASPFZK4G', 'story/events/main/missing-log.md'),
    ('event', 'event_6SVV6PN3XW92SE3X4SCRQ5GWQA', 'story/events/main/route-inspected.md'),
    ('event', 'event_63W6ZYRM67EZ5VAB3PR5R19TFC', 'story/events/main/storm-damage.md'),
    ('event', 'event_7J9JDA9939Y1NJ08HZWE7RRCQE', 'story/events/main/workshop-arrival.md'),
    ('hypothesis', 'hyp_7CDNS4KZ3WNHDHW716KE9W6PN1', 'story/hypotheses/log-transcription-error.md'),
    ('parentage', 'kinship_6FMPV333HWER1D3NXKSSNP6AZ9', 'story/kinships/kinship_6FMPV333HWER1D3NXKSSNP6AZ9.md'),
    ('knowledge', 'know_5WR6NRNE78SCKKZ0FPAEYR61T5', 'story/knowledge/ferry-suspension.md'),
    ('knowledge', 'know_0XNM8BDXPVH3XAEMC1STDN9VMB', 'story/knowledge/log-absence.md'),
    ('knowledge', 'know_0WYS8PMTPS0KNEX3SCQZ30E6JF', 'story/knowledge/log-discrepancy.md'),
    ('knowledge', 'know_7EJZ8JXCFWVX6DEX37AY8N62KY', 'story/knowledge/repair-plan.md'),
    ('knowledge', 'know_16HV4MMSQFV7RVFESSE8GQEKJ0', 'story/knowledge/return-authorized.md'),
    ('knowledge', 'know_33RB1GJYPCGCPDA9KYT8GN4PN9', 'story/knowledge/storm-report.md'),
    ('legacy', 'legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ', 'story/legacies/legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ.md'),
    ('location', 'loc_2SYYRXM9AHJGKYKGG200TZV82Y', 'story/locations/archive.md'),
    ('location', 'loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ', 'story/locations/beacon.md'),
    ('location', 'loc_3VDY39471DED5V26TEVYXMRQK3', 'story/locations/ferry-house.md'),
    ('location', 'loc_7F7G4Y2CZAD32HYW0B28N61Q72', 'story/locations/marsh-road.md'),
    ('location', 'loc_0708459BXR2PVVHA4VASXHB69C', 'story/locations/quay.md'),
    ('location', 'loc_2DW2BZ5PN3XBMD2FWQA4Q61V1E', 'story/locations/tideglass-town.md'),
    ('location', 'loc_117ZGNWJM0Y3N7CWER2H3XK722', 'story/locations/workshop.md'),
    ('map', 'map:tideglass-town', 'story/maps/tideglass-town.md'),
    ('object', 'obj_13BZCJF8R08S5RSH4VRJXMA0YK', 'story/objects/ferry-key.md'),
    ('object', 'obj_0NQWK9WPK4PSR5722YH0RJ89CH', 'story/objects/maintenance-logbook.md'),
    ('object', 'obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D', 'story/objects/signal-lens.md'),
    ('object', 'obj_76VC9SVNSR7ASZK025X7PTRBJH', 'story/objects/survey-staff.md'),
    ('organization', 'organization_4YNRVZSRYTGBB27YKD7NW1T7CZ', 'story/organizations/organization_4YNRVZSRYTGBB27YKD7NW1T7CZ.md'),
    ('organization', 'organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG', 'story/organizations/organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG.md'),
    ('overlay', 'overlay:flood-warning', 'story/overlays/flood-warning.md'),
    ('relationship', 'rel_600QPBZBZVZJ96C0YX59790KNM', 'story/relationships/cora-mina-obligation.md'),
    ('relationship', 'rel_0Y5PE3MEG7ZK91Z24B1F6G1T31', 'story/relationships/jessa-sela-trust.md'),
    ('relationship', 'rel_7Y1Y0Z51JTGRZ02BTV2K31ERNA', 'story/relationships/mina-tavi-trust.md'),
    ('relationship', 'rel_4WEP0KV5C79HNMY7J2QTN67BY1', 'story/relationships/orin-jessa-trust.md'),
    ('relationship', 'rel_4QMDM336YCMY6TT554VXDFQHPE', 'story/relationships/tavi-orin-obligation.md'),
    ('route', 'route:beacon-quay', 'story/routes/beacon-quay.md'),
    ('route', 'route:quay-beacon', 'story/routes/quay-beacon.md'),
    ('scene', 'scene_098KPQFBY472Q3ZJDPN6NTA8B2', 'story/scenes/archive-consultation.md'),
    ('scene', 'scene_7H983H74J7N5W9QRF3496GBXDH', 'story/scenes/beacon-relight.md'),
    ('scene', 'scene_0ZF3ST1ZH9RVWF23T7ZR002ZMT', 'story/scenes/ferry-return.md'),
    ('scene', 'scene_0M3G43J3775Q54BHQJH4KRWQCX', 'story/scenes/quay-assessment.md'),
    ('scene', 'scene_0QXK1AT5DC93TB0YZSB1KAEQVC', 'story/scenes/workshop-assessment.md'),
    ('story-point', 'sp_2CQKBF1PHXAEWR8ST4CM7PG5HT', 'story/story-points/account-for-log.md'),
    ('story-point', 'sp_1ZTN64D3SV94044DGY1JCK7AXN', 'story/story-points/repair-beacon.md'),
    ('story-point', 'sp_3Z4N9HS0W9N4NJDP95KFXZVFZD', 'story/story-points/restore-ferry.md'),
    ('tenure', 'tenure_7XHCEMFF7JDR55YP3NCW76C2F0', 'story/tenures/tenure_7XHCEMFF7JDR55YP3NCW76C2F0.md'),
    ('union', 'union_04W52R482EPFAG76MX1Y0DK2M1', 'story/unions/union_04W52R482EPFAG76MX1Y0DK2M1.md'),
    ('vital-history', 'vital_7YPSPVFBM15VGK0NS739PNBQ18', 'story/vitals/vital_7YPSPVFBM15VGK0NS739PNBQ18.md'),
    ('world', 'world_7XGTHK671HA355KN17X2HRET5G', 'story/world.md'),
]


def _id(kind: str, slug: str) -> str:
    return id_from_seed(kind, f"tideglass-demo-v1/{kind}/{slug}")


def _story_bytes(root: Path) -> dict[str, bytes]:
    return {path.relative_to(root).as_posix(): path.read_bytes()
            for path in (root / "story").rglob("*.md")}


@pytest.fixture(scope="module")
def tideglass_repo(tmp_path_factory: pytest.TempPathFactory) -> Repository:
    # Git initialization is the existing product operation on this disposable copy.
    root = tmp_path_factory.mktemp("tideglass-native") / "repo"
    report = initialize(root, example="tideglass", profile_name="fts")
    assert report["example"] == "tideglass" and report["compile"]["recordCount"] == 64
    return Repository(root)


def test_tideglass_01_manifest_validation_and_cold_rebuild(tideglass_repo: Repository) -> None:
    repository = tideglass_repo
    package = resources.files("wedl.data.tideglass_v07")
    expected = {path: (kind, identifier) for kind, identifier, path in ROSTER}
    files = _story_bytes(repository.root)
    assert set(files) == set(expected) and len(files) == 64
    assert sum(map(len, files.values())) <= 512 * 1024
    assert sum(len(parse_record(data, path).body.split()) for path, data in files.items()) <= 15000
    for path, data in files.items():
        assert package.joinpath(path).read_bytes() == data
        record = parse_record(data, path)
        assert (record.kind, record.id) == expected[path]
        assert not (repository.root / path).is_symlink()
        assert not getattr((repository.root / path).stat(), "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    world = repository.load_world(cache_write=False)
    assert world.schema == "wedl/v0.7" and world.config["capabilities"] == CAPABILITIES
    assert not validate_world(world)
    assert Counter(record.kind for record in world.records.values()) == Counter(kind for kind, _, _ in ROSTER)
    database = repository.root / ".wedl/world.sqlite"
    projected = world_from_database(repository, database)
    assert set(projected.records) == set(world.records)
    for identifier, source in world.records.items():
        compiled = projected.get(identifier)
        assert (compiled.frontmatter, compiled.body, compiled.source_path) == (source.frontmatter, source.body, source.source_path)
    # Remove only this generated database in the disposable fixture; source/HEAD stay.
    original_head = repository.head()
    database.unlink()
    assert cache_readiness(repository)["state"] == "missing"
    with pytest.raises(CompileRequired):
        entity_state(repository, _id("object", "signal-lens"), 30, order=0, require_compiled=True)
    assert _story_bytes(repository.root) == files and repository.head() == original_head
    rebuilt = compile_world(repository, profile_name="fts")
    assert rebuilt["recordCount"] == 64 and cache_readiness(repository)["state"] == "ready"
    rebuilt_world = world_from_database(repository, database)
    for identifier, source in world.records.items():
        rebuilt_record = rebuilt_world.get(identifier)
        assert (rebuilt_record.frontmatter, rebuilt_record.body, rebuilt_record.source_path) == (source.frontmatter, source.body, source.source_path)
    with closing(connect(database)) as connection:
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []


def test_tideglass_02_causes_and_state_cli_http(tideglass_repo: Repository, capsys) -> None:
    repository = tideglass_repo
    world = repository.load_world(cache_write=False)
    lens = _id("object", "signal-lens")
    with TestClient(create_app(repository.root)) as client:
        for tick, condition, location, cause, key in [
            (10, "cracked", "beacon", "storm-damage", "condition"),
            (20, "cracked", "workshop", "workshop-arrival", "location"),
            (30, "repaired", "workshop", "lens-repaired", "condition"),
            (40, "repaired", "beacon", "beacon-lit", "location"),
        ]:
            source, citations = resolve_state(world, lens, StoryTime("main", tick, 0))
            value = entity_state(repository, lens, tick, order=0, require_compiled=True)
            assert value["state"] == source == {"condition": condition, "location": {"entity": _id("location", location)}}
            assert value["citations"] == citations and citations[key]["entityId"] == _id("event", cause)
            assert main(["--compact", "state", lens, "--repo", str(repository.root), "--tick", str(tick), "--order", "0", "--require-compiled"]) == 0
            assert json.loads(capsys.readouterr().out) == value
            response = client.get(f"/api/entities/{lens}/state", params={"tick": tick, "order": 0, "requireCompiled": "true"})
            assert response.status_code == 200 and response.json() == value
    trust = _id("relationship", "mina-tavi-trust")
    assert current_relationship(world, trust, StoryTime("main", 20, 0))["metrics"]["trust"] == 0.25
    assert current_relationship(world, trust, StoryTime("main", 30, 0))["metrics"]["trust"] == 0.75


def test_tideglass_03_same_tick_private_knowledge(tideglass_repo: Repository) -> None:
    repository = tideglass_repo
    private = _id("knowledge", "log-discrepancy")
    future = _id("knowledge", "return-authorized")
    for character, order, scene, present in [
        ("jessa-quill", 1, "quay-assessment", False),
        ("jessa-quill", 2, "archive-consultation", True),
        ("orin-reed", 1, "quay-assessment", False),
        ("orin-reed", 2, "quay-assessment", False),
    ]:
        character_id = _id("character", character)
        scene_id = _id("scene", scene)
        values = knowledge(repository, character_id, 20, order=order, require_compiled=True)["knowledge"]
        assert (private in {value["knowledgeId"] for value in values}) is present
        search = search_world(repository, '"Tideglass log discrepancy"', perspective="character", character_id=character_id, scene_id=scene_id, tick=20, order=order, mode="fts", include_text=True, require_compiled=True)
        assert (private in json.dumps(search)) is present
        if character == "orin-reed" and order == 2:
            with pytest.raises(UsageError, match="not present"):
                build_context(repository, character_id=character_id, scene_id=scene_id, tick=20, order=order, query="Tideglass log discrepancy", search_mode="fts", max_characters=8000, max_items=24, require_compiled=True)
            continue
        packet = build_context(repository, character_id=character_id, scene_id=scene_id, tick=20, order=order, query="Tideglass log discrepancy", search_mode="fts", max_characters=8000, max_items=24, require_compiled=True)
        # Caller query echo is input, while promptText/provenance is generated content.
        text = packet["promptText"]
        assert (private in text) is present and ("Tideglass log discrepancy" in text) is present
        assert future not in text and "Tideglass return authorization" not in text


def test_tideglass_04_concurrent_scene_contexts(tideglass_repo: Repository) -> None:
    repository = tideglass_repo
    for character, scene, place, present, absent, other in [
        ("mina-vale", "workshop-assessment", "Workshop", "Tavi Moss", "Orin Reed", "Quay Assessment"),
        ("orin-reed", "quay-assessment", "Quay", "Jessa Quill", "Tavi Moss", "Workshop Assessment"),
    ]:
        packet = build_context(repository, character_id=_id("character", character), scene_id=_id("scene", scene), tick=20, order=0, query="checked work", search_mode="fts", max_characters=8000, max_items=24, require_compiled=True)
        assert packet["sceneId"] == _id("scene", scene) and packet["effectiveTime"] == {"timeline": "main", "tick": 20, "order": 0}
        assert packet["selection"]["serializedCharacters"] <= 8000
        moment = packet["promptText"].split("## Present moment", 1)[1].split("##", 1)[0]
        assert place in moment and present in moment and absent not in moment
        assert f"Private {other} observation" not in packet["promptText"]


def test_tideglass_05_conversations_and_recollections(tideglass_repo: Repository) -> None:
    repository = tideglass_repo
    for conversation, total in [("quay-log-review", 2), ("workshop-repair-decision", 3)]:
        early = conversation_view(repository, _id("conversation", conversation), tick=20, order=0, require_compiled=True)
        second = conversation_view(repository, _id("conversation", conversation), tick=20, order=1, require_compiled=True)
        final = conversation_view(repository, _id("conversation", conversation), tick=30, order=0, require_compiled=True)
        assert len(early["beats"]) == 1 and not early["recollections"]
        assert len(second["beats"]) == 2 and len(final["beats"]) == total and len(final["recollections"]) == 1
    mina = conversation_view(repository, _id("conversation", "workshop-repair-decision"), perspective="character", character_id=_id("character", "mina-vale"), tick=30, order=0, require_compiled=True)
    assert mina["subjectiveRecollection"]["at"] == {"timeline": "main", "tick": 30, "order": 0}
    assert len(mina["subjectiveRecollection"]["exact_turns"]) == 1
    action = final["beats"][-1]
    assert action["kind"] == "action" and action["actorIds"] == [_id("character", "tavi-moss")]
    assert action["at"] == {"timeline": "main", "tick": 20, "order": 2}
    assert entity_state(repository, _id("object", "signal-lens"), 20, order=2, require_compiled=True)["state"]["condition"] == "cracked"


def test_tideglass_06_authored_spatial_reads_cli_http(tideglass_repo: Repository, tmp_path: Path, capsys) -> None:
    repository = tideglass_repo
    with closing(connect(repository.root / ".wedl/world.sqlite")) as connection:
        store = SQLiteSpatialStore(connection, repository.head())
        for source, route, target in [("quay", "route:quay-beacon", "beacon"), ("beacon", "route:beacon-quay", "quay")]:
            value = store.adjacency(_id("location", source), modes=["foot"]).value
            assert value.route_ids == (route,) and value.target_location_ids == (_id("location", target),)
        for tick, order, visible in [(10, 0, True), (30, 1, True), (30, 2, False)]:
            value = store.overlay_as_of(_id("location", "quay"), StoryTime("main", tick, order), audience="author", perspective="author").value
            assert ("overlay:flood-warning" in value.ids) is visible
    request = {"protocol": "wedl-spatial/v1", "revision": repository.head(), "capabilities": CAPABILITIES, "limit": 10, "cursor": None, "locationId": _id("location", "quay")}
    direct = spatial_execute(repository, "adjacency", request, require_compiled=True)
    assert direct["state"] == "ok" and direct["result"]["routeIds"] == ["route:quay-beacon"]
    path = tmp_path / "adjacency.json"
    path.write_text(json.dumps(request), encoding="utf-8")
    assert main(["--compact", "spatial", "adjacency", str(path), "--repo", str(repository.root), "--require-compiled"]) == 0
    assert json.loads(capsys.readouterr().out) == direct
    with TestClient(create_app(repository.root)) as client:
        response = client.post("/api/spatial/adjacency?requireCompiled=true", json=request)
        assert response.status_code == 200 and response.json() == direct


def test_tideglass_07_explicit_family_and_keeper_history(tideglass_repo: Repository) -> None:
    repository = tideglass_repo
    request = {"protocol": "wedl-generational/v1", "operation": "parents", "revision": repository.head(), "capabilities": CAPABILITIES, "mode": "author-as-of", "timeline": "main", "at": {"timeline": "main", "tick": "0", "order": "0"}, "items": 10, "depth": 8, "subject": _id("character", "mina-vale")}
    parents = generational_execute(repository, "parents", request)
    assert parents["state"] == "available" and len(parents["relations"]) == 1
    assert parents["relations"][0]["targetId"] == _id("character", "sela-vale")
    for organization, character, role in [
        ("beacon-cooperative", "mina-vale", "repair coordinator"),
        ("harbor-guild", "orin-reed", "ferry operator"),
    ]:
        organization_id = _id("organization", organization)
        membership = generational_execute(repository, "organization", {**request, "operation": "organization", "subject": organization_id})
        assert membership["state"] == "available" and membership["organization"]["recordId"] == organization_id
        assert len(membership["roles"]) == 1
        affiliation = membership["roles"][0]
        assert affiliation["state"] == "active" and affiliation["value"]["role"] == role
        assert affiliation["value"]["character_id"] == _id("character", character)
        assert affiliation["value"]["organization_id"] == organization_id
    vital_request = {**request, "operation": "vital", "subject": _id("character", "sela-vale")}
    before_birth = generational_execute(repository, "vital", {**vital_request, "at": {"timeline": "main", "tick": "-31", "order": "0"}})
    assert before_birth["state"] == "unknown"
    born = generational_execute(repository, "vital", {**vital_request, "at": {"timeline": "main", "tick": "-30", "order": "0"}})
    assert born["state"] == "available" and born["vital"] == "living"
    assert {"record_id": _id("vital-history", "sela-history"), "path": "story/vitals/vital_7YPSPVFBM15VGK0NS739PNBQ18.md", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": "-30", "order": "0"}}} in born["citations"]
    assert repository.load_world(cache_write=False).get(_id("vital-history", "sela-history")).frontmatter["disclosure"] == "known"
    assert generational_execute(repository, "vital", vital_request)["vital"] == "living"
    legacy = generational_execute(repository, "legacy", {**request, "operation": "legacy", "subject": _id("legacy", "beacon-keeper"), "at": {"timeline": "main", "tick": "20", "order": "0"}})
    assert legacy["state"] == "available"
    assert len(legacy["holders"]) == 1 and len(legacy["claims"]) == 1
    assert legacy["holders"][0]["value"]["holder_id"] == _id("character", "cora-finch")
    assert legacy["claims"][0]["state"] == "disputed"
    assert legacy["claims"][0]["value"]["claimant_id"] == _id("character", "tavi-moss")
    assert legacy["succession"] == []
    with TestClient(create_app(repository.root)) as client:
        headers = {"X-Wedl-Token": client.get("/api/session").json()["token"]}
        response = client.post("/api/generational/parents", json=request, headers=headers)
        assert response.status_code == 200 and response.json() == parents


def test_tideglass_08_confirmed_disposable_edit(tmp_path: Path) -> None:
    with resources.as_file(resources.files("wedl.data.tideglass_v07")) as package:
        original_package = _story_bytes(package)
    root = tmp_path / "editable"
    initialize(root, example="tideglass", profile_name="fts")
    repository = Repository(root)
    original_head = repository.head()
    original = _story_bytes(root)
    payload = {"protocol": "wedl-changeset/v1", "expectedHead": original_head, "idempotencyKey": "tideglass-survey-title-v1", "summary": "Calibrate the survey staff title", "operations": [{"type": "entity.update", "entityId": _id("object", "survey-staff"), "frontmatterPatch": {"title": "Calibrated Survey Staff"}}]}
    preview = changeset.preview(repository, payload, cache_write=False)
    assert preview["valid"] and preview["files"] == ["story/objects/survey-staff.md"]
    assert repository.head() == original_head and _story_bytes(root) == original
    with pytest.raises(ConfirmationMismatch):
        changeset.apply(repository, payload, confirmation_token_value="mismatch")
    with pytest.raises(StaleRevision):
        changeset.preview(repository, {**payload, "expectedHead": "0" * 40}, cache_write=False)
    assert repository.head() == original_head and _story_bytes(root) == original
    applied = changeset.apply(repository, payload, confirmation_token_value=preview["confirmationToken"])
    committed = repository.head()
    assert committed != original_head
    after = _story_bytes(root)
    assert {path for path in original if original[path] != after[path]} == {"story/objects/survey-staff.md"}
    old = parse_record(original["story/objects/survey-staff.md"], "story/objects/survey-staff.md")
    new = parse_record(after["story/objects/survey-staff.md"], "story/objects/survey-staff.md")
    assert new.body == old.body and new.frontmatter == {**old.frontmatter, "title": "Calibrated Survey Staff"}
    assert applied["idempotentReplay"] is False
    replay = changeset.apply(repository, payload, confirmation_token_value=preview["confirmationToken"])
    assert replay["idempotentReplay"] is True and repository.head() == committed and _story_bytes(root) == after
    with pytest.raises(StaleRevision):
        changeset.preview(repository, {**payload, "idempotencyKey": "stale-after-title"}, cache_write=False)
    with resources.as_file(resources.files("wedl.data.tideglass_v07")) as package:
        assert _story_bytes(package) == original_package
