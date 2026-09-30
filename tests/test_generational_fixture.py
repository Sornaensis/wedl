"""Bounded correctness and transport checks for the authored fixture builder."""
from __future__ import annotations

from collections import Counter
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess

from fastapi.testclient import TestClient

from tools.generate_generational_fixture import (build_fixture,
    main as generate_fixture, source_digest)
from wedl.cli import main
from wedl.compiler import (INDEX_DDL, _bootstrap_compiled_connection, _insert_entities,
                           cache_readiness, compile_world)
from wedl.generational_api import execute
from wedl.generational_context import build_generational_context
from wedl.generational_index import insert_generational_index
from wedl.generational_query import TrustedViewerScope, query_connection, query_generational
from wedl.model import StoryTime
from wedl.repository import Repository
from wedl.source import parse_record, serialize_record
from wedl.validation import validate_world


def _scope(revision: str, tick: int, order: int = 0, *,
           mode: str = "author-as-of", private: bool = True,
           character: str | None = None) -> TrustedViewerScope:
    return TrustedViewerScope(revision, mode, "main",
        None if mode == "author-all-time" else StoryTime("main", tick, order),
        frozenset({"public", "council", "archivist"} if private else {"public"}),
        frozenset({"ordinary", "council-ledger", "archive"} if private else {"ordinary"}),
        frozenset({"generational-core-v1"}), character)


def _seed(repository: Repository, world) -> None:
    files = {path.relative_to(repository.root).as_posix(): None
             for path in (repository.root / "story").rglob("*.md")}
    files.update({record.source_path: serialize_record(
        record.frontmatter, f"# {record.title}\n") for record in world.records.values()})
    repository.commit_files(expected_head=repository.head(), files=files,
                            message="seed bounded generational fixture")


def _wire(repository: Repository, operation: str, subject: str, *, tick: int = 0) -> dict:
    return {"protocol": "wedl-generational/v1", "operation": operation,
            "revision": repository.head(), "capabilities": ["generational-core-v1"],
            "mode": "author-as-of", "timeline": "main",
            "at": {"timeline": "main", "tick": str(tick), "order": "0"},
            "subject": subject, "items": 20, "depth": 3}


def test_bounded_builder_is_valid_reproducible_and_linked(
        tmp_path: Path, monkeypatch, capsys) -> None:
    first, manifest = build_fixture(generations=4, width=4)
    second, repeated = build_fixture(generations=4, width=4)
    assert validate_world(first) == []
    assert manifest == repeated and source_digest(first) == source_digest(second)
    assert manifest["counts"] == {"characters": 29, "generations": 4,
                                  "kinshipEdges": 28, "transitions": 14}
    assert Counter(record.kind for record in first.records.values())["organization"] >= 5
    assert all(record.frontmatter["schema"] == "wedl/v0.7"
               for record in first.records.values())

    minimum, minimum_manifest = build_fixture(generations=3, width=3)
    repeated_minimum, repeated_manifest = build_fixture(generations=3, width=3)
    assert validate_world(minimum) == validate_world(repeated_minimum) == []
    assert minimum_manifest == repeated_manifest
    assert source_digest(minimum) == source_digest(repeated_minimum)
    for record in minimum.records.values():
        if record.kind == "union":
            participants = record.frontmatter["participant_ids"]
            assert len(participants) >= 2 and participants == sorted(set(participants))
            assert record.frontmatter["initialization"]["payload"]["participant_ids"] == participants
            assert all(transition["payload"]["participant_ids"] == participants
                       for transition in record.frontmatter["transitions"])

    destination = tmp_path / "minimum-cli"
    monkeypatch.setattr("sys.argv", ["generate_generational_fixture.py", str(destination),
                                   "--generations", "3", "--width", "3"])
    generate_fixture()
    assert json.loads(capsys.readouterr().out) == {
        **minimum_manifest, "sourceSha256": source_digest(minimum)}
    written = {path.relative_to(destination).as_posix(): path.read_bytes()
               for path in destination.rglob("*.md")}
    assert set(written) == {record.source_path for record in minimum.records.values()}
    for record in minimum.records.values():
        raw = written[record.source_path]
        assert raw == serialize_record(record.frontmatter, f"# {record.title}\n")
        assert parse_record(raw, record.source_path).frontmatter == record.frontmatter


