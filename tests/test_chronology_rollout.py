"""Release-level checks for the executable v0.6 migration route."""

from __future__ import annotations

from copy import deepcopy
from pathlib import Path

import pytest
import yaml

from conftest import git
from wedl.migration import PROTOCOL, _classify_v06_source_schema, apply, preview
from wedl.repository import Repository
from wedl.source import serialize_record


ROOT = Path(__file__).resolve().parents[1]
VECTOR = ROOT / "docs/examples/chronology-migration-v06.yaml"


def _repo(tmp_path: Path, source: str) -> Repository:
    root = tmp_path / source
    (root / "story").mkdir(parents=True)
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))["goldens"][source]["input"]
    for item in data:
        path = root / item["path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(deepcopy(item["frontmatter"]), ""))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    git(root, "init", "-q"); git(root, "config", "user.name", "wedl test"); git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore"); git(root, "commit", "-qm", "legacy")
    return Repository(root)


def _request(repository: Repository, key: str) -> dict[str, str]:
    return {"protocol": PROTOCOL, "mode": "upgrade-v06", "expectedHead": repository.head(), "idempotencyKey": key}


@pytest.mark.parametrize("source", ("v03", "v05"))
def test_upgrade_v06_is_confirmed_and_preserves_v05_threads(tmp_path: Path, source: str) -> None:
    repository = _repo(tmp_path, source)
    request = _request(repository, source)
    planned = preview(repository, request)
    assert planned["valid"] is True and planned["noOp"] is False
    receipt = apply(repository, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert receipt["status"] == "committed"
    world = repository.load_world()
    assert world.schema == "wedl/v0.6"
    assert world.world_record.frontmatter["chronology"] == {"calendars": [], "eras": [], "anchors": []}
    if source == "v05":
        assert world.records["char_0123456789ABCDEFGHJKMNPQRS"].frontmatter["threads"] == ["thread_0123456789ABCDEFGHJKMNPQRS"]


def test_upgrade_v06_is_noop_and_rejects_legacy_chronology(tmp_path: Path) -> None:
    repository = _repo(tmp_path, "v03")
    request = _request(repository, "first")
    planned = preview(repository, request)
    apply(repository, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    no_op = preview(repository, _request(repository, "current"))
    assert no_op["noOp"] is True

    legacy = _repo(tmp_path, "v05")
    record = legacy.root / "story" / "world.md"
    record.write_text(record.read_text(encoding="utf-8").replace("aliases: []", "aliases: []\nchronology: {}", 1), encoding="utf-8")
    git(legacy.root, "add", "story/world.md"); git(legacy.root, "commit", "-qm", "legacy chronology")
    rejected = preview(legacy, _request(legacy, "legacy-chronology"))
    assert rejected["valid"] is False
    assert rejected["diagnostics"][0]["code"] == "WDL-MIG-V06-008"


def test_upgrade_v06_forward_rollback_is_bound_and_leaves_cache_disposable(tmp_path: Path, monkeypatch) -> None:
    repository = _repo(tmp_path, "v03")
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repo: {"status": "deferred-test-cache-rebuild"})
    request = _request(repository, "upgrade")
    plan = preview(repository, request)
    upgraded = apply(repository, {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]}, confirmation_token_value=plan["confirmationToken"])
    rollback_request = {"protocol": PROTOCOL, "mode": "rollback", "expectedHead": upgraded["newHead"], "idempotencyKey": "rollback", "rollbackBackupRef": upgraded["backupRef"]}
    rollback_plan = preview(repository, rollback_request)
    rolled_back = apply(repository, {**rollback_request, "sourceSnapshotHash": rollback_plan["sourceSnapshotHash"]}, confirmation_token_value=rollback_plan["confirmationToken"])
    assert rolled_back["status"] == "committed"
    assert repository.load_world().schema == "wedl/v0.3"


@pytest.mark.parametrize(("schema", "code"), (("wedl/v0.2", "WDL-MIG-V06-007"), ("wedl/v0.10", "WDL-MIG-V06-006"), ("wedl/v1.0", "WDL-MIG-V06-006"), ("wedl/vbroken", "WDL-MIG-V06-006"), (None, "WDL-MIG-V06-006"), (3, "WDL-MIG-V06-006")))
def test_upgrade_v06_schema_classification_is_numeric_and_total(schema, code) -> None:
    assert _classify_v06_source_schema(schema)[0] == code
