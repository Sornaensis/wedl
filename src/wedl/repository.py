from __future__ import annotations

from dataclasses import dataclass
from contextlib import closing, contextmanager
import json
import os
from pathlib import Path
import sqlite3
import stat
import subprocess
from typing import Any, Callable, Iterable

import yaml

from . import SUPPORTED_SOURCE_SCHEMAS, __version__
from .errors import DirtyManagedTree, RepositoryError, StaleRevision, SupersededSchemaError
from .model import Record, World
from .source import parse_record, quarantine_superseded_schema
from .transaction_recovery import Surface, TransactionJournal, mutation_checkpoint
from .util import sha256_bytes


@dataclass(frozen=True, slots=True)
class Snapshot:
    revision: str
    tree_oid: str
    files: dict[str, bytes]
    blob_ids: dict[str, str]
    is_worktree: bool = False


PARSER_FINGERPRINT = (
    f"wedl-parser:{__version__}:{','.join(sorted(SUPPORTED_SOURCE_SCHEMAS))}:capability-envelope-v1:pyyaml-{yaml.__version__}:"
    f"{'libyaml' if getattr(yaml, '__with_libyaml__', False) else 'python'}"
)


class Repository:
    def __init__(self, path: str | Path = ".", source_root: str = "story") -> None:
        self.root = self._discover(Path(path))
        self.source_root = source_root
        self._world_cache: dict[tuple[str, str], World] = {}
        self._compiled_world_cache: dict[tuple[str, str, int, int], World] = {}
        self.last_load_stats: dict[str, Any] = {}

    @staticmethod
    def _discover(path: Path) -> Path:
        requested = path.expanduser()
        path = requested.resolve()
        cwd = Path.cwd().resolve()
        generic_hint = "Use --repo . from a repository directory, or pass an absolute repository root."
        # An installed example may be nested inside a larger Git checkout. Its
        # own story directory is still the explicitly requested repository,
        # rather than the enclosing checkout's top-level worktree.
        if (path / "story").is_dir():
            return path
        process = subprocess.run(
            ["git", "-C", str(path), "rev-parse", "--show-toplevel"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        if process.returncode == 0:
            return Path(process.stdout.strip()).resolve()
        if (path / "story").is_dir():
            return path
        if not requested.is_absolute() and len(requested.parts) == 1:
            current = cwd
            current_process = subprocess.run(
                ["git", "-C", str(current), "rev-parse", "--show-toplevel"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            if current_process.returncode == 0:
                current_root = Path(current_process.stdout.strip()).resolve()
                if requested.name == current_root.name:
                    raise RepositoryError(
                        f"relative repository path {str(requested)!r} resolves inside the current repository "
                        f"at {path}",
                        details={
                            "requested": str(requested),
                            "resolved": str(path),
                            "cwd": str(cwd),
                            "hint": f"Use --repo . (or omit --repo), or pass the absolute repository path {current_root}.",
                        },
                    )
            if requested.name == current.name and (current / "story").is_dir():
                raise RepositoryError(
                    f"relative repository path {str(requested)!r} resolves inside the current repository "
                    f"at {path}",
                    details={
                        "requested": str(requested),
                        "resolved": str(path),
                        "cwd": str(cwd),
                        "hint": f"Use --repo . (or omit --repo), or pass the absolute repository path {current}.",
                    },
                )
        raise RepositoryError(
            f"not a Git repository or wedl source root: {path}",
            details={
                "requested": str(requested),
                "resolved": str(path),
                "cwd": str(cwd),
                "hint": generic_hint,
            },
        )

    @property
    def is_git(self) -> bool:
        if (self.root / ".git").exists():
            return True
        # A bare source root may be created beneath a larger checkout (as is
        # common for previews and temporary authoring fixtures).  ``git -C``
        # then finds that enclosing repository, but it must not make this
        # independent source root read the enclosing repository's HEAD.
        process = self._git(["rev-parse", "--show-toplevel"], check=False)
        if process.returncode != 0:
            return False
        return Path(process.stdout.decode().strip()).resolve() == self.root

    def _git(
        self,
        args: Iterable[str],
        *,
        input_bytes: bytes | None = None,
        env: dict[str, str] | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess[bytes]:
        process = subprocess.run(
            ["git", "-C", str(self.root), *args],
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        if check and process.returncode != 0:
            message = process.stderr.decode("utf-8", "replace").strip()
            raise RepositoryError(message or f"git {' '.join(args)} failed")
        return process

    def head(self) -> str:
        if not self.is_git:
            return "WORKTREE"
        return self._git(["rev-parse", "HEAD"]).stdout.decode().strip()

    def tree_oid(self, revision: str) -> str:
        if revision != "WORKTREE" and self.is_git:
            return self._git(["rev-parse", f"{revision}^{{tree}}"]).stdout.decode().strip()
        # Worktree identity must include content, not only path names. This makes
        # cache checks safe for non-Git repositories and explicit WORKTREE reads.
        digest = bytearray()
        root = self.root / self.source_root
        for path in sorted(root.rglob("*.md")):
            relative = path.relative_to(self.root).as_posix().encode("utf-8")
            data = path.read_bytes()
            digest.extend(relative)
            digest.extend(b"\0")
            digest.extend(sha256_bytes(data).encode("ascii"))
            digest.extend(b"\n")
        return sha256_bytes(bytes(digest))

    def resolve(self, revision: str | None = "HEAD") -> str:
        if not self.is_git or revision in {None, "WORKTREE"}:
            return "WORKTREE"
        return self._git(["rev-parse", str(revision)]).stdout.decode().strip()

    def _batch_blobs(self, object_ids: list[str]) -> list[bytes]:
        if not object_ids:
            return []
        process = subprocess.Popen(
            ["git", "-C", str(self.root), "cat-file", "--batch"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        output, error = process.communicate("".join(f"{oid}\n" for oid in object_ids).encode())
        if process.returncode != 0:
            raise RepositoryError(error.decode("utf-8", "replace"))
        result: list[bytes] = []
        offset = 0
        for expected in object_ids:
            newline = output.find(b"\n", offset)
            if newline < 0:
                raise RepositoryError("truncated git cat-file response")
            header = output[offset:newline].decode("ascii", "replace").split()
            offset = newline + 1
            if len(header) != 3 or header[1] != "blob":
                raise RepositoryError(f"unexpected git object for {expected}")
            size = int(header[2])
            result.append(output[offset : offset + size])
            offset += size + 1
        return result

    def _tree_entries(self, revision: str) -> list[tuple[str, str]]:
        process = self._git(["ls-tree", "-r", "-z", revision, "--", self.source_root])
        entries: list[tuple[str, str]] = []
        for entry in process.stdout.split(b"\0"):
            if not entry:
                continue
            metadata, raw_path = entry.split(b"\t", 1)
            _mode, object_type, oid = metadata.decode().split()
            path = raw_path.decode("utf-8")
            if object_type == "blob" and path.endswith(".md"):
                entries.append((path, oid))
        return entries

    def _source_cache(self) -> sqlite3.Connection:
        path = self.root / ".wedl" / "source-cache.sqlite"
        path.parent.mkdir(parents=True, exist_ok=True)
        connection = sqlite3.connect(path, timeout=30.0)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("PRAGMA synchronous=NORMAL")
        connection.execute("PRAGMA busy_timeout=30000")
        connection.execute(
            """
            CREATE TABLE IF NOT EXISTS parsed_record(
              parser_fingerprint TEXT NOT NULL,
              blob_oid TEXT NOT NULL,
              frontmatter_json TEXT NOT NULL,
              body TEXT NOT NULL,
              raw_bytes BLOB NOT NULL,
              last_used TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              PRIMARY KEY(parser_fingerprint, blob_oid)
            )
            """
        )
        return connection

    @staticmethod
    def _cache_lookup(connection: sqlite3.Connection, object_ids: list[str]) -> dict[str, tuple[dict[str, Any], str, bytes]]:
        result: dict[str, tuple[dict[str, Any], str, bytes]] = {}
        # Stay below SQLite's parameter limit even for stress repositories.
        for start in range(0, len(object_ids), 400):
            batch = object_ids[start : start + 400]
            if not batch:
                continue
            placeholders = ",".join("?" for _ in batch)
            rows = connection.execute(
                f"SELECT blob_oid,frontmatter_json,body,raw_bytes FROM parsed_record "
                f"WHERE parser_fingerprint=? AND blob_oid IN ({placeholders})",
                [PARSER_FINGERPRINT, *batch],
            ).fetchall()
            for oid, frontmatter_json, body, raw_bytes in rows:
                result[str(oid)] = (json.loads(frontmatter_json), str(body), bytes(raw_bytes))
        return result

    @staticmethod
    def _cache_store(connection: sqlite3.Connection, oid: str, record: Record) -> None:
        try:
            frontmatter = json.dumps(record.frontmatter, ensure_ascii=False, separators=(",", ":"))
        except TypeError:
            # Source schemas are JSON-compatible, but a custom YAML extension
            # should not make reading fail merely because it cannot be cached.
            return
        connection.execute(
            "INSERT OR REPLACE INTO parsed_record(parser_fingerprint,blob_oid,frontmatter_json,body,raw_bytes,last_used) "
            "VALUES (?,?,?,?,?,CURRENT_TIMESTAMP)",
            (PARSER_FINGERPRINT, oid, frontmatter, record.body, record.raw_bytes),
        )

    def snapshot(self, revision: str | None = "HEAD") -> Snapshot:
        resolved = self.resolve(revision)
        if resolved == "WORKTREE":
            files: dict[str, bytes] = {}
            blobs: dict[str, str] = {}
            root = self.root / self.source_root
            for path in sorted(root.rglob("*.md")):
                relative = path.relative_to(self.root).as_posix()
                data = path.read_bytes()
                files[relative] = data
                blobs[relative] = sha256_bytes(data)
            return Snapshot("WORKTREE", self.tree_oid("WORKTREE"), files, blobs, True)
        entries = self._tree_entries(resolved)
        contents = self._batch_blobs([oid for _path, oid in entries])
        return Snapshot(
            resolved,
            self.tree_oid(resolved),
            {path: data for (path, _oid), data in zip(entries, contents, strict=True)},
            {path: oid for path, oid in entries},
            False,
        )

    def load_world(self, revision: str | None = "HEAD", *, cache_write: bool = True) -> World:
        """Load a revision, optionally without creating or updating source cache files."""

        resolved = self.resolve(revision)
        tree_oid = self.tree_oid(resolved)
        memory_key = (resolved, tree_oid)
        if resolved != "WORKTREE" and memory_key in self._world_cache:
            world = self._world_cache[memory_key]
            self.last_load_stats = {
                "mode": "memory-cache",
                "records": len(world.records),
                "parsed": 0,
                "cacheHits": len(world.records),
                "blobReads": 0,
            }
            return world

        if resolved == "WORKTREE":
            entries: list[tuple[str, str, bytes | None]] = []
            for path in sorted((self.root / self.source_root).rglob("*.md")):
                relative = path.relative_to(self.root).as_posix()
                data = path.read_bytes()
                entries.append((relative, sha256_bytes(data), data))
            is_worktree = True
        else:
            entries = [(path, oid, None) for path, oid in self._tree_entries(resolved)]
            is_worktree = False

        object_ids = [oid for _path, oid, _data in entries]
        if not cache_write:
            missing_positions = [index for index, (_path, _oid, data) in enumerate(entries) if data is None]
            if missing_positions:
                blobs = self._batch_blobs([entries[index][1] for index in missing_positions])
                entries = list(entries)
                for index, data in zip(missing_positions, blobs, strict=True):
                    path, oid, _old = entries[index]
                    entries[index] = (path, oid, data)
            records: dict[str, Record] = {}
            for path, oid, data in entries:
                assert data is not None
                record = parse_record(data, path, blob_oid=oid, revision=resolved)
                if record.id in records:
                    raise RepositoryError(f"duplicate entity ID {record.id}")
                records[record.id] = record
            world = World(resolved, tree_oid, records, self.root, self.source_root, is_worktree)
            self.last_load_stats = {
                "mode": "read-only-source",
                "records": len(records),
                "parsed": len(records),
                "cacheHits": 0,
                "blobReads": len(missing_positions),
            }
            return world

        with closing(self._source_cache()) as cache:
            cached = self._cache_lookup(cache, object_ids)
            missing_positions = [index for index, (_path, oid, data) in enumerate(entries) if oid not in cached and data is None]
            if missing_positions:
                blobs = self._batch_blobs([entries[index][1] for index in missing_positions])
                entries = list(entries)
                for index, data in zip(missing_positions, blobs, strict=True):
                    path, oid, _old = entries[index]
                    entries[index] = (path, oid, data)

            records: dict[str, Record] = {}
            parsed = 0
            cache_hits = 0
            blob_reads = len(missing_positions)
            for path, oid, data in entries:
                cached_value = cached.get(oid)
                if cached_value is not None:
                    frontmatter, body, raw_bytes = cached_value
                    try:
                        quarantine_superseded_schema(frontmatter, path)
                    except SupersededSchemaError:
                        cache.execute(
                            "DELETE FROM parsed_record WHERE parser_fingerprint=? AND blob_oid=?",
                            (PARSER_FINGERPRINT, oid),
                        )
                        cache.commit()
                        raise
                    record = Record(frontmatter, body, path, raw_bytes, blob_oid=oid, revision=resolved)
                    cache_hits += 1
                else:
                    assert data is not None
                    record = parse_record(data, path, blob_oid=oid, revision=resolved)
                    self._cache_store(cache, oid, record)
                    parsed += 1
                if record.id in records:
                    raise RepositoryError(f"duplicate entity ID {record.id}")
                records[record.id] = record
            cache.commit()

        world = World(resolved, tree_oid, records, self.root, self.source_root, is_worktree)
        if resolved != "WORKTREE":
            self._world_cache[memory_key] = world
        self.last_load_stats = {
            "mode": "parsed-source-cache",
            "records": len(records),
            "parsed": parsed,
            "cacheHits": cache_hits,
            "blobReads": blob_reads,
        }
        return world

    def status(self) -> list[str]:
        if not self.is_git:
            return []
        output = self._git(["status", "--porcelain=v1", "--", self.source_root]).stdout.decode()
        return [line for line in output.splitlines() if line.strip()]

    def assert_clean_managed(self) -> None:
        dirty = self.status()
        if dirty:
            raise DirtyManagedTree("managed story files have uncommitted changes", details={"paths": dirty})

    def is_ancestor(self, older: str, newer: str) -> bool:
        if not self.is_git or older == "WORKTREE" or newer == "WORKTREE":
            return False
        return self._git(["merge-base", "--is-ancestor", older, newer], check=False).returncode == 0

    def changed_paths(self, older: str, newer: str) -> list[str]:
        if not self.is_git:
            return []
        output = self._git(["diff", "--name-only", older, newer, "--", self.source_root]).stdout.decode()
        return [line for line in output.splitlines() if line]

    def ref(self, name: str) -> str | None:
        """Return a fully qualified ref's object ID without resolving HEAD."""

        if not self.is_git:
            return None
        process = self._git(["rev-parse", "--verify", "--quiet", name], check=False)
        return process.stdout.decode().strip() or None

    def ensure_backup_ref(self, name: str, expected_head: str) -> str:
        """Create an immutable migration backup ref with compare-and-swap.

        A retry may observe a ref made by its own earlier interrupted attempt;
        accepting only that exact old head keeps the operation idempotent while
        never replacing a different backup.
        """

        if not self.is_git:
            raise RepositoryError("Git is required for migration backups")
        if not name.startswith("refs/wedl/backups/migration/"):
            raise RepositoryError("migration backup refs must live below refs/wedl/backups/migration/")
        zero = "0" * 40
        process = self._git(["update-ref", name, expected_head, zero], check=False)
        if process.returncode == 0:
            return name
        if self.ref(name) == expected_head:
            return name
        message = process.stderr.decode("utf-8", "replace").strip()
        raise RepositoryError(message or "migration backup ref already exists for a different revision")

    @contextmanager
    def _canonical_write_lock(self):
        """Serialize WEDL's real-index reconciliation without claiming Git locks.

        The alternate index below makes the commit object independent of the
        worktree index, but the final index reconciliation must still use
        Git's real index.  A WEDL-owned lock serializes that small critical
        section across local processes.  ``index.lock`` remains exclusively
        Git's: an existing lock is an external transaction and is reported,
        never removed or reused.
        """

        git_dir = self.root / ".git"
        lock_path = git_dir / "wedl-canonical-write.lock"
        token = None
        for attempt in range(2):
            try:
                mutation_checkpoint("canonical-lock-create")
                descriptor = os.open(lock_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL)
                break
            except FileExistsError as exc:
                # Never steal an ordinary WEDL lock.  The sole reclaimable case
                # is a dead process whose on-disk lock is cryptographically
                # linked to a still-pending, validated transaction journal.
                if attempt or not TransactionJournal.reclaim_owned_stale_lock(self.root, lock_path):
                    raise RepositoryError("another WEDL canonical write is in progress") from exc
        else:  # pragma: no cover - loop either acquires or raises
            raise RepositoryError("another WEDL canonical write is in progress")
        token = __import__("uuid").uuid4().hex
        try:
            with os.fdopen(descriptor, "w", encoding="ascii") as lock_file:
                lock_file.write(f"pid={os.getpid()}\ntoken={token}\n")
                lock_file.flush()
                os.fsync(lock_file.fileno())
            status = os.stat(lock_path)
            self._canonical_lock = (lock_path, token, None, f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}", sha256_bytes(lock_path.read_bytes()))
            yield
        finally:
            try:
                fields = dict(line.split("=", 1) for line in lock_path.read_text(encoding="ascii").splitlines() if "=" in line)
            except OSError:
                fields = {}
            bound = getattr(self, "_canonical_lock", None)
            journal = bound[2] if bound else None
            ephemeral_owned = False
            if bound and journal is None:
                try:
                    status = os.stat(lock_path)
                    ephemeral_owned = (f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}" == bound[3] and sha256_bytes(lock_path.read_bytes()) == bound[4])
                except OSError:
                    pass
            if (isinstance(journal, TransactionJournal) and journal.owns_canonical_lock(lock_path, pid=os.getpid(), token=token)) or ephemeral_owned:
                # Never release a pathname based only on a prior read: claim the
                # exact inode first so an identical-byte replacement cannot be
                # removed at the final checkpoint.
                claim = git_dir / f".wedl-canonical-release-{os.getpid()}-{token}"
                try:
                    if isinstance(journal, TransactionJournal):
                        durable = journal.record.get("canonicalLock")
                        if not isinstance(durable, dict):
                            raise RepositoryError("canonical write lock ownership changed")
                        expected_identity, expected_digest = durable.get("identity"), durable.get("sha256")
                    else:
                        expected_identity, expected_digest = bound[3], bound[4]
                    if not isinstance(expected_identity, str) or not isinstance(expected_digest, str):
                        raise RepositoryError("canonical write lock ownership changed")
                    os.link(lock_path, claim)
                    claim_status = os.stat(claim)
                    claimed = (claim_status.st_dev, claim_status.st_ino, claim_status.st_size)
                    current = os.stat(lock_path)
                    claim_identity = f"{claim_status.st_dev:x}:{claim_status.st_ino:x}:{claim_status.st_size:x}"
                    current_identity = f"{current.st_dev:x}:{current.st_ino:x}:{current.st_size:x}"
                    if (current.st_dev, current.st_ino, current.st_size) != claimed or claim_identity != expected_identity or current_identity != expected_identity or sha256_bytes(claim.read_bytes()) != expected_digest or sha256_bytes(lock_path.read_bytes()) != expected_digest:
                        raise RepositoryError("canonical write lock ownership changed")
                    mutation_checkpoint("canonical-lock-release")
                    current = os.stat(lock_path)
                    current_identity = f"{current.st_dev:x}:{current.st_ino:x}:{current.st_size:x}"
                    if (current.st_dev, current.st_ino, current.st_size) != claimed or current_identity != expected_identity or not (f"{claim_status.st_dev:x}:{claim_status.st_ino:x}:{claim_status.st_size:x}" == expected_identity) or sha256_bytes(claim.read_bytes()) != expected_digest or sha256_bytes(lock_path.read_bytes()) != expected_digest:
                        raise RepositoryError("canonical write lock ownership changed")
                    lock_path.unlink()
                except OSError as exc:
                    raise RepositoryError("canonical write lock ownership changed") from exc
                finally:
                    try:
                        claim.unlink()
                    except OSError:
                        pass
            self._canonical_lock = None
            for journal in getattr(self, "_completed_transactions", []):
                journal.cleanup()
            self._completed_transactions = []

    @staticmethod
    def _index_info(entries: dict[str, dict[str, object] | str | None]) -> bytes:
        """Encode Git index-info records without treating path bytes as lines."""

        return b"".join(
            (
                f"0 {'0' * 40}\t{path}\0"
                if entry is None
                else f"{entry['mode'] if isinstance(entry, dict) else '100644'} {entry['blob'] if isinstance(entry, dict) else entry}\t{path}\0"
            ).encode()
            for path, entry in sorted(entries.items())
        )

    def _real_index_entries(self, paths: Iterable[str]) -> dict[str, dict[str, object] | None]:
        """Read exact real-index mode/stage/blob records for the owned paths."""
        selected = tuple(sorted(set(paths)))
        found: dict[str, dict[str, object] | None] = {path: None for path in selected}
        if not selected:
            return found
        output = self._git(["ls-files", "--stage", "-z", "--", *selected]).stdout
        for record in output.split(b"\0"):
            if not record:
                continue
            header, separator, path_bytes = record.partition(b"\t")
            fields = header.split()
            if not separator or len(fields) != 3:
                raise RepositoryError("invalid real Git index record")
            path = path_bytes.decode("utf-8", "surrogateescape")
            mode, blob, stage = (field.decode("ascii") for field in fields)
            if path not in found or stage != "0":
                raise RepositoryError("real Git index has an unowned conflict entry")
            found[path] = {"mode": mode, "stage": 0, "blob": blob}
        return found

    def _publish_real_index(
        self,
        journal: TransactionJournal,
        *,
        expected: dict[str, dict[str, object] | None],
        desired: dict[str, dict[str, object] | None],
    ) -> None:
        """Install a sealed private index through Git's final lock protocol.

        The mutable build never lives at ``.git/index.lock``.  We first retain
        both the old index and the complete new index in the transaction's
        private staging directory, seal and journal their identities, and only
        then hard-link the immutable install image to Git's conventional lock
        name for the one final replacement.  A crash before that point has no
        public index mutation; a crash after it has an authenticated artifact
        for restart adoption.
        """
        git_dir = self.root / ".git"
        index_path = git_dir / "index"
        lock_path = git_dir / "index.lock"
        if lock_path.exists():
            raise RepositoryError("Git index is locked by another process (index.lock)")
        artifacts = journal.record.get("indexArtifacts")
        if artifacts is None:
            if self._real_index_entries(expected) != expected:
                raise RepositoryError("canonical write lost real-index ownership")
            original = index_path.read_bytes() if index_path.exists() else b""
            backup = journal.write_staging_artifact("real-index-before", original)
            journal.seal_staging_artifact("real-index-before")
            install = journal.write_staging_artifact("real-index-install", original)
            env = os.environ.copy(); env["GIT_INDEX_FILE"] = str(install)
            index_info = self._index_info(desired)
            if index_info:
                mutation_checkpoint("real-index-update-private")
                self._git(["update-index", "-z", "--index-info"], input_bytes=index_info, env=env)
            with install.open("r+b") as handle:
                handle.flush(); os.fsync(handle.fileno())
            journal.seal_staging_artifact("real-index-install")
            journal.bind_index_artifacts(backup="real-index-before", install="real-index-install")
        else:
            if not isinstance(artifacts, dict) or not isinstance(artifacts.get("install"), str):
                raise RepositoryError("invalid WEDL transaction journal index artifact")
            install = journal.staging_path(artifacts["install"])
            journal.seal_staging_artifact(artifacts["install"])
        if self._real_index_entries(expected) != expected:
            raise RepositoryError("canonical write lost real-index ownership")
        # The final lock is an immutable hard link to a sealed private artifact.
        # If any other Git writer wins, O_EXCL-equivalent link creation leaves it
        # untouched and our durable artifact remains available for retry.
        try:
            mutation_checkpoint("real-index-lock-link")
            os.link(install, lock_path)
        except FileExistsError as exc:
            raise RepositoryError("Git index is locked by another process (index.lock)") from exc
        except OSError as exc:
            raise RepositoryError("could not claim Git index lock from private transaction artifact") from exc
        token = __import__("uuid").uuid4().hex
        status = os.stat(lock_path)
        identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
        digest = sha256_bytes(lock_path.read_bytes())
        journal.claim_index_lock(token=token, identity=identity, digest=digest)
        # A hard link must still be the sealed install image immediately before
        # the final namespace replacement.
        sealed = journal.record["sealedArtifacts"].get(journal.record["indexArtifacts"]["install"])
        try:
            current = os.stat(lock_path)
            current_identity = f"{current.st_dev:x}:{current.st_ino:x}:{current.st_size:x}"
            current_digest = sha256_bytes(lock_path.read_bytes())
        except OSError as exc:
            raise RepositoryError("canonical write lost real-index lock ownership") from exc
        if not isinstance(sealed, dict) or current_identity != identity or current_digest != digest or sealed.get("identity") != identity or sealed.get("sha256") != digest:
            raise RepositoryError("canonical write lost real-index lock ownership")
        if self._real_index_entries(expected) != expected:
            raise RepositoryError("canonical write lost real-index ownership")
        mutation_checkpoint("real-index-publish")
        # Revalidate *after* the injectable final checkpoint.  Digest alone is
        # insufficient: a hostile same-byte rename has a different inode and
        # must remain an external Git lock rather than be installed as ours.
        try:
            final = os.stat(lock_path)
            final_identity = f"{final.st_dev:x}:{final.st_ino:x}:{final.st_size:x}"
            final_digest = sha256_bytes(lock_path.read_bytes())
        except OSError as exc:
            raise RepositoryError("canonical write lost real-index lock ownership") from exc
        if final_identity != identity or final_digest != digest or self._real_index_entries(expected) != expected:
            raise RepositoryError("canonical write lost real-index lock ownership")
        os.replace(lock_path, index_path)
        # Windows can retain the source directory entry of a hard-linked file
        # after replacement.  Remove only that exact sealed inode; any other
        # post-replace lock is external state and remains fail-closed.
        if lock_path.exists():
            leftover = os.stat(lock_path)
            leftover_identity = f"{leftover.st_dev:x}:{leftover.st_ino:x}:{leftover.st_size:x}"
            if leftover_identity != identity or sha256_bytes(lock_path.read_bytes()) != digest:
                raise RepositoryError("canonical write lost real-index lock ownership")
            lock_path.unlink()
        if self._real_index_entries(desired) != desired:
            raise RepositoryError("canonical write lost real-index ownership")

    def _bind_transaction_lock(self, journal: TransactionJournal) -> None:
        lock = getattr(self, "_canonical_lock", None)
        if lock is None:
            raise RepositoryError("canonical transaction lock is not held")
        journal.bind_lock(lock[0], pid=os.getpid(), token=lock[1])
        self._canonical_lock = (lock[0], lock[1], journal, lock[3], lock[4])

    def _complete_transaction(self, journal: TransactionJournal) -> None:
        journal.finish()
        completed = getattr(self, "_completed_transactions", [])
        completed.append(journal)
        self._completed_transactions = completed

    def _reconcile_recorded_real_index(self, journal: TransactionJournal, *, roll_forward: bool) -> None:
        """Install only an exact durable index image recorded by the journal.

        The ref CAS precedes public-surface and real-index publication.  A
        downstream surface failure in that interval therefore legitimately
        leaves the real index at ``indexBefore`` even though the ref was briefly
        advanced.  The sole ambiguous interval is a crash during index
        replacement after ``sources_published``.  Never treat a third image as
        ours.
        """
        before = journal.record["indexBefore"]
        after = journal.record["indexAfter"]
        if not isinstance(before, dict) or not isinstance(after, dict):
            raise RepositoryError("invalid WEDL transaction journal")
        phase = journal.phase
        if phase not in {"prepared", "ref_committed", "sources_published", "index_published", "surfaces_published", "completed"}:
            raise RepositoryError("invalid WEDL transaction journal")
        if phase in {"prepared", "ref_committed"}:
            permitted = (before,)
        elif phase == "sources_published":
            # The real-index replacement starts after this durable phase and
            # can complete immediately before ``index_published`` is recorded.
            permitted = (before, after)
        elif phase in {"index_published", "surfaces_published", "completed"}:
            # With the committed ref, durable index publication proves after.
            # With the previous ref, its successful CAS rollback is the durable
            # proof that an interrupted rollback may already have restored
            # before; it may also still be poised to restore it from after.
            permitted = (after,) if roll_forward else (before, after)
        else:
            raise RepositoryError("invalid WEDL transaction journal")
        desired = after if roll_forward else before
        current = self._real_index_entries(before)
        if current not in permitted:
            raise RepositoryError("transaction recovery lost real-index ownership")
        if current != desired:
            self._publish_real_index(journal, expected=current, desired=desired)

    def _reconcile_authoring_transactions_locked(self) -> None:
        """Complete interrupted canonical writes before exposing a new one.

        A journal whose ref CAS completed deterministically rolls forward; one
        whose ref is still old deterministically rolls back.  Any third ref or
        filesystem state belongs to another writer and remains untouched.
        """

        journals = TransactionJournal.pending(self.root)
        self._adopt_owned_index_lock(journals)
        for journal in journals:
            if journal.phase == "completed":
                # Recovery acquires a fresh canonical lock after the crashed
                # writer's lock has been reclaimed. Bind that replacement at
                # finalization so a crash after ``finish`` leaves an exact
                # journal-authenticated lock, without mutating a journal whose
                # earlier recovery checks must still fail closed.
                self._bind_transaction_lock(journal)
                self._complete_transaction(journal)
                continue
            previous = str(journal.record["previousHead"])
            committed = str(journal.record["committedHead"])
            ref = str(journal.record["ref"])
            actual = self.ref(ref)
            if actual == previous:
                journal.restore_surfaces()
                self._reconcile_recorded_real_index(journal, roll_forward=False)
            elif actual == committed:
                journal.publish_surfaces()
                self._reconcile_recorded_real_index(journal, roll_forward=True)
            else:
                raise RepositoryError("transaction recovery lost ref ownership")
            self._bind_transaction_lock(journal)
            self._complete_transaction(journal)

    def _adopt_owned_index_lock(self, journals: tuple[TransactionJournal, ...]) -> None:
        """Finish only a journal-authenticated crashed real-index publication."""
        lock_path = self.root / ".git" / "index.lock"
        if not lock_path.exists():
            return
        owners = [journal for journal in journals if isinstance(journal.record.get("indexLock"), dict)]
        for journal in owners:
            record = journal.record["indexLock"]
            artifacts = journal.record.get("indexArtifacts")
            if not isinstance(artifacts, dict) or not isinstance(artifacts.get("install"), str):
                continue
            install_name = artifacts["install"]
            try:
                install = journal.staging_path(install_name)
                journal.seal_staging_artifact(install_name)
            except RepositoryError:
                continue
            sealed_install = journal.record["sealedArtifacts"].get(install_name)
            if not isinstance(sealed_install, dict):
                continue
            if sha256_bytes(lock_path.read_bytes()) != record.get("sha256"):
                continue
            status = os.stat(lock_path)
            identity = f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}"
            if identity != record.get("identity") or identity != sealed_install.get("identity") or sha256_bytes(install.read_bytes()) != sealed_install.get("sha256"):
                continue
            expected = journal.record["indexBefore"]
            desired = journal.record["indexAfter"]
            if self._real_index_entries(expected) != expected:
                raise RepositoryError("canonical write lost real-index ownership")
            # Hard-link claim binds the name's identity before replacement; its
            # own sealing is durable evidence should a process die between the
            # claim and install. A substituted external lock is retained.
            claim = journal.staging_path("index-lock-claim")
            try:
                if claim.exists():
                    claimed = os.stat(claim)
                    if (claimed.st_dev, claimed.st_ino, claimed.st_size) != (status.st_dev, status.st_ino, status.st_size):
                        raise RepositoryError("canonical write lost real-index lock ownership")
                else:
                    os.link(lock_path, claim)
                journal.seal_staging_artifact("index-lock-claim")
                if sha256_bytes(lock_path.read_bytes()) != record.get("sha256") or sha256_bytes(claim.read_bytes()) != record.get("sha256"):
                    raise RepositoryError("canonical write lost real-index lock ownership")
                mutation_checkpoint("real-index-crash-adopt")
                final = os.stat(lock_path)
                if (final.st_dev, final.st_ino, final.st_size) != (status.st_dev, status.st_ino, status.st_size) or sha256_bytes(lock_path.read_bytes()) != record.get("sha256"):
                    raise RepositoryError("canonical write lost real-index lock ownership")
                os.replace(lock_path, self.root / ".git" / "index")
                if lock_path.exists():
                    leftover = os.stat(lock_path)
                    if (leftover.st_dev, leftover.st_ino, leftover.st_size) != (status.st_dev, status.st_ino, status.st_size) or sha256_bytes(lock_path.read_bytes()) != record.get("sha256"):
                        raise RepositoryError("canonical write lost real-index lock ownership")
                    lock_path.unlink()
            except OSError as exc:
                raise RepositoryError("canonical write lost real-index lock ownership") from exc
            if self._real_index_entries(desired) != desired:
                raise RepositoryError("canonical write lost real-index ownership")
            return
        raise RepositoryError("Git index is locked by another process (index.lock)")

    def recover_authoring_transactions(self) -> None:
        """Recover owned journal entries under canonical-write serialization."""

        if not self.is_git:
            raise RepositoryError("Git is required for canonical writes")
        with self._canonical_write_lock():
            self._reconcile_authoring_transactions_locked()

    def commit_files(
        self,
        *,
        expected_head: str,
        files: dict[str, bytes | None],
        message: str,
        trailers: dict[str, str] | None = None,
        transaction_enroll: Callable[[TransactionJournal], None] | None = None,
        retain_transaction: bool = False,
    ) -> str:
        if not self.is_git:
            raise RepositoryError("Git is required for canonical writes")
        with self._canonical_write_lock():
            self._reconcile_authoring_transactions_locked()
            self.assert_clean_managed()
            actual = self.head()
            if actual != expected_head:
                raise StaleRevision("repository HEAD changed", details={"expected": expected_head, "actual": actual})
            env = os.environ.copy()
            index_path = self.root / ".git" / f"wedl-index-{os.getpid()}"
            env["GIT_INDEX_FILE"] = str(index_path)
            try:
                self._git(["read-tree", expected_head], env=env)
                real_index_before = self._real_index_entries(files)
                blob_ids: dict[str, str | None] = {}
                for path, data in sorted(files.items()):
                    if data is None:
                        blob_ids[path] = None
                        continue
                    blob = self._git(["hash-object", "-w", "--stdin"], input_bytes=data).stdout.decode().strip()
                    blob_ids[path] = blob
                index_after = {
                    path: None if blob is None else {
                        "mode": str((real_index_before[path] or {"mode": "100644"})["mode"]),
                        "stage": 0,
                        "blob": blob,
                    }
                    for path, blob in blob_ids.items()
                }
                alternate_index_info = self._index_info(index_after)
                if alternate_index_info:
                    self._git(["update-index", "-z", "--index-info"], input_bytes=alternate_index_info, env=env)
                tree = self._git(["write-tree"], env=env).stdout.decode().strip()
                subject = message.strip().splitlines()[0][:72] or "wedl: update story"
                full_message = subject
                if trailers:
                    full_message += "\n\n" + "\n".join(f"{key}: {value}" for key, value in sorted(trailers.items()))
                commit = self._git(["commit-tree", tree, "-p", expected_head], input_bytes=(full_message + "\n").encode()).stdout.decode().strip()
                # Recheck the compare-and-swap preconditions after acquiring
                # ownership and before advancing the ref or real index.
                self.assert_clean_managed()
                actual = self.head()
                if actual != expected_head:
                    raise StaleRevision("repository HEAD changed", details={"expected": expected_head, "actual": actual})
                if (self.root / ".git" / "index.lock").exists():
                    raise RepositoryError("Git index is locked by another process")
                ref = self._git(["symbolic-ref", "-q", "HEAD"], check=False).stdout.decode().strip()
                if not ref:
                    raise RepositoryError("detached HEAD writes are not supported")
                originals = {}
                original_modes: dict[str, int | None] = {}
                for path in files:
                    target = self.root / path
                    if target.exists() and not target.is_file():
                        raise RepositoryError("canonical write target is not a regular file")
                    originals[path] = target.read_bytes() if target.exists() else None
                    original_modes[path] = stat.S_IMODE(target.stat().st_mode) if target.exists() else None
                journal = TransactionJournal.create(
                    self.root,
                    ref=ref,
                    previous_head=expected_head,
                    committed_head=commit,
                    surfaces=[Surface(
                        path, originals[path], data, "source", original_modes[path],
                        None if data is None else int(str(index_after[path]["mode"]), 8) & 0o777,
                    ) for path, data in sorted(files.items())],
                    index_before=real_index_before,
                    index_after=index_after,
                )
                self._bind_transaction_lock(journal)
                # Downstream compiler/cache/receipt owners enroll every private
                # surface before the ref CAS.  The immutable record then stays
                # available for their publication/recovery phase.
                if transaction_enroll is not None:
                    transaction_enroll(journal)
                mutation_checkpoint("ref-cas-commit")
                self._git(["update-ref", ref, commit, expected_head])
                journal.advance("ref_committed")
                # Materialize only touched managed paths; unrelated worktree state
                # stays untouched. Then advance the *real* index for exactly those
                # paths to the same blobs as the new commit. Without this second
                # step, newly committed files appear simultaneously staged-deleted
                # and untracked because the alternate transaction index is removed.
                try:
                    journal.publish_surfaces()
                    journal.advance("sources_published")
                    if self._real_index_entries(files) != real_index_before:
                        raise RepositoryError("canonical write lost real-index ownership")
                    self._publish_real_index(journal, expected=real_index_before, desired=journal.record["indexAfter"])
                    journal.advance("index_published")
                except Exception as failure:
                    # A source or real-index failure happens after the ref CAS.
                    # Put both authoritative surfaces back only when the ref is
                    # still ours; otherwise retain the coherent committed source
                    # and refuse to overwrite another writer's work.
                    if (self.root / ".git" / "index.lock").exists():
                        # Git owns this lock.  We may still roll back only when
                        # its failed publication demonstrably left every owned
                        # real-index entry at the recorded before image.
                        current_index = self._real_index_entries(files)
                        if current_index != journal.record["indexBefore"]:
                            raise RepositoryError("Git index is locked by another process (index.lock)") from failure
                        mutation_checkpoint("ref-cas-rollback")
                        rollback = self._git(["update-ref", ref, expected_head, commit], check=False)
                        if rollback.returncode:
                            raise RepositoryError("canonical write failed after advancing HEAD; manual recovery is required") from failure
                        try:
                            journal.restore_surfaces()
                            self._complete_transaction(journal)
                        except Exception as recovery:
                            raise RepositoryError("canonical write recovery lost transaction ownership") from recovery
                        raise RepositoryError("Git index is locked by another process (index.lock)") from failure
                    mutation_checkpoint("ref-cas-rollback")
                    rollback = self._git(["update-ref", ref, expected_head, commit], check=False)
                    if rollback.returncode != 0:
                        raise RepositoryError("canonical write failed after advancing HEAD; manual recovery is required") from failure
                    try:
                        journal.restore_surfaces()
                        self._reconcile_recorded_real_index(journal, roll_forward=False)
                        self._complete_transaction(journal)
                    except Exception as recovery:
                        raise RepositoryError("canonical write recovery lost transaction ownership") from recovery
                    raise
                # Refresh stat information after materialization. Failures here are
                # non-fatal because the cache entries already contain the correct
                # blobs and a subsequent status/refresh will reconcile timestamps.
                if files:
                    self._git(["update-index", "--refresh", "--", *sorted(files)], check=False)
                journal.advance("surfaces_published")
                if not retain_transaction:
                    self._complete_transaction(journal)
                # A new commit invalidates only the HEAD alias. Commit-keyed worlds
                # remain safe historical snapshots.
                return commit
            finally:
                index_path.unlink(missing_ok=True)

    def _blob_at(self, revision: str, path: str) -> str | None:
        result = self._git(["rev-parse", f"{revision}:{path}"], check=False)
        return result.stdout.decode().strip() if result.returncode == 0 else None

    def rollback_committed_files(
        self,
        *,
        previous_head: str,
        committed_head: str,
        paths: Iterable[str],
    ) -> None:
        """Enroll a post-commit failure in the same durable recovery protocol."""
        if not self.is_git:
            raise RepositoryError("Git is required for canonical writes")
        selected = tuple(sorted(set(paths)))
        with self._canonical_write_lock():
            if self.head() != committed_head:
                raise RepositoryError("changeset post-commit recovery lost its HEAD compare-and-swap")
            ref = self._git(["symbolic-ref", "-q", "HEAD"], check=False).stdout.decode().strip()
            if not ref:
                raise RepositoryError("detached HEAD writes are not supported")
            previous_blob_ids: dict[str, str | None] = {}
            committed_blob_ids: dict[str, str | None] = {}
            surfaces: list[Surface] = []
            for path in selected:
                previous = self._git(["rev-parse", f"{previous_head}:{path}"], check=False)
                committed = self._git(["rev-parse", f"{committed_head}:{path}"], check=False)
                previous_blob_ids[path] = previous.stdout.decode().strip() if not previous.returncode else None
                committed_blob_ids[path] = committed.stdout.decode().strip() if not committed.returncode else None
                before = self._git(["cat-file", "-p", previous_blob_ids[path]]).stdout if previous_blob_ids[path] else None
                after = self._git(["cat-file", "-p", committed_blob_ids[path]]).stdout if committed_blob_ids[path] else None
                previous_mode = None
                committed_mode = None
                for revision, destination in ((previous_head, "previous"), (committed_head, "committed")):
                    listing = self._git(["ls-tree", revision, "--", path], check=False).stdout.decode().strip()
                    mode = int(listing.split()[0], 8) & 0o777 if listing else None
                    if destination == "previous":
                        previous_mode = mode
                    else:
                        committed_mode = mode
                surfaces.append(Surface(path, before, after, "source", previous_mode, committed_mode))
            index_after = self._real_index_entries(selected)
            expected_after = {
                path: None if blob is None else {"mode": str((index_after[path] or {"mode": "100644"})["mode"]), "stage": 0, "blob": blob}
                for path, blob in committed_blob_ids.items()
            }
            if index_after != expected_after:
                raise RepositoryError("changeset post-commit recovery lost real-index ownership")
            journal = TransactionJournal.create(
                self.root, ref=ref, previous_head=previous_head, committed_head=committed_head,
                surfaces=surfaces, index_before={
                    path: None if blob is None else {"mode": str((index_after[path] or {"mode": "100644"})["mode"]), "stage": 0, "blob": blob}
                    for path, blob in previous_blob_ids.items()
                }, index_after=expected_after,
            )
            self._bind_transaction_lock(journal)
            mutation_checkpoint("ref-cas-rollback")
            rollback = self._git(["update-ref", ref, previous_head, committed_head], check=False)
            if rollback.returncode:
                raise RepositoryError("changeset post-commit recovery lost its HEAD compare-and-swap")
            try:
                self._reconcile_authoring_transactions_locked()
            except Exception as failure:
                # Keep the newer revision authoritative when the durable record
                # cannot prove ownership of every restoration surface.
                self._git(["update-ref", ref, committed_head, previous_head], check=False)
                raise RepositoryError("changeset post-commit recovery failed; manual recovery is required") from failure
