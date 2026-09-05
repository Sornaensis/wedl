from __future__ import annotations

from copy import deepcopy

import pytest

from wedl.generational import Applicability, Transition, generational_record, literal_payload_ok
from wedl.source import generated_path


def _organization() -> dict[str, object]:
    return {
        "schema": "wedl/v0.7", "kind": "organization", "id": "organization_00000000000000000000000001", "title": "House", "domain": "history.organizations", "status": "canonical", "tags": [], "aliases": [], "threads": [], "audience": ["public"], "perspectives": ["ordinary"], "organization_kind": "house",
        "initialization": {"transition_id": "transition_00000000000000000000000001", "transition_kind": "organization-initialize", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}}, "payload": {"title": "House", "aliases": ["H"]}}, "transitions": [],
    }


def test_literal_accessor_is_immutable_and_rejects_extra_leaf_fields() -> None:
    data = _organization()
    record = generational_record(data)
    assert record.transitions == () and record.initialization.payload["aliases"] == ("H",)
    with pytest.raises(TypeError): record.initialization.payload["title"] = "Other"  # type: ignore[index]
    with pytest.raises(ValueError): Applicability.from_value({"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}, "extra": True})
    data["initialization"]["payload"] = {"title": "House", "aliases": [], "extra": True}
    with pytest.raises(ValueError): generational_record(data)
    data["initialization"]["payload"] = {"title": "House", "aliases": []}
    data["initialization"]["transition_kind"] = "union-initialize"
    with pytest.raises(ValueError): generational_record(data)


def test_generational_generated_paths_are_canonical() -> None:
    identifier = "union_00000000000000000000000001"
    assert generated_path("story", "union", "Ignored", identifier, {"schema": "wedl/v0.7"}) == f"story/unions/{identifier}.md"


@pytest.mark.parametrize(
    ("kind", "payload"),
    [
        ("organization-initialize", {"title": "House", "aliases": ["H", {"bad": "leaf"}]}),
        ("organization-initialize", {"title": "House", "aliases": ["H", 1]}),
        ("parentage-initialize", {"basis": {"bad": "enum"}}),
        ([], {}),
    ],
)
def test_literal_payload_predicate_is_total_for_malformed_yaml_leaves(kind: object, payload: object) -> None:
    assert literal_payload_ok(kind, payload) is False  # type: ignore[arg-type]


@pytest.mark.parametrize(
    ("field", "bad_value"),
    [
        ("title", " "),
        ("domain", "history"),
        ("status", {"not": "a status"}),
        ("organization_kind", ["house"]),
        ("threads", ["not-a-thread"]),
        ("audience", ["public", {"bad": "leaf"}]),
        ("aliases", ["House", ["bad", "leaf"]]),
    ],
)
def test_record_envelope_and_list_leaves_reject_malformed_values_as_value_error(field: str, bad_value: object) -> None:
    data = _organization()
    data[field] = bad_value
    with pytest.raises(ValueError):
        generational_record(data)


def test_transition_rejects_malformed_record_discriminator_as_value_error() -> None:
    transition = _organization()["initialization"]
    with pytest.raises(ValueError):
        Transition.from_value(transition, initialization=True, record_kind=[])


def test_tenure_transfer_requires_a_cause_event() -> None:
    transition = {
        "transition_id": "transition_00000000000000000000000002",
        "transition_kind": "tenure-transfer",
        "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 1, "order": 0}},
        "payload": {
            "from_tenure_id": "tenure_00000000000000000000000001",
            "to_tenure_id": "tenure_00000000000000000000000002",
        },
    }
    with pytest.raises(ValueError, match="requires a cause event"):
        Transition.from_value(transition, record_kind="tenure")
    transition["cause_event_id"] = "not-an-event"
    with pytest.raises(ValueError, match="invalid cause event"):
        Transition.from_value(transition, record_kind="tenure")
    transition["cause_event_id"] = "event_00000000000000000000000001"
    accepted = Transition.from_value(transition, record_kind="tenure")
    assert accepted.cause_event_id == "event_00000000000000000000000001"
