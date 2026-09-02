from __future__ import annotations

import copy
from pathlib import Path
import json
import os
import stat
import subprocess
import sys
import time
import tracemalloc

import pytest
import wedl.transaction_recovery as transaction_recovery

from wedl.errors import RepositoryError
from wedl.repository import Repository
from wedl.transaction_recovery import JournalLiveByteBudget, Surface, SurfaceEnrollment, TransactionJournal


def _repository(tmp_path: Path) -> Repository:
    root = tmp_path / "world"
    (root / "story").mkdir(parents=True)
    (root / "story" / "world.md").write_bytes(b"before\n")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def _require_bounded_no_follow() -> None:
    if os.name == "nt":
        return
    if os.name != "posix" or not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_DIRECTORY"):
        pytest.skip("bounded capture fails closed without descriptor-relative no-follow support")


def _journal_record(path: Path) -> dict[str, object]:
    payload = path.read_bytes()
    payload = transaction_recovery._budgeted_document(payload)
    value = json.loads(payload.decode("utf-8"))
    assert isinstance(value, dict)
    return value


def _budgeted_payload(record: dict[str, object]) -> bytes:
    return transaction_recovery._canonical_journal_payload(record)


def test_registered_surfaces_are_durable_idempotent_and_exact(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    receipt = root / ".wedl" / "receipts.json"; receipt.parent.mkdir()
    receipt.write_bytes(b"old receipt")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/receipts.json", capture_before=True, after=b"new receipt")
    journal.register_surface(".wedl/revisions/new.sqlite", before=None, after=b"compiled")
    journal.publish_surfaces(); journal.publish_surfaces()
    reloaded = TransactionJournal.load(root, journal.path)
    assert receipt.read_bytes() == b"new receipt"
    assert (root / ".wedl/revisions/new.sqlite").read_bytes() == b"compiled"
    reloaded.restore_surfaces(); reloaded.restore_surfaces()
    assert receipt.read_bytes() == b"old receipt"
    assert not (root / ".wedl/revisions/new.sqlite").exists()
    reloaded.finish(); reloaded.cleanup()
    assert TransactionJournal.pending(root) == ()


def test_bounded_before_image_at_cap_restores_exactly(tmp_path: Path) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir()
    target.write_bytes(b"exact")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=5, after=b"new", role="cache")
    assert journal.surfaces[0].before == b"exact"
    journal.publish_surfaces(); journal.restore_surfaces()
    assert target.read_bytes() == b"exact"


@pytest.mark.parametrize("limit", (-1, True, "5"))
def test_bounded_before_image_rejects_invalid_limits_without_persisting(tmp_path: Path, limit) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"old")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError, match="capture limit"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=limit, after=b"new")
    assert journal.surfaces == () and target.read_bytes() == b"old"
    with pytest.raises(RepositoryError, match="capture limit"):
        journal.register_surface(".wedl/cache.sqlite", max_before_bytes=5, after=b"new")
    with pytest.raises(RepositoryError, match="cannot be supplied"):
        journal.register_surface(".wedl/cache.sqlite", before=b"old", capture_before=True, max_before_bytes=5, after=b"new")


def test_bounded_before_image_rejects_large_sparse_file_before_any_read(tmp_path: Path, monkeypatch) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir()
    with target.open("wb") as output:
        output.seek(4 * 1024 * 1024)
        output.write(b"x")
    reads: list[int] = []
    actual_read = transaction_recovery.os.read

    def tracked_read(descriptor, count):
        reads.append(count)
        return actual_read(descriptor, count)

    monkeypatch.setattr(transaction_recovery.os, "read", tracked_read)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError, match="exceeds capture limit"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=1024, after=b"new")
    assert reads == [] and journal.surfaces == () and target.stat().st_size == 4 * 1024 * 1024 + 1


def test_bounded_before_image_reads_only_fixed_chunks_and_detects_growth(tmp_path: Path, monkeypatch) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"x" * 1024)
    actual_read = transaction_recovery.os.read
    requests: list[int] = []
    grew = False

    def grow_after_first_read(descriptor, count):
        nonlocal grew
        result = actual_read(descriptor, count)
        requests.append(count)
        if result and not grew:
            grew = True
            with target.open("ab") as output:
                output.write(b"!")
        return result

    monkeypatch.setattr(transaction_recovery.os, "read", grow_after_first_read)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError, match="before image is unstable"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=1024, after=b"new")
    assert requests and max(requests) <= 1024 and journal.surfaces == () and target.read_bytes().endswith(b"!")


def test_bounded_before_image_rejects_replaced_or_nonregular_path_without_persisting(tmp_path: Path, monkeypatch) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"old")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    monkeypatch.setattr(transaction_recovery, "_path_has_identity", lambda *_args: False)
    with pytest.raises(RepositoryError, match="before image is unstable"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=3, after=b"new")
    assert journal.surfaces == () and target.read_bytes() == b"old"
    monkeypatch.undo()
    target.unlink(); target.mkdir()
    with pytest.raises(RepositoryError, match="regular file"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=3, after=b"new")
    assert journal.surfaces == ()


def test_bounded_before_image_rejects_same_name_replacement_before_persist(tmp_path: Path, monkeypatch) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"old")
    replacement = root / ".wedl" / "replacement.sqlite"; replacement.write_bytes(b"external")
    actual_read = transaction_recovery.os.read
    replaced = False

    def replace_after_read(descriptor, count):
        nonlocal replaced
        result = actual_read(descriptor, count)
        if result and not replaced:
            replaced = True
            try:
                os.replace(replacement, target)
            except OSError:
                pytest.skip("Windows test host does not permit replacement of an open read handle")
        return result

    monkeypatch.setattr(transaction_recovery.os, "read", replace_after_read)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError, match="before image is unstable"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=3, after=b"new")
    assert journal.surfaces == () and target.read_bytes() == b"external"


def test_bounded_before_image_rejects_symlink_without_touching_external_file(tmp_path: Path) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir()
    outside = tmp_path / "outside.sqlite"; outside.write_bytes(b"external")
    try:
        target.symlink_to(outside)
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError, match="reparse point|regular file"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=8, after=b"new")
    assert journal.surfaces == () and outside.read_bytes() == b"external"


def test_bounded_before_image_rechecks_same_byte_identity_after_persist_hook(tmp_path: Path) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"same")
    replacement = root / ".wedl" / "external.sqlite"; replacement.write_bytes(b"same")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal_before = journal.path.read_bytes()

    def replace_at_persist(name: str) -> None:
        if name == "journal-persist":
            os.replace(replacement, target)

    transaction_recovery.set_mutation_hook(replace_at_persist)
    try:
        with pytest.raises(RepositoryError, match="before image is unstable"):
            journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=4, after=b"new")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"same"
    assert journal.surfaces == () and journal.path.read_bytes() == journal_before


def test_bounded_before_image_rejects_parent_substitution_before_payload_read(tmp_path: Path, monkeypatch) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; nested = root / ".wedl" / "nested"; nested.mkdir(parents=True)
    target = nested / "cache.sqlite"; target.write_bytes(b"old")
    outside = tmp_path / "outside"; outside.mkdir()
    substituted = False

    def substitute_parent() -> None:
        nonlocal substituted
        if not substituted:
            substituted = True
            moved = nested.with_name("nested-external")
            nested.rename(moved)
            nested.symlink_to(outside, target_is_directory=True)

    if os.name == "nt":
        actual_safe_target = transaction_recovery._safe_target
        safe_calls = 0

        def substitute_after_windows_open(capture_root: Path, relative: str) -> Path:
            nonlocal safe_calls
            if relative == ".wedl/nested/cache.sqlite":
                safe_calls += 1
                # Metadata opens one protected handle between its first and
                # second walks; substitute the parent at that exact boundary.
                if safe_calls == 2:
                    substitute_parent()
            return actual_safe_target(capture_root, relative)

        monkeypatch.setattr(transaction_recovery, "_safe_target", substitute_after_windows_open)
    else:
        actual_open = transaction_recovery.os.open

        def substitute_after_posix_open(path, flags, *args, **kwargs):
            if path == "cache.sqlite":
                substitute_parent()
            return actual_open(path, flags, *args, **kwargs)

        monkeypatch.setattr(transaction_recovery.os, "open", substitute_after_posix_open)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError, match="unstable|reparse point|cannot be opened"):
        journal.register_surface(".wedl/nested/cache.sqlite", capture_before=True, max_before_bytes=3, after=b"new")
    assert journal.surfaces == () and not (outside / "cache.sqlite").exists()


def test_bounded_before_image_fails_closed_when_no_follow_walk_is_unavailable(tmp_path: Path) -> None:
    if os.name == "nt" or (os.name == "posix" and hasattr(os, "O_NOFOLLOW") and hasattr(os, "O_DIRECTORY")):
        pytest.skip("platform exercises handle- or descriptor-scoped bounded capture")
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"old")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal_before = journal.path.read_bytes()
    with pytest.raises(RepositoryError, match="descriptor no-follow"):
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, max_before_bytes=3, after=b"new")
    assert journal.surfaces == () and journal.path.read_bytes() == journal_before and target.read_bytes() == b"old"


def test_bounded_surface_identity_and_cap_survive_journal_reload(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl/cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"old")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", capture_before=True, max_before_bytes=8, after=b"new", role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    reloaded = TransactionJournal.load(root, journal.path)
    surface, = reloaded.surfaces
    assert surface.before == b"old" and surface.after == b"new" and surface.before_limit == 8
    assert surface.before_identity is not None


def test_bounded_matcher_uses_legacy_windows_mode_semantics(tmp_path: Path, monkeypatch) -> None:
    target = tmp_path / "created.txt"
    target.write_bytes(b"created")
    os.chmod(target, 0o666)

    if os.name != "nt":
        assert not transaction_recovery._bounded_path_matches_image(
            target, b"created", len(b"created"), mode=0o644,
        )
    monkeypatch.setattr(transaction_recovery.os, "name", "nt")
    assert transaction_recovery._bounded_path_matches_image(
        target, b"created", len(b"created"), mode=0o644,
    )


def test_real_v6_six_key_pending_journal_restarts_and_recovers(tmp_path: Path) -> None:
    """The immediately preceding wire format has no capture identity fields."""
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(
        root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o640, 0o600, (1, 2, 3), 99)],
    )
    raw = _journal_record(journal.path)
    assert raw["version"] == transaction_recovery.FORMAT_VERSION
    assert set(raw["surfaces"][0]) == {"path", "before", "after", "role", "beforeMode", "afterMode"}
    restarted, = TransactionJournal.pending(root)
    assert restarted.surfaces[0].before_identity is None and restarted.surfaces[0].before_limit is None
    restarted.publish_surfaces(); restarted.restore_surfaces(); restarted.finish(); restarted.cleanup()
    assert target.read_bytes() == b"before" and TransactionJournal.pending(root) == ()


