from pathlib import Path
from copy import deepcopy
import re
import yaml

import wedl.chronology_validation as chronology_validation
from wedl.chronology_validation import PRODUCTION_DIAGNOSTICS, _error, validate_v06_candidate
from wedl.chronology import (
    CalendarDefinition, CalendarEpoch, CivilDate, CycleOverride, CycleRule,
    EraDate, EraDefinition, Invalid, LocalDay, Month, MonthDelta, Ok,
    AxisDay, era_to_civil, normalize_date, prepare_calendar, prepare_catalog,
    prepare_era,
)
from wedl.validation import validate_world
from wedl.model import Record, World
from wedl.errors import SupersededSchemaError

I64_MAX = 2**63 - 1


def _record(data, path="story/world.md"):
    return Record(data, "", path, b"")


def _world(table_years=None):
    calendar = {
        "id": "calendar_0123456789abcdefghjkmnpqrs", "label": "Solar",
        "months": [{"number": 1, "days": 30}],
        "rule": {"kind": "table", "years": [{"year": 0, "overrides": []}] if table_years is None else table_years},
        "epoch": {"civil": {"year": 0, "month": 1, "day": 1}, "axis_day": 0},
    }
    world = _record({"schema": "wedl/v0.6", "id": "world_0123456789ABCDEFGHJKMNPQRS", "kind": "world", "status": "canonical", "title": "World", "domain": "main", "tags": [], "aliases": [], "threads": [], "timelines": [{"id": "main", "label": "Main"}], "chronology": {"calendars": [calendar], "eras": [], "anchors": []}})
    return World("", "", {world.id: world}, Path("."))


def test_v06_validation_declares_the_active_read_side_boundary():
    assert "active ``wedl/v0.6`` chronology grammar" in chronology_validation.__doc__
    assert "public chronology boundary" in chronology_validation.__doc__


def test_valid_candidate_is_clean_for_active_v06_read_side_support():
    assert validate_v06_candidate(_world()) == []


def test_empty_table_is_a_single_precise_definition_error():
    errors = validate_v06_candidate(_world([]))
    assert [(item["code"], item["field"]) for item in errors] == [("WDL-CAL-007", "chronology.calendars[0].rule.years")]


def test_calendar_diagnostics_preserve_source_identity_and_exact_field():
    candidate = _world()
    candidate.world_record.frontmatter["chronology"]["calendars"][0]["months"] = []
    assert validate_v06_candidate(candidate) == [{
        "code": "WDL-CAL-002", "message": "calendar months must contain 1..64 rows",
        "severity": "error", "entityId": candidate.world_record.id,
        "path": "story/world.md", "field": "chronology.calendars[0].months",
    }]


def test_nested_chronology_is_owned_by_the_v06_boundary_not_generic_refs():
    candidate = _world()
    candidate.world_record.frontmatter["effects"] = [{"chronology": {"id": "chronology_nope"}}]
    errors = validate_v06_candidate(candidate)
    assert [(item["code"], item["field"]) for item in errors] == [("WDL-CHRON-003", "effects[0].chronology")]


def test_mixed_versions_are_rejected_without_generic_reference_noise():
    candidate = _world()
    other = _record({"schema": "wedl/v0.5", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character"}, "story/char.md")
    candidate.records[other.id] = other
    errors = validate_v06_candidate(candidate)
    assert [item["code"] for item in errors] == ["WDL-SRC-008"]


def test_existing_runtime_versions_stay_on_the_existing_validator_path():
    candidate = _world()
    candidate.world_record.frontmatter["schema"] = "wedl/v0.3"
    assert validate_v06_candidate(candidate) == validate_world(candidate)


def test_table_gap_is_a_date_diagnostic_at_the_authored_year_leaf():
    candidate = _world()
    record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"civil": {"calendar_id": "calendar_0123456789abcdefghjkmnpqrs", "year": 1}}}]}, "story/c.md")
    candidate.records[record.id] = record
    errors = validate_v06_candidate(candidate)
    assert [(item["code"], item["path"], item["field"]) for item in errors] == [("WDL-DATE-002", "story/c.md", "chronology[0].value.civil.year")]


def test_v04_quarantine_precedes_v06_dispatch():
    candidate = _world()
    candidate.world_record.frontmatter["schema"] = "wedl/v0.4"
    try:
        validate_v06_candidate(candidate)
    except SupersededSchemaError as exc:
        assert exc.details["schema"] == "wedl/v0.4"
    else:
        raise AssertionError("v0.4 quarantine was not preserved")


