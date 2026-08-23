from __future__ import annotations

from dataclasses import dataclass
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import subprocess
from typing import Any, Iterable

import yaml

from . import SOURCE_SCHEMA, __version__
from .errors import DirtyManagedTree, RepositoryError, StaleRevision
from .model import Record, World
from .source import parse_record
from .util import sha256_bytes


@dataclass(frozen=True, slots=True)
class Snapshot:
    revision: str
    tree_oid: str
    files: dict[str, bytes]
    blob_ids: dict[str, str]
    is_worktree: bool = False


PARSER_FINGERPRINT = (
    f"wedl-parser:{__version__}:{SOURCE_SCHEMA}:pyyaml-{yaml.__version__}:"
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
        return (self.root / ".git").exists() or self._git(["rev-parse", "--git-dir"], check=False).returncode == 0

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

    def commit_files(
        self,
        *,
        expected_head: str,
        files: dict[str, bytes | None],
        message: str,
        trailers: dict[str, str] | None = None,
    ) -> str:
        if not self.is_git:
            raise RepositoryError("Git is required for canonical writes")
        self.assert_clean_managed()
        actual = self.head()
        if actual != expected_head:
            raise StaleRevision("repository HEAD changed", details={"expected": expected_head, "actual": actual})
        env = os.environ.copy()
        index_path = self.root / ".git" / f"wedl-index-{os.getpid()}"
        env["GIT_INDEX_FILE"] = str(index_path)
        try:
            self._git(["read-tree", expected_head], env=env)
            blob_ids: dict[str, str | None] = {}
            for path, data in sorted(files.items()):
                if data is None:
                    blob_ids[path] = None
                    self._git(["update-index", "--force-remove", "--", path], env=env, check=False)
                    continue
                blob = self._git(["hash-object", "-w", "--stdin"], input_bytes=data).stdout.decode().strip()
                blob_ids[path] = blob
                self._git(["update-index", "--add", "--cacheinfo", "100644", blob, path], env=env)
            tree = self._git(["write-tree"], env=env).stdout.decode().strip()
            subject = message.strip().splitlines()[0][:72] or "wedl: update story"
            full_message = subject
            if trailers:
                full_message += "\n\n" + "\n".join(f"{key}: {value}" for key, value in sorted(trailers.items()))
            commit = self._git(["commit-tree", tree, "-p", expected_head], input_bytes=(full_message + "\n").encode()).stdout.decode().strip()
            ref = self._git(["symbolic-ref", "-q", "HEAD"], check=False).stdout.decode().strip()
            if not ref:
                raise RepositoryError("detached HEAD writes are not supported")
            self._git(["update-ref", ref, commit, expected_head])
            # Materialize only touched managed paths; unrelated worktree state
            # stays untouched. Then advance the *real* index for exactly those
            # paths to the same blobs as the new commit. Without this second
            # step, newly committed files appear simultaneously staged-deleted
            # and untracked because the alternate transaction index is removed.
            for path, data in files.items():
                target = self.root / path
                if data is None:
                    target.unlink(missing_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(data)
            for path, blob in blob_ids.items():
                if blob is None:
                    self._git(["update-index", "--force-remove", "--", path], check=False)
                else:
                    self._git(["update-index", "--add", "--cacheinfo", "100644", blob, path])
            # Refresh stat information after materialization. Failures here are
            # non-fatal because the cache entries already contain the correct
            # blobs and a subsequent status/refresh will reconcile timestamps.
            if files:
                self._git(["update-index", "--refresh", "--", *sorted(files)], check=False)
            # A new commit invalidates only the HEAD alias. Commit-keyed worlds
            # remain safe historical snapshots.
            return commit
        finally:
            index_path.unlink(missing_ok=True)
