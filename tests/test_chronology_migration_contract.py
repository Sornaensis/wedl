"""Contract oracle for the planned chronology source-upgrade transaction."""

from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re

import yaml

from wedl.model import Record, World
from wedl.validation import validate_world


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/CHRONOLOGY_MIGRATION_CONTRACT.md"
VECTOR = ROOT / "docs/examples/chronology-migration-v06.yaml"
THREAD_CONTRACT = ROOT / "docs/THREAD_SCHEMA_CONTRACT.md"
MIGRATION_RECOVERY = ROOT / "docs/MIGRATION_AND_RECOVERY.md"
EMPTY_CHRONOLOGY = {"calendars": [], "eras": [], "anchors": []}


def _schemas(records):
    return {record["frontmatter"]["schema"] for record in records}


def _legacy_chronology_preflight(records):
    """The real command delegates full validity to the pinned legacy validator."""
    for record in records:
        if "chronology" in record["frontmatter"]:
            return "WDL-MIG-V06-008"
    return None


def _pinned_legacy_failure(records):
    """Call the current pinned validators; they only inspect constructed records."""
    parsed = [Record(deepcopy(item["frontmatter"]), "", item["path"], b"") for item in records]
    if len({record.id for record in parsed}) != len(parsed):
        return "WDL-MIG-V06-010"
    world = World("fixture", "fixture", {record.id: record for record in parsed}, ROOT)
    return "WDL-MIG-V06-010" if validate_world(world) else None


def _v06_candidate_failure(records):
    worlds = [record["frontmatter"] for record in records if record["frontmatter"]["kind"] == "world"]
    if len(worlds) != 1 or any(record["frontmatter"].get("schema") != "wedl/v0.6" for record in records):
        return "WDL-MIG-V06-009"
    chronology = worlds[0].get("chronology")
    if not isinstance(chronology, dict) or any(not isinstance(chronology.get(key), list) for key in EMPTY_CHRONOLOGY):
        return "WDL-MIG-V06-009"
    if any("threadIds" in record["frontmatter"] for record in records):
        return "WDL-MIG-V06-009"
    return None