def test_nested_lifecycle_chronology_and_one_sided_approximation():
    candidate = _world()
    record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "lifecycle": {"transitions": [{"chronology": []}]}, "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"approx": {"display_value": "around", "bounds": {"lower": {"calendar_id": "calendar_0123456789abcdefghjkmnpqrs", "year": 0}, "upper": None}}}}]}, "story/c.md")
    candidate.records[record.id] = record
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [("WDL-CHRON-003", "lifecycle.transitions[0].chronology")]


def _catalog_from_markdown(document: str):
    rows = re.findall(r"^\| (WDL-[A-Z]+-\d+) \| (error) \| (.+) \|$", document, re.M)
    return rows


def _set_path(document, path, value, *, delete=False, append=False):
    target = document
    for key in path[:-1]:
        target = target[key]
    if append:
        target[path[-1]].append(value)
    elif delete:
        del target[path[-1]]
    else:
        target[path[-1]] = value


def _schema_world(document):
    candidate = _world()
    candidate.world_record.frontmatter["schema"] = document["schema"]
    chronology = deepcopy(document["valid"]["world"]["chronology"])
    candidate.world_record.frontmatter["chronology"] = chronology
    other = _record({
        "schema": document["schema"], "id": "char_0123456789ABCDEFGHJKMNPQRS",
        "kind": "character", "status": "canonical", "title": "C", "domain": "main",
        "tags": [], "aliases": [], "chronology": deepcopy(document["valid"]["nonworld"]["chronology"]),
    }, "story/c.md")
    candidate.records[other.id] = other
    return candidate


def _apply_schema_mutation(document, mutation):
    for item in mutation.get("mutations", [mutation]):
        path = item["path"]
        if item.get("reverse"):
            target = document
            for key in path: target = target[key]
            target.reverse()
            continue
        if "append_repeat" in item:
            for _ in range(item["count"]): _set_path(document, path, deepcopy(item["append_repeat"]), append=True)
        elif "append_year_rows" in item:
            for index in range(item["append_year_rows"]): _set_path(document, path, {"year": index + 1, "overrides": []}, append=True)
        else:
            _set_path(document, path, deepcopy(item.get("append", item.get("value"))), delete=item.get("delete", False), append="append" in item)


def test_production_registry_markdown_and_schema_vector_are_bidirectionally_exact():
    root = Path(__file__).parents[1]
    example = yaml.safe_load((root / "docs/examples/chronology-validation-v06.yaml").read_text())
    catalog_rows = _catalog_from_markdown((root / "docs/CHRONOLOGY_VALIDATION.md").read_text())
    vector_rows = [tuple(item) for item in example["diagnostics"]]
    # Check raw published rows before set conversion, so a duplicate cannot be
    # hidden by the production registry's intentionally immutable frozenset.
    assert len(catalog_rows) == len(set(catalog_rows)) == len(PRODUCTION_DIAGNOSTICS)
    assert len(vector_rows) == len(set(vector_rows)) == len(PRODUCTION_DIAGNOSTICS)
    catalog, vector = set(catalog_rows), set(vector_rows)
    assert PRODUCTION_DIAGNOSTICS == vector == catalog
    representative = _record({"id": "world_0123456789ABCDEFGHJKMNPQRS"})
    for code, severity, message in vector:
        assert _error(code, message, representative, "chronology") == {
            "code": code, "message": message, "severity": severity,
            "entityId": representative.id, "path": "story/world.md", "field": "chronology",
        }
    with __import__("pytest").raises(AssertionError, match="unregistered chronology diagnostic"):
        _error("WDL-CHRON-999", "not a wire message", None, None)


