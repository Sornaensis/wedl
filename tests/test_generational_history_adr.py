"""Static conformance checks for accepted ADR 0005; no runtime is implied."""

from __future__ import annotations

from adrai_fixtures import current_decision, current_status

import re
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = current_decision("A01M48NQZ50KH9V094Q5A3138R0")
SCHEMA = ROOT / "architecture/adrai/examples/generational-schema-v07.yaml"
QUERIES = ROOT / "architecture/adrai/examples/generational-query-v1.yaml"
MIGRATION = ROOT / "architecture/adrai/examples/generational-migration-v07.yaml"


def _yaml(path: Path) -> dict:
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def _citations(value: object) -> list[dict]:
    if isinstance(value, dict):
        own = [value] if {"record_id", "path", "applicability"} <= set(value) else []
        return own + [item for child in value.values() for item in _citations(child)]
    if isinstance(value, list):
        return [item for child in value for item in _citations(child)]
    return []


def test_adr_is_accepted_indexed_and_contract_only() -> None:
    text = ADR.read_text(encoding="utf-8")
    index = ADR.metadata
    assert current_status(index["adr"])["state"] == "active"
    assert "**Status:** Accepted" in text
    assert "**Ratification:** The current project owner explicitly approved this exact proposal on 2026-08-30 after independent review." in text
    assert "not a parser,\nruntime, canonical story source, SQLite schema" in text
    assert "d76aacb1-671c-458e-b6a3-ce132c82c83a" in text
    assert "1910b6ab-b0ec-4dae-8263-e54f570687df" in text
    assert index["adr"] == "A01M48NQZ50KH9V094Q5A3138R0" and ADR.path.is_file()
    assert index["title"] == "First-class generational history and coordinated v0.7 contract"
    assert "# ADR 0005:" in text
    assert 'schema = "adrai/decision/v1"' in text
    for document in ("ADR 0002", "ADR 0003", "ADR 0004", "THREAD_SCHEMA_CONTRACT", "CHRONOLOGY_MIGRATION_CONTRACT"):
        assert document in text


