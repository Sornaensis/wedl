"""Durable, ownership-checked records for interrupted authoring writes."""

from __future__ import annotations

from dataclasses import dataclass
import base64
import ctypes
import json
import os
from pathlib import Path
import re
import stat
import tempfile
import uuid
from typing import Callable

from .errors import RepositoryError
from .util import sha256_bytes

FORMAT_VERSION = 6
_PHASES = ("prepared", "ref_committed", "sources_published", "index_published", "surfaces_published", "completed")
_SHA = re.compile(r"^[0-9a-f]{40}$")
_MODE = re.compile(r"^[0-7]{6}$")
_ROLES = frozenset({"source", "cache", "receipt", "private", "staging", "surface"})
_mutation_hook: Callable[[str], None] | None = None


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

    def as_json(self) -> dict[str, object]:
        return {"path": self.path, "before": _encode(self.before), "after": _encode(self.after), "role": self.role,
                "beforeMode": self.before_mode, "afterMode": self.after_mode}

    @classmethod
    def from_json(cls, value: object) -> "Surface":
        if not isinstance(value, dict) or not isinstance(value.get("path"), str) or not isinstance(value.get("role", "surface"), str):
            raise RepositoryError("invalid WEDL transaction journal surface")
        before_mode, after_mode = value.get("beforeMode"), value.get("afterMode")
        if before_mode is not None and (not isinstance(before_mode, int) or before_mode < 0 or before_mode > 0o777):
            raise RepositoryError("invalid WEDL transaction journal mode")
        if after_mode is not None and (not isinstance(after_mode, int) or after_mode < 0 or after_mode > 0o777):
            raise RepositoryError("invalid WEDL transaction journal mode")
        return cls(str(value["path"]), _decode(value.get("before")), _decode(value.get("after")), str(value.get("role", "surface")), before_mode, after_mode)


def _encode(data: bytes | None) -> dict[str, str] | None:
    if data is None:
        return None
    return {"sha256": sha256_bytes(data), "base64": base64.b64encode(data).decode("ascii")}


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
    return data


def _validate_path(path: str) -> str:
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


def _matches_image(root: Path, relative: str, data: bytes | None, mode: int | None) -> bool:
    actual = _read_file(root, relative)
    if actual != data:
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


def _unlink_inode_claim(path: Path, identity: tuple[int, int, int], digest: str, *, mutation: str, error: str) -> None:
    """Remove only the inode we just hard-link claimed from a private path.

    The claim both makes a durable same-inode witness for the final check and
    lets a restarted cleanup distinguish an inherited interrupted claim from a
    substituted private file.  The claim is deliberately adjacent to the
    target, never a broad temporary-directory cleanup target.
    """
    claim = path.with_name(f".{path.name}.wedl-unlink-claim")

    def verified(candidate: Path) -> bool:
        try:
            return (_path_has_identity(candidate, identity)
                    and sha256_bytes(candidate.read_bytes()) == digest)
        except OSError:
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
                  expected_identity: tuple[int, int, int] | None = None) -> tuple[int, int, int]:
    """Atomically install bytes, with a final ownership check at replacement."""
    # Parents must not resolve through a symlink/junction; create them one at a
    # time after the caller has validated the repository-relative target.
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".wedl-txn-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data); output.flush(); os.fsync(output.fileno())
        if expected is not ...:
            current = path.read_bytes() if path.exists() else None
            if current != expected:
                raise RepositoryError("transaction recovery lost surface ownership")
            if expected is not None and expected_identity is not None and not _path_has_identity(path, expected_identity):
                raise RepositoryError("transaction recovery lost surface ownership")
        _before_mutation("atomic-replace")
        # Failure injection and an interleaving writer run at this exact final
        # boundary.  Recheck the recorded generation image after the hook, not
        # just before it, so an identical-byte replacement is not accepted.
        if expected is not ...:
            current = path.read_bytes() if path.exists() else None
            if current != expected or (expected is not None and expected_identity is not None and not _path_has_identity(path, expected_identity)):
                raise RepositoryError("transaction recovery lost surface ownership")
        os.replace(temporary, path)
        _fsync_directory(path.parent)
        if path.read_bytes() != data:
            raise RepositoryError("transaction recovery lost surface ownership")
        return _identity(os.stat(path))
    finally:
        Path(temporary).unlink(missing_ok=True)


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