def test_promoting_v6_surfaces_writes_v7_wire_and_survives_restart_publish_restore(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(
        root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source")],
    )
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    record = _journal_record(journal.path)
    assert record["version"] == transaction_recovery.BUDGETED_FORMAT_VERSION
    assert all(set(surface) == {"path", "before", "after", "role", "beforeMode", "afterMode", "beforeIdentity", "beforeLimit"}
               for surface in record["surfaces"])
    restarted = TransactionJournal.load(root, journal.path)
    restarted.publish_surfaces(); assert target.read_bytes() == b"after"
    restarted.restore_surfaces(); assert target.read_bytes() == b"before"


def test_budgeted_envelope_accepts_index_entries_literally_named_path(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    entry = {"mode": "100644", "stage": 0, "blob": "0" * 40}
    journal = TransactionJournal.create(
        root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        index_before={"path": entry}, index_after={"path": entry},
    )
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    assert TransactionJournal.load(root, journal.path).surfaces[0].path == ".wedl/cache.sqlite"


def test_v7_surface_wire_rejects_missing_capture_keys(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    record = _journal_record(journal.path)
    del record["surfaces"][0]["beforeIdentity"]
    journal.path.write_bytes(_budgeted_payload(record))
    with pytest.raises(RepositoryError, match="surface"):
        TransactionJournal.load(root, journal.path)


def test_batch_surface_admission_rejects_before_persisting_and_persists_once(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    before = journal.path.read_bytes(); generation = journal.record["generation"]
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces([SurfaceEnrollment(".wedl/a", after=b"x" * 32)], live_budget=JournalLiveByteBudget(1, 0))
    assert journal.path.read_bytes() == before and journal.record["generation"] == generation and journal.surfaces == ()
    peak = journal.register_surfaces([SurfaceEnrollment(".wedl/a", after=b"x"), SurfaceEnrollment(".wedl/b", after=b"y")],
                                     live_budget=JournalLiveByteBudget(16384, 0))
    reloaded = TransactionJournal.load(root, journal.path)
    assert peak == reloaded.record["liveByteBudget"]["admittedPeakBytes"] and len(reloaded.surfaces) == 2


def test_batch_capture_preflight_over_budget_reads_no_payload_and_does_not_persist(tmp_path: Path, monkeypatch) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / ".wedl" / "cache.sqlite"; target.parent.mkdir(); target.write_bytes(b"x" * 128)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal_bytes = journal.path.read_bytes()
    reads: list[int] = []
    persists: list[bool] = []
    actual_read = transaction_recovery.os.read
    actual_persist = TransactionJournal._persist

    def tracked_read(descriptor, count):
        reads.append(count)
        return actual_read(descriptor, count)

    def tracked_persist(self, *args, **kwargs):
        if self is journal:
            persists.append(True)
        return actual_persist(self, *args, **kwargs)

    monkeypatch.setattr(transaction_recovery.os, "read", tracked_read)
    monkeypatch.setattr(TransactionJournal, "_persist", tracked_persist)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces(
            [SurfaceEnrollment(".wedl/cache.sqlite", capture_before=True, max_before_bytes=128, after=b"new", role="cache")],
            live_budget=JournalLiveByteBudget(1, 0),
        )
    assert reads == [] and persists == []
    assert journal.path.read_bytes() == journal_bytes and journal.surfaces == ()


def test_budgeted_batch_keeps_total_monotonic_and_recomputes_reload_evidence(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/a", after=b"a")], live_budget=JournalLiveByteBudget(16384, 0))
    with pytest.raises(RepositoryError, match="requires bounded batch"):
        journal.register_surface(".wedl/b", after=b"b")
    with pytest.raises(RepositoryError, match="total cannot change"):
        journal.register_surfaces([SurfaceEnrollment(".wedl/b", after=b"b")], live_budget=JournalLiveByteBudget(32768, 0))
    record = _journal_record(journal.path)
    record["liveByteBudget"]["admittedPeakBytes"] = 0
    record["$liveByteBudget"]["admittedPeakBytes"] = 0
    journal.path.write_bytes(_budgeted_payload(record))
    with pytest.raises(RepositoryError):
        TransactionJournal.load(root, journal.path)


def test_budgeted_preflight_rejects_existing_encoded_images_before_decode_or_candidate_json(tmp_path: Path, monkeypatch) -> None:
    """A later batch must price retained images without decoding their base64."""
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    cap = 65_536
    journal.register_surfaces([SurfaceEnrollment(".wedl/retained", after=b"retained" * 64)], live_budget=JournalLiveByteBudget(cap, 0))
    before = journal.path.read_bytes(); generation = journal.record["generation"]
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("preflight decoded retained surface"))))
    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: (_ for _ in ()).throw(AssertionError("preflight built candidate JSON")))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces([SurfaceEnrollment(".wedl/new", after=b"new" * 32_768)], live_budget=JournalLiveByteBudget(cap, 0))
    assert journal.path.read_bytes() == before and journal.record["generation"] == generation


def test_budgeted_batch_rechecks_every_path_at_atomic_boundary(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    target = root / "story" / "world.md"; target.parent.mkdir(); target.write_bytes(b"old")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    durable = journal.path.read_bytes(); generation = journal.record["generation"]

    def replace_at_boundary(name: str) -> None:
        if name == "atomic-replace":
            target.unlink(); target.write_bytes(b"old")

    transaction_recovery.set_mutation_hook(replace_at_boundary)
    try:
        with pytest.raises(RepositoryError, match="before image is unstable"):
            journal.register_surfaces([SurfaceEnrollment("story/world.md", before=b"old", after=b"new", role="source")], live_budget=JournalLiveByteBudget(65536, 0))
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"old" and journal.path.read_bytes() == durable and journal.record["generation"] == generation


def test_budgeted_registration_is_frozen_after_prepared_phase(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.record["phase"] = "ref_committed"
    with pytest.raises(RepositoryError, match="frozen"):
        journal.register_surfaces([SurfaceEnrollment(".wedl/a", after=b"a")], live_budget=JournalLiveByteBudget(65536, 0))


def test_budgeted_persist_memory_error_restores_generation_and_record(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    durable = journal.path.read_bytes(); generation = journal.record["generation"]
    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: (_ for _ in ()).throw(MemoryError()))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal._persist()
    assert journal.record["generation"] == generation and journal.path.read_bytes() == durable and journal._journal_bytes == durable


def test_budgeted_phase_persist_rechecks_cap_before_durable_mutation(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65536, 0))
    durable = journal.path.read_bytes(); generation = journal.record["generation"]
    for name in ("liveByteBudget", "$liveByteBudget"):
        journal.record[name]["totalBytes"] = 1
        journal.record[name]["admittedPeakBytes"] = 1
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.advance("ref_committed")
    assert journal.phase == "prepared" and journal.record["generation"] == generation and journal.path.read_bytes() == durable
    # Every later allocation/namespace boundary uses the same limit-minus-one
    # guard before creating a lock/private artifact or touching a public image.
    for operation in ("private-artifact", "lock", "publish", "restore", "cleanup"):
        case_root = tmp_path / f"later-{operation}"; (case_root / "story").mkdir(parents=True)
        target = case_root / "story/world.md"; target.write_bytes(b"before")
        case = TransactionJournal.create(case_root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
        case.register_surfaces([SurfaceEnrollment("story/world.md", before=b"before", after=b"after", role="source")], live_budget=JournalLiveByteBudget(1_000_000, 0))
        if operation in {"restore", "cleanup"}:
            case.publish_surfaces()
        if operation == "cleanup":
            case.restore_surfaces(); case.finish()
        lock = case_root / ".git" / "wedl-canonical-write.lock"; lock.parent.mkdir(exist_ok=True)
        lock.write_text("pid=1\ntoken=11111111-1111-1111-1111-111111111111\n", encoding="ascii")
        path = {"private-artifact": "private.sqlite", "lock": str(lock), "publish": "", "restore": "", "cleanup": ""}[operation]
        limit = case._require_budgeted_operation(operation, path=path) - 1
        for name in ("liveByteBudget", "$liveByteBudget"):
            case.record[name]["totalBytes"] = limit
            case.record[name]["admittedPeakBytes"] = min(limit, case.record[name]["admittedPeakBytes"])
        case_durable = case.path.read_bytes(); target_before = target.read_bytes()
        with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
            if operation == "private-artifact":
                case.staging_path("private.sqlite")
            elif operation == "lock":
                case.bind_lock(lock, pid=1, token="11111111-1111-1111-1111-111111111111")
            elif operation == "publish":
                case.publish_surfaces()
            elif operation == "restore":
                case.restore_surfaces()
            else:
                case.cleanup()
        assert case.path.read_bytes() == case_durable and target.read_bytes() == target_before
        assert not (case.path.parent / case.transaction_id / "staging" / "private.sqlite").exists()


def test_budgeted_version_cannot_be_downgraded_to_legacy_with_budget_fields(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65536, 0))
    record = _journal_record(journal.path)
    record["version"] = transaction_recovery.FORMAT_VERSION
    journal.path.write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    with pytest.raises(RepositoryError, match="invalid|unsupported"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_version_rejects_stripped_budget_metadata_without_legacy_fallback(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65536, 0))
    record = _journal_record(journal.path)
    old_budget = dict(record["liveByteBudget"])
    del record["$liveByteBudget"]; del record["liveByteBudget"]
    document = (json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
    header = (
        f"total={old_budget['totalBytes']} reserve={old_budget['callerReserveBytes']} "
        f"admitted={old_budget['admittedPeakBytes']} surfaces={len(record['surfaces'])} "
        f"bytes={len(document)} sha256={transaction_recovery.sha256_bytes(document)}\n"
    ).encode("ascii")
    journal.path.write_bytes(transaction_recovery.BUDGETED_ENVELOPE + header + document)
    monkeypatch.setattr(transaction_recovery.json, "loads", lambda *_args: (_ for _ in ()).throw(AssertionError("downgrade reached legacy decode")))
    with pytest.raises(RepositoryError, match="invalid|unsupported"):
        TransactionJournal.load(root, journal.path)


def test_restart_keeps_untrusted_transaction_named_atomic_temp(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65536, 0))
    foreign = journal.path.parent / f".wedl-txn-{journal.transaction_id}-foreign"
    foreign.write_bytes(b"not a journal generation")
    TransactionJournal.load(root, journal.path)
    assert foreign.read_bytes() == b"not a journal generation"


def test_budgeted_real_child_exit_during_bounded_load_decode_preserves_pending_journal(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65536, 0))
    durable = journal.path.read_bytes()
    script = (
        "import os,sys; from pathlib import Path; import wedl.transaction_recovery as r; "
        "r.json.loads=lambda *_args,**_kwargs: os._exit(0); "
        "r.TransactionJournal.load(Path(sys.argv[1]),Path(sys.argv[2]))"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root), str(journal.path)], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    assert journal.path.read_bytes() == durable
    assert TransactionJournal.load(root, journal.path).surfaces[0].after == b"cache"


@pytest.mark.parametrize("checkpoint", (
    "journal-persist", "surface-after-create", "surface-publish-create",
    "surface-restore-rename", "backup-cleanup-unlink",
))
def test_budgeted_real_child_exit_each_phase_restarts_and_converges(tmp_path: Path, checkpoint: str) -> None:
    """Exercise real process loss in phase, publish, restore, and cleanup paths."""
    root = tmp_path / checkpoint; (root / "story").mkdir(parents=True)
    script = (
        "import os,sys; from pathlib import Path; import wedl.transaction_recovery as r; "
        "from wedl.transaction_recovery import TransactionJournal,SurfaceEnrollment,JournalLiveByteBudget; "
        "root=Path(sys.argv[1]); target=root/'story/world.md'; target.write_bytes(b'before'); "
        "j=TransactionJournal.create(root,ref='refs/heads/main',previous_head='0'*40,committed_head='1'*40); "
        "j.register_surfaces([SurfaceEnrollment('story/world.md',before=b'before',after=b'after',role='source')],live_budget=JournalLiveByteBudget(65536,0)); "
        "r.set_mutation_hook(lambda name: os._exit(0) if name == sys.argv[2] else None); "
        "checkpoint=sys.argv[2]; "
        "(j.advance('ref_committed') if checkpoint == 'journal-persist' else (j.publish_surfaces() if checkpoint in ('surface-after-create','surface-publish-create') else (j.publish_surfaces(),j.restore_surfaces(),j.finish(),j.cleanup())))"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root), checkpoint], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    pending = TransactionJournal.pending(root)
    for journal in pending:
        journal.publish_surfaces(); journal.restore_surfaces(); journal.finish(); journal.cleanup()
    assert (root / "story/world.md").read_bytes() == b"before"
    assert not tuple((root / ".wedl" / "transactions").glob(".wedl-txn-*"))


@pytest.mark.parametrize(("checkpoint", "expected_count"), (
    ("journal-persist", 0),
    ("atomic-replace", 0),
    ("atomic-replace-after-primary", 3),
))
def test_budgeted_batch_child_crash_has_old_or_complete_recoverable_generation(
    tmp_path: Path, checkpoint: str, expected_count: int,
) -> None:
    """A real process loss cannot expose a prefix of a cache/receipt batch."""
    root = tmp_path / checkpoint; root.mkdir()
    script = (
        "import os,sys; from pathlib import Path; import wedl.transaction_recovery as recovery; "
        "from wedl.transaction_recovery import TransactionJournal,SurfaceEnrollment,JournalLiveByteBudget; "
        "root=Path(sys.argv[1]); (root/'story').mkdir(); (root/'story/world.md').write_bytes(b'before'); "
        "j=TransactionJournal.create(root,ref='refs/heads/main',previous_head='0'*40,committed_head='1'*40); "
        "recovery.set_mutation_hook(lambda name: os._exit(0) if name == sys.argv[2] else None); "
        "j.register_surfaces([SurfaceEnrollment('story/world.md',before=b'before',after=b'after',role='source'),"
        "SurfaceEnrollment('.wedl/cache.sqlite',after=b'cache',role='cache'),"
        "SurfaceEnrollment('.wedl/idempotency.json',after=b'receipt',role='receipt')],live_budget=JournalLiveByteBudget(65536,0))"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root), checkpoint], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    journal_path, = (root / ".wedl" / "transactions").glob("*.json")
    reloaded = TransactionJournal.load(root, journal_path)
    # A child death before replacement leaves a same-directory mkstemp file;
    # restart removes it only after authenticating its next-generation record.
    assert not tuple((root / ".wedl" / "transactions").glob(".wedl-txn-*"))
    assert len(reloaded.surfaces) == expected_count
    # The old generation remains an empty prepared journal.  The complete one
    # can be repeatedly published/restored without a cache/receipt prefix.
    if expected_count:
        for _ in range(2):
            reloaded.publish_surfaces()
            assert (root / "story/world.md").read_bytes() == b"after"
            assert (root / ".wedl/cache.sqlite").read_bytes() == b"cache"
            assert (root / ".wedl/idempotency.json").read_bytes() == b"receipt"
        for _ in range(2):
            reloaded.restore_surfaces()
            assert (root / "story/world.md").read_bytes() == b"before"
            assert not (root / ".wedl/cache.sqlite").exists()
            assert not (root / ".wedl/idempotency.json").exists()
    # The restart can durably finish and reclaim only its own journal/private
    # artifacts.  No cache, receipt, WAL, or SHM artifact survives cleanup.
    reloaded.finish()
    reloaded.cleanup()
    assert not tuple(root.glob(".wedl/cache.sqlite*"))
    assert not tuple(root.glob(".wedl/idempotency.json*"))
    assert not tuple(root.rglob("*.sqlite-wal"))
    assert not tuple(root.rglob("*.sqlite-shm"))


def test_budgeted_restart_rejects_oversized_record_before_json_decode(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/a", after=b"a")], live_budget=JournalLiveByteBudget(16384, 0))
    record = _journal_record(journal.path)
    for key in ("liveByteBudget", "$liveByteBudget"):
        record[key]["totalBytes"] = 1
        record[key]["callerReserveBytes"] = 0
        record[key]["admittedPeakBytes"] = 0
    journal.path.write_bytes(_budgeted_payload(record))

    def decoded_never_called(*_args, **_kwargs):
        raise AssertionError("over-limit budgeted journal reached JSON decode")

    monkeypatch.setattr(transaction_recovery.json, "loads", decoded_never_called)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        TransactionJournal.load(root, journal.path)


@pytest.mark.parametrize("corruption", ("missing", "malformed"))
def test_budgeted_restart_rejects_budget_header_corruption_before_json_decode(
    tmp_path: Path, monkeypatch, corruption: str,
) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(16384, 0),
    )
    record = _journal_record(journal.path)
    if corruption == "missing":
        del record["$liveByteBudget"]
    elif corruption == "contradictory":
        record["$liveByteBudget"]["totalBytes"] += 1
    else:
        # Keep the field first but make its early canonical representation
        # invalid, so a budgeted record cannot fall back to legacy loading.
        record["$liveByteBudget"] = {"admittedPeakBytes": "not-an-int", "callerReserveBytes": 0, "totalBytes": 16384}
    journal.path.write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")

    def decoded_never_called(*_args, **_kwargs):
        raise AssertionError("corrupt budgeted journal reached JSON decode")

    monkeypatch.setattr(transaction_recovery.json, "loads", decoded_never_called)
    with pytest.raises(RepositoryError, match="live-byte budget|unsupported"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_restart_rejects_contradictory_budget_header_after_bounded_decode(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(16384, 0),
    )
    record = _journal_record(journal.path)
    record["$liveByteBudget"]["totalBytes"] += 1
    journal.path.write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    with pytest.raises(RepositoryError, match="invalid WEDL transaction journal"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_restart_publish_and_restore_remain_recoverable(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    target = root / "story" / "world.md"; target.parent.mkdir(); target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment("story/world.md", before=b"before", after=b"after", role="source")],
        live_budget=JournalLiveByteBudget(32768, 0),
    )
    reloaded = TransactionJournal.load(root, journal.path)
    reloaded.publish_surfaces()
    assert target.read_bytes() == b"after"
    reloaded.restore_surfaces()
    assert target.read_bytes() == b"before"


def test_budgeted_publish_rejects_post_enrollment_growth_before_any_payload_read(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    target = root / "story" / "world.md"; target.parent.mkdir(); target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment("story/world.md", before=b"before", after=b"after", role="source")],
        live_budget=JournalLiveByteBudget(32768, 0),
    )
    target.write_bytes(b"x" * 8192)
    reads: list[int] = []
    actual_read = transaction_recovery.os.read

    def tracked_read(descriptor, count):
        reads.append(count)
        return actual_read(descriptor, count)

    monkeypatch.setattr(transaction_recovery.os, "read", tracked_read)
    monkeypatch.setattr(transaction_recovery, "_read_file", lambda *_args: (_ for _ in ()).throw(AssertionError("budgeted publish used unbounded path read")))
    with pytest.raises(RepositoryError, match="bounded limit"):
        journal.publish_surfaces()
    assert reads == []
    assert target.read_bytes() == b"x" * 8192
    assert not journal._backup_path(0).exists()