def test_every_schema_negative_document_runs_through_the_source_boundary():
    root = Path(__file__).parents[1]
    schema = yaml.safe_load((root / "docs/examples/chronology-schema-v06.yaml").read_text())
    validation = yaml.safe_load((root / "docs/examples/chronology-validation-v06.yaml").read_text())
    observed = set()
    expected_fields = ("code", "severity", "message", "entityId", "path", "field")
    expectations = validation["negative_diagnostic_expectations"]
    assert {item["category"] for item in schema["negative_documents"]} == set(expectations)
    for mutation in schema["negative_documents"]:
        document = deepcopy(schema)
        _apply_schema_mutation(document, mutation)
        diagnostics = validate_v06_candidate(_schema_world(document))
        expected = [dict(zip(expected_fields, row, strict=True)) for row in expectations[mutation["category"]]]
        assert diagnostics == expected, mutation["category"]
        for item in diagnostics:
            assert (item["code"], item["severity"], item["message"]) in PRODUCTION_DIAGNOSTICS
            assert item["entityId"] and item["path"] and item["field"] is not None
            observed.add((item["code"], item["severity"], item["message"]))
    # The schema fixture is a conformance corpus, not a hand-picked sample.
    assert observed <= PRODUCTION_DIAGNOSTICS


def test_complete_schema_positive_corpus_is_clean_through_the_boundary():
    root = Path(__file__).parents[1]
    schema = yaml.safe_load((root / "docs/examples/chronology-schema-v06.yaml").read_text())
    assert validate_v06_candidate(_schema_world(schema)) == []
    for key in ("positive_document_mutations", "positive_annotation_order_mutations"):
        for mutation in schema[key]:
            document = deepcopy(schema)
            _apply_schema_mutation(document, mutation)
            assert validate_v06_candidate(_schema_world(document)) == [], (key, mutation)
    for key in ("positive_effective_epoch_mutations", "positive_table_delta_epoch_mutations", "positive_effective_order_mutations"):
        document = deepcopy(schema)
        for mutation in schema[key]: _apply_schema_mutation(document, mutation)
        assert validate_v06_candidate(_schema_world(document)) == [], key
    for key in ("positive_anchor_alias", "positive_anchor_boundaries", "positive_anchor_id_order_not_axis_order"):
        document = deepcopy(schema)
        document["valid"]["world"]["chronology"]["anchors"] = deepcopy(schema[key]["anchors"])
        assert validate_v06_candidate(_schema_world(document)) == [], key


def test_anchor_map_failures_have_exact_authored_leaves():
    candidate = _world()
    anchors = candidate.world_record.frontmatter["chronology"]["anchors"]
    anchors[:] = [
        {"id": "chronology_0123456789abcdefghjkmnpqrs", "axis_day": 0, "story_time": {"timeline": "main", "tick": 0, "order": 0}, "provenance": ["a"]},
        {"id": "chronology_1123456789abcdefghjkmnpqrs", "axis_day": 0, "story_time": {"timeline": "main", "tick": 1, "order": 0}, "provenance": ["b"]},
    ]
    assert validate_v06_candidate(candidate) == [{"code": "WDL-ANCHOR-007", "message": "anchors must map monotonically between StoryTime and axis day", "severity": "error", "entityId": candidate.world_record.id, "path": "story/world.md", "field": "chronology.anchors[1].story_time"}]


def test_canonical_definition_id_collisions_are_owned_by_second_source_id():
    candidate = _world()
    lower = deepcopy(candidate.world_record.frontmatter["chronology"]["calendars"][0])
    upper = deepcopy(lower); upper["id"] = upper["id"].upper().replace("CALENDAR_", "calendar_")
    candidate.world_record.frontmatter["chronology"]["calendars"] = [upper, lower]
    error = validate_v06_candidate(candidate)
    assert [(item["code"], item["field"]) for item in error] == [("WDL-CAL-001", "chronology.calendars[1].id")]
    candidate = _world()
    era = {
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": "calendar_0123456789abcdefghjkmnpqrs",
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
    }
    candidate.world_record.frontmatter["chronology"]["eras"] = [{**deepcopy(era), "id": era["id"].upper().replace("ERA_", "era_")}, era]
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [("WDL-ERA-001", "chronology.eras[1].id")]
    candidate = _world()
    anchor = {"id": "chronology_0123456789abcdefghjkmnpqrs", "axis_day": 0, "story_time": {"timeline": "main", "tick": 0, "order": 0}, "provenance": ["ledger"]}
    candidate.world_record.frontmatter["chronology"]["anchors"] = [{**deepcopy(anchor), "id": anchor["id"].upper().replace("CHRONOLOGY_", "chronology_")}, anchor]
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [("WDL-ANCHOR-001", "chronology.anchors[1].id")]


