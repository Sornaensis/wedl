"""Explicit eight-kind authoring through one confirmed changeset transaction."""
from __future__ import annotations

from copy import deepcopy
from contextlib import closing
import subprocess

import pytest

from wedl.authoring import apply_intent, compile_intent, preview_intent
from wedl.compiler import compile_world, connect
from wedl.errors import ChronologyUpgradeRequired, ConfirmationMismatch, ConfirmationRequired, ConflictError, NotFound, ParseError, StaleRevision, UsageError
from wedl.generational_authoring import scaffold, schema
from wedl.ids import id_from_seed
from wedl.repository import Repository
from wedl.source import serialize_record

from test_generational_api import request
from test_generational_compiler import _world
from wedl.generational_api import execute


def _point(tick: str = "-2", order: str = "0") -> dict[str, str]:
    return {"timeline": "main", "tick": tick, "order": order}


def _create(kind: str, title: str, fields: dict, payload: dict, *,
            tick: str = "-2", order: str = "0", identifier: str | None = None) -> dict:
    result = {"action": "generational.create", "kind": kind, "title": title,
              "audience": ["public"], "perspectives": ["ordinary"],
              "fields": fields, "payload": payload, "at": _point(tick, order)}
    if identifier:
        result["id"] = identifier
    return result


def _folded_current(repository: Repository) -> list[tuple]:
    with closing(connect(repository.root / ".wedl" / "world.sqlite", True)) as connection:
        return [tuple(row) for row in connection.execute(
            "SELECT record_id,timeline,at_tick,at_order,state,value_json "
            "FROM generational_current ORDER BY record_id"
        )]


def _small_authoring_repository(repository: Repository) -> tuple[Repository, dict[str, str]]:
    world, mapping = _world()
    names = ["character_no_lineage", "character_alpha", "character_beta",
             "event_compact", "event_appointment", "event_transfer"]
    keep = [world.world_record, *(world.get(mapping[name]) for name in names)]
    files = {path.relative_to(repository.root).as_posix(): None
             for path in (repository.root / "story").rglob("*.md")}
    files.update({("story/world.md" if record.kind == "world" else record.source_path):
                  serialize_record(record.frontmatter, f"# {record.title}\n")
                  for record in keep})
    repository.commit_files(expected_head=repository.head(), files=files,
                            message="seed bounded generational authoring")
    return repository, mapping


def _authoring_bytes(repository: Repository) -> dict[str, bytes]:
    paths = [repository.root / ".git" / name for name in ("index", "index.lock")]
    paths.extend((repository.root / "story").rglob("*.md"))
    paths.extend(path for path in (repository.root / ".wedl").rglob("*") if path.is_file())
    return {path.relative_to(repository.root).as_posix(): path.read_bytes()
            for path in paths if path.is_file()}