def test_budgeted_publish_rejects_same_byte_identity_replacement_without_deleting_it(tmp_path: Path) -> None:
    _require_bounded_no_follow()
    root = tmp_path / "world"; root.mkdir()
    target = root / "story" / "world.md"; target.parent.mkdir(); target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment("story/world.md", capture_before=True, max_before_bytes=6, after=b"after", role="source")],
        live_budget=JournalLiveByteBudget(32768, 0),
    )

    def replace_after_first_check(name: str) -> None:
        if name == "surface-backup-create":
            target.unlink()
            target.write_bytes(b"before")

    transaction_recovery.set_mutation_hook(replace_after_first_check)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"before"
    assert not journal._backup_path(0).exists()


def test_live_budget_model_counts_abi_containers_and_aliased_surface_occurrences(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    shared = b"same"
    peak = journal.register_surfaces(
        [SurfaceEnrollment(".wedl/a", after=shared), SurfaceEnrollment(".wedl/b", after=shared)],
        live_budget=JournalLiveByteBudget(32768, 0),
    )
    retained_floor = (
        transaction_recovery._LIVE_FIXED_BYTES
        + 2 * transaction_recovery._LIVE_PER_IMAGE_BYTES
        + 2 * len(shared) * 2
        + 2 * transaction_recovery._base64_len(len(shared)) * 4
    )
    assert peak >= retained_floor
    assert journal.record["liveByteBudget"]["admittedPeakBytes"] == peak


def test_live_budget_instruments_base64_json_atomic_and_surface_decode(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    observations: list[tuple[str, dict[str, int]]] = []
    transaction_recovery.set_live_byte_observer(lambda phase, terms: observations.append((phase, terms)))
    try:
        peak = journal.register_surfaces(
            [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache" * 7, role="cache")],
            live_budget=JournalLiveByteBudget(32768, 19),
        )
        TransactionJournal.load(root, journal.path)
    finally:
        transaction_recovery.set_live_byte_observer(None)
    by_phase = {phase: terms for phase, terms in observations}
    assert {"base64-encode", "json-dump-encode", "atomic-replace", "surface-decode", "projected-admission"} <= set(by_phase)
    projections = [terms for phase, terms in observations if phase == "projected-admission"]
    projection = max(projections, key=lambda terms: terms["projected_peak"])
    assert projection["projected_peak"] == peak
    assert projection["caller_reserve"] == 19
    assert all(value > 0 for key, value in by_phase["base64-encode"].items() if key != "image_record")
    assert by_phase["json-dump-encode"]["encoded_payload"] >= len(journal._journal_bytes)
    assert by_phase["atomic-replace"]["replacement_payload"] >= len(journal._journal_bytes)
    assert by_phase["surface-decode"]["decoded_image"] >= len(b"cache" * 7)


@pytest.mark.parametrize("term", ("reserve", "raw", "base64", "journal", "containers"))
def test_live_budget_projection_terms_have_independent_boundary_effect(term: str, monkeypatch) -> None:
    image_sizes = [4]
    reserve = 7
    journal_bytes = 11
    baseline = transaction_recovery._projected_live_sizes(image_sizes, caller_reserve=reserve, journal_bytes=journal_bytes)
    if term == "reserve":
        candidate = transaction_recovery._projected_live_sizes(image_sizes, caller_reserve=reserve + 1, journal_bytes=journal_bytes)
    elif term == "raw":
        # Both 4 and 5 encode to eight bytes, isolating decoded image storage.
        candidate = transaction_recovery._projected_live_sizes([5], caller_reserve=reserve, journal_bytes=journal_bytes)
    elif term == "base64":
        original = transaction_recovery._base64_len
        monkeypatch.setattr(transaction_recovery, "_base64_len", lambda size: original(size) + 1)
        candidate = transaction_recovery._projected_live_sizes(image_sizes, caller_reserve=reserve, journal_bytes=journal_bytes)
    elif term == "journal":
        candidate = transaction_recovery._projected_live_sizes(image_sizes, caller_reserve=reserve, journal_bytes=journal_bytes + 1)
    else:
        monkeypatch.setattr(transaction_recovery, "_LIVE_FIXED_BYTES", transaction_recovery._LIVE_FIXED_BYTES + 1)
        candidate = transaction_recovery._projected_live_sizes(image_sizes, caller_reserve=reserve, journal_bytes=journal_bytes)
    assert candidate > baseline
    with pytest.raises(RepositoryError, match="budget exceeded"):
        transaction_recovery._checked_live_sum(sys.maxsize, 1)
    ledger = transaction_recovery._projected_live_ledger(
        [5, 9], caller_reserve=13, journal_bytes=101, path_sizes=[sys.getsizeof(".wedl/cache.sqlite")],
        operation_path_bytes=sys.getsizeof("private.sqlite"),
    )
    assert {"enrollment", "early-restart", "json-preparse", "restart-load", "surface-decode", "publish", "restore", "private-artifact", "lock", "phase", "cleanup"} == set(ledger)
    assert transaction_recovery._projected_live_sizes([5, 9], caller_reserve=13, journal_bytes=101, path_sizes=[sys.getsizeof(".wedl/cache.sqlite")], operation_path_bytes=sys.getsizeof("private.sqlite")) == max(ledger.values())


def test_overbudget_cache_retention_receipt_batch_enrolls_nothing_then_receipt_only_admits(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    prior_bytes = journal.path.read_bytes(); prior_generation = journal.record["generation"]
    enrollments = [
        SurfaceEnrollment(".wedl/cache.sqlite", after=b"c" * 256, role="cache"),
        SurfaceEnrollment(".wedl/cache-retention.sqlite", before=b"old", after=None, role="cache"),
        SurfaceEnrollment(".wedl/idempotency.json", after=b"receipt", role="receipt"),
    ]
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(1, 0))
    assert journal.surfaces == ()
    assert journal.record["generation"] == prior_generation
    assert journal.path.read_bytes() == prior_bytes
    peak = journal.register_surfaces(
        [SurfaceEnrollment(".wedl/idempotency.json", after=b"receipt", role="receipt")],
        live_budget=JournalLiveByteBudget(16384, 0),
    )
    assert peak == journal.record["liveByteBudget"]["admittedPeakBytes"]
    assert [(surface.path, surface.role) for surface in journal.surfaces] == [(".wedl/idempotency.json", "receipt")]


def test_journal_atomic_replace_verifier_fails_before_replacement(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    before = journal.path.read_bytes()

    def reject_at_atomic_replace(name: str) -> None:
        if name == "atomic-replace":
            transaction_recovery.set_mutation_hook(None)

    transaction_recovery.set_mutation_hook(reject_at_atomic_replace)
    try:
        with pytest.raises(RepositoryError, match="before image is unstable"):
            journal._persist(pre_persist=lambda: False)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert journal.path.read_bytes() == before and journal.record["generation"] == 1


def test_registered_surface_ownership_loss_is_fail_closed(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40, surfaces=[Surface(".wedl/cache.bin", None, b"ours")])
    target = root / ".wedl/cache.bin"; target.parent.mkdir(exist_ok=True); target.write_bytes(b"external")
    with pytest.raises(RepositoryError, match="lost surface ownership"):
        journal.publish_surfaces()
    assert target.read_bytes() == b"external"
    assert journal.path.exists()


def test_recovery_rolls_forward_only_when_committed_ref_is_still_owned(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    # Recreate the crash boundary: new ref, but source materialization never
    # happened.  The journal contains the only safe after image.
    (repository.root / "story/world.md").write_bytes(b"before\n")
    old_index = {"story/world.md": {"mode": "100644", "stage": 0, "blob": repository._blob_at(old, "story/world.md")}}
    after_index = repository._real_index_entries(["story/world.md"])
    repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(old_index))
    journal = TransactionJournal.create(
        repository.root,
        ref="refs/heads/master",
        previous_head=old,
        committed_head=new,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n")],
        index_before=old_index,
        index_after=after_index,
    )
    repository.recover_authoring_transactions()
    assert repository.head() == new
    assert (repository.root / "story/world.md").read_bytes() == b"after\n"
    assert not journal.path.exists()
    repository.recover_authoring_transactions()


def test_recovery_restores_partial_write_when_ref_never_advanced(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    target = repository.root / "story/world.md"; target.write_bytes(b"after\n")
    journal = TransactionJournal.create(
        repository.root,
        ref="refs/heads/master",
        previous_head=old,
        committed_head="f" * 40,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n")],
    )
    repository.recover_authoring_transactions()
    assert repository.head() == old
    assert target.read_bytes() == b"before\n"
    assert not journal.path.exists()


def test_recovery_never_overwrites_an_intervening_ref_or_file(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    target = repository.root / "story/world.md"; target.write_bytes(b"external\n")
    journal = TransactionJournal.create(
        repository.root,
        ref="refs/heads/master",
        previous_head="0" * 40,
        committed_head="f" * 40,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n")],
    )
    with pytest.raises(RepositoryError, match="lost ref ownership"):
        repository.recover_authoring_transactions()
    assert target.read_bytes() == b"external\n"
    assert journal.path.exists()


def test_journal_rejects_tampering_and_freezes_generic_enrollment(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/receipt.json", after=b"receipt", role="receipt")
    journal.advance("ref_committed")
    with pytest.raises(RepositoryError, match="enrollment is frozen"):
        journal.register_surface(".wedl/late.sqlite", after=b"late", role="cache")
    payload = journal.path.read_text(encoding="utf-8").replace("refs/heads/main", "../foreign")
    journal.path.write_text(payload, encoding="utf-8")
    with pytest.raises(RepositoryError, match="invalid WEDL transaction journal"):
        TransactionJournal.load(root, journal.path)


@pytest.mark.parametrize("path", (".git/config", ".wedl/transactions/foreign.json", "../outside"))
def test_generic_surfaces_cannot_claim_git_or_journal_internals(tmp_path: Path, path: str) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError):
        journal.register_surface(path, after=b"forbidden", role="cache")


def test_casefold_aliases_cannot_be_enrolled_twice(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/Cache.bin", after=b"one")
    with pytest.raises(RepositoryError, match="already registered"):
        journal.register_surface(".wedl/cache.bin", after=b"two")


def test_pre_ref_enrollment_survives_commit_for_downstream_publication(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    captured: list[TransactionJournal] = []

    def enroll(journal: TransactionJournal) -> None:
        journal.register_surface(".wedl/receipt.json", after=b"receipt", role="receipt")
        captured.append(journal)

    repository.commit_files(
        expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="after",
        transaction_enroll=enroll, retain_transaction=True,
    )
    assert len(captured) == 1 and captured[0].path.exists()
    repository.recover_authoring_transactions()
    assert (repository.root / ".wedl/receipt.json").read_bytes() == b"receipt"
    assert not captured[0].path.exists()


@pytest.mark.parametrize("source_kind", ("update", "delete", "create"))
@pytest.mark.parametrize("existing_cache", (True, False))
def test_downstream_enrollment_failure_before_index_restores_exact_before_images(
        tmp_path: Path, monkeypatch, source_kind: str, existing_cache: bool) -> None:
    """Task-92-shaped enrollment cannot make pre-index rollback require after."""
    repository = _repository(tmp_path)
    old = repository.head()
    if source_kind == "create":
        source_path, files = "story/new.md", {"story/new.md": b"created\n"}
        source_before, source_mode = None, None
    else:
        source_path = "story/world.md"
        source_before = (repository.root / source_path).read_bytes()
        source_mode = stat.S_IMODE((repository.root / source_path).stat().st_mode)
        files = {source_path: None if source_kind == "delete" else b"after\n"}
    index_before = repository._real_index_entries(files)
    cache = repository.root / ".wedl/cache.sqlite"
    receipt = repository.root / ".wedl/receipt.json"
    if existing_cache:
        cache.parent.mkdir(); cache.write_bytes(b"old cache")
        failing, existing = ".wedl/cache.sqlite", cache
    else:
        receipt.parent.mkdir(); receipt.write_bytes(b"old receipt")
        failing, existing = ".wedl/receipt.json", receipt
    captured: list[TransactionJournal] = []

    def enroll(journal: TransactionJournal) -> None:
        journal.register_surface(".wedl/cache.sqlite", capture_before=True, after=b"new cache", role="cache")
        journal.register_surface(".wedl/receipt.json", capture_before=True, after=b"new receipt", role="receipt")
        captured.append(journal)

    actual_publish = TransactionJournal._publish_claimed_surface

    def fail_downstream(root, relative, claim, target, expected, *, mutation):
        if relative == failing and mutation == "surface-publish-create":
            raise OSError("downstream publication failed")
        return actual_publish(root, relative, claim, target, expected, mutation=mutation)

    monkeypatch.setattr(TransactionJournal, "_publish_claimed_surface", staticmethod(fail_downstream))
    with pytest.raises(OSError, match="downstream publication failed"):
        repository.commit_files(expected_head=old, files=files, message="downstream failure",
                                transaction_enroll=enroll, retain_transaction=True)

    assert repository.head() == old
    assert repository._real_index_entries(files) == index_before
    target = repository.root / source_path
    if source_before is None:
        assert not target.exists()
    else:
        assert target.read_bytes() == source_before
        if os.name != "nt":
            assert stat.S_IMODE(target.stat().st_mode) == source_mode
    assert existing.read_bytes() == (b"old cache" if existing_cache else b"old receipt")
    assert not (receipt if existing_cache else cache).exists()
    assert captured and not captured[0].path.exists()
    repository.recover_authoring_transactions()
    assert repository.head() == old and repository._real_index_entries(files) == index_before


def test_preindex_recovery_rejects_external_index_then_retries(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; old = repository.head(); before = repository._real_index_entries([path])
    new = repository.commit_files(expected_head=old, files={path: b"after\n"}, message="candidate")
    after = repository._real_index_entries([path])
    repository._git(["update-ref", "refs/heads/master", old, new])
    (repository.root / path).write_bytes(b"before\n")
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
                                        surfaces=[Surface(path, b"before\n", b"after\n", "source")],
                                        index_before=before, index_after=after)
    journal.advance("ref_committed")
    # A durable pre-index phase cannot claim the after image merely because a
    # ref CAS happened; retain it for operator-visible recovery.
    with pytest.raises(RepositoryError, match="real-index ownership"):
        repository.recover_authoring_transactions()
    assert repository._real_index_entries([path]) == after and journal.path.exists()
    journal.advance("sources_published")
    external = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"external\n").stdout.decode().strip()
    repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(
        {path: {"mode": "100644", "stage": 0, "blob": external}}))
    with pytest.raises(RepositoryError, match="real-index ownership"):
        repository.recover_authoring_transactions()
    assert repository._real_index_entries([path])[path]["blob"] == external and journal.path.exists()
    repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(before))
    repository.recover_authoring_transactions(); repository.recover_authoring_transactions()
    assert repository.head() == old and repository._real_index_entries([path]) == before and not journal.path.exists()


def test_sources_published_phase_rolls_back_exact_index_after_image(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; old = repository.head(); before = repository._real_index_entries([path])
    new = repository.commit_files(expected_head=old, files={path: b"after\n"}, message="candidate")
    after = repository._real_index_entries([path])
    repository._git(["update-ref", "refs/heads/master", old, new])
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
                                        surfaces=[Surface(path, b"before\n", b"after\n", "source")],
                                        index_before=before, index_after=after)
    journal.advance("ref_committed"); journal.advance("sources_published")
    repository.recover_authoring_transactions()
    assert repository.head() == old and (repository.root / path).read_bytes() == b"before\n"
    assert repository._real_index_entries([path]) == before and not journal.path.exists()


def test_index_published_phase_rejects_preindex_image_while_committed_ref_is_owned(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; old = repository.head(); before = repository._real_index_entries([path])
    new = repository.commit_files(expected_head=old, files={path: b"after\n"}, message="candidate")
    after = repository._real_index_entries([path])
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
                                        surfaces=[Surface(path, b"before\n", b"after\n", "source")],
                                        index_before=before, index_after=after)
    journal.advance("ref_committed"); journal.advance("sources_published"); journal.advance("index_published")
    repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(before))
    with pytest.raises(RepositoryError, match="real-index ownership"):
        repository.recover_authoring_transactions()
    assert repository.head() == new and repository._real_index_entries([path]) == before and journal.path.exists()


def test_source_interleaving_before_no_clobber_publish_fails_closed(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(
        root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source")],
    )
    def interleave(mutation: str) -> None:
        if mutation == "surface-publish-create":
            target.write_bytes(b"external")

    transaction_recovery.set_mutation_hook(interleave)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"external"
    assert journal.path.exists()


def test_claim_gap_after_real_mutation_recovers_from_private_backup(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source")])

    def crash(mutation: str) -> None:
        if mutation == "surface-publish-create":
            raise RuntimeError("injected crash")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="injected crash"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not target.exists() and journal.path.exists()
    journal.publish_surfaces()
    assert target.read_bytes() == b"after"
    journal.restore_surfaces()
    assert target.read_bytes() == b"before"


@pytest.mark.parametrize("after", [b"after", None])
def test_surface_backup_is_sealed_before_public_removal_and_restart_recovers(tmp_path: Path, after: bytes | None) -> None:
    """The exact backup-create crash window never loses the public before image."""
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40,
                                        committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", after, "source")])

    def crash(name: str) -> None:
        if name == "surface-backup-create":
            raise RuntimeError("crash before private backup claim")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="private backup"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    backup = journal.path.parent / journal.transaction_id / "backups" / "0000.before"
    assert target.read_bytes() == b"before" and not backup.exists()
    reloaded = TransactionJournal.load(root, journal.path)
    reloaded.publish_surfaces()
    if after is None:
        assert not target.exists()
    else:
        assert target.read_bytes() == after
    reloaded.restore_surfaces()
    assert target.read_bytes() == b"before"


@pytest.mark.parametrize("after", (b"after\n", None), ids=("update", "delete"))
@pytest.mark.parametrize("roll_forward", (True, False), ids=("roll-forward", "roll-back"))
def test_real_restart_after_surface_backup_checkpoint_converges_update_and_delete(
    tmp_path: Path, after: bytes | None, roll_forward: bool,
) -> None:
    """An interrupted authenticated before-claim has no unrecoverable gap."""
    repository = _repository(tmp_path)
    path = "story/world.md"; target = repository.root / path
    old = repository.head()
    before_index = repository._real_index_entries([path])
    new = repository.commit_files(expected_head=old, files={path: after}, message="candidate")
    after_index = repository._real_index_entries([path])
    # Reconstruct exactly the source state at the backup-create checkpoint: the
    # new ref may be authoritative (roll forward) or have been CAS-rolled back
    # (roll back), but the old public bytes remain until the sealed claim exists.
    target.write_bytes(b"before\n")
    if roll_forward:
        repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(before_index))
    else:
        repository._git(["update-ref", "refs/heads/master", old, new])
        repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(before_index))
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
        surfaces=[Surface(path, b"before\n", after, "source")],
        index_before=before_index, index_after=after_index,
    )

    def crash(name: str) -> None:
        if name == "surface-backup-create":
            raise RuntimeError("crash at durable source backup boundary")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="durable source backup"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"before\n" and journal.path.exists()

    restarted = Repository(repository.root)
    restarted.recover_authoring_transactions()
    if roll_forward:
        assert restarted.head() == new
        assert (not target.exists()) if after is None else target.read_bytes() == after
        assert restarted._real_index_entries([path]) == after_index
    else:
        assert restarted.head() == old and target.read_bytes() == b"before\n"
        assert restarted._real_index_entries([path]) == before_index
    assert not journal.path.exists()


@pytest.mark.parametrize("artifact_kind,same_bytes", [("before", True), ("before", False), ("published", True), ("published", False), ("staging", True), ("staging", False)])
def test_cleanup_final_cas_preserves_substituted_private_artifacts(tmp_path: Path, artifact_kind: str, same_bytes: bool) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40,
                                        committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source")])
    journal.publish_surfaces(); journal.restore_surfaces()
    staging = journal.write_staging_artifact("private.bin", b"staging"); journal.seal_staging_artifact("private.bin")
    journal.finish()
    candidates = {
        "before": journal.path.parent / journal.transaction_id / "backups" / "0000.before",
        "published": journal.path.parent / journal.transaction_id / "backups" / "0000.published",
        "staging": staging,
    }
    artifact = candidates[artifact_kind]
    original = artifact.read_bytes()
    checkpoint = {"before": "backup-cleanup", "published": "published-cleanup", "staging": "staging-cleanup"}[artifact_kind]

    def substitute(name: str) -> None:
        if name == checkpoint:
            artifact.unlink(); artifact.write_bytes(original if same_bytes else b"external private replacement")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="changed ownership"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert artifact.exists() and artifact.read_bytes() == (original if same_bytes else b"external private replacement")


@pytest.mark.parametrize("same_bytes", (True, False))
def test_cleanup_unlink_inode_claim_rejects_late_private_substitution(tmp_path: Path, same_bytes: bool) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40,
                                        committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source")])
    journal.publish_surfaces(); journal.restore_surfaces(); journal.finish()
    backup = journal.path.parent / journal.transaction_id / "backups" / "0000.before"
    original = backup.read_bytes()

    def substitute(name: str) -> None:
        if name == "backup-cleanup-unlink":
            backup.unlink(); backup.write_bytes(original if same_bytes else b"external replacement")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="backup changed ownership"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert backup.exists() and backup.read_bytes() == (original if same_bytes else b"external replacement")


def test_surface_modes_and_journal_reparse_containment(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o744, 0o600)])
    journal.publish_surfaces()
    assert target.read_bytes() == b"after"
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o600
    journal.restore_surfaces()
    assert target.read_bytes() == b"before"
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o744
    journal.finish(); journal.cleanup()

    outside = tmp_path / "outside"; outside.mkdir()
    journal_root = root / ".wedl" / "transactions"; journal_root.parent.mkdir()
    try:
        journal_root.symlink_to(outside, target_is_directory=True)
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    with pytest.raises(RepositoryError, match="reparse point"):
        TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)


