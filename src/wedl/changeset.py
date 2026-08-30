from __future__ import annotations

from copy import deepcopy
import difflib
import hashlib
import json
from pathlib import Path
from typing import Any

from . import SOURCE_SCHEMA, __version__
from .compiler import compile_world
from .errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, StaleRevision, UsageError, ValidationFailed
from .ids import id_from_seed, new_id
from .model import Record, World
from .repository import Repository
from .source import generated_path, serialize_record
from .util import atomic_write, canonical_json, deep_replace, slugify
from .validation import validate_world


class ProtocolError(UsageError):
    code = "protocol_error"


CHANGESET_OPERATION_SCHEMA = [
    {
        "type": "entity.create",
        "summary": "create an authored entity from frontmatter and optional bodyMarkdown",
        "fields": ["temporaryId", "value"],
    },
    {
        "type": "entity.upsert",
        "summary": "create or replace an authored entity by ID",
        "fields": ["temporaryId or value.id", "value"],
    },
    {
        "type": "entity.update",
        "summary": "patch an existing entity's frontmatter and/or bodyMarkdown; thread declarations and memberships are full replacements",
        "fields": ["entity or entityId", "frontmatterPatch and/or bodyMarkdown (world threads; ordinary threadIds)"],
    },
    {
        "type": "entity.delete",
        "summary": "delete an existing entity",
        "fields": ["entity or entityId"],
    },
    {
        "type": "event.create",
        "summary": "create a timed event with participants, causes, and effects",
        "fields": ["temporaryId", "title", "time"],
    },
    {
        "type": "conversation.create",
        "summary": "create a conversation with participants, turns, and recollections",
        "fields": ["temporaryId", "value"],
    },
    {
        "type": "conversation.turn.append",
        "summary": "append one speech or action beat to an existing or newly-created conversation",
        "fields": ["conversationId or conversation", "turn"],
    },
    {
        "type": "conversation.recollection.record",
        "summary": "record one character recollection for a conversation",
        "fields": ["conversationId or conversation", "recollection"],
    },
]

# Keep the protocol's advertised vocabulary and its executable dispatcher in
# one place.  The schema is user-facing, so accepting an operation that it
# does not advertise (or advertising one that cannot be applied) is a
# contract error rather than merely a documentation drift.
CHANGESET_OPERATION_TYPES = tuple(item["type"] for item in CHANGESET_OPERATION_SCHEMA)


def schema() -> dict[str, Any]:
    """Return concise, machine-readable guidance for the changeset protocol."""

    return {
        "protocol": "wedl-changeset-schema/v1",
        "changesetProtocol": "wedl-changeset/v1",
        "required": ["protocol", "expectedHead", "idempotencyKey", "summary", "operations"],
        "notes": [
            "expectedHead must equal the repository HEAD used for preview or apply.",
            "Use a stable idempotencyKey for retries of the same request.",
            "Preview before applying any edited changeset, then pass its confirmationToken to apply --confirm.",
            "--yes is an explicit unsafe CLI bypass for deliberate one-shot automation; confirmation is not authorization.",
            "For entity.update, world frontmatterPatch.threads replaces the complete thread declaration list.",
            "For entity.update, an ordinary non-hypothesis record's frontmatterPatch.threadIds replaces its complete membership list and is serialized as source frontmatter threads.",
        ],
        "operations": CHANGESET_OPERATION_SCHEMA,
    }


def scaffold(repository: Repository) -> dict[str, Any]:
    """Build a valid starter request tied to the repository's current HEAD.

    The starter includes a deliberate no-op update against an existing record,
    because an empty or placeholder operation cannot be previewed. Authors
    replace that operation while keeping the request envelope and HEAD guard.
    """

    expected_head = repository.head()
    world = repository.load_world(expected_head)
    if not world.records:
        raise ProtocolError("cannot scaffold a changeset for a world with no records")
    record = min(world.records.values(), key=lambda value: value.id)
    return {
        "protocol": "wedl-changeset/v1",
        "expectedHead": expected_head,
        "idempotencyKey": f"wedl-scaffold-{expected_head[:12]}",
        "summary": "Describe the canonical story change (thread declarations and memberships replace complete lists)",
        "operations": [
            {
                "type": "entity.update",
                "entity": record.id,
                "frontmatterPatch": {},
            }
        ],
    }


