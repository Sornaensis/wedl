"""Explicit, local source migrations for the shared-world thread schema.

This module deliberately does not use :meth:`Repository.load_world` while
auditing a withdrawn v0.4 tree.  The normal loader correctly quarantines that
schema; recovery instead reads immutable Git blobs, parses only their Markdown
envelopes, and validates the complete proposed v0.5 world before it writes.
"""

from __future__ import annotations

from copy import deepcopy
import difflib
import hashlib
import json
import re
from typing import Any

from . import CHRONOLOGY_SOURCE_SCHEMA, THREAD_SOURCE_SCHEMA, V04_SOURCE_SCHEMA, V07_SOURCE_SCHEMA
from .compiler import compile_world
from .errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, RepositoryError, StaleRevision, ValidationFailed
from .model import Record, World
from .repository import Repository, Snapshot
from .source import serialize_record, split_envelope
from .util import atomic_write, canonical_json
from .validation import validate_world
from .v07 import canonical_capabilities


PROTOCOL = "wedl-migration/v1"
MODES = frozenset(("upgrade-v03", "upgrade-v06", "upgrade-v07", "recover-v04", "rollback"))
V07_DEFAULT_CAPABILITIES = ("generational-core-v1", "spatial-core-v1")
BACKUP_PREFIX = "refs/wedl/backups/migration/"
_BACKUP_REF_RE = re.compile(r"^refs/wedl/backups/migration/[0-9a-f]{64}$")


def _diagnostic(code: str, message: str, *, path: str = "", field: str = "") -> dict[str, Any]:
    return {"code": code, "message": message, "severity": "error", "path": path, "field": field}


def _source_hash(snapshot: Snapshot) -> str:
    material = bytearray()
    for path, data in sorted(snapshot.files.items()):
        material.extend(path.encode("utf-8")); material.extend(b"\0")
        material.extend(hashlib.sha256(data).digest()); material.extend(b"\n")
    return hashlib.sha256(bytes(material)).hexdigest()


def _request_hash(request: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_json(request).encode("utf-8")).hexdigest()


def _confirmation_token(request: dict[str, Any], request_hash: str) -> str:
    material = {"domain": "wedl-migration-confirmation/v1", "request": request, "requestHash": request_hash}
    return "wedl-migration-confirmation/v1:" + hashlib.sha256(canonical_json(material).encode("utf-8")).hexdigest()


def _raw_records(snapshot: Snapshot) -> list[tuple[str, dict[str, Any], str, bytes]]:
    records: list[tuple[str, dict[str, Any], str, bytes]] = []
    for path, data in sorted(snapshot.files.items()):
        frontmatter, body = split_envelope(data, path)
        records.append((path, frontmatter, body, data))
    return records


def _request_fields(request: dict[str, Any], *, require_source_hash: bool = False) -> tuple[str, str, str | None, str, str | None, Any]:
    """Read the complete public apply identity before replaying a receipt."""

    if request.get("protocol") != PROTOCOL:
        raise RepositoryError(f"migration request protocol must be {PROTOCOL}")
    mode = request.get("mode")
    if mode not in MODES:
        raise RepositoryError("migration mode must be upgrade-v03, upgrade-v06, upgrade-v07, recover-v04, or rollback")
    expected = request.get("expectedHead")
    source_hash = request.get("sourceSnapshotHash")
    key = request.get("idempotencyKey")
    if not isinstance(expected, str) or not expected:
        raise RepositoryError("migration requires an exact expectedHead")
    if source_hash is not None and (not isinstance(source_hash, str) or not source_hash):
        raise RepositoryError("migration sourceSnapshotHash must be a non-empty string")
    if require_source_hash and source_hash is None:
        raise RepositoryError("migration apply requires an exact sourceSnapshotHash")
    if not isinstance(key, str) or not key.strip():
        raise RepositoryError("migration requires a non-empty idempotencyKey")
    rollback_ref = request.get("rollbackBackupRef")
    if mode == "rollback":
        if not isinstance(rollback_ref, str) or not _BACKUP_REF_RE.fullmatch(rollback_ref):
            raise RepositoryError("rollback requires a migration rollbackBackupRef")
    elif rollback_ref is not None:
        raise RepositoryError("rollbackBackupRef is only valid for rollback")
    return mode, expected, source_hash, key.strip(), rollback_ref, request.get("backupRef")


