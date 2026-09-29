from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

import pytest

from conftest import git
from wedl import migration
from wedl.cli import EXAMPLE_PACKAGES as CLI_EXAMPLES, initialize
from wedl.command_parser import EXAMPLE_PACKAGES as PARSER_EXAMPLES, parser
from wedl.compiler import cache_readiness, compile_world, world_from_database
from wedl.errors import StaleRevision
from wedl.repository import Repository
from wedl.source import split_envelope
from wedl.validation import validate_world


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("build_v07_packaged_examples", ROOT / "tools" / "build_v07_packaged_examples.py")
assert SPEC and SPEC.loader
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)


def test_full_three_package_conversion_rebuild_reproducibility(tmp_path: Path) -> None:
    started = time.perf_counter()
    reports = builder.run(write=False)
    assert {item["package"]: item["records"] for item in reports} == {
        "ash_archive_v07": 262,
        "frontiersmen_v07": 309,
        "chronology_conformance_v07": 7,
    }
    for item in reports:
        assert item["legacy_sha256"] != item["v07_sha256"]
        assert len(item["legacy_sha256"]) == len(item["v07_sha256"]) == 64
    data_root = ROOT / "src" / "wedl" / "data"
    generated = tmp_path / "generated"
    for package in builder.PACKAGES:
        shutil.copytree(data_root / package, generated / package)
    assert builder.run(write=True, data_root=generated) == reports
    assert builder.run(write=False, data_root=generated) == reports

    counts = {"fields": 0, "empty": 0, "nonempty": 0}
    tokens: set[str] = set()
    cache_states: dict[str, str] = {}
    for package, report in zip(builder.PACKAGES, reports, strict=True):
        original = builder._story_files(data_root / package)
        converted = builder._story_files(data_root / f"{package}_v07")
        regenerated = builder._story_files(generated / f"{package}_v07")
        assert original.keys() == converted.keys() == regenerated.keys()
        assert converted == regenerated
        assert builder.source_digest(original) == report["legacy_sha256"]
        assert builder.source_digest(converted) == report["v07_sha256"]
        assert len(converted) == report["records"]
        for path in original:
            before, old_body = split_envelope(original[path], path)
            after, new_body = split_envelope(converted[path], path)
            assert old_body == new_body
            assert after["schema"] == "wedl/v0.7"
            if before["kind"] == "world":
                assert after["capabilities"] == builder.CAPABILITIES
            else:
                assert "capabilities" not in after
            if before["kind"] == "object":
                if "capabilities" in before:
                    authored = before["capabilities"]
                    assert after["object_affordances"] == authored
                    counts["fields"] += 1
                    counts["empty" if not authored else "nonempty"] += 1
                    tokens.update(authored)
                else:
                    assert "object_affordances" not in after
            else:
                assert "object_affordances" not in after

        root = tmp_path / package
        if package == "chronology_conformance":
            root.mkdir()
            shutil.copytree(data_root / f"{package}_v07" / "story", root / "story")
            (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
            git(root, "init", "-q")
            git(root, "config", "user.name", "wedl test")
            git(root, "config", "user.email", "wedl@test.invalid")
            git(root, "add", "story", ".gitignore")
            git(root, "commit", "-qm", "seed")
            repository = Repository(root)
            compiled = compile_world(repository, profile_name="fts")
        else:
            example = "ash-archive-v07" if package == "ash_archive" else "frontiersmen-v07"
            bootstrap = initialize(root, example=example, profile_name="fts")
            compiled = bootstrap["compile"]
            repository = Repository(root)
        assert compiled["recordCount"] == report["records"]
        source_world = repository.load_world(cache_write=False)
        assert not [item for item in validate_world(source_world) if item["severity"] == "error"]
        database = Path(compiled["database"])
        compiled_world = world_from_database(repository, database)
        assert set(source_world.records) == set(compiled_world.records)
        for identifier, source_record in source_world.records.items():
            built_record = compiled_world.records[identifier]
            assert source_record.frontmatter == built_record.frontmatter
            assert source_record.body == built_record.body
            assert source_record.source_path == built_record.source_path
        database.unlink()
        assert cache_readiness(repository)["state"] != "ready"
        rebuilt = compile_world(repository, profile_name="fts")
        assert rebuilt["recordCount"] == report["records"]
        cache_states[package] = cache_readiness(repository)["state"]
        assert cache_states[package] == "ready"
        rebuilt_world = world_from_database(repository, Path(rebuilt["database"]))
        assert set(rebuilt_world.records) == set(source_world.records)
        for identifier, source_record in source_world.records.items():
            assert rebuilt_world.records[identifier].frontmatter == source_record.frontmatter
            assert rebuilt_world.records[identifier].body == source_record.body
    assert counts == {"fields": 53, "empty": 25, "nonempty": 28}
    assert len(tokens) == 19
    print(json.dumps({"reports": reports, "affordances": counts, "tokens": len(tokens),
                      "cacheStates": cache_states, "elapsedSeconds": round(time.perf_counter() - started, 3)},
                     sort_keys=True))


def test_semantic_projection_preserves_absent_empty_order_and_duplicates() -> None:
    base = {"schema": "wedl/v0.3", "kind": "object", "id": "obj_00000000000000000000000001"}
    for authored in (None, [], ["open", "close", "open"]):
        before = {**base}
        after = {**base, "schema": "wedl/v0.7"}
        if authored is not None:
            before["capabilities"] = authored
            after["object_affordances"] = authored.copy()
        builder.assert_semantic_record("story/object.md", before, after, "body", "body")
        if authored is not None:
            after["object_affordances"] = authored[:-1] if authored else ["unexpected"]
        else:
            after["object_affordances"] = []
        with pytest.raises(ValueError, match="changed record semantics"):
            builder.assert_semantic_record("story/object.md", before, after, "body", "body")
    world = {"schema": "wedl/v0.7", "kind": "world", "capabilities": list(reversed(builder.CAPABILITIES))}
    with pytest.raises(ValueError, match="invalid world capability order"):
        builder.assert_semantic_record("story/world.md", {"kind": "world"}, world, "body", "body")


def test_check_reports_drift_without_modifying_packages(tmp_path: Path) -> None:
    data = tmp_path / "data"
    source = ROOT / "src" / "wedl" / "data" / "chronology_conformance"
    shutil.copytree(source, data / source.name)
    assert builder.run(write=True, names=(source.name,), data_root=data)[0]["records"] == 7
    world = data / "chronology_conformance_v07" / "story" / "world.md"
    world.write_bytes(world.read_bytes().replace(b"spatial-core-v1", b"route-v1"))
    damaged = world.read_bytes()
    with pytest.raises(ValueError, match="differ from conversion"):
        builder.run(write=False, names=(source.name,), data_root=data)
    assert world.read_bytes() == damaged


def test_mixed_schema_fails_before_publication(tmp_path: Path) -> None:
    data = tmp_path / "data"
    source = ROOT / "src" / "wedl" / "data" / "chronology_conformance"
    shutil.copytree(source, data / source.name)
    record = next((data / source.name / "story").rglob("*.md"))
    record.write_bytes(record.read_bytes().replace(b"wedl/v0.6", b"wedl/v0.3"))
    with pytest.raises(ValueError, match="conversion refused"):
        builder.run(write=True, names=(source.name,), data_root=data)
    assert not (data / "chronology_conformance_v07").exists()


def test_missing_legacy_record_fails_before_publication(tmp_path: Path) -> None:
    data = tmp_path / "data"
    source = ROOT / "src" / "wedl" / "data" / "ash_archive"
    shutil.copytree(source, data / source.name)
    (data / source.name / "story" / "objects" / "red-notebook.md").unlink()
    with pytest.raises(ValueError, match="source differs from pinned legacy package"):
        builder.run(write=True, names=(source.name,), data_root=data)
    assert not (data / "ash_archive_v07").exists()


def test_symlinked_destination_cannot_overwrite_pinned_source(tmp_path: Path) -> None:
    data = tmp_path / "data"
    source = ROOT / "src" / "wedl" / "data" / "chronology_conformance"
    shutil.copytree(source, data / source.name)
    legacy_world = data / source.name / "story" / "world.md"
    original = legacy_world.read_bytes()
    destination = data / "chronology_conformance_v07"
    destination.mkdir()
    link = destination / "story"
    if os.name == "nt":
        result = subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(legacy_world.parent)],
                                capture_output=True, text=True, check=False)
        assert result.returncode == 0, result.stderr
    else:
        link.symlink_to(legacy_world.parent, target_is_directory=True)
    with pytest.raises(ValueError, match="symlinked destination"):
        builder.run(write=True, names=(source.name,), data_root=data)
    assert legacy_world.read_bytes() == original


