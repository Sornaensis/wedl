"""Pure test-local structural oracle for the active ``wedl/v0.6`` grammar.

This intentionally imports no runtime schema code, preserving an independent
structural oracle for the documented contract.
"""
from adrai_fixtures import current_decision

from copy import deepcopy
from pathlib import Path
import re

import yaml

ROOT = Path(__file__).resolve().parents[1]
DOC = current_decision("A01M48XWQD0809VQ3RD9NBYPSKB")
VECTOR = ROOT / "architecture/adrai/examples/chronology-schema-v06.yaml"
LEGACY_CROCKFORD = r"[0-9abcdefghjkmnpqrstvwxyz]{26}"
UPPER_CROCKFORD = r"[0-9ABCDEFGHJKMNPQRSTVWXYZ]{26}"
I64 = (-(2**63), 2**63 - 1)
I32 = (-(2**31), 2**31 - 1)


def _id(value, prefix):
    assert isinstance(value, str) and re.fullmatch(prefix + f"(?:{LEGACY_CROCKFORD}|{UPPER_CROCKFORD})", value)


def _generated_id(value, prefix):
    assert isinstance(value, str) and re.fullmatch(prefix + UPPER_CROCKFORD, value)


def _i64(value):
    assert type(value) is int and I64[0] <= value <= I64[1]


def _i32(value):
    assert type(value) is int and I32[0] <= value <= I32[1]


def _checked_add(left, right):
    _i64(left); _i64(right)
    result = left + right
    _i64(result)
    return result


def _checked_sub(left, right):
    _i64(left); _i64(right)
    result = left - right
    _i64(result)
    return result


def _display_ordinal(year, zero_enabled):
    _i64(year)
    if zero_enabled:
        return year
    assert year != 0
    return year if year < 0 else year - 1


def _era_machine_year(era, display_year):
    epoch = era["display_epoch"]
    zero_enabled = era["display_year_zero"]
    return _checked_add(
        epoch["machine_year"],
        _checked_sub(_display_ordinal(display_year, zero_enabled), _display_ordinal(epoch["display_year"], zero_enabled)),
    )


def _keys(value, required, optional=()):
    assert isinstance(value, dict)
    allowed = set(required) | set(optional)
    assert set(required) <= set(value)
    assert all(key in allowed or key.startswith("x-") for key in value)


def _provenance(value):
    assert isinstance(value, list) and value and all(isinstance(item, str) for item in value)


def _civil(value, calendars, eras, *, implicit_calendar=None):
    required = ("year",) if implicit_calendar is not None else ("calendar_id", "year")
    _keys(value, required, ("calendar_id", "month", "day"))
    calendar_id = value.get("calendar_id", implicit_calendar)
    assert calendar_id in calendars
    _i64(value["year"])
    if "month" in value:
        assert type(value["month"]) is int and 1 <= value["month"] <= 64
    assert "day" not in value or "month" in value
    if "day" in value:
        assert type(value["day"]) is int and 1 <= value["day"] <= 4096
    if implicit_calendar is None:
        _calendar_civil({key: value[key] for key in ("year", "month", "day") if key in value}, calendars[calendar_id])