def _request_hash(payload: dict[str, Any]) -> str:
    value = deepcopy(payload)
    value.pop("requestId", None)
    return hashlib.sha256(canonical_json(value).encode()).hexdigest()


def confirmation_token(payload: dict[str, Any], *, request_hash: str, expected_head: str) -> str:
    """Return a versioned proof that a complete request was previewed at HEAD.

    This is deliberately separate from :func:`_request_hash`: request IDs are
    ignored by the latter for established generated-ID/idempotency behaviour,
    but are part of this full-payload safety proof.  It is a checksum, not an
    authorization credential.
    """

    material = {
        "domain": "wedl-confirmation/v1",
        "expectedHead": expected_head,
        "payload": payload,
        "requestHash": request_hash,
    }
    digest = hashlib.sha256(canonical_json(material).encode()).hexdigest()
    return f"wedl-confirmation/v1:{digest}"


def _full_payload_hash(payload: dict[str, Any]) -> str:
    """Hash every supplied JSON field for receipt replay safety."""

    return hashlib.sha256(canonical_json(payload).encode()).hexdigest()


def _temporary(value: dict[str, Any]) -> str | None:
    return value.get("temporaryId") or value.get("tempId")


def _allocate(payload: dict[str, Any]) -> dict[str, str]:
    digest = _request_hash(payload)
    generated: dict[str, str] = {}
    for index, operation in enumerate(payload.get("operations") or []):
        temporary = _temporary(operation)
        operation_type = operation.get("type")
        kind = None
        if operation_type == "event.create": kind = "event"
        elif operation_type == "conversation.create": kind = "conversation"
        elif operation_type in {"entity.create", "entity.upsert"}:
            value = operation.get("value") or {}
            kind = (value.get("frontmatter") or value).get("kind") if isinstance(value, dict) else None
        if temporary and kind:
            generated[str(temporary)] = id_from_seed(str(kind), f"{digest}:{index}:{temporary}")
        elif temporary and operation_type == "conversation.turn.append":
            generated[str(temporary)] = id_from_seed("conversation-turn", f"{digest}:{index}:{temporary}")
        elif temporary and operation_type == "conversation.recollection.record":
            generated[str(temporary)] = id_from_seed("conversation-recollection", f"{digest}:{index}:{temporary}")
        if operation_type == "knowledge.transition":
            value = operation.get("knowledge") or {}
            create = value.get("create") if isinstance(value, dict) else None
            if isinstance(create, dict) and (temp := _temporary(create)):
                generated[str(temp)] = id_from_seed("knowledge", f"{digest}:{index}:{temp}")
        # Chronology replacement stays an ordinary entity.update.  Allocate
        # only its explicitly scoped declaration/annotation identifiers; free
        # prose and provenance are never examined or substituted.
        patch = operation.get("frontmatterPatch") if isinstance(operation, dict) else None
        chronology = patch.get("chronology") if isinstance(patch, dict) else None
        if operation_type == "entity.update" and isinstance(chronology, dict):
            for collection, kind in (("calendars", "calendar"), ("eras", "era"), ("anchors", "chronology")):
                for item in chronology.get(collection) or []:
                    if isinstance(item, dict) and isinstance(item.get("temporaryId"), str):
                        temporary = item["temporaryId"]
                        generated[temporary] = id_from_seed(kind, f"{digest}:{index}:{temporary}")
        elif operation_type == "entity.update" and isinstance(chronology, list):
            for item in chronology:
                if isinstance(item, dict) and isinstance(item.get("temporaryId"), str):
                    temporary = item["temporaryId"]
                    generated[temporary] = id_from_seed("chronology", f"{digest}:{index}:{temporary}")
    return generated