def test_relative_null_and_x_nested_chronology_keep_precise_paths():
    candidate = _world()
    record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "x-operational": {"chronology": []}, "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"relative": {"relation": "before", "before_id": None}}}]}, "story/c.md")
    candidate.records[record.id] = record
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [
        ("WDL-DATE-006", "chronology[0].value.relative.before_id"),
        ("WDL-CHRON-003", "x-operational.chronology"),
    ]


def test_definition_and_annotation_replay_stays_at_authored_leaves():
    """Kernel rejection chooses a supplied operand, never a reconstructed parent."""
    cases = [
        (lambda world: world.world_record.frontmatter["chronology"]["calendars"][0]["rule"]["years"][0]["overrides"].append({"target_month": True, "delta_days": 1}), "chronology.calendars[0].rule.years[0].overrides[0].target_month"),
        (lambda world: world.world_record.frontmatter["chronology"]["calendars"][0]["rule"]["years"][0]["overrides"].append({"intercalary_month": {"number": 1, "days": 4097}}), "chronology.calendars[0].rule.years[0].overrides[0].intercalary_month.number"),
        (lambda world: world.world_record.frontmatter["chronology"]["calendars"][0]["epoch"].__setitem__("axis_day", True), "chronology.calendars[0].epoch.axis_day"),
        (lambda world: world.world_record.frontmatter["chronology"]["calendars"][0]["epoch"]["civil"].__setitem__("year", True), "chronology.calendars[0].epoch.civil.year"),
        (lambda world: world.world_record.frontmatter["chronology"]["calendars"][0]["epoch"]["civil"].__setitem__("month", True), "chronology.calendars[0].epoch.civil.month"),
        (lambda world: world.world_record.frontmatter["chronology"]["calendars"][0]["epoch"]["civil"].__setitem__("day", True), "chronology.calendars[0].epoch.civil.day"),
    ]
    for mutate, field in cases:
        candidate = _world(); mutate(candidate)
        assert validate_v06_candidate(candidate)[0]["field"] == field
    candidate = _world()
    era = {"id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": "calendar_0123456789abcdefghjkmnpqrs", "label": "E", "aliases": [], "display_year_zero": True, "display_epoch": {"display_year": True, "machine_year": 0}, "provenance": ["ledger"], "bounds": {"lower": {"year": 0, "month": 1, "day": 1}, "upper": {"year": 1, "month": 1, "day": 1}}}
    candidate.world_record.frontmatter["chronology"]["eras"] = [era]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].display_epoch.display_year"
    candidate = _world(); era["display_epoch"] = {"display_year": 0, "machine_year": 0}; era["bounds"]["lower"]["day"] = True; candidate.world_record.frontmatter["chronology"]["eras"] = [era]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].bounds.lower.day"


def test_selected_signed_year_layout_drives_epoch_date_range_and_approximation_leaves():
    candidate = _world()
    calendar = candidate.world_record.frontmatter["chronology"]["calendars"][0]
    calendar["rule"] = {"kind": "table", "years": [
        {"year": -1, "overrides": []},
        {"year": 1, "overrides": [{"intercalary_month": {"number": 2, "days": 2}}]},
    ]}
    calendar["epoch"]["civil"] = {"year": 1, "month": 2, "day": 3}
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.calendars[0].epoch.civil.day"

    def annotation(tag, payload):
        world = _world([{"year": -1, "overrides": []}, {"year": 1, "overrides": [{"intercalary_month": {"number": 2, "days": 2}}]}])
        world.world_record.frontmatter["chronology"]["calendars"][0]["epoch"]["civil"]["year"] = -1
        record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {tag: payload}}]}, "story/c.md")
        world.records[record.id] = record
        return validate_v06_candidate(world)[0]["field"]

    calendar_id = "calendar_0123456789abcdefghjkmnpqrs"
    assert annotation("civil", {"calendar_id": calendar_id, "year": 0}) == "chronology[0].value.civil.year"
    assert annotation("civil", {"calendar_id": calendar_id, "year": -1, "month": 2}) == "chronology[0].value.civil.month"
    assert annotation("range", {"calendar_id": calendar_id, "lower": {"calendar_id": calendar_id, "year": 1, "month": 2, "day": 3}, "upper": None}) == "chronology[0].value.range.lower.day"
    assert annotation("approx", {"display_value": "about", "bounds": {"lower": None, "upper": {"calendar_id": calendar_id, "year": -1, "month": 2}}}) == "chronology[0].value.approx.bounds.upper.month"


