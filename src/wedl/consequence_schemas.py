"""One executable consequence schema registry for discovery and OpenAPI.

Source validation, reference existence, temporal order, world invariants and
aggregate byte/item budgets remain runtime gates. JSON Schema describes their
closed JSON shapes; it does not claim to execute those relational constraints.
"""
from __future__ import annotations

from copy import deepcopy
import math
import re
from typing import Any

from . import COMPILED_SOURCE_SCHEMAS
from .errors import UsageError
from .event_consequences import AuthorScope
from .ids import AUX_PREFIX, KIND_PREFIX
from .model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, World
from .v07 import canonical_capabilities


def obj(properties: dict, required=None, *, opened=False) -> dict:
    return {"type": "object", "properties": properties, "required": list(properties if required is None else required),
            "additionalProperties": opened}


def ref(name: str) -> dict:
    return {"$ref": "#/$defs/" + name}


def array(item: dict, **bounds) -> dict:
    return {"type": "array", "items": item, **bounds}


def nullable(item: dict) -> dict:
    return {"anyOf": [item, {"type": "null"}]}


def enum(*values) -> dict:
    return {"enum": list(values)}


def identity(kind: str) -> dict:
    return {"type": "string", "pattern": "^" + KIND_PREFIX.get(kind, AUX_PREFIX.get(kind, kind)) + "_[0-9A-HJKMNP-TV-Z]{26}$"}


def typed_ref(*kinds) -> dict:
    return {"allOf": [ref("Ref")], "x-wedl-reference-kinds": list(kinds)}


def _decimal(maximum: int, magnitude: int) -> dict:
    # Reuse the existing exact signed-decimal schema codec, not a second bound.
    from .api_schemas import _bounded_decimal_string
    return _bounded_decimal_string(maximum, magnitude)