def _lock_fields(path: Path) -> dict[str, str] | None:
    try:
        fields = dict(line.split("=", 1) for line in path.read_text(encoding="ascii").splitlines() if "=" in line)
    except OSError:
        return None
    return fields or None


class TransactionJournal:
    """Versioned journal whose mutations are individually ownership checked."""

    def __init__(self, root: Path, record: dict[str, object], journal_path: Path) -> None:
        self.root, self.record, self.path = root, record, journal_path
        self._journal_bytes: bytes | None = None
        self._journal_identity: tuple[int, int, int] | None = None
        self._validate()

    @property
    def transaction_id(self) -> str:
        return str(self.record["id"])

    @property
    def phase(self) -> str:
        return str(self.record["phase"])

    @property
    def surfaces(self) -> tuple[Surface, ...]:
        return tuple(Surface.from_json(value) for value in self.record["surfaces"])

    @classmethod
    def create(cls, root: Path, *, ref: str, previous_head: str, committed_head: str,
               surfaces: list[Surface] | tuple[Surface, ...] = (),
               index_before: dict[str, dict[str, object] | None] | None = None,
               index_after: dict[str, dict[str, object] | None] | None = None) -> "TransactionJournal":
        transaction_id = str(uuid.uuid4())
        record: dict[str, object] = {"version": FORMAT_VERSION, "generation": 0, "id": transaction_id, "phase": "prepared", "ref": ref,
            "previousHead": previous_head, "committedHead": committed_head, "surfaces": [surface.as_json() for surface in surfaces],
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
            payload = path.read_bytes()
            identity = _identity(os.stat(path))
            record = json.loads(payload.decode("utf-8"))
        except (OSError, ValueError) as exc:
            raise RepositoryError("invalid WEDL transaction journal") from exc
        if not isinstance(record, dict):
            raise RepositoryError("invalid WEDL transaction journal")
        journal = cls(root, record, path)
        # Do not trust a parsed image alone: every later generation update is a
        # CAS from this exact inode and canonical JSON bytes.
        journal._journal_bytes, journal._journal_identity = payload, identity
        if not _path_has_identity(path, identity) or path.read_bytes() != payload:
            raise RepositoryError("invalid WEDL transaction journal")
        return journal

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
                os.link(claim, journal_path)
                _fsync_directory(directory)
            except OSError:
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
        return journal.transaction_id == fields["transaction"] and journal.owns_canonical_lock(lock_path, pid=pid, token=fields["token"])

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
        claim_dir = _journal_directory(root, fields["transaction"], create=True) / "lock-claims"
        if not claim_dir.exists():
            _before_mutation("lock-claim-directory-create")
            claim_dir.mkdir()
        claim = claim_dir / f"{fields['token']}.claim"
        try:
            _before_mutation("stale-lock-claim")
            os.link(lock_path, claim)
        except (FileExistsError, OSError):
            return False
        try:
            claimed = os.stat(claim)
            current = os.stat(lock_path)
            if (claimed.st_dev, claimed.st_ino, claimed.st_size) != (current.st_dev, current.st_ino, current.st_size):
                return False
            identity = f"{claimed.st_dev:x}:{claimed.st_ino:x}:{claimed.st_size:x}"
            if identity != expected_identity or sha256_bytes(claim.read_bytes()) != expected_digest or sha256_bytes(lock_path.read_bytes()) != expected_digest or _lock_fields(lock_path) != fields:
                return False
            _before_mutation("stale-lock-reclaim")
            # Recheck the name immediately before unlinking; no broad stale-lock
            # deletion is permitted, and failure leaves both artifacts intact.
            current = os.stat(lock_path)
            current_identity = f"{current.st_dev:x}:{current.st_ino:x}:{current.st_size:x}"
            claim_identity = f"{claimed.st_dev:x}:{claimed.st_ino:x}:{claimed.st_size:x}"
            if (claimed.st_dev, claimed.st_ino, claimed.st_size) != (current.st_dev, current.st_ino, current.st_size) or current_identity != expected_identity or claim_identity != expected_identity or sha256_bytes(claim.read_bytes()) != expected_digest or sha256_bytes(lock_path.read_bytes()) != expected_digest or _lock_fields(lock_path) != fields:
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
        flags = os.O_RDWR
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            descriptor = os.open(lock_path, flags)
        except OSError as exc:
            raise RepositoryError("canonical write lock ownership changed") from exc
        with os.fdopen(descriptor, "r+b") as handle:
            opened_identity = _identity(os.fstat(handle.fileno()))
            if not _path_has_identity(lock_path, opened_identity):
                raise RepositoryError("canonical write lock ownership changed")
            handle.seek(0)
            fields = dict(line.split("=", 1) for line in handle.read().decode("ascii").splitlines() if "=" in line)
            if fields != {"pid": str(pid), "token": token}:
                raise RepositoryError("canonical write lock ownership changed")
            _before_mutation("canonical-lock-bind")
            handle.seek(0); handle.truncate()
            handle.write(f"pid={pid}\ntoken={token}\ntransaction={self.transaction_id}\n".encode("ascii"))
            handle.flush()
            os.fsync(handle.fileno())
            bound_identity = _identity(os.fstat(handle.fileno()))
            if not _path_has_identity(lock_path, bound_identity):
                raise RepositoryError("canonical write lock ownership changed")
            digest = sha256_bytes(lock_path.read_bytes())
            if not _path_has_identity(lock_path, bound_identity):
                raise RepositoryError("canonical write lock ownership changed")
        self.record["canonicalLock"] = {"pid": pid, "token": token, "identity": f"{bound_identity[0]:x}:{bound_identity[1]:x}:{bound_identity[2]:x}", "sha256": digest}
        self._persist()

    def owns_canonical_lock(self, lock_path: Path, *, pid: int, token: str) -> bool:
        value = self.record.get("canonicalLock")
        if not isinstance(value, dict) or value.get("pid") != pid or value.get("token") != token:
            return False
        try:
            status = os.stat(lock_path)
            identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
            return identity == value.get("identity") and sha256_bytes(lock_path.read_bytes()) == value.get("sha256")
        except OSError:
            return False

    def register_surface(self, path: str, *, before: bytes | None = None, after: bytes | None = None,
                         capture_before: bool = False, role: str = "surface") -> None:
        if self.phase != "prepared":
            raise RepositoryError("transaction surface enrollment is frozen")
        relative = _validate_path(path)
        if role not in _ROLES or any(surface.path.casefold() == relative.casefold() for surface in self.surfaces):
            raise RepositoryError("transaction-owned path is already registered")
        if capture_before:
            before = _read_file(self.root, relative)
        before_mode = None
        target = _safe_target(self.root, relative)
        if before is not None and target.exists():
            before_mode = stat.S_IMODE(os.stat(target).st_mode)
        self.record["surfaces"].append(Surface(relative, before, after, role, before_mode, None).as_json())
        self._persist()

    def staging_path(self, name: str) -> Path:
        """Return one private artifact path, never an arbitrary repository path."""
        if not name or Path(name).name != name or name in {".", ".."}:
            raise RepositoryError("invalid transaction staging artifact")
        artifacts = self.record["privateArtifacts"]
        if name not in artifacts:
            artifacts.append(name); self._persist()
        private = _journal_directory(self.root, self.transaction_id, create=True)
        staging = _safe_directory(self.root, f".wedl/transactions/{self.transaction_id}/staging", create=True)
        if staging.parent != private:
            raise RepositoryError("invalid transaction staging artifact")
        return staging / name

    def write_staging_artifact(self, name: str, data: bytes) -> Path:
        """Create one producer artifact without trusting a public pathname."""
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
        sealed = self.record.setdefault("sealedArtifacts", {})
        value = {"identity": identity, "sha256": sha256_bytes(artifact.read_bytes())}
        # A producer can be interrupted or interleaved after the digest read.
        # Bind only the exact inode/content that is still present at the final
        # registration boundary; otherwise retain the journal for retry.
        _before_mutation("staging-artifact-seal")
        final = os.lstat(artifact)
        final_reparse = getattr(final, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        final_identity = f"{final.st_dev:x}:{final.st_ino:x}:{final.st_size:x}"
        if artifact.is_symlink() or final_reparse or not stat.S_ISREG(final.st_mode) or final_identity != identity or sha256_bytes(artifact.read_bytes()) != value["sha256"]:
            raise RepositoryError("transaction staging artifact changed ownership")
        if name in sealed and sealed[name] != value:
            raise RepositoryError("transaction staging artifact changed ownership")
        sealed[name] = value
        self._persist()
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
        self.record["indexArtifacts"] = value
        self._persist()

    def _backup_path(self, ordinal: int) -> Path:
        if ordinal < 0:
            raise RepositoryError("invalid transaction backup")
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

    def _surface_artifact_path(self, ordinal: int, kind: str) -> Path:
        before = self._backup_path(ordinal)
        return before if kind == "before" else before.with_suffix(f".{kind}")

    def _seal_surface_artifact(self, ordinal: int, kind: str, artifact: Path, expected: bytes) -> None:
        """Durably bind a private surface claim before public-name removal."""
        try:
            status = os.lstat(artifact)
        except FileNotFoundError as exc:
            raise RepositoryError("transaction-owned backup changed ownership") from exc
        reparse = getattr(status, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
        identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
        if artifact.is_symlink() or reparse or not stat.S_ISREG(status.st_mode) or sha256_bytes(artifact.read_bytes()) != sha256_bytes(expected):
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
        if artifact.is_symlink() or final_reparse or not stat.S_ISREG(final.st_mode) or final_identity != identity or sha256_bytes(artifact.read_bytes()) != value["sha256"]:
            raise RepositoryError("transaction-owned backup changed ownership")
        if existing is None:
            artifacts[key] = value
            self._persist()

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
                or sha256_bytes(artifact.read_bytes()) != sha256_bytes(expected)):
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

    def _claim_surface_image(self, ordinal: int, kind: str, target: Path, expected: bytes, *, mutation: str) -> Path:
        """Make an inode-bound private claim and seal it before unlinking public."""
        artifact = self._surface_artifact_path(ordinal, kind)
        if artifact.exists():
            # A crash after the private hard-link but before its journal update
            # is safe only while the original public name proves same-inode
            # ownership.  Once the name is gone, an unsealed artifact is never
            # adopted.
            if self._surface_artifact_key(ordinal, kind) not in self.record["surfaceArtifacts"]:
                try:
                    if _identity(os.stat(target)) != _identity(os.stat(artifact)) or target.read_bytes() != expected:
                        raise RepositoryError("transaction-owned backup changed ownership")
                except OSError as exc:
                    raise RepositoryError("transaction-owned backup changed ownership") from exc
            self._seal_surface_artifact(ordinal, kind, artifact, expected)
            return self._verified_surface_artifact(ordinal, kind, expected)
        try:
            if not target.is_file() or target.read_bytes() != expected:
                raise RepositoryError("transaction recovery lost surface ownership")
            _before_mutation(mutation)
            # Recheck after the exact crash/interleaving hook, then create the
            # private hard link.  The journal is persisted before public unlink.
            source_identity = _identity(os.stat(target))
            if not _path_has_identity(target, source_identity) or target.read_bytes() != expected:
                raise RepositoryError("transaction recovery lost surface ownership")
            os.link(target, artifact)
            _fsync_directory(artifact.parent)
        except FileExistsError:
            return self._claim_surface_image(ordinal, kind, target, expected, mutation=mutation)
        except OSError as exc:
            raise RepositoryError("transaction recovery lost surface ownership") from exc
        if _identity(os.stat(target)) != _identity(os.stat(artifact)):
            raise RepositoryError("transaction recovery lost surface ownership")
        self._seal_surface_artifact(ordinal, kind, artifact, expected)
        return self._verified_surface_artifact(ordinal, kind, expected)

    @staticmethod
    def _remove_claimed_surface(root: Path, relative: str, target: Path, claim: Path, expected: bytes, *, mutation: str) -> None:
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
        self.record["phase"] = phase; self._persist()

    def claim_index_lock(self, *, token: str, identity: str, digest: str) -> None:
        """Durably bind a real-index lock to this journal before replacement."""
        try:
            uuid.UUID(token)
        except ValueError as exc:
            raise RepositoryError("invalid real-index lock token") from exc
        if not identity or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise RepositoryError("invalid real-index lock identity")
        existing = self.record.get("indexLock")
        value = {"token": token, "identity": identity, "sha256": digest}
        if existing is not None and (not isinstance(existing, dict) or existing.get("token") != token):
            raise RepositoryError("canonical write lost real-index lock ownership")
        self.record["indexLock"] = value
        self._persist()

    def publish_surfaces(self) -> None:
        for ordinal, surface in enumerate(self.surfaces):
            current = _read_file(self.root, surface.path)
            if _matches_image(self.root, surface.path, surface.after, surface.after_mode):
                continue
            target = _safe_target(self.root, surface.path)
            backup = self._backup_path(ordinal)
            claimed_gap = current is None and surface.before is not None and self._surface_artifact_key(ordinal, "before") in self.record["surfaceArtifacts"]
            if not _matches_image(self.root, surface.path, surface.before, surface.before_mode) and not claimed_gap:
                raise RepositoryError("transaction recovery lost surface ownership")
            if surface.before is not None and not claimed_gap:
                backup = self._claim_surface_image(ordinal, "before", target, surface.before,
                                                    mutation="surface-backup-create")
                self._remove_claimed_surface(self.root, surface.path, target, backup, surface.before, mutation="surface-claim-rename")
            elif claimed_gap:
                backup = self._verified_surface_artifact(ordinal, "before", surface.before)
            if surface.after is None:
                continue
            else:
                after = self._surface_artifact_path(ordinal, "after")
                if self._surface_artifact_key(ordinal, "after") not in self.record["surfaceArtifacts"]:
                    if after.exists():
                        raise RepositoryError("transaction-owned backup changed ownership")
                    _create_private_file(self.root, self.transaction_id, after, surface.after,
                                         mutation="surface-after-create", mode=surface.after_mode)
                    self._seal_surface_artifact(ordinal, "after", after, surface.after)
                after = self._verified_surface_artifact(ordinal, "after", surface.after)
                self._publish_claimed_surface(self.root, surface.path, after, target, surface.after,
                                              mutation="surface-publish-create")

    def restore_surfaces(self) -> None:
        for ordinal, surface in enumerate(self.surfaces):
            current = _read_file(self.root, surface.path)
            if _matches_image(self.root, surface.path, surface.before, surface.before_mode):
                continue
            target = _safe_target(self.root, surface.path)
            backup = self._backup_path(ordinal)
            claimed_gap = current is None and surface.before is not None and self._surface_artifact_key(ordinal, "before") in self.record["surfaceArtifacts"]
            if not _matches_image(self.root, surface.path, surface.after, surface.after_mode) and not claimed_gap:
                raise RepositoryError("transaction recovery lost surface ownership")
            if surface.before is not None and self._surface_artifact_key(ordinal, "before") not in self.record["surfaceArtifacts"]:
                # A prepared journal may be reconciled after a test/process
                # interruption has installed ``after`` without ever publishing
                # the ordinary before-image.  The journal's authenticated image
                # can seed a sealed private restore artifact while the public
                # name is still present; it is never used to overwrite one.
                if backup.exists():
                    raise RepositoryError("transaction-owned backup changed ownership")
                _create_private_file(self.root, self.transaction_id, backup, surface.before,
                                     mutation="recovery-backup-create", mode=surface.before_mode)
                self._seal_surface_artifact(ordinal, "before", backup, surface.before)
            if surface.after is not None and not claimed_gap:
                published = self._claim_surface_image(ordinal, "published", target, surface.after,
                                                       mutation="surface-published-create")
                self._remove_claimed_surface(self.root, surface.path, target, published, surface.after, mutation="surface-restore-rename")
            if surface.before is None:
                continue
            else:
                backup = self._verified_surface_artifact(ordinal, "before", surface.before)
                self._publish_claimed_surface(self.root, surface.path, backup, target, surface.before, mutation="surface-restore-backup-publish")
                if not _matches_image(self.root, surface.path, surface.before, surface.before_mode):
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
                                            mutation="backup-cleanup-unlink", error="transaction-owned backup changed ownership")
                        continue
                    self._verified_surface_artifact(ordinal, "before", _surface.before or b"")
                    _before_mutation("backup-cleanup")
                    # Never unlink after a same-byte private replacement.
                    self._revalidate_private_artifact_containment(backup)
                    self._verified_surface_artifact(ordinal, "before", _surface.before or b"")
                    sealed = self.record["surfaceArtifacts"][self._surface_artifact_key(ordinal, "before")]
                    _unlink_inode_claim(backup, _identity(os.stat(backup)), str(sealed["sha256"]),
                                        mutation="backup-cleanup-unlink", error="transaction-owned backup changed ownership")
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
                                        mutation="published-cleanup-unlink", error="transaction-owned backup changed ownership")
                    continue
                self._verified_surface_artifact(ordinal, "published", _surface.after or b"")
                _before_mutation("published-cleanup")
                self._revalidate_private_artifact_containment(published)
                self._verified_surface_artifact(ordinal, "published", _surface.after or b"")
                sealed = self.record["surfaceArtifacts"][self._surface_artifact_key(ordinal, "published")]
                _unlink_inode_claim(published, _identity(os.stat(published)), str(sealed["sha256"]),
                                    mutation="published-cleanup-unlink", error="transaction-owned backup changed ownership")
            after = private / "backups" / f"{ordinal:04d}.after"
            after_claim = after.with_name(f".{after.name}.wedl-unlink-claim")
            if after.exists() or after_claim.exists():
                sealed = self.record["surfaceArtifacts"].get(self._surface_artifact_key(ordinal, "after"))
                if not isinstance(sealed, dict):
                    raise RepositoryError("transaction-owned backup changed ownership")
                if not after.exists():
                    _unlink_inode_claim(after, _parse_identity(str(sealed["identity"])), str(sealed["sha256"]),
                                        mutation="after-cleanup-unlink", error="transaction-owned backup changed ownership")
                    continue
                self._verified_surface_artifact(ordinal, "after", _surface.after or b"")
                _before_mutation("after-cleanup")
                self._revalidate_private_artifact_containment(after)
                self._verified_surface_artifact(ordinal, "after", _surface.after or b"")
                sealed = self.record["surfaceArtifacts"][self._surface_artifact_key(ordinal, "after")]
                _unlink_inode_claim(after, _identity(os.stat(after)), str(sealed["sha256"]),
                                    mutation="after-cleanup-unlink", error="transaction-owned backup changed ownership")
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
            if not isinstance(sealed, dict) or sealed.get("identity") != identity or sealed.get("sha256") != sha256_bytes(artifact.read_bytes()):
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
            if artifact.is_symlink() or final_reparse or not stat.S_ISREG(final.st_mode) or final_identity != identity or sha256_bytes(artifact.read_bytes()) != sealed["sha256"]:
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
        if self._journal_bytes is None or self._journal_identity is None or not _path_has_identity(self.path, self._journal_identity) or self.path.read_bytes() != self._journal_bytes:
            raise RepositoryError("transaction journal changed ownership during cleanup")
        _unlink_inode_claim(self.path, self._journal_identity, sha256_bytes(self._journal_bytes),
                            mutation="journal-cleanup-unlink", error="transaction journal changed ownership during cleanup")
        for candidate in (directory, directory.parent):
            try:
                candidate.rmdir()
            except OSError:
                continue
            _fsync_directory(candidate.parent)

    def _persist(self) -> None:
        directory = _journal_directory(self.root, create=True)
        if self.path.parent != directory:
            raise RepositoryError("invalid WEDL transaction journal path")
        # Each durable record is a monotonic, compare-and-swap sealed generation.
        # This makes a journal replacement (including identical bytes) fail
        # closed rather than letting a later cleanup delete another writer's log.
        self.record["generation"] = int(self.record["generation"]) + 1
        payload = (json.dumps(self.record, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
        _before_mutation("journal-persist")
        try:
            identity = _atomic_write(self.path, payload, expected=self._journal_bytes,
                                     expected_identity=self._journal_identity)
        except Exception:
            self.record["generation"] = int(self.record["generation"]) - 1
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

    def _validate(self) -> None:
        required = {"version", "generation", "id", "phase", "ref", "previousHead", "committedHead", "surfaces", "indexBefore", "indexAfter", "indexLock", "indexArtifacts", "canonicalLock", "privateArtifacts", "sealedArtifacts", "surfaceArtifacts"}
        if set(self.record) != required or self.record.get("version") != FORMAT_VERSION or self.record.get("phase") not in _PHASES:
            raise RepositoryError("unsupported WEDL transaction journal")
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