def test_recovery_preserves_intervening_real_index_entry(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    expected_after = repository._real_index_entries(["story/world.md"])
    before = {"story/world.md": {"mode": "100644", "stage": 0, "blob": repository._blob_at(old, "story/world.md")}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
        surfaces=[Surface("story/world.md", b"after\n", b"after\n", "source")], index_before=before, index_after=expected_after)
    external = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"external\n").stdout.decode().strip()
    repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info({"story/world.md": {"mode": "100644", "stage": 0, "blob": external}}))
    with pytest.raises(RepositoryError, match="real-index ownership"):
        repository.recover_authoring_transactions()
    assert repository._real_index_entries(["story/world.md"])["story/world.md"]["blob"] == external
    assert journal.path.exists()


def test_recovery_uses_recorded_ref_and_reclaims_only_owned_dead_transaction_lock(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    subprocess.run(["git", "-C", str(repository.root), "branch", "recovery-side", old], check=True)
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/recovery-side", previous_head=old, committed_head=new,
        surfaces=[Surface("story/world.md", b"after\n", b"after\n")],
        index_before=repository._real_index_entries(["story/world.md"]),
        index_after=repository._real_index_entries(["story/world.md"]),
    )
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    token = "11111111-1111-1111-1111-111111111111"
    lock.write_text(f"pid=999999\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=999999, token=token)
    monkeypatch.setattr(transaction_recovery, "_process_dead", lambda _pid: True)
    repository.recover_authoring_transactions()
    assert repository.head() == new
    assert repository.ref("refs/heads/recovery-side") == old
    assert not journal.path.exists()


def test_partial_canonical_lock_bind_is_never_reclaimed(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    token = "11111111-1111-1111-1111-111111111111"
    payload = f"pid=999999\ntoken={token}\n"
    lock.write_text(payload, encoding="ascii")
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: (_ for _ in ()).throw(ProcessLookupError()))
    with pytest.raises(RepositoryError, match="another WEDL canonical write"):
        repository.recover_authoring_transactions()
    assert lock.read_text(encoding="ascii") == payload


def test_real_index_lock_claim_preserves_external_lock_and_retries(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"after\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head="1" * 40,
        index_before=before, index_after=after)
    lock = repository.root / ".git" / "index.lock"; lock.write_bytes(b"external Git transaction")
    with pytest.raises(RepositoryError, match="index.lock"):
        repository._publish_real_index(journal, expected=before, desired=after)
    assert lock.read_bytes() == b"external Git transaction"
    lock.unlink()
    repository._publish_real_index(journal, expected=before, desired=after)
    assert repository._real_index_entries([path]) == after


def test_real_index_lock_identity_loss_preserves_replacement(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"after\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head="1" * 40,
        index_before=before, index_after=after)
    actual_claim = journal.claim_index_lock

    def lose_lock(**kwargs) -> None:
        actual_claim(**kwargs)
        lock = repository.root / ".git" / "index.lock"
        lock.unlink(); lock.write_bytes(b"external replacement")

    monkeypatch.setattr(journal, "claim_index_lock", lose_lock)
    with pytest.raises(RepositoryError, match="real-index lock ownership"):
        repository._publish_real_index(journal, expected=before, desired=after)
    assert (repository.root / ".git" / "index.lock").read_bytes() == b"external replacement"
    assert repository._real_index_entries([path]) == before


def test_subprocess_lock_holder_is_never_removed_and_retry_is_safe(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "index.lock"
    script = "from pathlib import Path; import sys,time; Path(sys.argv[1]).write_bytes(b'external holder'); time.sleep(20)"
    holder = subprocess.Popen([sys.executable, "-c", script, str(lock)])
    try:
        for _ in range(100):
            if lock.exists():
                break
            time.sleep(0.02)
        with pytest.raises(RepositoryError, match="index"):
            repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="locked")
        assert lock.read_bytes() == b"external holder"
    finally:
        holder.terminate(); holder.wait(timeout=10)
        lock.unlink(missing_ok=True)
    revision = repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="retry")
    assert repository.head() == revision


def test_subprocess_canonical_lock_holder_survives_and_retry_is_safe(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    script = "from pathlib import Path; import sys,time; Path(sys.argv[1]).write_bytes(b'external canonical holder'); time.sleep(20)"
    holder = subprocess.Popen([sys.executable, "-c", script, str(lock)])
    try:
        for _ in range(100):
            if lock.exists():
                break
            time.sleep(0.02)
        with pytest.raises(RepositoryError, match="canonical write"):
            repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="blocked")
        assert lock.read_bytes() == b"external canonical holder"
    finally:
        holder.terminate(); holder.wait(timeout=10)
        lock.unlink(missing_ok=True)
    revision = repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="retry")
    assert repository.head() == revision


def test_subprocess_death_owned_index_lock_is_adopted_from_journal(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; index = repository.root / ".git" / "index"; lock = repository.root / ".git" / "index.lock"
    before = repository._real_index_entries([path])
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head=repository.head(),
        surfaces=[Surface(path, b"before\n", b"before\n", "source")], index_before=before, index_after=before)
    journal.write_staging_artifact("real-index-before", index.read_bytes())
    journal.seal_staging_artifact("real-index-before")
    install = journal.write_staging_artifact("real-index-install", index.read_bytes())
    journal.seal_staging_artifact("real-index-install")
    journal.bind_index_artifacts(backup="real-index-before", install="real-index-install")
    os.link(install, lock)
    status = lock.stat()
    journal.claim_index_lock(token="22222222-2222-2222-2222-222222222222", identity=f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}", digest=__import__("hashlib").sha256(lock.read_bytes()).hexdigest())
    repository.recover_authoring_transactions()
    assert not lock.exists() and not journal.path.exists()