def _replace_chronology_identifiers(value: Any, replacements: dict[str, str], *, declaration: bool = False) -> Any:
    """Resolve only chronology identifier leaves, never arbitrary strings."""
    if isinstance(value, list): return [_replace_chronology_identifiers(item, replacements, declaration=declaration) for item in value]
    if not isinstance(value, dict): return value
    result: dict[str, Any] = {}
    temporary = value.get("temporaryId")
    for key, item in value.items():
        # Extensions are opaque authored data.  In particular, identifiers and
        # temporaryId-shaped values inside them are not chronology references.
        if isinstance(key, str) and key.startswith("x-"):
            result[key] = deepcopy(item)
            continue
        # Only declaration roots consume temporaryId; nested core values retain
        # it as ordinary data unless a schema-specific reference field applies.
        if key == "temporaryId":
            if not declaration: result[key] = deepcopy(item)
            continue
        if key in {"calendar_id", "era_id", "before_id", "after_id"} and isinstance(item, str):
            result[key] = replacements.get(item, item)
        elif key == "id" and declaration and isinstance(item, str):
            result[key] = replacements.get(item, item)
        else:
            result[key] = _replace_chronology_identifiers(item, replacements)
    if declaration and isinstance(temporary, str): result["id"] = replacements.get(temporary, temporary)
    return result


def _replace_chronology_patch(value: Any, replacements: dict[str, str]) -> Any:
    if isinstance(value, list):
        return [_replace_chronology_identifiers(item, replacements, declaration=True) for item in value]
    if not isinstance(value, dict): return value
    result = _replace_chronology_identifiers(value, replacements)
    for collection in ("calendars", "eras", "anchors"):
        if isinstance(value.get(collection), list):
            result[collection] = sorted(
                (_replace_chronology_identifiers(item, replacements, declaration=True) for item in value[collection]),
                key=lambda item: str(item.get("id") or ""),
            )
    return result


def _common(kind: str, entity_id: str, value: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema": SOURCE_SCHEMA,
        "kind": kind,
        "id": entity_id,
        "title": value.get("title") or entity_id,
        "domain": value.get("domain") or f"story.{kind}",
        "status": value.get("status") or ("active" if kind in {"scene", "conversation"} else "canonical"),
        "tags": list(value.get("tags") or []),
        "aliases": list(value.get("aliases") or []),
        **({"section_audiences": deepcopy(value.get("section_audiences") or {})} if value.get("section_audiences") else {}),
    }


def _upsert(operation: dict[str, Any], records: dict[str, Record], source_root: str) -> str:
    value = deepcopy(operation.get("value") or {})
    frontmatter = deepcopy(value.get("frontmatter") or value)
    body = str(value.get("bodyMarkdown") or value.get("body") or "")
    entity_id = str(frontmatter.get("id") or _temporary(operation) or "")
    if not entity_id:
        raise ProtocolError("entity.upsert requires an ID or temporaryId")
    frontmatter["id"] = entity_id
    frontmatter.setdefault("schema", SOURCE_SCHEMA)
    kind = str(frontmatter.get("kind") or "")
    title = str(frontmatter.get("title") or entity_id)
    existing = records.get(entity_id)
    path = existing.source_path if existing else generated_path(source_root, kind, title, entity_id, frontmatter)
    raw = serialize_record(frontmatter, body or f"# {title}\n")
    records[entity_id] = Record(frontmatter, body or f"# {title}\n", path, raw)
    return entity_id


def _event(operation: dict[str, Any], records: dict[str, Record], source_root: str, seed: str) -> str:
    entity_id = str(_temporary(operation) or operation.get("id") or id_from_seed("event", seed))
    value = deepcopy(operation)
    frontmatter = _common("event", entity_id, value)
    frontmatter.update({
        "time": value.get("time"), "location": value.get("location"),
        "participants": value.get("participants") or [], "causes": value.get("causes") or [],
        "related_story_points": value.get("relatedStoryPoints") or value.get("related_story_points") or [],
        "effects": value.get("effects") or [],
    })
    body = str(value.get("bodyMarkdown") or f"# {frontmatter['title']}\n")
    path = generated_path(source_root, "event", str(frontmatter["title"]), entity_id, frontmatter)
    records[entity_id] = Record(frontmatter, body, path, serialize_record(frontmatter, body))
    return entity_id


