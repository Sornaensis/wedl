from __future__ import annotations

from copy import deepcopy
import json

import pytest
from fastapi.testclient import TestClient

from wedl import V04_SOURCE_SCHEMA
from wedl.changeset import ProtocolError, _apply, _canonical_frontmatter_patch, apply, preview
from wedl.cli import main
from wedl.errors import ValidationFailed
from wedl.model import Record
from wedl.server import create_app


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"


def _append_frontmatter(path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _v05(ash_repo):
    for path in (ash_repo.root / "story").rglob("*.md"):
        path.write_text(path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1), encoding="utf-8")
    _append_frontmatter(ash_repo.root / "story" / "world.md", "threads: []")
    ash_repo._git(["add", "story"])
    ash_repo._git(["commit", "-m", "upgrade fixture to v0.5 grouping"])
    world = ash_repo.load_world()
    return world, world.find("Mara Vale", "character")


def _payload(repository, operations, *, key: str = "thread-changeset-v1") -> dict:
    return {
        "protocol": "wedl-changeset/v1",
        "expectedHead": repository.head(),
        "idempotencyKey": key,
        "summary": "replace optional narrative thread grouping",
        "operations": operations,
    }


def _codes(plan: dict) -> set[str]:
    return {item["code"] for item in plan["diagnostics"]}


def test_thread_changeset_replaces_declarations_and_memberships_canonically_and_idempotently(ash_repo) -> None:
    world, character = _v05(ash_repo)
    before_body = character.body
    request = _payload(
        ash_repo,
        [
            {"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": [{"id": THREAD_A, "label": "Archive"}]}},
            {"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": [THREAD_A]}},
        ],
    )
    submitted = deepcopy(request)

    plan = preview(ash_repo, request)
    assert plan["valid"] is True
    assert request == submitted
    assert "threadIds" not in plan["diff"]
    assert "threads:" in plan["diff"]

    receipt = apply(ash_repo, request, confirmation_token_value=plan["confirmationToken"])
    assert receipt["status"] == "committed"
    assert apply(ash_repo, request, confirmation_token_value=plan["confirmationToken"])["idempotentReplay"] is True

    changed = ash_repo.load_world()
    assert changed.world_record.frontmatter["threads"] == [{"id": THREAD_A, "label": "Archive"}]
    changed_character = changed.get(character.id)
    assert changed_character.frontmatter["threads"] == [THREAD_A]
    assert "threadIds" not in changed_character.frontmatter
    assert changed_character.body == before_body
    assert "threadIds" not in (ash_repo.root / changed_character.source_path).read_text(encoding="utf-8")

    replacement = _payload(
        ash_repo,
        [
            {"type": "entity.update", "entity": changed.world_record.id, "frontmatterPatch": {"threads": [{"id": THREAD_A, "label": "Renamed archive"}, {"id": THREAD_B, "label": "Road"}]}},
            {"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": []}},
        ],
        key="thread-changeset-replace-v1",
    )
    replacement_plan = preview(ash_repo, replacement)
    assert replacement_plan["valid"] is True
    apply(ash_repo, replacement, confirmation_token_value=replacement_plan["confirmationToken"])
    replaced = ash_repo.load_world()
    assert replaced.world_record.frontmatter["threads"] == [
        {"id": THREAD_A, "label": "Renamed archive"},
        {"id": THREAD_B, "label": "Road"},
    ]
    assert replaced.get(character.id).frontmatter["threads"] == []


