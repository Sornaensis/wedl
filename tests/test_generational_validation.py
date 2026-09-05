from __future__ import annotations

from pathlib import Path
from copy import deepcopy

from wedl.generational_validation import _STATE_RULES, _initial_state, _payload_leaf, validate_generational_world
from wedl.ids import id_from_seed
from wedl.model import Record, StoryTime, World
from wedl.source import KIND_DIR, serialize_record
from wedl.validation import validate_world


def _record(data: dict) -> Record:
    directory = KIND_DIR.get(data["kind"])
    path = f"story/{directory}/{data['id']}.md" if directory else f"story/{data['id']}.md"
    return Record(data, "", path, b"")


def test_generational_validation_accepts_literal_union_and_rejects_bad_payload() -> None:
    world = _record({"schema": "wedl/v0.7", "kind": "world", "id": "world_00000000000000000000000001", "title": "World", "capabilities": ["generational-core-v1"], "domain": "world", "status": "canonical", "tags": [], "aliases": [], "threads": [], "timelines": [{"id": "main", "label": "Main"}], "default_timeline": "main", "chronology": {"calendars": [], "eras": [], "anchors": []}})
    alpha = _record({"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000001", "title": "Alpha", "domain": "people", "status": "canonical", "tags": [], "aliases": []})
    beta = _record({"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000002", "title": "Beta", "domain": "people", "status": "canonical", "tags": [], "aliases": []})
    union_data = {"schema": "wedl/v0.7", "kind": "union", "id": "union_00000000000000000000000001", "title": "Compact", "domain": "history.unions", "status": "canonical", "tags": [], "aliases": [], "threads": [], "audience": ["public"], "perspectives": ["ordinary"], "participant_ids": [alpha.id, beta.id], "initialization": {"transition_id": "transition_00000000000000000000000001", "transition_kind": "union-initialize", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}}, "payload": {"participant_ids": [alpha.id, beta.id]}}, "transitions": []}
    union = _record(union_data)
    candidate = World("r", "t", {item.id: item for item in (world, alpha, beta, union)}, Path("."))
    assert validate_generational_world(candidate) == []
    union_data["initialization"]["payload"]["extra"] = True
    assert any(item["code"] == "GEN-PAYLOAD-001" for item in validate_generational_world(candidate))
    union_data["initialization"]["transition_kind"] = []
    assert any(item["code"] == "GEN-TRANSITION-003" for item in validate_world(candidate))


def test_transition_replacement_cause_and_malformed_leaves_are_total() -> None:
    world = _record({"schema": "wedl/v0.7", "kind": "world", "id": "world_00000000000000000000000001", "title": "World", "capabilities": ["generational-core-v1"], "domain": "world", "status": "canonical", "tags": [], "aliases": [], "threads": [], "timelines": [{"id": "main", "label": "Main"}], "default_timeline": "main", "chronology": {"calendars": [], "eras": [], "anchors": []}})
    alpha = _record({"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000001", "title": "Alpha", "domain": "people", "status": "canonical", "tags": [], "aliases": []})
    beta = _record({"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000002", "title": "Beta", "domain": "people", "status": "canonical", "tags": [], "aliases": []})
    data = {"schema": "wedl/v0.7", "kind": "union", "id": "union_00000000000000000000000001", "title": "Compact", "domain": "history.unions", "status": "canonical", "tags": [], "aliases": [], "threads": [], "audience": ["public"], "perspectives": ["ordinary"], "participant_ids": [alpha.id, beta.id], "initialization": {"transition_id": "transition_00000000000000000000000001", "transition_kind": "union-initialize", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}}, "payload": {"participant_ids": [alpha.id, beta.id]}}, "transitions": []}
    union = _record(data); candidate = World("r", "t", {item.id: item for item in (world, alpha, beta, union)}, Path("."))
    data["transitions"] = [{"transition_id": "transition_00000000000000000000000002", "transition_kind": "union-form", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}}, "payload": {"participant_ids": [alpha.id, beta.id]}, "cause_event_id": None}]
    diagnostics = validate_generational_world(candidate)
    assert {item["code"] for item in diagnostics} >= {"GEN-REPLACEMENT-002", "GEN-CAUSE-001"}
    assert any(item["field"] == "transitions[0].cause_event_id" for item in diagnostics)
    data["transitions"] = [{"transition_id": "transition_00000000000000000000000002", "transition_kind": "union-form", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 1, "order": 0}}, "payload": {"participant_ids": [alpha.id, beta.id]}, "replaces_transition_id": "transition_00000000000000000000000002"}]
    assert any(item["code"] == "GEN-REPLACEMENT-001" for item in validate_generational_world(candidate))
    data["initialization"] = []
    diagnostics = validate_generational_world(candidate)
    assert any(item["code"] == "GEN-TRANSITION-001" and item["field"] == "initialization" for item in diagnostics)
    world.frontmatter["capabilities"] = []
    capabilities = [item for item in validate_world(candidate) if item["code"] == "GEN-CAPABILITY-001" and item["entityId"] == world.id and item["field"] == "capabilities"]
    assert len(capabilities) == 1


