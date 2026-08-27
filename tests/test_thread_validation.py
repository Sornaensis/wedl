from __future__ import annotations

from copy import deepcopy

import pytest

from wedl import SOURCE_SCHEMA, THREAD_SOURCE_SCHEMA, V04_RECOVERY_CONTRACT
from wedl.errors import SupersededSchemaError
from wedl.ids import id_from_seed
from wedl.model import Record, World
from wedl.validation import validate_world


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"
THREAD_UNKNOWN = "thread_0123456789ABCDEFGHJKMNPQRV"


def _v05_world(ash_repo) -> World:
    original = ash_repo.load_world()
    records = {identifier: deepcopy(record) for identifier, record in original.records.items()}
    for record in records.values():
        record.frontmatter["schema"] = THREAD_SOURCE_SCHEMA
        if record.kind != "world":
            record.frontmatter.pop("threads", None)
    world = World(
        original.revision,
        original.tree_oid,
        records,
        original.root,
        original.source_root,
        original.is_worktree,
    )
    world.world_record.frontmatter["threads"] = []
    return world


def _thread_diagnostics(world: World) -> list[dict[str, object]]:
    return [item for item in validate_world(world) if item["code"].startswith("WDL-THREAD-")]


def test_direct_v04_model_validation_raises_the_stable_supersession_error(ash_repo) -> None:
    world = _v05_world(ash_repo)
    world.world_record.frontmatter["schema"] = "wedl/v0.4"

    with pytest.raises(SupersededSchemaError) as raised:
        validate_world(world)

    assert raised.value.code == "v04_superseded"
    assert str(raised.value) == "wedl/v0.4 is superseded; see docs/THREAD_SCHEMA_CONTRACT.md#4-quarantined-v04-recovery"
    assert raised.value.details["recoveryContract"] == V04_RECOVERY_CONTRACT


def test_schema_dispatch_is_homogeneous_and_reserved_members_do_not_emit_reference_noise(ash_repo) -> None:
    mixed = _v05_world(ash_repo)
    next(record for record in mixed.records.values() if record.kind == "character").frontmatter["schema"] = SOURCE_SCHEMA
    assert validate_world(mixed) == [{
        "code": "WDL-SRC-008",
        "message": "source records must use one homogeneous schema",
        "severity": "error",
        "entityId": mixed.world_record.id,
        "path": mixed.world_record.source_path,
        "field": "schema",
    }]

    v03 = ash_repo.load_world()
    v03_character = next(record for record in v03.records.values() if record.kind == "character")
    v03_character.frontmatter["threads"] = [THREAD_A]
    v03_codes = [item["code"] for item in validate_world(v03) if item["entityId"] == v03_character.id]
    assert v03_codes == ["WDL-SRC-009"]

    v05 = _v05_world(ash_repo)
    character = next(record for record in v05.records.values() if record.kind == "character")
    character.frontmatter["continuity"] = "char_0123456789ABCDEFGHJKMNPQRV"
    diagnostics = [item for item in validate_world(v05) if item["entityId"] == character.id]
    assert diagnostics == [{
        "code": "WDL-SRC-009",
        "message": "member is incompatible with this source schema",
        "severity": "error",
        "entityId": character.id,
        "path": character.source_path,
        "field": "continuity",
    }]


@pytest.mark.parametrize(
    ("mutate", "expected_code", "expected_field", "expected_message"),
    [
        (lambda world, record: world.world_record.frontmatter.__setitem__("threads", {}), "WDL-THREAD-001", "threads", "wedl/v0.5 world threads must be an array"),
        (lambda world, record: world.world_record.frontmatter.__setitem__("threads", [{"id": THREAD_A, "label": "A", "extra": True}]), "WDL-THREAD-002", "threads[0]", "thread declaration must be exactly an id and label mapping"),
        (lambda world, record: world.world_record.frontmatter.__setitem__("threads", [{"id": "thread_bad", "label": "A"}]), "WDL-THREAD-003", "threads[0].id", "thread id must use the thread_<26 Crockford> format"),
        (lambda world, record: world.world_record.frontmatter.__setitem__("threads", [{"id": THREAD_A, "label": "A"}, {"id": THREAD_A, "label": "B"}]), "WDL-THREAD-004", "threads[1].id", "thread declaration id is duplicated"),
        (lambda world, record: world.world_record.frontmatter.__setitem__("threads", [{"id": THREAD_A, "label": ""}]), "WDL-THREAD-005", "threads[0].label", "thread label must be a non-empty string"),
        (lambda world, record: world.world_record.frontmatter.__setitem__("threads", [{"id": THREAD_B, "label": "B"}, {"id": THREAD_A, "label": "A"}]), "WDL-THREAD-006", "threads", "thread declarations must be sorted by id"),
        (lambda world, record: record.frontmatter.__setitem__("threads", THREAD_A), "WDL-THREAD-007", "threads", "record thread membership must be an array of thread ids"),
        (lambda world, record: (world.world_record.frontmatter.__setitem__("threads", [{"id": THREAD_A, "label": "A"}, {"id": THREAD_B, "label": "B"}]), record.frontmatter.__setitem__("threads", [THREAD_B, THREAD_A, THREAD_A])), "WDL-THREAD-008", "threads", "record thread memberships must be unique and sorted by id"),
        (lambda world, record: (world.world_record.frontmatter.__setitem__("threads", [{"id": THREAD_A, "label": "A"}]), record.frontmatter.__setitem__("threads", [THREAD_UNKNOWN])), "WDL-THREAD-009", "threads[0]", "record thread membership is not declared by the world"),
    ],
)
def test_v05_thread_diagnostics_are_exact_and_table_driven(ash_repo, mutate, expected_code, expected_field, expected_message) -> None:
    world = _v05_world(ash_repo)
    record = next(item for item in world.records.values() if item.kind == "character")
    mutate(world, record)

    diagnostics = _thread_diagnostics(world)

    assert [item["code"] for item in diagnostics] == [expected_code]
    assert diagnostics[0]["field"] == expected_field
    assert diagnostics[0]["message"] == expected_message