def _annotation_value(value, calendars, eras, stable_ids, depth=0):
    assert depth <= 64 and isinstance(value, dict)
    tags = [key for key in value if not key.startswith("x-")]
    assert len(tags) == 1
    tag, payload = tags[0], value[tags[0]]
    if tag == "civil":
        _civil(payload, calendars, eras)
    elif tag == "era":
        _keys(payload, ("era_id", "year"), ("month", "day"))
        assert payload["era_id"] in eras
        _i64(payload["year"])
        if "month" in payload:
            assert type(payload["month"]) is int and 1 <= payload["month"] <= 64
        assert "day" not in payload or "month" in payload
        if "day" in payload:
            assert type(payload["day"]) is int and 1 <= payload["day"] <= 4096
        era = eras[payload["era_id"]]
        machine_year = _era_machine_year(era, payload["year"])
        calendar = calendars[era["calendar_id"]]
        interval = _civil_interval({"year": machine_year, **{key: payload[key] for key in ("month", "day") if key in payload}}, calendar)
        if "bounds" in era:
            bounds = era["bounds"]
            lower_bound = _civil_interval(bounds["lower"], calendar)[0]
            upper_bound = _civil_interval(bounds["upper"], calendar)[1]
            assert _interval_within(interval, (lower_bound, upper_bound))
    elif tag == "range":
        _keys(payload, ("calendar_id", "lower", "upper"))
        assert payload["calendar_id"] in calendars
        for endpoint in (payload["lower"], payload["upper"]):
            if endpoint is not None:
                _civil(endpoint, calendars, eras)
                assert endpoint["calendar_id"] == payload["calendar_id"]
        if payload["lower"] is not None and payload["upper"] is not None:
            calendar = calendars[payload["calendar_id"]]
            lower = _civil_interval({key: payload["lower"][key] for key in ("year", "month", "day") if key in payload["lower"]}, calendar)
            upper = _civil_interval({key: payload["upper"][key] for key in ("year", "month", "day") if key in payload["upper"]}, calendar)
            assert _ordered_intervals(lower, upper)
    elif tag == "approx":
        _keys(payload, ("display_value", "bounds"))
        assert isinstance(payload["display_value"], str)
        _keys(payload["bounds"], ("lower", "upper"))
        endpoints = [payload["bounds"][key] for key in ("lower", "upper") if payload["bounds"][key] is not None]
        for endpoint in endpoints:
            _civil(endpoint, calendars, eras)
        assert len({endpoint["calendar_id"] for endpoint in endpoints}) <= 1
        if len(endpoints) == 2:
            calendar = calendars[endpoints[0]["calendar_id"]]
            lower = _civil_interval({key: endpoints[0][key] for key in ("year", "month", "day") if key in endpoints[0]}, calendar)
            upper = _civil_interval({key: endpoints[1][key] for key in ("year", "month", "day") if key in endpoints[1]}, calendar)
            assert _ordered_intervals(lower, upper)
    elif tag == "conflict":
        _keys(payload, ("claims",))
        assert isinstance(payload["claims"], list) and 2 <= len(payload["claims"]) <= 64
        for claim in payload["claims"]:
            _annotation_value(claim, calendars, eras, stable_ids, depth + 1)
    elif tag == "relative":
        _keys(payload, ("relation",), ("before_id", "after_id"))
        assert isinstance(payload["relation"], str)
        targets = [payload[key] for key in ("before_id", "after_id") if key in payload]
        assert targets and all(target in stable_ids for target in targets)
    elif tag == "duration":
        _keys(payload, ("unit", "value"))
        assert payload["unit"] in ("year", "month", "day")
        _i64(payload["value"])
    else:
        raise AssertionError(f"unknown annotation tag {tag}")


def _overrides(overrides, base_months, *, period=None):
    assert isinstance(overrides, list) and len(overrides) <= 500
    sort_keys, seen = [], set()
    for override in overrides:
        assert isinstance(override, dict)
        if period is None:
            assert "residue" not in override
            prefix = ()
        else:
            assert type(override.get("residue")) is int and 0 <= override["residue"] < period
            prefix = (override["residue"],)
        if "target_month" in override:
            expected = {"target_month", "delta_days"} | ({"residue"} if period is not None else set())
            _keys(override, expected)
            target = override["target_month"]
            assert type(target) is int and target in base_months
            _i64(override["delta_days"])
            assert 1 <= base_months[target] + override["delta_days"] <= 4096
            identity, order = target, target
        else:
            expected = {"intercalary_month"} | ({"residue"} if period is not None else set())
            _keys(override, expected)
            month = override["intercalary_month"]
            _keys(month, ("number", "days"), ("label",))
            assert type(month["number"]) is int and 1 <= month["number"] <= 64
            assert type(month["days"]) is int and 1 <= month["days"] <= 4096
            assert month["number"] not in base_months
            if "label" in month:
                assert isinstance(month["label"], str)
            identity, order = month["number"], month["number"]
        key = prefix + (identity,)
        assert key not in seen
        seen.add(key)
        sort_keys.append(prefix + (order,))
    assert sort_keys == sorted(sort_keys)