def test_completed_journal_removes_registered_staging_artifact(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    artifact = journal.write_staging_artifact("private.sqlite", b"private"); journal.seal_staging_artifact("private.sqlite")
    journal.finish(); journal.cleanup()
    assert not artifact.exists() and not journal.path.exists()


def test_crash_after_completion_retains_then_retries_exact_cleanup(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    artifact = journal.write_staging_artifact("private.sqlite", b"private"); journal.seal_staging_artifact("private.sqlite")
    journal.finish()

    def crash(mutation: str) -> None:
        if mutation == "journal-cleanup":
            raise RuntimeError("crash after completed phase")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="completed phase"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    # Private artifacts are removed before the journal. A crash at the final
    # journal unlink retains only the completed ownership record for retry.
    assert journal.path.exists() and not artifact.exists()
    TransactionJournal.load(root, journal.path).cleanup()
    assert not journal.path.exists() and not artifact.exists()


def test_staging_seal_rechecks_post_write_identity_and_digest(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    artifact = journal.write_staging_artifact("compiled.sqlite", b"first")

    def interleave(mutation: str) -> None:
        if mutation == "staging-artifact-seal":
            artifact.write_bytes(b"external replacement")

    transaction_recovery.set_mutation_hook(interleave)
    try:
        with pytest.raises(RepositoryError, match="staging artifact changed ownership"):
            journal.seal_staging_artifact("compiled.sqlite")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert artifact.read_bytes() == b"external replacement"
    assert journal.record["sealedArtifacts"] == {}
    assert journal.path.exists()


def test_private_index_build_crash_adopts_only_sealed_install(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"third\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new, index_before=before, index_after=after)
    journal.advance("ref_committed"); journal.advance("sources_published")
    lock = repository.root / ".git" / "index.lock"
    observed_private_build = False

    def crash(mutation: str) -> None:
        nonlocal observed_private_build
        if mutation == "real-index-update-private":
            observed_private_build = True
            assert not lock.exists()
        if mutation == "real-index-publish":
            raise RuntimeError("crash before final index install")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="final index install"):
            repository._publish_real_index(journal, expected=before, desired=after)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert observed_private_build and lock.exists()
    assert journal.record["indexArtifacts"] == {"backup": "real-index-before", "install": "real-index-install"}
    repository.recover_authoring_transactions()
    assert not lock.exists() and repository._real_index_entries([path]) == after and not journal.path.exists()


def test_parent_reparse_after_mkdir_is_rechecked_at_actual_write(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    outside = tmp_path / "outside"; outside.mkdir()
    target = root / "story" / "nested" / "world.md"
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/nested/world.md", None, b"after", "source")])

    def replace_parent(mutation: str) -> None:
        if mutation != "surface-parent-created":
            return
        try:
            target.parent.rmdir()
            target.parent.symlink_to(outside, target_is_directory=True)
        except OSError:
            pytest.skip("Windows test host does not permit reparse-point fixtures")

    transaction_recovery.set_mutation_hook(replace_parent)
    try:
        with pytest.raises(RepositoryError, match="reparse point|changed ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not (outside / "world.md").exists()


@pytest.mark.skipif(os.name == "nt", reason="Windows does not expose POSIX executable mode semantics")
def test_mode_only_drift_fails_closed(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before"); os.chmod(target, 0o600)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o744, 0o600)])
    with pytest.raises(RepositoryError, match="lost surface ownership"):
        journal.publish_surfaces()


@pytest.mark.parametrize("phase", ("prepared", "ref_committed", "sources_published", "index_published", "surfaces_published"))
def test_restart_rolls_forward_every_durable_phase_with_registered_cache_and_receipt(tmp_path: Path, phase: str) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    before_index = {"story/world.md": {"mode": "100644", "stage": 0, "blob": repository._blob_at(old, "story/world.md")}}
    after_index = repository._real_index_entries(["story/world.md"])
    (repository.root / "story/world.md").write_bytes(b"before\n")
    receipt = repository.root / ".wedl" / "receipt.json"; receipt.parent.mkdir(exist_ok=True); receipt.write_bytes(b"old")
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n"), Surface(".wedl/receipt.json", b"old", b"new", "receipt"), Surface(".wedl/cache.sqlite", None, b"cache", "cache")],
        index_before=before_index, index_after=after_index,
    )
    if phase != "prepared":
        for name in ("ref_committed", "sources_published", "index_published", "surfaces_published"):
            journal.advance(name)
            if name == phase:
                break
    if phase in {"prepared", "ref_committed"}:
        repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(before_index))
    repository.recover_authoring_transactions()
    assert (repository.root / "story/world.md").read_bytes() == b"after\n"
    assert receipt.read_bytes() == b"new"
    assert (repository.root / ".wedl/cache.sqlite").read_bytes() == b"cache"
    assert repository._real_index_entries(["story/world.md"]) == after_index
    assert not journal.path.exists()


@pytest.mark.parametrize("replacement", [b"before", b"external bytes"])
def test_source_final_claim_checkpoint_preserves_identical_and_different_substitution(tmp_path: Path, replacement: bytes) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source")])

    def substitute(name: str) -> None:
        if name == "surface-claim-rename":
            target.unlink(); target.write_bytes(replacement)

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == replacement
    assert journal.path.exists()


@pytest.mark.parametrize("same_bytes", [True, False])
def test_real_index_final_checkpoint_preserves_identical_and_different_lock_substitution(tmp_path: Path, same_bytes: bool) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"after\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head="1" * 40,
                                        index_before=before, index_after=after)
    lock = repository.root / ".git" / "index.lock"

    def substitute(name: str) -> None:
        if name == "real-index-publish":
            original = lock.read_bytes(); lock.unlink()
            lock.write_bytes(original if same_bytes else b"external Git lock")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="real-index lock ownership"):
            repository._publish_real_index(journal, expected=before, desired=after)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert repository._real_index_entries([path]) == before
    assert lock.exists()
    lock.unlink()


@pytest.mark.parametrize("same_bytes", [True, False])
def test_journal_generation_cas_and_journal_last_cleanup_preserve_substitution(tmp_path: Path, same_bytes: bool) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    original = journal.path.read_bytes()

    def substitute_persist(name: str) -> None:
        if name == "atomic-replace":
            journal.path.unlink(); journal.path.write_bytes(original if same_bytes else b"{}\n")

    transaction_recovery.set_mutation_hook(substitute_persist)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.advance("ref_committed")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert journal.path.read_bytes() == (original if same_bytes else b"{}\n")

    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.finish(); final_bytes = journal.path.read_bytes()

    def substitute_cleanup(name: str) -> None:
        if name == "journal-cleanup":
            journal.path.unlink(); journal.path.write_bytes(final_bytes if same_bytes else b"external\n")

    transaction_recovery.set_mutation_hook(substitute_cleanup)
    try:
        with pytest.raises(RepositoryError, match="journal changed ownership"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert journal.path.exists()


def test_real_os_exit_dead_pid_reclaims_exact_journal_bound_lock_after_restart(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    token = "33333333-3333-3333-3333-333333333333"
    script = (
        "import os,sys; from pathlib import Path; from wedl.transaction_recovery import TransactionJournal; "
        "root=Path(sys.argv[1]); head=sys.argv[2]; token=sys.argv[3]; "
        "journal=TransactionJournal.create(root,ref='refs/heads/master',previous_head=head,committed_head=head); "
        "lock=root/'.git'/'wedl-canonical-write.lock'; "
        "lock.write_text(f'pid={os.getpid()}\\ntoken={token}\\n',encoding='ascii'); "
        "journal.bind_lock(lock,pid=os.getpid(),token=token); os._exit(0)"
    )
    child = subprocess.run([sys.executable, "-c", script, str(repository.root), repository.head(), token],
                           check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    assert lock.exists()
    repository.recover_authoring_transactions()
    assert not lock.exists()
    journal_dir = repository.root / ".wedl" / "transactions"
    assert not journal_dir.exists() or not tuple(journal_dir.glob("*.json"))


def test_real_os_exit_after_finish_reclaims_exact_journal_bound_lock_after_restart(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/master",
        previous_head=repository.head(), committed_head=repository.head(),
    )
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/recovery", after=b"x", role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    journal.finish()
    script = (
        "import os,sys; from pathlib import Path; from wedl.repository import Repository; "
        "from wedl.transaction_recovery import TransactionJournal; root=Path(sys.argv[1]); "
        "original=TransactionJournal.finish; "
        "TransactionJournal.finish=lambda self:(original(self),os._exit(92))[1]; "
        "Repository(root).recover_authoring_transactions()"
    )
    child = subprocess.run([sys.executable, "-c", script, str(repository.root)],
                           check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 92, child.stderr.decode("utf-8", "replace")
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    assert lock.exists()
    repository.recover_authoring_transactions()
    assert not lock.exists()
    journal_dir = repository.root / ".wedl" / "transactions"
    assert not journal_dir.exists() or not tuple(journal_dir.glob("*.json"))


@pytest.mark.parametrize("same_bytes", [True, False])
def test_stale_lock_reclaim_keeps_same_and_different_byte_substitutions(tmp_path: Path, monkeypatch, same_bytes: bool) -> None:
    repository = _repository(tmp_path)
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head=repository.head())
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    token = "44444444-4444-4444-4444-444444444444"
    lock.write_text(f"pid=999999\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=999999, token=token)
    original = lock.read_bytes()
    monkeypatch.setattr(transaction_recovery, "_process_dead", lambda _pid: True)

    def substitute(name: str) -> None:
        if name == "stale-lock-reclaim":
            lock.unlink(); lock.write_bytes(original if same_bytes else b"external lock\n")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        assert not TransactionJournal.reclaim_owned_stale_lock(repository.root, lock)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert lock.read_bytes() == (original if same_bytes else b"external lock\n") and journal.path.exists()


def test_restore_final_checkpoint_rejects_parent_reparse_substitution(tmp_path: Path) -> None:
    root = tmp_path / "world"; story = root / "story"; story.mkdir(parents=True)
    outside = tmp_path / "outside"; outside.mkdir()
    target = story / "world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after")])
    journal.publish_surfaces()

    def substitute(name: str) -> None:
        if name == "surface-restore-rename":
            target.unlink(); story.rmdir(); story.symlink_to(outside, target_is_directory=True)

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="reparse point"):
            journal.restore_surfaces()
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not (outside / "world.md").exists() and journal.path.exists()


@pytest.mark.skipif(os.name == "nt", reason="POSIX-only liveness contract")
def test_posix_process_dead_uses_kill_zero_outcomes(monkeypatch) -> None:
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: (_ for _ in ()).throw(ProcessLookupError()))
    assert transaction_recovery._process_dead(123) is True
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: None)
    assert transaction_recovery._process_dead(123) is False
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: (_ for _ in ()).throw(PermissionError()))
    assert transaction_recovery._process_dead(123) is None


@pytest.mark.parametrize("same_bytes", [True, False])
def test_canonical_lock_release_keeps_same_and_different_byte_substitutions(tmp_path: Path, same_bytes: bool) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    captured: list[TransactionJournal] = []
    original: list[bytes] = []

    def substitute(name: str) -> None:
        if name == "canonical-lock-release":
            original.append(lock.read_bytes()); lock.unlink(); lock.write_bytes(original[0] if same_bytes else b"external release lock\n")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="ownership changed"):
            with repository._canonical_write_lock():
                journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head=repository.head())
                repository._bind_transaction_lock(journal); captured.append(journal)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert lock.read_bytes() == (original[0] if same_bytes else b"external release lock\n")
    assert captured and captured[0].path.exists()


@pytest.mark.parametrize("before,after", [(None, b"created"), (b"before", b"updated")])
def test_private_after_image_is_sealed_before_public_link_and_preserves_mode(tmp_path: Path, before: bytes | None, after: bytes) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"
    if before is not None:
        target.write_bytes(before)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", before, after, "source", 0o640 if before else None, 0o600)])
    journal.publish_surfaces()
    private_after = journal.path.parent / journal.transaction_id / "backups" / "0000.after"
    assert target.read_bytes() == after and private_after.exists()
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o600


@pytest.mark.parametrize("checkpoint,directory_name", [("backup-cleanup", "backups"), ("published-cleanup", "backups"), ("staging-cleanup", "staging")])
def test_cleanup_final_checkpoint_rejects_private_parent_reparse(tmp_path: Path, checkpoint: str, directory_name: str) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    outside = tmp_path / "outside"; outside.mkdir()
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after")])
    journal.publish_surfaces(); journal.restore_surfaces()
    artifact = journal.write_staging_artifact("private.bin", b"private"); journal.seal_staging_artifact("private.bin")
    journal.finish()
    private_directory = journal.path.parent / journal.transaction_id / directory_name

    def substitute(name: str) -> None:
        if name == checkpoint:
            moved = private_directory.with_name(f"{directory_name}-external")
            private_directory.rename(moved); private_directory.symlink_to(outside, target_is_directory=True)

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="reparse point|changed ownership"):
            journal.cleanup()
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not tuple(outside.iterdir()) and journal.path.exists() and artifact.exists()


@pytest.mark.parametrize("checkpoint", ["surface-after-create", "surface-after-create-written", "surface-mode-publish"])
def test_private_after_create_write_and_chmod_crashes_leave_no_public_after_and_restart_rolls_back(tmp_path: Path, checkpoint: str) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o640, 0o600)])

    def crash(name: str) -> None:
        if name == checkpoint:
            raise RuntimeError("private after crash")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="private after crash"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not target.exists() and journal.path.exists()
    TransactionJournal.load(root, journal.path).restore_surfaces()
    assert target.read_bytes() == b"before"
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o640


def test_real_os_exit_with_unsealed_private_after_restarts_to_before_image(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    script = (
        "import os,sys; from pathlib import Path; "
        "import wedl.transaction_recovery as recovery; "
        "from wedl.transaction_recovery import Surface,TransactionJournal; "
        "root=Path(sys.argv[1]); "
        "journal=TransactionJournal.create(root,ref='refs/heads/main',previous_head='0'*40,committed_head='1'*40,"
        "surfaces=[Surface('story/world.md',b'before',b'after','source',0o640,0o600)]); "
        "recovery.set_mutation_hook(lambda name: os._exit(0) if name == 'surface-after-create-written' else None); "
        "journal.publish_surfaces()"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root)], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    journal_path, = (root / ".wedl" / "transactions").glob("*.json")
    private_after = journal_path.parent / journal_path.stem / "backups" / "0000.after"
    assert not target.exists() and private_after.read_bytes() == b"after"

    TransactionJournal.load(root, journal_path).restore_surfaces()

    assert target.read_bytes() == b"before"
    assert private_after.read_bytes() == b"after"


@pytest.mark.parametrize("checkpoint", [
    "backup-cleanup-unlink-after-primary-unlink",
    "after-cleanup-unlink-after-primary-unlink",
    "published-cleanup-unlink-after-primary-unlink",
    "staging-cleanup-unlink-after-primary-unlink",
    "journal-cleanup-unlink-after-primary-unlink",
])
def test_real_os_exit_after_each_cleanup_primary_unlink_is_restart_adopted(tmp_path: Path, checkpoint: str) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    (root / "story/world.md").write_bytes(b"before")
    script = (
        "import os,sys; from pathlib import Path; import wedl.transaction_recovery as recovery; "
        "from wedl.transaction_recovery import Surface,TransactionJournal; root=Path(sys.argv[1]); checkpoint=sys.argv[2]; "
        "j=TransactionJournal.create(root,ref='refs/heads/main',previous_head='0'*40,committed_head='1'*40,"
        "surfaces=[Surface('story/world.md',b'before',b'after')]); j.publish_surfaces(); j.restore_surfaces(); "
        "j.write_staging_artifact('private.bin',b'private'); j.seal_staging_artifact('private.bin'); j.finish(); "
        "recovery.set_mutation_hook(lambda name: os._exit(0) if name == checkpoint else None); j.cleanup()"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root), checkpoint], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    # Repeated restart scans must adopt any surviving exact hard-link tombstone
    # and converge without deleting a foreign/private sibling.
    for _ in range(3):
        for journal in TransactionJournal.pending(root):
            journal.cleanup()
    transactions = root / ".wedl" / "transactions"
    assert not transactions.exists() or not tuple(transactions.rglob("*"))


