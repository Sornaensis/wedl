from __future__ import annotations

from copy import deepcopy
import difflib
import hashlib
import inspect
from itertools import chain
import json
import os
from pathlib import Path
import sqlite3
import stat
from typing import Any

from . import SOURCE_SCHEMA, __version__
from .compiler import _authoring_preconstruction_admitted, _authoring_preconstruction_bytes, _authoring_source_metadata_bytes, compile_world, compile_world_bytes
from .errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, RepositoryError, StaleRevision, UsageError, ValidationFailed
from .ids import id_from_seed, new_id
from .model import Record, World
from .repository import Repository
from .transaction_recovery import JournalLiveByteBudget, SurfaceEnrollment, TransactionJournal
from .source import generated_path, serialize_record
from .util import atomic_write, canonical_json, deep_replace, slugify
from .validation import validate_world
from . import consequence_operations


_DIRECT_COMPILE_WORLD = compile_world
_DIRECT_COMPILE_WORLD_BYTES = compile_world_bytes
_DIRECT_ATOMIC_WRITE = atomic_write
_MIB = 1024 * 1024
_AUTHORING_PROCESS_LIMIT = 512 * _MIB
# Runtime, allocator slack, and the non-byte Python request/preview/journal
# graphs receive 32 MiB each in both overlapping phase estimates.
_AUTHORING_CALLER_FIXED_RESERVE = (32 + 32 + 32) * _MIB
_AUTHORING_MAX_CACHE_BEFORE = 8 * _MIB
_AUTHORING_MAX_RECEIPT = 16 * _MIB
_AUTHORING_MAX_SOURCE = 16 * _MIB
_AUTHORING_MAX_SOURCE_PATHS = 4096
_AUTHORING_MAX_REQUEST = 8 * _MIB
_AUTHORING_MAX_REQUEST_NODES = 131072
_AUTHORING_MAX_REVISIONS = 1024


def _authoring_input_bytes(value: Any, maximum: int, *, error: str = "authoring request byte limit exceeded") -> int:
    """Price a JSON input graph before copying or encoding any payload string."""
    pending = [(iter((value,)), 0)]
    seen: set[int] = set()
    total = nodes = 0
    while pending:
        iterator, depth = pending[-1]
        try:
            item = next(iterator)
        except StopIteration:
            pending.pop()
            continue
        nodes += 1
        if nodes > _AUTHORING_MAX_REQUEST_NODES or depth > 64:
            raise RepositoryError(error)
        if isinstance(item, str):
            total += len(item) * 6
        elif item is None or isinstance(item, (bool, int, float)):
            total += 32
        elif isinstance(item, (dict, list, tuple)):
            if id(item) in seen:
                raise RepositoryError(error)
            seen.add(id(item))
            total += 64
            pending.append((
                iter(chain(item.keys(), item.values())) if isinstance(item, dict) else iter(item),
                depth + 1,
            ))
        else:
            raise RepositoryError(error)
        if total > maximum:
            raise RepositoryError(error)
    return total