def _assert_generational_dispatch(repository: Repository) -> None:
    """Exercise real dispatch and fresh-HEAD refusal without another Git fixture."""
    expected, changed = "a" * 40, "b" * 40
    world, _ = _world()
    intent = {**_create("organization", "Dispatch house", {"organization_kind": "house"},
                        {"title": "Dispatch house", "aliases": []}),
              "expectedHead": expected, "idempotencyKey": "dispatch-house"}
    before = _authoring_bytes(repository)

    class ReadOnlyRepository:
        def __init__(self, *, source_error=None, change_head=False):
            self.source_error = source_error
            self.change_head = change_head
            self.head_calls = 0
            self.loads = []

        def head(self):
            self.head_calls += 1
            return changed if self.change_head and self.head_calls > 1 else expected

        def load_world(self, revision="HEAD", *, cache_write=True):
            self.loads.append((revision, cache_write))
            if self.source_error is not None:
                raise self.source_error
            return world

        def __getattr__(self, name):
            raise AssertionError(f"unexpected repository operation: {name}")

    reader = ReadOnlyRepository()
    payload = compile_intent(reader, intent)
    assert payload["expectedHead"] == expected and len(payload["operations"]) == 1
    assert reader.loads == [(expected, False)] and reader.head_calls == 1

    # Metadata preconditions precede source parsing; valid requests still
    # propagate exactly the error raised by the read-only expected-world load.
    source_error = ParseError("malformed source sentinel")
    invalid_metadata = [
        ({"expectedHead": "HEAD"}, UsageError),
        ({"expectedHead": None}, UsageError),
        ({"expectedHead": "0" * 40}, StaleRevision),
        ({"idempotencyKey": ""}, UsageError),
        ({"idempotencyKey": "x" * 257}, UsageError),
        ({"summary": ""}, UsageError),
        ({"summary": "x" * 257}, UsageError),
        ({"kind": ["organization"]}, UsageError),
    ]
    for patch, error_type in invalid_metadata:
        reader = ReadOnlyRepository(source_error=source_error)
        with pytest.raises(error_type):
            compile_intent(reader, {**intent, **patch})
        assert reader.loads == []
    for patch in ({}, {"kind": "unsupported-kind"}):
        reader = ReadOnlyRepository(source_error=source_error)
        with pytest.raises(ParseError) as refusal:
            compile_intent(reader, {**intent, **patch})
        assert refusal.value is source_error and reader.loads == [(expected, False)]
    reader = ReadOnlyRepository()
    with pytest.raises(UsageError, match="unsupported generational kind"):
        compile_intent(reader, {**intent, "kind": "unsupported-kind"})
    assert reader.loads == [(expected, False)]

    capabilities = world.world_record.frontmatter["capabilities"]
    world.world_record.frontmatter["capabilities"] = ["spatial-core-v1"]
    try:
        reader = ReadOnlyRepository()
        with pytest.raises(ChronologyUpgradeRequired):
            compile_intent(reader, intent)
        assert reader.loads == [(expected, False)]
    finally:
        world.world_record.frontmatter["capabilities"] = capabilities

    reader = ReadOnlyRepository(change_head=True)
    with pytest.raises(StaleRevision, match="changeset expected a different HEAD") as refusal:
        preview_intent(reader, intent)
    assert refusal.value.details == {"expected": expected, "actual": changed}
    assert reader.head_calls == 2 and reader.loads == [(expected, False)]
    assert _authoring_bytes(repository) == before


