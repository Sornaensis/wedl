"""Deterministic validation for the authored v0.7 generational records.

The validator is intentionally source-only: diagnostics describe malformed
facts and references but never manufacture a relationship, holder, or vital
state.  Every branch is total over YAML-shaped values.
"""
from __future__ import annotations

from typing import Any

from .generational import GENERATIONAL_KINDS, TRANSITIONS, literal_payload_ok
from .ids import valid_id
from .model import Record, StoryTime, World
from .source import KIND_DIR
from .v07 import canonical_capabilities


_PREFIX = {"organization": "organization", "parentage": "kinship", "union": "union", "affiliation": "affiliation", "legacy": "legacy", "tenure": "tenure", "claim": "claim", "vital-history": "vital"}
_TERMINAL = {"organization-dissolve", "parentage-end", "union-end", "union-annul", "affiliation-end", "legacy-dissolve", "tenure-end", "claim-withdraw", "claim-reject", "vital-death", "vital-existence-end"}
_COMMON = {"schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "initialization", "transitions"}
_SPECIFIC = {
    "organization": {"organization_kind", "parent_id", "location_id"}, "parentage": {"child_id", "parent_id"}, "union": {"participant_ids"},
    "affiliation": {"character_id", "organization_id"}, "legacy": {"legacy_kind", "organization_id"},
    "tenure": {"legacy_id", "predecessor_tenure_id", "successor_tenure_id"}, "claim": {"legacy_id", "claimant_id"}, "vital-history": {"character_id", "disclosure"},
}
_OPTIONAL_SPECIFIC = {
    "organization": {"parent_id", "location_id"}, "legacy": {"organization_id"},
    "tenure": {"predecessor_tenure_id", "successor_tenure_id"},
}


def _diag(code: str, record: Record | None, field: str, message: str) -> dict[str, Any]:
    return {"code": code, "message": message, "severity": "error", "entityId": record.id if record else None, "path": record.source_path if record else None, "field": field}


def _closed(data: Any, required: set[str], optional: set[str] = set()) -> str | None:
    if not isinstance(data, dict): return ""
    if not all(isinstance(key, str) for key in data): return "<key>"
    missing = sorted(required - set(data))
    extra = sorted(set(data) - required - optional)
    return missing[0] if missing else (extra[0] if extra else None)


def _ordered_strings(value: Any, *, nonempty: bool = False) -> bool:
    if not isinstance(value, list) or (nonempty and not value) or not all(isinstance(item, str) and item.strip() for item in value):
        return False
    return value == sorted(value) and len(value) == len(set(value))


def _point(value: Any, timelines: set[str]) -> StoryTime | None:
    if not isinstance(value, dict) or set(value) != {"timeline", "tick", "order"}: return None
    try: point = StoryTime.from_value(value)
    except (TypeError, ValueError, KeyError): return None
    return point if point.timeline in timelines else None


def _app(value: Any, timelines: set[str]) -> tuple[str, StoryTime | None, StoryTime | None] | None:
    if not isinstance(value, dict): return None
    kind = value.get("applicability_kind")
    if kind == "static" and set(value) == {"applicability_kind"}: return kind, None, None
    if kind == "instant" and set(value) == {"applicability_kind", "point"}:
        point = _point(value.get("point"), timelines); return (kind, point, point) if point else None
    if kind == "inclusive-interval" and set(value) == {"applicability_kind", "first", "last"}:
        first, last = _point(value.get("first"), timelines), _point(value.get("last"), timelines)
        return (kind, first, last) if first and last and first.not_after(last) else None
    return None


def _app_leaf(value: Any, timelines: set[str]) -> str:
    """Return the first exact malformed applicability leaf without raising."""
    if not isinstance(value, dict): return "applicability"
    kind = value.get("applicability_kind")
    if not isinstance(kind, str): return "applicability.applicability_kind"
    expected = {"static": {"applicability_kind"}, "instant": {"applicability_kind", "point"}, "inclusive-interval": {"applicability_kind", "first", "last"}}.get(kind)
    if expected is None: return "applicability.applicability_kind"
    missing = sorted(expected - set(value))
    extra = sorted(key for key in set(value) - expected if isinstance(key, str))
    if missing: return f"applicability.{missing[0]}"
    if extra or any(not isinstance(key, str) for key in value): return f"applicability.{extra[0] if extra else '<key>'}"
    if kind == "instant" and _point(value.get("point"), timelines) is None: return "applicability.point"
    if kind == "inclusive-interval":
        if _point(value.get("first"), timelines) is None: return "applicability.first"
        if _point(value.get("last"), timelines) is None: return "applicability.last"
    return "applicability"


def _transitions(value: Any) -> list[Any]:
    """Only actual transition arrays are iterable in validation paths."""
    return value if isinstance(value, list) else []


def _same_app(left: Any, right: Any) -> bool:
    return left == right


def _record_ref(world: World, identifier: Any, kind: str) -> bool:
    record = world.maybe_get(identifier if isinstance(identifier, str) else None)
    return record is not None and record.kind == kind and record.status == "canonical"


def _payload_ok(kind: str, payload: Any, world: World) -> bool:
    try:
        if not literal_payload_ok(kind, payload): return False
    except (TypeError, ValueError):
        return False
    if not isinstance(payload, dict) or not all(isinstance(key, str) for key in payload): return False
    empty = {"organization-dormant", "organization-dissolve", "parentage-end", "union-end", "union-annul", "affiliation-end", "legacy-dormant", "legacy-dissolve", "tenure-end", "claim-recognize", "claim-withdraw", "claim-reject", "vital-initialize", "vital-birth", "vital-death", "vital-existence-start", "vital-existence-end"}
    rename = {"organization-initialize", "organization-rename", "legacy-initialize", "legacy-rename"}
    if kind in empty: return payload == {}
    if kind in rename: return set(payload) == {"title", "aliases"} and isinstance(payload["title"], str) and bool(payload["title"].strip()) and _ordered_strings(payload["aliases"])
    if kind == "organization-reparent": return set(payload) == {"parent_id"} and (payload["parent_id"] is None or _record_ref(world, payload["parent_id"], "organization"))
    if kind in {"parentage-initialize", "parentage-confirm"}: return set(payload) == {"basis"} and isinstance(payload["basis"], str) and payload["basis"] in {"biological", "adoptive"}
    if kind in {"union-initialize", "union-form", "union-reconcile"}:
        value = payload.get("participant_ids")
        return set(payload) == {"participant_ids"} and isinstance(value, list) and all(isinstance(item, str) for item in value) and len(value) >= 2 and value == sorted(value) and len(value) == len(set(value)) and all(_record_ref(world, item, "character") for item in value)
    if kind == "affiliation-initialize": return set(payload) == {"role"} and (payload["role"] is None or isinstance(payload["role"], str))
    if kind == "affiliation-role": return set(payload) == {"role"} and isinstance(payload["role"], str) and bool(payload["role"].strip())
    if kind in {"tenure-initialize", "tenure-designate", "tenure-hold"}:
        holder = payload.get("holder_id")
        basis = payload.get("basis")
        return set(payload) == {"holder_id", "basis"} and isinstance(basis, str) and basis in {"legal", "de-facto"} and ((holder is None and kind == "tenure-initialize") or _record_ref(world, holder, "character"))
    if kind == "tenure-vacate": return payload == {"holder_id": None}
    if kind == "tenure-transfer": return set(payload) == {"from_tenure_id", "to_tenure_id"} and all(_record_ref(world, payload.get(key), "tenure") for key in payload) and payload["from_tenure_id"] != payload["to_tenure_id"]
    if kind in {"claim-initialize", "claim-dispute"}:
        values = payload.get("competes_with")
        return set(payload) == {"competes_with"} and _ordered_strings(values) and all(_record_ref(world, item, "claim") for item in values)
    return False


def _payload_leaf(kind: Any, payload: Any, world: World | None = None) -> str:
    if not isinstance(payload, dict): return "payload"
    if not isinstance(kind, str): return "payload"
    required = {
        "organization-initialize": {"title", "aliases"}, "organization-rename": {"title", "aliases"}, "organization-reparent": {"parent_id"}, "parentage-initialize": {"basis"}, "parentage-confirm": {"basis"},
        "union-initialize": {"participant_ids"}, "union-form": {"participant_ids"}, "union-reconcile": {"participant_ids"}, "affiliation-initialize": {"role"}, "affiliation-role": {"role"},
        "tenure-initialize": {"holder_id", "basis"}, "tenure-designate": {"holder_id", "basis"}, "tenure-hold": {"holder_id", "basis"}, "tenure-vacate": {"holder_id"}, "tenure-transfer": {"from_tenure_id", "to_tenure_id"},
        "claim-initialize": {"competes_with"}, "claim-dispute": {"competes_with"},
    }.get(kind, set())
    if required is None: return "payload"
    missing = sorted(required - set(payload)); extra = sorted(key for key in set(payload) - required if isinstance(key, str))
    if missing: return f"payload.{missing[0]}"
    if extra or any(not isinstance(key, str) for key in payload): return f"payload.{extra[0] if extra else '<key>'}"
    for key in sorted(required):
        value = payload.get(key)
        if key == "aliases" and not _ordered_strings(value): return "payload.aliases"
        if key == "participant_ids" and (not isinstance(value, list) or len(value) < 2 or not _ordered_strings(value) or (world is not None and not all(_record_ref(world, item, "character") for item in value))): return "payload.participant_ids"
        if key == "competes_with" and (not _ordered_strings(value) or (world is not None and not all(_record_ref(world, item, "claim") for item in value))): return "payload.competes_with"
        if key == "title" and (not isinstance(value, str) or not value.strip()): return "payload.title"
        if key == "parent_id" and (value is not None and (not valid_id(value, "organization") or (world is not None and not _record_ref(world, value, "organization")))): return "payload.parent_id"
        if key == "role" and ((kind == "affiliation-role" and (not isinstance(value, str) or not value.strip())) or (kind == "affiliation-initialize" and value is not None and not isinstance(value, str))): return "payload.role"
        if key == "holder_id" and ((kind in {"tenure-designate", "tenure-hold"} and (not valid_id(value, "character") or (world is not None and not _record_ref(world, value, "character")))) or (kind == "tenure-initialize" and value is not None and (not valid_id(value, "character") or (world is not None and not _record_ref(world, value, "character"))) ) or (kind == "tenure-vacate" and value is not None)): return "payload.holder_id"
        if key in {"from_tenure_id", "to_tenure_id"} and (not valid_id(value, "tenure") or (world is not None and not _record_ref(world, value, "tenure"))): return f"payload.{key}"
        if key == "basis" and (not isinstance(value, str) or value not in {"biological", "adoptive", "legal", "de-facto"}): return f"payload.basis"
    return "payload"


# Closed state/from/to table from the normative v0.7 schema vector.  The
# validator owns this table rather than inferring lifecycle from English names.
_STATE_RULES: dict[str, dict[str, tuple[frozenset[str], str]]] = {
    "organization": {"organization-rename": (frozenset({"active", "dormant"}), "same"), "organization-reparent": (frozenset({"active", "dormant"}), "same"), "organization-dormant": (frozenset({"active"}), "dormant"), "organization-dissolve": (frozenset({"active", "dormant"}), "dissolved")},
    "parentage": {"parentage-confirm": (frozenset({"asserted"}), "confirmed"), "parentage-end": (frozenset({"asserted", "confirmed"}), "ended")},
    "union": {"union-form": (frozenset({"declared"}), "formed"), "union-reconcile": (frozenset({"formed"}), "formed"), "union-end": (frozenset({"declared", "formed"}), "ended"), "union-annul": (frozenset({"declared", "formed"}), "annulled")},
    "affiliation": {"affiliation-role": (frozenset({"active"}), "active"), "affiliation-end": (frozenset({"active"}), "ended")},
    "legacy": {"legacy-rename": (frozenset({"active", "dormant"}), "same"), "legacy-dormant": (frozenset({"active"}), "dormant"), "legacy-dissolve": (frozenset({"active", "dormant"}), "dissolved")},
    "tenure": {"tenure-designate": (frozenset({"vacant"}), "designated"), "tenure-hold": (frozenset({"vacant", "designated"}), "holding"), "tenure-vacate": (frozenset({"designated", "holding"}), "vacant-interval"), "tenure-transfer": (frozenset({"holding"}), "vacant"), "tenure-end": (frozenset({"vacant", "designated", "holding"}), "ended")},
    "claim": {"claim-dispute": (frozenset({"proposed", "disputed"}), "disputed"), "claim-recognize": (frozenset({"proposed", "disputed"}), "recognized"), "claim-withdraw": (frozenset({"proposed", "disputed"}), "withdrawn"), "claim-reject": (frozenset({"proposed", "disputed"}), "rejected")},
    "vital-history": {"vital-birth": (frozenset({"unknown"}), "living"), "vital-death": (frozenset({"living"}), "dead"), "vital-existence-start": (frozenset({"unknown"}), "existing"), "vital-existence-end": (frozenset({"existing"}), "ended")},
}


def _initial_state(kind: str, payload: Any) -> str:
    if kind == "organization": return "active"
    if kind == "parentage": return "asserted"
    if kind == "union": return "declared"
    if kind == "affiliation": return "active"
    if kind == "legacy": return "active"
    if kind == "tenure": return "vacant" if isinstance(payload, dict) and payload.get("holder_id") is None else "holding"
    if kind == "claim": return "proposed"
    return "unknown"


def _time_key(point: StoryTime) -> tuple[str, int, int]:
    return point.timeline, point.tick, point.order


def _transition_errors(world: World, record: Record, timelines: set[str]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []; data = record.frontmatter; kind = record.kind
    initialization, transitions = data.get("initialization"), data.get("transitions")
    if not isinstance(transitions, list): return [_diag("GEN-TRANSITION-001", record, "transitions", "transitions must be an array")]
    chain = [("initialization", initialization, True)] + [(f"transitions[{index}]", item, False) for index, item in enumerate(transitions)]
    seen: dict[str, tuple[Any, str, str | None, bool]] = {}; previous: StoryTime | None = None; terminal = False; state: str | None = None
    for field, item, initial in chain:
        if not isinstance(item, dict): result.append(_diag("GEN-TRANSITION-001", record, field, "transition must be a mapping")); continue
        allowed = {"transition_id", "transition_kind", "applicability", "payload"} | (set() if initial else {"cause_event_id", "replaces_transition_id"})
        leaf = _closed(item, {"transition_id", "transition_kind", "applicability", "payload"}, allowed - {"transition_id", "transition_kind", "applicability", "payload"})
        if leaf is not None: result.append(_diag("GEN-TRANSITION-001", record, f"{field}.{leaf}" if leaf else field, "transition has invalid fields")); continue
        identifier, transition_kind = item.get("transition_id"), item.get("transition_kind")
        if not valid_id(identifier, "generational-transition") or identifier in seen: result.append(_diag("GEN-TRANSITION-002", record, f"{field}.transition_id", "transition id must be unique and use transition_ prefix"))
        if not isinstance(transition_kind, str) or transition_kind not in TRANSITIONS[kind] or transition_kind.endswith("-initialize") != initial:
            result.append(_diag("GEN-TRANSITION-003", record, f"{field}.transition_kind", "transition kind is not applicable to this record"))
        app = _app(item.get("applicability"), timelines)
        if app is None or (initial and app[0] != "instant") or (not initial and transition_kind == "tenure-vacate" and (app is None or app[0] != "inclusive-interval")) or (not initial and transition_kind != "tenure-vacate" and app is not None and app[0] != "instant"):
            result.append(_diag("GEN-APPLICABILITY-001", record, f"{field}.{_app_leaf(item.get('applicability'), timelines)}", "invalid closed applicability for transition"))
        elif app[1] is not None:
            if previous is not None and (app[1].timeline != previous.timeline or _time_key(app[1]) < _time_key(previous)):
                result.append(_diag("GEN-TRANSITION-004", record, f"{field}.applicability", "transition history must be monotonic on one timeline"))
            if previous is not None and _time_key(app[1]) == _time_key(previous):
                replacement_id = item.get("replaces_transition_id")
                target = seen.get(replacement_id) if isinstance(replacement_id, str) else None
                if target is None or not _same_app(target[0], item.get("applicability")):
                    result.append(_diag("GEN-REPLACEMENT-002", record, f"{field}.applicability", "equal applicability requires an explicit exact replacement"))
            previous = app[1]
        replacement = item.get("replaces_transition_id")
        target = seen.get(replacement) if isinstance(replacement, str) else None
        if target is not None and _same_app(target[0], item.get("applicability")) and replacement != identifier:
            # A replacement folds from exactly the target's pre-transition
            # state/terminal flag, not from the superseded transition's result.
            state, terminal = target[2], target[3]
        pre_state, pre_terminal = state, terminal
        if terminal: result.append(_diag("GEN-TRANSITION-005", record, field, "transition cannot follow a terminal transition"))
        if isinstance(transition_kind, str) and initial and transition_kind in TRANSITIONS[kind] and _payload_ok(transition_kind, item.get("payload"), world):
            state = _initial_state(kind, item.get("payload"))
        elif isinstance(transition_kind, str) and transition_kind in _STATE_RULES.get(kind, {}):
            allowed_states, output = _STATE_RULES[kind][transition_kind]
            if state not in allowed_states:
                result.append(_diag("GEN-STATE-001", record, f"{field}.transition_kind", "transition is not allowed from the current closed state"))
            else:
                state = state if output == "same" else output
        if isinstance(transition_kind, str) and transition_kind in _TERMINAL: terminal = True
        if not _payload_ok(str(transition_kind), item.get("payload"), world): result.append(_diag("GEN-PAYLOAD-001", record, f"{field}.{_payload_leaf(transition_kind, item.get('payload'), world)}", "transition payload is not the exact declared shape"))
        cause = item.get("cause_event_id")
        if "cause_event_id" in item:
            event = world.maybe_get(cause if isinstance(cause, str) else None); event_point = _point(event.frontmatter.get("time"), timelines) if event else None
            if event is None or event.kind != "event" or event.status != "canonical" or not valid_id(cause, "event") or event_point is None or app is None or app[1] is None or event_point.timeline != app[1].timeline or not (event_point.tick, event_point.order) < (app[1].tick, app[1].order): result.append(_diag("GEN-CAUSE-001", record, f"{field}.cause_event_id", "cause must be an earlier canonical event on the transition timeline"))
        if transition_kind == "tenure-transfer" and "cause_event_id" not in item:
            result.append(_diag("GEN-TENURE-TRANSFER-003", record, f"{field}.cause_event_id", "tenure transfer requires an earlier canonical event cause"))
        if "replaces_transition_id" in item:
            if replacement == identifier or target is None or not _same_app(target[0], item.get("applicability")):
                result.append(_diag("GEN-REPLACEMENT-001", record, f"{field}.replaces_transition_id", "replacement must target another transition in this record at the exact same applicability"))
        if isinstance(identifier, str) and valid_id(identifier, "generational-transition") and identifier not in seen:
            seen[identifier] = (item.get("applicability"), field, pre_state, pre_terminal)
    return result


def _envelope_errors(world: World, record: Record) -> list[dict[str, Any]]:
    data, kind = record.frontmatter, record.kind; result: list[dict[str, Any]] = []
    optional = _OPTIONAL_SPECIFIC.get(kind, set())
    leaf = _closed(data, _COMMON | (_SPECIFIC[kind] - optional), optional)
    if leaf is not None: result.append(_diag("GEN-ENVELOPE-001", record, leaf or "frontmatter", "generational record has missing or unknown fields"))
    if data.get("schema") != "wedl/v0.7": result.append(_diag("GEN-ENVELOPE-002", record, "schema", "generational record schema must be wedl/v0.7"))
    if not valid_id(data.get("id"), kind): result.append(_diag("GEN-ID-001", record, "id", "record identifier must use the canonical kind prefix"))
    expected_path = f"{world.source_root}/{KIND_DIR[kind]}/{record.id}.md"
    if record.source_path != expected_path: result.append(_diag("GEN-PATH-001", record, "source_path", "generational record must use its canonical kind directory path"))
    status = data.get("status")
    if not isinstance(status, str) or status not in {"canonical", "draft", "retired"}: result.append(_diag("GEN-ENVELOPE-003", record, "status", "invalid generational source status"))
    domain = data.get("domain")
    if not isinstance(domain, str) or "." not in domain or not all(part and part.replace("-", "").isalnum() for part in domain.split(".")): result.append(_diag("GEN-ENVELOPE-005", record, "domain", "domain must be a non-empty dotted identifier"))
    for key in ("audience", "perspectives"):
        if not _ordered_strings(data.get(key), nonempty=True): result.append(_diag("GEN-ENVELOPE-004", record, key, f"{key} must be a non-empty sorted unique string list"))
    for key in ("tags", "aliases", "threads"):
        if not _ordered_strings(data.get(key)): result.append(_diag("GEN-ENVELOPE-004", record, key, f"{key} must be a sorted unique string list"))
    if kind == "organization":
        if not isinstance(data.get("organization_kind"), str) or data.get("organization_kind") not in {"house", "dynasty", "clan", "institution", "other"}: result.append(_diag("GEN-ORGANIZATION-001", record, "organization_kind", "invalid organization kind"))
        if data.get("parent_id") is not None and not _record_ref(world, data.get("parent_id"), "organization"): result.append(_diag("GEN-REF-001", record, "parent_id", "organization parent must reference a canonical organization"))
        if data.get("location_id") is not None and not _record_ref(world, data.get("location_id"), "location"): result.append(_diag("GEN-REF-001", record, "location_id", "organization location must reference a canonical location"))
    elif kind == "parentage":
        for key in ("child_id", "parent_id"):
            if not _record_ref(world, data.get(key), "character"): result.append(_diag("GEN-REF-001", record, key, "parentage endpoint must reference a canonical character"))
        if data.get("child_id") == data.get("parent_id"): result.append(_diag("GEN-PARENTAGE-001", record, "parent_id", "parentage cannot reference itself"))
    elif kind == "union":
        values = data.get("participant_ids")
        if not isinstance(values, list) or not all(isinstance(value, str) for value in values) or len(values) < 2 or values != sorted(values) or len(values) != len(set(values)) or not all(_record_ref(world, item, "character") for item in values): result.append(_diag("GEN-UNION-001", record, "participant_ids", "union participants must be sorted, unique canonical characters with cardinality at least two"))
    elif kind == "affiliation":
        for key, target in (("character_id", "character"), ("organization_id", "organization")):
            if not _record_ref(world, data.get(key), target): result.append(_diag("GEN-REF-001", record, key, "affiliation endpoint has invalid kind"))
    elif kind == "legacy":
        if not isinstance(data.get("legacy_kind"), str) or data.get("legacy_kind") not in {"office", "estate", "title", "other"}: result.append(_diag("GEN-LEGACY-001", record, "legacy_kind", "invalid legacy kind"))
        if data.get("organization_id") is not None and not _record_ref(world, data.get("organization_id"), "organization"): result.append(_diag("GEN-REF-001", record, "organization_id", "legacy organization must reference an organization"))
    elif kind == "tenure":
        if not _record_ref(world, data.get("legacy_id"), "legacy"): result.append(_diag("GEN-REF-001", record, "legacy_id", "tenure must reference a canonical legacy"))
        for key in ("predecessor_tenure_id", "successor_tenure_id"):
            if data.get(key) is not None and (data.get(key) == record.id or not _record_ref(world, data.get(key), "tenure")): result.append(_diag("GEN-REF-001", record, key, "tenure link must reference another canonical tenure"))
    elif kind == "claim":
        if not _record_ref(world, data.get("legacy_id"), "legacy"): result.append(_diag("GEN-REF-001", record, "legacy_id", "claim must reference a canonical legacy"))
        if not _record_ref(world, data.get("claimant_id"), "character"): result.append(_diag("GEN-REF-001", record, "claimant_id", "claimant must reference a canonical character"))
    elif kind == "vital-history":
        if not _record_ref(world, data.get("character_id"), "character"): result.append(_diag("GEN-REF-001", record, "character_id", "vital history must reference a canonical character"))
        if not isinstance(data.get("disclosure"), str) or data.get("disclosure") not in {"known", "unknown", "withheld"}: result.append(_diag("GEN-VITAL-001", record, "disclosure", "invalid vital disclosure"))
    return result


def _cycles(world: World, records: list[Record]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for kind, child, parent, code in (("organization", "id", "parent_id", "GEN-ORGANIZATION-002"), ("parentage", "child_id", "parent_id", "GEN-PARENTAGE-002")):
        graph: dict[str, list[tuple[str, Record]]] = {}
        for record in records:
            if record.kind != kind or record.status != "canonical": continue
            source, target = record.id if child == "id" else record.frontmatter.get(child), record.frontmatter.get(parent)
            if isinstance(source, str) and isinstance(target, str): graph.setdefault(source, []).append((target, record))
        visiting: set[str] = set(); visited: set[str] = set()
        def visit(source: str) -> None:
            visiting.add(source)
            for target, origin in graph.get(source, []):
                if target in visiting:
                    result.append(_diag(code, origin, parent, "canonical directed graph contains a cycle"))
                elif target not in visited:
                    visit(target)
            visiting.remove(source); visited.add(source)
        for source in sorted(graph):
            if source not in visited: visit(source)
    return result


def _start(record: Record, timelines: set[str]) -> StoryTime | None:
    init = record.frontmatter.get("initialization")
    return _app(init.get("applicability"), timelines)[1] if isinstance(init, dict) and _app(init.get("applicability"), timelines) else None


def _effective_history(record: Record) -> list[tuple[str, dict[str, Any]]]:
    """Return literal history excluding entries superseded by valid raw links.

    Field labels retain source indices so later diagnostics never point at a
    compacted/effective ordinal instead of the authored leaf.
    """
    values: list[tuple[str, dict[str, Any]]] = []
    init = record.frontmatter.get("initialization")
    if isinstance(init, dict): values.append(("initialization", init))
    values.extend((f"transitions[{index}]", item) for index, item in enumerate(_transitions(record.frontmatter.get("transitions"))) if isinstance(item, dict))
    by_id = {item.get("transition_id"): (field, item) for field, item in values if isinstance(item.get("transition_id"), str)}
    superseded: set[str] = set()
    for _, item in values:
        target_id = item.get("replaces_transition_id")
        target = by_id.get(target_id) if isinstance(target_id, str) else None
        if target is not None and target_id != item.get("transition_id") and _same_app(target[1].get("applicability"), item.get("applicability")):
            superseded.add(target_id)
    return [(field, item) for field, item in values if item.get("transition_id") not in superseded]


def _tenure_state_before(record: Record, point: StoryTime, timelines: set[str]) -> str | None:
    """Return only an authored tenure state strictly before a transition point."""
    init = record.frontmatter.get("initialization")
    if not isinstance(init, dict): return None
    init_app = _app(init.get("applicability"), timelines)
    if init_app is None or init_app[1] is None or init_app[1].timeline != point.timeline or (init_app[1].tick, init_app[1].order) >= (point.tick, point.order): return None
    state = _initial_state("tenure", init.get("payload"))
    for _, item in _effective_history(record):
        if item is init: continue
        app = _app(item.get("applicability"), timelines)
        transition_kind = item.get("transition_kind")
        if app is None or app[1] is None or app[1].timeline != point.timeline or (app[1].tick, app[1].order) >= (point.tick, point.order) or transition_kind not in _STATE_RULES["tenure"]: continue
        if transition_kind == "tenure-vacate":
            assert app[2] is not None
            if (point.tick, point.order) > (app[2].tick, app[2].order):
                state = "expired"
            else:
                state = "vacant-interval"
            continue
        allowed, output = _STATE_RULES["tenure"][transition_kind]
        if state in allowed: state = state if output == "same" else output
    return state


def _affiliation_interval(record: Record, timelines: set[str]) -> tuple[StoryTime, StoryTime | None] | None:
    start = _start(record, timelines)
    if start is None: return None
    end: StoryTime | None = None
    for _, item in _effective_history(record):
        if item.get("transition_kind") == "affiliation-end":
            app = _app(item.get("applicability"), timelines)
            if app and app[1]: end = app[1]
    return start, end


def _intervals_overlap(left: tuple[StoryTime, StoryTime | None], right: tuple[StoryTime, StoryTime | None]) -> bool:
    left_start, left_end = left; right_start, right_end = right
    if left_start.timeline != right_start.timeline: return False
    return (right_end is None or _time_key(left_start) <= _time_key(right_end)) and (left_end is None or _time_key(right_start) <= _time_key(left_end))


def _cross_record_errors(world: World, records: list[Record]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []; parentage: set[tuple[Any, ...]] = set(); affiliations: list[Record] = []
    for record in records:
        if record.kind == "parentage" and record.status == "canonical":
            initial = record.frontmatter.get("initialization")
            basis = initial.get("payload", {}).get("basis") if isinstance(initial, dict) and isinstance(initial.get("payload"), dict) else None
            app = _app(initial.get("applicability"), world.timeline_ids) if isinstance(initial, dict) else None
            if app is None:
                # The transition pass owns the exact malformed applicability
                # diagnostic; cross-record duplicate normalization must remain
                # total and must not invent a second error from a bad leaf.
                continue
            app_key = (app[0], _time_key(app[1]) if app and app[1] else None, _time_key(app[2]) if app and app[2] else None)
            edge = (record.frontmatter.get("child_id"), record.frontmatter.get("parent_id"), basis, app_key)
            try:
                duplicate = edge in parentage
                parentage.add(edge)
            except TypeError:
                duplicate = False
            if duplicate: result.append(_diag("GEN-PARENTAGE-003", record, "initialization", "duplicate canonical parentage edge"))
        if record.kind == "claim":
            for field, item in [("initialization", record.frontmatter.get("initialization"))] + [(f"transitions[{i}]", value) for i, value in enumerate(_transitions(record.frontmatter.get("transitions")))]:
                payload = item.get("payload") if isinstance(item, dict) else None
                if not isinstance(payload, dict) or "competes_with" not in payload: continue
                competitors = payload.get("competes_with")
                if not isinstance(competitors, list): continue
                for index, target_id in enumerate(competitors):
                    target = world.maybe_get(target_id if isinstance(target_id, str) else None)
                    reciprocal = False
                    if target and target.kind == "claim" and target.frontmatter.get("legacy_id") == record.frontmatter.get("legacy_id"):
                        for _, candidate in _effective_history(target):
                            candidate_payload = candidate.get("payload")
                            if isinstance(candidate_payload, dict) and isinstance(candidate_payload.get("competes_with"), list) and candidate.get("applicability") == item.get("applicability") and record.id in candidate_payload["competes_with"]: reciprocal = True
                    if target_id == record.id or not reciprocal: result.append(_diag("GEN-CLAIM-001", record, f"{field}.payload.competes_with[{index}]", "competing claims must reciprocally cite the same legacy and applicability"))
        if record.kind == "affiliation" and record.status == "canonical":
            affiliations.append(record)
        if record.kind == "tenure" and record.status == "canonical":
            source_start = _start(record, world.timeline_ids)
            for field, direction in (("predecessor_tenure_id", "predecessor"), ("successor_tenure_id", "successor")):
                target_id = record.frontmatter.get(field)
                target = world.maybe_get(target_id if isinstance(target_id, str) else None)
                if target is None: continue
                target_start = _start(target, world.timeline_ids)
                compatible = target.kind == "tenure" and target.status == "canonical" and target.frontmatter.get("legacy_id") == record.frontmatter.get("legacy_id")
                ordered = source_start is not None and target_start is not None and source_start.timeline == target_start.timeline and ((direction == "predecessor" and (target_start.tick, target_start.order) < (source_start.tick, source_start.order)) or (direction == "successor" and (source_start.tick, source_start.order) < (target_start.tick, target_start.order)))
                if not compatible or not ordered:
                    result.append(_diag("GEN-TENURE-LINK-001", record, field, "tenure predecessor/successor must be compatible and strictly time-ordered"))
            for index, item in enumerate(record.frontmatter.get("transitions") if isinstance(record.frontmatter.get("transitions"), list) else []):
                if not isinstance(item, dict) or item.get("transition_kind") != "tenure-transfer": continue
                payload, app = item.get("payload"), _app(item.get("applicability"), world.timeline_ids)
                if not isinstance(payload, dict) or app is None or app[1] is None: continue
                if payload.get("from_tenure_id") != record.id:
                    result.append(_diag("GEN-TENURE-TRANSFER-001", record, f"transitions[{index}].payload.from_tenure_id", "tenure transfer source must be this holding tenure"))
                target = world.maybe_get(payload.get("to_tenure_id") if isinstance(payload.get("to_tenure_id"), str) else None)
                target_state = _tenure_state_before(target, app[1], world.timeline_ids) if target else None
                if target is None or target.kind != "tenure" or target.status != "canonical" or target.frontmatter.get("legacy_id") != record.frontmatter.get("legacy_id") or target_state not in {"vacant", "designated"}:
                    result.append(_diag("GEN-TENURE-TRANSFER-002", record, f"transitions[{index}].payload.to_tenure_id", "transfer target must be an extant vacant or designated tenure for the same legacy"))
    for index, record in enumerate(affiliations):
        interval = _affiliation_interval(record, world.timeline_ids)
        if interval is None: continue
        for other in affiliations[:index]:
            if record.frontmatter.get("character_id") != other.frontmatter.get("character_id") or record.frontmatter.get("organization_id") != other.frontmatter.get("organization_id"): continue
            other_interval = _affiliation_interval(other, world.timeline_ids)
            if other_interval and _intervals_overlap(interval, other_interval):
                result.append(_diag("GEN-AFFILIATION-001", record, "character_id", "duplicate affiliation histories cannot overlap"))
    return result


def validate_generational_world(world: World) -> list[dict[str, Any]]:
    """Validate generational source facts in a homogeneous v0.7 world."""
    if world.schema != "wedl/v0.7": return []
    result: list[dict[str, Any]] = []; records = list(world.records.values()); world_record = world.world_record
    caps = canonical_capabilities(world_record.frontmatter.get("capabilities"))
    if caps is None: result.append(_diag("GEN-CAPABILITY-001", world_record, "capabilities", "world capabilities must be the closed canonical v0.7 list"))
    for record in records:
        if record.kind == "world": continue
        if "capabilities" in record.frontmatter: result.append(_diag("GEN-CAPABILITY-001", record, "capabilities", "capabilities are permitted only on the world record"))
        if record.kind not in GENERATIONAL_KINDS: continue
        if caps is None or "generational-core-v1" not in caps: result.append(_diag("GEN-CAPABILITY-001", record, "schema", "generational records require generational-core-v1"))
        result.extend(_envelope_errors(world, record)); result.extend(_transition_errors(world, record, world.timeline_ids))
        if record.kind == "vital-history":
            values = [record.frontmatter.get("initialization"), *_transitions(record.frontmatter.get("transitions"))]
            kinds = [item.get("transition_kind") for item in values if isinstance(item, dict)]
            if record.frontmatter.get("disclosure") == "unknown" and any(kind in {"vital-birth", "vital-death", "vital-existence-start", "vital-existence-end"} for kind in kinds): result.append(_diag("GEN-VITAL-002", record, "transitions", "unknown vital history cannot contain affirmative transitions"))
            if "vital-birth" in kinds and "vital-death" in kinds:
                vital_apps = {item.get("transition_kind"): _app(item.get("applicability"), world.timeline_ids) for item in _transitions(record.frontmatter.get("transitions")) if isinstance(item, dict)}
                birth, death = vital_apps.get("vital-birth"), vital_apps.get("vital-death")
                if birth and death and birth[1] and death[1] and (birth[1].timeline != death[1].timeline or not birth[1].not_after(death[1])): result.append(_diag("GEN-VITAL-003", record, "transitions", "birth must not follow death"))
    transition_ids: dict[str, Record] = {}
    for record in records:
        if record.kind not in GENERATIONAL_KINDS: continue
        values = [record.frontmatter.get("initialization"), *(record.frontmatter.get("transitions") if isinstance(record.frontmatter.get("transitions"), list) else [])]
        for index, value in enumerate(values):
            identifier = value.get("transition_id") if isinstance(value, dict) else None
            if isinstance(identifier, str):
                if identifier in transition_ids:
                    result.append(_diag("GEN-TRANSITION-006", record, "initialization.transition_id" if index == 0 else f"transitions[{index - 1}].transition_id", "transition id must be globally unique"))
                else: transition_ids[identifier] = record
    result.extend(_cycles(world, records)); result.extend(_cross_record_errors(world, records))
    return sorted(result, key=lambda item: (str(item["path"]), str(item["entityId"]), str(item["field"]), item["code"]))
