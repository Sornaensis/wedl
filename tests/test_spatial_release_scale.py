"""Small authored-source scale smoke; the envelope run is an explicit command."""

from __future__ import annotations

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from dataclasses import replace
import ctypes
import os
import shutil
import sqlite3
import subprocess
import sys
import time

import pytest

from wedl.compiler import DDL, INDEX_DDL
from wedl.spatial_query import SpatialStore
from wedl.spatial_index import SpatialProjection, insert_spatial_index


ROOT = Path(__file__).resolve().parents[1]
SPEC = spec_from_file_location("benchmark_spatial_release", ROOT / "tools/benchmark_spatial_release.py")
assert SPEC and SPEC.loader
benchmark = module_from_spec(SPEC)
SPEC.loader.exec_module(benchmark)


def _assert_children_page_two_measures_production_cursor() -> None:
    with sqlite3.connect(":memory:") as connection:
        connection.executescript(DDL)
        connection.executescript(INDEX_DDL)
        connection.execute("INSERT INTO spatial_capability VALUES ('spatial-core-v1',0)")
        identifiers = ("location:parent", "location:z", "location:a", "location:m")
        for ordinal, ident in enumerate(identifiers):
            connection.execute("INSERT INTO entity VALUES (?, 'location', ?, 'world', 'canonical', ?, NULL, '', '{}')",
                               (ident, ident, f"story/{ordinal}.md"))
            connection.execute("INSERT INTO spatial_location(id,source_ordinal,has_spatial,parent_id,definition_json) VALUES (?,?,0,?,'{}')",
                               (ident, ordinal, None if ordinal == 0 else identifiers[0]))
        store = SpatialStore(connection, "benchmark-regression")
        statements = []
        connection.set_trace_callback(statements.append)
        evidence = benchmark._children_page_two_vm(store, identifiers[0], 1)
        connection.set_trace_callback(None)
        pages = [sql for sql in statements if "SELECT id,source_ordinal" in sql]
        assert len(pages) == 2 and all("ORDER BY source_ordinal,id" in sql for sql in pages)
        assert "(source_ordinal,id)>(1,'location:z')" in pages[1]
        assert connection.execute(pages[1]).fetchall() == [("location:a", 2), ("location:m", 3)]
        assert evidence["state"] == "ok" and evidence["vmStepsUpperBound"] <= 50_000


def test_generated_authored_source_compile_rebuild_and_http_are_repeat_stable(tmp_path: Path) -> None:
    _assert_timeout_reaps_outliving_child(tmp_path)
    _assert_children_page_two_measures_production_cursor()
    _assert_deferred_parent_build_is_bounded()
    _assert_generated_commit_matches_normal_git_staging(tmp_path / "staging")
    result = benchmark.run(base=ROOT, output=tmp_path / "evidence",
                           places=128, maps=2, routes=8, portals=1, overlays=1, repeats=2)
    assert result["counts"]["hierarchyDepth"] == 128
    assert result["counts"]["coordinateFreePlaces"] == 8
    assert result["compiledRows"] == {
        "spatial_map": 2, "spatial_location": 128, "spatial_route_edge": 8,
        "spatial_portal": 1, "spatial_overlay": 1,
    }
    assert len(result["compile"]["runs"]) == 2
    assert {sample["status"] for sample in result["compile"]["runs"]} == {"compiled"}
    assert all(case["outcome"]["state"] == "ok" for case in result["api"].values())
    assert all(len(case["samplesMs"]) == 2 and len(case["resultSha256"]) == 64
               for case in result["api"].values())
    assert (tmp_path / "evidence" / "manifest.json").is_file()
    assert result["fixturePreparation"]["verifiedBlobModes"]


