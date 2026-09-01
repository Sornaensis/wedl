"""Durable, ownership-checked records for interrupted authoring writes."""

from __future__ import annotations

from dataclasses import dataclass
import base64
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import uuid
from typing import Callable, Sequence

from .errors import RepositoryError
from .util import sha256_bytes

# Version 6 is the immediately preceding, deliberately unbudgeted journal
# format.  A bounded journal has its own version so damage to its budget fields
# cannot make it look like an old unrestricted record.
FORMAT_VERSION = 6
BUDGETED_FORMAT_VERSION = 7
BUDGETED_ENVELOPE = b"WEDL-BUDGETED-7\n"
_PHASES = ("prepared", "ref_committed", "sources_published", "index_published", "surfaces_published", "completed")
_SHA = re.compile(r"^[0-9a-f]{40}$")
_MODE = re.compile(r"^[0-7]{6}$")
_ROLES = frozenset({"source", "cache", "receipt", "private", "staging", "surface"})
_mutation_hook: Callable[[str], None] | None = None
# Test-only, deterministic allocation accounting.  This deliberately reports
# object sizes at the allocation seams rather than attempting to infer RSS.
_live_byte_observer: Callable[[str, dict[str, int]], None] | None = None
# Measured on the active Python ABI.  The multiplier reserves the list/dict/
# tuple shells used by decoded surfaces and canonical JSON construction, rather
# than treating image lengths as the entire live allocation.
_LIVE_FIXED_BYTES = 8 * sum(sys.getsizeof(value) for value in ({}, [], (), "", b"", 0))
_LIVE_PER_IMAGE_BYTES = 4 * sum(sys.getsizeof(value) for value in ({}, [], (), "", b"", 0))
# The bounded paths also construct a small amount of descriptor/path/claim
# metadata before a later journal replacement.  Keep that allocation explicit:
# it is deliberately measured on this ABI instead of assuming that namespace
# work is free merely because it does not add another surface image.
_LIVE_OPERATION_BYTES = sum(sys.getsizeof(value) for value in ({}, [], (), "", b"", 0))


def set_live_byte_observer(observer: Callable[[str, dict[str, int]], None] | None) -> None:
    """Install a test-only observer for journal allocation-producing seams."""
    global _live_byte_observer
    _live_byte_observer = observer


def _observe_live_bytes(phase: str, **terms: int) -> None:
    """Report concrete allocation sizes without making the hook authoritative."""
    if _live_byte_observer is None:
        return
    if any(isinstance(size, bool) or not isinstance(size, int) or size < 0 for size in terms.values()):
        raise RepositoryError("invalid transaction journal live-byte accounting")
    _live_byte_observer(phase, dict(terms))


def _process_dead(pid: int) -> bool | None:
    """Return True only for a provably dead process; ambiguity is contention."""
    if pid <= 0:
        return None
    if os.name == "nt":
        # PROCESS_QUERY_LIMITED_INFORMATION avoids signaling the process and
        # works for ordinary same-user processes. Access denial is deliberately
        # ambiguous, never permission to reclaim a live lock.
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.OpenProcess.argtypes = (ctypes.c_ulong, ctypes.c_bool, ctypes.c_ulong)
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.GetExitCodeProcess.argtypes = (ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong))
        kernel32.GetExitCodeProcess.restype = ctypes.c_bool
        kernel32.CloseHandle.argtypes = (ctypes.c_void_p,)
        kernel32.CloseHandle.restype = ctypes.c_bool
        ctypes.set_last_error(0)
        handle = kernel32.OpenProcess(0x1000, False, pid)
        if not handle:
            error = ctypes.get_last_error()
            return True if error == 87 else None  # ERROR_INVALID_PARAMETER
        try:
            code = ctypes.c_ulong()
            if not kernel32.GetExitCodeProcess(handle, ctypes.byref(code)):
                return None
            return code.value != 259  # STILL_ACTIVE
        finally:
            kernel32.CloseHandle(handle)
    # /proc is not a portable liveness authority (and is routinely absent in
    # constrained POSIX environments).  ``kill(pid, 0)`` does not signal the
    # process and has the precise conservative outcomes we need here.
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return True
    except PermissionError:
        return None
    except OSError:
        return None
    return False


def set_mutation_hook(hook: Callable[[str], None] | None) -> None:
    """Install a test-only hook immediately before each durable mutation.

    The hook is deliberately process-local: a crash injector must not become a
    journal capability.  Keeping it at the primitive boundary lets restart
    tests exercise the real create/rename/unlink/write operations.
    """
    global _mutation_hook
    _mutation_hook = hook


def _before_mutation(name: str) -> None:
    if _mutation_hook is not None:
        _mutation_hook(name)


def mutation_checkpoint(name: str) -> None:
    """Expose the durable failure-injection boundary to Repository primitives."""
    _before_mutation(name)


@dataclass(frozen=True, slots=True)
class Surface:
    """Exact bytes for one transaction-owned source, cache, or receipt file."""
    path: str
    before: bytes | None
    after: bytes | None
    role: str = "surface"
    before_mode: int | None = None
    after_mode: int | None = None
    before_identity: tuple[int, int, int] | None = None
    before_limit: int | None = None

    def as_json(self, *, version: int = BUDGETED_FORMAT_VERSION) -> dict[str, object]:
        """Encode the precise wire shape accepted by the selected journal version."""
        value: dict[str, object] = {
            "path": self.path,
            "before": _encode(self.before),
            "after": _encode(self.after),
            "role": self.role,
            "beforeMode": self.before_mode,
            "afterMode": self.after_mode,
        }
        if version == FORMAT_VERSION:
            return value
        if version != BUDGETED_FORMAT_VERSION:
            raise RepositoryError("unsupported WEDL transaction journal")
        value["beforeIdentity"] = list(self.before_identity) if self.before_identity is not None else None
        value["beforeLimit"] = self.before_limit
        return value

    @classmethod
    def from_json(cls, value: object, *, version: int = BUDGETED_FORMAT_VERSION) -> "Surface":
        fields = {"path", "before", "after", "role", "beforeMode", "afterMode"}
        if version == BUDGETED_FORMAT_VERSION:
            fields |= {"beforeIdentity", "beforeLimit"}
        elif version != FORMAT_VERSION:
            raise RepositoryError("unsupported WEDL transaction journal")
        if (not isinstance(value, dict) or set(value) != fields
                or not isinstance(value["path"], str) or not isinstance(value["role"], str)):
            raise RepositoryError("invalid WEDL transaction journal surface")
        before_mode, after_mode = value["beforeMode"], value["afterMode"]
        if before_mode is not None and (isinstance(before_mode, bool) or not isinstance(before_mode, int) or before_mode < 0 or before_mode > 0o777):
            raise RepositoryError("invalid WEDL transaction journal mode")
        if after_mode is not None and (isinstance(after_mode, bool) or not isinstance(after_mode, int) or after_mode < 0 or after_mode > 0o777):
            raise RepositoryError("invalid WEDL transaction journal mode")
        before_identity = value.get("beforeIdentity")
        if before_identity is not None and (
            not isinstance(before_identity, list)
            or len(before_identity) != 3
            or any(isinstance(item, bool) or not isinstance(item, int) or item < 0 for item in before_identity)
        ):
            raise RepositoryError("invalid WEDL transaction journal identity")
        before_limit = value.get("beforeLimit")
        if before_limit is not None and (isinstance(before_limit, bool) or not isinstance(before_limit, int) or before_limit < 0):
            raise RepositoryError("invalid WEDL transaction journal capture limit")
        return cls(value["path"], _decode(value["before"]), _decode(value["after"]), value["role"], before_mode, after_mode,
                   tuple(before_identity) if before_identity is not None else None, before_limit)


@dataclass(frozen=True, slots=True)
class SurfaceEnrollment:
    path: str
    before: bytes | None = None
    after: bytes | None = None
    capture_before: bool = False
    max_before_bytes: int | None = None
    role: str = "surface"


@dataclass(frozen=True, slots=True)
class JournalLiveByteBudget:
    total_bytes: int
    caller_reserve_bytes: int

    def as_json(self) -> dict[str, int]:
        """Caller-owned fields only; admitted peak is journal evidence."""
        return {"totalBytes": self.total_bytes, "callerReserveBytes": self.caller_reserve_bytes}

    @classmethod
    def from_json(cls, value: object) -> "JournalLiveByteBudget":
        if not isinstance(value, dict) or set(value) != {"totalBytes", "callerReserveBytes"}:
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        total, reserve = value["totalBytes"], value["callerReserveBytes"]
        if (isinstance(total, bool) or not isinstance(total, int) or total < 0
                or isinstance(reserve, bool) or not isinstance(reserve, int) or reserve < 0 or reserve > total):
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        return cls(total, reserve)


def _encode(data: bytes | None) -> dict[str, str] | None:
    if data is None:
        return None
    encoded_bytes = base64.b64encode(data)
    encoded_text = encoded_bytes.decode("ascii")
    value = {"sha256": sha256_bytes(data), "base64": encoded_text}
    _observe_live_bytes(
        "base64-encode",
        source=sys.getsizeof(data),
        base64_bytes=sys.getsizeof(encoded_bytes),
        base64_text=sys.getsizeof(encoded_text),
        image_record=sys.getsizeof(value),
    )
    return value


