"""Explicit eight-kind authoring through one confirmed changeset transaction."""
from __future__ import annotations

from copy import deepcopy
from contextlib import closing
import subprocess

import pytest

from wedl.authoring import apply_intent, preview_intent
from wedl.compiler import compile_world, connect
from wedl.errors import ChronologyUpgradeRequired, ConfirmationMismatch, ConfirmationRequired, ConflictError, NotFound, StaleRevision, UsageError
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


def test_starter_and_eight_kind_batch_share_one_confirmed_compiled_commit(ash_repo: Repository) -> None:
    repository, mapping = _small_authoring_repository(ash_repo)
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
    preview = preview_intent(repository, intent)
    assert preview["preview"]["valid"] is True, preview["preview"]["diagnostics"]
    assert len(preview["preview"]["files"]) == sum(item["action"] == "generational.create" for item in items)
    token = preview["preview"]["confirmationToken"]
    old = repository.head()
    with pytest.raises(ConfirmationRequired):
        apply_intent(repository, intent, allow_unconfirmed=True)
    assert repository.head() == old
    result = apply_intent(repository, intent, confirmation_token_value=token)
    assert result["newHead"] != old and not result["idempotentReplay"]
    with pytest.raises(ConfirmationMismatch):
        apply_intent(repository, intent, confirmation_token_value="changed-confirmation")
    assert repository.head() == result["newHead"]
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
    replay = apply_intent(repository, intent, confirmation_token_value=token)
    assert replay["idempotentReplay"] and replay["newHead"] == result["newHead"]
    assert _folded_current(repository) == byte_folded
    conflicting_items = [{**items[0], "title": "Different title"}, *items[1:]]
    with pytest.raises(ConflictError):
        apply_intent(repository, {**intent, "items": conflicting_items}, confirmation_token_value=token)
    with pytest.raises(StaleRevision):
        apply_intent(repository, {**intent, "idempotencyKey": "stale-transition-attempt"},
                     confirmation_token_value=token)
    assert repository.head() == result["newHead"]
    assert compile_world(repository, force=True)["status"] == "compiled"
    assert _folded_current(repository) == byte_folded
    # Negative paths use the same confirmed repository and must not change it.
    head = repository.head()
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
    union = committed_world.find("New compact", "union")
    bad_cause = {"action": "generational.append", "expectedHead": head,
                 "idempotencyKey": "late-cause", "kind": "union", "record": union.id,
                 "transition": "union-end", "payload": {}, "at": _point("-20", "0"),
                 "cause": "event_transfer"}
    with pytest.raises(UsageError):
        preview_intent(repository, bad_cause)
    assert repository.head() == head
    world_record = repository.load_world(head).world_record
    spatial_only = deepcopy(world_record.frontmatter)
    spatial_only["capabilities"] = ["spatial-core-v1"]
    repository.commit_files(expected_head=head,
                            files={"story/world.md": serialize_record(
                                spatial_only, f"# {world_record.title}\n")},
                            message="select spatial-only v0.7 capability")
    with pytest.raises(ChronologyUpgradeRequired):
        scaffold(repository)
    with pytest.raises(ChronologyUpgradeRequired):
        preview_intent(repository, {**base, "expectedHead": repository.head()})