def test_era_bounds_use_selected_layout_endpoints_then_lower_for_reversal():
    candidate = _world()
    candidate.world_record.frontmatter["chronology"]["calendars"][0]["rule"] = {"kind": "cycle", "period": 1, "overrides": []}
    era = {"id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": "calendar_0123456789abcdefghjkmnpqrs", "label": "E", "aliases": [], "display_year_zero": True, "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"], "bounds": {"lower": {"year": 0, "month": 2, "day": 1}, "upper": {"year": 0, "month": 1, "day": 31}}}
    candidate.world_record.frontmatter["chronology"]["eras"] = [era]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].bounds.lower.month"
    era["bounds"]["lower"] = {"year": 0, "month": 1, "day": 1}
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].bounds.upper.day"
    era["bounds"]["upper"] = {"year": -1, "month": 1, "day": 1}
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].bounds.lower.year"


def test_endpoint_layout_validation_precedes_range_and_approximation_ordering():
    calendar_id = "calendar_0123456789abcdefghjkmnpqrs"

    def candidate_for(tag, payload):
        candidate = _world()
        record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {tag: payload}}]}, "story/c.md")
        candidate.records[record.id] = record
        return candidate

    bad_lower = {"calendar_id": calendar_id, "year": 0, "month": 2, "day": 1}
    upper = {"calendar_id": calendar_id, "year": -1, "month": 1, "day": 1}
    result = validate_v06_candidate(candidate_for("range", {"calendar_id": calendar_id, "lower": bad_lower, "upper": upper}))
    assert [(item["code"], item["field"]) for item in result] == [("WDL-DATE-003", "chronology[0].value.range.lower.month")]
    result = validate_v06_candidate(candidate_for("approx", {"display_value": "about", "bounds": {"lower": bad_lower, "upper": upper}}))
    assert [(item["code"], item["field"]) for item in result] == [("WDL-DATE-003", "chronology[0].value.approx.bounds.lower.month")]

    candidate = _world()
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
        "bounds": {"lower": {"year": 0, "month": 2, "day": 1}, "upper": {"year": -1, "month": 1, "day": 1}},
    }]
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [("WDL-ERA-005", "chronology.eras[0].bounds.lower.month")]


def test_checked_arithmetic_keeps_the_authored_operand_leaf():
    calendar_id = "calendar_0123456789abcdefghjkmnpqrs"
    overflowing_year = (2**63 - 1) // 3

    def cycle_world():
        candidate = _world()
        calendar = candidate.world_record.frontmatter["chronology"]["calendars"][0]
        calendar["months"] = [{"number": 1, "days": 3}]
        calendar["rule"] = {"kind": "cycle", "period": 1, "overrides": []}
        return candidate

    candidate = cycle_world()
    calendar = candidate.world_record.frontmatter["chronology"]["calendars"][0]
    calendar["epoch"] = {"civil": {"year": overflowing_year, "month": 1, "day": 3}, "axis_day": 0}
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.calendars[0].epoch.civil.day"
    candidate = _world()
    calendar = candidate.world_record.frontmatter["chronology"]["calendars"][0]
    calendar["epoch"] = {"civil": {"year": 0, "month": 1, "day": 2}, "axis_day": -(2**63)}
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.calendars[0].epoch.axis_day"

    def annotation(value):
        candidate = cycle_world()
        record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": value}]}, "story/c.md")
        candidate.records[record.id] = record
        return validate_v06_candidate(candidate)

    exact = {"calendar_id": calendar_id, "year": overflowing_year, "month": 1, "day": 3}
    upper = {"calendar_id": calendar_id, "year": -1, "month": 1, "day": 1}
    assert annotation({"civil": exact})[0]["field"] == "chronology[0].value.civil.day"
    assert annotation({"range": {"calendar_id": calendar_id, "lower": exact, "upper": upper}})[0]["field"] == "chronology[0].value.range.lower.day"
    assert annotation({"approx": {"display_value": "about", "bounds": {"lower": exact, "upper": upper}}})[0]["field"] == "chronology[0].value.approx.bounds.lower.day"
    year_stage = {"calendar_id": calendar_id, "year": 2**63 - 1, "month": 1, "day": 1}
    assert annotation({"civil": year_stage})[0]["field"] == "chronology[0].value.civil.year"
    assert annotation({"range": {"calendar_id": calendar_id, "lower": year_stage, "upper": upper}})[0]["field"] == "chronology[0].value.range.lower.year"
    assert annotation({"approx": {"display_value": "about", "bounds": {"lower": year_stage, "upper": upper}}})[0]["field"] == "chronology[0].value.approx.bounds.lower.year"

    candidate = cycle_world()
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
        "bounds": {"lower": {"year": 0, "month": 1, "day": 1}, "upper": {"year": overflowing_year, "month": 1, "day": 3}},
    }]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].bounds.upper.day"
    candidate = cycle_world()
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
        "bounds": {"lower": {"year": 2**63 - 1, "month": 1, "day": 1}, "upper": {"year": -1, "month": 1, "day": 1}},
    }]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology.eras[0].bounds.lower.year"

    candidate = _world()
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 2**63 - 1}, "provenance": ["ledger"],
    }]
    record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"era": {"era_id": "era_0123456789abcdefghjkmnpqrs", "year": 1}}}]}, "story/c.md")
    candidate.records[record.id] = record
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology[0].value.era.year"