def _assert_deferred_parent_build_is_bounded() -> None:
    empty = SpatialProjection(*(() for _ in range(16)))
    count = 2000
    identifiers = sorted(range(count), key=lambda value: f"location:scale-{value}")
    locations = tuple((f"location:scale-{value}", ordinal, 0,
                       f"location:scale-{value-1}" if value else None,
                       None, None, None, None, None, None, None, None, None, "{}")
                      for ordinal, value in enumerate(identifiers))
    projection = replace(empty, locations=locations)
    with sqlite3.connect(":memory:") as connection:
        connection.execute("PRAGMA foreign_keys=ON")
        connection.executescript(DDL)
        connection.executemany("INSERT INTO entity VALUES (?, 'location', '', 'world', 'canonical', ?, NULL, '', '{}')",
                               ((row[0], f"story/{ordinal}.md") for ordinal, row in enumerate(locations)))
        steps = [0]
        def progress():
            steps[0] += 100
            return 0
        connection.set_progress_handler(progress, 100)
        insert_spatial_index(connection, projection)
        connection.set_progress_handler(None, 0)
        assert steps[0] < 1_000_000
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("SELECT id,parent_id FROM spatial_location ORDER BY source_ordinal").fetchall() == [
            (row[0], row[3]) for row in locations]
        assert connection.execute("SELECT name FROM sqlite_schema WHERE name='spatial_build_parent_idx'").fetchall() == []
        connection.commit()
        bad = replace(empty, locations=(("location:missing-entity", count, 0, None,
                                        None, None, None, None, None, None, None, None, None, "{}"),))
        with pytest.raises(sqlite3.IntegrityError):
            insert_spatial_index(connection, bad)
        assert connection.execute("SELECT name FROM sqlite_schema WHERE name='spatial_build_parent_idx'").fetchall() == []


def _assert_generated_commit_matches_normal_git_staging(sandbox: Path) -> None:
    sandbox.mkdir()
    base_parent = benchmark._git(ROOT, "rev-parse", "HEAD")
    trees = []
    for method in ("conventional", "stream"):
        root = sandbox / method
        root.mkdir()
        shutil.copytree(ROOT / ".git", root / ".git", copy_function=shutil.copyfile)
        benchmark._git(root, "config", "core.longpaths", "true")
        benchmark._git(root, "sparse-checkout", "init", "--cone")
        benchmark._git(root, "sparse-checkout", "set", "story")
        benchmark._git(root, "reset", "--hard", base_parent)
        executable = benchmark._run_command(["git", "-C", str(root), "hash-object", "-w", "--stdin"],
                                            input=b"#!/bin/sh\nexit 0\n").stdout.decode().strip()
        benchmark._git(root, "update-index", "--add", "--cacheinfo", f"100755,{executable},tools/fixture-executable")
        benchmark._git(root, "update-index", "--add", "--cacheinfo", f"160000,{base_parent},vendor/fixture-gitlink")
        benchmark._git(root, "-c", "user.name=spatial scale", "-c", "user.email=scale@test.invalid",
                       "commit", "-qm", "fixture parent with executable and gitlink")
        parent = benchmark._git(root, "rev-parse", "HEAD")
        benchmark.generate(root, places=128, maps=2, routes=8, portals=1, overlays=1)
        if method == "conventional":
            benchmark._git(root, "add", "-A", "story")
            benchmark._git(root, "-c", "user.name=spatial scale", "-c", "user.email=scale@test.invalid",
                           "commit", "-qm", "generated spatial scale fixture")
        else:
            metadata = benchmark._commit_generated(root, sandbox, parent)
            assert metadata["sourceFiles"] == 141
            assert benchmark._git(root, "status", "--porcelain", "--", "story") == ""
            assert "100755 blob" in benchmark._git(root, "ls-tree", "HEAD", "tools/fixture-executable")
            assert "160000 commit" in benchmark._git(root, "ls-tree", "HEAD", "vendor/fixture-gitlink")
            assert not (sandbox / "generated-import.fi").exists()
            with pytest.raises(RuntimeError, match="outside its sandbox"):
                benchmark._commit_generated(root, root, parent)
            with pytest.raises(RuntimeError, match="parent changed"):
                benchmark._commit_generated(root, sandbox, parent)
            current = benchmark._git(root, "rev-parse", "HEAD")
            _assert_staging_failures_preserve_branch_and_index(root, sandbox, current, base_parent)
            stream_file = sandbox / "generated-import.fi"
            stream_file.write_bytes(b"preexisting input")
            with pytest.raises(FileExistsError):
                benchmark._commit_generated(root, sandbox, current)
            assert stream_file.read_bytes() == b"preexisting input"
            stream_file.unlink()
            (root / "story" / ".gitignore").write_text("*.md\n")
            with pytest.raises(RuntimeError, match="ignore rules"):
                benchmark._commit_generated(root, sandbox, current)
            (root / "story" / ".gitignore").unlink()
            (root / "story" / ".gitattributes").write_text("*.md filter=unsupported\n")
            with pytest.raises(RuntimeError, match="attributes"):
                benchmark._commit_generated(root, sandbox, benchmark._git(root, "rev-parse", "HEAD"))
        trees.append(benchmark._git(root, "rev-parse", "HEAD^{tree}"))
        assert benchmark._git(root, "rev-parse", "HEAD^") == parent
        assert benchmark._git(root, "fsck", "--no-reflogs", "--no-dangling", "--connectivity-only") == ""
    assert trees[0] == trees[1]