def _effective_months(rule, base_months, year):
    """Build the finite date table selected by one signed machine year."""
    months = dict(base_months)
    if rule["kind"] == "cycle":
        applicable = [override for override in rule["overrides"] if override["residue"] == year % rule["period"]]
    else:
        matching_rows = [row for row in rule["years"] if row["year"] == year]
        assert len(matching_rows) == 1, "table calendar years are nonrecurring and gaps unavailable"
        applicable = matching_rows[0]["overrides"]
    for override in applicable:
        if "target_month" in override:
            months[override["target_month"]] = base_months[override["target_month"]] + override["delta_days"]
        else:
            month = override["intercalary_month"]
            months[month["number"]] = month["days"]
    return months


def _calendar_civil(civil, calendar):
    """Validate a complete owning-calendar civil date against its selected year."""
    _keys(civil, ("year",), ("month", "day"))
    _i64(civil["year"])
    base_months = {month["number"]: month["days"] for month in calendar["months"]}
    effective_months = _effective_months(calendar["rule"], base_months, civil["year"])
    if "month" not in civil:
        assert "day" not in civil
        return
    assert type(civil["month"]) is int and civil["month"] in effective_months
    if "day" in civil:
        assert type(civil["day"]) is int and 1 <= civil["day"] <= effective_months[civil["month"]]


def _civil_interval(civil, calendar):
    """Expand a valid year/month/day precision to inclusive effective civil days."""
    _calendar_civil(civil, calendar)
    year = civil["year"]
    base_months = {month["number"]: month["days"] for month in calendar["months"]}
    effective_months = _effective_months(calendar["rule"], base_months, year)
    months = sorted(effective_months)
    if "month" not in civil:
        return ((year, 0, 1), (year, len(months) - 1, effective_months[months[-1]]))
    ordinal = months.index(civil["month"])
    if "day" not in civil:
        return ((year, ordinal, 1), (year, ordinal, effective_months[civil["month"]]))
    return ((year, ordinal, civil["day"]), (year, ordinal, civil["day"]))


def _interval_within(inner, outer):
    return outer[0] <= inner[0] and inner[1] <= outer[1]


def _ordered_intervals(lower, upper):
    return lower[0] <= upper[1]


def _epoch(value, rule, base_months):
    """Validate an owning-calendar epoch against that year's effective months."""
    _keys(value, ("civil", "axis_day"))
    civil = value["civil"]
    _keys(civil, ("year", "month", "day"))
    _i64(civil["year"])
    effective_months = _effective_months(rule, base_months, civil["year"])
    assert type(civil["month"]) is int and civil["month"] in effective_months
    assert type(civil["day"]) is int and 1 <= civil["day"] <= effective_months[civil["month"]]
    assert "calendar_id" not in civil
    _i64(value["axis_day"])


