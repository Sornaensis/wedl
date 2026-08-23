from __future__ import annotations

import jsonschema
import pytest

from wedl.api_contract import control_endpoints, discovery_components, route_contracts
from wedl.api_schemas import components, operation_example


def _validate(schema_name: str, value: object, *, errors: bool = False) -> None:
    registry = discovery_components() if errors else components()
    document = {"components": registry, "$ref": f"#/components/schemas/{schema_name}"}
    jsonschema.Draft202012Validator.check_schema(document)
    jsonschema.validate(value, document)


def test_every_contract_success_example_validates_against_its_component() -> None:
    descriptors = [contract.discovery for contract in route_contracts()]
    descriptors.extend(endpoint.descriptor for endpoint in control_endpoints())
    assert all(descriptor is not None for descriptor in descriptors)
    for descriptor in descriptors:
        assert descriptor is not None and descriptor.success_schema is not None
        assert descriptor.success_examples
        for example in descriptor.success_examples:
            _validate(descriptor.success_schema, example["value"])


def test_every_declared_error_has_a_structured_example() -> None:
    descriptors = [contract.discovery for contract in route_contracts()]
    descriptors.extend(endpoint.descriptor for endpoint in control_endpoints())
    for descriptor in descriptors:
        assert descriptor is not None
        for code in descriptor.errors:
            _validate("WedlError", {"code": code, "message": "example", "details": {}}, errors=True)


def test_handler_shaped_context_and_conversation_variants_validate() -> None:
    # These are the separate envelopes emitted by build_context and
    # conversation_view for their perspective branches, rather than registry
    # response examples. Keep their stable handler fields covered explicitly.
    common_context = {
        "protocol": "wedl-context/v3", "revision": "0123456789abcdef",
        "characterId": "character-mara-vale", "sceneId": "scene-market-day",
        "effectiveTime": {"timeline": "main", "tick": 12, "order": 0},
        "focus": {}, "selection": {"budgetCharacters": 2400},
    }
    _validate("ContextResponse", {**common_context, "perspective": "character", "promptText": "Known evidence."})
    _validate("ContextResponse", {**common_context, "perspective": "dramatic-irony", "characterPrompt": "Known evidence.", "authorMargin": "Withheld evidence."})

    common_conversation = {
        "protocol": "wedl-conversation/v2", "revision": "0123456789abcdef",
        "perspective": "character", "conversationId": "conversation-market-meeting",
        "title": "Market meeting", "characterId": "character-mara-vale",
        "conversationTime": {"start": {"tick": 10}, "end": {"tick": 12}},
        "effectiveTime": {"timeline": "main", "tick": 12, "order": 0},
        "timeScope": {"mode": "as-of", "at": {"tick": 12}},
        "heardVerbatimTurns": [], "perceivedBeats": [], "subjectiveRecollection": None,
        "provenanceBoundary": "heardVerbatimTurns are canonical audible lines.",
    }
    _validate("ConversationResponse", common_conversation)


def test_conversation_discovery_examples_show_typed_beats_but_keep_legacy_turns_speech_only() -> None:
    author, character = (example["value"] for example in operation_example("conversation show"))
    assert [beat["kind"] for beat in author["beats"]] == ["speech", "action", "speech"]
    assert [beat["kind"] for beat in author["verbatimTurns"]] == ["speech", "speech"]
    assert author["beats"][-1]["interrupts"] == author["beats"][0]["id"]
    assert [beat["kind"] for beat in character["perceivedBeats"]] == ["speech", "action", "speech"]
    assert [beat["kind"] for beat in character["heardVerbatimTurns"]] == ["speech", "speech"]
    _validate("ConversationResponse", author)
    _validate("ConversationResponse", character)


def test_timeline_schema_requires_exact_string_coordinates_and_named_entries() -> None:
    response = {
        "protocol": "wedl-timeline/v1", "revision": "0123456789abcdef",
        "timeline": {"id": "main", "label": "Main chronology", "origin": {"tick": "-100", "label": "Archive opening"}},
        "temporalSemantics": {"spacing": "ordinal", "durationSemantics": "none", "intervalEndpoints": "inclusive"},
        "points": [{
            "kind": "plot-transition", "at": {"timeline": "main", "tick": "-9223372036854775808", "order": "2147483647"},
            "status": "draft", "lifecycleState": "active", "entity": {"id": "sp_01", "kind": "story-point", "title": "Find the signal"}, "summary": "The clue becomes active.",
        }],
        "spans": [{
            "kind": "scene", "start": {"timeline": "main", "tick": "-9223372036854775808", "order": "0"},
            "current": {"timeline": "main", "tick": "9223372036854775807", "order": "0"}, "end": None,
            "status": "active", "entity": {"id": "scene_01", "kind": "scene", "title": "At the edge"}, "summary": "A continuing scene.",
        }],
    }
    _validate("TimelineResponse", response)

    invalid_coordinate = {**response, "points": [{**response["points"][0], "at": {"timeline": "main", "tick": 9, "order": "0"}}]}
    invalid_reference = {**response, "spans": [{**response["spans"][0], "entity": {"id": "scene_01", "kind": "scene"}}]}
    invalid_causing_event = {**response, "points": [{**response["points"][0], "causingEvent": {}}]}
    invalid_location = {**response, "points": [{
        "kind": "event", "at": {"timeline": "main", "tick": "0", "order": "0"}, "status": "canonical",
        "entity": {"id": "event_01", "kind": "event", "title": "An event"}, "summary": "An event.", "location": {},
    }], "spans": []}
    invalid_scene = {**response, "points": [], "spans": [{
        "kind": "conversation", "start": {"timeline": "main", "tick": "0", "order": "0"}, "end": None,
        "status": "active", "entity": {"id": "conv_01", "kind": "conversation", "title": "A conversation"}, "summary": "A conversation.", "scene": {},
    }]}
    missing_lifecycle = {**response, "points": [{key: value for key, value in response["points"][0].items() if key != "lifecycleState"}]}
    missing_scene_current = {**response, "spans": [{key: value for key, value in response["spans"][0].items() if key != "current"}]}
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", invalid_coordinate)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", invalid_reference)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", invalid_causing_event)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", invalid_location)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", invalid_scene)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", missing_lifecycle)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", missing_scene_current)