def test_starter_and_eight_kind_batch_share_one_confirmed_compiled_commit(ash_repo: Repository) -> None:
    repository, mapping = _small_authoring_repository(ash_repo)
    _assert_generational_dispatch(repository)
    starter = scaffold(repository)
    assert starter["expectedHead"] == repository.head()
    assert set(schema()["kinds"]) == {"organization", "parentage", "union", "affiliation",
                                      "legacy", "tenure", "claim", "vital-history"}
    starter_item = {key: value for key, value in starter.items()
                    if key not in {"expectedHead", "idempotencyKey", "summary"}}
    organization_id = id_from_seed("organization", "author-new-house")
    starter_item["id"] = organization_id
    legacy_id = id_from_seed("legacy", "author-new-office")
    first_claim = id_from_seed("claim", "author-first-claim")
    second_claim = id_from_seed("claim", "author-second-claim")
    adoptive_id = id_from_seed("parentage", "author-adoptive-tie")
    holder_id = id_from_seed("tenure", "author-source-hold")
    successor_id = id_from_seed("tenure", "author-transfer-target")
    confirmation_id = id_from_seed("generational-transition", "author-adoption-confirm")
    child = mapping["character_no_lineage"]
    alpha, beta = mapping["character_alpha"], mapping["character_beta"]
    items = [starter_item,
        _create("parentage", "New adoptive tie", {"child_id": child, "parent_id": alpha},
                {"basis": "adoptive"}, identifier=adoptive_id),
        _create("parentage", "New biological tie", {"child_id": child, "parent_id": beta},
                {"basis": "biological"}),
        _create("union", "New compact", {"participant_ids": sorted([alpha, child])},
                {"participant_ids": sorted([alpha, child])}),
        _create("affiliation", "New house role", {"character_id": child,
                "organization_id": organization_id}, {"role": "scribe"}),
        _create("legacy", "New office", {"legacy_kind": "office",
                "organization_id": organization_id},
                {"title": "New office", "aliases": []}, identifier=legacy_id),
        _create("tenure", "New legal holder", {"legacy_id": legacy_id},
                {"holder_id": alpha, "basis": "legal"}, identifier=holder_id),
        _create("tenure", "New vacant successor", {"legacy_id": legacy_id,
                "predecessor_tenure_id": holder_id},
                {"holder_id": None, "basis": "legal"}, tick="1", identifier=successor_id),
        _create("claim", "First claim", {"legacy_id": legacy_id, "claimant_id": alpha},
                {"competes_with": [second_claim]}, tick="-1", identifier=first_claim),
        _create("claim", "Second claim", {"legacy_id": legacy_id, "claimant_id": beta},
                {"competes_with": [first_claim]}, tick="-1", identifier=second_claim),
        _create("vital-history", "New vital history", {"character_id": child,
                "disclosure": "known"}, {}),
        {"action": "generational.append", "kind": "parentage", "record": adoptive_id,
         "transition": "parentage-confirm", "payload": {"basis": "adoptive"},
         "at": _point("-1"), "cause": mapping["event_compact"],
         "transitionId": confirmation_id},
        {"action": "generational.correct", "kind": "parentage", "record": adoptive_id,
         "transition": "parentage-confirm", "payload": {"basis": "adoptive"},
         "at": _point("-1"), "cause": mapping["event_compact"],
         "replaces": confirmation_id},
        {"action": "generational.append", "kind": "claim", "record": first_claim,
         "transition": "claim-dispute", "payload": {"competes_with": [second_claim]},
         "at": _point("0"), "cause": mapping["event_appointment"]},
        {"action": "generational.append", "kind": "claim", "record": second_claim,
         "transition": "claim-dispute", "payload": {"competes_with": [first_claim]},
         "at": _point("0"), "cause": mapping["event_appointment"]},
        {"action": "generational.append", "kind": "tenure", "record": holder_id,
         "transition": "tenure-transfer",
         "payload": {"from_tenure_id": holder_id, "to_tenure_id": successor_id},
         "at": _point("1", "1"), "cause": mapping["event_transfer"]},
    ]
    intent = {"action": "generational.batch", "expectedHead": repository.head(),
              "idempotencyKey": "author-eight-kind-batch", "items": items}
    before_preview = _authoring_bytes(repository)
    preview = preview_intent(repository, intent)
    assert _authoring_bytes(repository) == before_preview
    assert preview["preview"]["valid"] is True, preview["preview"]["diagnostics"]
    assert len(preview["preview"]["files"]) == sum(item["action"] == "generational.create" for item in items)
    token = preview["preview"]["confirmationToken"]
    old = repository.head()
    with pytest.raises(ConfirmationRequired):
        apply_intent(repository, intent, allow_unconfirmed=True)
    assert repository.head() == old
    assert _authoring_bytes(repository) == before_preview
    result = apply_intent(repository, intent, confirmation_token_value=token)
    assert result["newHead"] != old and not result["idempotentReplay"]
    committed_bytes = _authoring_bytes(repository)
    with pytest.raises(ConfirmationMismatch):
        apply_intent(repository, intent, confirmation_token_value="changed-confirmation")
    assert repository.head() == result["newHead"]
    assert _authoring_bytes(repository) == committed_bytes
    count = subprocess.run(["git", "-C", str(repository.root), "rev-list", "--count",
                            f"{old}..{result['newHead']}"], check=True, capture_output=True, text=True)
    assert count.stdout.strip() == "1"
    assert result["compile"]["status"] == "compiled"
    committed_world = repository.load_world(repository.head())
    assert committed_world.find("New organization", "organization").id == organization_id
    byte_folded = _folded_current(repository)
    assert len(byte_folded) >= sum(item["action"] == "generational.create" and
                                   int(item["at"]["tick"]) <= 0 for item in items)
    assert {organization_id, legacy_id, first_claim, second_claim, adoptive_id,
            holder_id} <= {row[0] for row in byte_folded}
    assert any("New organization" in row[-1] for row in byte_folded)
    later = execute(repository, "legacy", request(repository, "legacy", subject=legacy_id,
                                                   at=_point("1", "1")))
    assert later["state"] == "available"
    assert len(later["claims"]) == 2
    assert all(row["state"] == "disputed" for row in later["claims"])
    assert all(row["recordId"] != holder_id for row in later["holders"])
    assert not any(row["recordId"] == successor_id for row in later["holders"])
    adoptive = committed_world.get(adoptive_id)
    assert [row["transition_kind"] for row in adoptive.frontmatter["transitions"]] == [
        "parentage-confirm", "parentage-confirm"]
    assert adoptive.frontmatter["transitions"][1]["replaces_transition_id"] == confirmation_id
    before_replay = _authoring_bytes(repository)
    replay = apply_intent(repository, intent, confirmation_token_value=token)
    assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
    assert _folded_current(repository) == byte_folded
    assert _authoring_bytes(repository) == before_replay
    conflicting_items = [{**items[0], "title": "Different title"}, *items[1:]]
    with pytest.raises(ConflictError):
        apply_intent(repository, {**intent, "items": conflicting_items}, confirmation_token_value=token)
    with pytest.raises(StaleRevision):
        apply_intent(repository, {**intent, "idempotencyKey": "stale-transition-attempt"},
                     confirmation_token_value=token)
    assert repository.head() == result["newHead"]
    assert _authoring_bytes(repository) == before_replay
    assert compile_world(repository, force=True)["status"] == "compiled"
    assert _folded_current(repository) == byte_folded
    # Negative paths use the same confirmed repository and must not change it.
    head = repository.head()
    before_refusal = _authoring_bytes(repository)
    base = {"action": "generational.create", "expectedHead": head,
            "idempotencyKey": "reject-bad-authoring", "kind": "parentage",
            "title": "Invalid tie", "audience": ["public"],
            "perspectives": ["ordinary"], "fields": {"child_id": mapping["character_no_lineage"],
            "parent_id": "missing parent"}, "payload": {"basis": "adoptive"},
            "at": _point()}
    with pytest.raises(NotFound):
        preview_intent(repository, base)
    with pytest.raises(UsageError):
        preview_intent(repository, {**base, "kind": ["parentage"]})
    with pytest.raises(UsageError):
        preview_intent(repository, {**base, "fields": {**base["fields"],
                                              "parent_id": mapping["character_alpha"]},
                                     "payload": {"basis": ["adoptive"]}})
    with pytest.raises(UsageError):
        preview_intent(repository, {**base, "fields": {**base["fields"],
                                              "parent_id": mapping["character_alpha"]},
                                     "at": {"timeline": "main", "tick": -2, "order": "0"}})
    with pytest.raises(StaleRevision):
        preview_intent(repository, {**base, "expectedHead": "0" * 40})
    assert repository.head() == head
    assert _authoring_bytes(repository) == before_refusal
    union = committed_world.find("New compact", "union")
    bad_cause = {"action": "generational.append", "expectedHead": head,
                 "idempotencyKey": "late-cause", "kind": "union", "record": union.id,
                 "transition": "union-end", "payload": {}, "at": _point("-20", "0"),
                 "cause": "event_transfer"}
    with pytest.raises(UsageError):
        preview_intent(repository, bad_cause)
    assert repository.head() == head
    assert _authoring_bytes(repository) == before_refusal
    world_record = repository.load_world(head).world_record
    spatial_only = deepcopy(world_record.frontmatter)
    spatial_only["capabilities"] = ["spatial-core-v1"]
    repository.commit_files(expected_head=head,
                            files={"story/world.md": serialize_record(
                                spatial_only, f"# {world_record.title}\n")},
                            message="select spatial-only v0.7 capability")
    before_capability_refusal = _authoring_bytes(repository)
    with pytest.raises(ChronologyUpgradeRequired):
        scaffold(repository)
    with pytest.raises(ChronologyUpgradeRequired):
        preview_intent(repository, {**base, "expectedHead": repository.head()})
    assert _authoring_bytes(repository) == before_capability_refusal
