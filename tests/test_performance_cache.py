from __future__ import annotations

from contextlib import closing
from importlib import resources
from io import BytesIO
import inspect
from pathlib import Path
import shutil
import sqlite3
import subprocess

from wedl import compiler
from wedl import changeset
from wedl.compiler import compile_world, compile_world_bytes
from wedl.repository import Repository


def _metadata(path):
    with closing(sqlite3.connect(path)) as connection:
        return {
            "revision": connection.execute(
                "SELECT head_commit, tree_oid, source_schema, sqlite_schema, search_profile, record_count FROM revision"
            ).fetchone(),
            "counts": tuple(
                connection.execute(f"SELECT count(*) FROM {table}").fetchone()[0]
                for table in ("entity", "search_document", "chronology_annotation", "spatial_location")
            ),
        }


def _chronology_repository(tmp_path: Path) -> Repository:
    root = tmp_path / "chronology"
    root.mkdir()
    source = Path(__file__).resolve().parent / "fixtures" / "legacy_worlds" / "chronology_conformance" / "story"
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", "story", ".gitignore"), ("commit", "-qm", "seed chronology")):
        subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def test_in_memory_authoring_cache_matches_direct_compiler_projection(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    revision = repository.head()
    memory = compile_world_bytes(repository, revision)

    assert memory.status == "compiled"
    assert memory.world_bytes is not None and memory.revision_bytes == memory.world_bytes
    byte_database = tmp_path / "authoring.sqlite"
    byte_database.write_bytes(memory.world_bytes)

    direct = compile_world(repository, revision)
    public_database = repository.root / ".wedl" / "world.sqlite"
    retained = repository.root / ".wedl" / "revisions" / f"{revision}.sqlite"
    assert direct["status"] == "compiled"
    assert public_database.is_file() and retained.is_file()
    assert _metadata(byte_database) == _metadata(public_database)
    assert memory.report["searchDocumentCount"] > 0
    assert memory.report["vectorDocumentCount"] > 0
    assert memory.report["chronology"]["calendarCount"] > 0
    for field in ("revision", "treeOid", "recordCount", "searchProfile", "searchDocumentCount", "ftsDocumentCount", "vectorDocumentCount", "uniqueVectorCount", "vectorLinkCount", "currentTime"):
        assert memory.report[field] == direct[field]
    assert {
        key: value for key, value in memory.report["chronology"].items() if key != "timingMs"
    } == {
        key: value for key, value in direct["chronology"].items() if key != "timingMs"
    }

    same_revision_memory = compile_world_bytes(repository, revision)
    same_revision_direct = compile_world(repository, revision, force=True)
    assert same_revision_memory.status == "compiled"
    assert same_revision_memory.report["buildMode"] == same_revision_direct["buildMode"] == "fast-forward-rebuild"
    assert same_revision_memory.report["changedPaths"] == same_revision_direct["changedPaths"] == []

    import pytest
    from wedl.errors import RepositoryError

    def world_contents(world):
        return (world.revision, world.tree_oid, world.root, world.source_root,
                world.is_worktree, world.diagnostics,
                [(key, record.frontmatter, record.body, record.raw_bytes,
                  record.blob_oid, record.source_path, record.revision)
                 for key, record in world.records.items()])

    tree = repository.tree_oid(revision)
    repository._git(["tag", "-a", "pinned-load-tag", "-m", "annotated fixture tag", revision])
    tag = repository.resolve("pinned-load-tag")
    assert repository._git(["cat-file", "-t", tag]).stdout.strip() == b"tag"
    source_cache = repository.root / ".wedl" / "source-cache.sqlite"
    cache_before = source_cache.read_bytes() if source_cache.exists() else None
    uncached_reader = Repository(repository.root)
    uncached_reader.load_world(revision, cache_write=False)
    assert (source_cache.read_bytes() if source_cache.exists() else None) == cache_before
    assert uncached_reader._world_cache == {}
    repository.load_world("HEAD")  # Populate the original parsed-source cache path.
    for target, original_path in ((revision, "HEAD"), (tree, f"{tree}^{{tree}}"),
                                  (tag, "pinned-load-tag")):
        baseline = Repository(repository.root)
        expected = baseline.load_world(original_path, cache_write=False)
        assert expected.records
        reader = Repository(repository.root)
        calls = []
        original_git = reader._git

        def observed_git(args, **kwargs):
            calls.append(list(args))
            return original_git(args, **kwargs)

        monkeypatch.setattr(reader, "_git", observed_git)
        cache_before = source_cache.read_bytes()
        actual = reader.load_world(target, cache_write=False)
        assert world_contents(actual) == world_contents(expected)
        assert source_cache.read_bytes() == cache_before and reader._world_cache == {}
        assert calls[0] == ["rev-parse", target, f"{target}^{{tree}}"]
        assert sum(args[0] == "rev-parse" for args in calls) == 1
        # Compare parsed-source cache behavior, then revalidate even a warm hit.
        calls.clear()
        warm = reader.load_world(target)
        baseline.load_world(original_path)
        assert world_contents(warm) == world_contents(expected)
        assert reader.last_load_stats == baseline.last_load_stats
        calls.clear()
        assert reader.load_world(target, cache_write=False) is warm
        assert calls == [["rev-parse", target, f"{target}^{{tree}}"]]
        assert reader.last_load_stats == {"mode": "memory-cache", "records": len(warm.records),
                                          "parsed": 0, "cacheHits": len(warm.records), "blobReads": 0}

    # Real Git failures retain their precedence before source/cache admission.
    blob = next(iter(expected.records.values())).blob_oid
    assert blob is not None
    for invalid in ("0" * 40, blob):
        with pytest.raises(RepositoryError) as original_failure:
            resolved = repository.resolve(invalid)
            repository.tree_oid(resolved)
        with monkeypatch.context() as isolated:
            isolated.setattr(repository, "_tree_entries", lambda *_: pytest.fail("failed target reached source"))
            isolated.setattr(repository, "_source_cache", lambda: pytest.fail("failed target reached cache"))
            with pytest.raises(RepositoryError) as paired_failure:
                repository.load_world(invalid)
        assert str(paired_failure.value) == str(original_failure.value)

    # Every noncanonical/type-mismatched path keeps the original resolution.
    for target in (None, "HEAD", "pinned-load-tag", revision[:12], revision.upper(), "WORKTREE", 17, []):
        try:
            resolved = repository.resolve(target)
            original_tree = repository.tree_oid(resolved)
        except (RepositoryError, TypeError) as error:
            with pytest.raises(type(error)) as actual_failure:
                repository.load_world(target, cache_write=False)
            assert str(actual_failure.value) == str(error)
        else:
            with monkeypatch.context() as isolated:
                isolated.setattr(repository, "_resolve_pinned_target", lambda *_: pytest.fail("fallback used paired lookup"))
                world = repository.load_world(target, cache_write=False)
            assert (world.revision, world.tree_oid) == (resolved, original_tree)

    # A standalone source root nested inside Git must remain a WORKTREE world.
    independent = repository.root / "independent"
    shutil.copytree(repository.root / "story", independent / "story")
    bare = Repository(independent)
    assert bare.root == independent and not bare.is_git
    plain = bare.load_world("WORKTREE", cache_write=False)
    assert world_contents(bare.load_world(revision, cache_write=False)) == world_contents(plain)
    assert not (independent / ".wedl").exists()

    # Mutable HEAD and explicit WORKTREE reads still observe the next edit.
    old = repository.load_world("HEAD", cache_write=False)
    record = next(iter(old.records.values()))
    changed = repository.root / record.source_path
    changed.write_bytes(record.raw_bytes + b"\nPinned lookup freshness fixture.\n")
    pending = repository.load_world("WORKTREE", cache_write=False)
    assert pending.is_worktree and pending.records[record.id].raw_bytes != record.raw_bytes
    assert repository.load_world("HEAD", cache_write=False).records[record.id].raw_bytes == record.raw_bytes
    repository._git(["add", "--", record.source_path])
    repository._git(["commit", "-qm", "fresh HEAD fixture"])
    fresh = repository.load_world("HEAD", cache_write=False)
    assert fresh.revision != old.revision and fresh.tree_oid != old.tree_oid
    assert fresh.records[record.id].raw_bytes == changed.read_bytes()
    assert repository.load_world(revision, cache_write=False).records[record.id].raw_bytes == record.raw_bytes


def test_unknown_authoring_capacity_defers_before_world_load(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    monkeypatch.setattr(compiler, "_authoring_preconstruction_bytes", lambda *_args: None)

    def forbidden_load(*_args, **_kwargs):
        raise AssertionError("source loaded before capacity admission")

    monkeypatch.setattr(repository, "load_world", forbidden_load)
    result = compile_world_bytes(repository, repository.head())
    assert result.status == "deferred-to-restart"
    assert result.report["reason"] == "authoring-preconstruction-memory-limit"
    assert result.world_bytes is result.revision_bytes is result.vector_bytes is None
    assert not (repository.root / ".wedl" / "world.sqlite").exists()


def test_sqlite_serialization_failure_defers_without_shared_cache_write(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    monkeypatch.setattr(compiler, "_serialize_authoring_connection", lambda *_args: None)
    result = compile_world_bytes(repository, repository.head())
    assert result.status == "deferred-to-restart"
    assert result.report["reason"] == "sqlite-serialize-unavailable"
    assert result.world_bytes is result.revision_bytes is result.vector_bytes is None
    assert not (repository.root / ".wedl" / "world.sqlite").exists()
    assert not (repository.root / ".wedl" / "vector-cache-v2.sqlite").exists()


def test_malformed_profile_in_valid_cache_defers_without_losing_source_commit(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    cache = repository.root / ".wedl" / "world.sqlite"
    cache.parent.mkdir(exist_ok=True)
    with closing(sqlite3.connect(cache)) as connection:
        connection.execute("CREATE TABLE revision(profile_json TEXT NOT NULL)")
        connection.execute("INSERT INTO revision(profile_json) VALUES ('{invalid')")
        connection.commit()
    original_cache = cache.read_bytes()
    original_head = repository.head()
    source = repository.root / "story" / "characters" / "reference-a.md"
    changed_source = source.read_bytes() + b"\n"
    request = {
        "protocol": "wedl-changeset/v1", "expectedHead": original_head,
        "idempotencyKey": "malformed-profile", "summary": "source survives cache deferral",
        "operations": [],
    }

    def planned_preview(_repository, payload, **_kwargs):
        request_hash = changeset._request_hash(payload)
        return {
            "valid": True, "expectedHead": original_head, "requestHash": request_hash,
            "confirmationToken": changeset.confirmation_token(
                payload, request_hash=request_hash, expected_head=original_head,
            ),
            "generatedIds": {}, "touchedEntityIds": [],
            "_changes": {source.relative_to(repository.root).as_posix(): changed_source},
            "_recordCount": 1,
        }

    monkeypatch.setattr(changeset, "preview", planned_preview)
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert committed["status"] == "committed"
    assert committed["newHead"] != original_head
    assert committed["compile"]["reason"] == "authoring-cache-profile-invalid"
    assert source.read_bytes() == changed_source
    assert cache.read_bytes() == original_cache
    assert not list((repository.root / ".wedl" / "revisions").glob("*.sqlite"))


def test_bounded_git_tree_metadata_defers_before_world_load(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    monkeypatch.setattr(compiler, "_AUTHORING_TREE_ENTRY_LIMIT", 8)

    def forbidden_load(*_args, **_kwargs):
        raise AssertionError("world loaded after metadata admission failed")

    monkeypatch.setattr(repository, "load_world", forbidden_load)
    result = compile_world_bytes(repository, repository.head())
    assert result.status == "deferred-to-restart"
    assert result.report["reason"] == "authoring-preconstruction-memory-limit"
    assert not (repository.root / ".wedl" / "world.sqlite").exists()


def test_non_source_git_tree_output_is_bounded_before_world_load(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    revision = repository.head()
    monkeypatch.setattr(repository, "resolve", lambda _revision: revision)
    monkeypatch.setattr(compiler, "_AUTHORING_TREE_OUTPUT_LIMIT", 80)

    class NonSourceTree:
        def __init__(self):
            self.stdout = BytesIO(b"100644 blob " + b"a" * 40 + b" 4\tstory/asset.bin\0" * 3)

        def wait(self):
            return 0

        def poll(self):
            return 0

    monkeypatch.setattr(compiler.subprocess, "Popen", lambda *_args, **_kwargs: NonSourceTree())
    monkeypatch.setattr(repository, "load_world", lambda *_args, **_kwargs: (_ for _ in ()).throw(
        AssertionError("world loaded after non-source tree overflow")
    ))
    result = compile_world_bytes(repository, revision)
    assert result.status == "deferred-to-restart"
    assert result.report["reason"] == "authoring-preconstruction-memory-limit"


def test_compiler_prices_simultaneously_live_caller_bytes(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)

    def forbidden_load(*_args, **_kwargs):
        raise AssertionError("world loaded after overlapping phase budget failed")

    monkeypatch.setattr(repository, "load_world", forbidden_load)
    result = compile_world_bytes(repository, repository.head(), caller_live_bytes=512 * 1024 * 1024)
    assert result.status == "deferred-to-restart"
    assert result.report["reason"] == "authoring-preconstruction-memory-limit"


def test_vector_serialization_retains_world_image_and_both_page_stores(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    revision = repository.head()
    original = compiler._serialize_authoring_connection
    images: list[bytes] = []

    def observe(connection):
        if images:
            caller = inspect.currentframe().f_back.f_locals
            assert caller["data"] is images[0]
            assert caller["connection"].execute("PRAGMA page_count").fetchone()[0] > 0
            assert caller["vector_connection"].execute("PRAGMA page_count").fetchone()[0] > 0
        image = original(connection)
        if image is not None:
            images.append(image)
        return image

    monkeypatch.setattr(compiler, "_serialize_authoring_connection", observe)
    compiled = compile_world_bytes(repository, revision)
    assert compiled.status == "compiled"
    assert compiled.world_bytes is images[0]
    assert compiled.vector_bytes is images[1]
    assert compiler._AUTHORING_PRECONSTRUCTION_FIXED == 384 * 1024 * 1024


def test_malformed_or_unknown_git_tree_metadata_has_no_capacity_proof(tmp_path, monkeypatch) -> None:
    repository = _chronology_repository(tmp_path)
    revision = repository.head()
    assert compiler._authoring_source_metadata_bytes(repository, "missing-ref") is None

    class MalformedTree:
        def __init__(self):
            self.stdout = BytesIO(b"malformed\0")

        def wait(self):
            return 0

        def poll(self):
            return 0

    monkeypatch.setattr(compiler.subprocess, "Popen", lambda *_args, **_kwargs: MalformedTree())
    assert compiler._authoring_source_metadata_bytes(repository, revision) is None