def _read_bounded_receipt(path: Path) -> bytes | None:
    """Read the shared receipt through one bounded descriptor identity."""
    try:
        named = os.lstat(path)
    except FileNotFoundError:
        return None
    reparse = getattr(named, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    if reparse or path.is_symlink() or not stat.S_ISREG(named.st_mode) or named.st_size > _AUTHORING_MAX_RECEIPT:
        raise RepositoryError("idempotency receipt exceeds authoring byte limit")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0))
    try:
        initial = os.fstat(descriptor)
        identity = (initial.st_dev, initial.st_ino, initial.st_size)
        if identity != (named.st_dev, named.st_ino, named.st_size):
            raise RepositoryError("idempotency receipt changed during read")
        chunks: list[bytes] = []
        remaining = initial.st_size
        while remaining:
            chunk = os.read(descriptor, min(_MIB, remaining))
            if not chunk:
                raise RepositoryError("idempotency receipt changed during read")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1) or os.fstat(descriptor).st_size != initial.st_size:
            raise RepositoryError("idempotency receipt changed during read")
        current = os.lstat(path)
        if (current.st_dev, current.st_ino, current.st_size) != identity:
            raise RepositoryError("idempotency receipt changed during read")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def _bounded_canonical_json_bytes(value: Any, maximum: int, *, error: str) -> bytes:
    """Encode canonical JSON without constructing an unbounded intermediate."""
    # JSONEncoder emits an entire string as one chunk. Reject a string whose
    # character count alone exceeds the byte limit before it can allocate an
    # unbounded escaped chunk. A character can escape to at most six bytes.
    pending = [iter((value,))]
    seen: set[int] = set()
    while pending:
        try:
            item = next(pending[-1])
        except StopIteration:
            pending.pop()
            continue
        if isinstance(item, str):
            if len(item) > maximum:
                raise RepositoryError(error)
        elif isinstance(item, (dict, list, tuple)) and id(item) not in seen:
            seen.add(id(item))
            pending.append(iter(chain(item.keys(), item.values())) if isinstance(item, dict) else iter(item))
    encoder = json.JSONEncoder(ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    output = bytearray()
    total = 0
    for chunk in encoder.iterencode(value):
        encoded = chunk.encode("utf-8")
        total += len(encoded)
        if total > maximum:
            raise RepositoryError(error)
        output.extend(encoded)
    return bytes(output)


class ProtocolError(UsageError):
    code = "protocol_error"


def _compile_committed_world(repository: Repository, committed: str) -> dict[str, Any]:
    """Invoke the compiler with its revision when its declared callable accepts it.

    The changeset boundary has historically exposed a repository-only compiler
    seam for authoring callers.  Inspect the callable *before* invocation so a
    one-argument test or integration replacement remains supported without
    treating an actual compiler ``TypeError`` as a signature mismatch.
    """

    signature = inspect.signature(compile_world)
    try:
        signature.bind(repository, committed)
    except TypeError:
        signature.bind(repository)
        return compile_world(repository)
    return compile_world(repository, committed)


def _authoring_compile_fault_hook() -> None:
    """Preserve injected compile failures without exposing a live repository."""
    if compile_world is _DIRECT_COMPILE_WORLD:
        return
    # Compatibility replacements used by older callers are fault injectors.
    # An inert object ensures a successful replacement cannot compile into, or
    # otherwise mutate, the shared authoring cache before journal enrollment.
    _compile_committed_world(object(), "")  # type: ignore[arg-type]


def _authoring_receipt_fault_hook() -> None:
    """Preserve injected receipt failures without exposing a shared path."""
    if atomic_write is _DIRECT_ATOMIC_WRITE:
        return
    # Existing failure-injection consumers use a lambda that raises.  A
    # successful wrapper/spying replacement may delegate to the real atomic
    # writer, so invoking it here would create an extra receipt write before
    # enrollment.  Do not call such successful replacements at all; normal
    # receipt publication remains wholly owned by the transaction journal.
    if getattr(atomic_write, "__name__", None) == "<lambda>":
        atomic_write(object(), b"")  # type: ignore[arg-type]


def _authoring_cache_preflight(
    repository: Repository,
    expected_head: str,
    changes: dict[str, bytes | None],
    caller_live_bytes: int = 0,
    phase_peak: list[int] | None = None,
) -> bool:
    """Prove the cache build bound before creating the authoring journal."""
    try:
        resolved = repository.resolve(expected_head)
        required = _authoring_preconstruction_bytes(repository, resolved)
        # The committed candidate may add source bytes after the expected tree.
        # Counting every replacement as an addition is deliberately conservative
        # and avoids loading/parsing the candidate merely to price it.
        compiler_phase_peak = (
            required + caller_live_bytes + 4 * sum(len(value) for value in changes.values() if value is not None)
            if required is not None
            else None
        )
        if phase_peak is not None and compiler_phase_peak is not None:
            phase_peak.append(compiler_phase_peak)
        # This construction peak ends before register_surfaces begins. Its
        # returned buffers remain live and are priced as journal images there;
        # the whole apply peak is max(construction peak, journal peak).
        return _authoring_preconstruction_admitted(compiler_phase_peak)
    except (MemoryError, OverflowError, OSError, sqlite3.Error):
        return False


def _authoring_phase_peak(compiler_peak: int, journal_peak: int) -> int:
    """Combine sequential construction and journal phases by their live peak."""
    return max(compiler_peak, journal_peak)


def _authoring_source_preflight(repository: Repository, expected_head: str) -> int:
    """Reject unknown or oversized source before preview constructs the world."""
    try:
        source_bytes = _authoring_source_metadata_bytes(repository, repository.resolve(expected_head))
    except (MemoryError, OverflowError, OSError, ValueError) as exc:
        raise RepositoryError("authoring source byte limit exceeded") from exc
    if source_bytes is None or source_bytes > _AUTHORING_MAX_SOURCE:
        raise RepositoryError("authoring source byte limit exceeded")
    return source_bytes


def _authoring_commit_source_preflight(
    repository: Repository, changes: dict[str, bytes | None], source_bytes: int,
) -> int:
    """Bound every source image read by commit_files before calling it."""
    if len(changes) > _AUTHORING_MAX_SOURCE_PATHS:
        raise RepositoryError("authoring source byte limit exceeded")
    before_bytes = after_bytes = 0
    for relative, data in changes.items():
        target = repository.root / relative
        try:
            metadata = os.lstat(target)
        except FileNotFoundError:
            metadata = None
        if metadata is not None:
            reparse = getattr(metadata, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
            if reparse or not stat.S_ISREG(metadata.st_mode):
                raise RepositoryError("authoring source byte limit exceeded")
            before_bytes += metadata.st_size
        if data is not None:
            after_bytes += len(data) + len(relative.encode("utf-8"))
        if before_bytes > _AUTHORING_MAX_SOURCE or after_bytes > _AUTHORING_MAX_SOURCE:
            raise RepositoryError("authoring source byte limit exceeded")
    # Count replacements as additions: this admits only a candidate whose
    # source tree remains bounded even if all touched paths are new.
    if source_bytes + after_bytes > _AUTHORING_MAX_SOURCE:
        raise RepositoryError("authoring source byte limit exceeded")
    return before_bytes


def _deferred_compile_report(
    reason: str,
    *,
    record_count: int,
    report: dict[str, Any] | None = None,
    diagnostic: str | None = None,
) -> dict[str, Any]:
    """Keep the public compile-report shape when disposable caches defer."""
    deferred = dict(report or {})
    deferred.update({"status": "deferred-to-restart", "reason": reason})
    deferred.setdefault("recordCount", record_count)
    if diagnostic is not None:
        deferred["diagnostic"] = diagnostic
    return deferred


def _bounded_retained_revisions(directory: Path) -> list[Path] | None:
    """Inspect at most the admitted number of directory entries."""
    if not directory.is_dir():
        return []
    retained: list[tuple[int, str, Path]] = []
    scanned = 0
    try:
        with os.scandir(directory) as entries:
            for entry in entries:
                scanned += 1
                if scanned > _AUTHORING_MAX_REVISIONS:
                    return None
                if not entry.name.endswith(".sqlite") or not entry.is_file(follow_symlinks=False):
                    continue
                retained.append((entry.stat(follow_symlinks=False).st_mtime_ns, entry.name, Path(entry.path)))
    except OSError:
        return None
    retained.sort(key=lambda item: (-item[0], item[1]))
    return [item[2] for item in retained]


def _authoring_cache_enrollments(
    repository: Repository,
    journal: TransactionJournal,
    *,
    admitted: bool = True,
    record_count: int,
    caller_live_bytes: int = 0,
) -> tuple[dict[str, Any], str, tuple[SurfaceEnrollment, ...]]:
    """Prepare every bounded cache surface before the one journal admission."""
    shared = repository.root / ".wedl"
    committed = str(journal.record["committedHead"])
    revision_dir = shared / "revisions"
    if not admitted:
        return _deferred_compile_report(
            "authoring-preconstruction-memory-limit", record_count=record_count,
        ), committed, ()
    retained_before = _bounded_retained_revisions(revision_dir)
    if retained_before is None:
        return _deferred_compile_report(
            "authoring-revision-enumeration-limit", record_count=record_count,
        ), committed, ()
    # These public before-images are already known from their paths. Admit
    # their aggregate before constructing either in-memory database.
    before_paths = (
        shared / "world.sqlite",
        shared / "vector-cache-v2.sqlite",
        revision_dir / f"{committed}.sqlite",
        *retained_before[4:],
    )
    before_size = 0
    try:
        for public_path in dict.fromkeys(before_paths):
            try:
                metadata = os.lstat(public_path)
            except FileNotFoundError:
                continue
            if not stat.S_ISREG(metadata.st_mode) or (
                getattr(metadata, "st_file_attributes", 0)
                & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
            ):
                return _deferred_compile_report(
                    "authoring-cache-before-size-limit", record_count=record_count,
                ), committed, ()
            before_size += metadata.st_size
            if before_size > _AUTHORING_MAX_CACHE_BEFORE:
                return _deferred_compile_report(
                    "authoring-cache-before-size-limit", record_count=record_count,
                ), committed, ()
    except OSError:
        return _deferred_compile_report(
            "authoring-cache-before-size-limit", record_count=record_count,
        ), committed, ()
    # Keep the longstanding changeset compiler seam observable for integrations
    # which deliberately inject a post-commit compile failure. Normal authoring
    # never calls the disk-backed compiler; its bounded byte builder remains the
    # sole cache producer.
    _authoring_compile_fault_hook()
    _authoring_receipt_fault_hook()
    try:
        result = (
            compile_world_bytes(repository, committed, caller_live_bytes=caller_live_bytes)
            if compile_world_bytes is _DIRECT_COMPILE_WORLD_BYTES
            else compile_world_bytes(repository, committed)
        )
    except (MemoryError, OverflowError, sqlite3.Error) as exc:
        # Cache products are disposable. A bounded authoring build must never
        # turn a capacity failure into a partially enrolled cache set.
        return _deferred_compile_report(
            "authoring-cache-build-capacity-limit",
            record_count=record_count,
            diagnostic=f"{type(exc).__name__}: {exc}",
        ), committed, ()
    report = dict(result.report)
    if result.status != "compiled" or result.world_bytes is None or result.revision_bytes is None:
        return report, committed, ()
    outputs: dict[str, bytes | None] = {
        ".wedl/world.sqlite": result.world_bytes,
        f".wedl/revisions/{committed}.sqlite": result.revision_bytes,
    }
    if result.vector_bytes is not None:
        outputs[".wedl/vector-cache-v2.sqlite"] = result.vector_bytes
    # Construction retains each immutable buffer once. The journal's v7
    # admission independently prices every final surface, including the
    # intentionally aliased world/revision bytes.
    unique_outputs = {id(value): value for value in outputs.values() if value is not None}
    if sum(len(value) for value in unique_outputs.values()) > 96 * 1024 * 1024:
        return _deferred_compile_report(
            "authoring-cache-aggregate-size-limit", record_count=record_count, report=report,
        ), committed, ()

    # The in-memory compiler starts without old revision copies. Its final
    # retained set is the new revision plus the newest four existing ones.
    for stale in retained_before[4:]:
        outputs[f".wedl/revisions/{stale.name}"] = None

    before_sizes = [
        public_path.stat().st_size
        for public in outputs
        if (public_path := repository.root / public).is_file()
    ]
    if sum(before_sizes) > _AUTHORING_MAX_CACHE_BEFORE:
        return _deferred_compile_report(
            "authoring-cache-before-size-limit", record_count=record_count, report=report,
        ), committed, ()
    enrollments = tuple(
        SurfaceEnrollment(
            public,
            after=after,
            capture_before=True,
            max_before_bytes=_AUTHORING_MAX_CACHE_BEFORE,
            role="cache",
        )
        for public, after in sorted(outputs.items())
    )
    report["database"] = str(shared / "world.sqlite")
    return report, committed, enrollments


def _authoring_caller_reserve(
    enrollments: tuple[SurfaceEnrollment, ...],
    *,
    changes: dict[str, bytes | None],
    diff: str,
    authoring_impact: bytes | None,
    request_bytes: int = 0,
    source_before_bytes: int = 0,
) -> int:
    """Reserve live caller graphs and runtime headroom outside journal images."""
    receipt_images = [
        value for enrollment in enrollments if enrollment.role == "receipt"
        for value in (enrollment.before, enrollment.after) if value is not None
    ]
    # Cache images are priced by the journal. Receipt graphs, pending source
    # changes, preview diff, and impact input remain owned by this caller while
    # enrollment allocates its own ledger. The fixed reserve covers Python,
    # SQLite and allocator state that does not scale with these byte strings.
    return (
        _AUTHORING_CALLER_FIXED_RESERVE
        + 8 * sum(len(value) for value in receipt_images)
        + 4 * sum(len(value) for value in changes.values() if value is not None)
        + 4 * len(diff)
        + 8 * len(authoring_impact or b"")
        + 8 * request_bytes
        + 8 * source_before_bytes
    )


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
CHANGESET_OPERATION_SCHEMA.extend([
    {"type": operation_type, "summary": "append an explicitly authored consequence without replacing history",
     "fields": fields}
    for operation_type, fields in (
        ("knowledge.create", ["temporaryId", "value.frontmatter", "value.bodyMarkdown"]),
        ("relationship.create", ["temporaryId", "value.frontmatter", "value.bodyMarkdown"]),
        ("knowledge.transition.append", ["knowledge", "transition.time", "transition.state", "transition.causing_event"]),
        ("relationship.transition.append", ["relationship", "transition.time", "transition.causing_event"]),
        ("story-point.transition.append", ["storyPoint", "transition.time", "transition.state", "transition.causing_event"]),
        ("outcome.link", ["event", "storyPoints", "scenes"]),
    )
])
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
    value = consequence_operations.normalize(payload) if _has_consequence_operations(payload) else deepcopy(payload)
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


def _has_consequence_operations(payload: dict[str, Any]) -> bool:
    return any(
        isinstance(operation, dict) and (
            operation.get("type") in consequence_operations.TYPES
            or consequence_operations.is_typed_event(operation)
        ) for operation in payload.get("operations") or []
    )


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
    return consequence_operations.allocate(payload, generated, digest) if _has_consequence_operations(payload) else generated


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
    typed = _has_consequence_operations(payload)
    digest = _request_hash(payload)
    operations = (consequence_operations.normalize(payload) if typed else payload).get("operations") or []
    links: list[dict[str, Any]] = []
    declared: list[dict[str, Any]] = []
    for index, raw_operation in enumerate(operations):
        operation = (consequence_operations.expand(raw_operation, replacements, digest, index, world)
                     if raw_operation.get("type") in consequence_operations.TYPES or consequence_operations.is_typed_event(raw_operation)
                     else _replace_operation_references(raw_operation, replacements))
        operation_type = operation.get("type")
        if operation_type in consequence_operations.TYPES or consequence_operations.is_typed_event(raw_operation):
            declared.append(operation)
        if operation_type not in CHANGESET_OPERATION_TYPES:
            raise ProtocolError(f"unsupported operation {operation_type!r}")
        candidate = operation.get("value") or {}
        if (isinstance(operation.get("frontmatterPatch"), dict) and "importance" in operation["frontmatterPatch"]) or (isinstance(candidate, dict) and ("importance" in candidate or (isinstance(candidate.get("frontmatter"), dict) and "importance" in candidate["frontmatter"]))):
            raise ProtocolError("importance is calculated output and cannot appear in changesets")
        seed = f"{digest}:{index}"
        if operation_type == "outcome.link":
            links.append(operation)
        elif operation_type in consequence_operations.TYPES:
            touched.update(consequence_operations.apply_operation(operation, records, world))
        elif operation_type in {"entity.create", "entity.upsert"}:
            touched.add(_upsert(operation, records, world.source_root))
        elif operation_type == "entity.delete":
            entity_id = str(operation.get("entity") or operation.get("entityId"))
            if entity_id not in records:
                raise ProtocolError(f"unknown entity {entity_id}")
            records.pop(entity_id); touched.add(entity_id)
        elif operation_type == "event.create":
            new_typed_event = consequence_operations.is_typed_event(raw_operation)
            event_id = str(_temporary(operation) or operation.get("id") or id_from_seed("event", seed))
            if new_typed_event and event_id in records:
                raise UsageError("typed event cannot replace an existing record")
            identifier = _event(operation, records, world.source_root, seed)
            if new_typed_event:
                record = records[identifier]
                record.frontmatter["schema"] = world.schema
                record.raw_bytes = serialize_record(record.frontmatter, record.body)
            touched.add(identifier)
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
    for operation in links:
        touched.update(consequence_operations.apply_operation(operation, records, world))
    for operation in declared:
        consequence_operations.admit_references(operation, records, world)
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
    return {"protocol": "wedl-preview/v1", "valid": valid, "expectedHead": expected, "requestHash": request_hash, "confirmationToken": confirmation_token(payload, request_hash=request_hash, expected_head=expected), "generatedIds": replacements, "touchedEntityIds": sorted(touched), "diagnostics": diagnostics, "files": sorted(changes), "diff": "".join(diffs), "_changes": changes, "_recordCount": len(records)}


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

    # A previous process may have advanced the ref before it reached receipt,
    # cache, real-index, or finalization publication.  Reconcile that durable
    # transaction before inspecting receipts: otherwise an idempotent retry can
    # either plan against a stale expected HEAD or return a published receipt
    # while its transaction still owns an incomplete index/cache state.
    repository.recover_authoring_transactions()
    request_bytes = _authoring_input_bytes(payload, _AUTHORING_MAX_REQUEST)
    impact_input_bytes = (
        _authoring_input_bytes(
            authoring_impact, _AUTHORING_MAX_RECEIPT,
            error="idempotency receipt exceeds authoring byte limit",
        )
        if authoring_impact is not None else 0
    )
    expected_head = str(payload.get("expectedHead") or "")
    if use_current_head or expected_head == "HEAD":
        expected_head = repository.head()
    source_bytes = _authoring_source_preflight(repository, expected_head)
    key = str(payload.get("idempotencyKey") or "")
    request_hash = _request_hash(payload)
    full_payload_hash = _full_payload_hash(payload)
    receipt_path = repository.root / ".wedl" / "idempotency.json"
    receipt_before = _read_bounded_receipt(receipt_path)
    receipts: dict[str, Any] = {}
    if receipt_before is not None:
        receipts = json.loads(receipt_before.decode("utf-8"))
    bounded_authoring_impact: bytes | None = None
    if authoring_impact is not None:
        bounded_authoring_impact = _bounded_canonical_json_bytes(
            authoring_impact,
            _AUTHORING_MAX_RECEIPT,
            error="idempotency receipt exceeds authoring byte limit",
        )
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
    changes = result.pop("_changes")
    record_count = result.pop("_recordCount")
    enrolled: dict[str, Any] = {}

    source_before_bytes = _authoring_commit_source_preflight(repository, changes, source_bytes)
    source_after_bytes = sum(len(value) for value in changes.values() if value is not None)
    # The prepared source journal, preview/result graph, original request and
    # receipt graph remain live while the optional in-memory compiler runs.
    # Its own 320 MiB fixed construction estimate is added separately.
    compiler_caller_live = (
        _AUTHORING_CALLER_FIXED_RESERVE
        + 8 * (request_bytes + impact_input_bytes + len(receipt_before or b""))
        + 8 * (source_before_bytes + source_after_bytes)
        + 4 * len(result.get("diff", ""))
    )
    compiler_phase_peak: list[int] = []
    cache_preflight = _authoring_cache_preflight(
        repository, result["expectedHead"], changes, compiler_caller_live, compiler_phase_peak,
    )

    def enroll(journal: TransactionJournal) -> None:
        compile_report, committed, cache_enrollments = _authoring_cache_enrollments(
            repository, journal, admitted=cache_preflight, record_count=record_count,
            caller_live_bytes=compiler_caller_live,
        )
        response = {
            "protocol": "wedl-command-result/v1", "status": "committed",
            "previousHead": result["expectedHead"], "newHead": committed,
            "generatedIds": result["generatedIds"], "touchedEntityIds": result["touchedEntityIds"],
            "compile": compile_report, "idempotentReplay": False,
        }
        receipt_enrollments: tuple[SurfaceEnrollment, ...] = (
            SurfaceEnrollment(
                ".wedl/idempotency.json", before=receipt_before,
                after=receipt_before, role="receipt",
            ),
        )
        if key:
            next_receipts = dict(receipts)
            def encode_receipt() -> tuple[SurfaceEnrollment, ...]:
                receipt = {"requestHash": request_hash, "fullPayloadHash": full_payload_hash,
                           "confirmationToken": result["confirmationToken"], "result": response}
                if authoring_intent_hash is not None:
                    receipt["authoringIntentHash"] = authoring_intent_hash
                if bounded_authoring_impact is not None:
                    # This value passed a streaming byte bound before parsing,
                    # so receipt construction cannot copy an unbounded input.
                    receipt["authorImpact"] = json.loads(bounded_authoring_impact)
                next_receipts[key] = receipt
                receipt_after = _bounded_canonical_json_bytes(
                    next_receipts,
                    _AUTHORING_MAX_RECEIPT,
                    error="idempotency receipt exceeds authoring byte limit",
                )
                return (
                    SurfaceEnrollment(
                        ".wedl/idempotency.json",
                        before=receipt_before,
                        after=receipt_after,
                        role="receipt",
                    ),
                )

            receipt_enrollments = encode_receipt()
        enrollments = (*cache_enrollments, *receipt_enrollments)
        caller_reserve = _authoring_caller_reserve(
            enrollments, changes=changes, diff=result.get("diff", ""),
            authoring_impact=bounded_authoring_impact,
            request_bytes=request_bytes, source_before_bytes=source_before_bytes,
        )
        budget = JournalLiveByteBudget(
            total_bytes=_AUTHORING_PROCESS_LIMIT,
            caller_reserve_bytes=caller_reserve,
        )
        # SQLite page stores and temporary serialization probes are gone now.
        # Final cache images remain live and register_surfaces prices them in
        # its exact journal peak, so the apply peak is the maximum of these
        # two sequential phases rather than their sum.
        cache_rejected = False
        try:
            if _authoring_phase_peak(
                compiler_phase_peak[0] if cache_preflight and compiler_phase_peak else 0,
                caller_reserve,
            ) > _AUTHORING_PROCESS_LIMIT:
                raise RepositoryError("transaction journal live-byte budget exceeded")
            journal.register_surfaces(enrollments, live_budget=budget)
        except RepositoryError as exc:
            # A failed v7 admission has not mutated the journal. Cache
            # products are disposable, so retry only the unchanged receipt
            # surface or the newly encoded mandatory receipt.
            if not cache_enrollments or str(exc) != "transaction journal live-byte budget exceeded":
                raise
            cache_rejected = True
        if cache_rejected:
            # Leave the exception handler first: its traceback also retains
            # the rejected register_surfaces argument and final cache bytes.
            # Then drop both local tuples before pricing the receipt-only peak.
            cache_enrollments = ()
            enrollments = ()
            compile_report = _deferred_compile_report(
                "authoring-process-memory-limit",
                record_count=record_count,
                report=compile_report,
            )
            response["compile"] = compile_report
            if key:
                receipt_enrollments = encode_receipt()
            enrollments = receipt_enrollments
            caller_reserve = _authoring_caller_reserve(
                enrollments, changes=changes, diff=result.get("diff", ""),
                authoring_impact=bounded_authoring_impact,
                request_bytes=request_bytes, source_before_bytes=source_before_bytes,
            )
            if _authoring_phase_peak(
                compiler_phase_peak[0] if cache_preflight and compiler_phase_peak else 0,
                caller_reserve,
            ) > _AUTHORING_PROCESS_LIMIT:
                raise RepositoryError("transaction journal live-byte budget exceeded")
            journal.register_surfaces(
                enrollments,
                live_budget=JournalLiveByteBudget(
                    total_bytes=_AUTHORING_PROCESS_LIMIT,
                    caller_reserve_bytes=caller_reserve,
                ),
            )
        enrolled["response"] = response

    try:
        repository.commit_files(
            expected_head=result["expectedHead"], files=changes,
            message=str(payload.get("summary") or "wedl: narrative change"),
            trailers={"Wedl-Request": request_hash, "Wedl-Idempotency": hashlib.sha256(key.encode()).hexdigest() if key else ""},
            transaction_enroll=enroll, retain_transaction=True,
            max_before_bytes=_AUTHORING_MAX_SOURCE,
        )
    except Exception as failure:
        # Enrollment failures occur before the ref CAS.  Let the prerequisite
        # reconcile its prepared journal instead of constructing a second
        # changeset rollback transaction.
        try:
            repository.recover_authoring_transactions()
        except Exception as recovery:
            raise RepositoryError(
                f"changeset transaction recovery failed after {type(failure).__name__}: {failure}; {recovery}"
            ) from failure
        raise
    # The prerequisite owns finalization.  Its restart reconciliation is also
    # the normal success path, so no second rollback or direct cache/receipt
    # mutation is needed here.
    repository.recover_authoring_transactions()
    return enrolled["response"]