def test_budgeted_zero_image_many_surface_boundary_rejects_before_candidate_work(tmp_path: Path, monkeypatch) -> None:
    """Empty-image enrollments still consume the modeled Surface/container budget."""
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    calls: list[str] = []
    monkeypatch.setattr(transaction_recovery.base64, "b64encode", lambda *_args: calls.append("base64") or b"")
    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: calls.append("json") or b"")
    monkeypatch.setattr(transaction_recovery, "_atomic_write", lambda *_args, **_kwargs: calls.append("atomic") or (0, 0, 0))
    enrollments = [SurfaceEnrollment(f".wedl/empty-{number}") for number in range(1_000)]
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(200_000, 0))
    assert calls == [] and journal.surfaces == ()


def test_budgeted_later_lock_payload_is_admitted_before_descriptor_read(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    lock = root / "lock"; lock.write_text("pid=1\ntoken=token\n", encoding="ascii")
    baseline = journal._require_budgeted_operation("lock", path=str(lock))
    for name in ("liveByteBudget", "$liveByteBudget"):
        journal.record[name]["totalBytes"] = baseline + lock.stat().st_size - 1
        journal.record[name]["admittedPeakBytes"] = 0
    monkeypatch.setattr(transaction_recovery, "_bounded_descriptor_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("read after rejected admission")))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.bind_lock(lock, pid=1, token="token")


def test_budgeted_lock_successor_limit_minus_one_preserves_lock_and_journal(tmp_path: Path) -> None:
    """Lock binding prices its exact detached journal successor before rewrite."""
    token = "t" * 32_768

    def prepared(root: Path, total: int) -> tuple[TransactionJournal, Path]:
        root.mkdir()
        result = TransactionJournal.create(
            root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
        result.register_surfaces(
            [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
            live_budget=JournalLiveByteBudget(total, 0),
        )
        lock = root / "canonical.lock"
        lock.write_text(f"pid=1\ntoken={token}\n", encoding="ascii")
        return result, lock

    admitted, admitted_lock = prepared(tmp_path / "admitted", 8_000_000)
    admitted.bind_lock(admitted_lock, pid=1, token=token)
    peak = admitted.record["liveByteBudget"]["admittedPeakBytes"]
    assert isinstance(peak, int)

    rejected, lock = prepared(tmp_path / "rejected", peak - 1)
    before_record = copy.deepcopy(rejected.record)
    before_bytes = rejected.path.read_bytes()
    before_identity = rejected._journal_identity
    before_cache = rejected._surfaces_cache
    lock_before = lock.read_bytes()
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        rejected.bind_lock(lock, pid=1, token=token)
    assert rejected.record == before_record
    assert rejected.path.read_bytes() == before_bytes
    assert rejected._journal_identity == before_identity
    assert rejected._surfaces_cache is before_cache
    assert lock.read_bytes() == lock_before


def test_budgeted_registration_converts_types_and_post_replace_failure_without_split_state(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    for enrollment in (SurfaceEnrollment(42), SurfaceEnrollment(".wedl/a", role=[])):
        with pytest.raises(RepositoryError):
            journal.register_surfaces([enrollment], live_budget=JournalLiveByteBudget(65_536, 0))
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    previous = journal.record["generation"]
    transaction_recovery.set_mutation_hook(lambda name: (_ for _ in ()).throw(RuntimeError("after replace")) if name == "atomic-replace-after-primary" else None)
    try:
        journal.advance("ref_committed")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert journal.record["generation"] == previous + 1
    assert TransactionJournal.load(root, journal.path).phase == "ref_committed"


def test_budgeted_duplicate_metadata_is_rejected_before_surface_decode(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    record = _journal_record(journal.path)
    record["$liveByteBudget"]["totalBytes"] += 1
    journal.path.write_bytes(_budgeted_payload(record))
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("decoded before duplicate budget validation"))))
    with pytest.raises(RepositoryError, match="live-byte budget"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_zero_image_later_phase_retains_surface_container_admission(tmp_path: Path) -> None:
    """A later persist must not forget the zero-image enrollment containers."""
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    enrollments = [SurfaceEnrollment(f".wedl/empty-{number}") for number in range(24)]
    journal.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(1_000_000, 0))
    exact = journal._require_budgeted_operation("phase", path="ref_committed")
    for name in ("liveByteBudget", "$liveByteBudget"):
        journal.record[name]["totalBytes"] = exact
        journal.record[name]["admittedPeakBytes"] = exact
    journal.advance("ref_committed")

    rejected = TransactionJournal.load(root, journal.path)
    exact = rejected._require_budgeted_operation("phase", path="sources_published")
    for name in ("liveByteBudget", "$liveByteBudget"):
        rejected.record[name]["totalBytes"] = exact - 1
        rejected.record[name]["admittedPeakBytes"] = exact - 1
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        rejected.advance("sources_published")


def test_budgeted_staging_payload_is_rejected_before_artifact_registration(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    payload = b"private" * 128
    limit = journal._require_budgeted_operation("private-artifact", path="private.sqlite", payload_bytes=len(payload)) - 1
    for name in ("liveByteBudget", "$liveByteBudget"):
        journal.record[name]["totalBytes"] = limit
        journal.record[name]["admittedPeakBytes"] = 0
    monkeypatch.setattr(transaction_recovery, "_create_private_file", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("wrote after rejected admission")))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.write_staging_artifact("private.sqlite", payload)
    assert journal.record["privateArtifacts"] == []


def test_budgeted_reload_rejects_many_empty_surfaces_before_surface_decode(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(f".wedl/empty-{number}") for number in range(48)], live_budget=JournalLiveByteBudget(1_000_000, 0))
    record = _journal_record(journal.path)
    surfaces = record["surfaces"]
    assert isinstance(surfaces, list)
    # The cap itself is represented in the canonical payload, so converge the
    # final serialized length before asserting the boundary.
    cap = 1
    for _ in range(3):
        for name in ("liveByteBudget", "$liveByteBudget"):
            record[name]["totalBytes"] = cap
            record[name]["admittedPeakBytes"] = 0
        candidate = _budgeted_payload(record)
        cap = transaction_recovery._projected_live_sizes(
            [], caller_reserve=0, journal_bytes=len(candidate),
            path_sizes=[sys.getsizeof(str(surface["path"])) for surface in surfaces], surface_count=len(surfaces),
        ) - 1
    for name in ("liveByteBudget", "$liveByteBudget"):
        record[name]["totalBytes"] = cap
        record[name]["admittedPeakBytes"] = 0
    journal.path.write_bytes(_budgeted_payload(record))
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("decoded oversized container"))))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_owned_lock_check_uses_admitted_descriptor_read(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    lock = root / "owned.lock"; token = "11111111-1111-1111-1111-111111111111"
    lock.write_text(f"pid=7\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=7, token=token)
    monkeypatch.setattr(Path, "read_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("budgeted lock used read_bytes")))
    assert journal.owns_canonical_lock(lock, pid=7, token=token)


def test_budgeted_persist_adopts_primary_generation_if_wrapper_raises_after_install(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    actual = transaction_recovery._atomic_write

    def install_then_fail(*args, **kwargs):
        actual(*args, **kwargs)
        raise MemoryError("wrapper verifier allocation failed")

    monkeypatch.setattr(transaction_recovery, "_atomic_write", install_then_fail)
    journal.advance("ref_committed")
    assert journal.phase == "ref_committed"
    assert TransactionJournal.load(root, journal.path).phase == "ref_committed"


def test_budgeted_envelope_rejects_structural_count_before_json_load(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    payload = journal.path.read_bytes()
    payload = payload.replace(b"surfaces=1 ", b"surfaces=2 ", 1)
    journal.path.write_bytes(payload)
    monkeypatch.setattr(transaction_recovery.json, "loads", lambda *_args: (_ for _ in ()).throw(AssertionError("JSON parsed unauthenticated envelope")))
    with pytest.raises(RepositoryError, match="live-byte budget"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_cleanup_leaves_marker_free_temp_without_unbounded_read(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    stray = journal.path.parent / f".wedl-txn-{journal.transaction_id}-marker-free"
    stray.write_bytes(b"x" * 65_537)
    monkeypatch.setattr(Path, "read_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("marker-free temp read")))
    assert TransactionJournal.load(root, journal.path).transaction_id == journal.transaction_id
    assert stray.exists()


def test_budgeted_cleanup_verifies_surface_tombstones_without_path_read_bytes(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment("story/world.md", capture_before=True, max_before_bytes=64, after=b"after", role="source")],
        live_budget=JournalLiveByteBudget(1_000_000, 0),
    )
    journal.publish_surfaces(); journal.restore_surfaces(); journal.finish()
    monkeypatch.setattr(Path, "read_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("budgeted cleanup used read_bytes")))
    journal.cleanup()
    assert not journal.path.exists()


def test_budgeted_stale_lock_reclaim_uses_admitted_descriptor_reads(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    lock = root / "owned.lock"; token = "11111111-1111-1111-1111-111111111111"
    lock.write_text(f"pid=7\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=7, token=token)
    monkeypatch.setattr(transaction_recovery, "_process_dead", lambda _pid: True)
    monkeypatch.setattr(Path, "read_bytes", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("stale lock read_bytes")))
    assert TransactionJournal.reclaim_owned_stale_lock(root, lock)
    assert not lock.exists()


def test_budgeted_registration_memory_error_during_preflight_preserves_journal(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    before_record = json.loads(json.dumps(journal.record))
    before_bytes = journal.path.read_bytes()
    monkeypatch.setattr(transaction_recovery, "_bounded_before_metadata", lambda *_args: (_ for _ in ()).throw(MemoryError()))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces([SurfaceEnrollment("story/world.md", capture_before=True, max_before_bytes=64)], live_budget=JournalLiveByteBudget(65_536, 0))
    assert journal.record == before_record and journal.path.read_bytes() == before_bytes


def test_budgeted_registration_candidate_memory_error_preserves_exact_live_journal(tmp_path: Path, monkeypatch) -> None:
    """Candidate construction must not mutate the live record before persistence."""
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    record, cache = journal.record, journal.surfaces
    journal_bytes, identity = journal._journal_bytes, journal._journal_identity
    durable = journal.path.read_bytes()
    actual = transaction_recovery._canonical_journal_payload

    def fail_candidate(value):
        if value is not record and value.get("version") == transaction_recovery.BUDGETED_FORMAT_VERSION:
            raise MemoryError("candidate payload allocation")
        return actual(value)

    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", fail_candidate)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces(
            [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
            live_budget=JournalLiveByteBudget(1_000_000, 0),
        )
    assert journal.record is record and journal.surfaces is cache
    assert journal._journal_bytes is journal_bytes and journal._journal_identity == identity
    assert journal.path.read_bytes() == durable


def test_budgeted_envelope_bounds_dense_non_surface_json_before_load(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    record = _journal_record(journal.path)
    # This field is intentionally schema-invalid.  The loader must nevertheless
    # price its dense parsed graph before json.loads can construct it.
    record["dense"] = [[] for _ in range(1_024)]
    for _ in range(3):
        payload = _budgeted_payload(record)
        header = transaction_recovery._budget_header_from_prefix(payload[len(transaction_recovery.BUDGETED_ENVELOPE):])
        assert header is not None
        document = payload[len(transaction_recovery.BUDGETED_ENVELOPE) + transaction_recovery._budget_header_length(header):]
        total = transaction_recovery._checked_live_sum(
            5 * len(payload), transaction_recovery._budgeted_json_graph_upper_bound(document),
        ) - 1
        for name in ("liveByteBudget", "$liveByteBudget"):
            record[name]["totalBytes"] = total
            record[name]["admittedPeakBytes"] = 0
    journal.path.write_bytes(_budgeted_payload(record))
    monkeypatch.setattr(transaction_recovery.json, "loads", lambda *_args: (_ for _ in ()).throw(AssertionError("dense JSON was parsed")))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        TransactionJournal.load(root, journal.path)


def test_budgeted_temp_cleanup_streams_entries_before_requesting_next(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    directory = journal.path.parent
    first = directory / f".wedl-txn-{journal.transaction_id}-marker-free"
    second = directory / "unrelated-entry"
    first.write_bytes(b"foreign")
    second.write_bytes(b"ignored")
    initial_checked = False
    actual_iterdir, actual_lstat = Path.iterdir, transaction_recovery.os.lstat

    def ordered_iterdir(path: Path):
        if path != directory:
            return actual_iterdir(path)
        def entries():
            yield first
            assert initial_checked, "cleanup materialized directory entries before filtering/admission"
            yield second
        return entries()

    def tracked_lstat(path, *args, **kwargs):
        nonlocal initial_checked
        if Path(path) == first:
            initial_checked = True
        return actual_lstat(path, *args, **kwargs)

    monkeypatch.setattr(Path, "iterdir", ordered_iterdir)
    monkeypatch.setattr(transaction_recovery.os, "lstat", tracked_lstat)
    journal._cleanup_authenticated_atomic_temps()
    assert initial_checked and first.exists() and second.exists()


def test_budgeted_temp_cleanup_admits_each_candidate_before_decode_and_processes_one_at_a_time(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(1_000_000, 0),
    )
    record = _journal_record(journal.path)
    record["generation"] += 1
    payload = _budgeted_payload(record)
    first = journal.path.parent / f".wedl-txn-{journal.transaction_id}-first"
    second = journal.path.parent / f".wedl-txn-{journal.transaction_id}-second"
    first.write_bytes(payload); second.write_bytes(payload)
    admitted: list[Path] = []
    loaded: list[Path] = []
    actual_require = TransactionJournal._require_budgeted_operation
    actual_load = transaction_recovery._load_journal_payload

    def require(self, operation, **kwargs):
        if self is journal and operation == "cleanup":
            admitted.append(Path(kwargs["path"]))
        return actual_require(self, operation, **kwargs)

    def load(path, **kwargs):
        assert Path(path) in admitted, "candidate was decoded before cleanup-ledger admission"
        loaded.append(Path(path))
        return actual_load(path, **kwargs)

    monkeypatch.setattr(TransactionJournal, "_require_budgeted_operation", require)
    monkeypatch.setattr(transaction_recovery, "_load_journal_payload", load)
    journal._cleanup_authenticated_atomic_temps()
    assert admitted == [first, second] and loaded == [first, second]
    assert not first.exists() and not second.exists()


def test_budgeted_temp_cleanup_uses_current_journal_budget_before_candidate_parse(tmp_path: Path, monkeypatch) -> None:
    """A forged temp must not use its own larger envelope to buy a parse."""
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    candidate_record = _journal_record(journal.path)
    candidate_record["generation"] += 1
    candidate_record["dense"] = [[] for _ in range(2_048)]
    for name in ("liveByteBudget", "$liveByteBudget"):
        candidate_record[name]["totalBytes"] = 10_000_000
        candidate_record[name]["admittedPeakBytes"] = 10_000_000
    candidate = journal.path.parent / f".wedl-txn-{journal.transaction_id}-oversized-graph"
    candidate.write_bytes(_budgeted_payload(candidate_record))
    monkeypatch.setattr(
        transaction_recovery.json, "loads",
        lambda *_args: (_ for _ in ()).throw(AssertionError("candidate parsed under its forged envelope")),
    )
    journal._cleanup_authenticated_atomic_temps()
    assert candidate.exists()


def test_budgeted_later_record_update_limit_minus_one_preserves_exact_state(tmp_path: Path) -> None:
    """Later staging bookkeeping is detached when its next generation exceeds cap."""
    name = "x" * 300_000

    def prepared(root: Path, total: int) -> TransactionJournal:
        root.mkdir()
        result = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
        result.register_surfaces(
            [SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")],
            live_budget=JournalLiveByteBudget(total, 0),
        )
        return result

    admitted = prepared(tmp_path / "admitted", 8_000_000)
    admitted.staging_path(name)
    peak = admitted.record["liveByteBudget"]["admittedPeakBytes"]
    assert isinstance(peak, int)

    rejected = prepared(tmp_path / "rejected", 8_000_000)
    for budget_name in ("liveByteBudget", "$liveByteBudget"):
        rejected.record[budget_name]["totalBytes"] = peak - 1
        rejected.record[budget_name]["admittedPeakBytes"] = min(
            rejected.record[budget_name]["admittedPeakBytes"], peak - 1)
    record = rejected.record
    snapshot = copy.deepcopy(record)
    durable = rejected.path.read_bytes()
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        rejected.staging_path(name)
    assert rejected.record is record
    assert rejected.record == snapshot
    assert rejected.path.read_bytes() == durable
    assert not (rejected.root / ".wedl" / "transactions" / rejected.transaction_id / "staging").exists()


def test_budgeted_surface_successor_limit_minus_one_preserves_publish_and_restore_witnesses(tmp_path: Path) -> None:
    """Seal-successor admission precedes every publish/restore artifact mutation."""
    def prepared(root: Path) -> tuple[TransactionJournal, Path]:
        root.mkdir(); (root / "story").mkdir()
        target = root / "story/world.md"; target.write_bytes(b"before")
        journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
        journal.register_surfaces(
            [SurfaceEnrollment("story/world.md", capture_before=True, max_before_bytes=64, after=b"after", role="source")],
            live_budget=JournalLiveByteBudget(1_000_000, 0),
        )
        return journal, target

    admitted, _ = prepared(tmp_path / "admitted-publish")
    admitted.publish_surfaces()
    publish_peak = admitted.record["liveByteBudget"]["admittedPeakBytes"]
    assert isinstance(publish_peak, int)

    rejected, target = prepared(tmp_path / "rejected-publish")
    for name in ("liveByteBudget", "$liveByteBudget"):
        rejected.record[name]["totalBytes"] = publish_peak - 1
    record, durable, before = copy.deepcopy(rejected.record), rejected.path.read_bytes(), target.read_bytes()
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        rejected.publish_surfaces()
    assert rejected.record == record and rejected.path.read_bytes() == durable and target.read_bytes() == before
    assert not (rejected.root / ".wedl" / "transactions" / rejected.transaction_id).exists()

    admitted, _ = prepared(tmp_path / "admitted-restore")
    admitted.publish_surfaces(); admitted.restore_surfaces()
    restore_peak = admitted.record["liveByteBudget"]["admittedPeakBytes"]
    assert isinstance(restore_peak, int)

    rejected, target = prepared(tmp_path / "rejected-restore")
    rejected.publish_surfaces()
    for name in ("liveByteBudget", "$liveByteBudget"):
        rejected.record[name]["totalBytes"] = restore_peak - 1
    record, durable, after = copy.deepcopy(rejected.record), rejected.path.read_bytes(), target.read_bytes()
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        rejected.restore_surfaces()
    assert rejected.record == record and rejected.path.read_bytes() == durable and target.read_bytes() == after


def test_budgeted_stale_lock_claim_limit_minus_one_preserves_lock_and_namespace(tmp_path: Path, monkeypatch) -> None:
    """Stale-lock admission happens before creating the private claim directory."""
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    lock = root / "owned.lock"; token = "11111111-1111-1111-1111-111111111111"
    lock.write_text(f"pid=7\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=7, token=token)
    before_record, before_journal, before_lock = copy.deepcopy(journal.record), journal.path.read_bytes(), lock.read_bytes()
    monkeypatch.setattr(transaction_recovery, "_process_dead", lambda _pid: True)
    actual_admission = TransactionJournal._require_budgeted_operation

    def reject_claim_admission(self, operation, *, path="", payload_bytes=0):
        if operation == "lock" and path == str(lock):
            raise RepositoryError("transaction journal live-byte budget exceeded")
        return actual_admission(self, operation, path=path, payload_bytes=payload_bytes)

    monkeypatch.setattr(TransactionJournal, "_require_budgeted_operation", reject_claim_admission)
    assert not TransactionJournal.reclaim_owned_stale_lock(root, lock)
    assert journal.record == before_record and journal.path.read_bytes() == before_journal and lock.read_bytes() == before_lock
    assert not (root / ".wedl" / "transactions" / journal.transaction_id / "lock-claims").exists()


@pytest.mark.parametrize("operation", ("phase", "publish", "restore"))
def test_budgeted_successor_rejection_precedes_json_base64_and_atomic_seams(
        tmp_path: Path, monkeypatch, operation: str) -> None:
    """Later successor admission never serializes merely to discover overflow."""
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces(
        [SurfaceEnrollment("story/world.md", capture_before=True, max_before_bytes=64, after=b"after", role="source")],
        live_budget=JournalLiveByteBudget(2_000_000, 0),
    )
    if operation == "restore":
        journal.publish_surfaces()
        assert target.read_bytes() == b"after"
    journal.record["privateArtifacts"] = ["x" * 300_000]
    before_record, before_journal, before_target = copy.deepcopy(journal.record), journal.path.read_bytes(), target.read_bytes()
    calls: list[str] = []
    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: calls.append("json") or b"")
    monkeypatch.setattr(transaction_recovery.base64, "b64encode", lambda *_args: calls.append("base64") or b"")
    monkeypatch.setattr(transaction_recovery, "_atomic_write", lambda *_args, **_kwargs: calls.append("atomic") or (0, 0, 0))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        if operation == "phase":
            journal.advance("ref_committed")
        elif operation == "publish":
            journal.publish_surfaces()
        else:
            journal.restore_surfaces()
    assert calls == []
    assert journal.record == before_record and journal.path.read_bytes() == before_journal and target.read_bytes() == before_target


@pytest.mark.parametrize("operation", ("phase", "publish", "restore"))
def test_budgeted_successor_exact_projection_minus_one_rejects_before_serialization(
        tmp_path: Path, monkeypatch, operation: str) -> None:
    """The no-serialization bound dominates the exact successor projection."""
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment("story/world.md", capture_before=True, max_before_bytes=64, after=b"after", role="source")], live_budget=JournalLiveByteBudget(1_000_000, 0))
    if operation == "restore":
        journal.publish_surfaces()
    record = journal._detached_record()
    if operation == "phase":
        record["phase"] = "ref_committed"
    else:
        record["privateArtifacts"] = ["x" * 64]
    record["generation"] = int(record["generation"]) + 1
    images, paths = transaction_recovery._encoded_surface_image_sizes(record["surfaces"])
    # Changing totalBytes is itself canonical JSON/header input.  Converge the
    # exact successor projection before asserting its one-less rejection.
    exact = 1_000_000
    for _attempt in range(4):
        for name in ("liveByteBudget", "$liveByteBudget"):
            journal.record[name]["totalBytes"] = exact - 1
            journal.record[name]["admittedPeakBytes"] = min(journal.record[name]["admittedPeakBytes"], exact - 1)
        candidate = journal._detached_record()
        if operation == "phase": candidate["phase"] = "ref_committed"
        else: candidate["privateArtifacts"] = ["x" * 64]
        candidate["generation"] = int(candidate["generation"]) + 1
        payload = transaction_recovery._canonical_journal_payload(candidate)
        next_exact = transaction_recovery._projected_live_sizes(images, caller_reserve=0,
            journal_bytes=max(len(journal._journal_bytes or b""), len(payload)), path_sizes=paths,
            surface_count=len(candidate["surfaces"]), json_graph_bytes=transaction_recovery._budgeted_payload_json_graph(payload))
        if next_exact == exact:
            break
        exact = next_exact
    else:
        pytest.fail("successor exact projection did not converge")
    calls: list[str] = []
    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: calls.append("json") or b"")
    monkeypatch.setattr(transaction_recovery.base64, "b64encode", lambda *_args: calls.append("base64") or b"")
    monkeypatch.setattr(transaction_recovery, "_atomic_write", lambda *_args, **_kwargs: calls.append("atomic") or (0, 0, 0))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        if operation == "phase": journal.advance("ref_committed")
        elif operation == "publish": journal.publish_surfaces()
        else: journal.restore_surfaces()
    assert calls == []


def _resigned_budgeted_document(document: bytes, header: tuple[int, int, int, int, int, bytes]) -> bytes:
    total, reserve, admitted, count, _old_bytes, _old_digest = header
    envelope = (
        f"total={total} reserve={reserve} admitted={admitted} surfaces={count} "
        f"bytes={len(document)} sha256={transaction_recovery.sha256_bytes(document)}\n"
    ).encode("ascii")
    return transaction_recovery.BUDGETED_ENVELOPE + envelope + document


@pytest.mark.parametrize("mutation", ("whitespace", "duplicate-key"))
def test_budgeted_envelope_rejects_noncanonical_or_duplicate_keys_before_json_or_surface_decode(
        tmp_path: Path, monkeypatch, mutation: str) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    payload = journal.path.read_bytes()
    header = transaction_recovery._budget_header_from_prefix(payload[len(transaction_recovery.BUDGETED_ENVELOPE):])
    assert header is not None
    offset = len(transaction_recovery.BUDGETED_ENVELOPE) + transaction_recovery._budget_header_length(header)
    document = payload[offset:]
    if mutation == "whitespace":
        document = document.replace(b",", b", ", 1)
    else:
        document = document[:-2] + b',"phase":"prepared"}\n'
    journal.path.write_bytes(_resigned_budgeted_document(document, header))
    monkeypatch.setattr(transaction_recovery.json, "loads", lambda *_args: (_ for _ in ()).throw(AssertionError("parsed noncanonical journal")))
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("decoded noncanonical journal"))))
    with pytest.raises(RepositoryError, match="live-byte budget"):
        TransactionJournal.load(root, journal.path)


@pytest.mark.parametrize("mutation", ("escaped-equivalent", "reordered", "nested-duplicate"))
def test_budgeted_envelope_rejects_semantic_noncanonical_keys_before_document_decode(
        tmp_path: Path, monkeypatch, mutation: str) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    payload = journal.path.read_bytes()
    header = transaction_recovery._budget_header_from_prefix(payload[len(transaction_recovery.BUDGETED_ENVELOPE):])
    assert header is not None
    offset = len(transaction_recovery.BUDGETED_ENVELOPE) + transaction_recovery._budget_header_length(header)
    document = payload[offset:]
    if mutation == "escaped-equivalent":
        document = document.replace(b'"phase"', b'"ph\\u0061se"', 1)
    elif mutation == "reordered":
        document = document.replace(b'"committedHead":"' + b"1" * 40 + b'","generation"',
                                    b'"generation":0,"committedHead":"' + b"1" * 40 + b'"', 1)
    else:
        document = document.replace(b'"liveByteBudget":{', b'"liveByteBudget":{"totalBytes":1,"totalBytes":1,', 1)
    journal.path.write_bytes(_resigned_budgeted_document(document, header))
    monkeypatch.setattr(transaction_recovery.json, "loads", lambda *_args: (_ for _ in ()).throw(AssertionError("decoded semantic noncanonical journal")))
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("decoded semantic noncanonical surface"))))
    with pytest.raises(RepositoryError, match="live-byte budget"):
        TransactionJournal.load(root, journal.path)


def test_candidate_path_canonical_json_expansion_prices_nonbmp_and_controls_at_exact_boundary(tmp_path: Path) -> None:
    path = "story/\U0001f642-\x01-\t.md"
    assert transaction_recovery._canonical_json_string_bytes(path) == len(
        json.dumps(path, ensure_ascii=True, separators=(",", ":")).encode("utf-8"))
    enrollment = SurfaceEnrollment(path, after=b"cache", role="cache")

    def fresh(name: str) -> TransactionJournal:
        root = tmp_path / name; root.mkdir()
        return TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)

    def exact_cap(journal: TransactionJournal) -> int:
        """Converge the complete fresh-v6 preflight, not a later v7 return value."""
        preflight = [(transaction_recovery._validate_path(path), enrollment, None)]
        limit = 1_000_000
        for _ in range(4):
            payload, graph = transaction_recovery._budgeted_initial_successor_upper_bounds(
                journal.record, additions=preflight, total=limit, reserve=0)
            projection = max(transaction_recovery._projected_live_ledger(
                [len(b"cache")], caller_reserve=0,
                journal_bytes=max(len(journal._journal_bytes or b""), payload),
                path_sizes=[sys.getsizeof(path)], surface_count=1,
                json_graph_bytes=graph,
            ).values())
            if projection == limit:
                return limit
            limit = projection
        raise AssertionError("fresh v6 exact-cap preflight did not converge")

    exact = fresh("exact")
    limit = exact_cap(exact)
    assert exact.register_surfaces([enrollment], live_budget=JournalLiveByteBudget(limit, 0)) <= limit
    rejected = fresh("rejected")
    rejected_limit = exact_cap(rejected)
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        rejected.register_surfaces(
            [enrollment], live_budget=JournalLiveByteBudget(rejected_limit - 1, 0),
        )


def test_budgeted_envelope_binds_parsed_budget_and_surface_count_to_authenticated_header(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surfaces([SurfaceEnrollment(".wedl/cache.sqlite", after=b"cache", role="cache")], live_budget=JournalLiveByteBudget(65_536, 0))
    payload = journal.path.read_bytes()
    header = transaction_recovery._budget_header_from_prefix(payload[len(transaction_recovery.BUDGETED_ENVELOPE):])
    assert header is not None
    offset = len(transaction_recovery.BUDGETED_ENVELOPE) + transaction_recovery._budget_header_length(header)
    document = payload[offset:]
    total, reserve, admitted, count, document_bytes, digest = header
    forged = (total, reserve, admitted + 1, count, document_bytes, digest)
    journal.path.write_bytes(_resigned_budgeted_document(document, forged))
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("decoded header-mismatched journal"))))
    with pytest.raises(RepositoryError, match="live-byte budget"):
        TransactionJournal.load(root, journal.path)


def test_v6_to_v7_candidate_preflight_rejects_before_surface_base64_json_or_atomic_write(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(
        root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface(".wedl/retained", None, b"retained", "cache")],
    )
    raw = journal.record["surfaces"]
    assert isinstance(raw, list)
    existing_images, paths = transaction_recovery._encoded_surface_image_sizes(raw)
    new_after = b"new-cache"
    images = [*existing_images, len(new_after)]
    relative = ".wedl/new-cache"
    candidate_payload = transaction_recovery._budgeted_candidate_payload_upper_bound(
        current_bytes=len(journal._journal_bytes or b""), version=transaction_recovery.FORMAT_VERSION,
        existing_surfaces=len(raw), additions=((relative, None, len(new_after)),),
    )
    boundary = transaction_recovery._projected_live_sizes(
        images, caller_reserve=0, journal_bytes=max(len(journal._journal_bytes or b""), candidate_payload),
        path_sizes=[*paths, sys.getsizeof(relative)], surface_count=2,
    )
    # First obtain the converged v7 ledger boundary, then prove that exact final
    # boundary on a fresh v6 journal.  The separate candidate-envelope
    # boundary below is earlier and deliberately rejects before conversion.
    accepted_root = tmp_path / "accepted"; accepted_root.mkdir()
    accepted = TransactionJournal.create(
        accepted_root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface(".wedl/retained", None, b"retained", "cache")],
    )
    exact_boundary = accepted.register_surfaces(
        [SurfaceEnrollment(relative, after=new_after, role="cache")],
        live_budget=JournalLiveByteBudget(65_536, 0),
    )
    exact_root = tmp_path / "exact"; exact_root.mkdir()
    exact = TransactionJournal.create(
        exact_root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface(".wedl/retained", None, b"retained", "cache")],
    )
    assert exact.register_surfaces(
        [SurfaceEnrollment(relative, after=new_after, role="cache")],
        live_budget=JournalLiveByteBudget(exact_boundary, 0),
    ) == exact_boundary
    before = journal.path.read_bytes()
    monkeypatch.setattr(transaction_recovery.Surface, "from_json", classmethod(lambda *_args: (_ for _ in ()).throw(AssertionError("decoded v6 surface before candidate admission"))))
    monkeypatch.setattr(transaction_recovery.base64, "b64encode", lambda *_args: (_ for _ in ()).throw(AssertionError("base64 before candidate admission")))
    monkeypatch.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: (_ for _ in ()).throw(AssertionError("JSON before candidate admission")))
    monkeypatch.setattr(transaction_recovery, "_atomic_write", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("atomic write before candidate admission")))
    with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
        journal.register_surfaces([SurfaceEnrollment(relative, after=new_after, role="cache")], live_budget=JournalLiveByteBudget(boundary - 1, 0))
    assert journal.path.read_bytes() == before