def test_bounded_source_compiled_privacy_horizon_and_confirmed_transports(
        task91_repo: Repository, tmp_path: Path, capsys) -> None:
    world, manifest = build_fixture(generations=4, width=4)
    _seed(task91_repo, world)
    revision = task91_repo.head()
    built = compile_world(task91_repo)
    assert built["status"] == "compiled" and built["recordCount"] == len(world.records)
    assert cache_readiness(task91_repo, revision)["state"] == "ready"

    # Independently project the authored source and compare it with the pinned,
    # strict compiled repository path, including literal citation paths.
    source = sqlite3.connect(":memory:")
    source.row_factory = sqlite3.Row
    try:
        _bootstrap_compiled_connection(source)
        _insert_entities(source, world)
        source.executescript(INDEX_DDL)
        insert_generational_index(source, world, StoryTime("main", 0, 0))
        matrix = [
            (_scope(revision, 0), {"operation": "parents", "subject_id": manifest["answers"]["last"], "items": 8}),
            (_scope(revision, 0), {"operation": "ancestors", "subject_id": manifest["answers"]["last"], "items": 20, "depth": 2}),
            (_scope(revision, 0), {"operation": "descendants", "subject_id": manifest["answers"]["first"], "items": 20, "depth": 1}),
            (_scope(revision, -100), {"operation": "organization", "subject_id": manifest["answers"]["dynasty"], "items": 20}),
            (_scope(revision, 0), {"operation": "organization", "subject_id": manifest["answers"]["dynasty"], "items": 20}),
            (_scope(revision, 0), {"operation": "legacy", "subject_id": manifest["answers"]["legacy"], "items": 20}),
            (_scope(revision, 0, mode="author-all-time"), {"operation": "legacy", "subject_id": manifest["answers"]["legacy"], "items": 20}),
            (_scope(revision, 0), {"operation": "parents", "subject_id": manifest["answers"]["futureChild"]}),
            (_scope(revision, 6), {"operation": "parents", "subject_id": manifest["answers"]["futureChild"]}),
            (_scope(revision, -10, 0), {"operation": "parents", "subject_id": next(
                record.frontmatter["child_id"] for record in world.records.values()
                if record.title == "Child of the Archive")}),
        ]
        for scope, request in matrix:
            expected = query_connection(source, scope, request)
            assert query_generational(task91_repo, scope, request, require_compiled=True) == expected
            if expected["state"] == "available":
                assert "story/" in json.dumps(expected)
        last = matrix[0][1]
        parents = query_connection(source, _scope(revision, 0), last)
        assert {row["targetId"] for row in parents["relations"]} == set(manifest["answers"]["lastParents"])
        assert matrix[7][0].at.tick == 0
        assert query_connection(source, *matrix[7])["state"] == "unknown"
        assert query_connection(source, *matrix[8])["state"] == "available"
        secret = {"operation": "parents", "subject_id": manifest["answers"]["secretChild"]}
        assert query_connection(source, _scope(revision, 0), secret)["state"] == "available"
        assert query_connection(source, _scope(revision, 0, private=False), secret)["state"] == "unknown"
        assert query_connection(source, _scope(revision, 0, mode="character",
            character=manifest["answers"]["secretChild"]), secret)["state"] == "unknown"
        for term in ("sealed", "future"):
            result = query_connection(source, _scope(revision, 0, private=False),
                                      {"operation": "search", "text": term, "items": 2})
            assert result["state"] == "available" and result["results"] == []
            assert result["cursor"] is None
    finally:
        source.close()
    packet = build_generational_context(task91_repo, _scope(revision, 0, private=False),
        manifest["answers"]["secretChild"], require_compiled=True)
    assert "Sealed lineage" not in json.dumps(packet)

    # Exercise the real CLI parser and authenticated HTTP authoring routes.
    child = next(record.id for record in world.records.values()
                 if record.kind == "character" and record.title == "Chronicle 000 001")
    parent = manifest["answers"]["first"]
    request_path = tmp_path / "intent.json"

    def intent(key: str, target: str) -> dict:
        return {"action": "generational.create", "expectedHead": task91_repo.head(),
                "idempotencyKey": key, "kind": "parentage", "title": f"Authored {key}",
                "audience": ["public"], "perspectives": ["ordinary"],
                "fields": {"child_id": target, "parent_id": parent},
                "payload": {"basis": "adoptive"},
                "at": {"timeline": "main", "tick": "1", "order": "0"}}

    # A root character is selected so the new edge cannot introduce a cycle.
    cli_intent = intent("fixture-cli", child)
    request_path.write_text(json.dumps(cli_intent), encoding="utf-8")
    assert main(["--compact", "author", "request", "preview", str(request_path),
                 "--repo", str(task91_repo.root)]) == 0
    preview = json.loads(capsys.readouterr().out)
    assert preview["preview"]["valid"]
    old = task91_repo.head()
    assert main(["--compact", "author", "request", "apply", str(request_path),
                 "--repo", str(task91_repo.root)]) == 2
    capsys.readouterr()
    assert task91_repo.head() == old
    assert subprocess.run(["git", "-C", str(task91_repo.root), "status", "--porcelain"],
                          capture_output=True, text=True, check=True).stdout == ""
    assert main(["--compact", "author", "request", "apply", str(request_path),
                 "--repo", str(task91_repo.root), "--confirm", preview["preview"]["confirmationToken"]]) == 0
    applied = json.loads(capsys.readouterr().out)
    assert applied["newHead"] == task91_repo.head() and applied["compile"]["status"] == "compiled"
    assert subprocess.run(["git", "-C", str(task91_repo.root), "rev-list", "--count",
                           f"{old}..HEAD"], capture_output=True, text=True, check=True).stdout.strip() == "1"
    assert main(["--compact", "author", "request", "apply", str(request_path),
                 "--repo", str(task91_repo.root), "--confirm", preview["preview"]["confirmationToken"]]) == 0
    assert json.loads(capsys.readouterr().out)["idempotentReplay"] is True
    assert execute(task91_repo, "parents", _wire(task91_repo, "parents", child, tick=1),
                   require_compiled=True)["state"] == "available"

    with TestClient(__import__("wedl.server", fromlist=["create_app"]).create_app(task91_repo.root)) as client:
        token = client.get("/api/session").json()["token"]
        headers = {"X-Wedl-Token": token}
        second_child = next(record.id for record in world.records.values()
                            if record.kind == "character" and record.title == "Chronicle 000 002")
        http_intent = intent("fixture-http", second_child)
        http_preview = client.post("/api/authoring/preview", json=http_intent, headers=headers)
        assert http_preview.status_code == 200, http_preview.text
        token2 = http_preview.json()["preview"]["confirmationToken"]
        before_http = task91_repo.head()
        missing = client.post("/api/authoring/apply", json=http_intent, headers=headers)
        assert missing.status_code == 400 and task91_repo.head() == before_http
        assert subprocess.run(["git", "-C", str(task91_repo.root), "status", "--porcelain"],
                              capture_output=True, text=True, check=True).stdout == ""
        confirmed = {**headers, "X-Wedl-Confirmation": token2}
        response = client.post("/api/authoring/apply", json=http_intent, headers=confirmed)
        assert response.status_code == 200, response.text
        assert response.json()["newHead"] == task91_repo.head()
        assert subprocess.run(["git", "-C", str(task91_repo.root), "rev-list", "--count",
                               f"{before_http}..HEAD"], capture_output=True, text=True,
                              check=True).stdout.strip() == "1"
        assert client.post("/api/authoring/apply", json=http_intent,
                           headers=confirmed).json()["idempotentReplay"] is True
        stale = {**http_intent, "idempotencyKey": "fixture-http-stale"}
        stale_response = client.post("/api/authoring/apply", json=stale, headers=confirmed)
        assert stale_response.status_code == 409
        assert task91_repo.head() == response.json()["newHead"]
        assert subprocess.run(["git", "-C", str(task91_repo.root), "status", "--porcelain"],
                              capture_output=True, text=True, check=True).stdout == ""
        read = _wire(task91_repo, "parents", second_child, tick=1)
        direct = execute(task91_repo, "parents", read, require_compiled=True)
        assert direct == client.post("/api/generational/parents", json=read,
                                     headers=headers).json()
        request_path.write_text(json.dumps(read), encoding="utf-8")
        assert main(["--compact", "generational", "parents", str(request_path),
                     "--repo", str(task91_repo.root), "--require-compiled"]) == 0
        assert json.loads(capsys.readouterr().out) == direct
        assert direct["state"] == "available"
        assert all(isinstance(row["citations"][0]["applicability"]["point"]["tick"], str)
                   for row in direct["relations"])
