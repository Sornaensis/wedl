from __future__ import annotations

import json
import inspect
from pathlib import Path
import subprocess
import sys

import pytest

from wedl import changeset
from wedl.compiler import AuthoringByteResult
from wedl.errors import RepositoryError
from wedl.repository import Repository
from wedl.transaction_recovery import TransactionJournal


def _request(repository, key: str, monkeypatch: pytest.MonkeyPatch) -> dict[str, object]:
    """Use a small validated-preview seam; commit and recovery remain real."""
    request = {
        "protocol": "wedl-changeset/v1",
        "expectedHead": repository.head(),
        "idempotencyKey": key,
        "summary": "journal fixture",
        "operations": [],
    }

    def preview(_repository, payload, **_kwargs):
        expected = _repository.head()
        request_hash = changeset._request_hash(payload)
        return {
            "valid": True,
            "expectedHead": expected,
            "requestHash": request_hash,
            "confirmationToken": changeset.confirmation_token(
                payload, request_hash=request_hash, expected_head=expected,
            ),
            "generatedIds": {}, "touchedEntityIds": [],
            "_changes": {"story/authoring-journal.md": b"# Journal fixture\n"},
            "_recordCount": 1,
        }

    monkeypatch.setattr(changeset, "preview", preview)
    monkeypatch.setattr(changeset, "_authoring_cache_preflight", lambda *_args: True)
    return request


def _compiled_bytes(repository, revision: str) -> AuthoringByteResult:
    return AuthoringByteResult(
        "compiled",
        {"status": "compiled", "revision": revision, "recordCount": 1},
        b"world-cache", b"revision-cache", b"vector-cache",
    )