def test_thread_changeset_validator_owns_final_candidate_errors_and_is_atomic(ash_repo) -> None:
    world, character = _v05(ash_repo)
    used = _payload(
        ash_repo,
        [
            {"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": [{"id": THREAD_A, "label": "Archive"}]}},
            {"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": [THREAD_A]}},
        ],
    )
    plan = preview(ash_repo, used)
    apply(ash_repo, used, confirmation_token_value=plan["confirmationToken"])
    committed_head = ash_repo.head()

    removal = _payload(
        ash_repo,
        [{"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": []}}],
        key="thread-changeset-removal-v1",
    )
    removal_plan = preview(ash_repo, removal)
    assert "WDL-THREAD-009" in _codes(removal_plan)
    with pytest.raises(ValidationFailed):
        apply(ash_repo, removal, confirmation_token_value=removal_plan["confirmationToken"])
    assert ash_repo.head() == committed_head

    invalid_order = _payload(
        ash_repo,
        [{"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": [{"id": THREAD_B, "label": "Road"}, {"id": THREAD_A, "label": "Archive"}]}}],
        key="thread-changeset-order-v1",
    )
    assert "WDL-THREAD-006" in _codes(preview(ash_repo, invalid_order))

    undeclared = _payload(
        ash_repo,
        [{"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": [THREAD_B]}}],
        key="thread-changeset-undeclared-v1",
    )
    assert "WDL-THREAD-009" in _codes(preview(ash_repo, undeclared))

    non_list = _payload(
        ash_repo,
        [{"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": None}}],
        key="thread-changeset-membership-type-v1",
    )
    assert "WDL-THREAD-007" in _codes(preview(ash_repo, non_list))


@pytest.mark.parametrize(
    "record_kind,patch",
    [
        ("world", {"threadIds": []}),
        ("character", {"threads": []}),
        ("character", {"threads": [], "threadIds": []}),
        ("hypothesis", {"threads": []}),
        ("hypothesis", {"threadIds": []}),
    ],
)
def test_thread_changeset_enforces_target_specific_grouping_keys(record_kind: str, patch: dict) -> None:
    record = Record({"id": f"{record_kind}_test", "kind": record_kind}, "", "story/test.md", b"")
    with pytest.raises(ProtocolError):
        _canonical_frontmatter_patch(record, patch)


def test_thread_ids_are_literal_memberships_not_entity_or_temporary_references(ash_repo) -> None:
    world, character = _v05(ash_repo)
    records, _touched = _apply(
        {
            "operations": [
                {"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": [THREAD_A]}},
            ],
        },
        world,
        {THREAD_A: "character_replacement_must_not_be_used"},
    )
    assert records[character.id].frontmatter["threads"] == [THREAD_A]


def test_thread_changeset_cli_http_preview_parity_and_v03_v04_boundaries(ash_repo, tmp_path, capsys) -> None:
    world, character = _v05(ash_repo)
    request = _payload(
        ash_repo,
        [
            {"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": [{"id": THREAD_A, "label": "Archive"}]}},
            {"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": [THREAD_A]}},
        ],
    )
    request_path = tmp_path / "threads.json"
    request_path.write_text(json.dumps(request), encoding="utf-8")
    assert main(["--compact", "changeset", "preview", str(request_path), "--repo", str(ash_repo.root)]) == 0
    cli_plan = json.loads(capsys.readouterr().out)
    direct_plan = preview(ash_repo, request)
    assert cli_plan == {key: value for key, value in direct_plan.items() if key != "_changes"}

    with TestClient(create_app(ash_repo.root)) as client:
        headers = {"X-Wedl-Token": client.get("/api/session").json()["token"]}
        response = client.post("/api/changesets/preview", json=request, headers=headers)
    assert response.status_code == 200
    assert response.json() == cli_plan

    v03 = _payload(
        ash_repo,
        [{"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": []}}],
        key="thread-changeset-v03-v1",
    )
    # A v0.3 source cannot be promoted by a grouping changeset.
    for path in (ash_repo.root / "story").rglob("*.md"):
        path.write_text(path.read_text(encoding="utf-8").replace("schema: wedl/v0.5", "schema: wedl/v0.3", 1), encoding="utf-8")
    ash_repo._git(["add", "story"])
    ash_repo._git(["commit", "-m", "downgrade fixture to v0.3"])
    v03["expectedHead"] = ash_repo.head()
    assert "WDL-SRC-009" in _codes(preview(ash_repo, v03))

    world_path = ash_repo.root / "story" / "world.md"
    world_path.write_text(world_path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", f"schema: {V04_SOURCE_SCHEMA}", 1), encoding="utf-8")
    ash_repo._git(["add", "story"])
    ash_repo._git(["commit", "-m", "quarantine fixture as v0.4"])
    v03["expectedHead"] = ash_repo.head()
    with pytest.raises(Exception, match="wedl/v0.4 is superseded"):
        preview(ash_repo, v03)