def test_v6_initial_successor_bound_prices_zero_empty_and_captured_batch_before_serialization(tmp_path: Path, monkeypatch) -> None:
    """The scalar v6 preflight dominates the later exact v7 payload and graph."""
    enrollments = (
        SurfaceEnrollment(".wedl/no-image", role="receipt"),
        SurfaceEnrollment(".wedl/empty-image", after=b"", role="cache"),
        SurfaceEnrollment("story/captured.md", capture_before=True, max_before_bytes=16 * 1024,
                          after=b"after", role="source"),
    )

    def fresh(name: str) -> TransactionJournal:
        root = tmp_path / name; root.mkdir()
        target = root / "story/captured.md"; target.parent.mkdir(); target.write_bytes(b"b" * (16 * 1024))
        return TransactionJournal.create(
            root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
            surfaces=[Surface("story/retained.md", b"old", b"retained", "source")],
        )

    def upper_bound(journal: TransactionJournal, total: int) -> tuple[int, int, int, list[int], list[int]]:
        raw = journal.record["surfaces"]
        assert isinstance(raw, list)
        preflight = [
            (transaction_recovery._validate_path(enrollment.path), enrollment,
             transaction_recovery._bounded_before_metadata(journal.root, enrollment.path))
            for enrollment in enrollments
        ]
        images, paths = transaction_recovery._encoded_surface_image_sizes(raw)
        images.extend(
            size for _relative, enrollment, metadata in preflight for size in (
                (metadata.size if metadata is not None else None) if enrollment.capture_before else (
                    len(enrollment.before) if enrollment.before is not None else None),
                len(enrollment.after) if enrollment.after is not None else None,
            ) if size is not None
        )
        paths.extend(sys.getsizeof(relative) for relative, _enrollment, _metadata in preflight)
        payload, graph = transaction_recovery._budgeted_initial_successor_upper_bounds(
            journal.record, additions=preflight, total=total, reserve=0)
        projected = max(transaction_recovery._projected_live_ledger(
            images, caller_reserve=0,
            journal_bytes=max(len(journal._journal_bytes or b""), payload),
            path_sizes=paths, surface_count=len(raw) + len(preflight),
            json_graph_bytes=graph,
        ).values())
        return projected, payload, graph, images, paths

    def stable_limit(journal: TransactionJournal) -> int:
        # The decimal budget fields are part of both mirrored budget maps, so
        # converge the exact cap without serializing a candidate record.  File
        # identity integers are deliberately included, hence each fresh v6
        # journal obtains its own scalar cap.
        limit = 1_000_000
        for _ in range(4):
            projection, _payload, _graph, _images, _paths = upper_bound(journal, limit)
            if projection == limit:
                return limit
            limit = projection
        raise AssertionError("initial v7 budget cap did not converge")

    rejected = fresh("rejected")
    rejected_limit = stable_limit(rejected)
    rejected_projection, _payload, _graph, _images, _paths = upper_bound(rejected, rejected_limit)
    assert rejected_projection == rejected_limit
    durable = rejected.path.read_bytes()
    with monkeypatch.context() as hooks:
        hooks.setattr(transaction_recovery.Surface, "from_json", classmethod(
            lambda *_args: (_ for _ in ()).throw(AssertionError("constructed Surface before exact v7 preflight"))))
        hooks.setattr(transaction_recovery.base64, "b64encode", lambda *_args: (_ for _ in ()).throw(
            AssertionError("base64 before exact v7 preflight")))
        hooks.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: (_ for _ in ()).throw(
            AssertionError("canonical JSON before exact v7 preflight")))
        hooks.setattr(transaction_recovery, "_atomic_write", lambda *_args, **_kwargs: (_ for _ in ()).throw(
            AssertionError("atomic replacement before exact v7 preflight")))
        with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
            rejected.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(rejected_limit - 1, 0))
    assert rejected.path.read_bytes() == durable

    admitted = fresh("admitted")
    admitted_limit = stable_limit(admitted)
    admitted_projection, upper_payload, upper_graph, images, paths = upper_bound(admitted, admitted_limit)
    assert admitted_projection == admitted_limit
    peak = admitted.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(admitted_limit, 0))
    actual_payload = admitted._journal_bytes
    assert actual_payload is not None and upper_payload >= len(actual_payload)
    actual_graph = transaction_recovery._budgeted_payload_json_graph(actual_payload)
    assert upper_graph >= actual_graph
    actual_projection = max(transaction_recovery._projected_live_ledger(
        images, caller_reserve=0,
        journal_bytes=max(len(durable), len(actual_payload)), path_sizes=paths,
        surface_count=len(admitted.surfaces), json_graph_bytes=actual_graph,
    ).values())
    assert admitted_limit >= actual_projection and peak <= admitted_limit