def _journal_repository(tmp_path: Path) -> Repository:
    """A real Git root is sufficient because this test stubs source loading."""
    root = tmp_path / "journal"
    root.mkdir()
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", ".gitignore"), ("commit", "-qm", "seed journal")):
        subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def test_changeset_journal_surfaces_deferral_recovery_and_replay(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Exercise all transaction outcomes in one isolated Git world."""
    ash_repo = _journal_repository(tmp_path)
    calls: list[tuple[str, ...]] = []
    original = TransactionJournal.register_surfaces

    def reject_cache(self, enrollments, *, live_budget):
        paths = tuple(surface.path for surface in enrollments)
        calls.append(paths)
        if any(surface.role == "cache" for surface in enrollments):
            raise RepositoryError("transaction journal live-byte budget exceeded")
        return original(self, enrollments, live_budget=live_budget)

    monkeypatch.setattr(changeset, "compile_world_bytes", _compiled_bytes)
    monkeypatch.setattr(TransactionJournal, "register_surfaces", reject_cache)
    committed = changeset.apply(
        ash_repo, _request(ash_repo, "receipt-only", monkeypatch), allow_unconfirmed=True,
    )
    assert calls[0][-1] == ".wedl/idempotency.json"
    assert calls[1] == (".wedl/idempotency.json",)
    assert committed["compile"]["status"] == "deferred-to-restart"
    assert committed["compile"]["reason"] == "authoring-process-memory-limit"
    assert not (ash_repo.root / ".wedl" / "world.sqlite").exists()
    assert not (ash_repo.root / ".wedl" / "vector-cache-v2.sqlite").exists()
    assert (ash_repo.root / ".wedl" / "idempotency.json").is_file()

    seen: list[tuple[object, ...]] = []

    def capture(self, enrollments, *, live_budget):
        seen.append(tuple(enrollments))
        return original(self, enrollments, live_budget=live_budget)

    monkeypatch.setattr(TransactionJournal, "register_surfaces", capture)
    request = _request(ash_repo, "journal-byte-surfaces", monkeypatch)
    head_before = ash_repo.head()
    original_publish = TransactionJournal.publish_surfaces
    failed = [False]

    def fail_publish(self):
        if not failed[0]:
            failed[0] = True
            raise RuntimeError("injected publication failure")
        return original_publish(self)

    monkeypatch.setattr(changeset, "compile_world_bytes", _compiled_bytes)
    monkeypatch.setattr(TransactionJournal, "publish_surfaces", fail_publish)
    with pytest.raises(RuntimeError, match="injected publication failure"):
        changeset.apply(ash_repo, request, allow_unconfirmed=True)

    assert ash_repo.head() == head_before
    assert not (ash_repo.root / ".wedl" / "world.sqlite").exists()
    receipts = json.loads((ash_repo.root / ".wedl" / "idempotency.json").read_text())
    assert "journal-byte-surfaces" not in receipts

    monkeypatch.setattr(TransactionJournal, "publish_surfaces", original_publish)
    committed = changeset.apply(ash_repo, request, allow_unconfirmed=True)
    assert committed["newHead"] != head_before
    surfaces = {surface.path: surface for surface in seen[-1]}
    assert set(surfaces) == {
        ".wedl/world.sqlite", f".wedl/revisions/{committed['newHead']}.sqlite",
        ".wedl/vector-cache-v2.sqlite", ".wedl/idempotency.json",
    }
    assert all(surface.after is None or type(surface.after) is bytes for surface in surfaces.values())
    assert json.loads((ash_repo.root / ".wedl" / "idempotency.json").read_text())["journal-byte-surfaces"]["result"] == committed
    replay = changeset.apply(ash_repo, request, allow_unconfirmed=True)
    assert replay["idempotentReplay"] is True and replay["newHead"] == committed["newHead"]


def test_receipt_has_no_post_journal_write_and_reserves_caller_memory(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "one-journal-write", monkeypatch)
    monkeypatch.setattr(changeset, "compile_world_bytes", _compiled_bytes)
    original = TransactionJournal.register_surfaces
    reserves: list[int] = []

    def capture(self, enrollments, *, live_budget):
        reserves.append(live_budget.caller_reserve_bytes)
        assert live_budget.total_bytes == 512 * 1024 * 1024
        assert all(surface.after is None or type(surface.after) is bytes for surface in enrollments)
        return original(self, enrollments, live_budget=live_budget)

    def forbidden_extra_write(_path, _data):
        raise AssertionError("receipt writer bypassed transaction journal")

    monkeypatch.setattr(TransactionJournal, "register_surfaces", capture)
    monkeypatch.setattr(changeset, "atomic_write", forbidden_extra_write)
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert committed["status"] == "committed"
    assert reserves and reserves[0] >= 96 * 1024 * 1024
    receipt = json.loads((repository.root / ".wedl" / "idempotency.json").read_text())
    assert receipt["one-journal-write"]["result"] == committed
    shared_files = {
        path.relative_to(repository.root / ".wedl").as_posix()
        for path in (repository.root / ".wedl").rglob("*") if path.is_file()
    }
    assert shared_files == {
        "world.sqlite", "vector-cache-v2.sqlite", "idempotency.json",
        f"revisions/{committed['newHead']}.sqlite",
    }


def test_surface_publication_failure_restores_cache_and_receipt_together(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "partial-publication", monkeypatch)
    monkeypatch.setattr(changeset, "compile_world_bytes", _compiled_bytes)
    initial_head = repository.head()
    original = TransactionJournal._publish_claimed_surface_bounded
    failed = [False]

    def fail_after_first(self, *args, **kwargs):
        result = original(self, *args, **kwargs)
        if not failed[0]:
            failed[0] = True
            raise RuntimeError("injected partial surface publication")
        return result

    monkeypatch.setattr(TransactionJournal, "_publish_claimed_surface_bounded", fail_after_first)
    with pytest.raises(RuntimeError, match="injected partial surface publication"):
        changeset.apply(repository, request, allow_unconfirmed=True)
    assert failed[0] and repository.head() == initial_head
    assert not (repository.root / ".wedl" / "world.sqlite").exists()
    assert not (repository.root / ".wedl" / "vector-cache-v2.sqlite").exists()
    assert not (repository.root / ".wedl" / "idempotency.json").exists()
    assert not list((repository.root / ".wedl" / "revisions").glob("*.sqlite"))

    monkeypatch.setattr(TransactionJournal, "_publish_claimed_surface_bounded", original)
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert (repository.root / ".wedl" / "world.sqlite").read_bytes() == b"world-cache"
    assert (repository.root / ".wedl" / "idempotency.json").is_file()
    assert changeset.apply(repository, request, allow_unconfirmed=True)["newHead"] == committed["newHead"]


@pytest.mark.parametrize("limit", [96 * 1024 * 1024, 1])
def test_mandatory_enrollment_capacity_fails_before_ref(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, limit: int,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "must-fit", monkeypatch)
    initial_head = repository.head()
    monkeypatch.setattr(changeset, "compile_world_bytes", _compiled_bytes)
    if limit == 1:
        monkeypatch.setattr(changeset, "_AUTHORING_MAX_RECEIPT", limit)
        message = "idempotency receipt exceeds authoring byte limit"
    else:
        monkeypatch.setattr(changeset, "_AUTHORING_PROCESS_LIMIT", limit)
        message = "transaction journal live-byte budget exceeded"
    with pytest.raises(RepositoryError, match=message):
        changeset.apply(repository, request, allow_unconfirmed=True)
    assert repository.head() == initial_head
    assert not (repository.root / ".wedl" / "idempotency.json").exists()
    assert not (repository.root / ".wedl" / "world.sqlite").exists()


def test_oversized_author_impact_fails_before_preview_or_ref(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "large-impact", monkeypatch)
    initial_head = repository.head()
    monkeypatch.setattr(changeset, "_AUTHORING_MAX_RECEIPT", 128)

    def forbidden_preview(*_args, **_kwargs):
        raise AssertionError("oversized impact reached source construction")

    monkeypatch.setattr(changeset, "preview", forbidden_preview)
    with pytest.raises(RepositoryError, match="idempotency receipt exceeds authoring byte limit"):
        changeset.apply(repository, request, allow_unconfirmed=True, authoring_impact={"body": "x" * 129})
    assert repository.head() == initial_head


def test_legacy_receipt_fault_hook_cannot_publish_a_partial_receipt(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "receipt-fault", monkeypatch)
    initial_head = repository.head()
    monkeypatch.setattr(changeset, "compile_world_bytes", _compiled_bytes)
    monkeypatch.setattr(
        changeset, "atomic_write",
        lambda *_args: (_ for _ in ()).throw(RuntimeError("injected receipt failure")),
    )
    with pytest.raises(RuntimeError, match="injected receipt failure"):
        changeset.apply(repository, request, allow_unconfirmed=True)
    assert repository.head() == initial_head
    assert not (repository.root / ".wedl" / "idempotency.json").exists()
    assert not (repository.root / ".wedl" / "world.sqlite").exists()


def test_keyless_deferred_commit_still_budgets_its_source_journal(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "unused", monkeypatch)
    request.pop("idempotencyKey")
    monkeypatch.setattr(
        changeset, "compile_world_bytes",
        lambda *_args: AuthoringByteResult(
            "deferred-to-restart",
            {"status": "deferred-to-restart", "reason": "sqlite-serialize-unavailable"},
        ),
    )
    original = TransactionJournal.register_surfaces
    admissions: list[tuple[object, ...]] = []

    def capture(self, enrollments, *, live_budget):
        admissions.append((tuple(enrollments), live_budget))
        return original(self, enrollments, live_budget=live_budget)

    monkeypatch.setattr(TransactionJournal, "register_surfaces", capture)
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert committed["status"] == "committed"
    assert committed["compile"]["reason"] == "sqlite-serialize-unavailable"
    assert len(admissions) == 1
    surfaces, budget = admissions[0]
    assert len(surfaces) == 1
    assert surfaces[0].path == ".wedl/idempotency.json"
    assert surfaces[0].before is surfaces[0].after is None
    assert budget.total_bytes == 512 * 1024 * 1024
    assert budget.caller_reserve_bytes >= 96 * 1024 * 1024
    assert not (repository.root / ".wedl" / "idempotency.json").exists()


@pytest.mark.parametrize("source_size", [None, 16 * 1024 * 1024 + 1])
def test_source_capacity_rejected_before_preview(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, source_size: int | None,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "oversized-source", monkeypatch)
    initial_head = repository.head()
    monkeypatch.setattr(changeset, "_authoring_source_metadata_bytes", lambda *_args: source_size)

    def forbidden_preview(*_args, **_kwargs):
        raise AssertionError("source constructed before source admission")

    monkeypatch.setattr(changeset, "preview", forbidden_preview)
    with pytest.raises(RepositoryError, match="authoring source byte limit exceeded"):
        changeset.apply(repository, request, allow_unconfirmed=True)
    assert repository.head() == initial_head


def test_commit_before_image_capacity_rejected_before_repository_commit(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    source = repository.root / "story" / "authoring-journal.md"
    source.parent.mkdir()
    source.write_bytes(b"123456789")
    subprocess.run(
        ["git", "-C", str(repository.root), "add", "story/authoring-journal.md"], check=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    subprocess.run(
        ["git", "-C", str(repository.root), "commit", "-qm", "seed source"], check=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    request = _request(repository, "large-before", monkeypatch)
    initial_head = repository.head()
    monkeypatch.setattr(changeset, "_authoring_source_metadata_bytes", lambda *_args: 0)
    monkeypatch.setattr(changeset, "_AUTHORING_MAX_SOURCE", 8)

    def forbidden_commit(**_kwargs):
        raise AssertionError("oversized before image reached commit_files")

    monkeypatch.setattr(repository, "commit_files", forbidden_commit)
    with pytest.raises(RepositoryError, match="authoring source byte limit exceeded"):
        changeset.apply(repository, request, allow_unconfirmed=True)
    assert repository.head() == initial_head


def test_oversized_request_rejects_before_hash_or_source_metadata(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    monkeypatch.setattr(changeset, "_AUTHORING_MAX_REQUEST", 128)

    def forbidden(*_args, **_kwargs):
        raise AssertionError("payload allocation preceded admission")

    monkeypatch.setattr(changeset, "_request_hash", forbidden)
    monkeypatch.setattr(changeset, "_authoring_source_metadata_bytes", forbidden)
    with pytest.raises(RepositoryError, match="authoring request byte limit exceeded"):
        changeset.apply(repository, {"summary": "x" * 129}, allow_unconfirmed=True)


def test_revision_enumeration_limit_defers_without_building_cache(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "bounded-revisions", monkeypatch)
    revisions = repository.root / ".wedl" / "revisions"
    revisions.mkdir(parents=True)
    (revisions / "old.sqlite").write_bytes(b"old")
    monkeypatch.setattr(changeset, "_AUTHORING_MAX_REVISIONS", 0)

    def forbidden(*_args, **_kwargs):
        raise AssertionError("cache compiler ran after revision admission failed")

    monkeypatch.setattr(changeset, "compile_world_bytes", forbidden)
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert committed["status"] == "committed"
    assert committed["compile"]["reason"] == "authoring-revision-enumeration-limit"
    assert not (repository.root / ".wedl" / "world.sqlite").exists()


def test_non_cache_revision_entries_count_toward_scan_limit(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "bounded-non-cache-entries", monkeypatch)
    revisions = repository.root / ".wedl" / "revisions"
    revisions.mkdir(parents=True)
    for index in range(12):
        (revisions / f"unrelated-{index}.txt").write_bytes(b"x")
    monkeypatch.setattr(changeset, "_AUTHORING_MAX_REVISIONS", 3)
    monkeypatch.setattr(changeset, "compile_world_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(
        AssertionError("cache compiler ran after non-cache entry scan overflow")
    ))
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert committed["status"] == "committed"
    assert committed["compile"]["reason"] == "authoring-revision-enumeration-limit"
    assert not (repository.root / ".wedl" / "world.sqlite").exists()


def test_oversized_existing_cache_defers_before_compiler_construction(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "large-cache-before", monkeypatch)
    world_cache = repository.root / ".wedl" / "world.sqlite"
    world_cache.parent.mkdir(exist_ok=True)
    world_cache.write_bytes(b"x" * 9)
    monkeypatch.setattr(changeset, "_AUTHORING_MAX_CACHE_BEFORE", 8)
    monkeypatch.setattr(changeset, "compile_world_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(
        AssertionError("cache compiler ran after before-image overflow")
    ))
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert committed["status"] == "committed"
    assert committed["compile"]["reason"] == "authoring-cache-before-size-limit"
    assert world_cache.read_bytes() == b"x" * 9


def test_compiler_and_journal_phase_ledger_boundary(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    monkeypatch.setattr(changeset, "_authoring_preconstruction_bytes", lambda *_args: 384 * 1024 * 1024)
    mebibyte = 1024 * 1024
    phase: list[int] = []
    assert changeset._authoring_cache_preflight(repository, repository.head(), {}, 128 * mebibyte - 1, phase)
    assert phase == [512 * mebibyte - 1]
    assert changeset._authoring_phase_peak(phase[0], 512 * mebibyte - 2) == 512 * mebibyte - 1
    assert not changeset._authoring_cache_preflight(repository, repository.head(), {}, 128 * mebibyte)
    assert changeset._authoring_phase_peak(phase[0], 512 * mebibyte + 1) > 512 * mebibyte


def test_receipt_only_retry_drops_rejected_cache_graph_before_readmission(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    repository = _journal_repository(tmp_path)
    request = _request(repository, "released-cache", monkeypatch)
    def compiled_bytes(_repository, revision):
        world = b"w" * (2 * 1024 * 1024)
        return AuthoringByteResult(
            "compiled", {"status": "compiled", "revision": revision, "recordCount": 1},
            world, world, b"v" * 1024 * 1024,
        )

    monkeypatch.setattr(changeset, "compile_world_bytes", compiled_bytes)
    original = TransactionJournal.register_surfaces
    attempts = [0]

    def reject_then_inspect(self, enrollments, *, live_budget):
        attempts[0] += 1
        if attempts[0] == 1:
            assert any(surface.role == "cache" for surface in enrollments)
            raise RepositoryError("transaction journal live-byte budget exceeded")
        caller = inspect.currentframe().f_back.f_locals
        assert sys.exc_info()[1] is None
        assert caller["cache_enrollments"] == ()
        assert all(surface.role == "receipt" for surface in caller["enrollments"])
        assert all(surface.role == "receipt" for surface in enrollments)
        assert live_budget.caller_reserve_bytes >= 96 * 1024 * 1024
        return original(self, enrollments, live_budget=live_budget)

    monkeypatch.setattr(TransactionJournal, "register_surfaces", reject_then_inspect)
    committed = changeset.apply(repository, request, allow_unconfirmed=True)
    assert attempts == [2]
    assert committed["compile"]["reason"] == "authoring-process-memory-limit"