def test_prepared_kernel_cycle_prefix_and_calendar_gate_keep_exact_leaves():
    """The validator replays prepared divmod/prefix stages before eras exist."""
    calendar_id = "calendar_0123456789abcdefghjkmnpqrs"
    candidate = _world()
    calendar = candidate.world_record.frontmatter["chronology"]["calendars"][0]
    calendar["months"] = [{"number": 1, "days": 2}]
    calendar["rule"] = {"kind": "cycle", "period": 2, "overrides": [
        {"residue": 1, "target_month": 1, "delta_days": -1},
    ]}
    # The prepared layouts have cycle_days=3 and prefix[1]=2.  The positive
    # product is I64_MAX-1, while the negative product is I64_MIN-1 and only
    # becomes representable after that residue prefix is added.
    prefix_year = 6148914691236517205
    negative_prefix_year = -6148914691236517205
    direct = CalendarDefinition(
        "calendar_0123456789ABCDEFGHJKMNPQRS", "Solar", (Month(1, 2),),
        CycleRule(2, (CycleOverride(1, MonthDelta(1, -1)),)),
        CalendarEpoch(LocalDay(0, 1, 1), AxisDay(0)),
    )
    prepared = prepare_calendar(direct)
    assert isinstance(prepared, Ok)
    catalog = prepare_catalog((direct,))
    assert isinstance(catalog, Ok)
    assert isinstance(normalize_date(catalog.value, CivilDate(direct.id, prefix_year, 1, 1)), Invalid)
    assert isinstance(normalize_date(catalog.value, CivilDate(direct.id, negative_prefix_year, 1, 1)), Ok)

    epoch_candidate = _world()
    epoch_calendar = epoch_candidate.world_record.frontmatter["chronology"]["calendars"][0]
    epoch_calendar["months"] = deepcopy(calendar["months"])
    epoch_calendar["rule"] = deepcopy(calendar["rule"])
    epoch_calendar["epoch"]["civil"]["year"] = prefix_year
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(epoch_candidate)] == [
        ("WDL-CAL-011", "chronology.calendars[0].epoch.civil.year"),
    ]

    record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"civil": {"calendar_id": calendar_id, "year": prefix_year, "month": 1, "day": 1}}}]}, "story/c.md")
    candidate.records[record.id] = record
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [
        ("WDL-DATE-003", "chronology[0].value.civil.year"),
    ]

    candidate.records[record.id].frontmatter["chronology"][0]["value"]["civil"]["year"] = negative_prefix_year
    assert validate_v06_candidate(candidate) == []

    # A failed full calendar preparation always wins over a later malformed era
    # endpoint; the adapter must never inspect eras against an unprepared layout.
    candidate = _world()
    candidate.world_record.frontmatter["chronology"]["calendars"][0]["rule"]["years"][0]["overrides"] = [{"target_month": 2, "delta_days": 1}]
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
        "bounds": {"lower": {"year": 0, "month": 2, "day": 1}, "upper": {"year": 0, "month": 1, "day": 1}},
    }]
    assert [(item["code"], item["field"]) for item in validate_v06_candidate(candidate)] == [
        ("WDL-CAL-011", "chronology.calendars[0].rule.years[0].overrides[0].target_month"),
    ]