def test_v7_repeated_successor_bound_prices_zero_empty_and_captured_batch_before_serialization(tmp_path: Path, monkeypatch) -> None:
    """Later v7 enrollment must admit its complete wire/parsed envelope first."""
    enrollments = (
        SurfaceEnrollment(".wedl/no-image", role="receipt"),
        SurfaceEnrollment(".wedl/empty-image", after=b"", role="cache"),
        SurfaceEnrollment("story/captured.md", capture_before=True, max_before_bytes=4096,
                          after=b"after", role="source"),
    )

    def fresh(name: str, total: int) -> tuple[TransactionJournal, int]:
        root = tmp_path / name; root.mkdir()
        target = root / "story/captured.md"; target.parent.mkdir(); target.write_bytes(b"b" * 4096)
        journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
        seed_peak = journal.register_surfaces(
            [SurfaceEnrollment(".wedl/retained", after=b"retained", role="cache")],
            live_budget=JournalLiveByteBudget(total, 0),
        )
        return journal, seed_peak

    def repeated_projection(journal: TransactionJournal, total: int) -> int:
        raw = journal.record["surfaces"]
        assert isinstance(raw, list)
        preflight = [
            (transaction_recovery._validate_path(enrollment.path), enrollment,
             transaction_recovery._bounded_before_metadata(journal.root, enrollment.path))
            for enrollment in enrollments
        ]
        images, paths = transaction_recovery._encoded_surface_image_sizes(raw)
        images.extend(
            size for _relative, enrollment, metadata in preflight for size in (
                (metadata.size if metadata is not None else None) if enrollment.capture_before else (
                    len(enrollment.before) if enrollment.before is not None else None),
                len(enrollment.after) if enrollment.after is not None else None,
            ) if size is not None
        )
        paths.extend(sys.getsizeof(relative) for relative, _enrollment, _metadata in preflight)
        payload, graph = transaction_recovery._budgeted_enrollment_successor_upper_bounds(
            journal.record, additions=preflight, total=total, reserve=0)
        return max(transaction_recovery._projected_live_ledger(
            images, caller_reserve=0,
            journal_bytes=max(len(journal._journal_bytes or b""), payload),
            path_sizes=paths, surface_count=len(raw) + len(preflight),
            json_graph_bytes=graph,
        ).values())

    limit = 1_000_000
    for attempt in range(4):
        calibration, seed_peak = fresh(f"calibration-{attempt}", limit)
        next_limit = max(seed_peak, repeated_projection(calibration, limit))
        if next_limit == limit:
            break
        limit = next_limit
    else:
        raise AssertionError("repeated v7 budget cap did not converge")

    admitted, _seed_peak = fresh("admitted", limit)
    assert repeated_projection(admitted, limit) == limit
    assert admitted.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(limit, 0)) <= limit

    rejected, rejected_seed_peak = fresh("rejected", limit - 1)
    assert rejected_seed_peak <= limit - 1
    assert repeated_projection(rejected, limit - 1) > limit - 1
    durable = rejected.path.read_bytes()
    with monkeypatch.context() as hooks:
        hooks.setattr(transaction_recovery.Surface, "from_json", classmethod(
            lambda *_args: (_ for _ in ()).throw(AssertionError("decoded retained v7 surface before admission"))))
        hooks.setattr(transaction_recovery.base64, "b64encode", lambda *_args: (_ for _ in ()).throw(
            AssertionError("base64 before repeated-v7 admission")))
        hooks.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: (_ for _ in ()).throw(
            AssertionError("canonical JSON before repeated-v7 admission")))
        hooks.setattr(transaction_recovery, "_atomic_write", lambda *_args, **_kwargs: (_ for _ in ()).throw(
            AssertionError("atomic replacement before repeated-v7 admission")))
        with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
            rejected.register_surfaces(enrollments, live_budget=JournalLiveByteBudget(limit - 1, 0))
    assert rejected.path.read_bytes() == durable


def test_v7_repeated_rejection_scans_large_retained_wire_without_payload_copy(tmp_path: Path, monkeypatch) -> None:
    """Repeated v7 preflight must not copy retained base64 before rejection."""
    retained = b"r" * (1024 * 1024)
    enrollment = SurfaceEnrollment(".wedl/later", after=b"x", role="cache")

    def fresh(name: str, total: int) -> TransactionJournal:
        root = tmp_path / name; root.mkdir()
        journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
        journal.register_surfaces(
            [SurfaceEnrollment(".wedl/retained", after=retained, role="cache")],
            live_budget=JournalLiveByteBudget(total, 0),
        )
        return journal

    def repeated_projection(journal: TransactionJournal, total: int) -> int:
        raw = journal.record["surfaces"]
        assert isinstance(raw, list)
        images, paths = transaction_recovery._encoded_surface_image_sizes(raw)
        images.append(len(enrollment.after or b""))
        paths.append(sys.getsizeof(enrollment.path))
        additions = [(transaction_recovery._validate_path(enrollment.path), enrollment, None)]
        payload, graph = transaction_recovery._budgeted_enrollment_successor_upper_bounds(
            journal.record, additions=additions, total=total, reserve=0)
        return max(transaction_recovery._projected_live_ledger(
            images, caller_reserve=0,
            journal_bytes=max(len(journal._journal_bytes or b""), payload),
            path_sizes=paths, surface_count=len(raw) + 1, json_graph_bytes=graph,
        ).values())

    limit = 20_000_000
    for attempt in range(4):
        calibration = fresh(f"calibration-{attempt}", limit)
        next_limit = repeated_projection(calibration, limit)
        if next_limit == limit:
            break
        limit = next_limit
    else:
        pytest.fail("large retained v7 cap did not converge")

    rejected = fresh("rejected", limit - 1)
    assert repeated_projection(rejected, limit - 1) > limit - 1
    durable = rejected.path.read_bytes()
    with monkeypatch.context() as hooks:
        hooks.setattr(transaction_recovery.Surface, "from_json", classmethod(
            lambda *_args: (_ for _ in ()).throw(AssertionError("decoded retained v7 surface before admission"))))
        hooks.setattr(transaction_recovery.base64, "b64encode", lambda *_args: (_ for _ in ()).throw(
            AssertionError("base64 before repeated-v7 admission")))
        hooks.setattr(transaction_recovery, "_canonical_journal_payload", lambda *_args: (_ for _ in ()).throw(
            AssertionError("canonical JSON before repeated-v7 admission")))
        tracemalloc.start()
        try:
            with pytest.raises(RepositoryError, match="live-byte budget exceeded"):
                rejected.register_surfaces([enrollment], live_budget=JournalLiveByteBudget(limit - 1, 0))
            _current, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
    # A retained 1 MiB image has a roughly 1.4 MiB base64 string.  The old
    # ``encoded[:-2]`` check cloned that string before the cap rejection.
    assert peak < len(retained) // 2
    assert rejected.path.read_bytes() == durable
