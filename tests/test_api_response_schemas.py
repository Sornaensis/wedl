from __future__ import annotations

import jsonschema
import pytest

from wedl.api_contract import Transport, control_endpoints, discovery_components, discovery_responses, route_contracts
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


def test_spatial_route_filters_allow_omitted_nullable_modes_but_remain_closed() -> None:
    # FastAPI removes null-valued fields while encoding OpenAPI examples.  The
    # nullable optional member therefore may be absent in a published example,
    # while any supplied value still has the closed route-filter shape.
    _validate("SpatialRouteFilters", {"availability": ["open"]})
    _validate("SpatialRouteFilters", {"modes": None, "availability": ["open"]})
    _validate("SpatialRouteFilters", {"modes": ["foot"], "availability": ["open"]})
    with pytest.raises(jsonschema.ValidationError):
        _validate("SpatialRouteFilters", {"availability": ["open"], "unexpected": True})
    with pytest.raises(jsonschema.ValidationError):
        _validate("SpatialRouteFilters", {"availability": ["closed"]})


def test_spatial_command_http_request_and_status_matrix_is_exact() -> None:
    actions = ("containment", "children", "bbox", "nearby", "adjacency", "reachability", "path", "overlay-as-of")
    spatial = [contract for contract in route_contracts() if contract.command[0] == "spatial"]
    assert tuple(contract.command[1] for contract in spatial) == actions
    assert len({contract.binding.path for contract in spatial}) == len(actions)
    for contract in spatial:
        assert contract.binding.method == "POST"
        assert contract.binding.path == f"/api/spatial/{contract.command[1]}"
        assert contract.binding.parameters == (("require_compiled", "requireCompiled"),)
        body, = (argument for argument in contract.arguments if argument.transport == Transport.BODY)
        assert body.dest == "file" and body.transport_name == "payload"
        descriptor = contract.discovery
        assert descriptor is not None and descriptor.request_schema == f"Spatial{''.join(part.capitalize() for part in contract.command[1].split('-'))}Request"
        responses = discovery_responses(descriptor)
        assert {200, 400, 403, 409, 422} <= set(responses)
        for status, state in ((400, "invalid"), (403, "forbidden"), (409, "unavailable"), (422, "limit")):
            assert responses[status]["x-wedl-spatial-state"] == state


def test_spatial_authoring_union_rejects_title_update_and_static_validity() -> None:
    common = {"expectedHead": "a" * 40, "idempotencyKey": "request-1"}
    update = {"action": "spatial.map.update", **common, "payload": {"id": "map:town", "unit": "league"}}
    _validate("AuthoringRequest", update)
    with pytest.raises(jsonschema.ValidationError):
        _validate("AuthoringRequest", {**update, "payload": {**update["payload"], "title": "Renamed"}})
    with pytest.raises(jsonschema.ValidationError):
        _validate("AuthoringRequest", {**update, "payload": {**update["payload"], "unit": "x" * 257}})
    static = {"action": "spatial.overlay.create", **common, "payload": {"id": "overlay:new", "title": "New", "lifecycle": "static", "membership": {"locationIds": ["location:gate"]}, "audience": ["author"], "perspectives": ["author"]}}
    _validate("AuthoringRequest", static)
    with pytest.raises(jsonschema.ValidationError):
        _validate("AuthoringRequest", {**static, "payload": {**static["payload"], "valid": {"start": {"timeline": "main", "tick": "0", "order": "0"}, "end": {"timeline": "main", "tick": "1", "order": "0"}}}})


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
        "protocol": "wedl-whereabouts/v2", "revision": "0123456789abcdef",
        "effectiveTime": {"timeline": "main", "tick": "9223372036854775807", "order": "2147483647"},
        "timeScope": {"mode": "as-of", "at": {"timeline": "main", "tick": "9223372036854775807", "order": "2147483647"}},
        "characterPolicy": {"includedStatuses": ["canonical", "retired"], "excludedStatuses": ["draft"], "locationEvidence": "initial state and canonical location effects only", "inference": "none"},
        "importancePolicy": {"algorithm": "wedl-character-importance/v1", "calculated": True, "nonCanonical": True, "cohort": "all canonical and retired characters before filtering", "normalization": "per-signal log1p(raw) / log1p(cohort maximum); zero maximum contributes zero", "weights": {"scenes": 40, "pointOfViewScenes": 25, "events": 20, "relationshipNeighbors": 15}, "evidence": "scene appearances and POV subset; canonical event participants/effect targets deduplicated per event; distinct reciprocal relationship neighbors", "exclusions": "No prose, tags, inferred travel, co-presence, knowledge, or manual overrides are used."},
        "characterFilter": None,
        "characters": [{
            "character": {"id": "char_01", "kind": "character", "title": "Mara"}, "recordStatus": "retired", "role": None,
            "importance": {"algorithm": "wedl-character-importance/v1", "score": 0.0, "raw": {"scenes": 0, "pointOfViewScenes": 0, "events": 0, "relationshipNeighbors": 0}, "normalized": {"scenes": 0.0, "pointOfViewScenes": 0.0, "events": 0.0, "relationshipNeighbors": 0.0}, "contributions": {"scenes": 0.0, "pointOfViewScenes": 0.0, "events": 0.0, "relationshipNeighbors": 0.0}, "explanation": "Calculated from authored evidence."},
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