def _rollback_identity(repository: Repository, backup_ref: str) -> tuple[str, str]:
    oid = repository.ref(backup_ref)
    if oid is None:
        raise RepositoryError("rollback backup ref does not exist")
    return oid, _source_hash(repository.snapshot(oid))


def _normalized_request(repository: Repository, request: dict[str, Any], snapshot: Snapshot) -> dict[str, Any]:
    mode, expected, supplied_hash, key, rollback_ref, requested_backup = _request_fields(request)
    actual = repository.head()
    if expected != actual:
        raise StaleRevision("migration expected a different HEAD", details={"expected": expected, "actual": actual})
    source_hash = _source_hash(snapshot)
    if supplied_hash is not None and supplied_hash != source_hash:
        raise StaleRevision("migration source snapshot changed", details={"expected": supplied_hash, "actual": source_hash})
    base = {"protocol": PROTOCOL, "mode": mode, "expectedHead": expected, "sourceSnapshotHash": source_hash, "idempotencyKey": key}
    if mode == "upgrade-v07":
        source_worlds = [frontmatter for _path, frontmatter, _body, _data in _raw_records(snapshot) if frontmatter.get("kind") == "world"]
        existing = canonical_capabilities(source_worlds[0].get("capabilities")) if len(source_worlds) == 1 and source_worlds[0].get("schema") == V07_SOURCE_SCHEMA else None
        target_capabilities = list(existing or V07_DEFAULT_CAPABILITIES)
        requested_capabilities = request.get("targetCapabilities")
        if requested_capabilities is not None and requested_capabilities != target_capabilities:
            raise RepositoryError("migration targetCapabilities are server-derived")
        base["targetCapabilities"] = target_capabilities
    if rollback_ref is not None:
        base["rollbackBackupRef"] = rollback_ref
        oid, rollback_hash = _rollback_identity(repository, rollback_ref)
        base["rollbackBackupOid"] = oid
        base["rollbackBackupSourceSnapshotHash"] = rollback_hash
    default_backup = BACKUP_PREFIX + hashlib.sha256(canonical_json(base).encode("utf-8")).hexdigest()
    if requested_backup is not None and requested_backup != default_backup:
        raise RepositoryError("migration backupRef does not match the deterministic request backup")
    if mode != "upgrade-v07" and request.get("targetCapabilities") is not None:
        raise RepositoryError("targetCapabilities is only valid for upgrade-v07")
    return {**base, "backupRef": default_backup}


def _world_for_candidate(repository: Repository, revision: str, tree_oid: str, entries: list[tuple[str, dict[str, Any], str]]) -> World:
    records: dict[str, Record] = {}
    for path, frontmatter, body in entries:
        record = Record(frontmatter, body, path, serialize_record(frontmatter, body), revision=revision)
        if record.id in records:
            raise RepositoryError(f"migration candidate has duplicate entity ID {record.id}")
        records[record.id] = record
    return World(revision, tree_oid, records, repository.root, repository.source_root)


