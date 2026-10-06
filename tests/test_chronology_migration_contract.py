"""Contract oracle for the planned chronology source-upgrade transaction."""

from __future__ import annotations

from adrai_fixtures import current_decision

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re

import pytest
import yaml

from wedl import migration
from wedl.errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, ParseError, RepositoryError, StaleRevision
from wedl.model import Record, World
from wedl.repository import Snapshot
from wedl.validation import validate_world


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/CHRONOLOGY_MIGRATION_CONTRACT.md"
VECTOR = ROOT / "docs/examples/chronology-migration-v06.yaml"
THREAD_CONTRACT = current_decision("A01M48RX5WAQT5ECH66KTCFVC0T")
MIGRATION_RECOVERY = ROOT / "docs/MIGRATION_AND_RECOVERY.md"
EMPTY_CHRONOLOGY = {"calendars": [], "eras": [], "anchors": []}


class _RollbackRepository:
    """Source-free backup reader; every mutation entry point is forbidden."""

    def __init__(self, root, source):
        self.root = root
        self.source_root = "story"
        self.backup_ref = migration.BACKUP_PREFIX + "a" * 64
        self.oid = source.revision
        self.source = source
        self.head_oid = "head"
        self.ref_reads = []
        self.snapshot_reads = []
        self.next_reads = []
        self.writes = []

    def ref(self, name):
        assert name == self.backup_ref
        self.ref_reads.append(name)
        return self.oid

    def snapshot(self, oid):
        self.snapshot_reads.append(oid)
        assert oid == self.oid
        value = self.next_reads.pop(0) if self.next_reads else self.source
        if isinstance(value, BaseException):
            raise value
        return value

    def head(self):
        return self.head_oid

    def _write(self, *args, **kwargs):
        self.writes.append((args, kwargs))
        raise AssertionError("source/ref/index write forbidden")

    ensure_backup_ref = commit_files = _await_external_index_lock_release = _write


def _backup_snapshot(files, oid="captured-backup"):
    return Snapshot(oid, "backup-tree", files, {})


def _backup_hash(source):
    value = hashlib.sha256()
    for path, data in sorted(source.files.items()):
        value.update(path.encode("utf-8") + b"\0" + hashlib.sha256(data).digest() + b"\n")
    return value.hexdigest()


def _backup_request(repository, current):
    return {
        "protocol": migration.PROTOCOL, "mode": "rollback", "expectedHead": "head",
        "sourceSnapshotHash": _backup_hash(current), "idempotencyKey": "tiny-rollback",
        "rollbackBackupRef": repository.backup_ref, "rollbackBackupOid": repository.oid,
        "rollbackBackupSourceSnapshotHash": _backup_hash(repository.source),
    }


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