def definitions() -> dict[str, dict]:
    """Build fresh definitions; importing this module performs no repository I/O."""
    text = {"type": "string", "minLength": 1, "pattern": r"\S"}
    string, boolean, number = {"type": "string"}, {"type": "boolean"}, {"type": "number"}
    nonnegative = {"type": "integer", "minimum": 0}
    metrics = {"type": "object", "propertyNames": text, "additionalProperties": number}
    strings = array(string)
    d = {"Text": text, "Ref": {**text, "description": "Stable ID or exact eligible title/alias/slug; kinds and existence are runtime checked."},
         "Revision": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
         "Hash": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
         "Temp": {"type": "string", "pattern": r"^tmp:[\s\S]*\S[\s\S]*$"},
         "Value": {"anyOf": [{"type": value} for value in ("null", "boolean", "number", "string")] +
                   [array(ref("Value")), {"type": "object", "additionalProperties": ref("Value")}],
                   "description": "Finite JSON only; expected state values additionally use authored world type/reference hints."},
         "Time": obj({"timeline": text, "tick": _decimal(TICK_MAX, -TICK_MIN), "order": _decimal(ORDER_MAX, -ORDER_MIN)}),
         "SourceTime": obj({"timeline": text, "tick": {"type": "integer", "minimum": TICK_MIN, "maximum": TICK_MAX},
                             "order": {"type": "integer", "minimum": ORDER_MIN, "maximum": ORDER_MAX}}),
         "LegacyEventTime": obj({"timeline": text, "tick": {"type": "integer", "minimum": TICK_MIN, "maximum": TICK_MAX},
                                  "order": {"type": "integer", "minimum": ORDER_MIN, "maximum": ORDER_MAX}}, ["tick"], opened=True),
         "KnowledgeState": enum("accepted", "suspected", "rejected", "uncertain", "remembered", "forgotten"),
         "PlotState": enum("dormant", "active", "resolved", "failed", "cancelled"),
         "Policy": enum("required", "advisory"), "Limit": {"type": "integer", "minimum": 1, "maximum": 1000},
         "Reference": obj({"id": text, "kind": text, "title": string}),
         "Candidate": obj({"baseRevision": ref("Revision"), "requestHash": ref("Hash")})}
    for kind in ("effect", "knowledge-transition", "relationship-transition", "story-point-transition"):
        name = {"effect": "Effect", "knowledge-transition": "Knowledge", "relationship-transition": "Relationship", "story-point-transition": "Plot"}[kind]
        d[name + "Id"] = identity(kind)
        d[name + "InputId"] = {"anyOf": [identity(kind), ref("Temp")]}
    def effect(identifier, required, opened=False):
        common = {"id": identifier, "target": typed_ref("character", "object", "location", "environment"), "key": text}
        clear = obj({**common, "operation": {"const": "clear"}}, [*required, "target", "key", "operation"], opened=opened)
        if opened: clear["properties"]["value"] = ref("Value")  # Legacy clear+value is only a source warning.
        value = obj({**common, "operation": enum("set", "add-to-set", "remove-from-set"), "value": ref("Value")},
                    [*required, "target", "key", "operation", "value"], opened=opened)
        return {"oneOf": [clear, value]}
    d["Effect"] = effect(ref("EffectId"), ["id"])
    d["EffectInput"] = effect(ref("EffectInputId"), [])
    d["LegacyEffect"] = effect(ref("EffectId"), ["id"], True)
    from .api_schemas import _KNOWLEDGE_ASSERTION
    def source_times(value):
        if isinstance(value, dict):
            if value.get("type") == "object" and set(value.get("properties", {})) == {"timeline", "tick", "order"}:
                return ref("SourceTime")
            return {key: source_times(item) for key, item in value.items()}
        if isinstance(value, list): return [source_times(item) for item in value]
        return value
    d["GenealogyAssertion"] = source_times(deepcopy(_KNOWLEDGE_ASSERTION))
    legacy_claim = obj({"key": string, "statement": string}, [], opened=True)
    legacy_claim["not"] = {"required": ["genealogy"]}
    d["KnowledgeClaim"] = {"oneOf": [legacy_claim, obj({"key": string, "statement": string, "genealogy": ref("GenealogyAssertion")}, ["genealogy"])]}
    for family in ("Knowledge", "Relationship", "Plot"):
        common = {"id": ref(family + "InputId"), "time": ref("Time"), "causing_event": typed_ref("event"), "note": string}
        required = ["time", "causing_event"]
        if family == "Knowledge":
            common.update(state=ref("KnowledgeState"), confidence={"type": "number", "minimum": 0, "maximum": 1},
                          acquisition=text, source_entity=typed_ref())
            required.append("state")
        elif family == "Relationship":
            common.update(relationship_status=text, metrics=metrics, facets=array(text))
        else:
            common["state"] = ref("PlotState"); required.append("state")
        d[family + "AppendTransition"] = obj(common, required)
        initial = deepcopy(common); initial["time"] = ref("SourceTime")
        for field in ("causing_event", "source_entity", "acquisition", "relationship_status"):
            if field in initial: initial[field] = nullable(initial[field])
        d[family + "CreateTransition"] = obj(initial, ["time"] + (["state"] if family != "Relationship" else []), opened=True)
        persisted = deepcopy(initial); persisted["time"] = ref("Time"); persisted["id"] = nullable(ref(family + "Id")) if family == "Plot" else ref(family + "Id")
        d[family + "Transition"] = obj(persisted, ["time"] + (["id"] if family != "Plot" else []) + (["state"] if family != "Relationship" else []), opened=True)
    source_common = {"schema": enum(*sorted(COMPILED_SOURCE_SCHEMAS)), "title": text, "domain": text, "status": text, "tags": strings, "aliases": strings}
    for family, kind in (("Knowledge", "knowledge"), ("Relationship", "relationship")):
        fields = {**source_common, "kind": {"const": kind}, "id": identity(kind), "transitions": array(ref(family + "CreateTransition"))}
        fields.update({"knower": typed_ref("character"), "claim": ref("KnowledgeClaim")} if family == "Knowledge" else
                      {"from": typed_ref("character"), "to": typed_ref("character"), "relationship_kind": text})
        required = list(fields.keys() - {"id"})
        d[family + "CreateSource"] = obj(fields, sorted(required), opened=True)
        if family == "Knowledge":
            d[family + "CreateSource"]["allOf"] = [{"if": {"properties": {"claim": {"required": ["genealogy"]}}},
                                                     "then": {"properties": {"schema": {"const": "wedl/v0.7"}}}}]
            d[family + "CreateSource"]["x-wedl-genealogy-requires-capability"] = "generational-knowledge-v1"
        d[family + "Create"] = obj({"type": {"const": kind + ".create"}, "temporaryId": ref("Temp"),
                                    "value": obj({"frontmatter": ref(family + "CreateSource"), "bodyMarkdown": string})})
    event_fields = {"type": {"const": "event.create"}, "temporaryId": text, "id": identity("event"), "title": text,
                    "time": ref("LegacyEventTime"), "domain": string, "status": string, "tags": strings, "aliases": strings,
                    "section_audiences": {"type": "object"}, "location": nullable(typed_ref("location")),
                    "participants": array(obj({"character": typed_ref("character"), "role": string}, ["character"], opened=True)),
                    "causes": array(typed_ref("event")), "relatedStoryPoints": array(typed_ref("story-point")),
                    "related_story_points": array(typed_ref("story-point")), "effects": array(ref("EffectInput")), "bodyMarkdown": string}
    event_required = ["type", "temporaryId", "title", "time"]
    selector = {"type": "object", "required": ["effects"], "properties": {"effects": {"type": "array", "contains": {
        "type": "object", "anyOf": [{"not": {"required": ["id"]}}, {"required": ["id"], "properties": {"id": ref("Temp")}}]}}}}
    raw_typed = obj(event_fields, event_required, opened=True); raw_typed["allOf"] = [selector]
    legacy = obj({**event_fields, "effects": array(ref("LegacyEffect"))}, event_required, opened=True); legacy["not"] = selector
    d["RawAllocationEvent"] = raw_typed; d["RawLegacyEvent"] = legacy
    d["RawEventCreate"] = {"oneOf": [ref("RawAllocationEvent"), ref("RawLegacyEvent")]}
    d["BatchEventCreate"] = obj({**event_fields, "temporaryId": ref("Temp"), "time": ref("Time")}, event_required)
    d["BatchEventCreate"]["not"] = {"required": ["relatedStoryPoints", "related_story_points"]}
    operations = {"event.create": "RawEventCreate", "knowledge.create": "KnowledgeCreate", "relationship.create": "RelationshipCreate"}
    for family, kind, target in (("Knowledge", "knowledge", "knowledge"), ("Relationship", "relationship", "relationship"), ("Plot", "story-point", "storyPoint")):
        name = family + "Append"
        d[name] = obj({"type": {"const": kind + ".transition.append"}, target: typed_ref(kind), "transition": ref(family + "AppendTransition")})
        operations[kind + ".transition.append"] = name
    d["OutcomeLink"] = obj({"type": {"const": "outcome.link"}, "event": typed_ref("event"),
                            "storyPoints": array(typed_ref("story-point"), uniqueItems=True), "scenes": array(typed_ref("scene"), uniqueItems=True)})
    d["OutcomeLink"]["anyOf"] = [{"properties": {key: {"minItems": 1}}} for key in ("storyPoints", "scenes")]
    operations["outcome.link"] = "OutcomeLink"
    predicates = []
    variants = {
        "state.equals": {"target": typed_ref("character", "object", "location", "environment"), "key": text, "value": ref("Value")},
        "state.absent": {"target": typed_ref("character", "object", "location", "environment"), "key": text},
        "knowledge.state": {"knowledge": typed_ref("knowledge"), "state": ref("KnowledgeState")},
        "relationship.matches": {"relationship": typed_ref("relationship"), "values": {**obj({"status": text, "metrics": metrics, "facets": strings}, []), "minProperties": 1}},
        "story-point.state": {"storyPoint": typed_ref("story-point"), "state": ref("PlotState")},
        "outcome.linked": {"event": typed_ref("event"), "target": typed_ref("story-point", "scene")},
        "transition.caused": {"record": typed_ref("knowledge", "relationship", "story-point"), "transitionId": text, "event": typed_ref("event")}}
    for kind, fields in variants.items(): predicates.append(obj({"kind": {"const": kind}, **fields}))
    d["Predicate"] = {"oneOf": predicates}
    d["Check"] = obj({"id": text, "predicate": ref("Predicate")})
    d["Expectations"] = obj({"policy": ref("Policy"), "items": array(ref("Check"), maxItems=100)})
    d["ExpectationCheck"] = obj({"type": {"const": "expectation.check"}, "event": typed_ref("event"), "at": ref("Time"),
                                 "policy": ref("Policy"), "items": array(ref("Check"), minItems=1, maxItems=100)})
    operations["expectation.check"] = "ExpectationCheck"
    # Generic operations intentionally retain existing open extension/source data.
    generic = {
        "entity.create": ({"value": {"type": "object"}, "temporaryId": string}, ["value"]),
        "entity.upsert": ({"value": {"type": "object"}, "temporaryId": string}, ["value"]),
        "entity.update": ({"entity": ref("Ref"), "entityId": ref("Ref"), "frontmatterPatch": {"type": "object"}, "bodyMarkdown": string}, []),
        "entity.delete": ({"entity": ref("Ref"), "entityId": ref("Ref")}, []),
        "conversation.create": ({"temporaryId": string, "value": {"type": "object"}}, []),
        "conversation.turn.append": ({"conversation": ref("Ref"), "conversationId": ref("Ref"), "turn": {"type": "object"}, "value": {"type": "object"}}, []),
        "conversation.recollection.record": ({"conversation": ref("Ref"), "conversationId": ref("Ref"), "recollection": {"type": "object"}, "value": {"type": "object"}}, [])}
    for index, (kind, (fields, required)) in enumerate(generic.items()):
        name = "GenericOperation" + str(index)
        d[name] = obj({"type": {"const": kind}, **fields}, ["type", *required], opened=True)
        if kind in {"entity.update", "entity.delete"}: d[name]["anyOf"] = [{"required": [key]} for key in ("entity", "entityId")]
        if kind.startswith("conversation.") and not kind.endswith("create"): d[name]["anyOf"] = [{"required": [key]} for key in ("conversation", "conversationId")]
        operations[kind] = name
    d["Operation"] = {"oneOf": [ref(value) for value in operations.values()]}
    d["BatchOperation"] = {"oneOf": [ref("BatchEventCreate" if kind == "event.create" else value) for kind, value in operations.items()]}
    d["DeltaRequest"] = obj({"protocol": {"const": "wedl-event-consequence-delta/v1"}, "at": ref("Time"), "limit": ref("Limit")})
    envelope = {"protocol": {"const": "wedl-changeset/v1"}, "expectedHead": ref("Revision"), "idempotencyKey": text, "summary": text,
                "requestId": string, "operations": array(ref("Operation"), minItems=1), "consequenceRequest": ref("DeltaRequest")}
    d["ChangesetRequest"] = obj(envelope, ["protocol", "expectedHead", "idempotencyKey", "summary", "operations"], opened=True)
    d["BatchIntent"] = obj({"action": {"const": "consequence.batch"}, "expectedHead": ref("Revision"), "idempotencyKey": text,
                            "summary": text, "operations": array(ref("BatchOperation"), minItems=1, maxItems=1000), "consequenceRequest": ref("DeltaRequest")},
                           ["action", "expectedHead", "idempotencyKey", "operations"])
    d["EventRequest"] = obj({"protocol": {"const": "wedl-event-consequences/v1"}, "revision": ref("Revision"), "event": typed_ref("event"),
                             "at": ref("Time"), "limit": ref("Limit"), "expectations": ref("Expectations")}, ["protocol", "revision", "event", "at", "limit"])
    d["Provenance"] = {"oneOf": [obj({"kind": {"const": "source"}, "revision": ref("Revision"), "blobOid": ref("Revision")}),
                                  obj({"kind": {"const": "candidate"}, "baseRevision": ref("Revision"), "requestHash": ref("Hash")})]}
    d["Citation"] = obj({"recordId": text, "sourcePath": {**text, "pattern": r"^(?!/)(?!.*\\)(?!.*:)(?!.*(?:^|/)\.\.(?:/|$)).+$"},
                         "section": text, "sourceOrdinal": nullable(nonnegative), "memberId": nullable(text), "time": nullable(ref("Time")), "provenance": ref("Provenance")})
    cites = array(ref("Citation"))
    d["ObservedValue"] = {"oneOf": [obj({"presence": {"const": "absent"}}), obj({"presence": {"const": "present"}, "value": ref("Value")})]}
    d["ObservedMetric"] = {"oneOf": [obj({"presence": {"const": "absent"}}), obj({"presence": {"const": "present"}, "value": number})]}
    d["ObservedReference"] = {"oneOf": [obj({"presence": {"const": "absent"}}), obj({"presence": {"const": "present"}, "value": text})]}
    d["Subject"] = {"oneOf": [obj({"kind": {"const": "state"}, "recordId": text, "key": text}),
                               obj({"kind": enum("knowledge", "relationship", "story-point"), "recordId": text}),
                               obj({"kind": {"const": "outcome"}, "recordId": text, "targetId": text, "targetKind": enum("story-point", "scene")})]}
    payloads = [obj({"value": ref("Value")}), obj({"state": ref("KnowledgeState"), "confidence": nullable(number), "transitionId": text, "time": ref("Time")}),
                obj({"status": text, "metrics": metrics, "facets": strings, "transitionId": text, "time": ref("Time")}),
                obj({"storedState": ref("PlotState"), "derivedState": enum("dormant", "active", "resolved", "failed", "cancelled", "eligible", "blocked"),
                     "eligible": boolean, "dependenciesSatisfied": boolean, "triggerSatisfied": boolean}), obj({"eventLinked": boolean, "targetLinked": boolean})]
    d["Payload"] = {"oneOf": payloads}
    d["Snapshot"] = {"oneOf": [obj({"presence": {"const": "absent"}, "payload": {"type": "null"}, "citations": cites}),
                                obj({"presence": {"const": "present"}, "payload": ref("Payload"), "citations": cites})]}
    d["Change"] = obj({"subject": ref("Subject"), "before": ref("Snapshot"), "after": ref("Snapshot")})
    d["SectionDelta"] = obj({"section": text, "before": ref("ObservedValue"), "after": ref("ObservedValue")})
    d["RecordDelta"] = obj({"recordId": text, "kind": text, "change": enum("created", "deleted", "updated", "noop"), "before": nullable(ref("Reference")),
                            "after": nullable(ref("Reference")), "sections": array(ref("SectionDelta")), "operationIndexes": array(nonnegative, uniqueItems=True), "citations": cites})
    d["EffectEntry"] = obj({"effect": ref("LegacyEffect"), "citation": ref("Citation")})
    d["CausedTransition"] = {"oneOf": [obj({"kind": {"const": kind}, "record": ref("Reference"), "transitionId": nullable(text), "sourceOrdinal": nonnegative,
                                          "time": ref("Time"), "atEventTime": boolean, "transition": ref(family + "Transition"), "citation": ref("Citation")})
                                       for kind, family in (("knowledge", "Knowledge"), ("relationship", "Relationship"), ("story-point", "Plot"))]}
    d["OutcomeEntry"] = obj({"event": ref("Reference"), "target": ref("Reference"), "reciprocal": boolean, "citations": cites})
    d["CausalSuccessor"] = obj({"event": ref("Reference"), "time": ref("Time"), "citation": ref("Citation")})
    d["CurrentAtHorizon"] = obj({"subject": ref("Subject"), "at": ref("Time"), "snapshot": ref("Snapshot"),
                                "supersession": enum("unchanged", "superseded", "unknown"), "supersedingCitations": cites})
    d["LinkAdvisory"] = obj({"code": {"const": "CONSEQUENCE-LINK-001"}, "event": ref("Reference"), "target": ref("Reference"), "citations": cites})
    actuals = [obj({"kind": {"const": "state"}, "observed": ref("ObservedValue")}),
               obj({"kind": {"const": "knowledge"}, "state": ref("KnowledgeState"), "transitionId": text, "time": ref("Time")}),
               obj({"kind": {"const": "relationship"}, "values": obj({"status": text, "metrics": {"type": "object", "additionalProperties": ref("ObservedMetric")}, "facets": strings}, [])}),
               obj({"kind": {"const": "story-point"}, "state": ref("PlotState")}), obj({"kind": {"const": "outcome"}, "eventLinked": boolean, "targetLinked": boolean}),
               obj({"kind": {"const": "transition"}, "causingEvent": ref("ObservedReference"), "transitionId": text, "time": ref("Time")})]
    d["Actual"] = {"oneOf": actuals}
    d["ComparisonIdentity"] = obj({"world": {"oneOf": [obj({"kind": {"const": "source"}, "revision": ref("Revision")}),
                                                     obj({"kind": {"const": "candidate"}, "baseRevision": ref("Revision"), "requestHash": ref("Hash")})]},
                                   "at": ref("Time"), "predicate": ref("Predicate")})
    d["CheckResult"] = {"oneOf": [obj({"id": text, "policy": ref("Policy"), "outcome": {"const": outcome}, "code": {"const": "EXPECTATION-" + outcome.upper()},
                                      "comparison": ref("ComparisonIdentity"), "actual": ref("Actual") if outcome in {"pass", "fail"} else obj({"kind": {"const": "redacted"}}),
                                      "citations": cites if outcome in {"pass", "fail"} else {**cites, "maxItems": 0}}) for outcome in ("pass", "fail", "unknown", "unsupported")]}
    for variant in d["CheckResult"]["oneOf"][:2]:
        variant["allOf"] = [{"if": {"properties": {"comparison": {"properties": {"predicate": {"properties": {"kind": enum(*kinds)}}}}}},
                             "then": {"properties": {"actual": {"properties": {"kind": {"const": family}}}}}}
                            for family, kinds in (("state", ("state.equals", "state.absent")), ("knowledge", ("knowledge.state",)),
                                                  ("relationship", ("relationship.matches",)), ("story-point", ("story-point.state",)),
                                                  ("outcome", ("outcome.linked",)), ("transition", ("transition.caused",)))]
    checks = array(ref("CheckResult"), maxItems=100)
    codes = {"invalid": enum("CONSEQUENCE-REQUEST-001", "CONSEQUENCE-SOURCE-001"), "unavailable": {"const": "CONSEQUENCE-UNAVAILABLE-001"}, "limit": {"const": "CONSEQUENCE-LIMIT-001"}}
    d["CheckFailure"] = {"oneOf": [obj({"outcome": {"const": outcome}, "code": code, "message": string}) for outcome, code in codes.items()]}
    d["CheckGroup"] = obj({"operationIndex": nonnegative, "event": ref("Reference"), "at": ref("Time"), "policy": ref("Policy"), "results": checks})
    d["CheckReportOk"] = obj({"outcome": {"const": "ok"}, "baseRevision": ref("Revision"), "candidate": ref("Candidate"), "groups": array(ref("CheckGroup")), "applyAllowed": boolean})
    d["CheckReport"] = {"oneOf": [ref("CheckReportOk"), ref("CheckFailure")]}
    d["CheckOnlyApplyResult"] = obj({"protocol": {"const": "wedl-command-result/v1"}, "status": {"const": "checked"},
                                     "previousHead": ref("Revision"), "newHead": ref("Revision"),
                                     "generatedIds": {"type": "object", "additionalProperties": text}, "touchedEntityIds": {**array(text), "maxItems": 0},
                                     "compile": obj({"status": {"const": "not-required"}, "revision": ref("Revision")}),
                                     "idempotentReplay": boolean, "expectationChecks": ref("CheckReportOk")})
    d["TimeScope"] = obj({"mode": {"const": "author-as-of"}, "at": ref("Time")})
    event_proto, delta_proto = {"const": "wedl-event-consequences/v1"}, {"const": "wedl-event-consequence-delta/v1"}
    d["EventOk"] = obj({"protocol": event_proto, "outcome": {"const": "ok"}, "revision": ref("Revision"), "event": ref("Reference"), "eventTime": ref("Time"), "at": ref("Time"),
                        "timeScope": ref("TimeScope"), "effects": array(ref("EffectEntry")), "changes": array(ref("Change")), "causedTransitions": array(ref("CausedTransition")),
                        "outcomes": array(ref("OutcomeEntry")), "causalSuccessors": array(ref("CausalSuccessor")), "currentAtHorizon": array(ref("CurrentAtHorizon")),
                        "advisories": array(ref("LinkAdvisory")), "expectations": checks, "applyAllowed": boolean})
    d["EventDeltaGroup"] = obj({"event": ref("Reference"), "eventTime": ref("Time"), "changes": array(ref("Change")), "recordChanges": array(ref("RecordDelta"))})
    d["DeltaOk"] = obj({"protocol": delta_proto, "outcome": {"const": "ok"}, "baseRevision": ref("Revision"), "candidate": ref("Candidate"), "at": ref("Time"), "timeScope": ref("TimeScope"),
                        "focusEvents": array(ref("Reference"), maxItems=100), "eventGroups": array(ref("EventDeltaGroup")), "unattributedChanges": array(ref("Change")),
                        "unattributedRecordChanges": array(ref("RecordDelta")), "expectations": checks, "applyAllowed": boolean})
    d["Failure"] = {"oneOf": [obj({"protocol": enum("wedl-event-consequences/v1", "wedl-event-consequence-delta/v1"), "outcome": {"const": outcome}, "code": code, "message": string}) for outcome, code in codes.items()]}
    d["EventReport"] = {"oneOf": [ref("EventOk"), {"allOf": [ref("Failure"), {"properties": {"protocol": event_proto}}]}]}
    d["SemanticDelta"] = {"oneOf": [ref("DeltaOk"), {"allOf": [ref("Failure"), {"properties": {"protocol": delta_proto}}]}]}
    d["SchemaContextRequest"] = obj({"revision": ref("Revision")})
    d["StateKeyDefinition"] = obj({"entityKind": text, "key": text, "valueType": enum("string", "entity", "boolean", "number", "array", "object", "null"),
                                   "entityKindConstraint": nullable(text), "nullable": boolean, "itemDefinition": nullable(ref("Value")),
                                   "objectDefinition": nullable(ref("Value")), "exclusiveGroup": nullable(text), "allowedValues": nullable(array(ref("Value")))})
    d["MetricDefinition"] = obj({"name": text, "minimum": nullable(number), "maximum": nullable(number)})
    from .api_schemas import _CANONICAL_CAPABILITY_LISTS
    d["SchemaContext"] = obj({"revision": ref("Revision"), "sourceSchema": enum(*sorted(COMPILED_SOURCE_SCHEMAS)), "capabilities": {"oneOf": [{"const": value} for value in _CANONICAL_CAPABILITY_LISTS]},
                              "stateKeys": array(ref("StateKeyDefinition")), "relationshipMetrics": array(ref("MetricDefinition"))})
    d["SchemaContext"]["allOf"] = [{"if": {"properties": {"sourceSchema": {"const": "wedl/v0.7"}}},
                                    "then": {"properties": {"capabilities": {"minItems": 1}}},
                                    "else": {"properties": {"capabilities": {"const": []}}}}]
    d["operationNames"] = operations  # Removed by public adapters; binds guidance to the exact dispatcher.
    return d