def _v03_candidate(repository: Repository, snapshot: Snapshot, raw: list[tuple[str, dict[str, Any], str, bytes]]) -> tuple[list[tuple[str, dict[str, Any], str]], list[dict[str, Any]], bool]:
    schemas = {frontmatter.get("schema") for _path, frontmatter, _body, _data in raw}
    if schemas == {THREAD_SOURCE_SCHEMA}:
        return [], [], True
    if schemas != {"wedl/v0.3"}:
        return [], [_diagnostic("WDL-MIG-001", "upgrade-v03 requires a homogeneous wedl/v0.3 source", field="schema")], False
    worlds = [(path, frontmatter) for path, frontmatter, _body, _data in raw if frontmatter.get("kind") == "world"]
    if len(worlds) != 1:
        return [], [_diagnostic("WDL-MIG-002", "upgrade-v03 requires exactly one world record", field="kind")], False
    world_path, world = worlds[0]
    timelines = world.get("timelines")
    if not isinstance(timelines, list) or len(timelines) != 1 or not isinstance(timelines[0], dict) or not isinstance(timelines[0].get("id"), str) or world.get("default_timeline") != timelines[0]["id"]:
        return [], [_diagnostic("WDL-MIG-003", "upgrade-v03 requires exactly one timeline and a matching default_timeline", path=world_path, field="timelines")], False
    candidate: list[tuple[str, dict[str, Any], str]] = []
    for path, original, body, _data in raw:
        frontmatter = deepcopy(original); frontmatter["schema"] = THREAD_SOURCE_SCHEMA
        if path == world_path:
            frontmatter["threads"] = []
        else:
            frontmatter.pop("threads", None)
        candidate.append((path, frontmatter, body))
    return candidate, [], False


_EMPTY_CHRONOLOGY = {"calendars": [], "eras": [], "anchors": []}
_SCHEMA_VERSION_RE = re.compile(r"^wedl/v(\d+)\.(\d+)$")


def _v06_diagnostic(code: str, message: str, *, path: str = "", field: str = "schema") -> dict[str, Any]:
    return _diagnostic(code, message, path=path, field=field)


def _classify_v06_source_schema(schema: Any) -> tuple[str, str]:
    """Classify schema labels numerically; lexical order misreads ``v0.10``."""
    if not isinstance(schema, str):
        return "WDL-MIG-V06-006", "source_schema_too_new"
    match = _SCHEMA_VERSION_RE.fullmatch(schema)
    if match is None:
        return "WDL-MIG-V06-006", "source_schema_too_new"
    version = (int(match.group(1)), int(match.group(2)))
    if version < (0, 3):
        return "WDL-MIG-V06-007", "source_schema_too_old"
    return "WDL-MIG-V06-006", "source_schema_too_new"


def _v06_candidate(repository: Repository, snapshot: Snapshot, raw: list[tuple[str, dict[str, Any], str, bytes]]) -> tuple[list[tuple[str, dict[str, Any], str]], list[dict[str, Any]], bool]:
    """Build the narrow v0.3/v0.5 -> v0.6 source candidate.

    The existing migration envelope, backup, confirmation, and receipt logic is
    the transaction authority. This transformation adds no time interpretation.
    """
    schemas = {frontmatter.get("schema") for _path, frontmatter, _body, _data in raw}
    if len(schemas) != 1:
        return [], [_v06_diagnostic("WDL-MIG-V06-002", "mixed_source_schema")], False
    schema = next(iter(schemas), None)
    if schema == V04_SOURCE_SCHEMA:
        return [], [_v06_diagnostic("WDL-MIG-V06-003", "recover_v04_to_v05_first")], False
    if schema == CHRONOLOGY_SOURCE_SCHEMA:
        candidate = [(path, deepcopy(frontmatter), body) for path, frontmatter, body, _data in raw]
        world = _world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, candidate)
        if any(item["severity"] == "error" for item in validate_world(world)):
            return [], [_v06_diagnostic("WDL-MIG-V06-009", "v06_candidate_invalid")], False
        return [], [], True
    if schema not in {"wedl/v0.3", THREAD_SOURCE_SCHEMA}:
        code, message = _classify_v06_source_schema(schema)
        return [], [_v06_diagnostic(code, message)], False
    for path, frontmatter, _body, _data in raw:
        if "chronology" in frontmatter:
            return [], [_v06_diagnostic("WDL-MIG-V06-008", "legacy_chronology_present", path=path, field="chronology")], False
    legacy = [(path, deepcopy(frontmatter), body) for path, frontmatter, body, _data in raw]
    legacy_world = _world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, legacy)
    if any(item["severity"] == "error" for item in validate_world(legacy_world)):
        return [], [_v06_diagnostic("WDL-MIG-V06-010", "pinned_legacy_invalid")], False
    worlds = [(path, frontmatter) for path, frontmatter, _body, _data in raw if frontmatter.get("kind") == "world"]
    if len(worlds) != 1:
        return [], [_v06_diagnostic("WDL-MIG-V06-010", "pinned_legacy_invalid")], False
    world_path, old_world = worlds[0]
    if schema == "wedl/v0.3":
        timelines = old_world.get("timelines")
        if not isinstance(timelines, list) or len(timelines) != 1 or not isinstance(timelines[0], dict) or old_world.get("default_timeline") != timelines[0].get("id"):
            return [], [_v06_diagnostic("WDL-MIG-V06-001", "v03_requires_one_timeline", path=world_path, field="timelines")], False
    candidate: list[tuple[str, dict[str, Any], str]] = []
    for path, original, body, _data in raw:
        frontmatter = deepcopy(original)
        frontmatter["schema"] = CHRONOLOGY_SOURCE_SCHEMA
        if path == world_path:
            if schema == "wedl/v0.3":
                frontmatter["threads"] = []
            frontmatter["chronology"] = deepcopy(_EMPTY_CHRONOLOGY)
        elif schema == "wedl/v0.3":
            frontmatter.pop("threads", None)
        candidate.append((path, frontmatter, body))
    world = _world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, candidate)
    if any(item["severity"] == "error" for item in validate_world(world)):
        return [], [_v06_diagnostic("WDL-MIG-V06-009", "v06_candidate_invalid")], False
    return candidate, [], False