def test_same_v1_wire_parity_uses_current_hash_order_and_response_identity(tmp_path, monkeypatch):
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

    raw = b"---\nschema: wedl/v0.4\n---\nrestored\n"
    source = _backup_snapshot({"story/a.md": raw})
    current = _backup_snapshot({"story/a.md": b"current\n"}, "head")
    repository = _RollbackRepository(tmp_path, source)
    request = _backup_request(repository, current)
    assert migration._rollback_identity(repository, repository.backup_ref) == (source.revision, _backup_hash(source))

    # Same captured OID, different content in a separate request must be read again.
    repository.source = _backup_snapshot({"story/a.md": raw + b"changed\n"})
    with monkeypatch.context() as patch:
        patch.setattr(migration, "_raw_records", lambda _source: pytest.fail("identity must precede parsing"))
        with pytest.raises(StaleRevision, match="changed after preview"):
            migration._rollback_plan(repository, request, current)
    assert repository.snapshot_reads == [source.revision, source.revision]
    assert repository.writes == []

    # Read failures propagate unchanged; malformed bytes are parsed only after identity.
    for failure in (OSError("unavailable backup"), RepositoryError("corrupt object")):
        repository.next_reads = [failure]
        with pytest.raises(type(failure)) as raised:
            migration._rollback_plan(repository, request, current)
        assert raised.value is failure
    repository.source = _backup_snapshot({"story/a.md": b"not a frontmatter record\n"})
    with pytest.raises(StaleRevision):
        migration._rollback_plan(repository, request, current)
    malformed_request = _backup_request(repository, current)
    with pytest.raises(ParseError):
        migration._rollback_plan(repository, malformed_request, current)
    repository.source = _backup_snapshot({"story/a.md": b"\xff"})
    with pytest.raises(ParseError):
        migration._rollback_plan(repository, _backup_request(repository, current), current)

    # HEAD and current-source checks remain ahead of backup ref resolution.
    repository.source = source
    before_reads = len(repository.ref_reads)
    repository.head_oid = "moved-head"
    with pytest.raises(StaleRevision, match="different HEAD"):
        migration._normalized_request(repository, request, current)
    repository.head_oid = "head"
    with pytest.raises(StaleRevision, match="source snapshot changed"):
        migration._normalized_request(repository, {**request, "sourceSnapshotHash": "wrong"}, current)
    assert len(repository.ref_reads) == before_reads
    normalized = migration._normalized_request(repository, request, current)
    assert normalized["rollbackBackupOid"] == source.revision
    assert normalized["rollbackBackupSourceSnapshotHash"] == _backup_hash(source)
    assert len(repository.ref_reads) == before_reads + 1
    migration._rollback_plan(repository, normalized, current)
    assert len(repository.ref_reads) == before_reads + 2

    # A tiny stored receipt exercises the public replay branch without a Git/source fixture.
    receipt_path = tmp_path / ".wedl" / "migration-receipts.json"
    receipt_path.parent.mkdir()
    response = {"protocol": migration.PROTOCOL, "phase": "apply", "status": "committed"}
    receipt_path.write_text(json.dumps({request["idempotencyKey"]: {
        "request": normalized, "confirmationToken": "confirmed", "result": response,
    }}), encoding="utf-8")
    receipt_bytes = receipt_path.read_bytes()
    for token, error in ((None, ConfirmationRequired), ("wrong", ConfirmationMismatch)):
        before_reads = len(repository.ref_reads)
        with pytest.raises(error):
            migration.apply(repository, request, confirmation_token_value=token)
        assert len(repository.ref_reads) == before_reads + 1
    before_reads = len(repository.ref_reads)
    with pytest.raises(ConflictError):
        migration.apply(repository, {**request, "expectedHead": "another-head"}, confirmation_token_value=None)
    with pytest.raises(RepositoryError):
        migration.apply(repository, {**request, "sourceSnapshotHash": None}, confirmation_token_value=None)
    assert len(repository.ref_reads) == before_reads
    repository.source = _backup_snapshot({"story/a.md": raw + b"rebound content\n"})
    with pytest.raises(StaleRevision):
        migration.apply(repository, request, confirmation_token_value=None)
    repository.source = source
    repository.oid = "rebound-oid"
    with pytest.raises(StaleRevision):
        migration.apply(repository, request, confirmation_token_value=None)
    repository.oid = source.revision
    repository.head_oid = "moved-head"
    for _ in range(2):
        before_reads = len(repository.ref_reads)
        assert migration.apply(repository, request, confirmation_token_value="confirmed") == {**response, "idempotentReplay": True}
        assert len(repository.ref_reads) == before_reads + 1
    assert receipt_path.read_bytes() == receipt_bytes
    assert repository.writes == []

    invalid = _backup_snapshot({"story/a.md": b"---\nschema: wedl/v0.5\nkind: object\nid: invalid\n---\nbody\n"})
    invalid_repository = _RollbackRepository(tmp_path, invalid)
    rejected = migration._rollback_plan(invalid_repository, _backup_request(invalid_repository, current), current)
    assert not rejected["valid"] and not rejected["skipCompile"]
    assert any(item["severity"] == "error" for item in rejected["diagnostics"])
    assert rejected["changes"] == {} and rejected["diff"] == ""
    assert invalid_repository.writes == [] and receipt_path.read_bytes() == receipt_bytes


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


def test_rollback_pointer_uses_only_the_literal_preview_backup_ref(tmp_path):
    text = MIGRATION_RECOVERY.read_text(encoding="utf-8")
    assert "--rollback-backup-ref <BACKUP_REF_FROM_PREVIEW>" in text
    assert "--rollback-backup-ref refs/wedl/backups/migration/<REQUEST_HASH>" not in text

    restored = b"---\nschema: wedl/v0.4\n---\nrestored\r\n"
    source = _backup_snapshot({"story/a.md": restored})
    current = _backup_snapshot({"story/a.md": b"current\n"}, "head")
    repository = _RollbackRepository(tmp_path, source)
    request = _backup_request(repository, current)
    repository.next_reads = [source, AssertionError("duplicate snapshot read")]
    plan = migration._rollback_plan(repository, request, current)
    assert repository.ref_reads == [repository.backup_ref]
    assert repository.snapshot_reads == [source.revision]
    assert len(repository.next_reads) == 1
    assert plan == {
        "valid": True, "diagnostics": [], "changes": {"story/a.md": restored},
        "diff": "--- a/story/a.md\n+++ b/story/a.md\n@@ -1 +1,4 @@\n-current\n+---\n+schema: wedl/v0.4\n+---\n+restored\r\n",
        "noOp": False, "skipCompile": True,
    }
    assert request["rollbackBackupSourceSnapshotHash"] == _backup_hash(source)
    repository.next_reads = []
    assert migration._rollback_plan(repository, request, current) == plan
    assert repository.ref_reads == [repository.backup_ref] * 2
    assert repository.snapshot_reads == [source.revision] * 2

    repository.oid = "changed-backup-oid"
    repository.source = _backup_snapshot(source.files, repository.oid)
    with pytest.raises(StaleRevision, match="changed after preview"):
        migration._rollback_plan(repository, request, current)
    assert repository.snapshot_reads[-1] == repository.oid
    repository.oid = None
    before_snapshots = list(repository.snapshot_reads)
    with pytest.raises(RepositoryError, match="backup ref does not exist"):
        migration._rollback_plan(repository, request, current)
    assert repository.snapshot_reads == before_snapshots
    repository.oid = source.revision
    repository.source = source
    assert migration._rollback_identity(repository, repository.backup_ref) == (source.revision, _backup_hash(source))
    assert repository.writes == []