def validate_document(document):
    """Validate one complete vector document and return coalesced anchor aliases."""
    assert document["schema"] == "wedl/v0.6"
    valid = document["valid"]
    _keys(valid, ("records", "world", "nonworld"))
    assert isinstance(valid["records"], list)
    record_ids = []
    for record in valid["records"]:
        _keys(record, ("id",))
        assert isinstance(record["id"], str) and re.fullmatch(r"[a-z]+_(?:" + LEGACY_CROCKFORD + "|" + UPPER_CROCKFORD + ")", record["id"])
        record_ids.append(record["id"])
    assert len(record_ids) == len(set(record_ids))
    world = valid["world"]
    _keys(world, ("timelines", "chronology"))
    assert isinstance(world["timelines"], list) and world["timelines"]
    timelines = set(world["timelines"])
    assert len(timelines) == len(world["timelines"]) and all(isinstance(item, str) for item in timelines)
    chronology = world["chronology"]
    _keys(chronology, ("calendars", "eras", "anchors"))
    for name in ("calendars", "eras", "anchors"):
        assert isinstance(chronology[name], list) and len(chronology[name]) <= 500

    calendar_ids = [calendar["id"] for calendar in chronology["calendars"]]
    assert calendar_ids == sorted(set(calendar_ids))
    calendars = {}
    for calendar in chronology["calendars"]:
        _keys(calendar, ("id", "label", "rule", "months", "epoch"))
        _id(calendar["id"], "calendar_")
        assert isinstance(calendar["label"], str)
        months = calendar["months"]
        assert isinstance(months, list) and 1 <= len(months) <= 64
        numbers, base_months = [], {}
        for month in months:
            _keys(month, ("number", "days"), ("label",))
            assert type(month["number"]) is int and 1 <= month["number"] <= 64
            assert type(month["days"]) is int and 1 <= month["days"] <= 4096
            if "label" in month:
                assert isinstance(month["label"], str)
            numbers.append(month["number"])
            base_months[month["number"]] = month["days"]
        assert numbers == sorted(set(numbers))
        rule = calendar["rule"]
        assert isinstance(rule, dict) and rule.get("kind") in ("cycle", "table")
        if rule["kind"] == "cycle":
            _keys(rule, ("kind", "period", "overrides"))
            assert type(rule["period"]) is int and 1 <= rule["period"] <= 500
            _overrides(rule["overrides"], base_months, period=rule["period"])
        else:
            _keys(rule, ("kind", "years"))
            assert isinstance(rule["years"], list) and 1 <= len(rule["years"]) <= 500
            years = []
            for row in rule["years"]:
                _keys(row, ("year", "overrides"))
                _i64(row["year"])
                years.append(row["year"])
                _overrides(row["overrides"], base_months)
            assert years == sorted(set(years))
        if calendar["epoch"] is not None:
            _epoch(calendar["epoch"], rule, base_months)
        assert len(months) + (
            len(rule["overrides"])
            if rule["kind"] == "cycle"
            else len(rule["years"]) + sum(len(row["overrides"]) for row in rule["years"])
        ) <= 500
        calendars[calendar["id"]] = calendar

    era_ids = [era["id"] for era in chronology["eras"]]
    assert era_ids == sorted(set(era_ids))
    eras = {}
    for era in chronology["eras"]:
        _keys(era, ("id", "calendar_id", "label", "aliases", "display_year_zero", "display_epoch", "provenance"), ("bounds",))
        _id(era["id"], "era_")
        assert era["calendar_id"] in calendars and isinstance(era["label"], str)
        assert isinstance(era["aliases"], list) and all(isinstance(alias, str) for alias in era["aliases"])
        assert type(era["display_year_zero"]) is bool
        _keys(era["display_epoch"], ("display_year", "machine_year"))
        _i64(era["display_epoch"]["display_year"])
        _i64(era["display_epoch"]["machine_year"])
        _display_ordinal(era["display_epoch"]["display_year"], era["display_year_zero"])
        _provenance(era["provenance"])
        if "bounds" in era:
            _keys(era["bounds"], ("lower", "upper"))
            calendar = calendars[era["calendar_id"]]
            for bound in (era["bounds"]["lower"], era["bounds"]["upper"]):
                _keys(bound, ("year", "month", "day"))
                _calendar_civil(bound, calendar)
            lower = era["bounds"]["lower"]
            upper = era["bounds"]["upper"]
            assert (lower["year"], lower["month"], lower["day"]) <= (upper["year"], upper["month"], upper["day"])
        eras[era["id"]] = era

    anchors = chronology["anchors"]
    anchor_ids = [anchor["id"] for anchor in anchors]
    assert anchor_ids == sorted(set(anchor_ids))
    pairs = {}
    for anchor in anchors:
        _keys(anchor, ("id", "axis_day", "story_time", "provenance"))
        _id(anchor["id"], "chronology_")
        _i64(anchor["axis_day"])
        _provenance(anchor["provenance"])
        story_time = anchor["story_time"]
        _keys(story_time, ("timeline", "tick", "order"))
        assert story_time["timeline"] in timelines
        _i64(story_time["tick"])
        _i32(story_time["order"])
        pair = (anchor["axis_day"], (story_time["timeline"], story_time["tick"], story_time["order"]))
        pairs.setdefault(pair, []).extend(anchor["provenance"])
    coordinates = sorted(pairs)
    assert len({axis for axis, _ in pairs}) == len(pairs)
    assert len({story_time for _, story_time in pairs}) == len(pairs)
    assert all(coordinates[index][1] < coordinates[index + 1][1] for index in range(len(coordinates) - 1))

    nonworld = valid["nonworld"]
    _keys(nonworld, ("kind", "chronology"))
    assert isinstance(nonworld["kind"], str)
    annotations = nonworld["chronology"]
    assert isinstance(annotations, list) and len(annotations) <= 500
    annotation_ids = [annotation["id"] for annotation in annotations]
    assert len(annotation_ids) == len(set(annotation_ids))
    for annotation in annotations:
        _keys(annotation, ("id", "provenance", "value"), ("role", "display"))
        _id(annotation["id"], "chronology_")
        _provenance(annotation["provenance"])
        if "role" in annotation:
            assert isinstance(annotation["role"], str)
        if "display" in annotation:
            assert isinstance(annotation["display"], str)
        _annotation_value(annotation["value"], calendars, eras, set(record_ids))
    return {"anchor_provenance": pairs}


