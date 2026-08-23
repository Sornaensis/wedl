from __future__ import annotations

from contextlib import closing
from importlib import resources
from pathlib import Path
import shutil

import pytest

import wedl.cli as cli
import wedl.compiler as compiler
import wedl.context as context
from wedl.compiler import cache_readiness, compile_world, connect
from wedl.context import build_context
from wedl.errors import CompileRequired
from wedl.repository import Repository


@pytest.fixture()
def ash_worktree_repo(tmp_path: Path) -> Repository:
    """A short, non-Git copy of Ash for cache-policy acceptance coverage.

    The shared ``ash_repo`` fixture commits every fixture source file.  That
    operation exceeds Windows' path limit for one deliberately descriptive Ash
    filename, so these tests use the supported WORKTREE repository mode instead.
    """
    root = tmp_path / "a"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    return Repository(root)


def _cache_snapshot(cache: Path) -> dict[str, bytes]:
    """Capture every cache file so strict reads prove they did not write."""
    if not cache.exists():
        return {}
    return {
        str(path.relative_to(cache)): path.read_bytes()
        for path in sorted(cache.rglob("*"))
        if path.is_file()
    }


def _character_and_scene(repository):
    world = repository.load_world()
    return world.find("Mara Vale", "character"), world.find("An Honest Absence", "scene")


def test_dramatic_irony_strict_ready_cache_succeeds_without_writes(ash_worktree_repo) -> None:
    character, scene = _character_and_scene(ash_worktree_repo)
    compile_world(ash_worktree_repo, profile_name="fts")
    cache = ash_worktree_repo.root / ".wedl"
    before = _cache_snapshot(cache)

    result = build_context(
        ash_worktree_repo,
        character_id=character.id,
        scene_id=scene.id,
        perspective="dramatic-irony",
        require_compiled=True,
    )

    assert result["perspective"] == "dramatic-irony"
    assert _cache_snapshot(cache) == before


@pytest.mark.parametrize("state", ["missing", "stale", "incompatible"])
def test_dramatic_irony_strict_unready_cache_never_mutates_it(ash_worktree_repo, state: str) -> None:
    character, scene = _character_and_scene(ash_worktree_repo)
    cache = ash_worktree_repo.root / ".wedl"
    if state != "missing":
        compile_world(ash_worktree_repo, profile_name="fts")
        database = cache / "world.sqlite"
        with closing(connect(database)) as connection:
            if state == "stale":
                connection.execute("UPDATE revision SET tree_oid='not-the-current-tree'")
            else:
                connection.execute("UPDATE revision SET compiler_fingerprint='incompatible'")
            connection.commit()

    assert cache_readiness(ash_worktree_repo)["state"] == state
    before = _cache_snapshot(cache)
    with pytest.raises(CompileRequired) as failure:
        build_context(
            ash_worktree_repo,
            character_id=character.id,
            scene_id=scene.id,
            perspective="dramatic-irony",
            require_compiled=True,
        )

    assert failure.value.details["cache"]["state"] == state
    assert _cache_snapshot(cache) == before


def test_dramatic_irony_strict_mode_cannot_compile_after_nested_readiness_loss(ash_worktree_repo, monkeypatch) -> None:
    """An outer strict check must not make a later nested read permissive."""
    character, scene = _character_and_scene(ash_worktree_repo)
    compile_world(ash_worktree_repo, profile_name="fts")
    database = ash_worktree_repo.root / ".wedl" / "world.sqlite"
    actual_require_database = context.require_database
    strict_calls: list[bool] = []

    def lose_cache_after_outer_read(repository, revision="HEAD", *, require_compiled=False):
        strict_calls.append(require_compiled)
        value = actual_require_database(repository, revision, require_compiled=require_compiled)
        if len(strict_calls) == 1:
            database.unlink()
        return value

    def compile_must_not_run(*args, **kwargs):
        raise AssertionError("a recursive strict read attempted to compile")

    monkeypatch.setattr(context, "require_database", lose_cache_after_outer_read)
    monkeypatch.setattr(compiler, "compile_world", compile_must_not_run)

    with pytest.raises(CompileRequired):
        build_context(
            ash_worktree_repo,
            character_id=character.id,
            scene_id=scene.id,
            perspective="dramatic-irony",
            require_compiled=True,
        )

    assert strict_calls == [True, True]
    assert not database.exists()


def test_dramatic_irony_default_mode_builds_a_missing_cache(ash_worktree_repo) -> None:
    """Without strict mode, a normal read retains the documented auto-build path."""
    character, scene = _character_and_scene(ash_worktree_repo)
    assert cache_readiness(ash_worktree_repo)["state"] == "missing"

    result = build_context(
        ash_worktree_repo,
        character_id=character.id,
        scene_id=scene.id,
        perspective="dramatic-irony",
    )

    assert result["perspective"] == "dramatic-irony"
    assert cache_readiness(ash_worktree_repo)["state"] == "ready"


def test_all_read_cli_dispatches_propagate_require_compiled(monkeypatch) -> None:
    """Every cache-backed CLI endpoint must carry the opt-in strict policy."""
    received: dict[str, list[bool]] = {}

    def capture(name: str):
        def handler(*args, **kwargs):
            received.setdefault(name, []).append(kwargs["require_compiled"])
            return {}
        return handler

    monkeypatch.setattr(cli, "Repository", lambda path: object())
    for name in (
        "list_entities", "show_entity", "entity_state", "knowledge",
        "interactions_between", "story_points", "search_world", "build_context",
        "conversation_view",
    ):
        monkeypatch.setattr(cli, name, capture(name))

    invocations = [
        ["entity", "list", "--repo", ".", "--require-compiled"],
        ["entity", "show", "Mara", "--repo", ".", "--require-compiled"],
        ["state", "Mara", "--repo", ".", "--tick", "1", "--require-compiled"],
        ["knowledge", "Mara", "--repo", ".", "--tick", "1", "--require-compiled"],
        ["interactions", "Mara", "Caldrin", "--repo", ".", "--require-compiled"],
        ["story-points", "--repo", ".", "--require-compiled"],
        ["search", "needle", "--repo", ".", "--require-compiled"],
        ["context", "Mara", "--repo", ".", "--require-compiled"],
        ["conversation", "show", "records", "--repo", ".", "--require-compiled"],
    ]
    for invocation in invocations:
        cli.dispatch(cli.parser().parse_args(invocation))

    assert received == {
        "list_entities": [True], "show_entity": [True], "entity_state": [True],
        "knowledge": [True], "interactions_between": [True], "story_points": [True],
        "search_world": [True], "build_context": [True], "conversation_view": [True],
    }
