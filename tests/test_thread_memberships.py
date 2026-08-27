from __future__ import annotations

import json
from importlib import resources
from pathlib import Path
import shutil

import pytest
from fastapi.testclient import TestClient

from wedl import V04_SOURCE_SCHEMA
from wedl.cli import main
from wedl.compiler import compile_world, connect, require_database
from wedl.errors import UsageError
from wedl.ids import id_from_seed
from wedl.query import thread_memberships
from wedl.repository import Repository
from wedl.server import create_app
from wedl.thread_filter import project_thread_memberships, resolve_thread_filter


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"
THREAD_UNKNOWN = "thread_0123456789ABCDEFGHJKMNPQRV"


def _append_frontmatter(path: Path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _worktree_repository(tmp_path: Path) -> Repository:
    root = tmp_path / "worktree"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    return Repository(root)


def _v05(repository: Repository) -> tuple[str, str]:
    for path in (repository.root / "story").rglob("*.md"):
        path.write_text(path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1), encoding="utf-8")
    _append_frontmatter(repository.root / "story" / "world.md", f"threads:\n  - id: {THREAD_A}\n    label: Archive\n  - id: {THREAD_B}\n    label: Road")
    member_path = sorted((repository.root / "story" / "characters").glob("*.md"))[0]
    _append_frontmatter(member_path, f"threads:\n  - {THREAD_A}\n  - {THREAD_B}")
    world = repository.load_world()
    member = next(record for record in world if record.source_path == member_path.relative_to(repository.root).as_posix())
    other = next(record for record in world if record.id != member.id and record.kind != "hypothesis")
    compile_world(repository, profile_name="fts")
    return member.id, other.id


def _append_membership(repository: Repository, directory: str, thread_id: str = THREAD_A, index: int = 0) -> str:
    path = sorted((repository.root / "story" / directory).glob("*.md"))[index]
    _append_frontmatter(path, f"threads:\n  - {thread_id}")
    return next(record.id for record in repository.load_world() if record.source_path == path.relative_to(repository.root).as_posix())


def test_membership_projection_cli_http_parity_and_no_oracle(tmp_path, capsys) -> None:
    repository = _worktree_repository(tmp_path)
    member_id, other_id = _v05(repository)
    record_ids = tuple(sorted((member_id, other_id)))
    expected = thread_memberships(repository, record_ids, (THREAD_A, THREAD_B), require_compiled=True)
    assert expected == {
        "protocol": "wedl-thread-memberships/v1", "revision": expected["revision"], "sourceSchema": "wedl/v0.5",
        "selectedThreadIds": [THREAD_A, THREAD_B],
        "records": [{"recordId": member_id, "threadIds": [THREAD_A, THREAD_B]}],
    }
    assert main(["thread-memberships", "--repo", str(repository.root), "--require-compiled", *[part for record_id in record_ids for part in ("--record-id", record_id)], "--thread-id", THREAD_A, "--thread-id", THREAD_B]) == 0
    assert json.loads(capsys.readouterr().out) == expected
    with TestClient(create_app(repository.root)) as client:
        response = client.get("/api/thread-memberships", params=[*[("recordId", record_id) for record_id in record_ids], ("threadId", THREAD_A), ("threadId", THREAD_B), ("requireCompiled", "true")])
    assert response.status_code == 200
    assert response.json() == expected


@pytest.mark.parametrize(
    ("record_ids", "thread_ids", "message"),
    [
        ((), (THREAD_A,), "thread membership projection requires at least one record id"),
        (("bad",), (THREAD_A,), "thread membership record ids must use stable entity ID format"),
        (("char_0123456789ABCDEFGHJKMNPQRS", "char_0123456789ABCDEFGHJKMNPQRS"), (THREAD_A,), "thread membership record ids must be unique"),
        (("world_0123456789ABCDEFGHJKMNPQRS", "char_0123456789ABCDEFGHJKMNPQRS"), (THREAD_A,), "thread membership record ids must be sorted"),
        (("char_0123456789ABCDEFGHJKMNPQRS",), (), "thread filter must contain at least one thread id"),
        (("char_0123456789ABCDEFGHJKMNPQRS",), (THREAD_A, THREAD_A), "thread filter thread ids must be unique"),
        (("char_0123456789ABCDEFGHJKMNPQRS",), (THREAD_B, THREAD_A), "thread filter thread ids must be sorted"),
        (("char_0123456789ABCDEFGHJKMNPQRS",), (THREAD_UNKNOWN,), "thread filter id is not declared by the world"),
    ],
)
def test_membership_projection_rejects_noncanonical_public_selectors(tmp_path, record_ids, thread_ids, message) -> None:
    repository = _worktree_repository(tmp_path)
    _v05(repository)
    with pytest.raises(UsageError, match=f"^{message}$"):
        thread_memberships(repository, record_ids, thread_ids)