def _v07_candidate(repository: Repository, snapshot: Snapshot, raw: list[tuple[str, dict[str, Any], str, bytes]]) -> tuple[list[tuple[str, dict[str, Any], str]], list[dict[str, Any]], bool]:
    """Create the one lossless v0.7 envelope from a homogeneous legacy tree."""

    schemas = {frontmatter.get("schema") for _path, frontmatter, _body, _data in raw}
    if len(schemas) != 1:
        return [], [_diagnostic("GEN-VERSION-001", "upgrade-v07 requires one homogeneous source version", field="schema")], False
    schema = next(iter(schemas), None)
    if schema == V07_SOURCE_SCHEMA:
        candidate = [(path, deepcopy(frontmatter), body) for path, frontmatter, body, _data in raw]
        world = _world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, candidate)
        diagnostics = validate_world(world)
        return [], diagnostics, not any(item["severity"] == "error" for item in diagnostics)
    if schema not in {"wedl/v0.3", THREAD_SOURCE_SCHEMA, CHRONOLOGY_SOURCE_SCHEMA}:
        return [], [_diagnostic("GEN-VERSION-001", "upgrade-v07 requires homogeneous wedl/v0.3, wedl/v0.5, wedl/v0.6, or wedl/v0.7 source", field="schema")], False
    legacy = [(path, deepcopy(frontmatter), body) for path, frontmatter, body, _data in raw]
    legacy_world = _world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, legacy)
    legacy_diagnostics = validate_world(legacy_world)
    if any(item["severity"] == "error" for item in legacy_diagnostics):
        return [], legacy_diagnostics, False
    worlds = [(path, frontmatter) for path, frontmatter, _body, _data in raw if frontmatter.get("kind") == "world"]
    if len(worlds) != 1:
        return [], [_diagnostic("GEN-VERSION-001", "upgrade-v07 requires exactly one world record", field="kind")], False
    world_path, _world = worlds[0]
    candidate: list[tuple[str, dict[str, Any], str]] = []
    for path, original, body, _data in raw:
        frontmatter = deepcopy(original)
        frontmatter["schema"] = V07_SOURCE_SCHEMA
        if path == world_path:
            frontmatter["capabilities"] = list(V07_DEFAULT_CAPABILITIES)
        else:
            frontmatter.pop("capabilities", None)
        candidate.append((path, frontmatter, body))
    return candidate, [], False