def _conversation(operation: dict[str, Any], records: dict[str, Record], source_root: str, seed: str) -> str:
    entity_id = str(_temporary(operation) or operation.get("id") or id_from_seed("conversation", seed))
    value = deepcopy(operation.get("value") or operation)
    frontmatter = _common("conversation", entity_id, value)
    frontmatter.update({
        "time": value.get("time"), "scene": value.get("scene") or value.get("sceneId"),
        "location": value.get("location") or value.get("locationId"),
        "participants": value.get("participants") or [], "topics": value.get("topics") or [],
        "turns": value.get("turns") or [], "recollections": value.get("recollections") or [],
    })
    body = str(value.get("bodyMarkdown") or f"# {frontmatter['title']}\n\nCanonical transcript and subjective recollections.\n")
    path = generated_path(source_root, "conversation", str(frontmatter["title"]), entity_id, frontmatter)
    records[entity_id] = Record(frontmatter, body, path, serialize_record(frontmatter, body))
    return entity_id


def _replace_operation_references(raw_operation: dict[str, Any], replacements: dict[str, str]) -> dict[str, Any]:
    """Expand entity temporary IDs without treating grouping identifiers as refs.

    ``threadIds`` is public changeset vocabulary, not authored source
    frontmatter.  Preserve both grouping patch values literally while resolving
    the ordinary entity references elsewhere in the operation.
    """

    operation = deepcopy(raw_operation)
    patch = operation.get("frontmatterPatch")
    grouping_values: dict[str, Any] = {}
    chronology_value: Any = None
    if operation.get("type") == "entity.update" and isinstance(patch, dict):
        grouping_values = {key: patch.pop(key) for key in ("threads", "threadIds") if key in patch}
        if "chronology" in patch: chronology_value = patch.pop("chronology")
    operation = deep_replace(operation, replacements)
    if grouping_values:
        operation["frontmatterPatch"].update(grouping_values)
    if chronology_value is not None:
        operation["frontmatterPatch"]["chronology"] = _replace_chronology_patch(chronology_value, replacements)
    return operation


def _canonical_frontmatter_patch(record: Record, patch: dict[str, Any]) -> dict[str, Any]:
    """Validate changeset-only grouping keys and map memberships to source.

    Source Markdown intentionally has one canonical spelling: ``threads``.
    The public ``threadIds`` spelling is accepted only for ordinary record
    membership changes, where it always replaces the complete membership list.
    The complete candidate is validated later, atomically, by the v0.5 source
    validator.
    """

    has_threads = "threads" in patch
    has_thread_ids = "threadIds" in patch
    if not (has_threads or has_thread_ids):
        return patch
    if has_threads and has_thread_ids:
        raise ProtocolError("frontmatterPatch cannot contain both threads and threadIds")
    if record.kind == "hypothesis":
        raise ProtocolError("hypothesis records cannot carry thread grouping")
    if record.kind == "world":
        if has_thread_ids:
            raise ProtocolError("world thread declarations use frontmatterPatch.threads")
        return patch
    if has_threads:
        raise ProtocolError("non-world thread memberships use frontmatterPatch.threadIds")
    canonical = dict(patch)
    canonical["threads"] = canonical.pop("threadIds")
    return canonical