def _mutate(document, mutation):
    target = document
    for key in mutation["path"][:-1]:
        target = target[key]
    key = mutation["path"][-1]
    if mutation.get("delete"):
        del target[key]
    elif mutation.get("reverse"):
        target[key].reverse()
    elif "append" in mutation:
        target[key].append(mutation["append"])
    elif "append_repeat" in mutation:
        target[key].extend(deepcopy(mutation["append_repeat"]) for _ in range(mutation["count"]))
    elif "append_year_rows" in mutation:
        target[key].extend({"year": year, "overrides": []} for year in range(1, mutation["append_year_rows"] + 1))
    else:
        target[key] = mutation["value"]


def _apply_mutation(document, mutation):
    if "mutations" in mutation:
        for nested in mutation["mutations"]:
            _mutate(document, nested)
    else:
        _mutate(document, mutation)


def test_complete_closed_world_and_annotation_shapes():
    data = yaml.safe_load(VECTOR.read_text())
    assert validate_document(data)
    assert len(data["valid"]["nonworld"]["chronology"]) == 7


def test_x_extensions_are_accepted_without_changing_closed_meaning():
    data = yaml.safe_load(VECTOR.read_text())
    document = deepcopy(data)
    for mutation in data["positive_document_mutations"]:
        _mutate(document, mutation)
    assert validate_document(document)
    document = deepcopy(data)
    for mutation in data["positive_effective_order_mutations"]:
        _mutate(document, mutation)
    assert validate_document(document)
    for fixture in ("positive_effective_epoch_mutations", "positive_table_delta_epoch_mutations"):
        document = deepcopy(data)
        for mutation in data[fixture]:
            _mutate(document, mutation)
        assert validate_document(document)
    document = deepcopy(data)
    for mutation in data["positive_annotation_order_mutations"]:
        _mutate(document, mutation)
    assert validate_document(document)


def test_documented_contract_matches_the_pure_closed_oracle():
    text = DOC.read_text()
    for phrase in (
        "Only `kind: world`", "a list, not the world declaration map", "forbidden under effects",
        "signed i64", "v0.3 and v0.5 remain unchanged", "v0.4 remains quarantined",
        "internal typed reads", "truth/canon, causality, horizon, auth,",
        "search rank/corpus/vector", "no canonical winner", "order:i32",
        "no month/year addition", "Different StoryTime at one day",
        "zero-skipping display ordinal", "checked signed-i64 subtraction and addition",
        "Its complete interval must lie inside", "the era bounds; a partial", "are ordered lower through upper",
    ):
        assert phrase in text