def test_closed_state_tables_cover_every_generational_kind_and_vital_sequence() -> None:
    assert set(_STATE_RULES) == {"organization", "parentage", "union", "affiliation", "legacy", "tenure", "claim", "vital-history"}
    assert _STATE_RULES["organization"]["organization-dormant"] == (frozenset({"active"}), "dormant")
    assert _STATE_RULES["parentage"]["parentage-confirm"] == (frozenset({"asserted"}), "confirmed")
    assert _STATE_RULES["union"]["union-reconcile"] == (frozenset({"formed"}), "formed")
    assert _STATE_RULES["affiliation"]["affiliation-end"] == (frozenset({"active"}), "ended")
    assert _STATE_RULES["legacy"]["legacy-dissolve"] == (frozenset({"active", "dormant"}), "dissolved")
    assert _STATE_RULES["tenure"]["tenure-transfer"] == (frozenset({"holding"}), "vacant")
    assert _STATE_RULES["claim"]["claim-recognize"] == (frozenset({"proposed", "disputed"}), "recognized")
    assert _initial_state("vital-history", {}) == "unknown"
    assert _STATE_RULES["vital-history"]["vital-birth"] == (frozenset({"unknown"}), "living")
    assert _STATE_RULES["vital-history"]["vital-death"] == (frozenset({"living"}), "dead")
    assert _STATE_RULES["vital-history"]["vital-existence-start"] == (frozenset({"unknown"}), "existing")
    assert _STATE_RULES["vital-history"]["vital-existence-end"] == (frozenset({"existing"}), "ended")