def _canonical_json(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _sha256_json(value):
    return hashlib.sha256(_canonical_json(value).encode("utf-8")).hexdigest()


def _planned_upgrade(records, *, mutate_candidate=None):
    """Test-local transformation oracle; it performs no repository writes."""
    schemas = _schemas(records)
    if schemas == {"wedl/v0.4"}:
        return "WDL-MIG-V06-003"
    if len(schemas) != 1:
        return "WDL-MIG-V06-002"
    schema = schemas.pop()
    if schema == "wedl/v0.6":
        return deepcopy(records)
    if schema == "wedl/v0.2":
        return "WDL-MIG-V06-007"
    if schema not in {"wedl/v0.3", "wedl/v0.5"}:
        return "WDL-MIG-V06-006"
    chronology_failure = _legacy_chronology_preflight(records)
    if chronology_failure:
        return chronology_failure
    legacy_failure = _pinned_legacy_failure(records)
    if legacy_failure:
        return legacy_failure

    candidate = deepcopy(records)
    world = next(record["frontmatter"] for record in candidate if record["frontmatter"]["kind"] == "world")
    if schema == "wedl/v0.3":
        timelines = world.get("timelines", [])
        if len(timelines) != 1 or world.get("default_timeline") != timelines[0].get("id"):
            return "WDL-MIG-V06-001"
        world["threads"] = []
    for record in candidate:
        record["frontmatter"]["schema"] = "wedl/v0.6"
    world["chronology"] = deepcopy(EMPTY_CHRONOLOGY)
    if mutate_candidate is not None:
        mutate_candidate(candidate)
    candidate_failure = _v06_candidate_failure(candidate)
    if candidate_failure:
        return candidate_failure
    return candidate


def test_compatibility_matrix_and_exact_migration_protocol_are_stable():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    assert data["protocol"] == "wedl-migration/v1"
    assert data["mode"] == "upgrade-v06"
    assert data["world_chronology"] == EMPTY_CHRONOLOGY
    assert data["compatibility_matrix"] == {
        "wedl/v0.3": {"reader": "pinned_legacy", "chronology_write": "upgrade_required", "upgrade": "v03_to_v06"},
        "wedl/v0.5": {"reader": "pinned_legacy", "chronology_write": "upgrade_required", "upgrade": "v05_to_v06"},
        "wedl/v0.2": {"reader": "too_old", "chronology_write": "unavailable", "upgrade": "reject_too_old"},
        "wedl/v0.4": {"reader": "quarantined", "chronology_write": "unavailable", "upgrade": "recover_v04_to_v05_first"},
        "wedl/v0.6": {"reader": "validator_and_compiler_required", "chronology_write": "supported_after_release", "upgrade": "no_op"},
        "wedl/v0.7": {"reader": "too_new", "chronology_write": "unavailable", "upgrade": "reject_too_new"},
    }


def test_v03_and_v05_goldens_transform_only_the_documented_fields():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    for source in ("v03", "v05"):
        golden = data["goldens"][source]
        actual = _planned_upgrade(golden["input"])
        assert actual == golden["output"]
    v03_output = data["goldens"]["v03"]["output"]
    assert "threads" not in v03_output[1]["frontmatter"]
    assert "chronology" not in v03_output[1]["frontmatter"]


def test_v05_grouping_data_is_preserved_without_reinterpretation():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    before = data["goldens"]["v05"]["input"]
    after = _planned_upgrade(before)
    assert after[0]["frontmatter"]["threads"] == before[0]["frontmatter"]["threads"]
    assert after[1]["frontmatter"]["threads"] == before[1]["frontmatter"]["threads"]
    assert after[1]["frontmatter"].keys() == {"schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads"}


def test_cross_contract_persists_group_membership_as_threads_never_thread_ids():
    migration = " ".join(DOC.read_text(encoding="utf-8").split())
    thread_schema = THREAD_CONTRACT.read_text(encoding="utf-8")
    vector_text = VECTOR.read_text(encoding="utf-8")
    assert "`threadIds` is never a persisted source field" in migration
    assert "Every non-world, non-hypothesis record may omit or carry `threads`" in thread_schema
    assert "threadIds:" not in migration
    assert "threadIds" not in thread_schema
    assert "threadIds" not in vector_text


def test_v06_is_an_explicit_no_op_and_invalid_paths_are_stable():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    current = data["goldens"]["v06"]["input"]
    assert _planned_upgrade(current) == current
    v03 = deepcopy(data["goldens"]["v03"]["input"])
    v03[0]["frontmatter"]["timelines"].append({"id": "later", "label": "Later"})
    assert _planned_upgrade(v03) == data["rejections"]["v03_multiple_timelines"]
    mixed = deepcopy(data["goldens"]["v03"]["input"])
    mixed[1]["frontmatter"]["schema"] = "wedl/v0.5"
    assert _planned_upgrade(mixed) == data["rejections"]["mixed_source_schema"]
    v04 = deepcopy(data["goldens"]["v03"]["input"])
    for record in v04:
        record["frontmatter"]["schema"] = "wedl/v0.4"
    assert _planned_upgrade(v04) == data["rejections"]["v04_direct"]
    future = deepcopy(data["goldens"]["v06"]["input"])
    future[0]["frontmatter"]["schema"] = "wedl/v0.7"
    assert _planned_upgrade(future) == data["rejections"]["too_new"]
    retired = deepcopy(data["goldens"]["v06"]["input"])
    retired[0]["frontmatter"]["schema"] = "wedl/v0.2"
    assert _planned_upgrade(retired) == data["rejections"]["too_old"]


def test_legacy_chronology_is_rejected_before_transform_without_writing_input():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    original = data["goldens"]["v05"]["input"]
    for fixture in data["legacy_chronology_negatives"].values():
        candidate = deepcopy(original)
        target = candidate
        for index in fixture["path"][:-1]:
            target = target[index]
        target[fixture["path"][-1]] = deepcopy(fixture["value"])
        before = deepcopy(candidate)
        assert _planned_upgrade(candidate) == data["rejections"]["legacy_chronology_present"]
        assert candidate == before, "preflight rejection must not mutate source"
    assert data["compatibility_matrix"]["wedl/v0.3"]["chronology_write"] == "upgrade_required"
    assert data["compatibility_matrix"]["wedl/v0.5"]["chronology_write"] == "upgrade_required"


def test_invalid_pinned_legacy_or_transformed_v06_candidate_writes_nothing():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    source = data["goldens"]["v05"]["input"]
    legacy = deepcopy(source)
    target = legacy
    for index in data["legacy_invalid_negative"]["path"][:-1]:
        target = target[index]
    target[data["legacy_invalid_negative"]["path"][-1]] = data["legacy_invalid_negative"]["value"]
    before_legacy = deepcopy(legacy)
    assert _planned_upgrade(legacy) == data["rejections"]["pinned_legacy_invalid"]
    assert legacy == before_legacy

    before_source = deepcopy(source)
    path = data["candidate_invalid_negative"]["path"]
    def make_candidate_invalid(candidate):
        target = candidate
        for index in path[:-1]:
            target = target[index]
        target[path[-1]] = data["candidate_invalid_negative"]["value"]
    assert _planned_upgrade(source, mutate_candidate=make_candidate_invalid) == data["rejections"]["v06_candidate_invalid"]
    assert source == before_source, "candidate rejection precedes every ref/source/receipt/cache write"


def test_same_v1_wire_parity_uses_current_hash_order_and_response_identity():
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    parity = data["wire_parity"]
    base = {**parity["request"], "sourceSnapshotHash": parity["sourceSnapshotHash"]}
    assert base == parity["base_request"]
    backup = "refs/wedl/backups/migration/" + _sha256_json(base)
    assert backup == parity["backupRef"]
    assert _sha256_json(base) == parity["baseRequestHash"]
    normalized = {**base, "backupRef": backup}
    assert _sha256_json(normalized) == parity["requestHash"]
    assert parity["baseRequestHash"] != parity["requestHash"]
    for value in (parity["sourceSnapshotHash"], parity["requestHash"], backup.rsplit("/", 1)[1]):
        assert re.fullmatch(r"[0-9a-f]{64}", value)
        assert not value.startswith("sha256:")
    assert parity["preview_phase"] == "preview"
    assert parity["apply_phase"] == "apply"
    assert "phase" in parity["preview_fields"] and "idempotencyKey" in parity["preview_fields"]
    assert "phase" in parity["apply_fields"] and "idempotencyKey" in parity["apply_fields"]
    assert parity["receipt_value_fields"] == ["request", "confirmationToken", "result"]
    assert parity["noop_fields"] == [
        "protocol", "phase", "mode", "valid", "noOp", "expectedHead", "sourceSnapshotHash",
        "idempotencyKey", "backupRef", "requestHash", "confirmationToken", "diagnostics",
        "status", "idempotentReplay",
    ]
    assert parity["noop_omits"] == ["diff", "files"]
    assert parity["replay_identity_fields"] == [
        "protocol", "mode", "expectedHead", "sourceSnapshotHash", "idempotencyKey", "rollbackBackupRef", "backupRef",
    ]


def test_document_binds_atomic_preview_apply_and_disposable_cache_policy():
    text = " ".join(DOC.read_text(encoding="utf-8").split())
    for phrase in (
        "bounded multi-version read",
        "validator and compiler prerequisite is satisfied",
        "WDL-MIG-V06-004 upgrade_required",
        "recover_v04_to_v05_first",
        "expectedHead", "sourceSnapshotHash", "confirmationToken", "backupRef", "\"phase\": \"preview\"",
        "idempotentReplay: true", "managed source tree and managed index", "forward commit",
        "Compiled SQLite is disposable output", "never migrated, copied, preserved, or rolled back",
        "byte-semantically", "does not relabel, sort, deduplicate, infer",
        "Mixed v0.3/v0.5/v0.6", "source_schema_too_new", "source_schema_too_old", "Receipt-write failure",
        "legacy_chronology_present", "pinned legacy parser and validator", "never a persisted source field",
        "v06_candidate_invalid", "before `ensure_backup_ref` or commit", "shipped v0.6 validator",
        "baseRequestHash", "distinct `requestHash`", "removes exactly the public `diff` and `files` fields",
        "bare SHA-256 hex", "current derivation order", "Receipts remain keyed by the trimmed `idempotencyKey`",
        "src/wedl/repository.py", "src/wedl/compiler.py", "src/wedl/migration.py",
    ):
        assert phrase in text
    data = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    assert data["receipt_required_fields"] == [
        "request", "confirmationToken", "result",
    ]


def test_rollback_pointer_uses_only_the_literal_preview_backup_ref():
    text = MIGRATION_RECOVERY.read_text(encoding="utf-8")
    assert "--rollback-backup-ref <BACKUP_REF_FROM_PREVIEW>" in text
    assert "--rollback-backup-ref refs/wedl/backups/migration/<REQUEST_HASH>" not in text