def test_membership_projection_preserves_v03_and_v04_boundaries(ash_repo, tmp_path) -> None:
    world = ash_repo.load_world()
    candidate = next(record.id for record in world if record.kind != "hypothesis")
    compile_world(ash_repo, profile_name="fts")
    with pytest.raises(UsageError, match="^thread filtering requires a validated wedl/v0.5 world$"):
        thread_memberships(ash_repo, (candidate,), (THREAD_A,))

    root = tmp_path / "v04"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    repository = Repository(root)
    world_path = root / "story" / "world.md"
    world_path.write_text(world_path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", f"schema: {V04_SOURCE_SCHEMA}", 1), encoding="utf-8")
    client = TestClient(create_app(root))
    try:
        response = client.get("/api/thread-memberships", params=[("recordId", candidate), ("threadId", THREAD_A)])
    finally:
        client.close()
    assert response.status_code == 400
    assert response.json()["code"] == "v04_superseded"


def test_membership_projection_uses_settled_kind_specific_status_policy_and_one_lookup(tmp_path) -> None:
    repository = _worktree_repository(tmp_path)
    member_id, _other_id = _v05(repository)
    scene_ids = tuple(_append_membership(repository, "scenes", index=index) for index in range(3))
    conversation_ids = tuple(_append_membership(repository, "conversations", index=index) for index in range(3))
    compile_world(repository, profile_name="fts")
    world, database = require_database(repository, require_compiled=True)
    hypothesis_id = id_from_seed("hypothesis", "projection-hypothesis")
    with connect(database, False) as connection:
        connection.execute("UPDATE entity SET status='draft' WHERE id=?", (member_id,))
        for record_id, status in zip(scene_ids, ("active", "closed", "retired"), strict=True): connection.execute("UPDATE entity SET status=? WHERE id=?", (status, record_id))
        for record_id, status in zip(conversation_ids, ("active", "closed", "retired"), strict=True): connection.execute("UPDATE entity SET status=? WHERE id=?", (status, record_id))
        connection.execute("INSERT INTO entity(id,kind,title,domain,status,source_path,blob_oid,body_markdown,frontmatter_json) VALUES (?,?,?,?,?,?,?,?,?)", (hypothesis_id, "hypothesis", "Hidden possibility", "story", "open", "story/hypotheses/projection.md", None, "", "{}"))
        connection.execute("INSERT INTO record_thread(record_id,thread_id) VALUES (?,?)", (hypothesis_id, THREAD_A))
        connection.commit()
    record_ids = tuple(sorted((member_id, *scene_ids, *conversation_ids, hypothesis_id)))
    projected = thread_memberships(repository, record_ids, (THREAD_A,))
    assert [entry["recordId"] for entry in projected["records"]] == sorted((*scene_ids, *conversation_ids))

    with connect(database, True) as connection:
        thread_filter = resolve_thread_filter(connection, (THREAD_A,))
        assert thread_filter is not None
        statements: list[str] = []; connection.set_trace_callback(statements.append)
        project_thread_memberships(connection, record_ids, thread_filter)
    assert len(statements) == 1


def test_membership_projection_bounds_and_well_formed_unknown_candidates(tmp_path) -> None:
    repository = _worktree_repository(tmp_path)
    member_id, _other_id = _v05(repository)
    unknown = id_from_seed("character", "unknown-projection-candidate")
    assert thread_memberships(repository, tuple(sorted((member_id, unknown))), (THREAD_A,))["records"] == [{"recordId": member_id, "threadIds": [THREAD_A]}]
    records = tuple(sorted(id_from_seed("character", f"record-{index}") for index in range(257)))
    with pytest.raises(UsageError, match="^thread membership projection accepts at most 256 record ids$"):
        thread_memberships(repository, records, (THREAD_A,))
    threads = tuple(sorted(id_from_seed("thread", f"thread-{index}") for index in range(33)))
    with pytest.raises(UsageError, match="^thread membership projection accepts at most 32 thread ids$"):
        thread_memberships(repository, (member_id,), threads)