def test_whereabouts_schema_keeps_journeys_named_and_coordinates_exact() -> None:
    response = {
        "protocol": "wedl-whereabouts/v1", "revision": "0123456789abcdef",
        "effectiveTime": {"timeline": "main", "tick": "9223372036854775807", "order": "2147483647"},
        "timeScope": {"mode": "as-of", "at": {"timeline": "main", "tick": "9223372036854775807", "order": "2147483647"}},
        "characterPolicy": {"includedStatuses": ["canonical", "retired"], "excludedStatuses": ["draft"], "locationEvidence": "initial state and canonical location effects only", "inference": "none"},
        "characterFilter": None,
        "characters": [{
            "character": {"id": "char_01", "kind": "character", "title": "Mara"}, "recordStatus": "retired",
            "presence": "unlocated", "location": None, "lastKnownLocation": {"id": "loc_01", "kind": "location", "title": "The crossing"}, "activeScene": None,
            "journey": [{"kind": "clear", "at": {"timeline": "main", "tick": "9223372036854775807", "order": "2147483647"}, "from": {"id": "loc_01", "kind": "location", "title": "The crossing"}, "to": None, "event": {"id": "event_01", "kind": "event", "title": "The bridge falls"}}],
        }],
        "locations": [], "activeScenes": [], "offstageCharacters": [],
        "unlocatedCharacters": [{"id": "char_01", "kind": "character", "title": "Mara"}],
    }
    _validate("WhereaboutsResponse", response)
    invalid = {**response, "effectiveTime": {"timeline": "main", "tick": 12, "order": "0"}}
    with pytest.raises(jsonschema.ValidationError):
        _validate("WhereaboutsResponse", invalid)


def test_timeline_presence_and_conversation_beats_have_typed_safe_projections() -> None:
    timeline_response = {
        "protocol": "wedl-timeline/v1", "revision": "0123456789abcdef",
        "timeline": {"id": "main", "label": "Main chronology"},
        "temporalSemantics": {"spacing": "ordinal", "durationSemantics": "none", "intervalEndpoints": "inclusive"},
        "points": [],
        "spans": [{
            "kind": "scene", "start": {"timeline": "main", "tick": "10", "order": "0"},
            "current": {"timeline": "main", "tick": "12", "order": "0"}, "end": None,
            "status": "active", "entity": {"id": "scene_01", "kind": "scene", "title": "The crossing"}, "summary": "A crossing.",
            "participants": [{"id": "char_01", "kind": "character", "title": "Mara", "from": {"timeline": "main", "tick": "11", "order": "0"}, "to": None}],
        }],
    }
    _validate("TimelineResponse", timeline_response)
    with pytest.raises(jsonschema.ValidationError):
        _validate("TimelineResponse", {**timeline_response, "spans": [{**timeline_response["spans"][0], "participants": [{"id": "char_01", "kind": "character", "title": "Mara"}]}]})

    common = {
        "protocol": "wedl-conversation/v2", "revision": "0123456789abcdef", "perspective": "author",
        "conversationId": "conversation-market-meeting", "title": "Market meeting", "conversationTime": {},
        "effectiveTime": {"timeline": "main", "tick": 12, "order": 0}, "timeScope": {"mode": "as-of"},
        "recollections": [],
    }
    speech = {"id": "turn_01", "kind": "speech", "at": {"timeline": "main", "tick": 12, "order": 0}, "text": "Move.", "audience": ["participants"], "citation": {}, "speakerId": "char_01", "speaker": "Mara", "delivery": None, "addresseeId": None, "addressee": None, "interrupts": None}
    action = {"id": "turn_02", "kind": "action", "at": {"timeline": "main", "tick": 12, "order": 1}, "text": "Mara moves.", "audience": ["participants"], "citation": {}, "actorIds": ["char_01"], "actors": ["Mara"]}
    _validate("ConversationResponse", {**common, "verbatimTurns": [speech], "beats": [speech, action]})
    with pytest.raises(jsonschema.ValidationError):
        _validate("ConversationResponse", {**common, "verbatimTurns": [speech], "beats": [{**action, "speaker": "Mara"}]})