def test_v05_thread_validation_is_declaration_then_source_path_membership_order(ash_repo) -> None:
    world = _v05_world(ash_repo)
    world.world_record.frontmatter["threads"] = [{"id": THREAD_B, "label": "B"}, {"id": THREAD_A, "label": "A"}]
    character = next(item for item in world.records.values() if item.kind == "character")
    character.frontmatter["threads"] = [THREAD_UNKNOWN]

    diagnostics = _thread_diagnostics(world)

    assert [(item["code"], item["field"]) for item in diagnostics] == [
        ("WDL-THREAD-006", "threads"),
        ("WDL-THREAD-009", "threads[0]"),
    ]


@pytest.mark.parametrize("membership", [[None], ["thread_bad"]])
def test_v05_malformed_membership_element_stops_before_order_or_declaration_cascades(ash_repo, membership) -> None:
    world = _v05_world(ash_repo)
    world.world_record.frontmatter["threads"] = [{"id": THREAD_A, "label": "A"}]
    character = next(item for item in world.records.values() if item.kind == "character")
    character.frontmatter["threads"] = membership

    diagnostics = _thread_diagnostics(world)

    assert diagnostics == [{
        "code": "WDL-THREAD-007",
        "message": "record thread membership must be an array of thread ids",
        "severity": "error",
        "entityId": character.id,
        "path": character.source_path,
        "field": "threads[0]",
    }]


def test_v05_one_global_timeline_and_hypothesis_membership_rules(ash_repo) -> None:
    world = _v05_world(ash_repo)
    world.world_record.frontmatter["timelines"].append({"id": "later", "label": "Later"})
    timeline_codes = [item["code"] for item in validate_world(world)]
    assert "WDL-TIMELINE-012" in timeline_codes
    timeline = next(item for item in validate_world(world) if item["code"] == "WDL-TIMELINE-012")
    assert timeline["field"] == "timelines"
    assert timeline["message"] == "wedl/v0.5 requires exactly one timeline declaration"

    world = _v05_world(ash_repo)
    hypothesis = Record({
        "schema": THREAD_SOURCE_SCHEMA,
        "kind": "hypothesis",
        "id": id_from_seed("hypothesis", "thread-membership"),
        "title": "Unresolved",
        "domain": "notes",
        "status": "open",
        "tags": [],
        "aliases": [],
        "threads": [THREAD_A],
    }, "", "story/hypotheses/unresolved.md", b"")
    world.records[hypothesis.id] = hypothesis
    codes = [item["code"] for item in validate_world(world) if item["entityId"] == hypothesis.id]
    assert "WDL-HYP-010" in codes
    assert not any(code.startswith("WDL-THREAD-") for code in codes)


@pytest.mark.parametrize(
    "member",
    [
        "default_continuity", "continuities", "continuity", "membership", "strands",
        "synchronization", "current_horizon", "retcon", "causal_handoffs", "presentation_frames",
    ],
)
def test_v05_withdrawn_continuity_members_are_source_errors_without_reference_noise(ash_repo, member) -> None:
    world = _v05_world(ash_repo)
    record = next(item for item in world.records.values() if item.kind == "character")
    record.frontmatter[member] = "char_0123456789ABCDEFGHJKMNPQRV"

    diagnostics = [item for item in validate_world(world) if item["entityId"] == record.id]

    assert [(item["code"], item["field"]) for item in diagnostics] == [("WDL-SRC-009", member)]


def test_v05_occurrence_domain_wrapper_is_rejected_without_changing_global_causality(ash_repo) -> None:
    world = _v05_world(ash_repo)
    world.world_record.frontmatter["threads"] = [{"id": THREAD_A, "label": "Archive"}]
    events = sorted(world.by_kind("event"), key=lambda record: (record.frontmatter["time"]["tick"], record.id))
    first, second = events[:2]
    first.frontmatter["threads"] = [THREAD_A]
    second.frontmatter["threads"] = [THREAD_A]
    second.frontmatter["causes"] = [first.id]
    assert not [item for item in validate_world(world) if item["code"].startswith("WDL-THREAD-")]

    second.frontmatter["time"] = {**second.frontmatter["time"], "domain": "withdrawn"}
    diagnostics = [item for item in validate_world(world) if item["entityId"] == second.id]
    assert [(item["code"], item["field"]) for item in diagnostics] == [("WDL-SRC-009", "time.domain")]