def test_literal_frontmatter_uses_one_transition_model() -> None:
    vector = _yaml(SCHEMA)
    assert vector["status"] == "accepted-contract-only"
    assert vector["schema"] == "wedl/v0.7" and vector["protocol"] == "wedl-generational/v1"
    registry = vector["capability_registry"]
    assert registry["canonical_order"] == ["generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"]
    envelopes = registry["envelopes"]
    assert envelopes["combined_core_and_optional"]["world"]["capabilities"] == registry["canonical_order"]
    for name in ("generational_only", "spatial_only", "combined_core_and_optional"):
        assert envelopes[name]["state"] == "valid"
    for name in ("missing_capabilities", "unknown_capability", "duplicate_capability", "unsorted_capability", "missing_prerequisite"):
        assert envelopes[name]["state"] == "invalid" and envelopes[name]["code"] == "GEN-CAPABILITY-001"
    world_required = vector["authoritative_world_envelope"]
    assert world_required == {"capability_location": "world.frontmatter.capabilities-only", "required": ["schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "timelines", "default_timeline", "chronology", "capabilities"], "kind": "world", "record_capabilities": "forbidden", "timelines": "array-of-closed-id-label-optional-origin-mappings", "chronology": "closed-v06-world-declaration-calendars-eras-anchors"}
    for envelope_case in envelopes.values():
        world = envelope_case["world"]
        required_fields = world_required["required"] if "capabilities" in world else [field for field in world_required["required"] if field != "capabilities"]
        assert all(field in world for field in required_fields)
        assert world["kind"] == "world"
        assert isinstance(world["timelines"], list) and world["timelines"]
        assert world["default_timeline"] == world["timelines"][0]["id"]
        for timeline in world["timelines"]:
            assert {"id", "label"} <= set(timeline) <= {"id", "label", "origin"}
            assert isinstance(timeline["id"], str) and timeline["id"]
            assert isinstance(timeline["label"], str) and timeline["label"]
            if "origin" in timeline:
                assert set(timeline["origin"]) == {"tick", "label"}
                assert isinstance(timeline["origin"]["tick"], int) and isinstance(timeline["origin"]["label"], str) and timeline["origin"]["label"]
        assert world["chronology"] == {"calendars": [], "eras": [], "anchors": []}
    envelope = vector["source_envelope"]
    assert envelope["kind_is_record_discriminator"] is True
    assert envelope["capabilities"] == "forbidden-on-non-world-records"
    assert set(envelope["kinds"]) == {"organization", "parentage", "union", "affiliation", "legacy", "tenure", "claim", "vital-history"}
    variants = vector["closed_variants"]
    assert variants["organization_kind"] == ["house", "dynasty", "clan", "institution", "other"]
    assert variants["legacy_kind"] == ["office", "estate", "title", "other"]
    assert len(variants["transition_kind"]) == 36
    assert "organization-rename" in variants["transition_kind"] and "tenure-transfer" in variants["transition_kind"] and "vital-existence-end" in variants["transition_kind"]
    contract = vector["transition_contract"]
    assert contract["initialization"]["required"] == ["transition_id", "transition_kind", "applicability", "payload"]
    assert contract["transition"]["optional"] == ["cause_event_id", "replaces_transition_id"]
    assert contract["replacement"]["fold"] == "replacement-wins-but-superseded-citation-retained"
    assert contract["cause_endpoint"] == {"field": "cause_event_id", "optional": True, "nullable": False, "reference_kind": "event", "reference_prefix": "event_", "target_must_exist": True, "target_type": "event", "target_timeline": "same-as-transition", "target_story_time": "strictly-before-transition"}
    payload_schemas = contract["payload_schemas"]
    assert set(payload_schemas) == set(variants["transition_kind"])
    for payload in payload_schemas.values():
        assert set(payload) == {"required", "properties", "exact"}
        assert payload["exact"] is True and set(payload["required"]) == set(payload["properties"])
    for empty_kind in ("organization-dormant", "parentage-end", "union-end", "affiliation-end", "legacy-dissolve", "tenure-end", "claim-reject", "vital-initialize", "vital-birth", "vital-death", "vital-existence-start", "vital-existence-end"):
        assert payload_schemas[empty_kind] == {"required": [], "properties": {}, "exact": True}
    folds = vector["state_fold_contract"]
    assert set(folds) == {"universal", "organization", "parentage", "union", "affiliation", "legacy", "tenure", "claim", "vital_history"}
    assert folds["universal"]["initialization"] == "exactly-one-first-transition-and-matches-record-kind"
    assert folds["universal"]["timeline"] == "one-per-record-and-every-transition-or-interval-endpoint-matches-it"
    for record_kind, table_key in (("organization", "organization"), ("parentage", "parentage"), ("union", "union"), ("affiliation", "affiliation"), ("legacy", "legacy"), ("tenure", "tenure"), ("claim", "claim"), ("vital-history", "vital_history")):
        table = folds[table_key]
        assert table["states"] and table["initialization"] and table["transitions"]
        assert all(kind.startswith(record_kind.replace("vital-history", "vital")) for kind in table["initialization"])
    applicability = vector["applicability_union"]
    assert applicability["discriminator"] == "applicability_kind"
    assert set(applicability["variants"]) == {"static", "instant", "inclusive_interval"}
    records = vector["frontmatter_vectors"]
    assert set(records).issuperset({"organization", "parentage", "union", "affiliation", "legacy", "tenure", "claim", "vital_history", "future_parentage", "withheld_parentage"})
    transition_ids = set()
    events = vector["endpoint_vectors"]["events"]
    for record in records.values():
        assert all(field in record for field in envelope["required"])
        assert "capabilities" not in record
        assert record["schema"] == "wedl/v0.7" and record["status"] == "canonical" and record["threads"] == []
        assert re.fullmatch(r"(?:organization|kinship|union|affiliation|legacy|tenure|claim|vital)_[0-9A-HJKMNP-TV-Z]{26}", record["id"])
        for transition in [record["initialization"], *record["transitions"]]:
            assert set(transition).issuperset({"transition_id", "transition_kind", "applicability", "payload"})
            assert re.fullmatch(r"transition_[0-9A-HJKMNP-TV-Z]{26}", transition["transition_id"])
            assert transition["transition_kind"] in variants["transition_kind"]
            assert transition["transition_id"] not in transition_ids
            transition_ids.add(transition["transition_id"])
            assert set(transition["payload"]) == set(payload_schemas[transition["transition_kind"]]["required"])
            applicability_kind = transition["applicability"]["applicability_kind"]
            if transition["transition_kind"] == "tenure-vacate":
                assert applicability_kind == "inclusive-interval"
                assert set(transition["applicability"]) == {"applicability_kind", "first", "last"}
            else:
                assert applicability_kind == "instant"
                assert set(transition["applicability"]["point"]) == {"timeline", "tick", "order"}
            if "cause_event_id" in transition:
                event = events[transition["cause_event_id"]]
                point = transition["applicability"]["point"]
                assert event["kind"] == "event" and event["id"] == transition["cause_event_id"]
                assert event["story_time"]["timeline"] == point["timeline"]
                assert (event["story_time"]["tick"], event["story_time"]["order"]) < (point["tick"], point["order"])
        def sort_start(item: dict) -> tuple[str, int, int]:
            applicability = item["applicability"]
            point = applicability["point"] if applicability["applicability_kind"] == "instant" else applicability["first"]
            return point["timeline"], point["tick"], point["order"]
        starts = [sort_start(item) for item in [record["initialization"], *record["transitions"]]]
        assert len({timeline for timeline, _, _ in starts}) == 1
        assert starts == sorted(starts, key=lambda value: value[1:])
    assert records["organization"]["organization_kind"] == "house"
    assert records["legacy"]["legacy_kind"] == "office"
    assert records["future_parentage"]["initialization"]["applicability"]["point"] == {"timeline": "main", "tick": 5, "order": 0}
    assert records["withheld_parentage"]["audience"] == ["archivist"]
    assert vector["privacy_contract"]["viewer_selector"]["binding"] == "server-authenticated-principal-not-caller-chosen"
    assert records["vital_history"]["initialization"]["payload"] == {}
    assert records["tenure_vacancy"]["initialization"]["payload"] == {"holder_id": "character_alpha", "basis": "legal"}
    assert records["tenure_vacancy"]["transitions"][0]["applicability"] == {"applicability_kind": "inclusive-interval", "first": {"timeline": "main", "tick": -1, "order": 1}, "last": {"timeline": "main", "tick": 0, "order": -1}}
    assert vector["history_rules"]["tenure_interval_rule"] == "tenancy-is-folded-at-the-query-point; tenure-vacate-replaces-holding-with-vacant-only-for-its-literal-inclusive-interval; before-first-use-predecessor-fold; after-last-run-closed-tenure-vacate-expire-and-fold-other-literal-tenures; current-holder-selects-only-holding-tenures-at-the-query-point"
    assert vector["history_rules"]["tenure_vacate_expiry"] == {"operation": "tenure-vacate-expire", "closed": True, "trigger": "query-point-strictly-after-inclusive-last", "input_state": "vacant-interval", "output_state": "expired", "persisted_transition": "forbidden", "effect": "vacancy-no-longer-applies-and-other-literal-tenures-fold"}
    assert vector["history_rules"]["tenure_fold_vectors"] == {"before": {"at": {"timeline": "main", "tick": -1, "order": 0}, "vacancy_record_state": "holding", "holders": ["character_alpha"]}, "inside": {"at": {"timeline": "main", "tick": -1, "order": 1}, "vacancy_record_state": "vacant-interval", "holders": []}, "after": {"at": {"timeline": "main", "tick": 0, "order": 0}, "vacancy_record_state": "expired", "holders": ["character_alpha", "character_beta"]}}
    assert folds["tenure"]["fold_operations"] == {"tenure-vacate-expire": {"from": "vacant-interval", "to": "expired", "trigger": "query-point-strictly-after-inclusive-last", "persisted_transition": "forbidden"}}
    assert records["tenure_coholder"]["initialization"]["payload"] == {"holder_id": "character_beta", "basis": "de-facto"}
    assert records["tenure"]["transitions"][-1]["transition_kind"] == "tenure-transfer"
    assert records["tenure"]["transitions"][-1]["payload"] == {"from_tenure_id": "tenure_00000000000000000000000002", "to_tenure_id": "tenure_00000000000000000000000004"}
    assert records["tenure"]["transitions"][-1]["cause_event_id"] == "event_transfer"
    record_ids = {record["id"] for record in records.values()}
    catalog = vector["reference_catalog"]
    characters = set(catalog["characters"])
    for target_id, target in catalog["characters"].items():
        assert target == {"kind": "character", "id": target_id}
    for target_id, target in catalog["locations"].items():
        assert target == {"kind": "location", "id": target_id}
    assert catalog["records"] == "frontmatter_vectors-by-id"
    for key in ("parentage", "parentage_adoptive", "future_parentage", "withheld_parentage"):
        assert records[key]["child_id"] in characters and records[key]["parent_id"] in characters
    assert set(records["union"]["participant_ids"]) <= characters
    assert records["affiliation"]["character_id"] in characters and records["affiliation"]["organization_id"] in record_ids
    assert records["organization"]["location_id"] in catalog["locations"] and records["organization_child"]["parent_id"] in record_ids
    for key in ("legacy", "tenure_vacancy", "tenure", "tenure_coholder", "tenure_transfer_target", "claim", "claim_competing"):
        record = records[key]
        if "legacy_id" in record:
            assert record["legacy_id"] in record_ids
    for key in ("tenure", "tenure_transfer_target"):
        assert records[key]["predecessor_tenure_id"] in record_ids
    transfer = records["tenure"]["transitions"][-1]["payload"]
    assert transfer["from_tenure_id"] == records["tenure"]["id"] and transfer["to_tenure_id"] == records["tenure_transfer_target"]["id"]
    for key in ("claim", "claim_competing"):
        record = records[key]
        assert record["claimant_id"] in characters
        for transition in [record["initialization"], *record["transitions"]]:
            assert set(transition["payload"].get("competes_with", [])) <= record_ids
    for key in ("vital_history", "vital_construct"):
        assert records[key]["character_id"] in characters
    tenure = vector["history_rules"]["tenure_example"]
    assert tenure["vacancy"]["applicability"]["last"] == {"timeline": "main", "tick": 0, "order": -1}
    assert tenure["current_holders"] == ["tenure_00000000000000000000000002", "tenure_00000000000000000000000003"]


def test_query_vectors_close_privacy_time_and_citation_applicability() -> None:
    vector = _yaml(QUERIES)
    schema = _yaml(SCHEMA)
    record_paths = {
        record["id"]: schema["identity"]["paths"][next(prefix for prefix in schema["identity"]["paths"] if record["id"].startswith(prefix))].replace("<id>", record["id"])
        for record in schema["frontmatter_vectors"].values()
    }
    common = vector["common"]
    assert common["states"] == ["available", "unknown", "unavailable", "invalid", "limit"]
    assert common["viewer_contract"]["selector"] == {"required": True, "value": "authenticated-principal", "binding": "server-authenticated-principal-not-caller-chosen"}
    assert common["mode_contracts"]["author-all-time"] == {"required": ["timeline"], "forbidden": ["at"], "filter_order": ["authenticated_viewer", "server_derived_audience", "server_derived_perspective", "capability", "timeline", "record_audience", "record_perspective", "applicability", "ordering", "pagination", "serialization", "derived_traversal"], "future_visibility": "all-authored-applicable-facts-through-revision"}
    catalog_characters = set(schema["reference_catalog"]["characters"])
    record_ids = {record["id"] for record in schema["frontmatter_vectors"].values()}
    for case in vector["queries"].values():
        request = case["request"]
        assert request["protocol"] == vector["protocol"] and request["viewer"] == {"selector": "authenticated-principal"}
        assert "audience" not in request and "perspective" not in request
        if request["mode"] == "author-all-time":
            assert request["timeline"] == "main" and "at" not in request
        else:
            assert isinstance(request["at"]["tick"], str) and request["at"]["tick"] == str(int(request["at"]["tick"]))
            assert isinstance(request["at"]["order"], str) and request["at"]["order"] == str(int(request["at"]["order"]))
        if "character_id" in request:
            assert request["character_id"] in catalog_characters
        for field in ("union_id", "organization_id", "legacy_id"):
            if field in request:
                assert request[field] in record_ids
        for relation in case["response"].get("relations", []):
            assert relation["target_id"] in catalog_characters
        if case["response"]["state"] == "available":
            citations = _citations(case["response"])
            assert citations
            for citation in citations:
                assert set(citation) == {"record_id", "path", "applicability"}
                assert citation["path"].endswith(".md")
                assert citation["record_id"] in record_paths and citation["path"] == record_paths[citation["record_id"]]
                assert citation["applicability"]["applicability_kind"] in {"static", "instant", "inclusive-interval"}
    hidden = vector["queries"]["secret_lineage_character"]["response"]
    assert hidden == vector["queries"]["secret_lineage_absent_character"]["response"] == vector["queries"]["secret_lineage_future_character"]["response"] == {"state": "unknown", "relations": []}
    assert vector["queries"]["vital_pre_birth_character"]["response"] == vector["queries"]["vital_unknown"]["response"] == {"state": "unknown"}
    assert vector["queries"]["author_all_time_future_visible"]["response"]["relations"][0]["target_id"] == "character_future_parent"
    assert vector["queries"]["vital_death_before_same_tick"]["response"]["vital"] == "living"
    assert vector["queries"]["vital_death_at_same_tick"]["response"]["vital"] == "dead"
    assert vector["transport_rejections"]["numeric_tick"]["expected"]["code"] == "GEN-REQUEST-001"
    assert vector["transport_rejections"]["plus_prefixed_tick"]["expected"]["state"] == "invalid"
    assert vector["transport_rejections"]["leading_zero_negative_tick"]["expected"]["state"] == "invalid"


def test_migration_binds_closed_target_capabilities_to_preview_apply() -> None:
    vector = _yaml(MIGRATION)
    assert vector["status"] == "accepted-contract-only"
    assert vector["protocol"] == "wedl-migration/v1" and vector["mode"] == "upgrade-v07"
    target = vector["target"]
    assert target["capability_location"] == "authoritative-world-frontmatter-only"
    assert target["canonical_capability_order"] == ["generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"]
    assert target["fingerprint_input"] == '{"schema":"wedl/v0.7","capabilities":["generational-core-v1","spatial-core-v1","geometry-v1","route-v1","overlay-v1"]}'
    migration = vector["single_migration"]
    assert migration["command"] == "wedl migrate preview|apply --mode upgrade-v07"
    assert migration["reproducible_preview_command"].startswith("wedl migrate preview --mode upgrade-v07 --expected-head")
    assert "target-capabilities" not in migration["reproducible_preview_command"]
    assert migration["reproducible_apply_command"].endswith("--confirm <preview-confirmation-token>")
    assert "--request-hash" not in migration["reproducible_apply_command"] and "--confirmation-token" not in migration["reproducible_apply_command"]
    request = migration["request_contract"]
    capabilities = ["generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"]
    assert request["preview_cli_required"] == ["protocol", "mode", "expectedHead", "idempotencyKey"]
    assert request["targetCapabilities"] == {"closed": True, "normalized_by": "canonical_capability_order", "allowed": capabilities, "location": "authoritative-world-preview-plan", "transport": "server-derived-not-cli-flag", "legacy_default": ["generational-core-v1", "spatial-core-v1"], "selection": "legacy-v03-v05-v06-use-legacy_default; v07-noop-uses-exact-existing-world-capabilities"}
    assert request["normalized_request"] == {"base_required": ["protocol", "mode", "expectedHead", "sourceSnapshotHash", "idempotencyKey", "targetCapabilities"], "base_request_hash": "canonical-json-sha256-before-backupRef", "append": ["backupRef"], "request_hash": "canonical-json-sha256-after-backupRef"}
    assert request["requestHash"] == "server-derived-preview-response-not-cli-argument"
    assert request["confirmation_binds"] == ["normalized_request", "requestHash"]
    assert request["confirmation_transport"] == "preview-derives-token-from-normalized_request-and-requestHash; apply-passes-token-only-via---confirm; token-is-not-an-input-to-its-own-binding"
    assert request["apply_cli_required"] == ["protocol", "mode", "expectedHead", "sourceSnapshotHash", "idempotencyKey", "confirm"]
    assert request["apply_rechecks"] == ["expectedHead", "cleanManagedSourceAndIndex", "sourceSnapshotHash", "completeNormalizedRequest", "serverDerivedRequestHash", "confirm"]
    assert request["preservation"] == ["thread-declarations", "record-thread-membership-byte-semantics", "chronology-world-declaration", "chronology-record-semantics", "no-added-spatial-facts", "no-added-generational-facts"]
    for name in ("generational_only_v07", "spatial_only_v07", "combined_optional_v07"):
        assert vector["cases"][name]["expected"] == {"state": "noop", "idempotent": True}
    for case in vector["cases"].values():
        assert case["targetCapabilities"] == sorted(case["targetCapabilities"], key=capabilities.index)
        assert set(case["targetCapabilities"]) <= set(capabilities)
    for name in ("v03_preservation", "v05_thread_preservation", "v06_chronology_preservation"):
        assert vector["cases"][name]["targetCapabilities"] == request["targetCapabilities"]["legacy_default"]
    assert vector["cases"]["unsupported_capability"]["expected"] == {"state": "rejected", "code": "GEN-CAPABILITY-001"}