_PROHIBITED_V04_KEYS = frozenset((
    "parent", "fork", "fork_at", "retcon", "retcons", "sync", "synchronization", "synchronizations",
    "handoff", "handoffs", "vector", "vectors", "synchronized_horizon", "presentation", "presentation_frame",
    "presentation_frames", "author_all", "author-all",
))


def _walk_mapping(value: Any, *, path: str = "") -> list[tuple[str, Any, str]]:
    result: list[tuple[str, Any, str]] = []
    if isinstance(value, dict):
        for key, child in value.items():
            key_text = str(key); child_path = f"{path}.{key_text}" if path else key_text
            result.append((key_text, child, child_path)); result.extend(_walk_mapping(child, path=child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            result.extend(_walk_mapping(child, path=f"{path}[{index}]"))
    return result


def _v04_candidate(repository: Repository, snapshot: Snapshot, raw: list[tuple[str, dict[str, Any], str, bytes]]) -> tuple[list[tuple[str, dict[str, Any], str]], list[dict[str, Any]], bool]:
    schemas = {frontmatter.get("schema") for _path, frontmatter, _body, _data in raw}
    if schemas != {V04_SOURCE_SCHEMA}:
        return [], [_diagnostic("WDL-MIG-010", "recover-v04 requires a homogeneous wedl/v0.4 source", field="schema")], False
    worlds = [(path, frontmatter) for path, frontmatter, _body, _data in raw if frontmatter.get("kind") == "world"]
    if len(worlds) != 1:
        return [], [_diagnostic("WDL-MIG-011", "recover-v04 requires exactly one world record", field="kind")], False
    world_path, world = worlds[0]
    for path, frontmatter, _body, _data in raw:
        for key, _value, field in _walk_mapping(frontmatter):
            if key.casefold() in _PROHIBITED_V04_KEYS:
                return [], [_diagnostic("WDL-MIG-012", "v0.4 topology requires an author-supplied mapping", path=path, field=field)], False
    declarations = world.get("continuities")
    continuity_records = [(path, frontmatter) for path, frontmatter, _body, _data in raw if frontmatter.get("kind") == "continuity"]
    declaration: dict[str, Any] | None = None
    if isinstance(declarations, list) and len(declarations) == 1 and isinstance(declarations[0], dict):
        declaration = declarations[0]
        if continuity_records:
            return [], [_diagnostic("WDL-MIG-013", "v0.4 continuity declaration is ambiguous; an author-supplied mapping is required", path=world_path, field="continuities")], False
    elif declarations is None and len(continuity_records) == 1:
        declaration = continuity_records[0][1]
    else:
        return [], [_diagnostic("WDL-MIG-013", "v0.4 recovery requires one sole primary continuity; an author-supplied mapping is required", path=world_path, field="continuities")], False
    identifier = declaration.get("id") if declaration else None
    timeline = declaration.get("timeline") if declaration else None
    if not isinstance(identifier, str) or not identifier or declaration.get("status") != "primary" or world.get("default_continuity") != identifier:
        return [], [_diagnostic("WDL-MIG-014", "v0.4 recovery requires the sole primary continuity to match default_continuity", path=world_path, field="default_continuity")], False
    if not isinstance(timeline, str) or not timeline:
        return [], [_diagnostic("WDL-MIG-015", "v0.4 recovery requires one declared legacy timeline", path=world_path, field="continuities[0].timeline")], False
    if declaration.get("strands", []) != [] or world.get("strands", []) != []:
        return [], [_diagnostic("WDL-MIG-016", "v0.4 recovery permits only global empty strands", path=world_path, field="strands")], False
    timelines = world.get("timelines")
    if not isinstance(timelines, list) or len(timelines) != 1 or not isinstance(timelines[0], dict) or timelines[0].get("id") != timeline or world.get("default_timeline") != timeline:
        return [], [_diagnostic("WDL-MIG-017", "v0.4 recovery requires one legacy timeline matching the sole continuity", path=world_path, field="timelines")], False
    horizon = world.get("horizon")
    if horizon is not None and (isinstance(horizon, bool) or not isinstance(horizon, int)):
        return [], [_diagnostic("WDL-MIG-018", "v0.4 recovery accepts only a scalar global horizon", path=world_path, field="horizon")], False
    for path, frontmatter, _body, _data in raw:
        if path != world_path and frontmatter.get("strands", []) != []:
            return [], [_diagnostic("WDL-MIG-016", "v0.4 recovery permits only global empty strands", path=path, field="strands")], False
        if path != world_path and frontmatter.get("continuity") is not None and frontmatter.get("continuity") != identifier:
            return [], [_diagnostic("WDL-MIG-019", "record continuity does not match the sole primary continuity", path=path, field="continuity")], False
        for key, value, field in _walk_mapping(frontmatter):
            if key == "timeline" and value != timeline:
                return [], [_diagnostic("WDL-MIG-020", "every v0.4 coordinate must use the sole legacy timeline", path=path, field=field)], False
    candidate: list[tuple[str, dict[str, Any], str]] = []
    for path, original, body, _data in raw:
        if original.get("kind") == "continuity":
            continue
        frontmatter = deepcopy(original); frontmatter["schema"] = THREAD_SOURCE_SCHEMA
        for field in ("continuity", "continuities", "default_continuity", "strands", "horizon", "current_horizon"):
            frontmatter.pop(field, None)
        if path == world_path:
            frontmatter["threads"] = []
            if horizon is not None:
                frontmatter["current_time"] = {"timeline": timeline, "tick": horizon, "order": 0}
        else:
            frontmatter.pop("threads", None)
        candidate.append((path, frontmatter, body))
    return candidate, [], False


def _raw_changes(previous: dict[str, bytes], proposed: dict[str, bytes]) -> tuple[dict[str, bytes | None], str]:
    changes: dict[str, bytes | None] = {}
    diff: list[str] = []
    for path in sorted(set(previous) | set(proposed)):
        before, after = previous.get(path), proposed.get(path)
        if before == after:
            continue
        changes[path] = after
        diff.extend(difflib.unified_diff((before or b"").decode("utf-8").splitlines(True), (after or b"").decode("utf-8").splitlines(True), fromfile=f"a/{path}", tofile=f"b/{path}"))
    return changes, "".join(diff)


def _rollback_plan(repository: Repository, request: dict[str, Any], snapshot: Snapshot) -> dict[str, Any]:
    backup_ref = str(request["rollbackBackupRef"])
    current_oid, current_hash = _rollback_identity(repository, backup_ref)
    if current_oid != request["rollbackBackupOid"] or current_hash != request["rollbackBackupSourceSnapshotHash"]:
        raise StaleRevision("rollback backup ref changed after preview", details={"rollbackBackupRef": backup_ref})
    source = repository.snapshot(current_oid)
    schemas = {frontmatter.get("schema") for _path, frontmatter, _body, _data in _raw_records(source)}
    # A v0.4 backup is intentionally restored byte-for-byte only here.  The
    # ordinary loader remains quarantined and no derived cache is rebuilt.
    skip_compile = schemas == {V04_SOURCE_SCHEMA}
    if not skip_compile:
        candidate = [(path, frontmatter, body) for path, frontmatter, body, _data in _raw_records(source)]
        world = _world_for_candidate(repository, request["expectedHead"], snapshot.tree_oid, candidate)
        diagnostics = validate_world(world)
        if any(item["severity"] == "error" for item in diagnostics):
            return {"valid": False, "diagnostics": diagnostics, "changes": {}, "diff": "", "noOp": False, "skipCompile": False}
    changes, diff = _raw_changes(snapshot.files, source.files)
    return {"valid": True, "diagnostics": [], "changes": changes, "diff": diff, "noOp": not changes, "skipCompile": skip_compile}


def _plan(repository: Repository, request: dict[str, Any], snapshot: Snapshot) -> dict[str, Any]:
    if request["mode"] == "rollback":
        return _rollback_plan(repository, request, snapshot)
    raw = _raw_records(snapshot)
    mode = request["mode"]
    if mode == "upgrade-v03":
        candidate, diagnostics, no_op = _v03_candidate(repository, snapshot, raw)
    elif mode == "upgrade-v06":
        candidate, diagnostics, no_op = _v06_candidate(repository, snapshot, raw)
    elif mode == "upgrade-v07":
        candidate, diagnostics, no_op = _v07_candidate(repository, snapshot, raw)
    elif mode == "recover-v04":
        candidate, diagnostics, no_op = _v04_candidate(repository, snapshot, raw)
    if diagnostics:
        return {"valid": False, "diagnostics": diagnostics, "changes": {}, "diff": "", "noOp": False}
    if no_op:
        return {"valid": True, "diagnostics": [], "changes": {}, "diff": "", "noOp": True}
    world = _world_for_candidate(repository, request["expectedHead"], snapshot.tree_oid, candidate)
    diagnostics = validate_world(world)
    valid = not any(item["severity"] == "error" for item in diagnostics)
    previous = {path: data for path, _frontmatter, _body, data in raw}
    proposed = {path: serialize_record(frontmatter, body) for path, frontmatter, body in candidate}
    changes, diff = _raw_changes(previous, proposed)
    return {"valid": valid, "diagnostics": diagnostics, "changes": changes, "diff": diff, "noOp": not changes, "skipCompile": False}


def preview(repository: Repository, request: dict[str, Any]) -> dict[str, Any]:
    """Plan a migration without loading the source cache or writing anything."""

    if not repository.is_git:
        raise RepositoryError("Git is required for source migration")
    repository.assert_clean_managed()
    snapshot = repository.snapshot("HEAD")
    normalized = _normalized_request(repository, request, snapshot)
    plan = _plan(repository, normalized, snapshot)
    request_hash = _request_hash(normalized)
    return {
        "protocol": PROTOCOL, "phase": "preview", "mode": normalized["mode"], "valid": plan["valid"], "noOp": plan["noOp"],
        "expectedHead": normalized["expectedHead"], "sourceSnapshotHash": normalized["sourceSnapshotHash"], "idempotencyKey": normalized["idempotencyKey"],
        **({"targetCapabilities": normalized["targetCapabilities"]} if "targetCapabilities" in normalized else {}),
        "backupRef": normalized["backupRef"], **({"rollbackBackupRef": normalized["rollbackBackupRef"], "rollbackBackupOid": normalized["rollbackBackupOid"], "rollbackBackupSourceSnapshotHash": normalized["rollbackBackupSourceSnapshotHash"]} if "rollbackBackupRef" in normalized else {}),
        "requestHash": request_hash, "confirmationToken": _confirmation_token(normalized, request_hash), "diagnostics": plan["diagnostics"],
        "files": sorted(plan["changes"]), "diff": plan["diff"], "_changes": plan["changes"], "_request": normalized,
    }


def apply(repository: Repository, request: dict[str, Any], *, confirmation_token_value: str | None = None) -> dict[str, Any]:
    """Apply exactly one previewed local migration as a forward Git commit."""

    # Do this before the receipt fast path: a receipt is never permission to
    # replay a partial or differently-bound request.
    _mode, _expected, _source_hash_value, key, _rollback_ref, _backup_ref = _request_fields(request, require_source_hash=True)
    receipt_path = repository.root / ".wedl" / "migration-receipts.json"
    if receipt_path.exists() and key:
        receipts = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt = receipts.get(key)
        if receipt is not None:
            # Normalize against the stored source identity without requiring
            # the old HEAD to remain current; rollback still resolves its ref.
            stored = receipt["request"]
            supplied = {field: request.get(field) for field in ("protocol", "mode", "expectedHead", "sourceSnapshotHash", "idempotencyKey", "rollbackBackupRef", "targetCapabilities")}
            if stored.get("mode") == "upgrade-v07" and supplied["targetCapabilities"] is None:
                supplied["targetCapabilities"] = stored.get("targetCapabilities")
            expected_identity = {field: stored.get(field) for field in supplied}
            if supplied != expected_identity:
                raise ConflictError("migration idempotency key was used for a different request")
            if request.get("backupRef") is not None and request.get("backupRef") != stored.get("backupRef"):
                raise ConflictError("migration idempotency key was used for a different request")
            if stored.get("mode") == "rollback":
                oid, backup_hash = _rollback_identity(repository, stored["rollbackBackupRef"])
                if oid != stored.get("rollbackBackupOid") or backup_hash != stored.get("rollbackBackupSourceSnapshotHash"):
                    raise StaleRevision("rollback backup ref changed after preview", details={"rollbackBackupRef": stored["rollbackBackupRef"]})
            if not confirmation_token_value:
                raise ConfirmationRequired("migration apply requires a preview confirmation token")
            if confirmation_token_value != receipt["confirmationToken"]:
                raise ConfirmationMismatch("migration confirmation token does not match the previewed request")
            return {**receipt["result"], "idempotentReplay": True}
    result = preview(repository, request)
    if not result["valid"]:
        raise ValidationFailed("migration candidate is invalid", result["diagnostics"])
    if not confirmation_token_value:
        raise ConfirmationRequired("migration apply requires a preview confirmation token")
    if confirmation_token_value != result["confirmationToken"]:
        raise ConfirmationMismatch("migration confirmation token does not match the previewed request")
    if result["noOp"]:
        return {key: value for key, value in result.items() if key not in {"_changes", "_request", "diff", "files"}} | {"phase": "apply", "status": "noop", "idempotentReplay": False}
    normalized = result["_request"]
    # Admission is deliberately after preview/confirmation, but before the
    # backup ref.  The same private cell continues through commit_files.
    external_lock_deadline: list[float | None] = [None]
    waited = repository._await_external_index_lock_release(deadline=external_lock_deadline)
    if waited:
        repository.assert_clean_managed()
        actual = repository.head()
        if actual != normalized["expectedHead"]:
            raise StaleRevision("migration expected a different HEAD", details={"expected": normalized["expectedHead"], "actual": actual})
    repository.ensure_backup_ref(normalized["backupRef"], normalized["expectedHead"])
    commit = repository.commit_files(
        expected_head=normalized["expectedHead"], files=result["_changes"],
        message=f"wedl: {normalized['mode']} source migration",
        trailers={"Wedl-Migration": result["requestHash"], "Wedl-Backup": normalized["backupRef"]},
        _external_lock_deadline=external_lock_deadline,
    )
    skip_compile = normalized["mode"] == "rollback" and {
        frontmatter.get("schema") for _path, frontmatter, _body, _data in _raw_records(repository.snapshot(normalized["rollbackBackupOid"]))
    } == {V04_SOURCE_SCHEMA}
    compile_report = None if skip_compile else compile_world(repository)
    response = {
        "protocol": PROTOCOL, "phase": "apply", "status": "committed", "mode": normalized["mode"], "previousHead": normalized["expectedHead"],
        "newHead": commit, "sourceSnapshotHash": normalized["sourceSnapshotHash"], "idempotencyKey": normalized["idempotencyKey"],
        "backupRef": normalized["backupRef"], **({"targetCapabilities": normalized["targetCapabilities"]} if "targetCapabilities" in normalized else {}), **({"rollbackBackupRef": normalized["rollbackBackupRef"], "rollbackBackupOid": normalized["rollbackBackupOid"], "rollbackBackupSourceSnapshotHash": normalized["rollbackBackupSourceSnapshotHash"]} if "rollbackBackupRef" in normalized else {}),
        "requestHash": result["requestHash"], "compile": compile_report, "compileSkipped": skip_compile, "idempotentReplay": False,
    }
    receipts = json.loads(receipt_path.read_text(encoding="utf-8")) if receipt_path.exists() else {}
    receipts[normalized["idempotencyKey"]] = {"request": normalized, "confirmationToken": result["confirmationToken"], "result": response}
    atomic_write(receipt_path, (json.dumps(receipts, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    return response


__all__ = ["BACKUP_PREFIX", "MODES", "PROTOCOL", "V07_DEFAULT_CAPABILITIES", "apply", "preview"]