def test_validation_is_total_and_reports_exact_transition_leaves_and_paths() -> None:
    world = _record({"schema": "wedl/v0.7", "kind": "world", "id": "world_00000000000000000000000001", "title": "World", "capabilities": ["generational-core-v1"], "domain": "world", "status": "canonical", "tags": [], "aliases": [], "threads": [], "timelines": [{"id": "main", "label": "Main"}], "default_timeline": "main", "chronology": {"calendars": [], "eras": [], "anchors": []}})
    alpha = _record({"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000001", "title": "Alpha", "domain": "people", "status": "canonical", "tags": [], "aliases": []})
    beta = _record({"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000002", "title": "Beta", "domain": "people", "status": "canonical", "tags": [], "aliases": []})
    data = {"schema": "wedl/not-v07", "kind": "union", "id": "not-an-id", "title": "Compact", "domain": "history.unions", "status": "canonical", "tags": [], "aliases": [], "threads": [], "audience": ["public"], "perspectives": ["ordinary"], "participant_ids": [alpha.id, beta.id], "initialization": {"transition_id": "transition_00000000000000000000000001", "transition_kind": "union-initialize", "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}}, "payload": {"participant_ids": [alpha.id, beta.id]}}, "transitions": [{"transition_id": "transition_00000000000000000000000002", "transition_kind": "union-form", "applicability": {"applicability_kind": "instant", "point": []}, "payload": {"participant_ids": [["unhashable"]]}}]}
    union = _record(data); union.source_path = "story/wrong.md"
    candidate = World("r", "t", {item.id: item for item in (world, alpha, beta, union)}, Path("."))
    diagnostics = validate_generational_world(candidate)
    fields = {item["field"] for item in diagnostics}
    assert {"schema", "id", "source_path", "transitions[0].applicability.point", "transitions[0].payload.participant_ids"} <= fields
    data["transitions"] = {"truthy": "not a list"}
    assert isinstance(validate_generational_world(candidate), list)
    data["status"] = []
    assert any(item["field"] == "status" for item in validate_generational_world(candidate))
    assert _payload_leaf("organization-initialize", {"title": [], "aliases": []}) == "payload.title"
    assert _payload_leaf("organization-reparent", {"parent_id": []}) == "payload.parent_id"
    assert _payload_leaf("affiliation-role", {"role": None}) == "payload.role"
    assert _payload_leaf("tenure-hold", {"holder_id": None, "basis": "legal"}) == "payload.holder_id"
    assert _payload_leaf("tenure-transfer", {"from_tenure_id": None, "to_tenure_id": None}) == "payload.from_tenure_id"


def test_all_eight_runtime_kinds_accept_and_cross_record_integrity_rejects() -> None:
    def ident(kind: str, seed: str) -> str: return id_from_seed(kind, seed)
    def point(tick: int, timeline: str = "main") -> dict: return {"timeline": timeline, "tick": tick, "order": 0}
    def transition(kind: str, seed: str, tick: int, payload: dict, **extra: object) -> dict:
        return {"transition_id": ident("generational-transition", seed), "transition_kind": kind, "applicability": {"applicability_kind": "instant", "point": point(tick)}, "payload": payload, **extra}
    world = _record({"schema": "wedl/v0.7", "kind": "world", "id": ident("world", "world"), "title": "World", "capabilities": ["generational-core-v1"], "domain": "world.core", "status": "canonical", "tags": [], "aliases": [], "threads": [], "timelines": [{"id": "main", "label": "Main"}, {"id": "other", "label": "Other"}], "default_timeline": "main", "chronology": {"calendars": [], "eras": [], "anchors": []}})
    chars = [_record({"schema": "wedl/v0.7", "kind": "character", "id": ident("character", seed), "title": seed, "domain": "people.core", "status": "canonical", "tags": [], "aliases": []}) for seed in ("alpha", "beta", "gamma", "child")]
    alpha, beta, gamma, child = chars
    location = _record({"schema": "wedl/v0.7", "kind": "location", "id": ident("location", "hall"), "title": "Hall", "domain": "places.core", "status": "canonical", "tags": [], "aliases": []})
    event = _record({"schema": "wedl/v0.7", "kind": "event", "id": ident("event", "transfer"), "title": "Transfer", "domain": "events.core", "status": "canonical", "tags": [], "aliases": [], "time": point(4)})
    def base(kind: str, seed: str, domain: str, specific: dict, init_payload: dict, tick: int = 0) -> dict:
        return {"schema": "wedl/v0.7", "kind": kind, "id": ident(kind, seed), "title": seed, "domain": domain, "status": "canonical", "tags": [], "aliases": [], "threads": [], "audience": ["public"], "perspectives": ["ordinary"], **specific, "initialization": transition(f"{kind.replace('vital-history', 'vital')}-initialize", f"{seed}-init", tick, init_payload), "transitions": []}
    org_data = base("organization", "org", "history.organizations", {"organization_kind": "house", "location_id": location.id}, {"title": "org", "aliases": []}, -2)
    org = _record(org_data)
    parent = _record(base("parentage", "parent", "history.kinship", {"child_id": child.id, "parent_id": alpha.id}, {"basis": "biological"}, -1))
    participants = sorted([alpha.id, beta.id, gamma.id])
    union = _record(base("union", "union", "history.unions", {"participant_ids": participants}, {"participant_ids": participants}, -1))
    affiliation = _record(base("affiliation", "aff", "history.affiliations", {"character_id": child.id, "organization_id": org.id}, {"role": None}, 0))
    legacy = _record(base("legacy", "legacy", "history.legacies", {"legacy_kind": "office", "organization_id": org.id}, {"title": "legacy", "aliases": []}, -2))
    target_data = base("tenure", "target", "history.tenures", {"legacy_id": legacy.id}, {"holder_id": None, "basis": "legal"}, 3)
    target = _record(target_data)
    tenure_data = base("tenure", "source", "history.tenures", {"legacy_id": legacy.id}, {"holder_id": alpha.id, "basis": "legal"}, 0)
    tenure_data["transitions"] = [transition("tenure-transfer", "source-transfer", 5, {"from_tenure_id": tenure_data["id"], "to_tenure_id": target.id}, cause_event_id=event.id)]
    tenure = _record(tenure_data)
    claim_a_data = base("claim", "claim-a", "history.claims", {"legacy_id": legacy.id, "claimant_id": alpha.id}, {"competes_with": [ident("claim", "claim-b")]}, 0)
    claim_b_data = base("claim", "claim-b", "history.claims", {"legacy_id": legacy.id, "claimant_id": beta.id}, {"competes_with": [claim_a_data["id"]]}, 0)
    claim_a, claim_b = _record(claim_a_data), _record(claim_b_data)
    vital_data = base("vital-history", "vital", "history.vitals", {"character_id": child.id, "disclosure": "known"}, {}, -2)
    vital_data["transitions"] = [transition("vital-birth", "birth", -1, {}), transition("vital-death", "death", 6, {})]
    vital = _record(vital_data)
    records = [world, *chars, location, event, org, parent, union, affiliation, legacy, target, tenure, claim_a, claim_b, vital]
    candidate = World("r", "t", {record.id: record for record in records}, Path("."))
    assert validate_generational_world(candidate) == []
    # A terminal organization correction folds from the target's pre-terminal state.
    org_data["transitions"] = [transition("organization-dormant", "org-dormant", 1, {}), transition("organization-dissolve", "org-dissolve", 2, {}), transition("organization-rename", "org-fix", 2, {"title": "fixed", "aliases": []}, replaces_transition_id=ident("generational-transition", "org-dissolve"))]
    assert not {item["code"] for item in validate_generational_world(candidate)} & {"GEN-STATE-001", "GEN-TRANSITION-005"}
    org_data["transitions"] = []
    # Vacancy is finite, and cross-timeline tenure links cannot order lexically.
    from wedl.generational_validation import _tenure_state_before
    tenure_data["transitions"] = [{"transition_id": ident("generational-transition", "vacate"), "transition_kind": "tenure-vacate", "applicability": {"applicability_kind": "inclusive-interval", "first": point(1), "last": point(2)}, "payload": {"holder_id": None}}]
    assert _tenure_state_before(tenure, StoryTime("main", 2, 1), candidate.timeline_ids) == "expired"
    target_data["predecessor_tenure_id"] = tenure.id; target_data["initialization"]["applicability"]["point"] = point(3, "other")
    assert any(item["code"] == "GEN-TENURE-LINK-001" for item in validate_generational_world(candidate))
    target_data.pop("predecessor_tenure_id"); target_data["initialization"]["applicability"]["point"] = point(3)
    tenure_data["transitions"] = []
    duplicate = deepcopy(parent.frontmatter); duplicate["id"] = ident("parentage", "duplicate"); duplicate["initialization"]["transition_id"] = ident("generational-transition", "duplicate"); duplicate["initialization"]["applicability"] = {"point": point(-1), "applicability_kind": "instant"}
    duplicate_parent = _record(duplicate); candidate.records[duplicate_parent.id] = duplicate_parent
    assert any(item["code"] == "GEN-PARENTAGE-003" for item in validate_generational_world(candidate))
    candidate.records.pop(duplicate_parent.id)
    malformed_parent = deepcopy(parent.frontmatter); malformed_parent["id"] = ident("parentage", "malformed-app"); malformed_parent["initialization"]["transition_id"] = ident("generational-transition", "malformed-app"); malformed_parent["initialization"]["applicability"] = {"applicability_kind": "instant", "point": []}
    malformed_parent_record = _record(malformed_parent); candidate.records[malformed_parent_record.id] = malformed_parent_record
    assert any(item["field"] == "initialization.applicability.point" for item in validate_generational_world(candidate))
    candidate.records.pop(malformed_parent_record.id)
    parent_cycle = base("parentage", "parent-cycle", "history.kinship", {"child_id": alpha.id, "parent_id": child.id}, {"basis": "adoptive"}, 1)
    parent_cycle_record = _record(parent_cycle); candidate.records[parent_cycle_record.id] = parent_cycle_record
    assert any(item["code"] == "GEN-PARENTAGE-002" for item in validate_generational_world(candidate))
    candidate.records.pop(parent_cycle_record.id)
    org_cycle = base("organization", "org-cycle", "history.organizations", {"organization_kind": "house", "parent_id": org.id}, {"title": "cycle", "aliases": []}, -1)
    org_cycle_record = _record(org_cycle); candidate.records[org_cycle_record.id] = org_cycle_record; org_data["parent_id"] = org_cycle_record.id
    assert any(item["code"] == "GEN-ORGANIZATION-002" for item in validate_generational_world(candidate))
    org_data.pop("parent_id"); candidate.records.pop(org_cycle_record.id)
    overlap = deepcopy(affiliation.frontmatter); overlap["id"] = ident("affiliation", "overlap"); overlap["initialization"]["transition_id"] = ident("generational-transition", "overlap")
    overlap_record = _record(overlap); candidate.records[overlap_record.id] = overlap_record
    assert any(item["code"] == "GEN-AFFILIATION-001" for item in validate_generational_world(candidate))
    candidate.records.pop(overlap_record.id)
    # Compact table-like negative cases: mutate one valid vector at a time.
    negative_cases: list[tuple[str, object, object]] = [
        ("wrong endpoint kind", lambda: parent.frontmatter.__setitem__("parent_id", org.id), lambda: parent.frontmatter.__setitem__("parent_id", alpha.id)),
        ("participant cardinality", lambda: union.frontmatter.__setitem__("participant_ids", [alpha.id]), lambda: union.frontmatter.__setitem__("participant_ids", participants)),
        ("backward transition", lambda: org_data.__setitem__("transitions", [transition("organization-dormant", "late", 3, {}), transition("organization-rename", "early", 2, {"title": "early", "aliases": []})]), lambda: org_data.__setitem__("transitions", [])),
        ("illegal state", lambda: union.frontmatter.__setitem__("transitions", [transition("union-reconcile", "illegal", 1, {"participant_ids": participants})]), lambda: union.frontmatter.__setitem__("transitions", [])),
        ("nonreciprocal claim", lambda: claim_b_data["initialization"]["payload"].__setitem__("competes_with", []), lambda: claim_b_data["initialization"]["payload"].__setitem__("competes_with", [claim_a.id])),
        ("reversed vital", lambda: vital_data.__setitem__("transitions", [transition("vital-death", "reverse-death", -1, {}), transition("vital-birth", "reverse-birth", 1, {})]), lambda: vital_data.__setitem__("transitions", [transition("vital-birth", "birth", -1, {}), transition("vital-death", "death", 6, {})])),
    ]
    expected = {"wrong endpoint kind": "GEN-REF-001", "participant cardinality": "GEN-UNION-001", "backward transition": "GEN-TRANSITION-004", "illegal state": "GEN-STATE-001", "nonreciprocal claim": "GEN-CLAIM-001", "reversed vital": "GEN-STATE-001"}
    for label, apply, restore in negative_cases:
        apply()  # type: ignore[operator]
        assert any(item["code"] == expected[label] for item in validate_generational_world(candidate)), label
        restore()  # type: ignore[operator]
    org_data["initialization"]["payload"]["aliases"] = ["z", "a"]
    assert any(item["field"] == "initialization.payload.aliases" for item in validate_generational_world(candidate))
    org_data["initialization"]["payload"]["aliases"] = []
    unknown_character = ident("character", "missing")
    union.frontmatter["initialization"]["payload"]["participant_ids"] = sorted([alpha.id, unknown_character])
    assert any(item["field"] == "initialization.payload.participant_ids" for item in validate_generational_world(candidate))
    union.frontmatter["initialization"]["payload"]["participant_ids"] = participants
    unknown_claim = ident("claim", "missing")
    claim_a_data["initialization"]["payload"]["competes_with"] = [unknown_claim]
    assert any(item["field"] == "initialization.payload.competes_with" for item in validate_generational_world(candidate))
    claim_a_data["initialization"]["payload"]["competes_with"] = [claim_b.id]
    tenure_data["transitions"] = [{"transition_id": ident("generational-transition", "bad-interval"), "transition_kind": "tenure-vacate", "applicability": {"applicability_kind": "inclusive-interval", "first": point(4), "last": point(1)}, "payload": {"holder_id": None}}]
    assert any(item["code"] == "GEN-APPLICABILITY-001" for item in validate_generational_world(candidate))
    tenure_data["transitions"] = []


def test_generational_serialization_keeps_transition_and_payload_order() -> None:
    record = {"schema": "wedl/v0.7", "kind": "tenure", "id": id_from_seed("tenure", "ordered"), "title": "Ordered", "domain": "history.tenures", "status": "canonical", "tags": [], "aliases": [], "threads": [], "audience": ["public"], "perspectives": ["ordinary"], "legacy_id": id_from_seed("legacy", "ordered"), "initialization": {"payload": {"basis": "legal", "holder_id": None}, "applicability": {"point": {"order": 0, "tick": 0, "timeline": "main"}, "applicability_kind": "instant"}, "transition_kind": "tenure-initialize", "transition_id": id_from_seed("generational-transition", "ordered-init")}, "transitions": [{"payload": {"basis": "legal", "holder_id": id_from_seed("character", "ordered")}, "applicability": {"point": {"order": 0, "tick": 1, "timeline": "main"}, "applicability_kind": "instant"}, "transition_kind": "tenure-hold", "transition_id": id_from_seed("generational-transition", "ordered-hold")}]} 
    text = serialize_record(record, "").decode("utf-8")
    assert text.index("transition_id:") < text.index("transition_kind:") < text.index("applicability:") < text.index("payload:")
    initialization = text[text.index("initialization:"):text.index("transitions:")]
    assert initialization.index("transition_id:") < initialization.index("transition_kind:") < initialization.index("applicability:") < initialization.index("payload:")
    assert initialization.index("holder_id:") < initialization.index("basis:")
    payload = text[text.index("payload:", text.index("transitions:")):]
    assert payload.index("holder_id:") < payload.index("basis:")