def test_hardlinked_destination_cannot_overwrite_pinned_source(tmp_path: Path) -> None:
    data = tmp_path / "data"
    source = ROOT / "src" / "wedl" / "data" / "chronology_conformance"
    shutil.copytree(source, data / source.name)
    legacy_world = data / source.name / "story" / "world.md"
    original = legacy_world.read_bytes()
    destination = data / "chronology_conformance_v07" / "story"
    destination.mkdir(parents=True)
    os.link(legacy_world, destination / "world.md")
    with pytest.raises(ValueError, match="hard-linked destination"):
        builder.run(write=True, names=(source.name,), data_root=data)
    assert legacy_world.read_bytes() == original


def test_small_packaged_upgrade_stale_head_and_forward_rollback(tmp_path: Path) -> None:
    root = tmp_path / "chronology"
    root.mkdir()
    data = ROOT / "src" / "wedl" / "data"
    shutil.copytree(data / "chronology_conformance" / "story", root / "story")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed")
    repository = Repository(root)
    original = repository.snapshot("HEAD").files
    old_head = repository.head()
    request = {"protocol": migration.PROTOCOL, "mode": "upgrade-v07",
               "expectedHead": old_head, "idempotencyKey": "small-packaged-upgrade"}
    plan = migration.preview(repository, request)
    assert plan["valid"] and not plan["noOp"]
    assert repository.snapshot("HEAD").files == original
    result = migration.apply(repository, {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]},
                             confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "committed"
    assert repository.snapshot("HEAD").files == builder._story_files(data / "chronology_conformance_v07")
    with pytest.raises(StaleRevision):
        migration.preview(repository, {**request, "idempotencyKey": "stale-after-upgrade"})
    rollback = {"protocol": migration.PROTOCOL, "mode": "rollback",
                "expectedHead": repository.head(), "idempotencyKey": "small-packaged-rollback",
                "rollbackBackupRef": plan["backupRef"]}
    restore = migration.preview(repository, rollback)
    assert restore["valid"] and not restore["noOp"]
    reverted = migration.apply(repository, {**rollback, "sourceSnapshotHash": restore["sourceSnapshotHash"]},
                               confirmation_token_value=restore["confirmationToken"])
    assert reverted["status"] == "committed"
    assert repository.snapshot("HEAD").files == original
    rebuilt = compile_world(repository, force=True, profile_name="fts")
    assert rebuilt["recordCount"] == len(original)
    assert cache_readiness(repository)["state"] == "ready"


def test_explicit_bootstrap_choices_keep_legacy_default() -> None:
    assert CLI_EXAMPLES == PARSER_EXAMPLES
    assert PARSER_EXAMPLES == {
        "ash-archive": "ash_archive",
        "ash-archive-v07": "ash_archive_v07",
        "frontiersmen": "frontiersmen",
        "frontiersmen-v07": "frontiersmen_v07",
    }
    assert parser().parse_args(["init", "sample"]).example == "ash-archive"
    assert parser().parse_args(["init", "sample", "--example", "frontiersmen-v07"]).example == "frontiersmen-v07"