def test_era_conversion_replays_checked_subtraction_then_addition():
    calendar = CalendarDefinition(
        "calendar_0123456789ABCDEFGHJKMNPQRS", "Solar", (Month(1, 2),),
        CycleRule(1, ()), CalendarEpoch(LocalDay(0, 1, 1), AxisDay(0)),
    )
    prepared = prepare_calendar(calendar)
    assert isinstance(prepared, Ok)
    cases = [
        (I64_MAX, -1, -1),  # shown ordinal minus epoch ordinal overflows.
        (I64_MAX, 0, 1),    # subtraction fits, machine-year addition overflows.
    ]
    for display, display_epoch, machine_epoch in cases:
        era = EraDefinition("era_0123456789ABCDEFGHJKMNPQRS", calendar.id, "E", (), True, display_epoch, machine_epoch)
        prepared_era = prepare_era(era, prepared.value)
        assert isinstance(prepared_era, Ok)
        assert isinstance(era_to_civil(prepared_era.value, EraDate(era.id, display)), Invalid)

        candidate = _world()
        candidate.world_record.frontmatter["chronology"]["eras"] = [{
            "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": "calendar_0123456789abcdefghjkmnpqrs",
            "label": "E", "aliases": [], "display_year_zero": True,
            "display_epoch": {"display_year": display_epoch, "machine_year": machine_epoch}, "provenance": ["ledger"],
        }]
        record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"era": {"era_id": "era_0123456789abcdefghjkmnpqrs", "year": display}}}]}, "story/c.md")
        candidate.records[record.id] = record
        assert validate_v06_candidate(candidate) == [{
            "code": "WDL-ERA-008", "message": "era date is semantically invalid", "severity": "error",
            "entityId": record.id, "path": "story/c.md", "field": "chronology[0].value.era.year",
        }]


def test_endpoint_normalization_replays_axis_offset_at_authored_precision_before_ordering():
    """Every selected endpoint includes the final axis-offset stage.

    A calendar without an epoch is still a valid local-only calendar.  With an
    epoch at I64_MAX, however, the final selected endpoint overflows and must
    beat a simultaneous reversed upper endpoint.
    """
    calendar_id = "calendar_0123456789abcdefghjkmnpqrs"

    def candidate_for(tag, payload, *, epoch=True):
        candidate = _world()
        calendar = candidate.world_record.frontmatter["chronology"]["calendars"][0]
        if epoch:
            calendar["epoch"] = {"civil": {"year": 0, "month": 1, "day": 1}, "axis_day": I64_MAX}
        else:
            calendar["epoch"] = None
        record = _record({"schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character", "status": "canonical", "title": "C", "domain": "main", "tags": [], "aliases": [], "chronology": [{"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {tag: payload}}]}, "story/c.md")
        candidate.records[record.id] = record
        return candidate

    exact = {"calendar_id": calendar_id, "year": 0, "month": 1, "day": 2}
    month = {"calendar_id": calendar_id, "year": 0, "month": 1}
    year = {"calendar_id": calendar_id, "year": 0}
    assert validate_v06_candidate(candidate_for("civil", exact))[0]["field"] == "chronology[0].value.civil.day"
    assert validate_v06_candidate(candidate_for("civil", month))[0]["field"] == "chronology[0].value.civil.month"
    assert validate_v06_candidate(candidate_for("civil", year))[0]["field"] == "chronology[0].value.civil.year"
    assert validate_v06_candidate(candidate_for("civil", year, epoch=False)) == []

    reversed_upper = {"calendar_id": calendar_id, "year": -1, "month": 1, "day": 1}
    range_value = {"calendar_id": calendar_id, "lower": month, "upper": reversed_upper}
    approx_value = {"display_value": "about", "bounds": {"lower": month, "upper": reversed_upper}}
    assert validate_v06_candidate(candidate_for("range", range_value))[0]["field"] == "chronology[0].value.range.lower.month"
    assert validate_v06_candidate(candidate_for("approx", approx_value))[0]["field"] == "chronology[0].value.approx.bounds.lower.month"

    candidate = candidate_for("era", {"era_id": "era_0123456789abcdefghjkmnpqrs", "year": 1, "month": 1, "day": 1})
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": I64_MAX}, "provenance": ["ledger"],
    }]
    # Conversion fails before an otherwise-invalid selected month/day can be inspected.
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology[0].value.era.year"

    candidate = candidate_for("era", {"era_id": "era_0123456789abcdefghjkmnpqrs", "year": 0, "month": 1, "day": 2})
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
        # Bounds are local ordinals: an otherwise valid axis epoch must not
        # make this era definition invalid.
        "bounds": {"lower": {"year": 0, "month": 1, "day": 1}, "upper": {"year": 0, "month": 1, "day": 2}},
    }]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology[0].value.era.day"

    # Authored claims still normalize against the shared axis, including when
    # nested below a conflict. The local-only definition rule is not a waiver
    # for a conversion claim.
    candidate = candidate_for("conflict", {"claims": [
        {"era": {"era_id": "era_0123456789abcdefghjkmnpqrs", "year": 0, "month": 1, "day": 2}},
        {"civil": {"calendar_id": calendar_id, "year": 0, "month": 1, "day": 1}},
    ]})
    candidate.world_record.frontmatter["chronology"]["eras"] = [{
        "id": "era_0123456789abcdefghjkmnpqrs", "calendar_id": calendar_id,
        "label": "E", "aliases": [], "display_year_zero": True,
        "display_epoch": {"display_year": 0, "machine_year": 0}, "provenance": ["ledger"],
        "bounds": {"lower": {"year": 0, "month": 1, "day": 1}, "upper": {"year": 0, "month": 1, "day": 2}},
    }]
    assert validate_v06_candidate(candidate)[0]["field"] == "chronology[0].value.conflict.claims[0].value.era.day"