def test_version_classification_is_distinct_from_reserved_v06_validation():
    def classify_schema_version(schema):
        return {"wedl/v0.3": "unchanged", "wedl/v0.5": "unchanged", "wedl/v0.4": "v04_superseded", "wedl/v0.6": "internal_read_active"}[schema]

    assert [classify_schema_version(schema) for schema in ("wedl/v0.3", "wedl/v0.5", "wedl/v0.4", "wedl/v0.6")] == ["unchanged", "unchanged", "v04_superseded", "internal_read_active"]


def test_new_generated_chronology_ids_are_uppercase_while_existing_ids_stay_valid():
    _generated_id("calendar_0123456789ABCDEFGHJKMNPQRS", "calendar_")
    _generated_id("era_0123456789ABCDEFGHJKMNPQRS", "era_")
    _generated_id("chronology_0123456789ABCDEFGHJKMNPQRS", "chronology_")
    _id("calendar_0123456789abcdefghjkmnpqrs", "calendar_")


def test_zero_skipping_era_display_ordinals_are_checked_and_affine():
    data = yaml.safe_load(VECTOR.read_text())
    eras = {era["id"]: era for era in data["valid"]["world"]["chronology"]["eras"]}
    cases = data["display_ordinal_cases"]
    for name in ("zero_skipping", "zero_enabled"):
        era = eras[cases[name]["era_id"]]
        for display_year, machine_year in cases[name]["display_to_machine"].items():
            assert _era_machine_year(era, display_year) == machine_year
    try:
        _era_machine_year(eras[cases["zero_skipping"]["era_id"]], cases["zero_skipping"]["reject"])
    except AssertionError:
        pass
    else:
        raise AssertionError("zero-skipping era accepted display year zero")
    for operation, operands in cases["overflow"].items():
        try:
            (_checked_sub if operation == "subtraction" else _checked_add)(operands["left"], operands["right"])
        except AssertionError:
            pass
        else:
            raise AssertionError(operands)


def test_effective_calendar_civil_intervals_order_partial_bounds():
    data = yaml.safe_load(VECTOR.read_text())
    solar = data["valid"]["world"]["chronology"]["calendars"][0]
    february = _civil_interval({"year": 0, "month": 2}, solar)
    assert february == ((0, 1, 1), (0, 1, 29))
    assert _ordered_intervals(_civil_interval({"year": 0}, solar), _civil_interval({"year": 1}, solar))
    assert not _ordered_intervals(_civil_interval({"year": 2}, solar), _civil_interval({"year": 1}, solar))


def test_anchor_aliases_coalesce_and_positive_boundaries_are_valid():
    data = yaml.safe_load(VECTOR.read_text())
    document = deepcopy(data)
    document["valid"]["world"]["chronology"]["anchors"] = data["positive_anchor_alias"]["anchors"]
    result = validate_document(document)
    assert result["anchor_provenance"][(0, ("main", 0, 0))] == ["a", "b"]
    for fixture in ("positive_anchor_boundaries", "positive_anchor_id_order_not_axis_order"):
        document = deepcopy(data)
        document["valid"]["world"]["chronology"]["anchors"] = data[fixture]["anchors"]
        assert validate_document(document)


def test_complete_negative_documents_use_the_same_validator():
    data = yaml.safe_load(VECTOR.read_text())
    assert len(data["negative_documents"]) >= 40
    for mutation in data["negative_documents"]:
        document = deepcopy(data)
        _apply_mutation(document, mutation)
        try:
            validate_document(document)
        except (AssertionError, KeyError):
            pass
        else:
            raise AssertionError(mutation["category"])