def components() -> dict[str, dict]:
    def rewrite(value):
        if isinstance(value, dict): return {key: ("#/components/schemas/Consequence" + item[8:] if key == "$ref" and item.startswith("#/$defs/") else rewrite(item)) for key, item in value.items()}
        if isinstance(value, list): return [rewrite(item) for item in value]
        return value
    return {"Consequence" + name: rewrite(value) for name, value in definitions().items() if name != "operationNames"}


def schema_context(world: World, scope: AuthorScope) -> dict:
    """Describe one preloaded authorized revision; never load HEAD or a cache."""
    if not isinstance(world, World) or not isinstance(scope, AuthorScope) or not re.fullmatch(r"[0-9a-f]{40}", world.revision):
        raise UsageError("schema context requires an authorized exact revision")
    record = world.records.get(scope.world_id) if scope.world_id in scope.record_ids else None
    if record is None or record.kind != "world" or not all(scope.permits(record.id, field) for field in
            ("frontmatter.schema", "frontmatter.capabilities", "frontmatter.state_keys", "frontmatter.relationship_metrics")):
        raise UsageError("schema context unavailable")
    config = record.frontmatter
    schema = config.get("schema")
    if schema not in COMPILED_SOURCE_SCHEMAS:
        raise UsageError("unsupported schema context source")
    capabilities = canonical_capabilities(config.get("capabilities")) if schema == "wedl/v0.7" else ()
    if capabilities is None:
        raise UsageError("unsupported schema context capabilities")
    def hint(rule):
        if not isinstance(rule, dict) or not isinstance(rule.get("type"), str) or rule["type"] not in {"string", "entity", "boolean", "number", "array", "object", "null"}:
            raise UsageError("unrepresentable state definition")
        result = {"type": rule["type"]}
        if rule["type"] == "entity" and "entity_kind" in rule:
            if not isinstance(rule["entity_kind"], str) or not rule["entity_kind"].strip(): raise UsageError("unrepresentable entity restriction")
            result["entity_kind"] = rule["entity_kind"]
        if rule["type"] == "array" and rule.get("items") is not None: result["items"] = hint(rule["items"])
        if rule["type"] == "object" and rule.get("properties") is not None:
            if not isinstance(rule["properties"], dict): raise UsageError("unrepresentable object reference hints")
            if any(not isinstance(key, str) for key in rule["properties"]): raise UsageError("unrepresentable property name")
            result["properties"] = {key: hint(value) for key, value in sorted(rule["properties"].items())}
        return result
    state_keys, metric_rules = config.get("state_keys") or {}, config.get("relationship_metrics") or {}
    if not isinstance(state_keys, dict) or not isinstance(metric_rules, dict): raise UsageError("unrepresentable context declarations")
    if any(not isinstance(kind, str) or not kind.strip() or not isinstance(values, dict) for kind, values in state_keys.items()):
        raise UsageError("unrepresentable state declarations")
    keys = []
    for kind, values in sorted(state_keys.items()):
        if any(not isinstance(key, str) or not key.strip() for key in values): raise UsageError("unrepresentable state key")
        for key, rule in sorted(values.items()):
            definition = hint(rule)
            exclusive = rule.get("exclusive_group")
            if exclusive is not None and (not isinstance(exclusive, str) or not exclusive.strip()): raise UsageError("unrepresentable exclusivity group")
            keys.append({"entityKind": kind, "key": key, "valueType": definition["type"], "entityKindConstraint": definition.get("entity_kind"),
                         "nullable": definition["type"] == "null", "itemDefinition": definition.get("items"), "objectDefinition": definition.get("properties"),
                         "exclusiveGroup": exclusive, "allowedValues": None})
    metrics = []
    if any(not isinstance(name, str) or not name.strip() for name in metric_rules): raise UsageError("unrepresentable metric name")
    for name, rule in sorted(metric_rules.items()):
        if not isinstance(rule, dict): raise UsageError("unrepresentable metric definition")
        bounds = [rule.get("minimum"), rule.get("maximum")]
        if any(value is not None and (type(value) not in {int, float} or (type(value) is float and not math.isfinite(value))) for value in bounds):
            raise UsageError("unrepresentable metric bounds")
        if all(value is not None for value in bounds) and bounds[0] > bounds[1]: raise UsageError("unrepresentable metric bounds")
        metrics.append({"name": name, "minimum": bounds[0], "maximum": bounds[1]})
    return {"revision": world.revision, "sourceSchema": schema, "capabilities": list(capabilities), "stateKeys": keys, "relationshipMetrics": metrics}