def _apply(payload: dict[str, Any], world: World, replacements: dict[str, str]) -> tuple[dict[str, Record], set[str]]:
    records = {entity_id: Record(deepcopy(record.frontmatter), record.body, record.source_path, record.raw_bytes, record.blob_oid, record.revision) for entity_id, record in world.records.items()}
    touched: set[str] = set()
    for index, raw_operation in enumerate(payload.get("operations") or []):
        operation = _replace_operation_references(raw_operation, replacements)
        operation_type = operation.get("type")
        if operation_type not in CHANGESET_OPERATION_TYPES:
            raise ProtocolError(f"unsupported operation {operation_type!r}")
        candidate = operation.get("value") or {}
        if (isinstance(operation.get("frontmatterPatch"), dict) and "importance" in operation["frontmatterPatch"]) or (isinstance(candidate, dict) and ("importance" in candidate or (isinstance(candidate.get("frontmatter"), dict) and "importance" in candidate["frontmatter"]))):
            raise ProtocolError("importance is calculated output and cannot appear in changesets")
        seed = f"{_request_hash(payload)}:{index}"
        if operation_type in {"entity.create", "entity.upsert"}:
            touched.add(_upsert(operation, records, world.source_root))
        elif operation_type == "entity.delete":
            entity_id = str(operation.get("entity") or operation.get("entityId"))
            if entity_id not in records:
                raise ProtocolError(f"unknown entity {entity_id}")
            records.pop(entity_id); touched.add(entity_id)
        elif operation_type == "event.create":
            touched.add(_event(operation, records, world.source_root, seed))
        elif operation_type == "conversation.create":
            touched.add(_conversation(operation, records, world.source_root, seed))
        elif operation_type == "conversation.turn.append":
            entity_id = str(operation.get("conversationId") or operation.get("conversation"))
            record = records.get(entity_id)
            if not record or record.kind != "conversation": raise ProtocolError("conversation.turn.append requires conversation")
            turn = deepcopy(operation.get("turn") or operation.get("value") or {})
            turn.setdefault("id", str(_temporary(operation) or id_from_seed("conversation-turn", seed)))
            record.frontmatter.setdefault("turns", []).append(turn)
            record.raw_bytes = serialize_record(record.frontmatter, record.body); touched.add(entity_id)
        elif operation_type == "conversation.recollection.record":
            entity_id = str(operation.get("conversationId") or operation.get("conversation"))
            record = records.get(entity_id)
            if not record or record.kind != "conversation": raise ProtocolError("conversation.recollection.record requires conversation")
            value = deepcopy(operation.get("recollection") or operation.get("value") or {})
            value.setdefault("id", str(_temporary(operation) or id_from_seed("conversation-recollection", seed)))
            record.frontmatter.setdefault("recollections", []).append(value)
            record.raw_bytes = serialize_record(record.frontmatter, record.body); touched.add(entity_id)
        elif operation_type == "entity.update":
            entity_id = str(operation.get("entity") or operation.get("entityId")); record = records.get(entity_id)
            if not record: raise ProtocolError(f"unknown entity {entity_id}")
            patch = operation.get("frontmatterPatch") or {}
            if not isinstance(patch, dict):
                raise ProtocolError("entity.update frontmatterPatch must be an object")
            grouping_value_supplied = "threads" in patch or "threadIds" in patch
            patch = _canonical_frontmatter_patch(record, patch)
            changed = False
            for key, value in patch.items():
                if value is None and not (grouping_value_supplied and key == "threads"):
                    if key in record.frontmatter:
                        record.frontmatter.pop(key)
                        changed = True
                elif record.frontmatter.get(key) != value:
                    record.frontmatter[key] = value
                    changed = True
            if "body" in operation or "bodyMarkdown" in operation:
                body = str(operation.get("bodyMarkdown") or operation.get("body"))
                if record.body != body:
                    record.body = body
                    changed = True
            if changed:
                record.raw_bytes = serialize_record(record.frontmatter, record.body)
                touched.add(entity_id)
    return records, touched


def preview(
    repository: Repository,
    payload: dict[str, Any],
    *,
    use_current_head: bool = False,
    cache_write: bool = True,
) -> dict[str, Any]:
    expected = str(payload.get("expectedHead") or "")
    current = repository.head()
    if use_current_head or expected == "HEAD": expected = current
    if expected != current:
        raise StaleRevision("changeset expected a different HEAD", details={"expected": expected, "actual": current})
    world = repository.load_world(expected, cache_write=cache_write)
    replacements = _allocate(payload)
    records, touched = _apply(payload, world, replacements)
    candidate = World(expected, world.tree_oid, records, world.root, world.source_root)
    diagnostics = validate_world(candidate)
    valid = not any(item["severity"] == "error" for item in diagnostics)
    changes: dict[str, bytes | None] = {}
    diffs: list[str] = []
    all_paths = {record.source_path for record in world.records.values()} | {record.source_path for record in records.values()}
    old_by_path = {record.source_path: record for record in world.records.values()}
    new_by_path = {record.source_path: record for record in records.values()}
    for path in sorted(all_paths):
        old = old_by_path.get(path); new = new_by_path.get(path)
        old_data = old.raw_bytes if old else None; new_data = new.raw_bytes if new else None
        if old_data == new_data: continue
        changes[path] = new_data
        diffs.extend(difflib.unified_diff((old_data or b"").decode().splitlines(True), (new_data or b"").decode().splitlines(True), fromfile=f"a/{path}", tofile=f"b/{path}"))
    request_hash = _request_hash(payload)
    return {"protocol": "wedl-preview/v1", "valid": valid, "expectedHead": expected, "requestHash": request_hash, "confirmationToken": confirmation_token(payload, request_hash=request_hash, expected_head=expected), "generatedIds": replacements, "touchedEntityIds": sorted(touched), "diagnostics": diagnostics, "files": sorted(changes), "diff": "".join(diffs), "_changes": changes}