def _assert_staging_failures_preserve_branch_and_index(root: Path, sandbox: Path,
                                                       current: str, wrong_parent: str) -> None:
    index = benchmark._git(root, "ls-files", "--stage")
    original = benchmark._run_command
    source = root / "story/locations/location-scale-0.md"
    content = source.read_bytes()
    for failure in ("import", "missing-blob", "cas", "source-drift"):
        def run(command, **kwargs):
            if "fast-import" in command and failure == "import":
                raise subprocess.CalledProcessError(1, command, stderr=b"injected importer failure")
            if "mktree" in command and failure == "missing-blob":
                kwargs["input"] = b"100644 blob " + b"0" * 40 + b"\tmissing\0"
            if "update-ref" in command and failure == "cas":
                command = [*command[:-1], wrong_parent]
            result = original(command, **kwargs)
            if "fast-import" in command and failure == "source-drift":
                source.write_bytes(content + b"\nchanged\n")
            return result
        try:
            with pytest.MonkeyPatch.context() as patch:
                patch.setattr(benchmark, "_run_command", run)
                with pytest.raises((RuntimeError, subprocess.CalledProcessError)):
                    benchmark._commit_generated(root, sandbox, current)
        finally:
            if source.read_bytes() != content:
                source.write_bytes(content)
        assert benchmark._git(root, "rev-parse", "HEAD") == current
        assert benchmark._git(root, "ls-files", "--stage") == index
        assert not (sandbox / "generated-import.fi").exists()


def _assert_timeout_reaps_outliving_child(sandbox: Path) -> None:
    child_record = sandbox / "timeout-child.txt"
    command = [sys.executable, "-c",
               "import subprocess,sys,time;from pathlib import Path;"
               "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']);"
               "Path(sys.argv[1]).write_text(str(child.pid));time.sleep(60)", str(child_record)]
    started = time.perf_counter()
    with pytest.raises(subprocess.TimeoutExpired):
        benchmark._run_command(command, timeout=3)
    assert time.perf_counter() - started < 25
    child_pid = int(child_record.read_text())
    if os.name == "nt":
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.restype = ctypes.c_void_p
        kernel.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
        kernel.CloseHandle.argtypes = [ctypes.c_void_p]
        handle = kernel.OpenProcess(0x1000, False, child_pid)
        if handle:
            try:
                code = ctypes.c_ulong()
                assert kernel.GetExitCodeProcess(handle, ctypes.byref(code))
                assert code.value != 259
            finally:
                kernel.CloseHandle(handle)
    else:
        _assert_posix_child_stopped(child_pid)


def _assert_posix_child_stopped(child_pid: int) -> None:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try:
            os.kill(child_pid, 0)
        except ProcessLookupError:
            return
        # An adopted zombie is stopped even while its new parent has not reaped it.
        status = subprocess.run(["ps", "-o", "stat=", "-p", str(child_pid)],
                                capture_output=True, text=True,
                                timeout=min(1, max(0.001, deadline - time.monotonic())))
        if status.returncode == 0 and status.stdout.strip().startswith("Z"):
            return
        time.sleep(0.05)
    pytest.fail(f"Timed-out child {child_pid} remains alive after bounded cleanup wait")