def test_published_endpoint_precedence_and_checked_arithmetic_examples_are_exact():
    example = yaml.safe_load((Path(__file__).parents[1] / "docs/examples/chronology-validation-v06.yaml").read_text())
    assert example["endpoint_precedence_examples"] == {
        "range_invalid_lower_month_reversed": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.range.lower.month"],
        "approximation_invalid_lower_month_reversed": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.approx.bounds.lower.month"],
        "era_bounds_invalid_lower_month_reversed": ["WDL-ERA-005", "error", "invalid era bounds", "chronology.eras[0].bounds.lower.month"],
    }
    assert example["endpoint_normalization_examples"] == {
        "exact_axis_offset_overflow": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.civil.day"],
        "partial_month_axis_offset_overflow": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.civil.month"],
        "partial_year_axis_offset_overflow": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.civil.year"],
        "range_lower_offset_overflow_precedes_order": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.range.lower.month"],
        "approximation_lower_offset_overflow_precedes_order": ["WDL-DATE-003", "error", "civil date is semantically invalid", "chronology[0].value.approx.bounds.lower.month"],
        "era_machine_overflow_precedes_month_day": ["WDL-ERA-008", "error", "era date is semantically invalid", "chronology[0].value.era.year"],
        "era_civil_offset_overflow": ["WDL-ERA-008", "error", "era date is semantically invalid", "chronology[0].value.era.day"],
        "era_bounds_are_local_but_conflict_claims_normalize": ["WDL-ERA-008", "error", "era date is semantically invalid", "chronology[0].value.conflict.claims[0].value.era.day"],
    }
    assert example["checked_arithmetic_examples"]["calendar_epoch_offset_overflow"][-1] == "chronology.calendars[0].epoch.axis_day"
    assert example["checked_arithmetic_examples"]["era_machine_conversion_overflow"][-1] == "chronology[0].value.era.year"
    assert example["kernel_parity_examples"] == {
        "cycle_prefix_overflow": ["WDL-DATE-003", "error", "civil date is semantically invalid", "char_0123456789ABCDEFGHJKMNPQRS", "story/c.md", "chronology[0].value.civil.year"],
        "calendar_precedes_era_endpoint": ["WDL-CAL-011", "error", "calendar definition is semantically invalid", "world_0123456789ABCDEFGHJKMNPQRS", "story/world.md", "chronology.calendars[0].rule.years[0].overrides[0].target_month"],
        "era_conversion_subtraction_overflow": ["WDL-ERA-008", "error", "era date is semantically invalid", "char_0123456789ABCDEFGHJKMNPQRS", "story/c.md", "chronology[0].value.era.year"],
        "era_conversion_addition_overflow": ["WDL-ERA-008", "error", "era date is semantically invalid", "char_0123456789ABCDEFGHJKMNPQRS", "story/c.md", "chronology[0].value.era.year"],
    }