def apply(
    repository: Repository,
    payload: dict[str, Any],
    *,
    use_current_head: bool = False,
    confirmation_token_value: str | None = None,
    allow_unconfirmed: bool = False,
    authoring_intent_hash: str | None = None,
    authoring_impact: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Apply a previously previewed changeset, or use an explicit trusted bypass.

    ``allow_unconfirmed`` exists solely for deliberate local/legacy callers
    such as the CLI's documented ``--yes`` flag.  Network entry points must
    always pass a confirmation token instead.
    """

    key = str(payload.get("idempotencyKey") or "")
    request_hash = _request_hash(payload)
    full_payload_hash = _full_payload_hash(payload)
    receipt_path = repository.root / ".wedl" / "idempotency.json"
    receipts: dict[str, Any] = {}
    if receipt_path.exists():
        receipts = json.loads(receipt_path.read_text())
    # Network retries must succeed even though the original request's expected
    # HEAD is now stale. Verify the canonical request hash before any planning.
    if key and key in receipts:
        if receipts[key]["requestHash"] != request_hash:
            raise ConflictError("idempotency key was used for a different request")
        # A requestId deliberately does not perturb requestHash, but it is
        # still part of confirmation identity. Do not let an old proof replay
        # a cosmetically similar envelope with a different full payload.
        if receipts[key].get("fullPayloadHash") not in {None, full_payload_hash}:
            raise ConflictError("idempotency key was used for a different request")
        if not allow_unconfirmed:
            if not confirmation_token_value:
                raise ConfirmationRequired("changeset apply requires a preview confirmation token")
            if confirmation_token_value != receipts[key].get("confirmationToken"):
                raise ConfirmationMismatch("changeset confirmation token does not match the previewed request")
        return {**receipts[key]["result"], "idempotentReplay": True}
    # Validate before confirmation so malformed candidates keep their existing
    # diagnostics, but plan read-only: a refused mutation must not create or
    # update cache/receipt/source state.
    result = preview(repository, payload, use_current_head=use_current_head, cache_write=False)
    if not result["valid"]:
        raise ValidationFailed("changeset candidate is invalid", result["diagnostics"])
    if not allow_unconfirmed:
        if not confirmation_token_value:
            raise ConfirmationRequired("changeset apply requires a preview confirmation token")
        if confirmation_token_value != result["confirmationToken"]:
            raise ConfirmationMismatch("changeset confirmation token does not match the previewed request")
    commit = repository.commit_files(expected_head=result["expectedHead"], files=result.pop("_changes"), message=str(payload.get("summary") or "wedl: narrative change"), trailers={"Wedl-Request": request_hash, "Wedl-Idempotency": hashlib.sha256(key.encode()).hexdigest() if key else ""})
    compile_report = compile_world(repository)
    response = {"protocol": "wedl-command-result/v1", "status": "committed", "previousHead": result["expectedHead"], "newHead": commit, "generatedIds": result["generatedIds"], "touchedEntityIds": result["touchedEntityIds"], "compile": compile_report, "idempotentReplay": False}
    if key:
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt = {"requestHash": request_hash, "fullPayloadHash": full_payload_hash, "confirmationToken": result["confirmationToken"], "result": response}
        if authoring_intent_hash is not None:
            receipt["authoringIntentHash"] = authoring_intent_hash
        # This is deliberately receipt metadata, not part of the generic raw
        # changeset response. Semantic authoring retries can recover their
        # name-only helper summary without changing raw API payloads.
        if authoring_impact is not None:
            receipt["authorImpact"] = deepcopy(authoring_impact)
        receipts[key] = receipt
        atomic_write(
            receipt_path,
            (json.dumps(receipts, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
        )
    return response