def _base64_len(size: int) -> int:
    if isinstance(size, bool) or not isinstance(size, int) or size < 0:
        raise RepositoryError("invalid transaction journal live-byte accounting")
    result = 4 * ((size + 2) // 3)
    if result > sys.maxsize:
        raise RepositoryError("transaction journal live-byte budget exceeded")
    return result


def _checked_live_sum(*terms: int) -> int:
    total = 0
    for term in terms:
        if isinstance(term, bool) or not isinstance(term, int) or term < 0 or term > sys.maxsize - total:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        total += term
    return total


def _live_budget_fields(value: object) -> tuple[int, int, int]:
    """Validate the durable aggregate-admission evidence without coercion."""
    if not isinstance(value, dict) or set(value) != {"totalBytes", "callerReserveBytes", "admittedPeakBytes"}:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    values = tuple(value.get(name) for name in ("totalBytes", "callerReserveBytes", "admittedPeakBytes"))
    if any(isinstance(item, bool) or not isinstance(item, int) or item < 0 for item in values):
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    total, reserve, admitted = values
    if reserve > total or admitted > total:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    return total, reserve, admitted


def _projected_live_ledger(image_sizes: Sequence[int], *, caller_reserve: int, journal_bytes: int,
                            path_sizes: Sequence[int] = (), operation_path_bytes: int = 0,
                            operation_payload_bytes: int = 0,
                            surface_count: int | None = None,
                            json_graph_bytes: int = 0) -> dict[str, int]:
    """Return every bounded allocation peak, including restart and namespace work.

    The journal retains raw decoded images and canonical base64 records at the
    same time.  Recovery additionally holds a bounded journal read and JSON
    document while reconstructing those images.  Publish/restore and private
    artifact/lock setup keep a descriptor chunk and path/claim metadata live
    before their next persisted generation.  The caller is admitted against
    the maximum of these phases, never just the enrollment serialization path.
    """
    raw = _checked_live_sum(*image_sizes)
    encoded = _checked_live_sum(*(_base64_len(size) for size in image_sizes))
    path_text = _checked_live_sum(*path_sizes)
    # Publication verifies one surface image at a time; journal loading has its
    # own file-sized early-restart term below.
    bounded_chunk = min(_BOUNDED_CAPTURE_CHUNK, max(image_sizes, default=0))
    if (isinstance(operation_path_bytes, bool) or not isinstance(operation_path_bytes, int)
            or operation_path_bytes < 0):
        raise RepositoryError("invalid transaction journal live-byte accounting")
    if (isinstance(operation_payload_bytes, bool) or not isinstance(operation_payload_bytes, int)
            or operation_payload_bytes < 0):
        raise RepositoryError("invalid transaction journal live-byte accounting")
    if surface_count is None:
        surface_count = len(image_sizes)
    if isinstance(surface_count, bool) or not isinstance(surface_count, int) or surface_count < 0:
        raise RepositoryError("invalid transaction journal live-byte accounting")
    if isinstance(json_graph_bytes, bool) or not isinstance(json_graph_bytes, int) or json_graph_bytes < 0:
        raise RepositoryError("invalid transaction journal live-byte accounting")
    # A zero-image enrollment still creates a Surface, path/seen entry, JSON
    # object and proposed-list slot.  Price every enrollment, not merely every
    # payload image, so a large empty batch is rejected before those containers
    # are constructed.
    containers = _checked_live_sum(_LIVE_FIXED_BYTES, _LIVE_PER_IMAGE_BYTES * surface_count)
    # CAS keeps the old generation live while the replacement payload, its
    # descriptor verification chunks and the joined comparison image exist.
    # Do not price this as merely three journal generations: the old cached
    # bytes, candidate bytes, write payload and post-replace comparison can
    # overlap in CPython even when they have equal content.
    # CAS retains its authenticated expected generation while descriptor reads
    # retain chunk-list bytes and a joined comparison image.  Keep these copies
    # explicit rather than relying on a vague generation multiplier.
    cas_compare = _checked_live_sum(2 * journal_bytes, min(_BOUNDED_CAPTURE_CHUNK, journal_bytes))
    public_image = max(image_sizes, default=0)
    public_compare = _checked_live_sum(2 * public_image, min(_BOUNDED_CAPTURE_CHUNK, public_image))
    operation_compare = _checked_live_sum(
        2 * operation_payload_bytes, min(_BOUNDED_CAPTURE_CHUNK, operation_payload_bytes), operation_path_bytes,
    )
    enrollment = _checked_live_sum(
        caller_reserve,
        2 * raw,
        4 * encoded,
        5 * journal_bytes,
        3 * path_text,
        containers,
        cas_compare,
    )
    # A bounded restart holds the descriptor chunks, joined payload, decoded
    # UTF-8 document and parsed container while validating the cached image
    # values.  A fourth journal generation-sized term covers that distinct
    # parsed representation without relying on Python object identity.
    restart = _checked_live_sum(
        caller_reserve, raw, 3 * encoded, 5 * journal_bytes, 3 * path_text, containers,
        cas_compare,
    )
    # Surface decode, publish and restore can retain the decoded surface tuple
    # while hashing one bounded descriptor chunk/claim.  Private/lock setup
    # similarly creates a path/claim record ahead of its durable generation.
    decode = _checked_live_sum(caller_reserve, 2 * raw, 4 * encoded, 5 * journal_bytes, 3 * path_text, containers, cas_compare)
    publication = _checked_live_sum(decode, bounded_chunk, public_compare, operation_compare, _LIVE_OPERATION_BYTES)
    private_artifact = _checked_live_sum(enrollment, operation_compare, _LIVE_OPERATION_BYTES)
    # Before JSON parsing, restart holds only descriptor chunks, the joined
    # payload and one UTF-8 document.  The later restart-load term prices the
    # parsed/decode graph; this early term proves the pre-parse allocation is
    # bounded without double-counting every eventual representation.
    early_restart = _checked_live_sum(
        caller_reserve, journal_bytes, journal_bytes, journal_bytes, journal_bytes,
    )
    # The authenticated envelope can price JSON's variable parsed graph before
    # parsing.  Charge that incremental graph independently of the retained
    # canonical generations already represented by ``journal_bytes``.
    json_parse = _checked_live_sum(caller_reserve, 5 * journal_bytes, json_graph_bytes)
    ledger = {
        "enrollment": enrollment,
        "early-restart": early_restart,
        "json-preparse": json_parse,
        "restart-load": restart,
        "surface-decode": decode,
        "publish": publication,
        "restore": publication,
        "private-artifact": private_artifact,
        "lock": private_artifact,
        "phase": private_artifact,
        "cleanup": publication,
    }
    observed_ledger = {f"{name}_peak": value for name, value in ledger.items()}
    _observe_live_bytes(
        "projected-ledger",
        **observed_ledger,
        caller_reserve=caller_reserve,
        raw_images=2 * raw,
        base64_images=4 * encoded,
        journal_generations=5 * journal_bytes,
        cas_expected_and_compare=cas_compare,
        public_expected_and_compare=public_compare,
        operation_expected_and_compare=operation_compare,
        path_text=3 * path_text,
        fixed_containers=containers,
        json_graph=json_graph_bytes,
    )
    return ledger


def _projected_live_sizes(image_sizes: Sequence[int], *, caller_reserve: int, journal_bytes: int,
                           path_sizes: Sequence[int] = (), operation_path_bytes: int = 0,
                           surface_count: int | None = None, json_graph_bytes: int = 0) -> int:
    """Conservative maximum of enrollment, restart, and mutation peaks."""
    ledger = _projected_live_ledger(
        image_sizes, caller_reserve=caller_reserve, journal_bytes=journal_bytes,
        path_sizes=path_sizes, operation_path_bytes=operation_path_bytes, surface_count=surface_count,
        json_graph_bytes=json_graph_bytes,
    )
    projected = max(ledger.values())
    _observe_live_bytes(
        "projected-admission",
        caller_reserve=caller_reserve,
        projected_peak=projected,
    )
    return projected


def _projected_live_bytes(surfaces: Sequence[Surface], *, caller_reserve: int, journal_bytes: int) -> int:
    image_sizes = [len(image) for surface in surfaces for image in (surface.before, surface.after) if image is not None]
    return _projected_live_sizes(
        image_sizes, caller_reserve=caller_reserve, journal_bytes=journal_bytes,
        path_sizes=[sys.getsizeof(surface.path) for surface in surfaces], surface_count=len(surfaces),
    )


def _encoded_surface_image_sizes(values: object) -> tuple[list[int], list[int]]:
    """Derive decoded image and path bounds without decoding old base64 text."""
    if not isinstance(values, list):
        raise RepositoryError("invalid WEDL transaction journal")
    images: list[int] = []
    paths: list[int] = []
    for value in values:
        if not isinstance(value, dict) or not isinstance(value.get("path"), str):
            raise RepositoryError("invalid WEDL transaction journal surface")
        paths.append(sys.getsizeof(value["path"]))
        for name in ("before", "after"):
            image = value.get(name)
            if image is None:
                continue
            if not isinstance(image, dict) or not isinstance(image.get("base64"), str) or not isinstance(image.get("sha256"), str):
                raise RepositoryError("invalid WEDL transaction journal image")
            encoded = image["base64"]
            # Do not slice the retained base64 text here: ``encoded[:-2]``
            # would clone an image-sized string before batch admission can
            # reject it.  A single index lookup preserves the same shape rule
            # without materializing any part of the retained wire payload.
            first_padding = encoded.find("=")
            if len(encoded) % 4 or (first_padding >= 0 and first_padding < len(encoded) - 2):
                raise RepositoryError("invalid WEDL transaction journal image")
            padding = 2 if encoded.endswith("==") else 1 if encoded.endswith("=") else 0
            images.append((len(encoded) // 4) * 3 - padding)
    return images, paths


def _preparse_budgeted_record_admission(record: object, *, journal_bytes: int,
                                        authoritative_budget: tuple[int, int] | None = None) -> None:
    """Reject a forged budgeted container before Surface/base64 decoding.

    ``json.loads`` necessarily creates the outer document, but it must not be
    allowed to create an unpriced many-surface graph and then discover the
    admission failure while decoding images.  This deliberately shallow pass
    only reads the budget envelope and encoded lengths; full schema/identity
    validation remains in ``TransactionJournal._validate``.
    """
    if not isinstance(record, dict):
        raise RepositoryError("invalid WEDL transaction journal")
    if record.get("version") != BUDGETED_FORMAT_VERSION:
        return
    budget = record.get("liveByteBudget")
    if record.get("$liveByteBudget") != budget:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    total, reserve, _admitted = _live_budget_fields(budget)
    if authoritative_budget is not None:
        total, reserve = authoritative_budget
    surfaces = record.get("surfaces")
    images, paths = _encoded_surface_image_sizes(surfaces)
    projected = _projected_live_sizes(
        images, caller_reserve=reserve, journal_bytes=journal_bytes,
        path_sizes=paths, surface_count=len(surfaces),
    )
    if projected > total:
        raise RepositoryError("transaction journal live-byte budget exceeded")


def _budget_header_from_prefix(prefix: bytes) -> tuple[int, int, int, int, int, bytes] | None:
    """Read the authenticated structural envelope before a JSON allocation.

    The outer envelope is deliberately not a trust boundary by itself: its
    digest binds its byte and surface-count claims to the canonical document.
    It does let the loader reject a deliberately huge/many-surface record
    before UTF-8 decoding or ``json.loads`` constructs the document graph.
    """
    match = re.match(
        rb'^total=(\d+) reserve=(\d+) admitted=(\d+) surfaces=(\d+) bytes=(\d+) sha256=([0-9a-f]{64})\n',
        prefix,
    )
    if match is None:
        return None
    try:
        total, reserve, admitted = _live_budget_fields({
            "admittedPeakBytes": int(match.group(3)),
            "callerReserveBytes": int(match.group(2)),
            "totalBytes": int(match.group(1)),
        })
        count, document_bytes = int(match.group(4)), int(match.group(5))
        if count < 0 or document_bytes < 0:
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        return total, reserve, admitted, count, document_bytes, match.group(6)
    except (ValueError, RepositoryError) as exc:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget") from exc


def _budgeted_document(payload: bytes, *, scanned_graph_bytes: int | None = None,
                       authoritative_budget: tuple[int, int] | None = None) -> bytes:
    """Return a structurally authenticated budgeted JSON document."""
    if not payload.startswith(BUDGETED_ENVELOPE):
        return payload
    header = _budget_header_from_prefix(payload[len(BUDGETED_ENVELOPE):])
    if header is None:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    total, reserve, _admitted, count, document_bytes, digest = header
    if authoritative_budget is not None:
        total, reserve = authoritative_budget
    offset = len(BUDGETED_ENVELOPE) + _budget_header_length(header)
    document = payload[offset:]
    if len(document) != document_bytes or sha256_bytes(document).encode("ascii") != digest:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    # Establish the parser/container ceiling before duplicate-key validation.
    # The latter deliberately retains decoded keys per nesting level, whereas
    # this lexical graph scan retains no document-derived state at all.
    parsed_graph = (_budgeted_json_graph_upper_bound(document)
                    if scanned_graph_bytes is None else scanned_graph_bytes)
    parse_peak = _checked_live_sum(reserve, 5 * len(payload), parsed_graph)
    _observe_live_bytes("budgeted-json-preparse", document_bytes=len(document), parsed_graph=parsed_graph,
                        projected_peak=parse_peak)
    if parse_peak > total:
        raise RepositoryError("transaction journal live-byte budget exceeded")
    _reject_noncanonical_budgeted_json(document)
    # The header's surface claim must be checked before UTF-8/JSON parsing, but
    # it cannot be inferred from a lexical ``"path":`` count: valid index
    # entries are themselves allowed to be named ``path``.  Instead inspect
    # only the canonical root ``surfaces`` array and count its direct elements.
    # The digest has already authenticated the document, so this structural
    # claim cannot be redirected by an unrelated nested key or string value.
    if _canonical_surface_count(document) != count:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    # The versioned envelope is non-downgradable: require both duplicate budget
    # fields as raw authenticated document structure before UTF-8/JSON parsing.
    if document.count(b'"liveByteBudget":') != 1 or document.count(b'"$liveByteBudget":') != 1:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    return document


def _reject_noncanonical_budgeted_json(document: bytes) -> None:
    """Reject non-canonical JSON and duplicate object keys without parsing it.

    Budgeted generations are emitted by ``_canonical_journal_payload``.  Their
    digest therefore authenticates the *canonical* byte representation, rather
    than merely any JSON document with similar meaning.  Check that invariant
    before UTF-8 decoding: otherwise a re-signed whitespace variant or a
    duplicate-key document could make ``json.loads`` allocate/rewrite an
    ambiguous record before the budgeted recovery guards get a vote.
    """
    if not document.endswith(b"\n") or not document[:-1]:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    # Each object keeps semantic (decoded) keys, not their source spelling.
    # That rejects ``"a"``/``"\\u0061"`` aliases and independently enforces
    # json.dumps(sort_keys=True)'s ordering for every nested object.  This is
    # intentionally after the non-allocating graph admission above.
    stacks: list[tuple[set[str], str | None] | None] = []
    index = 0
    limit = len(document) - 1
    while index < limit:
        byte = document[index]
        if byte in b" \t\r\n":
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        if byte == ord('{'):
            stacks.append((set(), None))
            index += 1
            continue
        if byte == ord('['):
            stacks.append(None)
            index += 1
            continue
        if byte in (ord('}'), ord(']')):
            if not stacks:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            stacks.pop()
            index += 1
            continue
        if byte != ord('"'):
            index += 1
            continue
        start = index
        index += 1
        escaped = False
        while index < limit:
            current = document[index]
            if escaped:
                escaped = False
            elif current == ord('\\'):
                escaped = True
            elif current == ord('"'):
                break
            index += 1
        if index >= limit:
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        # In canonical JSON a member name is immediately followed by a colon.
        # Require the exact json.dumps spelling as well as semantic uniqueness
        # and ascending key order.  ``json.loads`` is restricted to this one
        # bounded string token; the journal document itself is still unparsed.
        if index + 1 < limit and document[index + 1] == ord(':'):
            if not stacks or stacks[-1] is None:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            raw_key = document[start:index + 1]
            try:
                key_text = raw_key.decode("utf-8")
                key, end = json.decoder.scanstring(key_text, 1, True)
                if end != len(key_text):
                    raise ValueError("trailing JSON key data")
            except (UnicodeDecodeError, ValueError) as exc:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget") from exc
            if not isinstance(key, str) or json.dumps(key, ensure_ascii=True, separators=(",", ":")).encode("utf-8") != raw_key:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            keys, previous = stacks[-1]
            if key in keys or (previous is not None and key <= previous):
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            keys.add(key)
            stacks[-1] = (keys, key)
        index += 1
    if stacks:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")


def _validate_budgeted_record_header(record: object, payload: bytes) -> None:
    """Bind parsed budget/surface fields back to the authenticated envelope."""
    if not payload.startswith(BUDGETED_ENVELOPE):
        return
    header = _budget_header_from_prefix(payload[len(BUDGETED_ENVELOPE):])
    if header is None or not isinstance(record, dict):
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    total, reserve, admitted, count, _document_bytes, _digest = header
    if record.get("version") != BUDGETED_FORMAT_VERSION:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    budget = record.get("liveByteBudget")
    if record.get("$liveByteBudget") != budget:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    parsed_total, parsed_reserve, parsed_admitted = _live_budget_fields(budget)
    surfaces = record.get("surfaces")
    if (parsed_total, parsed_reserve, parsed_admitted) != (total, reserve, admitted) or not isinstance(surfaces, list) or len(surfaces) != count:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")


def _canonical_surface_count(document: bytes) -> int:
    """Count direct entries of the root canonical ``surfaces`` array.

    This is a deliberately tiny JSON structural scan used solely before the
    bounded parser admission. It does not decode strings or build containers;
    escaped quotes are skipped and only a depth-one property named ``surfaces``
    is eligible. Full JSON/schema validation remains after the bounded load.
    """
    depth = 0
    in_string = False
    escaped = False
    array_start: int | None = None
    index = 0
    while index < len(document):
        byte = document[index]
        if in_string:
            if escaped:
                escaped = False
            elif byte == ord("\\"):
                escaped = True
            elif byte == ord('"'):
                in_string = False
            index += 1
            continue
        if byte == ord('"'):
            if depth == 1 and document.startswith(b'"surfaces":[', index):
                if array_start is not None:
                    raise RepositoryError("invalid WEDL transaction journal live-byte budget")
                array_start = index + len(b'"surfaces":')
                index = array_start
                continue
            in_string = True
        elif byte in (ord('{'), ord('[')):
            depth += 1
        elif byte in (ord('}'), ord(']')):
            depth -= 1
            if depth < 0:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        index += 1
    if array_start is None:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")

    depth = 0
    in_string = False
    escaped = False
    count = 0
    has_value = False
    # Consume the array opener as the scanner boundary. Nested objects/arrays
    # then increase ``depth`` from zero, and this array's own closing bracket
    # is the unambiguous termination point.
    index = array_start + 1
    while index < len(document):
        byte = document[index]
        if in_string:
            if escaped:
                escaped = False
            elif byte == ord("\\"):
                escaped = True
            elif byte == ord('"'):
                in_string = False
            index += 1
            continue
        if byte == ord('"'):
            in_string = True
        elif byte in (ord('{'), ord('[')):
            depth += 1
            has_value = True
        elif byte == ord(']'):
            if depth == 0:
                return count + int(has_value)
            depth -= 1
        elif byte == ord('}'):
            if depth == 0:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            depth -= 1
        elif byte == ord(',') and depth == 0:
            if not has_value:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            count += 1
            has_value = False
        elif byte not in b' \t\r\n':
            has_value = True
        index += 1
    raise RepositoryError("invalid WEDL transaction journal live-byte budget")


def _budgeted_json_graph_upper_bound(document: bytes) -> int:
    """Bound a parsed JSON graph from authenticated bytes without parsing it.

    Quotes, opening containers and separators are cheap lexical upper bounds on
    independently allocated Python strings, mappings/lists and their slots.
    Every source byte is also charged at four Unicode bytes.  The scanner does
    not interpret JSON; it merely distinguishes escaped quote bytes so the
    accounting remains conservative for hostile strings before ``json.loads``.
    """
    quote_count = opening_count = separator_count = 0
    in_string = escaped = False
    for byte in document:
        if in_string:
            if escaped:
                escaped = False
            elif byte == 0x5C:
                escaped = True
            elif byte == 0x22:
                in_string = False
                quote_count += 1
            continue
        if byte == 0x22:
            in_string = True
            quote_count += 1
        elif byte in (0x5B, 0x7B):  # [ {
            opening_count += 1
        elif byte in (0x2C, 0x3A):  # , :
            separator_count += 1
    container_overhead = max(sys.getsizeof({}), sys.getsizeof([]))
    string_overhead = sys.getsizeof("")
    # A comma/colon can add at most one list slot or mapping entry.  This is
    # intentionally larger than a pointer while avoiding charging a full dict
    # object for every separator in ordinary canonical records.
    slot_overhead = 32
    return _checked_live_sum(
        4 * len(document),
        container_overhead * opening_count,
        string_overhead * ((quote_count + 1) // 2),
        slot_overhead * separator_count,
    )


def _budgeted_json_graph_upper_bound_from_descriptor(descriptor: int, *, offset: int,
                                                      document_bytes: int) -> int:
    """Price a budgeted document before retaining its bounded read chunks.

    This first pass intentionally carries only lexical counters and the two
    string-state booleans across chunks.  It is therefore safe to use before
    the duplicate-key sets and the later joined payload exist at once.
    """
    if any(isinstance(value, bool) or not isinstance(value, int) or value < 0
           for value in (descriptor, offset, document_bytes)):
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    try:
        os.lseek(descriptor, offset, os.SEEK_SET)
    except OSError as exc:
        raise RepositoryError("invalid WEDL transaction journal") from exc
    quote_count = opening_count = separator_count = 0
    in_string = escaped = False
    remaining = document_bytes
    try:
        while remaining:
            chunk = os.read(descriptor, min(_BOUNDED_CAPTURE_CHUNK, remaining))
            if not chunk:
                raise RepositoryError("invalid WEDL transaction journal")
            remaining -= len(chunk)
            for byte in chunk:
                if in_string:
                    if escaped:
                        escaped = False
                    elif byte == 0x5C:
                        escaped = True
                    elif byte == 0x22:
                        in_string = False
                        quote_count += 1
                    continue
                if byte == 0x22:
                    in_string = True
                    quote_count += 1
                elif byte in (0x5B, 0x7B):
                    opening_count += 1
                elif byte in (0x2C, 0x3A):
                    separator_count += 1
    except OSError as exc:
        raise RepositoryError("invalid WEDL transaction journal") from exc
    if in_string or escaped:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    container_overhead = max(sys.getsizeof({}), sys.getsizeof([]))
    string_overhead = sys.getsizeof("")
    return _checked_live_sum(
        4 * document_bytes,
        container_overhead * opening_count,
        string_overhead * ((quote_count + 1) // 2),
        32 * separator_count,
    )


def _canonical_json_string_bytes(text: str) -> int:
    """Return the exact UTF-8 byte count of json.dumps(..., ensure_ascii=True).

    Non-BMP code points become a surrogate-pair escape (twelve bytes), while
    controls use either their two-byte short escape or a six-byte ``\\u00xx``
    spelling.  Keeping this checked avoids a path-only cap bypass at the
    candidate-envelope boundary.
    """
    if not isinstance(text, str):
        raise RepositoryError("transaction journal live-byte budget exceeded")
    total = 2  # opening and closing quote
    for character in text:
        ordinal = ord(character)
        if character in ('"', '\\', '\b', '\f', '\n', '\r', '\t'):
            width = 2
        elif ordinal < 0x20:
            width = 6
        elif ordinal <= 0x7F:
            width = 1
        elif ordinal <= 0xFFFF:
            width = 6
        else:
            width = 12
        total = _checked_live_sum(total, width)
    return total


def _budgeted_payload_json_graph(payload: bytes) -> int:
    """Return candidate parsed-JSON overhead without parsing its document."""
    if not payload.startswith(BUDGETED_ENVELOPE):
        return 0
    header = _budget_header_from_prefix(payload[len(BUDGETED_ENVELOPE):])
    if header is None:
        raise RepositoryError("invalid WEDL transaction journal live-byte budget")
    offset = len(BUDGETED_ENVELOPE) + _budget_header_length(header)
    return _budgeted_json_graph_upper_bound(payload[offset:])


def _budgeted_candidate_payload_upper_bound(*, current_bytes: int, version: int,
                                            existing_surfaces: int,
                                            additions: Sequence[tuple[str, int | None, int | None]]) -> int:
    """Price a v6-to-v7 candidate before creating a Surface or JSON record.

    Existing v6 surface dictionaries must be decoded and rewritten to add the
    v7 capture fields.  That conversion is itself allocation-producing, so a
    failed admission cannot wait for ``Surface.from_json`` or ``json.dumps``.
    The bounds are byte-level and deliberately include worst-case JSON escaping
    for each already-validated new path, two image records and the duplicate
    budget/envelope metadata.
    """
    if version not in (FORMAT_VERSION, BUDGETED_FORMAT_VERSION):
        raise RepositoryError("invalid WEDL transaction journal")
    if any(isinstance(value, bool) or not isinstance(value, int) or value < 0
           for value in (current_bytes, existing_surfaces)):
        raise RepositoryError("transaction journal live-byte budget exceeded")
    # Promotion rewrites every retained surface into v7's eight-key shape.  In
    # addition to capture identity/limit fields, price object/list slots,
    # escaped field names, parser graph and the duplicated budget envelope.
    # This is deliberately a byte-only bound: no Surface/base64/JSON object is
    # materialized until it has passed admission.
    promoted = 96 * existing_surfaces if version == FORMAT_VERSION else 0
    addition_bytes = 0
    for path, before_size, after_size in additions:
        if not isinstance(path, str):
            raise RepositoryError("transaction journal live-byte budget exceeded")
        images = 0
        for size in (before_size, after_size):
            if size is not None:
                if isinstance(size, bool) or not isinstance(size, int) or size < 0:
                    raise RepositoryError("transaction journal live-byte budget exceeded")
                # Retained base64 text, transient encoding, image record,
                # parser string/object/slot graph and canonical punctuation.
                images = _checked_live_sum(images, _base64_len(size), 64)
        addition_bytes = _checked_live_sum(
            addition_bytes, 96, _canonical_json_string_bytes(path), images)
    # Two budget objects, the authenticated header, generation digits and the
    # root/container punctuation are independent of image bytes.
    return _checked_live_sum(current_bytes, promoted, addition_bytes, 256)


def _json_string_wire_upper_bound(value: str) -> int:
    """Bound one ensure_ascii JSON string without constructing its escape text."""
    total = 2
    for character in value:
        codepoint = ord(character)
        width = (2 if character in {'"', '\\'} else 6 if codepoint < 0x20 else
                 1 if codepoint < 0x80 else 6 if codepoint <= 0xffff else 12)
        total = _checked_live_sum(total, width)
    return total


def _json_int_wire_bytes(value: int) -> int:
    if value == 0:
        return 1
    total = 1 if value < 0 else 0
    value = abs(value)
    while value:
        total += 1
        value //= 10
    return total


def _json_wire_upper_bound(value: object) -> int:
    """Return a no-serialization upper bound for canonical JSON wire bytes."""
    if value is None:
        return 4
    if isinstance(value, bool):
        return 4 if value else 5
    if isinstance(value, int):
        return _json_int_wire_bytes(value)
    if isinstance(value, str):
        return _json_string_wire_upper_bound(value)
    if isinstance(value, list):
        total = 2
        for ordinal, item in enumerate(value):
            total = _checked_live_sum(total, _json_wire_upper_bound(item))
            if ordinal:
                total = _checked_live_sum(total, 1)
        return total
    if isinstance(value, dict):
        total = 2
        for ordinal, (key, item) in enumerate(value.items()):
            if not isinstance(key, str):
                raise RepositoryError("transaction journal live-byte budget exceeded")
            total = _checked_live_sum(total, _json_wire_upper_bound(key), 1,
                                      _json_wire_upper_bound(item))
            if ordinal:
                total = _checked_live_sum(total, 1)
        return total
    raise RepositoryError("transaction journal live-byte budget exceeded")


def _budgeted_successor_upper_bounds(record: dict[str, object]) -> tuple[int, int]:
    """Bound envelope payload and parsed graph before json/base64 seams run."""
    document = _json_wire_upper_bound(record)
    total, reserve, admitted = _live_budget_fields(record.get("liveByteBudget"))
    surfaces = record.get("surfaces")
    if not isinstance(surfaces, list):
        raise RepositoryError("transaction journal live-byte budget exceeded")
    # A fixed-point persist can raise admittedPeakBytes up to total in both
    # mirrored budget maps.  Reserve those decimal digits and the canonical
    # trailing newline before serialization.
    document = _checked_live_sum(document, 2 * max(0, _json_int_wire_bytes(total) - _json_int_wire_bytes(admitted)))
    header = _budget_header_length((total, reserve, total, len(surfaces), document + 1, b"0" * 64))
    payload = _checked_live_sum(len(BUDGETED_ENVELOPE), document, header, 1)
    def graph_counts(value: object) -> tuple[int, int, int]:
        """Canonical JSON's exact lexical container/string/separator counts."""
        if value is None or isinstance(value, (bool, int)):
            return 0, 0, 0
        if isinstance(value, str):
            return 0, 1, 0
        if isinstance(value, list):
            openings, strings, separators = 1, 0, max(0, len(value) - 1)
            for item in value:
                child_openings, child_strings, child_separators = graph_counts(item)
                openings += child_openings; strings += child_strings; separators += child_separators
            return openings, strings, separators
        if isinstance(value, dict):
            openings, strings, separators = 1, 0, (2 * len(value) - 1 if value else 0)
            for key, item in value.items():
                if not isinstance(key, str):
                    raise RepositoryError("transaction journal live-byte budget exceeded")
                child_openings, child_strings, child_separators = graph_counts(item)
                openings += child_openings; strings += 1 + child_strings; separators += child_separators
            return openings, strings, separators
        raise RepositoryError("transaction journal live-byte budget exceeded")
    parser_document = _checked_live_sum(document, 1)
    if parser_document > sys.maxsize // 4:
        raise RepositoryError("transaction journal live-byte budget exceeded")
    openings, strings, separators = graph_counts(record)
    return payload, _checked_live_sum(
        4 * parser_document, max(sys.getsizeof({}), sys.getsizeof([])) * openings,
        sys.getsizeof("") * strings, 32 * separators)


def _budgeted_enrollment_successor_upper_bounds(
        record: dict[str, object], *,
        additions: Sequence[tuple[str, SurfaceEnrollment, _BoundedBeforeMetadata | None]],
        total: int, reserve: int) -> tuple[int, int]:
    """Bound a v6 or v7 enrollment successor without materializing images.

    This scalar/counting counterpart to ``_budgeted_successor_upper_bounds`` is
    deliberately shared by first and repeated bounded batches.  Constructing a
    v7 candidate just to ask its size would allocate the new base64/JSON graph;
    decoding retained v7 records to rebuild them would additionally allocate
    images that the candidate already owns on wire.  Count the exact v7 field
    shapes instead.  A valid v7 retained wire record is retained verbatim, while
    a v6 record is only scalar-priced here and is promoted after admission.
    """
    version = record.get("version")
    if version not in {FORMAT_VERSION, BUDGETED_FORMAT_VERSION}:
        raise RepositoryError("invalid WEDL transaction journal")
    if any(isinstance(value, bool) or not isinstance(value, int) or value < 0
           for value in (total, reserve)) or reserve > total:
        raise RepositoryError("transaction journal live-byte budget exceeded")
    raw_surfaces = record.get("surfaces")
    generation = record.get("generation")
    if (not isinstance(raw_surfaces, list) or isinstance(generation, bool)
            or not isinstance(generation, int) or generation < 0):
        raise RepositoryError("invalid WEDL transaction journal")

    def merge(bounds: tuple[int, int, int], child: tuple[int, int, int]) -> tuple[int, int, int]:
        return (_checked_live_sum(bounds[0], child[0]),
                _checked_live_sum(bounds[1], child[1]),
                _checked_live_sum(bounds[2], child[2]))

    def scalar(value: object) -> tuple[int, int, int]:
        """Canonical wire bytes plus parsed-graph lexical counters."""
        wire = _json_wire_upper_bound(value)

        def counts(item: object) -> tuple[int, int, int]:
            if item is None or isinstance(item, (bool, int)):
                return 0, 0, 0
            if isinstance(item, str):
                return 0, 1, 0
            if isinstance(item, list):
                result = (1, 0, max(0, len(item) - 1))
                for nested in item:
                    result = merge(result, counts(nested))
                return result
            if isinstance(item, dict):
                result = (1, 0, 2 * len(item) - 1 if item else 0)
                for key, nested in item.items():
                    if not isinstance(key, str):
                        raise RepositoryError("transaction journal live-byte budget exceeded")
                    result = merge(result, (0, 1, 0))
                    result = merge(result, counts(nested))
                return result
            raise RepositoryError("transaction journal live-byte budget exceeded")

        openings, strings, separators = counts(value)
        return wire, openings, strings, separators

    def image(size: int | None) -> tuple[int, int, int]:
        if size is None:
            return scalar(None)
        if isinstance(size, bool) or not isinstance(size, int) or size < 0:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        # Base64 and SHA256 are ASCII-only, so their canonical JSON string wire
        # widths are exact without constructing either retained string.
        encoded = _base64_len(size)
        wire = _checked_live_sum(
            2, _json_wire_upper_bound("base64"), 1, encoded + 2, 1,
            _json_wire_upper_bound("sha256"), 1, 64 + 2,
        )
        return wire, 1, 4, 3

    def identity(value: tuple[int, int, int] | None) -> tuple[int, int, int]:
        if value is None:
            return scalar(None)
        if len(value) != 3 or any(isinstance(item, bool) or not isinstance(item, int) or item < 0 for item in value):
            raise RepositoryError("transaction journal live-byte budget exceeded")
        wire = _checked_live_sum(2, _json_int_wire_bytes(value[0]), 1,
                                 _json_int_wire_bytes(value[1]), 1,
                                 _json_int_wire_bytes(value[2]))
        return wire, 1, 0, 2

    def surface_fields(values: Sequence[tuple[str, tuple[int, int, int]]]) -> tuple[int, int, int]:
        if len(values) != 8:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        wire, openings, strings, separators = 2, 1, 0, 2 * len(values) - 1
        for key, value in values:
            if not isinstance(key, str):
                raise RepositoryError("transaction journal live-byte budget exceeded")
            wire = _checked_live_sum(wire, _json_wire_upper_bound(key), 1, value[0])
            openings, strings, separators = merge((openings, strings + 1, separators), (value[1], value[2], value[3]))
        return _checked_live_sum(wire, len(values) - 1), openings, strings, separators

    def retained_surface(value: object) -> tuple[int, int, int]:
        fields = ({"path", "before", "after", "role", "beforeMode", "afterMode"}
                  if version == FORMAT_VERSION else
                  {"path", "before", "after", "role", "beforeMode", "afterMode", "beforeIdentity", "beforeLimit"})
        if not isinstance(value, dict) or set(value) != fields:
            raise RepositoryError("invalid WEDL transaction journal surface")
        values: tuple[tuple[str, tuple[int, int, int]], ...] = (
            ("path", scalar(value["path"])), ("before", scalar(value["before"])),
            ("after", scalar(value["after"])), ("role", scalar(value["role"])),
            ("beforeMode", scalar(value["beforeMode"])), ("afterMode", scalar(value["afterMode"])),
        )
        if version == FORMAT_VERSION:
            values += (("beforeIdentity", scalar(None)), ("beforeLimit", scalar(None)))
        else:
            values += (("beforeIdentity", scalar(value["beforeIdentity"])),
                       ("beforeLimit", scalar(value["beforeLimit"])))
        return surface_fields(values)

    def addition_surface(relative: str, enrollment: SurfaceEnrollment,
                         metadata: _BoundedBeforeMetadata | None) -> tuple[int, int, int]:
        before_size = (metadata.size if metadata is not None else None) if enrollment.capture_before else (
            len(enrollment.before) if enrollment.before is not None else None)
        before_mode = metadata.mode if enrollment.capture_before and metadata is not None else None
        before_identity = metadata.identity if enrollment.capture_before and metadata is not None else None
        before_limit = enrollment.max_before_bytes if enrollment.capture_before else None
        return surface_fields((
            ("path", scalar(relative)), ("before", image(before_size)),
            ("after", image(len(enrollment.after) if enrollment.after is not None else None)),
            ("role", scalar(enrollment.role)), ("beforeMode", scalar(before_mode)),
            ("afterMode", scalar(None)), ("beforeIdentity", identity(before_identity)),
            ("beforeLimit", scalar(before_limit)),
        ))

    def budget() -> tuple[int, int, int]:
        # Unlike surface_fields, budget has exactly three fields.
        values = (("totalBytes", scalar(total)), ("callerReserveBytes", scalar(reserve)),
                  ("admittedPeakBytes", scalar(total)))
        wire, openings, strings, separators = 2, 1, 0, 2 * len(values) - 1
        for key, value in values:
            wire = _checked_live_sum(wire, _json_wire_upper_bound(key), 1, value[0])
            openings, strings, separators = merge((openings, strings + 1, separators), (value[1], value[2], value[3]))
        return _checked_live_sum(wire, len(values) - 1), openings, strings, separators

    # Do not materialize a prospective surface list just to price it: all
    # counts below are scalar and the existing preflight tuple is reused.
    surface_count = _checked_live_sum(len(raw_surfaces), len(additions))
    surface_wire, surface_openings, surface_strings, surface_separators = 2, 1, 0, max(0, surface_count - 1)
    for raw_surface in raw_surfaces:
        value = retained_surface(raw_surface)
        surface_wire = _checked_live_sum(surface_wire, value[0])
        surface_openings, surface_strings, surface_separators = merge(
            (surface_openings, surface_strings, surface_separators), value[1:])
    for addition in additions:
        value = addition_surface(*addition)
        surface_wire = _checked_live_sum(surface_wire, value[0])
        surface_openings, surface_strings, surface_separators = merge(
            (surface_openings, surface_strings, surface_separators), value[1:])
    surface_wire = _checked_live_sum(surface_wire, max(0, surface_count - 1))

    root_wire, root_openings, root_strings, root_separators = 2, 1, 0, 0
    root_count = 0
    for key, value in record.items():
        if key in {"version", "generation", "surfaces", "liveByteBudget", "$liveByteBudget"}:
            continue
        if not isinstance(key, str):
            raise RepositoryError("transaction journal live-byte budget exceeded")
        child = scalar(value)
        root_wire = _checked_live_sum(root_wire, _json_wire_upper_bound(key), 1, child[0])
        root_openings, root_strings, root_separators = merge(
            (root_openings, root_strings + 1, root_separators), child[1:])
        root_count += 1
    if generation >= sys.maxsize:
        raise RepositoryError("transaction journal live-byte budget exceeded")
    for key, child in (("version", scalar(BUDGETED_FORMAT_VERSION)),
                       ("generation", scalar(generation + 1)),
                       ("surfaces", (surface_wire, surface_openings, surface_strings, surface_separators)),
                       ("liveByteBudget", budget()), ("$liveByteBudget", budget())):
        root_wire = _checked_live_sum(root_wire, _json_wire_upper_bound(key), 1, child[0])
        root_openings, root_strings, root_separators = merge(
            (root_openings, root_strings + 1, root_separators), child[1:])
        root_count += 1
    root_wire = _checked_live_sum(root_wire, max(0, root_count - 1), 1)
    # A JSON mapping has one colon per field and one comma between fields.
    root_separators = _checked_live_sum(root_separators, 2 * root_count - 1)
    document_bytes = root_wire
    header = _budget_header_length((total, reserve, total, surface_count, document_bytes, b"0" * 64))
    payload = _checked_live_sum(len(BUDGETED_ENVELOPE), header, document_bytes)
    if document_bytes > sys.maxsize // 4:
        raise RepositoryError("transaction journal live-byte budget exceeded")
    graph = _checked_live_sum(
        4 * document_bytes,
        max(sys.getsizeof({}), sys.getsizeof([])) * root_openings,
        sys.getsizeof("") * root_strings,
        32 * root_separators,
    )
    return payload, graph


def _budgeted_initial_successor_upper_bounds(
        record: dict[str, object], *,
        additions: Sequence[tuple[str, SurfaceEnrollment, _BoundedBeforeMetadata | None]],
        total: int, reserve: int) -> tuple[int, int]:
    """Compatibility name for the shared bounded-enrollment scalar preflight."""
    return _budgeted_enrollment_successor_upper_bounds(
        record, additions=additions, total=total, reserve=reserve)


def _budget_header_length(header: tuple[int, int, int, int, int, bytes]) -> int:
    total, reserve, admitted, count, document_bytes, digest = header
    return len(
        f"total={total} reserve={reserve} admitted={admitted} surfaces={count} bytes={document_bytes} sha256=".encode("ascii")
    ) + len(digest) + 1


def _load_journal_payload(path: Path, *, budgeted_temp_limit: int | None = None,
                          authoritative_budget: tuple[int, int] | None = None) -> tuple[bytes, tuple[int, int, int]]:
    """Read a budgeted journal in bounded chunks after early cap admission."""
    try:
        descriptor = os.open(path, os.O_RDONLY)
    except OSError as exc:
        raise RepositoryError("invalid WEDL transaction journal") from exc
    try:
        initial = os.fstat(descriptor)
        if not stat.S_ISREG(initial.st_mode):
            raise RepositoryError("invalid WEDL transaction journal")
        prefix = os.read(descriptor, min(_BOUNDED_CAPTURE_CHUNK, initial.st_size))
        budgeted_envelope = prefix.startswith(BUDGETED_ENVELOPE)
        header = _budget_header_from_prefix(prefix[len(BUDGETED_ENVELOPE):] if budgeted_envelope else prefix)
        if header is None:
            # A versioned budget record must never be treated as a legacy
            # journal merely because its early header was damaged.  Doing so
            # would fall through to an unbounded whole-file legacy read.
            if budgeted_envelope or prefix.startswith(b'{"$liveByteBudget":') or b'"liveByteBudget"' in prefix:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            # A budgeted journal must never clean a marker-free candidate by
            # falling back to legacy whole-file reads.  Leave it visible.
            if budgeted_temp_limit is not None:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            # Pre-budget records retain their existing compatibility semantics.
            return path.read_bytes(), _identity(os.stat(path))
        if not budgeted_envelope:
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        total, reserve, _admitted, count, document_bytes, _digest = header
        if authoritative_budget is not None:
            total, reserve = authoritative_budget
        if initial.st_size != len(BUDGETED_ENVELOPE) + _budget_header_length(header) + document_bytes:
            raise RepositoryError("invalid WEDL transaction journal live-byte budget")
        if budgeted_temp_limit is not None and initial.st_size > budgeted_temp_limit:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        early_restart = _checked_live_sum(reserve, initial.st_size, initial.st_size, initial.st_size)
        _observe_live_bytes("restart-header-precheck", journal_file=initial.st_size, projected_peak=early_restart)
        if early_restart > total:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        # Count is an authenticated structural upper bound.  Price the record
        # containers before constructing JSON strings/lists for those surfaces.
        if _projected_live_sizes([], caller_reserve=reserve, journal_bytes=initial.st_size,
                                 surface_count=count) > total:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        document_offset = len(BUDGETED_ENVELOPE) + _budget_header_length(header)
        # Scan the authenticated document once without retaining chunks.  This
        # establishes the exact JSON graph ceiling before allocating the later
        # chunk list/join or the semantic duplicate-key sets.
        scanned_graph = _budgeted_json_graph_upper_bound_from_descriptor(
            descriptor, offset=document_offset, document_bytes=document_bytes)
        if _identity(os.fstat(descriptor)) != _identity(initial):
            raise RepositoryError("invalid WEDL transaction journal")
        pre_stream_peak = _checked_live_sum(reserve, 5 * initial.st_size, scanned_graph)
        _observe_live_bytes("budgeted-json-stream-pre-admit", document_bytes=document_bytes,
                            parsed_graph=scanned_graph, projected_peak=pre_stream_peak)
        if pre_stream_peak > total:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        try:
            os.lseek(descriptor, 0, os.SEEK_SET)
            prefix = os.read(descriptor, min(_BOUNDED_CAPTURE_CHUNK, initial.st_size))
        except OSError as exc:
            raise RepositoryError("invalid WEDL transaction journal") from exc
        if len(prefix) != min(_BOUNDED_CAPTURE_CHUNK, initial.st_size):
            raise RepositoryError("invalid WEDL transaction journal")
        chunks = [prefix]
        remaining = initial.st_size - len(prefix)
        while remaining:
            chunk = os.read(descriptor, min(_BOUNDED_CAPTURE_CHUNK, remaining))
            if not chunk:
                raise RepositoryError("invalid WEDL transaction journal")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1) or _identity(os.fstat(descriptor)) != _identity(initial):
            raise RepositoryError("invalid WEDL transaction journal")
        identity = _identity(initial)
        if not _path_has_identity(path, identity):
            raise RepositoryError("invalid WEDL transaction journal")
        payload = b"".join(chunks)
        _budgeted_document(payload, scanned_graph_bytes=scanned_graph,
                           authoritative_budget=authoritative_budget)
        return payload, identity
    except OSError as exc:
        raise RepositoryError("invalid WEDL transaction journal") from exc
    finally:
        os.close(descriptor)


def _decode(value: object) -> bytes | None:
    if value is None:
        return None
    if not isinstance(value, dict) or not isinstance(value.get("sha256"), str) or not isinstance(value.get("base64"), str):
        raise RepositoryError("invalid WEDL transaction journal image")
    try:
        data = base64.b64decode(value["base64"], validate=True)
    except (ValueError, TypeError) as exc:
        raise RepositoryError("invalid WEDL transaction journal image") from exc
    if sha256_bytes(data) != value["sha256"]:
        raise RepositoryError("corrupt WEDL transaction journal image")
    _observe_live_bytes(
        "surface-decode",
        encoded_text=sys.getsizeof(value["base64"]),
        decoded_image=sys.getsizeof(data),
        image_record=sys.getsizeof(value),
    )
    return data


def _validate_path(path: str) -> str:
    if not isinstance(path, str):
        raise RepositoryError("invalid transaction-owned path")
    candidate = Path(path)
    if not path or candidate.is_absolute() or ".." in candidate.parts or "\x00" in path:
        raise RepositoryError("invalid transaction-owned path")
    normalized = candidate.as_posix()
    folded = normalized.casefold()
    if folded == ".git" or folded.startswith(".git/") or folded == ".wedl/transactions" or folded.startswith(".wedl/transactions/"):
        raise RepositoryError("transaction-owned path is outside allowed surfaces")
    return normalized


def _safe_target(root: Path, relative: str) -> Path:
    """Reject existing symlink/junction components before any owned mutation."""
    root = root.resolve(strict=True)
    target = root / relative
    try:
        # Windows path comparison is case-insensitive even when pathlib's
        # lexical rendering is not.  Casefold both operands before containment.
        resolved = target.resolve(strict=False)
        if os.path.commonpath((str(root).casefold(), str(resolved).casefold())) != str(root).casefold():
            raise RepositoryError("transaction-owned path escapes repository")
    except ValueError as exc:
        raise RepositoryError("transaction-owned path escapes repository") from exc
    current = root
    for component in Path(relative).parts:
        current /= component
        try:
            status = os.lstat(current)
        except FileNotFoundError:
            continue
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if current.is_symlink() or reparse:
            raise RepositoryError("transaction-owned path crosses a reparse point")
    return target


def _safe_directory(root: Path, relative: str, *, create: bool = False) -> Path:
    """Return an owned journal directory without following links/reparse points."""
    if not relative or Path(relative).is_absolute() or ".." in Path(relative).parts:
        raise RepositoryError("invalid WEDL transaction directory")
    root = root.resolve(strict=True)
    current = root
    for component in Path(relative).parts:
        current /= component
        try:
            status = os.lstat(current)
        except FileNotFoundError:
            if not create:
                raise RepositoryError("missing WEDL transaction directory")
            _before_mutation("journal-directory-create")
            current.mkdir()
            status = os.lstat(current)
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if current.is_symlink() or reparse or not stat.S_ISDIR(status.st_mode):
            raise RepositoryError("WEDL transaction directory crosses a reparse point")
    return current


def _journal_directory(root: Path, transaction_id: str | None = None, *, create: bool = False) -> Path:
    base = _safe_directory(root, ".wedl", create=create)
    directory = _safe_directory(root, ".wedl/transactions", create=create)
    del base
    if transaction_id is None:
        return directory
    return _safe_directory(root, f".wedl/transactions/{transaction_id}", create=create)


def _read_file(root: Path, relative: str) -> bytes | None:
    target = _safe_target(root, relative)
    if not target.exists():
        return None
    if not target.is_file():
        raise RepositoryError("transaction-owned path is not a regular file")
    return target.read_bytes()


_BOUNDED_CAPTURE_CHUNK = 64 * 1024


def _bounded_descriptor_digest(path: Path, *, maximum: int,
                               error: str) -> tuple[str, tuple[int, int, int]]:
    """Hash one regular artifact through the opened descriptor, never read_bytes.

    Callers obtain the descriptor size first and admit it to the persisted
    ledger before calling this helper.  The fixed chunk keeps sealing, lock
    binding and cleanup from allocating an unbounded artifact after enrollment.
    """
    if isinstance(maximum, bool) or not isinstance(maximum, int) or maximum < 0:
        raise RepositoryError(error)
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise RepositoryError(error) from exc
    try:
        initial = os.fstat(descriptor)
        if not stat.S_ISREG(initial.st_mode) or initial.st_size > maximum:
            raise RepositoryError(error)
        digest = hashlib.sha256()
        remaining = initial.st_size
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            while remaining:
                chunk = handle.read(min(_BOUNDED_CAPTURE_CHUNK, remaining))
                if not chunk:
                    raise RepositoryError(error)
                digest.update(chunk)
                remaining -= len(chunk)
            if handle.read(1):
                raise RepositoryError(error)
        if _identity(os.fstat(descriptor)) != _identity(initial):
            raise RepositoryError(error)
        identity = _identity(initial)
        if not _path_has_identity(path, identity):
            raise RepositoryError(error)
        return digest.hexdigest(), identity
    except OSError as exc:
        raise RepositoryError(error) from exc
    finally:
        os.close(descriptor)


def _bounded_descriptor_bytes(path: Path, *, maximum: int, error: str) -> tuple[bytes, tuple[int, int, int]]:
    """Read an already-admitted small descriptor with bounded chunks."""
    if isinstance(maximum, bool) or not isinstance(maximum, int) or maximum < 0:
        raise RepositoryError(error)
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise RepositoryError(error) from exc
    try:
        initial = os.fstat(descriptor)
        if not stat.S_ISREG(initial.st_mode) or initial.st_size > maximum:
            raise RepositoryError(error)
        chunks: list[bytes] = []
        remaining = initial.st_size
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            while remaining:
                chunk = handle.read(min(_BOUNDED_CAPTURE_CHUNK, remaining))
                if not chunk:
                    raise RepositoryError(error)
                chunks.append(chunk)
                remaining -= len(chunk)
            if handle.read(1):
                raise RepositoryError(error)
        if _identity(os.fstat(descriptor)) != _identity(initial):
            raise RepositoryError(error)
        identity = _identity(initial)
        if not _path_has_identity(path, identity):
            raise RepositoryError(error)
        return b"".join(chunks), identity
    except OSError as exc:
        raise RepositoryError(error) from exc
    finally:
        os.close(descriptor)


@dataclass(frozen=True, slots=True)
class _BoundedBeforeImage:
    data: bytes
    identity: tuple[int, int, int]
    mode: int
    signature: tuple[int, int, int, int, int]


@dataclass(frozen=True, slots=True)
class _BoundedBeforeMetadata:
    """Descriptor-verified, payload-free capture facts used for admission."""
    identity: tuple[int, int, int]
    mode: int
    signature: tuple[int, int, int, int, int]

    @property
    def size(self) -> int:
        return self.signature[2]


def _open_no_follow_descriptor(root: Path, relative: str) -> int:
    """Open a file through descriptor-relative, no-follow parent traversal.

    POSIX uses descriptor-relative traversal.  Windows opens a single final
    handle with ``FILE_FLAG_OPEN_REPARSE_POINT``; the caller validates the
    handle identity and re-walks the public name before it accepts any bytes.
    That keeps capture bound to the opened handle instead of reopening the
    pathname to read it.
    """
    if os.name == "nt":
        target = _safe_target(root, relative)
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateFileW.argtypes = (
            ctypes.c_wchar_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p,
            ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p,
        )
        kernel32.CreateFileW.restype = ctypes.c_void_p
        kernel32.GetFileInformationByHandle.argtypes = (ctypes.c_void_p, ctypes.c_void_p)
        kernel32.GetFileInformationByHandle.restype = ctypes.c_bool
        kernel32.CloseHandle.argtypes = (ctypes.c_void_p,)
        kernel32.CloseHandle.restype = ctypes.c_bool

        class _FileInfo(ctypes.Structure):
            _fields_ = [
                ("attributes", ctypes.c_ulong), ("creation_low", ctypes.c_ulong),
                ("creation_high", ctypes.c_ulong), ("access_low", ctypes.c_ulong),
                ("access_high", ctypes.c_ulong), ("write_low", ctypes.c_ulong),
                ("write_high", ctypes.c_ulong), ("volume_serial", ctypes.c_ulong),
                ("size_high", ctypes.c_ulong), ("size_low", ctypes.c_ulong),
                ("links", ctypes.c_ulong), ("index_high", ctypes.c_ulong),
                ("index_low", ctypes.c_ulong),
            ]

        # Sharing delete is intentional: a concurrent replacement remains
        # observable and is rejected by the descriptor/public-name identity
        # checks instead of being hidden by an incidental sharing violation.
        handle = kernel32.CreateFileW(
            str(target), 0x80000000, 0x00000001 | 0x00000002 | 0x00000004,
            None, 3, 0x00200000, None,
        )
        invalid = ctypes.c_void_p(-1).value
        if handle == invalid:
            raise RepositoryError("transaction-owned before image cannot be opened")
        try:
            info = _FileInfo()
            if not kernel32.GetFileInformationByHandle(handle, ctypes.byref(info)):
                raise RepositoryError("transaction-owned before image cannot be opened")
            if info.attributes & 0x400:  # FILE_ATTRIBUTE_REPARSE_POINT
                raise RepositoryError("transaction-owned path crosses a reparse point")
            try:
                import msvcrt
                return msvcrt.open_osfhandle(handle, os.O_RDONLY)
            except OSError as exc:
                raise RepositoryError("transaction-owned before image cannot be opened") from exc
        except Exception:
            kernel32.CloseHandle(handle)
            raise
    if os.name != "posix" or not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_DIRECTORY"):
        raise RepositoryError("bounded before-image capture requires descriptor no-follow support")
    root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    current_fd = root_fd
    try:
        parts = Path(relative).parts
        for component in parts[:-1]:
            next_fd = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=current_fd)
            os.close(current_fd)
            current_fd = next_fd
        return os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=current_fd)
    except OSError as exc:
        raise RepositoryError("transaction-owned before image cannot be opened") from exc
    finally:
        os.close(current_fd)


def _capture_signature(status: os.stat_result) -> tuple[int, int, int, int, int]:
    """Include change times so in-place truncate/regrow is not accepted."""
    return (
        status.st_dev,
        status.st_ino,
        status.st_size,
        getattr(status, "st_mtime_ns", int(status.st_mtime * 1_000_000_000)),
        getattr(status, "st_ctime_ns", int(status.st_ctime * 1_000_000_000)),
    )


def _bounded_before_metadata(root: Path, relative: str) -> _BoundedBeforeMetadata | None:
    """Return stable descriptor facts without reading a before-image payload."""
    target = _safe_target(root, relative)
    try:
        named = os.lstat(target)
    except FileNotFoundError:
        return None
    reparse = getattr(named, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    if reparse or target.is_symlink() or not stat.S_ISREG(named.st_mode):
        raise RepositoryError("transaction-owned path is not a regular file")
    try:
        descriptor = _open_no_follow_descriptor(root, relative)
    except FileNotFoundError:
        return None
    except OSError as exc:
        raise RepositoryError("transaction-owned before image cannot be opened") from exc
    try:
        initial = os.fstat(descriptor)
        if not stat.S_ISREG(initial.st_mode):
            raise RepositoryError("transaction-owned path is not a regular file")
        # On Windows, opening the final name is handle-scoped but a parent can
        # still have been substituted between the first lexical walk and that
        # open. Re-walk every parent before trusting the handle identity.
        if _safe_target(root, relative) != target:
            raise RepositoryError("transaction-owned before image is unstable")
        current = os.lstat(target)
        current_reparse = getattr(current, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if current_reparse or target.is_symlink() or _identity(current) != _identity(initial):
            raise RepositoryError("transaction-owned before image is unstable")
        return _BoundedBeforeMetadata(
            _identity(initial), stat.S_IMODE(initial.st_mode), _capture_signature(initial)
        )
    except OSError as exc:
        raise RepositoryError("transaction-owned before image is unstable") from exc
    finally:
        os.close(descriptor)


def _bounded_before_image(root: Path, relative: str, maximum: int,
                          *, expected: _BoundedBeforeMetadata | None = None) -> _BoundedBeforeImage | None:
    """Capture one stable regular-file image without unbounded allocation.

    The largest descriptor read is ``_BOUNDED_CAPTURE_CHUNK`` and the only
    growth probe is one byte.  The final join allocates from the fstat-verified
    size, never from a current pathname size.  At cap ``B`` this function has
    at most ``B`` chunk bytes plus a ``B`` join result (and O(B/chunk) list
    references).  Enrollment then adds the base64 image (4*ceil(B/3)) to the
    decoded record plus prior and replacement JSON journal bytes.  Callers
    requiring a whole-process ceiling must reserve ``2B + 3*(J + 4*ceil(B/3))``
    with ``J`` their pre-enrollment journal payload, in addition to after-image
    and application-object budgets.
    """
    target = _safe_target(root, relative)
    try:
        named = os.lstat(target)
    except FileNotFoundError:
        return None
    reparse = getattr(named, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    if reparse or target.is_symlink() or not stat.S_ISREG(named.st_mode):
        raise RepositoryError("transaction-owned path is not a regular file")
    try:
        descriptor = _open_no_follow_descriptor(root, relative)
    except FileNotFoundError:
        return None
    except OSError as exc:
        raise RepositoryError("transaction-owned before image cannot be opened") from exc
    try:
        initial = os.fstat(descriptor)
        if not stat.S_ISREG(initial.st_mode):
            raise RepositoryError("transaction-owned path is not a regular file")
        # Do not let an open through a substituted parent become authoritative:
        # this second no-reparse containment walk is deliberately before the
        # first payload read and is required even for a protected Windows
        # final-file handle.
        if _safe_target(root, relative) != target:
            raise RepositoryError("transaction-owned before image is unstable")
        # Verify the pathname still denotes this descriptor before reading any
        # payload. This catches a replacement or reparse substitution between
        # the lexical safety check and open on platforms without O_NOFOLLOW.
        current = os.lstat(target)
        current_reparse = getattr(current, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if current_reparse or target.is_symlink() or _identity(current) != _identity(initial):
            raise RepositoryError("transaction-owned before image is unstable")
        signature = _capture_signature(initial)
        if expected is not None and (
            _identity(initial) != expected.identity
            or stat.S_IMODE(initial.st_mode) != expected.mode
            or signature != expected.signature
        ):
            raise RepositoryError("transaction-owned before image is unstable")
        size = initial.st_size
        if size > maximum:
            raise RepositoryError("transaction-owned before image exceeds capture limit")
        chunks: list[bytes] = []
        remaining = size
        while remaining:
            chunk = os.read(descriptor, min(_BOUNDED_CAPTURE_CHUNK, remaining))
            if not chunk:
                raise RepositoryError("transaction-owned before image is unstable")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise RepositoryError("transaction-owned before image is unstable")
        final = os.fstat(descriptor)
        if _capture_signature(final) != _capture_signature(initial):
            raise RepositoryError("transaction-owned before image is unstable")
        # Re-run the no-reparse containment walk and require the public name to
        # remain attached to the exact captured regular-file identity.
        final_target = _safe_target(root, relative)
        if final_target != target or not _path_has_identity(final_target, _identity(initial)):
            raise RepositoryError("transaction-owned before image is unstable")
        return _BoundedBeforeImage(
            b"".join(chunks), _identity(initial), stat.S_IMODE(initial.st_mode), _capture_signature(initial)
        )
    except OSError as exc:
        raise RepositoryError("transaction-owned before image is unstable") from exc
    finally:
        os.close(descriptor)


def _bounded_metadata_is_current(root: Path, relative: str,
                                 expected: _BoundedBeforeMetadata | None) -> bool:
    """Recheck the metadata-only identity-or-absence enrollment token."""
    target = _safe_target(root, relative)
    if expected is None:
        try:
            named = os.lstat(target)
        except FileNotFoundError:
            return True
        reparse = getattr(named, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        return False if not reparse and stat.S_ISREG(named.st_mode) else False
    try:
        descriptor = _open_no_follow_descriptor(root, relative)
    except (OSError, RepositoryError):
        return False
    try:
        current = os.fstat(descriptor)
        try:
            named = os.lstat(target)
        except OSError:
            return False
        reparse = getattr(named, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        return (
            not reparse and not target.is_symlink() and stat.S_ISREG(current.st_mode)
            and _identity(current) == expected.identity
            and stat.S_IMODE(current.st_mode) == expected.mode
            and _capture_signature(current) == expected.signature
            and _identity(named) == expected.identity
            and _safe_target(root, relative) == target
        )
    except OSError:
        return False
    finally:
        os.close(descriptor)


def _bounded_image_is_current(root: Path, relative: str, captured: _BoundedBeforeImage) -> bool:
    """Re-open safely at the persist boundary and require the captured inode."""
    try:
        descriptor = _open_no_follow_descriptor(root, relative)
    except RepositoryError:
        return False
    try:
        status = os.fstat(descriptor)
        return (
            _identity(status) == captured.identity
            and stat.S_IMODE(status.st_mode) == captured.mode
            and _capture_signature(status) == captured.signature
        )
    except OSError:
        return False
    finally:
        os.close(descriptor)


def _bounded_image_matches(root: Path, relative: str, expected: bytes | None, limit: int,
                           identity: tuple[int, int, int] | None) -> bool:
    """Compare a durable bounded image without reopening it through Path.read_bytes."""
    image = _bounded_before_image(root, relative, limit)
    if image is None:
        return expected is None
    return image.data == expected and (identity is None or image.identity == identity)


def _bounded_path_matches_image(path: Path, expected: bytes | None, maximum: int, *,
                                identity: tuple[int, int, int] | None = None,
                                mode: int | None = None) -> bool:
    """Compare an external file through a capped descriptor read.

    Budgeted recovery must never turn an adversarial post-enrollment pathname
    into a whole-file allocation.  ``maximum`` comes from durable capture
    metadata when present, otherwise from the authenticated journal image's
    own length.  The descriptor and public name are both rechecked so a
    same-byte replacement cannot be accepted before a claim or unlink.
    """
    if maximum < 0 or (expected is not None and len(expected) > maximum):
        raise RepositoryError("transaction-owned image exceeds bounded limit")
    try:
        named = os.lstat(path)
    except FileNotFoundError:
        return expected is None
    reparse = getattr(named, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    if reparse or path.is_symlink() or not stat.S_ISREG(named.st_mode):
        raise RepositoryError("transaction-owned image is unstable")
    if expected is None:
        return False
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0)
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise RepositoryError("transaction-owned image is unstable") from exc
    try:
        initial = os.fstat(descriptor)
        if not stat.S_ISREG(initial.st_mode) or _identity(named) != _identity(initial):
            raise RepositoryError("transaction-owned image is unstable")
        if identity is not None and _identity(initial) != identity:
            return False
        if mode is not None and stat.S_IMODE(initial.st_mode) != mode:
            return False
        size = initial.st_size
        if size > maximum:
            raise RepositoryError("transaction-owned image exceeds bounded limit")
        # A different size cannot equal the authenticated byte image, so avoid
        # any read at all.  This is also a hard guard against a substituted
        # sparse file whose current size is larger than its enrolled image.
        if size != len(expected):
            return False
        chunks: list[bytes] = []
        remaining = size
        while remaining:
            chunk = os.read(descriptor, min(_BOUNDED_CAPTURE_CHUNK, remaining))
            if not chunk:
                raise RepositoryError("transaction-owned image is unstable")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise RepositoryError("transaction-owned image is unstable")
        final = os.fstat(descriptor)
        current = os.lstat(path)
        current_reparse = getattr(current, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if (current_reparse or path.is_symlink() or _identity(current) != _identity(initial)
                or _capture_signature(final) != _capture_signature(initial)):
            raise RepositoryError("transaction-owned image is unstable")
        return b"".join(chunks) == expected
    except OSError as exc:
        raise RepositoryError("transaction-owned image is unstable") from exc
    finally:
        os.close(descriptor)


def _bounded_public_image_matches(root: Path, relative: str, expected: bytes | None, maximum: int, *,
                                  identity: tuple[int, int, int] | None = None,
                                  mode: int | None = None) -> bool:
    """Capped public-name comparison with a final no-reparse containment walk."""
    target = _safe_target(root, relative)
    matched = _bounded_path_matches_image(target, expected, maximum, identity=identity, mode=mode)
    if _safe_target(root, relative) != target:
        raise RepositoryError("transaction-owned path escapes repository")
    return matched


def _matches_image(root: Path, relative: str, data: bytes | None, mode: int | None,
                   identity: tuple[int, int, int] | None = None, limit: int | None = None) -> bool:
    if limit is not None:
        return _bounded_image_matches(root, relative, data, limit, identity)
    actual = _read_file(root, relative)
    if actual != data:
        return False
    if identity is not None:
        try:
            if _identity(os.stat(_safe_target(root, relative))) != identity:
                return False
        except OSError:
            return False
    if data is None or mode is None or os.name == "nt":
        return True
    return stat.S_IMODE(os.stat(_safe_target(root, relative)).st_mode) == mode


def _fsync_directory(path: Path) -> None:
    if os.name == "nt":
        return
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _identity(status: os.stat_result) -> tuple[int, int, int]:
    """The namespace identity used by every last-moment ownership check."""
    return (status.st_dev, status.st_ino, status.st_size)


def _path_has_identity(path: Path, expected: tuple[int, int, int]) -> bool:
    try:
        return _identity(os.stat(path)) == expected
    except OSError:
        return False


def _parse_identity(value: str) -> tuple[int, int, int]:
    try:
        pieces = tuple(int(piece, 16) for piece in value.split(":"))
    except ValueError as exc:
        raise RepositoryError("invalid transaction artifact identity") from exc
    if len(pieces) != 3:
        raise RepositoryError("invalid transaction artifact identity")
    return pieces  # type: ignore[return-value]


def _unlink_inode_claim(path: Path, identity: tuple[int, int, int], digest: str, *, mutation: str, error: str,
                        expected: bytes | None = None, bounded: bool = False) -> None:
    """Remove only the inode we just hard-link claimed from a private path.

    The claim both makes a durable same-inode witness for the final check and
    lets a restarted cleanup distinguish an inherited interrupted claim from a
    substituted private file.  The claim is deliberately adjacent to the
    target, never a broad temporary-directory cleanup target.
    """
    claim = path.with_name(f".{path.name}.wedl-unlink-claim")

    def verified(candidate: Path) -> bool:
        try:
            if not _path_has_identity(candidate, identity):
                return False
            if expected is not None:
                return (_bounded_path_matches_image(candidate, expected, len(expected), identity=identity)
                        and sha256_bytes(expected) == digest)
            observed_digest, observed_identity = _bounded_descriptor_digest(
                candidate, maximum=identity[2], error=error,
            )
            return observed_identity == identity and observed_digest == digest
        except (OSError, RepositoryError):
            return False

    if claim.exists():
        if not verified(claim):
            raise RepositoryError(error)
        # It can only be a surviving claim from our own earlier interrupted
        # cleanup, because it is the exact inode/digest we are about to remove.
        if not path.exists():
            # The primary unlink already happened (possibly via os._exit).
            # The sealed hard-link is the durable tombstone: clear only it and
            # let restart cleanup continue with the remaining exact targets.
            claim.unlink(); _fsync_directory(claim.parent)
            return
        claim.unlink(); _fsync_directory(claim.parent)
    if not verified(path):
        raise RepositoryError(error)
    try:
        os.link(path, claim)
    except OSError as exc:
        raise RepositoryError(error) from exc
    try:
        if not verified(path) or not verified(claim):
            raise RepositoryError(error)
        _before_mutation(mutation)
        if not verified(path) or not verified(claim):
            raise RepositoryError(error)
        path.unlink()
        _fsync_directory(path.parent)
        # This is intentionally after the primary namespace mutation.  A real
        # process death here leaves only the verified hard-link tombstone for
        # restart adoption, which is stronger than an in-process finally path.
        _before_mutation(f"{mutation}-after-primary-unlink")
    finally:
        # Leave a claim behind on a failed final ownership check; it is useful
        # evidence and a later retry will only remove it after revalidation.
        if not path.exists() and verified(claim):
            try:
                claim.unlink()
                _fsync_directory(claim.parent)
            except OSError:
                pass


def _atomic_write(path: Path, data: bytes, *, expected: bytes | None | object = ...,
                   expected_identity: tuple[int, int, int] | None = None,
                   pre_replace: Callable[[], bool] | None = None,
                   bounded_cas: bool = False, temporary_prefix: str = ".wedl-txn-") -> tuple[int, int, int]:
    """Atomically install bytes, with a final ownership check at replacement."""
    _observe_live_bytes("atomic-replace", replacement_payload=sys.getsizeof(data), path_text=sys.getsizeof(str(path)))
    # Parents must not resolve through a symlink/junction; create them one at a
    # time after the caller has validated the repository-relative target.
    path.parent.mkdir(parents=True, exist_ok=True)
    if not re.fullmatch(r"\.wedl-txn-[0-9a-f-]{36}-", temporary_prefix):
        raise RepositoryError("invalid transaction journal temporary name")
    descriptor, temporary = tempfile.mkstemp(prefix=temporary_prefix, dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data); output.flush(); os.fsync(output.fileno())
        if expected is not ...:
            current_matches = (
                _bounded_path_matches_image(path, expected, len(expected) if expected is not None else 0,
                                            identity=expected_identity)
                if bounded_cas else (path.read_bytes() if path.exists() else None) == expected
            )
            if not current_matches:
                raise RepositoryError("transaction recovery lost surface ownership")
            if expected is not None and expected_identity is not None and not _path_has_identity(path, expected_identity):
                raise RepositoryError("transaction recovery lost surface ownership")
        _before_mutation("atomic-replace")
        # Failure injection and an interleaving writer run at this exact final
        # boundary.  Recheck the recorded generation image after the hook, not
        # just before it, so an identical-byte replacement is not accepted.
        if expected is not ...:
            current_matches = (
                _bounded_path_matches_image(path, expected, len(expected) if expected is not None else 0,
                                            identity=expected_identity)
                if bounded_cas else (path.read_bytes() if path.exists() else None) == expected
            )
            if not current_matches or (expected is not None and expected_identity is not None and not _path_has_identity(path, expected_identity)):
                raise RepositoryError("transaction recovery lost surface ownership")
        if pre_replace is not None and not pre_replace():
            raise RepositoryError("transaction-owned before image is unstable")
        os.replace(temporary, path)
        # The namespace replacement is already durable state from the API's
        # point of view.  A post-replacement hook/fsync/verification failure
        # must therefore not make the caller roll its in-memory record back to
        # the old generation while this new one remains on disk.  Retain the
        # replacement when its descriptor identity can still be established;
        # ownership loss still fails closed below.
        try:
            _before_mutation("atomic-replace-after-primary")
            _fsync_directory(path.parent)
            if (not _bounded_path_matches_image(path, data, len(data)) if bounded_cas else path.read_bytes() != data):
                raise RepositoryError("transaction recovery lost surface ownership")
            return _identity(os.stat(path))
        except Exception:
            try:
                identity = _identity(os.stat(path))
                matches = (_bounded_path_matches_image(path, data, len(data), identity=identity)
                           if bounded_cas else path.read_bytes() == data)
            except OSError:
                raise
            if matches:
                return identity
            raise
    finally:
        Path(temporary).unlink(missing_ok=True)


def _canonical_journal_payload(record: dict[str, object]) -> bytes:
    """Build the one canonical durable representation and expose its live sizes."""
    text = json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n"
    document = text.encode("utf-8")
    payload = document
    if record.get("version") == BUDGETED_FORMAT_VERSION:
        total, reserve, admitted = _live_budget_fields(record.get("liveByteBudget"))
        surfaces = record.get("surfaces")
        if not isinstance(surfaces, list):
            raise RepositoryError("invalid WEDL transaction journal")
        header = (
            f"total={total} reserve={reserve} admitted={admitted} surfaces={len(surfaces)} "
            f"bytes={len(document)} sha256={sha256_bytes(document)}\n"
        ).encode("ascii")
        payload = BUDGETED_ENVELOPE + header + document
    _observe_live_bytes(
        "json-dump-encode",
        record=sys.getsizeof(record),
        canonical_text=sys.getsizeof(text),
        encoded_payload=sys.getsizeof(payload),
    )
    return payload


def _create_file(root: Path, relative: str, path: Path, data: bytes, *, mutation: str, mode: int | None = None) -> None:
    """Create a file without overwriting a contender's publication."""
    # The public path has already passed _safe_target.  Recheck after creating
    # parents so a junction introduced during directory creation is rejected.
    path.parent.mkdir(parents=True, exist_ok=True)
    # ``mkdir`` is a namespace mutation too.  Do not rely on the earlier
    # lexical check if an attacker replaces a just-created ancestor before the
    # actual file open.
    _before_mutation("surface-parent-created")
    if _safe_target(root, relative) != path:
        raise RepositoryError("transaction-owned path escapes repository")
    _before_mutation(mutation)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError as exc:
        raise RepositoryError("transaction recovery lost surface ownership") from exc
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data); output.flush(); os.fsync(output.fileno())
        _before_mutation(f"{mutation}-written")
        if mode is not None:
            _before_mutation("surface-mode-publish")
            os.chmod(path, mode)
        _fsync_directory(path.parent)
    except Exception:
        try:
            path.unlink()
        except OSError:
            pass
        raise


def _create_private_file(root: Path, transaction_id: str, path: Path, data: bytes, *, mutation: str, mode: int | None = None) -> None:
    """Create a journal-private file after rechecking its real containment."""
    private = _journal_directory(root, transaction_id, create=True)
    path.parent.mkdir(parents=True, exist_ok=True)
    _before_mutation("private-parent-created")
    try:
        resolved_parent = path.parent.resolve(strict=True)
        if os.path.commonpath((str(private).casefold(), str(resolved_parent).casefold())) != str(private).casefold():
            raise RepositoryError("WEDL transaction directory crosses a reparse point")
    except ValueError as exc:
        raise RepositoryError("WEDL transaction directory crosses a reparse point") from exc
    current = private
    for component in path.relative_to(private).parent.parts:
        current /= component
        status = os.lstat(current)
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if current.is_symlink() or reparse or not stat.S_ISDIR(status.st_mode):
            raise RepositoryError("WEDL transaction directory crosses a reparse point")
    _create_file_bytes(path, data, mutation=mutation, mode=mode)


def _create_file_bytes(path: Path, data: bytes, *, mutation: str, mode: int | None = None) -> None:
    """O_EXCL write for a path whose containment was just verified."""
    _before_mutation(mutation)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError as exc:
        raise RepositoryError("transaction recovery lost surface ownership") from exc
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data); output.flush(); os.fsync(output.fileno())
        _before_mutation(f"{mutation}-written")
        if mode is not None:
            _before_mutation("surface-mode-publish")
            os.chmod(path, mode)
        _fsync_directory(path.parent)
    except Exception:
        try:
            path.unlink()
        except OSError:
            pass
        raise


def _unlink_expected(path: Path, expected: bytes, *, mutation: str) -> None:
    if not path.is_file() or path.read_bytes() != expected:
        raise RepositoryError("transaction recovery lost surface ownership")
    _before_mutation(mutation)
    # A second read catches every injectable/interpreted interleaving before
    # mutating the namespace; the parent lock serializes WEDL writers.
    if not path.is_file() or path.read_bytes() != expected:
        raise RepositoryError("transaction recovery lost surface ownership")
    path.unlink()
    _fsync_directory(path.parent)


def _claim_then_remove(path: Path, claim: Path, *, expected: bytes, mutation: str) -> None:
    """Unlink one public name only after an inode-stable hard-link claim.

    A read/check/unlink sequence can remove a same-byte replacement.  The claim
    is a second name for the original inode; every final check compares the
    public name to it before removal, preserving substitutions byte-for-byte.
    """
    if claim.exists():
        raise RepositoryError("transaction recovery lost surface ownership")
    try:
        os.link(path, claim)
    except OSError as exc:
        raise RepositoryError("transaction recovery lost surface ownership") from exc
    try:
        claimed = _identity(os.stat(claim))
        if not _path_has_identity(path, claimed) or claim.read_bytes() != expected or path.read_bytes() != expected:
            raise RepositoryError("transaction recovery lost surface ownership")
        _before_mutation(mutation)
        if not _path_has_identity(path, claimed) or claim.read_bytes() != expected or path.read_bytes() != expected:
            raise RepositoryError("transaction recovery lost surface ownership")
        path.unlink()
        _fsync_directory(path.parent)
    finally:
        try:
            claim.unlink()
        except OSError:
            pass


_LOCK_RECORD_MAX_BYTES = 4096


def _lock_fields(path: Path, *, maximum: int = _LOCK_RECORD_MAX_BYTES) -> dict[str, str] | None:
    """Parse a lock record through an explicitly capped descriptor read."""
    try:
        payload, _identity_value = _bounded_descriptor_bytes(
            path, maximum=maximum, error="canonical write lock ownership changed")
        fields = dict(line.split("=", 1) for line in payload.decode("ascii").splitlines() if "=" in line)
    except (OSError, UnicodeDecodeError, ValueError, RepositoryError):
        return None
    return fields or None


class TransactionJournal:
    """Versioned journal whose mutations are individually ownership checked."""

    def __init__(self, root: Path, record: dict[str, object], journal_path: Path) -> None:
        self.root, self.record, self.path = root, record, journal_path
        self._journal_bytes: bytes | None = None
        self._journal_identity: tuple[int, int, int] | None = None
        self._surfaces_cache: tuple[Surface, ...] | None = None
        self._validate()

    @property
    def transaction_id(self) -> str:
        return str(self.record["id"])

    @property
    def phase(self) -> str:
        return str(self.record["phase"])

    @property
    def surfaces(self) -> tuple[Surface, ...]:
        # Budgeted restart validates decoded images once and retains that exact
        # tuple for publication/restore; repeatedly decoding every base64 image
        # would create an unmodeled recovery-only peak.
        if self._surfaces_cache is None:
            self._surfaces_cache = tuple(
                Surface.from_json(value, version=int(self.record["version"]))
                for value in self.record["surfaces"]
            )
        return self._surfaces_cache

    @classmethod
    def create(cls, root: Path, *, ref: str, previous_head: str, committed_head: str,
               surfaces: list[Surface] | tuple[Surface, ...] = (),
               index_before: dict[str, dict[str, object] | None] | None = None,
               index_after: dict[str, dict[str, object] | None] | None = None) -> "TransactionJournal":
        transaction_id = str(uuid.uuid4())
        record: dict[str, object] = {"version": FORMAT_VERSION, "generation": 0, "id": transaction_id, "phase": "prepared", "ref": ref,
            "previousHead": previous_head, "committedHead": committed_head,
            "surfaces": [surface.as_json(version=FORMAT_VERSION) for surface in surfaces],
            "indexBefore": dict(index_before or {}), "indexAfter": dict(index_after or {}), "indexLock": None, "indexArtifacts": None, "canonicalLock": None, "privateArtifacts": [], "sealedArtifacts": {}, "surfaceArtifacts": {}}
        journal_directory = _journal_directory(root, create=True)
        journal = cls(root, record, journal_directory / f"{transaction_id}.json")
        journal._persist()
        return journal

    @classmethod
    def load(cls, root: Path, path: Path) -> "TransactionJournal":
        directory = _journal_directory(root, create=False)
        if path.parent != directory or path.suffix != ".json":
            raise RepositoryError("invalid WEDL transaction journal path")
        try:
            payload, identity = _load_journal_payload(path)
            document = _budgeted_document(payload)
            record = json.loads(document.decode("utf-8"))
        except (OSError, ValueError) as exc:
            raise RepositoryError("invalid WEDL transaction journal") from exc
        if not isinstance(record, dict):
            raise RepositoryError("invalid WEDL transaction journal")
        _validate_budgeted_record_header(record, payload)
        # Admission of the parsed container happens before ``__init__`` can
        # materialize Surface objects or decode a single base64 image.
        _preparse_budgeted_record_admission(record, journal_bytes=len(payload))
        journal = cls(root, record, path)
        # Do not trust a parsed image alone: every later generation update is a
        # CAS from this exact inode and canonical JSON bytes.
        journal._journal_bytes, journal._journal_identity = payload, identity
        if not _path_has_identity(path, identity):
            raise RepositoryError("invalid WEDL transaction journal")
        journal._validate_persisted_live_budget(len(payload))
        journal._cleanup_authenticated_atomic_temps()
        return journal

    def _cleanup_authenticated_atomic_temps(self) -> None:
        """Reclaim only a crashed next-generation temp proven to be ours.

        `mkstemp` artifacts are discoverable by transaction id, but the name is
        not authority.  Recovery parses and validates the candidate record,
        requires the same transaction id and exactly the next generation, then
        removes the exact inode through a hard-link claim.  A foreign or stale
        lookalike is deliberately left operator-visible.
        """
        directory = _journal_directory(self.root, create=False)
        prefix = f".wedl-txn-{self.transaction_id}-"
        try:
            candidates = directory.iterdir()
        except OSError as exc:
            raise RepositoryError("invalid WEDL transaction journal directory") from exc
        for candidate in candidates:
            self._cleanup_authenticated_atomic_temp(candidate)

    def _cleanup_authenticated_atomic_temp(self, candidate: Path) -> None:
        """Process one next-generation candidate without retaining its payload.

        Directory enumeration is deliberately streaming.  In particular, do not
        retain one decoded candidate while opening the next: every candidate is
        admitted against the current journal cleanup ledger before its first
        payload read and its local graph dies at this method boundary.
        """
        prefix = f".wedl-txn-{self.transaction_id}-"
        if not candidate.name.startswith(prefix):
            return
        try:
            initial = os.lstat(candidate)
        except OSError:
            return
        reparse = getattr(initial, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if reparse or candidate.is_symlink() or not stat.S_ISREG(initial.st_mode):
            return
        try:
            # This is before `_load_journal_payload`, JSON decoding, or any
            # base64 allocation.  `payload_bytes` prices the candidate's
            # descriptor chunks, joined image and exact unlink verifier.
            self._require_budgeted_operation("cleanup", path=str(candidate), payload_bytes=initial.st_size)
            total = None
            authoritative_budget = None
            if self._is_budgeted:
                total, reserve, _admitted = _live_budget_fields(self.record["liveByteBudget"])
                authoritative_budget = (total, reserve)
            # A next-generation temporary is not authoritative.  Its claimed
            # envelope must never enlarge the live cap used to scan, parse, or
            # decode it before we have proved it belongs to this journal.
            payload, identity = _load_journal_payload(
                candidate, budgeted_temp_limit=total,
                authoritative_budget=authoritative_budget,
            )
            document = _budgeted_document(payload, authoritative_budget=authoritative_budget)
            record = json.loads(document.decode("utf-8"))
            if not isinstance(record, dict):
                return
            _validate_budgeted_record_header(record, payload)
            _preparse_budgeted_record_admission(
                record, journal_bytes=len(payload), authoritative_budget=authoritative_budget,
            )
            contender = TransactionJournal(self.root, record, self.path)
            contender._validate_persisted_live_budget(len(payload))
            if (contender.transaction_id != self.transaction_id
                    or contender.record["generation"] != int(self.record["generation"]) + 1):
                return
            _unlink_inode_claim(
                candidate, identity, sha256_bytes(payload),
                mutation="journal-temp-cleanup-unlink",
                error="transaction journal temporary changed ownership",
                expected=payload, bounded=contender._is_budgeted,
            )
        except (OSError, ValueError, RepositoryError):
            # A damaged or unowned temporary is never broad-cleaned.
            return

    @classmethod
    def pending(cls, root: Path) -> tuple["TransactionJournal", ...]:
        try:
            directory = _journal_directory(root, create=False)
        except RepositoryError:
            # A normal cache/receipt directory may exist without a transaction
            # directory; only an existing unsafe journal component is fatal.
            transaction_dir = root / ".wedl" / "transactions"
            if not transaction_dir.exists():
                return ()
            raise
        # A post-unlink crash may leave only the authenticated hard-link claim
        # for the journal itself.  Claims are discoverable at the journal root;
        # restore only an absent exact name from its same-inode witness.
        suffix = ".json.wedl-unlink-claim"
        for claim in sorted(directory.glob(f"*{suffix}")):
            name = claim.name
            if not name.startswith("."):
                continue
            original = name[1:-len(".wedl-unlink-claim")]
            if not re.fullmatch(r"[0-9a-f-]{36}\.json", original):
                continue
            journal_path = directory / original
            if journal_path.exists():
                continue
            try:
                # A claim is a recovery tombstone, not authority to create a
                # public journal name.  Admit its authenticated budget before
                # the hard-link mutates the journal namespace.
                payload, identity = _load_journal_payload(claim)
                document = _budgeted_document(payload)
                record = json.loads(document.decode("utf-8"))
                if not isinstance(record, dict):
                    continue
                _validate_budgeted_record_header(record, payload)
                _preparse_budgeted_record_admission(record, journal_bytes=len(payload))
                tombstone = cls(root, record, journal_path)
                tombstone._journal_bytes, tombstone._journal_identity = payload, identity
                tombstone._validate_persisted_live_budget(len(payload))
                if tombstone.transaction_id != original[:-5]:
                    continue
                tombstone._require_budgeted_operation(
                    "cleanup", path=str(journal_path), payload_bytes=len(payload))
                # The claim must remain the exact authenticated inode through
                # admission; an interleaving replacement stays visible.
                if not _path_has_identity(claim, identity):
                    continue
                _before_mutation("pending-tombstone-restore")
                if not _path_has_identity(claim, identity) or journal_path.exists():
                    continue
                os.link(claim, journal_path)
                _fsync_directory(directory)
            except (OSError, ValueError, RepositoryError):
                continue
        return tuple(cls.load(root, path) for path in sorted(directory.glob("*.json")))

    @classmethod
    def owns_stale_lock(cls, root: Path, lock_path: Path) -> bool:
        fields = _lock_fields(lock_path)
        if not fields or set(fields) != {"pid", "token", "transaction"}:
            return False
        try:
            pid = int(fields["pid"]); uuid.UUID(fields["token"]); uuid.UUID(fields["transaction"])
        except (ValueError, KeyError):
            return False
        if _process_dead(pid) is not True:
            return False
        try:
            journal = cls.load(root, root / ".wedl" / "transactions" / f"{fields['transaction']}.json")
        except RepositoryError:
            return False
        try:
            return (journal.transaction_id == fields["transaction"]
                    and journal.owns_canonical_lock(lock_path, pid=pid, token=fields["token"]))
        except RepositoryError:
            # A bounded recovery admission failure is contention, never a
            # reason to create a stale-lock claim or disturb the caller's lock.
            return False

    @classmethod
    def reclaim_owned_stale_lock(cls, root: Path, lock_path: Path) -> bool:
        """Atomically claim a dead journal-bound lock before removing it.

        The hard-link claim gives us a stable file identity.  We only unlink
        the canonical name while its stat identity still equals that claim;
        replacement by a new writer is therefore treated as contention.
        """
        if not cls.owns_stale_lock(root, lock_path):
            return False
        fields = _lock_fields(lock_path)
        assert fields is not None
        try:
            journal = cls.load(root, root / ".wedl" / "transactions" / f"{fields['transaction']}.json")
            durable = journal.record["canonicalLock"]
            if not isinstance(durable, dict):
                return False
            expected_identity = str(durable["identity"])
            expected_digest = str(durable["sha256"])
        except (RepositoryError, KeyError):
            return False
        try:
            lock_status = os.lstat(lock_path)
            reparse = getattr(lock_status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
            if lock_path.is_symlink() or reparse or not stat.S_ISREG(lock_status.st_mode):
                return False
            # The claim directory and hard link are external mutations.  Price
            # their bounded lock payload before either namespace change.
            journal._require_budgeted_operation(
                "lock", path=str(lock_path), payload_bytes=lock_status.st_size)
        except (OSError, RepositoryError):
            return False
        claim_dir = _journal_directory(root, fields["transaction"], create=True) / "lock-claims"
        if not claim_dir.exists():
            _before_mutation("lock-claim-directory-create")
            claim_dir.mkdir()
        claim = claim_dir / f"{fields['token']}.claim"
        try:
            if journal._is_budgeted:
                current = os.lstat(lock_path)
                journal._require_budgeted_operation(
                    "lock", path=str(lock_path), payload_bytes=current.st_size)
            _before_mutation("stale-lock-claim")
            os.link(lock_path, claim)
        except (FileExistsError, OSError):
            return False
        try:
            claimed = os.stat(claim)
            current = os.stat(lock_path)
            if journal._is_budgeted:
                journal._require_budgeted_operation("lock", path=str(lock_path), payload_bytes=max(claimed.st_size, current.st_size))
            if (claimed.st_dev, claimed.st_ino, claimed.st_size) != (current.st_dev, current.st_ino, current.st_size):
                return False
            identity = f"{claimed.st_dev:x}:{claimed.st_ino:x}:{claimed.st_size:x}"
            maximum = max(claimed.st_size, current.st_size)
            claimed_payload, claimed_identity = _bounded_descriptor_bytes(claim, maximum=maximum, error="canonical write lock ownership changed")
            lock_payload, lock_identity = _bounded_descriptor_bytes(lock_path, maximum=maximum, error="canonical write lock ownership changed")
            if (identity != expected_identity or claimed_identity != _identity(claimed) or lock_identity != _identity(current)
                    or sha256_bytes(claimed_payload) != expected_digest or sha256_bytes(lock_payload) != expected_digest
                    or _lock_fields(lock_path, maximum=maximum) != fields):
                return False
            _before_mutation("stale-lock-reclaim")
            # Recheck the name immediately before unlinking; no broad stale-lock
            # deletion is permitted, and failure leaves both artifacts intact.
            current = os.stat(lock_path)
            current_identity = f"{current.st_dev:x}:{current.st_ino:x}:{current.st_size:x}"
            claim_identity = f"{claimed.st_dev:x}:{claimed.st_ino:x}:{claimed.st_size:x}"
            if journal._is_budgeted:
                journal._require_budgeted_operation("lock", path=str(lock_path), payload_bytes=max(claimed.st_size, current.st_size))
            claimed_payload, claimed_identity = _bounded_descriptor_bytes(claim, maximum=max(claimed.st_size, current.st_size), error="canonical write lock ownership changed")
            lock_payload, lock_identity = _bounded_descriptor_bytes(lock_path, maximum=max(claimed.st_size, current.st_size), error="canonical write lock ownership changed")
            if (claimed.st_dev, claimed.st_ino, claimed.st_size) != (current.st_dev, current.st_ino, current.st_size) or current_identity != expected_identity or claim_identity != expected_identity or claimed_identity != _identity(claimed) or lock_identity != _identity(current) or sha256_bytes(claimed_payload) != expected_digest or sha256_bytes(lock_payload) != expected_digest or _lock_fields(lock_path, maximum=max(claimed.st_size, current.st_size)) != fields:
                return False
            lock_path.unlink()
            _fsync_directory(lock_path.parent)
            return True
        finally:
            try:
                claim.unlink()
            except OSError:
                pass

    def bind_lock(self, lock_path: Path, *, pid: int, token: str) -> None:
        if not isinstance(pid, int) or isinstance(pid, bool) or not isinstance(token, str):
            raise RepositoryError("canonical write lock ownership changed")
        try:
            initial = os.lstat(lock_path)
        except OSError as exc:
            raise RepositoryError("canonical write lock ownership changed") from exc
        reparse = getattr(initial, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if lock_path.is_symlink() or reparse or not stat.S_ISREG(initial.st_mode):
            raise RepositoryError("canonical write lock ownership changed")
        replacement = f"pid={pid}\ntoken={token}\ntransaction={self.transaction_id}\n".encode("ascii")
        # The lock itself is an external durable mutation.  Build and exactly
        # serialize its successor journal before changing that external state:
        # a limit-minus-one rejection must leave both witnesses untouched.
        predicted_identity = (initial.st_dev, initial.st_ino, len(replacement))
        predicted_digest = hashlib.sha256(replacement).hexdigest()
        successor = self._detached_record()
        successor["canonicalLock"] = {
            "pid": pid,
            "token": token,
            "identity": f"{predicted_identity[0]:x}:{predicted_identity[1]:x}:{predicted_identity[2]:x}",
            "sha256": predicted_digest,
        }
        self._preflight_detached_successor(
            successor, operation="lock", path=str(lock_path),
            payload_bytes=max(initial.st_size, len(replacement)),
        )
        initial_bytes, initial_identity = _bounded_descriptor_bytes(
            lock_path, maximum=initial.st_size, error="canonical write lock ownership changed")
        try:
            fields = dict(line.split("=", 1) for line in initial_bytes.decode("ascii").splitlines() if "=" in line)
        except (UnicodeDecodeError, ValueError) as exc:
            raise RepositoryError("canonical write lock ownership changed") from exc
        if fields != {"pid": str(pid), "token": token}:
            raise RepositoryError("canonical write lock ownership changed")
        flags = os.O_RDWR
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            descriptor = os.open(lock_path, flags)
        except OSError as exc:
            raise RepositoryError("canonical write lock ownership changed") from exc
        with os.fdopen(descriptor, "r+b") as handle:
            opened_identity = _identity(os.fstat(handle.fileno()))
            if opened_identity != initial_identity or not _path_has_identity(lock_path, opened_identity):
                raise RepositoryError("canonical write lock ownership changed")
            _before_mutation("canonical-lock-bind")
            handle.seek(0); handle.truncate()
            handle.write(replacement)
            handle.flush()
            os.fsync(handle.fileno())
            bound_identity = _identity(os.fstat(handle.fileno()))
            if not _path_has_identity(lock_path, bound_identity):
                raise RepositoryError("canonical write lock ownership changed")
            handle.seek(0)
            digest = hashlib.sha256()
            remaining = len(replacement)
            while remaining:
                chunk = handle.read(min(_BOUNDED_CAPTURE_CHUNK, remaining))
                if not chunk:
                    raise RepositoryError("canonical write lock ownership changed")
                digest.update(chunk); remaining -= len(chunk)
            if handle.read(1) or _identity(os.fstat(handle.fileno())) != bound_identity:
                raise RepositoryError("canonical write lock ownership changed")
            digest_text = digest.hexdigest()
            if not _path_has_identity(lock_path, bound_identity):
                raise RepositoryError("canonical write lock ownership changed")
        if bound_identity != predicted_identity or digest_text != predicted_digest:
            raise RepositoryError("canonical write lock ownership changed")
        self._persist_detached_record(successor)

    def owns_canonical_lock(self, lock_path: Path, *, pid: int, token: str) -> bool:
        value = self.record.get("canonicalLock")
        if not isinstance(value, dict) or value.get("pid") != pid or value.get("token") != token:
            return False
        try:
            status = os.stat(lock_path)
            identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
            if self._is_budgeted:
                # A recovery-side ownership check is still a budgeted lock
                # read.  Never let the old convenience read_bytes path bypass
                # descriptor admission after enrollment.
                self._require_budgeted_operation("lock", path=str(lock_path), payload_bytes=status.st_size)
                payload, opened_identity = _bounded_descriptor_bytes(
                    lock_path, maximum=status.st_size,
                    error="canonical write lock ownership changed",
                )
                return (opened_identity == _identity(status) and identity == value.get("identity")
                        and sha256_bytes(payload) == value.get("sha256"))
            return identity == value.get("identity") and sha256_bytes(lock_path.read_bytes()) == value.get("sha256")
        except OSError:
            return False

    def register_surface(self, path: str, *, before: bytes | None = None, after: bytes | None = None,
                          capture_before: bool = False, max_before_bytes: int | None = None,
                          role: str = "surface") -> None:
        if self.phase != "prepared":
            raise RepositoryError("transaction surface enrollment is frozen")
        if "liveByteBudget" in self.record:
            raise RepositoryError("budgeted transaction journal requires bounded batch enrollment")
        relative = _validate_path(path)
        if role not in _ROLES or any(surface.path.casefold() == relative.casefold() for surface in self.surfaces):
            raise RepositoryError("transaction-owned path is already registered")
        if max_before_bytes is not None and (
            not capture_before
            or isinstance(max_before_bytes, bool)
            or not isinstance(max_before_bytes, int)
            or max_before_bytes < 0
        ):
            raise RepositoryError("invalid bounded before-image capture limit")
        if capture_before and before is not None:
            raise RepositoryError("before image cannot be supplied when capture is requested")
        captured: _BoundedBeforeImage | None = None
        if capture_before:
            if max_before_bytes is None:
                before = _read_file(self.root, relative)
            else:
                captured = _bounded_before_image(self.root, relative, max_before_bytes)
                before = captured.data if captured is not None else None
        before_mode = None
        target = _safe_target(self.root, relative)
        if captured is not None:
            before_mode = captured.mode
        elif before is not None and target.exists():
            before_mode = stat.S_IMODE(os.stat(target).st_mode)
        surface = Surface(relative, before, after, role, before_mode, None,
                          captured.identity if captured is not None else None, max_before_bytes)
        self.record["surfaces"].append(surface.as_json(version=FORMAT_VERSION))
        self._surfaces_cache = None
        try:
            self._persist(
                pre_persist=(
                    (lambda: _bounded_image_is_current(self.root, relative, captured))
                    if captured is not None else None
                )
            )
        except Exception:
            self.record["surfaces"].pop()
            self._surfaces_cache = None
            raise

    def register_surfaces(self, enrollments: Sequence[SurfaceEnrollment], *,
                          live_budget: JournalLiveByteBudget) -> int:
        """Atomically admit surface enrollments under a live-byte cap."""
        try:
            return self._register_surfaces_bounded(enrollments, live_budget=live_budget)
        except (MemoryError, OverflowError) as exc:
            # Capture metadata, preflight lists, proposed Surface objects and
            # candidate serialization are one admission operation.  Never leak
            # allocator/platform exceptions or leave an uncommitted candidate.
            raise RepositoryError("transaction journal live-byte budget exceeded") from exc

    def _register_surfaces_bounded(self, enrollments: Sequence[SurfaceEnrollment], *,
                                   live_budget: JournalLiveByteBudget) -> int:
        """Implementation kept separate so the public boundary catches all allocation paths."""
        if self.phase != "prepared":
            raise RepositoryError("transaction surface enrollment is frozen")
        if not isinstance(live_budget, JournalLiveByteBudget):
            raise RepositoryError("invalid transaction journal live-byte budget")
        if (not isinstance(live_budget.total_bytes, int) or isinstance(live_budget.total_bytes, bool)
                or not isinstance(live_budget.caller_reserve_bytes, int) or isinstance(live_budget.caller_reserve_bytes, bool)
                or live_budget.total_bytes < 0 or live_budget.caller_reserve_bytes < 0
                or live_budget.caller_reserve_bytes > live_budget.total_bytes):
            raise RepositoryError("invalid transaction journal live-byte budget")
        if not isinstance(enrollments, Sequence) or isinstance(enrollments, (str, bytes, bytearray)) or not enrollments:
            raise RepositoryError("invalid transaction journal surface batch")
        # Metadata-only admission is deliberately performed for *every* public
        # path, even an enrollment with supplied images.  It records an exact
        # identity-or-absence token which is rechecked immediately before the
        # one atomic replacement, without allocating a payload image.
        preflight: list[tuple[str, SurfaceEnrollment, _BoundedBeforeMetadata | None]] = []
        raw_surfaces = self.record["surfaces"]
        if not isinstance(raw_surfaces, list):
            raise RepositoryError("invalid WEDL transaction journal")
        # Reject a many-surface, zero-image batch before growing the preflight
        # list/set.  The per-enrollment container model is deliberately paid
        # even when all before/after images are absent.
        try:
            enrollment_count = len(enrollments)
            existing_images, existing_paths = _encoded_surface_image_sizes(raw_surfaces)
            minimum = _projected_live_sizes(
                existing_images, caller_reserve=live_budget.caller_reserve_bytes,
                journal_bytes=len(self._journal_bytes or b""), path_sizes=existing_paths,
                surface_count=len(raw_surfaces) + enrollment_count,
            )
        except (MemoryError, OverflowError, TypeError, ValueError) as exc:
            raise RepositoryError("transaction journal live-byte budget exceeded") from exc
        if minimum > live_budget.total_bytes:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        seen: set[str] = set()
        for existing in raw_surfaces:
            if not isinstance(existing, dict) or not isinstance(existing.get("path"), str):
                raise RepositoryError("invalid WEDL transaction journal surface")
            seen.add(existing["path"].casefold())
        for enrollment in enrollments:
            if not isinstance(enrollment, SurfaceEnrollment):
                raise RepositoryError("invalid transaction journal surface batch")
            relative = _validate_path(enrollment.path)
            if not isinstance(enrollment.role, str):
                raise RepositoryError("invalid transaction journal surface batch")
            if relative.casefold() in seen or enrollment.role not in _ROLES:
                raise RepositoryError("transaction-owned path is already registered")
            if any(value is not None and not isinstance(value, bytes) for value in (enrollment.before, enrollment.after)):
                raise RepositoryError("invalid transaction journal surface batch")
            seen.add(relative.casefold())
            if not isinstance(enrollment.capture_before, bool):
                raise RepositoryError("invalid transaction journal surface batch")
            metadata = _bounded_before_metadata(self.root, relative)
            if enrollment.capture_before:
                if (enrollment.before is not None or enrollment.max_before_bytes is None
                        or isinstance(enrollment.max_before_bytes, bool)
                        or not isinstance(enrollment.max_before_bytes, int)
                        or enrollment.max_before_bytes < 0):
                    raise RepositoryError("invalid transaction journal surface batch")
                if metadata is not None and metadata.size > enrollment.max_before_bytes:
                    raise RepositoryError("transaction-owned before image exceeds capture limit")
            elif enrollment.max_before_bytes is not None:
                raise RepositoryError("invalid transaction journal surface batch")
            preflight.append((relative, enrollment, metadata))
        # Never decode retained base64 or construct a candidate JSON record
        # before the conservative metadata-only cap check succeeds.
        image_sizes, path_sizes = _encoded_surface_image_sizes(raw_surfaces)
        image_sizes.extend(
            size
            for _relative, enrollment, metadata in preflight
            for size in (
                (metadata.size if metadata is not None else None) if enrollment.capture_before else (len(enrollment.before) if enrollment.before is not None else None),
                len(enrollment.after) if enrollment.after is not None else None,
            )
            if size is not None
        )
        path_sizes.extend(sys.getsizeof(relative) for relative, _enrollment, _metadata in preflight)
        requested_reserve = live_budget.caller_reserve_bytes
        existing_budget = self.record.get("liveByteBudget")
        if existing_budget is not None:
            existing_total, existing_reserve, _existing_peak = _live_budget_fields(existing_budget)
            if existing_total != live_budget.total_bytes:
                raise RepositoryError("transaction journal live-byte budget total cannot change")
            requested_reserve = max(existing_reserve, requested_reserve)
        projected = _projected_live_sizes(
            image_sizes, caller_reserve=requested_reserve, journal_bytes=len(self._journal_bytes or b""),
            path_sizes=path_sizes, surface_count=len(raw_surfaces) + len(preflight),
        )
        if projected > live_budget.total_bytes:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        # Every bounded batch uses the complete scalar successor envelope
        # before capture, base64, JSON, or an atomic-write seam.  In particular
        # a later v7 batch must not rely on a payload-only heuristic and then
        # decode/re-encode retained surfaces to discover its parsed-graph peak.
        candidate_payload_upper, candidate_graph_upper = _budgeted_enrollment_successor_upper_bounds(
            self.record, additions=preflight, total=live_budget.total_bytes,
            reserve=requested_reserve)
        candidate_projection = max(_projected_live_ledger(
            image_sizes, caller_reserve=requested_reserve,
            journal_bytes=max(len(self._journal_bytes or b""), candidate_payload_upper),
            path_sizes=path_sizes, surface_count=len(raw_surfaces) + len(preflight),
            json_graph_bytes=candidate_graph_upper,
        ).values())
        _observe_live_bytes("v7-candidate-preflight", candidate_payload=candidate_payload_upper,
                            projected_peak=candidate_projection)
        if candidate_projection > live_budget.total_bytes:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        proposed: list[Surface] = []
        for relative, enrollment, metadata in preflight:
            if metadata is None and enrollment.capture_before:
                proposed.append(Surface(relative, None, enrollment.after, enrollment.role, None, None, None,
                                        enrollment.max_before_bytes))
            elif enrollment.capture_before:
                captured = _bounded_before_image(self.root, relative, enrollment.max_before_bytes, expected=metadata)
                if captured is None:
                    raise RepositoryError("transaction-owned before image is unstable")
                proposed.append(Surface(relative, captured.data, enrollment.after, enrollment.role, captured.mode, None,
                                        captured.identity, enrollment.max_before_bytes))
            else:
                proposed.append(Surface(relative, enrollment.before, enrollment.after, enrollment.role))
        # Construct and persist a detached candidate.  In particular, a
        # MemoryError while copying surfaces, attaching the budget envelope or
        # serializing must leave this journal's record/cache/generation and
        # authenticated on-disk generation byte-for-byte untouched.
        candidate_record = dict(self.record)
        if self.record.get("version") == FORMAT_VERSION:
            # Promoting a legacy v6 journal changes the enclosing wire format,
            # so every retained surface must be rewritten into the exact v7
            # eight-key form rather than leaving a mixed six/eight-key record.
            retained = [Surface.from_json(surface, version=FORMAT_VERSION).as_json(
                version=BUDGETED_FORMAT_VERSION,
            ) for surface in raw_surfaces]
        else:
            # A bounded journal already has validated v7 wire records.  Reuse
            # them verbatim: repeated enrollment must not decode/re-encode each
            # retained base64 image merely to append one new surface.
            retained = list(raw_surfaces)
        candidate_record["surfaces"] = [
            *retained,
            *(surface.as_json(version=BUDGETED_FORMAT_VERSION) for surface in proposed),
        ]
        candidate_record["version"] = BUDGETED_FORMAT_VERSION
        candidate_record["liveByteBudget"] = {
            "totalBytes": live_budget.total_bytes,
            "callerReserveBytes": requested_reserve,
            "admittedPeakBytes": projected,
        }
        candidate_record["$liveByteBudget"] = dict(candidate_record["liveByteBudget"])
        candidate = object.__new__(TransactionJournal)
        candidate.root, candidate.record, candidate.path = self.root, candidate_record, self.path
        candidate._journal_bytes, candidate._journal_identity = self._journal_bytes, self._journal_identity
        candidate._surfaces_cache = None
        try:
            # The payload itself is another simultaneously-live generation.
            # Iterate because persisting the decimal peak changes its JSON size.
            for _ in range(3):
                candidate_payload = _canonical_journal_payload(candidate.record)
                candidate_bytes = len(candidate_payload)
                exact = _projected_live_sizes(
                    image_sizes, caller_reserve=requested_reserve,
                    journal_bytes=max(len(self._journal_bytes or b""), candidate_bytes), path_sizes=path_sizes,
                    surface_count=len(raw_surfaces) + len(preflight),
                    json_graph_bytes=_budgeted_payload_json_graph(candidate_payload),
                )
                if exact > live_budget.total_bytes:
                    raise RepositoryError("transaction journal live-byte budget exceeded")
                if exact == projected:
                    break
                projected = exact
                candidate.record["liveByteBudget"]["admittedPeakBytes"] = projected
                candidate.record["$liveByteBudget"]["admittedPeakBytes"] = projected
            _observe_live_bytes("projected-admission", caller_reserve=requested_reserve, projected_peak=projected)
            # A second metadata-only complete-batch validation runs at the
            # atomic replacement boundary.  No enrollment may be retained if a
            # captured file, a supplied-image destination, or a previously
            # absent name was replaced, appeared, disappeared, or crossed a
            # reparse point after preflight.
            candidate._persist(pre_persist=lambda: all(
                _bounded_metadata_is_current(self.root, relative, metadata)
                for relative, _enrollment, metadata in preflight
            ))
        except Exception:
            # The candidate owns every tentative mutation.  Do not repair this
            # instance piecemeal: its original record object, decoded-surface
            # cache and CAS identity deliberately remain untouched.
            raise
        self.record = candidate.record
        self._journal_bytes, self._journal_identity = candidate._journal_bytes, candidate._journal_identity
        self._surfaces_cache = None
        return projected

    def _detached_record(self) -> dict[str, object]:
        """Copy mutable persistence fields before a later journal update."""
        record = dict(self.record)
        for name in ("liveByteBudget", "$liveByteBudget"):
            value = record.get(name)
            if value is not None:
                if not isinstance(value, dict):
                    raise RepositoryError("invalid WEDL transaction journal live-byte budget")
                record[name] = dict(value)
        return record

    def _persist_detached_record(self, record: dict[str, object]) -> None:
        """Persist a preflighted later-record candidate, then adopt it atomically."""
        candidate = object.__new__(TransactionJournal)
        candidate.root, candidate.record, candidate.path = self.root, record, self.path
        candidate._journal_bytes, candidate._journal_identity = self._journal_bytes, self._journal_identity
        candidate._surfaces_cache = self._surfaces_cache
        candidate._persist()
        self.record = candidate.record
        self._journal_bytes, self._journal_identity = candidate._journal_bytes, candidate._journal_identity
        self._surfaces_cache = candidate._surfaces_cache

    def _preflight_detached_successor(self, record: dict[str, object], *, operation: str,
                                      path: str = "", payload_bytes: int = 0) -> int:
        """Price an exact later-record successor before an external mutation.

        ``_persist`` rightly owns the eventual CAS and durable generation.  A
        caller such as ``bind_lock`` changes a different durable object first,
        though, so it must serialize the next detached record and establish its
        complete ledger before that object can change.  This helper is purely a
        preflight: it never mutates either live record or writes a generation.
        """
        if not self._is_budgeted:
            return 0
        if operation not in {"phase", "lock", "private-artifact", "publish", "restore", "cleanup"}:
            raise RepositoryError("invalid transaction journal live-byte accounting")
        total, reserve, admitted = _live_budget_fields(record.get("liveByteBudget"))
        shadow = dict(record)
        for name in ("liveByteBudget", "$liveByteBudget"):
            value = shadow.get(name)
            if not isinstance(value, dict):
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            shadow[name] = dict(value)
        try:
            shadow["generation"] = int(record["generation"]) + 1
            # This admission intentionally precedes _canonical_journal_payload:
            # a successor that cannot fit must not enter json/base64/atomic
            # allocation seams merely to discover its size.
            upper_payload, upper_graph = _budgeted_successor_upper_bounds(shadow)
            image_sizes, path_sizes = _encoded_surface_image_sizes(shadow["surfaces"])
            upper_journal = max(len(self._journal_bytes or b""), upper_payload)
            upper_ledger = _projected_live_ledger(
                image_sizes, caller_reserve=reserve, journal_bytes=upper_journal,
                path_sizes=path_sizes, operation_path_bytes=sys.getsizeof(path),
                operation_payload_bytes=payload_bytes, surface_count=len(shadow["surfaces"]),
                json_graph_bytes=upper_graph,
            )
            upper_projection = max(upper_ledger.values())
            if upper_projection > total:
                raise RepositoryError("transaction journal live-byte budget exceeded")
            payload = _canonical_journal_payload(shadow)
            for _ in range(3):
                image_sizes, path_sizes = _encoded_surface_image_sizes(shadow["surfaces"])
                journal_bytes = max(len(self._journal_bytes or b""), len(payload))
                graph_bytes = _budgeted_payload_json_graph(payload)
                ledger = _projected_live_ledger(
                    image_sizes, caller_reserve=reserve, journal_bytes=journal_bytes,
                    path_sizes=path_sizes, operation_path_bytes=sys.getsizeof(path),
                    operation_payload_bytes=payload_bytes, surface_count=len(shadow["surfaces"]),
                    json_graph_bytes=graph_bytes,
                )
                projected = max(max(ledger.values()), _projected_live_sizes(
                    image_sizes, caller_reserve=reserve, journal_bytes=journal_bytes,
                    path_sizes=path_sizes, surface_count=len(shadow["surfaces"]), json_graph_bytes=graph_bytes,
                ))
                if projected > total:
                    raise RepositoryError("transaction journal live-byte budget exceeded")
                if projected <= admitted:
                    return projected
                shadow["liveByteBudget"]["admittedPeakBytes"] = projected
                shadow["$liveByteBudget"]["admittedPeakBytes"] = projected
                admitted = projected
                payload = _canonical_journal_payload(shadow)
        except MemoryError as exc:
            raise RepositoryError("transaction journal live-byte budget exceeded") from exc
        # The bounded fixed-point loop must converge; otherwise an unpriced
        # decimal/header expansion would make the successor ambiguous.
        raise RepositoryError("transaction journal live-byte budget exceeded")

    def staging_path(self, name: str) -> Path:
        """Return one private artifact path, never an arbitrary repository path."""
        if not name or Path(name).name != name or name in {".", ".."}:
            raise RepositoryError("invalid transaction staging artifact")
        artifacts = self.record["privateArtifacts"]
        if name not in artifacts:
            self._require_budgeted_operation("private-artifact", path=name)
            if not isinstance(artifacts, list):
                raise RepositoryError("invalid transaction staging artifact")
            record = self._detached_record()
            record["privateArtifacts"] = [*artifacts, name]
            self._persist_detached_record(record)
        private = _journal_directory(self.root, self.transaction_id, create=True)
        staging = _safe_directory(self.root, f".wedl/transactions/{self.transaction_id}/staging", create=True)
        if staging.parent != private:
            raise RepositoryError("invalid transaction staging artifact")
        return staging / name

    def write_staging_artifact(self, name: str, data: bytes) -> Path:
        """Create one producer artifact without trusting a public pathname."""
        if not isinstance(data, bytes):
            raise RepositoryError("invalid transaction staging artifact")
        # Admission is intentionally before staging_path() records a new
        # artifact or creates its directory.  A rejected producer payload must
        # leave neither a namespace entry nor a durable journal generation.
        self._require_budgeted_operation("private-artifact", path=name, payload_bytes=len(data))
        artifact = self.staging_path(name)
        _create_private_file(self.root, self.transaction_id, artifact, data, mutation="staging-artifact-write")
        return artifact

    def seal_staging_artifact(self, name: str) -> dict[str, str]:
        """Durably bind one producer-written private artifact before cleanup."""
        if name not in self.record["privateArtifacts"]:
            raise RepositoryError("unregistered transaction staging artifact")
        artifact = self.staging_path(name)
        try:
            status = os.lstat(artifact)
        except FileNotFoundError as exc:
            raise RepositoryError("transaction staging artifact is not a regular file") from exc
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if artifact.is_symlink() or reparse or not stat.S_ISREG(status.st_mode):
            raise RepositoryError("transaction staging artifact is not a regular file")
        identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
        sealed = self.record.get("sealedArtifacts")
        if not isinstance(sealed, dict):
            raise RepositoryError("transaction staging artifact is unsealed")
        self._require_budgeted_operation("private-artifact", path=str(artifact), payload_bytes=status.st_size)
        digest, opened_identity = _bounded_descriptor_digest(
            artifact, maximum=status.st_size, error="transaction staging artifact changed ownership")
        if opened_identity != _identity(status):
            raise RepositoryError("transaction staging artifact changed ownership")
        value = {"identity": identity, "sha256": digest}
        # A producer can be interrupted or interleaved after the digest read.
        # Bind only the exact inode/content that is still present at the final
        # registration boundary; otherwise retain the journal for retry.
        _before_mutation("staging-artifact-seal")
        final = os.lstat(artifact)
        final_reparse = getattr(final, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        final_identity = f"{final.st_dev:x}:{final.st_ino:x}:{final.st_size:x}"
        final_digest, final_opened_identity = _bounded_descriptor_digest(
            artifact, maximum=status.st_size, error="transaction staging artifact changed ownership")
        if (artifact.is_symlink() or final_reparse or not stat.S_ISREG(final.st_mode)
                or final_identity != identity or final_opened_identity != _identity(final)
                or final_digest != value["sha256"]):
            raise RepositoryError("transaction staging artifact changed ownership")
        if name in sealed and sealed[name] != value:
            raise RepositoryError("transaction staging artifact changed ownership")
        record = self._detached_record()
        candidate_sealed = dict(sealed)
        candidate_sealed[name] = value
        record["sealedArtifacts"] = candidate_sealed
        self._persist_detached_record(record)
        return value

    def bind_index_artifacts(self, *, backup: str, install: str) -> None:
        """Record the sealed private real-index images before public install."""
        sealed = self.record["sealedArtifacts"]
        if backup not in self.record["privateArtifacts"] or install not in self.record["privateArtifacts"]:
            raise RepositoryError("unregistered transaction index artifact")
        if not isinstance(sealed, dict) or backup not in sealed or install not in sealed:
            raise RepositoryError("unsealed transaction index artifact")
        value = {"backup": backup, "install": install}
        existing = self.record.get("indexArtifacts")
        if existing is not None and existing != value:
            raise RepositoryError("transaction index artifact ownership changed")
        record = self._detached_record()
        record["indexArtifacts"] = value
        self._persist_detached_record(record)

    def _backup_path(self, ordinal: int) -> Path:
        if ordinal < 0:
            raise RepositoryError("invalid transaction backup")
        self._require_budgeted_operation("private-artifact", path=f"{ordinal:04d}.before")
        directory = _journal_directory(self.root, self.transaction_id, create=True)
        backups = directory / "backups"
        if not backups.exists():
            _before_mutation("backup-directory-create")
            backups.mkdir()
        status = os.lstat(backups)
        if backups.is_symlink() or getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400):
            raise RepositoryError("WEDL transaction directory crosses a reparse point")
        return backups / f"{ordinal:04d}.before"

    @staticmethod
    def _surface_artifact_key(ordinal: int, kind: str) -> str:
        if ordinal < 0 or kind not in {"before", "after", "published"}:
            raise RepositoryError("invalid transaction surface artifact")
        return f"{ordinal:04d}.{kind}"

    def _uncreated_surface_artifact_path(self, ordinal: int, kind: str) -> Path:
        """Return the canonical private artifact spelling without mkdir side effects."""
        if ordinal < 0 or kind not in {"before", "after", "published"}:
            raise RepositoryError("invalid transaction surface artifact")
        before = self.root / ".wedl" / "transactions" / self.transaction_id / "backups" / f"{ordinal:04d}.before"
        return before if kind == "before" else before.with_suffix(f".{kind}")

    def _surface_artifact_path(self, ordinal: int, kind: str) -> Path:
        before = self._backup_path(ordinal)
        return before if kind == "before" else before.with_suffix(f".{kind}")

    @property
    def _is_budgeted(self) -> bool:
        return "liveByteBudget" in self.record

    def _require_budgeted_operation(self, operation: str, *, path: str = "", payload_bytes: int = 0) -> int:
        """Reject a later bounded operation before it allocates or mutates.

        Enrollment is not the only allocation boundary: phase records, locks,
        private artifact claims, publication, restoration, and cleanup all
        retain the decoded journal state while creating descriptor/path state.
        Re-price those operations before their first namespace mutation.
        """
        if not self._is_budgeted:
            return 0
        if operation not in {"phase", "lock", "private-artifact", "publish", "restore", "cleanup"}:
            raise RepositoryError("invalid transaction journal live-byte accounting")
        total, reserve, _admitted = _live_budget_fields(self.record["liveByteBudget"])
        image_sizes, path_sizes = _encoded_surface_image_sizes(self.record["surfaces"])
        if isinstance(payload_bytes, bool) or not isinstance(payload_bytes, int) or payload_bytes < 0:
            raise RepositoryError("invalid transaction journal live-byte accounting")
        projection = _projected_live_ledger(
            image_sizes,
            caller_reserve=reserve,
            journal_bytes=len(self._journal_bytes or b""),
            path_sizes=path_sizes,
            operation_path_bytes=sys.getsizeof(path),
            operation_payload_bytes=payload_bytes,
            surface_count=len(self.record["surfaces"]),
        )[operation]
        _observe_live_bytes("operation-precheck", operation_projection=projection, operation_path=sys.getsizeof(path))
        if projection > total:
            raise RepositoryError("transaction journal live-byte budget exceeded")
        return projection

    def _matches_surface_image(self, relative: str, expected: bytes | None, mode: int | None, *,
                               identity: tuple[int, int, int] | None = None,
                               limit: int | None = None, maximum: int | None = None) -> bool:
        """Use capped descriptor verification for every budgeted public image."""
        if not self._is_budgeted:
            return _matches_image(self.root, relative, expected, mode, identity, limit)
        cap = maximum if maximum is not None else (limit if limit is not None else (len(expected) if expected is not None else 0))
        return _bounded_public_image_matches(self.root, relative, expected, cap,
                                             identity=identity, mode=mode)

    def _surface_is_missing(self, relative: str) -> bool:
        """Check a budgeted public name without materializing its contents."""
        if not self._is_budgeted:
            return _read_file(self.root, relative) is None
        target = _safe_target(self.root, relative)
        try:
            status = os.lstat(target)
        except FileNotFoundError:
            return True
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if reparse or target.is_symlink() or not stat.S_ISREG(status.st_mode):
            raise RepositoryError("transaction-owned image is unstable")
        return False

    def _matches_surface_artifact(self, artifact: Path, expected: bytes, maximum: int, *,
                                  identity: tuple[int, int, int] | None = None) -> bool:
        if not self._is_budgeted:
            try:
                return (identity is None or _path_has_identity(artifact, identity)) and artifact.read_bytes() == expected
            except OSError:
                return False
        self._revalidate_private_artifact_containment(artifact)
        return _bounded_path_matches_image(artifact, expected, maximum, identity=identity)

    def _seal_surface_artifact(self, ordinal: int, kind: str, artifact: Path, expected: bytes) -> None:
        """Durably bind a private surface claim before public-name removal."""
        try:
            status = os.lstat(artifact)
        except FileNotFoundError as exc:
            raise RepositoryError("transaction-owned backup changed ownership") from exc
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
        maximum = len(expected)
        if (artifact.is_symlink() or reparse or not stat.S_ISREG(status.st_mode)
                or not self._matches_surface_artifact(artifact, expected, maximum, identity=_identity(status))):
            raise RepositoryError("transaction-owned backup changed ownership")
        value = {"identity": identity, "sha256": sha256_bytes(expected)}
        artifacts = self.record["surfaceArtifacts"]
        assert isinstance(artifacts, dict)
        key = self._surface_artifact_key(ordinal, kind)
        existing = artifacts.get(key)
        if existing is not None and existing != value:
            raise RepositoryError("transaction-owned backup changed ownership")
        # Revalidate after the injectable sealing boundary.  This also rejects
        # a same-byte private-path substitution before we make it durable.
        _before_mutation("surface-artifact-seal")
        final = os.lstat(artifact)
        final_identity = f"{final.st_dev:x}:{final.st_ino:x}:{final.st_size:x}"
        final_reparse = getattr(final, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        if (artifact.is_symlink() or final_reparse or not stat.S_ISREG(final.st_mode) or final_identity != identity
                or not self._matches_surface_artifact(artifact, expected, maximum, identity=_identity(final))):
            raise RepositoryError("transaction-owned backup changed ownership")
        if existing is None:
            record = self._detached_record()
            record["surfaceArtifacts"] = {**artifacts, key: value}
            self._persist_detached_record(record)

    def _preflight_surface_artifact_successor(self, ordinal: int, kind: str, artifact: Path, expected: bytes, *,
                                               operation: str, identity: str | None = None) -> None:
        """Admit the sealed-artifact successor before creating its inode.

        Publishing and restoring first create an inode (by hard link or
        O_EXCL write) and only then seal it into the journal.  That external
        step must not happen if the exact next journal generation would exceed
        the persisted live cap.  A linked claim has its exact source identity;
        a new private image uses a deliberately widest native identity spelling
        so the preflight remains conservative until lstat supplies the exact
        value for the durable seal.
        """
        if not self._is_budgeted:
            return
        key = self._surface_artifact_key(ordinal, kind)
        artifacts = self.record.get("surfaceArtifacts")
        if not isinstance(artifacts, dict):
            raise RepositoryError("invalid WEDL transaction journal surface artifact")
        if identity is None:
            # st_dev, st_ino and st_size are non-negative native integers.
            # Three max-width hexadecimal fields price more JSON than any
            # supported stat identity without pretending to know a future inode.
            identity = "ffffffffffffffff:ffffffffffffffff:ffffffffffffffff"
        value = {"identity": identity, "sha256": sha256_bytes(expected)}
        existing = artifacts.get(key)
        if existing is not None:
            if existing != value and identity != "ffffffffffffffff:ffffffffffffffff:ffffffffffffffff":
                raise RepositoryError("transaction-owned backup changed ownership")
            return
        record = self._detached_record()
        record["surfaceArtifacts"] = {**artifacts, key: value}
        self._preflight_detached_successor(
            record, operation=operation, path=str(artifact), payload_bytes=len(expected))

    def _preflight_surface_operation_successor(self, operation: str) -> None:
        """Admit every seal an in-flight publish/restore can require.

        Per-artifact checks prevent an individual external link from preceding
        its seal.  They are insufficient when a later artifact would overflow:
        the earlier seal would already be durable.  Price the conservative
        complete successor before the loop's first directory/link/create.
        """
        if not self._is_budgeted:
            return
        if operation not in {"publish", "restore"}:
            raise RepositoryError("invalid transaction journal live-byte accounting")
        artifacts = self.record.get("surfaceArtifacts")
        if not isinstance(artifacts, dict):
            raise RepositoryError("invalid WEDL transaction journal surface artifact")
        successor = dict(artifacts)
        widest_identity = "ffffffffffffffff:ffffffffffffffff:ffffffffffffffff"
        for ordinal, surface in enumerate(self.surfaces):
            if operation == "publish":
                if self._matches_surface_image(
                        surface.path, surface.after, surface.after_mode,
                        maximum=max(len(surface.after or b""), surface.before_limit or len(surface.before or b""))):
                    continue
                missing = self._surface_is_missing(surface.path)
                claimed_gap = (missing and surface.before is not None
                               and self._surface_artifact_key(ordinal, "before") in artifacts)
                needed = (("before", surface.before) if surface.before is not None and not claimed_gap else None,
                          ("after", surface.after) if surface.after is not None else None)
            else:
                if self._matches_surface_image(
                        surface.path, surface.before, surface.before_mode, identity=surface.before_identity,
                        limit=surface.before_limit,
                        maximum=max(surface.before_limit or len(surface.before or b""), len(surface.after or b""))):
                    continue
                missing = self._surface_is_missing(surface.path)
                claimed_gap = (missing and surface.before is not None
                               and self._surface_artifact_key(ordinal, "before") in artifacts)
                needed = (("before", surface.before) if surface.before is not None else None,
                          ("published", surface.after) if surface.after is not None and not claimed_gap else None)
            for entry in needed:
                if entry is None:
                    continue
                kind, image = entry
                key = self._surface_artifact_key(ordinal, kind)
                if key not in successor:
                    assert image is not None
                    successor[key] = {"identity": widest_identity, "sha256": sha256_bytes(image)}
        if successor == artifacts:
            return
        record = self._detached_record()
        record["surfaceArtifacts"] = successor
        self._preflight_detached_successor(record, operation=operation)

    def _verified_surface_artifact(self, ordinal: int, kind: str, expected: bytes) -> Path:
        artifact = self._surface_artifact_path(ordinal, kind)
        artifacts = self.record["surfaceArtifacts"]
        assert isinstance(artifacts, dict)
        sealed = artifacts.get(self._surface_artifact_key(ordinal, kind))
        try:
            status = os.lstat(artifact)
        except FileNotFoundError as exc:
            raise RepositoryError("transaction-owned backup changed ownership") from exc
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
        if (not isinstance(sealed, dict) or artifact.is_symlink() or reparse or not stat.S_ISREG(status.st_mode)
                or sealed.get("identity") != identity or sealed.get("sha256") != sha256_bytes(expected)
                or not self._matches_surface_artifact(artifact, expected, len(expected), identity=_identity(status))):
            raise RepositoryError("transaction-owned backup changed ownership")
        return artifact

    def _revalidate_private_artifact_containment(self, artifact: Path) -> None:
        """Re-run every private-directory reparse check at final unlink time."""
        private = _journal_directory(self.root, self.transaction_id, create=False)
        try:
            relative = artifact.relative_to(private)
        except ValueError as exc:
            raise RepositoryError("WEDL transaction directory crosses a reparse point") from exc
        if not relative.parts or relative.name != artifact.name:
            raise RepositoryError("WEDL transaction directory crosses a reparse point")
        parent = relative.parent
        if parent != Path("."):
            directory = _safe_directory(self.root, f".wedl/transactions/{self.transaction_id}/{parent.as_posix()}", create=False)
            if directory != artifact.parent:
                raise RepositoryError("WEDL transaction directory crosses a reparse point")
        elif artifact.parent != private:
            raise RepositoryError("WEDL transaction directory crosses a reparse point")

    def _claim_surface_image(self, ordinal: int, kind: str, target: Path, expected: bytes, *, mutation: str,
                               expected_identity: tuple[int, int, int] | None = None,
                               expected_limit: int | None = None,
                               expected_mode: int | None = None, operation: str = "publish") -> Path:
        """Make an inode-bound private claim and seal it before unlinking public."""
        artifact = self._surface_artifact_path(ordinal, kind)
        if artifact.exists():
            # A crash after the private hard-link but before its journal update
            # is safe only while the original public name proves same-inode
            # ownership.  Once the name is gone, an unsealed artifact is never
            # adopted.
            if self._surface_artifact_key(ordinal, kind) not in self.record["surfaceArtifacts"]:
                try:
                    if (_identity(os.stat(target)) != _identity(os.stat(artifact))
                            or not self._matches_surface_image(target.relative_to(self.root).as_posix(), expected,
                                                               expected_mode, identity=expected_identity,
                                                               limit=expected_limit)
                            or (expected_identity is not None and _identity(os.stat(target)) != expected_identity)):
                        raise RepositoryError("transaction-owned backup changed ownership")
                except OSError as exc:
                    raise RepositoryError("transaction-owned backup changed ownership") from exc
            self._seal_surface_artifact(ordinal, kind, artifact, expected)
            return self._verified_surface_artifact(ordinal, kind, expected)
        try:
            if (not target.is_file() or not self._matches_surface_image(target.relative_to(self.root).as_posix(), expected,
                                                                         expected_mode, identity=expected_identity,
                                                                         limit=expected_limit)
                    or (expected_identity is not None and not _path_has_identity(target, expected_identity))):
                raise RepositoryError("transaction recovery lost surface ownership")
            _before_mutation(mutation)
            # Recheck after the exact crash/interleaving hook, then create the
            # private hard link.  The journal is persisted before public unlink.
            source_identity = _identity(os.stat(target))
            if (not _path_has_identity(target, source_identity)
                    or not self._matches_surface_image(target.relative_to(self.root).as_posix(), expected,
                                                        expected_mode, identity=expected_identity,
                                                        limit=expected_limit)
                    or (expected_identity is not None and source_identity != expected_identity)):
                raise RepositoryError("transaction recovery lost surface ownership")
            self._preflight_surface_artifact_successor(
                ordinal, kind, artifact, expected, operation=operation,
                identity=f"{source_identity[0]:x}:{source_identity[1]:x}:{source_identity[2]:x}")
            os.link(target, artifact)
            _fsync_directory(artifact.parent)
        except FileExistsError:
            return self._claim_surface_image(ordinal, kind, target, expected, mutation=mutation,
                                               expected_identity=expected_identity, expected_limit=expected_limit,
                                               expected_mode=expected_mode, operation=operation)
        except OSError as exc:
            raise RepositoryError("transaction recovery lost surface ownership") from exc
        if (_identity(os.stat(target)) != _identity(os.stat(artifact))
                or (expected_identity is not None and _identity(os.stat(target)) != expected_identity)
                or (expected_identity is not None and _identity(os.stat(artifact)) != expected_identity)):
            raise RepositoryError("transaction recovery lost surface ownership")
        self._seal_surface_artifact(ordinal, kind, artifact, expected)
        return self._verified_surface_artifact(ordinal, kind, expected)

    def _remove_claimed_surface_bounded(self, relative: str, target: Path, claim: Path, expected: bytes, *, mutation: str,
                                        expected_mode: int | None = None, maximum: int | None = None,
                                        operation: str = "publish") -> None:
        self._preflight_detached_successor(
            self._detached_record(), operation=operation, path=str(target), payload_bytes=len(expected))
        claimed = _identity(os.stat(claim))
        if (not _path_has_identity(target, claimed)
                or not self._matches_surface_artifact(claim, expected, maximum if maximum is not None else len(expected), identity=claimed)
                or not self._matches_surface_image(relative, expected, expected_mode, identity=claimed, limit=maximum)):
            raise RepositoryError("transaction recovery lost surface ownership")
        _before_mutation(mutation)
        if _safe_target(self.root, relative) != target:
            raise RepositoryError("transaction-owned path escapes repository")
        if (not _path_has_identity(claim, claimed) or not _path_has_identity(target, claimed)
                or not self._matches_surface_artifact(claim, expected, maximum if maximum is not None else len(expected), identity=claimed)
                or not self._matches_surface_image(relative, expected, expected_mode, identity=claimed, limit=maximum)):
            raise RepositoryError("transaction recovery lost surface ownership")
        target.unlink(); _fsync_directory(target.parent)

    def _publish_claimed_surface_bounded(self, relative: str, claim: Path, target: Path, expected: bytes, *, mutation: str,
                                         expected_mode: int | None = None, maximum: int | None = None,
                                         operation: str = "publish") -> None:
        self._preflight_detached_successor(
            self._detached_record(), operation=operation, path=str(target), payload_bytes=len(expected))
        claimed = _identity(os.stat(claim))
        if (target.exists()
                or not self._matches_surface_artifact(claim, expected, maximum if maximum is not None else len(expected), identity=claimed)):
            raise RepositoryError("transaction recovery lost surface ownership")
        target.parent.mkdir(parents=True, exist_ok=True)
        _before_mutation("surface-parent-created")
        if _safe_target(self.root, relative) != target:
            raise RepositoryError("transaction-owned path escapes repository")
        _before_mutation(mutation)
        if _safe_target(self.root, relative) != target:
            raise RepositoryError("transaction-owned path escapes repository")
        if (target.exists() or not _path_has_identity(claim, claimed)
                or not self._matches_surface_artifact(claim, expected, maximum if maximum is not None else len(expected), identity=claimed)):
            raise RepositoryError("transaction recovery lost surface ownership")
        try:
            os.link(claim, target)
        except OSError as exc:
            raise RepositoryError("transaction recovery lost surface ownership") from exc
        _fsync_directory(target.parent)

    @staticmethod
    def _remove_claimed_surface(root: Path, relative: str, target: Path, claim: Path, expected: bytes, *, mutation: str) -> None:
        """Legacy unbudgeted claim removal, retained for existing callers/hooks."""
        claimed = _identity(os.stat(claim))
        if not _path_has_identity(target, claimed) or claim.read_bytes() != expected or target.read_bytes() != expected:
            raise RepositoryError("transaction recovery lost surface ownership")
        _before_mutation(mutation)
        if _safe_target(root, relative) != target:
            raise RepositoryError("transaction-owned path escapes repository")
        if not _path_has_identity(claim, claimed) or not _path_has_identity(target, claimed) or claim.read_bytes() != expected or target.read_bytes() != expected:
            raise RepositoryError("transaction recovery lost surface ownership")
        target.unlink(); _fsync_directory(target.parent)

    @staticmethod
    def _publish_claimed_surface(root: Path, relative: str, claim: Path, target: Path, expected: bytes, *, mutation: str) -> None:
        """Legacy unbudgeted claim publication, retained for existing callers/hooks."""
        claimed = _identity(os.stat(claim))
        if target.exists() or claim.read_bytes() != expected:
            raise RepositoryError("transaction recovery lost surface ownership")
        target.parent.mkdir(parents=True, exist_ok=True)
        _before_mutation("surface-parent-created")
        if _safe_target(root, relative) != target:
            raise RepositoryError("transaction-owned path escapes repository")
        _before_mutation(mutation)
        if _safe_target(root, relative) != target:
            raise RepositoryError("transaction-owned path escapes repository")
        if target.exists() or not _path_has_identity(claim, claimed) or claim.read_bytes() != expected:
            raise RepositoryError("transaction recovery lost surface ownership")
        try:
            os.link(claim, target)
        except OSError as exc:
            raise RepositoryError("transaction recovery lost surface ownership") from exc
        _fsync_directory(target.parent)

    def advance(self, phase: str) -> None:
        current = _PHASES.index(self.phase) if self.phase in _PHASES else -1
        if phase not in _PHASES or _PHASES.index(phase) < current:
            raise RepositoryError("invalid WEDL transaction phase transition")
        self._require_budgeted_operation("phase", path=phase)
        record = self._detached_record()
        record["phase"] = phase
        self._persist_detached_record(record)

    def claim_index_lock(self, *, token: str, identity: str, digest: str) -> None:
        """Durably bind a real-index lock to this journal before replacement."""
        try:
            uuid.UUID(token)
        except ValueError as exc:
            raise RepositoryError("invalid real-index lock token") from exc
        if not identity or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise RepositoryError("invalid real-index lock identity")
        self._require_budgeted_operation("lock", path=".git/index.lock")
        existing = self.record.get("indexLock")
        value = {"token": token, "identity": identity, "sha256": digest}
        if existing is not None and (not isinstance(existing, dict) or existing.get("token") != token):
            raise RepositoryError("canonical write lost real-index lock ownership")
        record = self._detached_record()
        record["indexLock"] = value
        self._persist_detached_record(record)

    def publish_surfaces(self) -> None:
        self._require_budgeted_operation("publish")
        self._preflight_surface_operation_successor("publish")
        for ordinal, surface in enumerate(self.surfaces):
            current_missing = self._surface_is_missing(surface.path)
            if self._matches_surface_image(
                    surface.path, surface.after, surface.after_mode,
                    maximum=max(len(surface.after or b""), surface.before_limit or len(surface.before or b""))):
                continue
            target = _safe_target(self.root, surface.path)
            claimed_gap = current_missing and surface.before is not None and self._surface_artifact_key(ordinal, "before") in self.record["surfaceArtifacts"]
            if not self._matches_surface_image(surface.path, surface.before, surface.before_mode,
                                               identity=surface.before_identity, limit=surface.before_limit) and not claimed_gap:
                raise RepositoryError("transaction recovery lost surface ownership")
            if surface.before is not None and not claimed_gap:
                self._preflight_surface_artifact_successor(
                    ordinal, "before", self._uncreated_surface_artifact_path(ordinal, "before"), surface.before,
                    operation="publish")
            backup = self._backup_path(ordinal)
            if surface.before is not None and not claimed_gap:
                backup = self._claim_surface_image(ordinal, "before", target, surface.before,
                                                    mutation="surface-backup-create",
                                                    expected_identity=surface.before_identity,
                                                    expected_limit=surface.before_limit,
                                                    expected_mode=surface.before_mode, operation="publish")
                if self._is_budgeted:
                    self._remove_claimed_surface_bounded(surface.path, target, backup, surface.before,
                                                         mutation="surface-claim-rename", expected_mode=surface.before_mode,
                                                         maximum=surface.before_limit, operation="publish")
                else:
                    self._remove_claimed_surface(self.root, surface.path, target, backup, surface.before,
                                                 mutation="surface-claim-rename")
            elif claimed_gap:
                backup = self._verified_surface_artifact(ordinal, "before", surface.before)
            if surface.after is None:
                continue
            else:
                after = self._uncreated_surface_artifact_path(ordinal, "after")
                if self._surface_artifact_key(ordinal, "after") not in self.record["surfaceArtifacts"]:
                    if after.exists():
                        raise RepositoryError("transaction-owned backup changed ownership")
                    self._preflight_surface_artifact_successor(
                        ordinal, "after", after, surface.after, operation="publish")
                    after = self._surface_artifact_path(ordinal, "after")
                    _create_private_file(self.root, self.transaction_id, after, surface.after,
                                         mutation="surface-after-create", mode=surface.after_mode)
                    self._seal_surface_artifact(ordinal, "after", after, surface.after)
                after = self._verified_surface_artifact(ordinal, "after", surface.after)
                if self._is_budgeted:
                    self._publish_claimed_surface_bounded(surface.path, after, target, surface.after,
                                                          mutation="surface-publish-create", expected_mode=surface.after_mode,
                                                          operation="publish")
                else:
                    self._publish_claimed_surface(self.root, surface.path, after, target, surface.after,
                                                  mutation="surface-publish-create")

    def restore_surfaces(self) -> None:
        self._require_budgeted_operation("restore")
        self._preflight_surface_operation_successor("restore")
        for ordinal, surface in enumerate(self.surfaces):
            current_missing = self._surface_is_missing(surface.path)
            if self._matches_surface_image(
                    surface.path, surface.before, surface.before_mode, identity=surface.before_identity,
                    limit=surface.before_limit,
                    maximum=max(surface.before_limit or len(surface.before or b""), len(surface.after or b""))):
                continue
            target = _safe_target(self.root, surface.path)
            claimed_gap = current_missing and surface.before is not None and self._surface_artifact_key(ordinal, "before") in self.record["surfaceArtifacts"]
            if not self._matches_surface_image(surface.path, surface.after, surface.after_mode) and not claimed_gap:
                raise RepositoryError("transaction recovery lost surface ownership")
            if surface.before is not None and self._surface_artifact_key(ordinal, "before") not in self.record["surfaceArtifacts"]:
                # A prepared journal may be reconciled after a test/process
                # interruption has installed ``after`` without ever publishing
                # the ordinary before-image.  The journal's authenticated image
                # can seed a sealed private restore artifact while the public
                # name is still present; it is never used to overwrite one.
                backup = self._uncreated_surface_artifact_path(ordinal, "before")
                if backup.exists():
                    raise RepositoryError("transaction-owned backup changed ownership")
                self._preflight_surface_artifact_successor(
                    ordinal, "before", backup, surface.before, operation="restore")
                _create_private_file(self.root, self.transaction_id, backup, surface.before,
                                      mutation="recovery-backup-create", mode=surface.before_mode)
                self._seal_surface_artifact(ordinal, "before", backup, surface.before)
            backup = self._backup_path(ordinal)
            if surface.after is not None and not claimed_gap:
                published = self._claim_surface_image(ordinal, "published", target, surface.after,
                                                       mutation="surface-published-create",
                                                       expected_mode=surface.after_mode, operation="restore")
                if self._is_budgeted:
                    self._remove_claimed_surface_bounded(surface.path, target, published, surface.after,
                                                         mutation="surface-restore-rename", expected_mode=surface.after_mode,
                                                         operation="restore")
                else:
                    self._remove_claimed_surface(self.root, surface.path, target, published, surface.after,
                                                 mutation="surface-restore-rename")
            if surface.before is None:
                continue
            else:
                backup = self._verified_surface_artifact(ordinal, "before", surface.before)
                if self._is_budgeted:
                    self._publish_claimed_surface_bounded(surface.path, backup, target, surface.before,
                                                          mutation="surface-restore-backup-publish",
                                                          expected_mode=surface.before_mode,
                                                          maximum=surface.before_limit, operation="restore")
                else:
                    self._publish_claimed_surface(self.root, surface.path, backup, target, surface.before,
                                                  mutation="surface-restore-backup-publish")
                if not self._matches_surface_image(surface.path, surface.before, surface.before_mode,
                                                   identity=surface.before_identity, limit=surface.before_limit):
                    raise RepositoryError("transaction recovery lost surface ownership")

    def finish(self) -> None:
        if self.phase != "completed":
            self.advance("completed")
        # Repository releases the transaction-bound lock before cleanup. A
        # completed journal therefore remains as ownership evidence across a
        # crash in that narrow window.

    def cleanup(self) -> None:
        if self.phase != "completed":
            raise RepositoryError("cannot clean up an incomplete transaction")
        self._require_budgeted_operation("cleanup")
        directory = _journal_directory(self.root, create=False)
        if self.path.parent != directory:
            raise RepositoryError("invalid WEDL transaction journal path")
        # These are transaction-owned directories.  Remove them only when they
        # are empty; an unrelated cache/receipt beneath .wedl is never touched.
        # The journal is already durably marked completed. Parent directories
        # can contain other journals, so only empty directories are attempted.
        try:
            private: Path | None = _journal_directory(self.root, self.transaction_id, create=False)
        except RepositoryError:
            # An earlier cleanup attempt may already have durably removed every
            # private artifact before crashing at journal cleanup.  The journal
            # is still the final ownership record and must now be removable.
            private = None
        # Artifact cleanup is deliberately first.  A crash before the final
        # journal unlink leaves durable ownership evidence and an idempotent
        # retry, never an orphaned private artifact without its journal.
        for ordinal, _surface in enumerate(self.surfaces):
            if private is None:
                break
            backup = private / "backups" / f"{ordinal:04d}.before"
            try:
                claim = backup.with_name(f".{backup.name}.wedl-unlink-claim")
                if backup.exists() or claim.exists():
                    sealed = self.record["surfaceArtifacts"].get(self._surface_artifact_key(ordinal, "before"))
                    if not isinstance(sealed, dict):
                        raise RepositoryError("transaction-owned backup changed ownership")
                    if not backup.exists():
                        _unlink_inode_claim(backup, _parse_identity(str(sealed["identity"])), str(sealed["sha256"]),
                                            mutation="backup-cleanup-unlink", error="transaction-owned backup changed ownership",
                                            expected=_surface.before or b"", bounded=self._is_budgeted)
                        continue
                    self._verified_surface_artifact(ordinal, "before", _surface.before or b"")
                    _before_mutation("backup-cleanup")
                    # Never unlink after a same-byte private replacement.
                    self._revalidate_private_artifact_containment(backup)
                    self._verified_surface_artifact(ordinal, "before", _surface.before or b"")
                    sealed = self.record["surfaceArtifacts"][self._surface_artifact_key(ordinal, "before")]
                    _unlink_inode_claim(backup, _identity(os.stat(backup)), str(sealed["sha256"]),
                                        mutation="backup-cleanup-unlink", error="transaction-owned backup changed ownership",
                                        expected=_surface.before or b"", bounded=self._is_budgeted)
            except FileNotFoundError:
                pass
            except OSError as exc:
                raise RepositoryError("transaction-owned backup changed ownership") from exc
            published = private / "backups" / f"{ordinal:04d}.published"
            published_claim = published.with_name(f".{published.name}.wedl-unlink-claim")
            if published.exists() or published_claim.exists():
                sealed = self.record["surfaceArtifacts"].get(self._surface_artifact_key(ordinal, "published"))
                if not isinstance(sealed, dict):
                    raise RepositoryError("transaction-owned backup changed ownership")
                if not published.exists():
                    _unlink_inode_claim(published, _parse_identity(str(sealed["identity"])), str(sealed["sha256"]),
                                        mutation="published-cleanup-unlink", error="transaction-owned backup changed ownership",
                                        expected=_surface.after or b"", bounded=self._is_budgeted)
                    continue
                self._verified_surface_artifact(ordinal, "published", _surface.after or b"")
                _before_mutation("published-cleanup")
                self._revalidate_private_artifact_containment(published)
                self._verified_surface_artifact(ordinal, "published", _surface.after or b"")
                sealed = self.record["surfaceArtifacts"][self._surface_artifact_key(ordinal, "published")]
                _unlink_inode_claim(published, _identity(os.stat(published)), str(sealed["sha256"]),
                                    mutation="published-cleanup-unlink", error="transaction-owned backup changed ownership",
                                    expected=_surface.after or b"", bounded=self._is_budgeted)
            after = private / "backups" / f"{ordinal:04d}.after"
            after_claim = after.with_name(f".{after.name}.wedl-unlink-claim")
            if after.exists() or after_claim.exists():
                sealed = self.record["surfaceArtifacts"].get(self._surface_artifact_key(ordinal, "after"))
                if not isinstance(sealed, dict):
                    raise RepositoryError("transaction-owned backup changed ownership")
                if not after.exists():
                    _unlink_inode_claim(after, _parse_identity(str(sealed["identity"])), str(sealed["sha256"]),
                                        mutation="after-cleanup-unlink", error="transaction-owned backup changed ownership",
                                        expected=_surface.after or b"", bounded=self._is_budgeted)
                    continue
                self._verified_surface_artifact(ordinal, "after", _surface.after or b"")
                _before_mutation("after-cleanup")
                self._revalidate_private_artifact_containment(after)
                self._verified_surface_artifact(ordinal, "after", _surface.after or b"")
                sealed = self.record["surfaceArtifacts"][self._surface_artifact_key(ordinal, "after")]
                _unlink_inode_claim(after, _identity(os.stat(after)), str(sealed["sha256"]),
                                    mutation="after-cleanup-unlink", error="transaction-owned backup changed ownership",
                                    expected=_surface.after or b"", bounded=self._is_budgeted)
        staging = private / "staging" if private is not None else None
        for name in self.record["privateArtifacts"]:
            if staging is None:
                break
            sealed_entry = self.record["sealedArtifacts"].get(name)
            if not isinstance(sealed_entry, dict):
                raise RepositoryError("transaction-owned staging artifact is unsealed")
            artifact = staging / name
            try:
                status = os.lstat(artifact)
            except FileNotFoundError:
                claim = artifact.with_name(f".{artifact.name}.wedl-unlink-claim")
                if claim.exists():
                    _unlink_inode_claim(artifact, _parse_identity(str(sealed_entry["identity"])), str(sealed_entry["sha256"]),
                                        mutation="staging-cleanup-unlink", error="transaction-owned staging artifact changed ownership")
                continue
            reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
            if artifact.is_symlink() or reparse or not stat.S_ISREG(status.st_mode):
                raise RepositoryError("transaction-owned staging artifact changed ownership")
            sealed = self.record["sealedArtifacts"].get(name)
            identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
            self._require_budgeted_operation("cleanup", path=str(artifact), payload_bytes=status.st_size)
            digest, opened_identity = _bounded_descriptor_digest(
                artifact, maximum=status.st_size, error="transaction-owned staging artifact changed ownership")
            if (not isinstance(sealed, dict) or sealed.get("identity") != identity
                    or opened_identity != _identity(status) or sealed.get("sha256") != digest):
                raise RepositoryError("transaction-owned staging artifact changed ownership")
            _before_mutation("staging-cleanup")
            # The hook is deliberately the final interleaving point: validate
            # the same inode and digest again immediately before unlinking.
            self._revalidate_private_artifact_containment(artifact)
            try:
                final = os.lstat(artifact)
            except FileNotFoundError as exc:
                raise RepositoryError("transaction-owned staging artifact changed ownership") from exc
            final_identity = f"{final.st_dev:x}:{final.st_ino:x}:{final.st_size:x}"
            final_reparse = getattr(final, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
            final_digest, final_opened_identity = _bounded_descriptor_digest(
                artifact, maximum=status.st_size, error="transaction-owned staging artifact changed ownership")
            if (artifact.is_symlink() or final_reparse or not stat.S_ISREG(final.st_mode)
                    or final_identity != identity or final_opened_identity != _identity(final)
                    or final_digest != sealed["sha256"]):
                raise RepositoryError("transaction-owned staging artifact changed ownership")
            _unlink_inode_claim(artifact, _identity(os.stat(artifact)), str(sealed["sha256"]),
                                mutation="staging-cleanup-unlink", error="transaction-owned staging artifact changed ownership")
        if private is not None:
            for candidate in (private / "backups", private / "staging", private / "lock-claims", private):
                try:
                    candidate.rmdir()
                except OSError:
                    continue
                _fsync_directory(candidate.parent)
            # Never remove the journal while a private entry or claim survives.
            # A later restart will adopt its exact sealed tombstone first.
            if private.exists() and any(private.rglob("*")):
                return
        # The journal is deliberately the last cleanup target.  Verify both its
        # sealed generation and inode after the last injectable checkpoint;
        # a same-byte substitute remains operator-visible rather than erased.
        _before_mutation("journal-cleanup")
        if (self._journal_bytes is None or self._journal_identity is None
                or not _path_has_identity(self.path, self._journal_identity)
                or (not _bounded_path_matches_image(self.path, self._journal_bytes, len(self._journal_bytes), identity=self._journal_identity)
                    if self._is_budgeted else self.path.read_bytes() != self._journal_bytes)):
            raise RepositoryError("transaction journal changed ownership during cleanup")
        _unlink_inode_claim(self.path, self._journal_identity, sha256_bytes(self._journal_bytes),
                            mutation="journal-cleanup-unlink", error="transaction journal changed ownership during cleanup",
                            expected=self._journal_bytes, bounded=self._is_budgeted)
        for candidate in (directory, directory.parent):
            try:
                candidate.rmdir()
            except OSError:
                continue
            _fsync_directory(candidate.parent)

    def _persist(self, *, pre_persist: Callable[[], bool] | None = None) -> None:
        directory = _journal_directory(self.root, create=True)
        if self.path.parent != directory:
            raise RepositoryError("invalid WEDL transaction journal path")
        # Each durable record is a monotonic, compare-and-swap sealed generation.
        # This makes a journal replacement (including identical bytes) fail
        # closed rather than letting a later cleanup delete another writer's log.
        previous_generation = self.record["generation"]
        payload: bytes | None = None
        try:
            if self._is_budgeted:
                total, reserve, _admitted = _live_budget_fields(self.record["liveByteBudget"])
                image_sizes, path_sizes = _encoded_surface_image_sizes(self.record["surfaces"])
                current_projection = _projected_live_sizes(
                    image_sizes, caller_reserve=reserve,
                    journal_bytes=len(self._journal_bytes or b""), path_sizes=path_sizes,
                    surface_count=len(self.record["surfaces"]),
                    json_graph_bytes=_budgeted_payload_json_graph(self._journal_bytes or b""),
                )
                # All later phase/lock/artifact/cleanup persistence passes
                # through here.  Stored admission is evidence only: a mutated
                # in-memory record must still independently fit before it can
                # allocate/replace another durable generation.
                if current_projection > total:
                    raise RepositoryError("transaction journal live-byte budget exceeded")
            self.record["generation"] = int(previous_generation) + 1
            if self._is_budgeted and (self._journal_bytes or b"").startswith(BUDGETED_ENVELOPE):
                # Bound the exact record successor before canonical JSON/base64
                # serialization.  Keep the actual-payload check below as the
                # second, tighter gate immediately before replacement.
                upper_payload, upper_graph = _budgeted_successor_upper_bounds(self.record)
                image_sizes, path_sizes = _encoded_surface_image_sizes(self.record["surfaces"])
                upper_projection = max(_projected_live_ledger(
                    image_sizes, caller_reserve=reserve,
                    journal_bytes=max(len(self._journal_bytes or b""), upper_payload),
                    path_sizes=path_sizes, surface_count=len(self.record["surfaces"]),
                    json_graph_bytes=upper_graph,
                ).values())
                if upper_projection > total:
                    raise RepositoryError("transaction journal live-byte budget exceeded")
            # Serialization is part of the candidate generation.  In
            # particular, a MemoryError here must not leave a phantom in-memory
            # generation that no durable journal ever represented.
            payload = _canonical_journal_payload(self.record)
            if self._is_budgeted:
                # Later phase and artifact records can legitimately make the
                # canonical generation larger than the initial enrollment
                # record.  Reprice the actual candidate before replacement and
                # monotonically refresh its evidence; never let a stale lower
                # admitted peak authorize that generation on restart.
                total, reserve, admitted = _live_budget_fields(self.record["liveByteBudget"])
                for _ in range(3):
                    image_sizes, path_sizes = _encoded_surface_image_sizes(self.record["surfaces"])
                    actual_projection = _projected_live_sizes(
                        image_sizes, caller_reserve=reserve,
                        journal_bytes=max(len(self._journal_bytes or b""), len(payload)), path_sizes=path_sizes,
                        surface_count=len(self.record["surfaces"]),
                        json_graph_bytes=_budgeted_payload_json_graph(payload),
                    )
                    if actual_projection > total:
                        raise RepositoryError("transaction journal live-byte budget exceeded")
                    if actual_projection <= admitted:
                        break
                    self.record["liveByteBudget"]["admittedPeakBytes"] = actual_projection
                    self.record["$liveByteBudget"]["admittedPeakBytes"] = actual_projection
                    admitted = actual_projection
                    payload = _canonical_journal_payload(self.record)
            _before_mutation("journal-persist")
            identity = _atomic_write(self.path, payload, expected=self._journal_bytes,
                                      expected_identity=self._journal_identity,
                                      pre_replace=pre_persist,
                                      bounded_cas=self._is_budgeted,
                                      temporary_prefix=f".wedl-txn-{self.transaction_id}-")
        except MemoryError as exc:
            # A wrapped atomic writer can raise while verifying/fsyncing after
            # the primary replacement.  The installed generation is then the
            # only truthful in-memory state, even though the raised exception
            # is MemoryError rather than an ordinary wrapper failure.
            if self._is_budgeted and payload is not None:
                try:
                    identity = _identity(os.stat(self.path))
                    if _bounded_path_matches_image(self.path, payload, len(payload), identity=identity):
                        self._journal_bytes, self._journal_identity = payload, identity
                        return
                except (OSError, RepositoryError):
                    pass
            self.record["generation"] = previous_generation
            raise RepositoryError("transaction journal live-byte budget exceeded") from exc
        except Exception as exc:
            # An implementation/hook failure after the atomic primary
            # replacement must not roll the in-memory generation back while
            # its exact candidate is already durable.  Adopt that generation
            # if (and only if) it is still the authenticated installed image.
            if self._is_budgeted and payload is not None:
                try:
                    identity = _identity(os.stat(self.path))
                    if _bounded_path_matches_image(self.path, payload, len(payload), identity=identity):
                        self._journal_bytes, self._journal_identity = payload, identity
                        return
                except (OSError, RepositoryError):
                    pass
            self.record["generation"] = previous_generation
            if self._is_budgeted and isinstance(exc, (OverflowError, TypeError, ValueError)):
                raise RepositoryError("transaction journal live-byte budget exceeded") from exc
            raise
        self._journal_bytes, self._journal_identity = payload, identity

    def _validate_index(self, entries: object) -> None:
        if not isinstance(entries, dict):
            raise RepositoryError("invalid WEDL transaction journal")
        for path, entry in entries.items():
            if not isinstance(path, str) or _validate_path(path) != path:
                raise RepositoryError("invalid WEDL transaction journal")
            if entry is None:
                continue
            if not isinstance(entry, dict) or set(entry) != {"mode", "stage", "blob"} or entry.get("stage") != 0:
                raise RepositoryError("invalid WEDL transaction journal index entry")
            if not isinstance(entry.get("mode"), str) or not _MODE.fullmatch(entry["mode"]) or not isinstance(entry.get("blob"), str) or not _SHA.fullmatch(entry["blob"]):
                raise RepositoryError("invalid WEDL transaction journal index entry")

    def _validate_persisted_live_budget(self, journal_bytes: int) -> None:
        value = self.record.get("liveByteBudget")
        if value is None:
            return
        total, reserve, admitted = _live_budget_fields(value)
        recomputed = _projected_live_bytes(self.surfaces, caller_reserve=reserve, journal_bytes=journal_bytes)
        if recomputed > total or admitted < recomputed:
            raise RepositoryError("transaction journal live-byte budget exceeded")

    def _validate(self) -> None:
        required = {"version", "generation", "id", "phase", "ref", "previousHead", "committedHead", "surfaces", "indexBefore", "indexAfter", "indexLock", "indexArtifacts", "canonicalLock", "privateArtifacts", "sealedArtifacts", "surfaceArtifacts"}
        budgeted = required | {"liveByteBudget", "$liveByteBudget"}
        legacy = set(self.record) == required and self.record.get("version") == FORMAT_VERSION
        bounded = set(self.record) == budgeted and self.record.get("version") == BUDGETED_FORMAT_VERSION
        if not (legacy or bounded) or self.record.get("phase") not in _PHASES:
            raise RepositoryError("unsupported WEDL transaction journal")
        # This duplicated, early envelope is the downgrade guard.  Validate it
        # before calling ``self.surfaces`` so malformed/reduced metadata cannot
        # authorize a base64 decode or Surface allocation.
        if bounded:
            if self.record["$liveByteBudget"] != self.record["liveByteBudget"]:
                raise RepositoryError("invalid WEDL transaction journal live-byte budget")
            _live_budget_fields(self.record["liveByteBudget"])
        try:
            uuid.UUID(str(self.record["id"]))
        except ValueError as exc:
            raise RepositoryError("invalid WEDL transaction journal") from exc
        if not isinstance(self.record["generation"], int) or self.record["generation"] < 0:
            raise RepositoryError("invalid WEDL transaction journal generation")
        ref = self.record["ref"]
        if not isinstance(ref, str) or not ref.startswith("refs/") or ".." in ref or any(char.isspace() for char in ref):
            raise RepositoryError("invalid WEDL transaction journal")
        for name in ("previousHead", "committedHead"):
            if not isinstance(self.record[name], str) or not _SHA.fullmatch(str(self.record[name])):
                raise RepositoryError("invalid WEDL transaction journal")
        if not isinstance(self.record["surfaces"], list):
            raise RepositoryError("invalid WEDL transaction journal")
        paths = [surface.path for surface in self.surfaces]
        if len(paths) != len(set(paths)) or len({path.casefold() for path in paths}) != len(paths):
            raise RepositoryError("invalid WEDL transaction journal")
        for surface in self.surfaces:
            if _validate_path(surface.path) != surface.path or surface.role not in _ROLES:
                raise RepositoryError("invalid WEDL transaction journal")
        self._validate_index(self.record["indexBefore"]); self._validate_index(self.record["indexAfter"])
        index_lock = self.record["indexLock"]
        if index_lock is not None and (not isinstance(index_lock, dict) or set(index_lock) != {"token", "identity", "sha256"} or not isinstance(index_lock["token"], str) or not isinstance(index_lock["identity"], str) or not isinstance(index_lock["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", index_lock["sha256"])):
            raise RepositoryError("invalid WEDL transaction journal real-index lock")
        canonical_lock = self.record["canonicalLock"]
        if canonical_lock is not None and (not isinstance(canonical_lock, dict) or set(canonical_lock) != {"pid", "token", "identity", "sha256"} or not isinstance(canonical_lock["pid"], int) or not isinstance(canonical_lock["token"], str) or not isinstance(canonical_lock["identity"], str) or not isinstance(canonical_lock["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", canonical_lock["sha256"])):
            raise RepositoryError("invalid WEDL transaction journal canonical lock")
        if not isinstance(self.record["privateArtifacts"], list) or any(not isinstance(name, str) or Path(name).name != name for name in self.record["privateArtifacts"]):
            raise RepositoryError("invalid WEDL transaction journal private artifact")
        sealed = self.record["sealedArtifacts"]
        if not isinstance(sealed, dict) or any(name not in self.record["privateArtifacts"] or not isinstance(value, dict) or set(value) != {"identity", "sha256"} or not isinstance(value["identity"], str) or not isinstance(value["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", value["sha256"]) for name, value in sealed.items()):
            raise RepositoryError("invalid WEDL transaction journal sealed artifact")
        surface_artifacts = self.record["surfaceArtifacts"]
        if not isinstance(surface_artifacts, dict):
            raise RepositoryError("invalid WEDL transaction journal surface artifact")
        for key, value in surface_artifacts.items():
            match = re.fullmatch(r"(\d{4})\.(before|after|published)", key) if isinstance(key, str) else None
            if match is None or int(match.group(1)) >= len(self.surfaces) or not isinstance(value, dict) or set(value) != {"identity", "sha256"} or not isinstance(value.get("identity"), str) or not isinstance(value.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", value["sha256"]):
                raise RepositoryError("invalid WEDL transaction journal surface artifact")
            surface = self.surfaces[int(match.group(1))]
            expected = surface.before if match.group(2) == "before" else surface.after
            if expected is None or value["sha256"] != sha256_bytes(expected):
                raise RepositoryError("invalid WEDL transaction journal surface artifact")
        index_artifacts = self.record["indexArtifacts"]
        if index_artifacts is not None and (not isinstance(index_artifacts, dict) or set(index_artifacts) != {"backup", "install"} or any(not isinstance(index_artifacts[key], str) or index_artifacts[key] not in self.record["privateArtifacts"] or index_artifacts[key] not in sealed for key in ("backup", "install"))):
            raise RepositoryError("invalid WEDL transaction journal index artifact")
