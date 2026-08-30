from __future__ import annotations

import asyncio
from copy import deepcopy
import inspect
from types import SimpleNamespace

from fastapi import FastAPI, Header
from fastapi.responses import HTMLResponse
from fastapi.testclient import TestClient

import wedl.api_contract as api_contract
from wedl import command_parser
from wedl.api_contract import (
    ApiClass,
    AuthPolicy,
    Transport,
    command_contracts,
    contract_for_route,
    control_endpoint,
    control_endpoints,
    discovery_components,
    discovery_openapi_extra,
    discovery_responses,
    normalize_discovery_openapi,
    route_contracts,
    validate_request_fields,
)
from wedl.api_router import _endpoint, create_api_router, openapi_contract_errors
from wedl.command_parser import parser
from wedl.errors import UsageError
from wedl.server import create_app


def _parser_leaves() -> set[tuple[str, ...]]:
    import argparse

    def walk(current: argparse.ArgumentParser, prefix: tuple[str, ...] = ()) -> set[tuple[str, ...]]:
        subparsers = [action for action in current._actions if isinstance(action, argparse._SubParsersAction)]
        if not subparsers:
            return {prefix}
        return {
            leaf
            for action in subparsers
            for name, child in action.choices.items()
            for leaf in walk(child, (*prefix, name))
        }

    return walk(parser())


def test_command_contract_covers_each_parser_leaf_once() -> None:
    contracts = command_contracts()
    assert len(contracts) == 45
    assert {contract.command for contract in contracts} == _parser_leaves()
    assert {contract.command for contract in contracts if contract.api_class == ApiClass.LOCAL_ONLY} == {
        ("completion",), ("init",), ("serve",), ("migrate", "preview"), ("migrate", "apply"),
        ("author", "current-time", "set"), ("author", "scene", "create"),
        ("author", "scene", "advance"), ("author", "scene", "close"),
        ("author", "move"), ("author", "conversation", "create"),
        ("author", "conversation", "append"),
        ("author", "hypothesis", "create"), ("author", "hypothesis", "adopt"),
        ("author", "hypothesis", "reject"), ("author", "chronology", "replace"),
    }
    assert all(contract.binding is not None for contract in contracts if contract.availability == "mounted")
    assert all(argument.transport_reason for contract in contracts for argument in contract.arguments)
    assert {argument.transport for contract in contracts for argument in contract.arguments} >= {
        Transport.QUERY, Transport.PATH, Transport.BODY, Transport.INJECT, Transport.OMIT,
    }
    conversation_create = next(contract for contract in contracts if contract.command == ("author", "conversation", "create"))
    assert conversation_create.binding is None
    assert conversation_create.api_class == ApiClass.LOCAL_ONLY
    assert {argument.dest for argument in conversation_create.arguments} >= {"title", "scene", "characters"}


def test_contract_rejects_unknown_fields_without_matching_a_route() -> None:
    try:
        validate_request_fields("GET", "/api/search", ("q", "unexpected"))
    except UsageError as exc:
        assert exc.code == "usage_error"
        assert exc.details["unexpected"] == ["unexpected"]
    else:
        raise AssertionError("unknown parser-external API input must be rejected")


def test_source_loading_contracts_advertise_the_v04_quarantine_error() -> None:
    source_loading = [contract for contract in route_contracts() if contract.discovery is not None and "parse_error" in contract.discovery.errors]

    assert source_loading
    assert all("v04_superseded" in contract.discovery.errors for contract in source_loading)
    assert api_contract.ERROR_VOCABULARY["v04_superseded"][0] == 400


def test_api_rejects_unknown_query_field(ash_repo) -> None:
    with TestClient(create_app(ash_repo.root)) as client:
        response = client.get("/api/entities", params={"kind": "character", "unknown": "value"})

    assert response.status_code == 400
    assert response.json()["code"] == "usage_error"
    assert response.json()["details"]["unexpected"] == ["unknown"]


def _schema_from_contract_router() -> dict:
    """OpenAPI generation needs no live repository until an endpoint runs."""

    app = FastAPI()
    app.include_router(create_api_router(SimpleNamespace(), lambda: None))
    return normalize_discovery_openapi(app.openapi())


def _production_contract_schema() -> dict:
    """Use an auth signature matching the real server for the two-way gate."""

    async def authorize(
        x_wedl_token: str | None = Header(
            default=None,
            alias="X-Wedl-Token",
            description=AuthPolicy.SESSION.header_description,
        )
    ) -> None:
        return None

    app = FastAPI()
    app.include_router(create_api_router(SimpleNamespace(), authorize))
    root = control_endpoint("GET", "/")

    @app.get(
        "/",
        response_class=HTMLResponse,
        operation_id="root_get",
        summary=root.descriptor.summary,
        description=root.descriptor.description,
        tags=list(root.descriptor.tags),
        responses=discovery_responses(root.descriptor),
        openapi_extra=discovery_openapi_extra(root.descriptor, auth=root.auth),
    )
    async def root() -> dict:
        return {}
    return normalize_discovery_openapi(app.openapi())


def test_every_mounted_behavior_has_one_contract_owned_discovery_descriptor() -> None:
    routes = route_contracts()
    assert len(routes) == 29
    assert all(contract.discovery is not None and contract.binding is not None for contract in routes)
    assert all(contract.discovery.examples for contract in routes if contract.discovery is not None)
    assert {contract.command for contract in routes} == {
        ("status",), ("validate",), ("compile",), ("entity", "list"), ("entity", "show"),
        ("state",), ("knowledge",), ("interactions",), ("story-points",), ("timeline",), ("threads",), ("thread-memberships",), ("whereabouts",), ("hypotheses",), ("causal",), ("search",),
        ("context",), ("conversation", "show"), ("changeset", "scaffold"),
        ("changeset", "schema"), ("changeset", "preview"), ("changeset", "apply"),
        ("author", "request", "preview"), ("author", "request", "apply"),
        ("chronology", "catalog"), ("chronology", "format"), ("chronology", "convert"),
        ("chronology", "search"), ("chronology", "story-times"),
    }
    assert {(endpoint.method, endpoint.path) for endpoint in (control_endpoint("GET", "/api/session"), control_endpoint("GET", "/"))} == {
        ("GET", "/api/session"), ("GET", "/"),
    }
    assert all(endpoint.descriptor.examples for endpoint in control_endpoints())
    for command in (("changeset", "preview"), ("changeset", "apply")):
        contract = next(route for route in routes if route.command == command)
        assert contract.discovery is not None
        body = contract.discovery.examples[0]["value"]["body"]
        assert isinstance(body, dict)
        assert {"protocol", "expectedHead", "idempotencyKey", "summary", "operations"} <= set(body)


def test_control_endpoint_inventory_fails_closed_on_missing_duplicate_or_stale_entries(monkeypatch) -> None:
    original = control_endpoints()
    monkeypatch.setattr(api_contract, "_CONTROL_ENDPOINTS", original[:1])
    try:
        control_endpoints()
    except RuntimeError as exc:
        assert "exactly GET /api/session and GET /" in str(exc)
    else:
        raise AssertionError("missing control endpoint must fail closed")

    monkeypatch.setattr(api_contract, "_CONTROL_ENDPOINTS", (*original, original[0]))
    try:
        control_endpoints()
    except RuntimeError as exc:
        assert "exactly GET /api/session and GET /" in str(exc)
    else:
        raise AssertionError("duplicate control endpoint must fail closed")

    stale = api_contract.ControlEndpoint("GET", "/health", AuthPolicy.PUBLIC, original[0].descriptor)
    monkeypatch.setattr(api_contract, "_CONTROL_ENDPOINTS", (*original, stale))
    try:
        control_endpoints()
    except RuntimeError as exc:
        assert "exactly GET /api/session and GET /" in str(exc)
    else:
        raise AssertionError("stale control endpoint must fail closed")


def test_discovery_metadata_and_structural_error_components_are_generated() -> None:
    schema = _schema_from_contract_router()
    for contract in route_contracts():
        assert contract.binding is not None and contract.discovery is not None
        operation = schema["paths"][contract.binding.path][contract.binding.method.lower()]
        assert operation["summary"] == contract.discovery.summary
        assert operation["description"] == contract.discovery.description
        assert operation["tags"] == list(contract.discovery.tags)
        assert operation["x-wedl-error-codes"] == list(contract.discovery.errors)
        assert operation["x-wedl-examples"] == list(contract.discovery.examples)
        assert operation["responses"]["200"]["description"] == contract.discovery.success_description
        assert "422" not in operation["responses"]

    components = discovery_components()
    assert components["schemas"]["WedlError"]["properties"]["code"]["type"] == "string"
    assert set(components["responses"]) == {"Wedl400Error", "Wedl401Error", "Wedl409Error"}
    assert set(schema["components"]) == {"schemas", "responses"}
    assert set(schema["components"]["schemas"]) == set(components["schemas"])
    assert "HTTPValidationError" not in schema["components"]["schemas"]
    assert "ValidationError" not in schema["components"]["schemas"]


def test_openapi_parameters_are_synthesized_from_mounted_contracts() -> None:
    schema = _schema_from_contract_router()
    for contract in route_contracts():
        assert contract.binding is not None
        operation = schema["paths"][contract.binding.path][contract.binding.method.lower()]
        documented = {
            parameter["name"]
            for parameter in operation.get("parameters", [])
            if parameter["in"] in {"query", "path"}
        }
        expected = {
            argument.transport_name
            for argument in contract.arguments
            if argument.transport in {Transport.QUERY, Transport.PATH}
        }
        assert documented == expected

    search = {parameter["name"]: parameter for parameter in schema["paths"]["/api/search"]["get"]["parameters"]}
    assert search["q"]["required"] is True
    assert search["perspective"]["schema"]["enum"] == ["author", "character"]
    assert search["limit"]["schema"] == {
        "type": "integer", "maximum": 50, "minimum": 1,
        "description": "maximum results, from 1 to 50 (default: 20)",
        "default": 20, "title": "Limit"
    }
    assert search["allTime"]["schema"]["default"] is False

    context = {parameter["name"]: parameter for parameter in schema["paths"]["/api/context"]["get"]["parameters"]}
    assert context["maxCharacters"]["schema"]["minimum"] == 1800
    assert context["maxItems"]["schema"]["minimum"] == 1


def test_read_route_contracts_have_full_cli_parameter_parity() -> None:
    expected = {
        ("status",): ("GET", "/api/status", set()),
        ("validate",): ("GET", "/api/validate", set()),
        ("entity", "list"): ("GET", "/api/entities", {"kind", "text", "requireCompiled"}),
        ("entity", "show"): ("GET", "/api/entities/{entity_id}", {"requireCompiled"}),
        ("state",): ("GET", "/api/entities/{entity_id}/state", {"tick", "timeline", "order", "requireCompiled"}),
        ("knowledge",): ("GET", "/api/entities/{character_id}/knowledge", {"tick", "timeline", "order", "requireCompiled"}),
        ("interactions",): ("GET", "/api/interactions", {"first", "second", "requireCompiled"}),
        ("story-points",): ("GET", "/api/story-points", {"scene", "tick", "timeline", "order", "requireCompiled"}),
        ("timeline",): ("GET", "/api/timeline", {"timeline", "requireCompiled"}),
        ("threads",): ("GET", "/api/threads", {"requireCompiled"}),
        ("thread-memberships",): ("GET", "/api/thread-memberships", {"recordId", "threadId", "requireCompiled"}),
        ("whereabouts",): ("GET", "/api/whereabouts", {"character", "tick", "timeline", "order", "requireCompiled"}),
        ("hypotheses",): ("GET", "/api/hypotheses", {"hypothesis", "status", "text", "requireCompiled"}),
        ("causal",): ("GET", "/api/causal/{event_id}", {"direction", "tick", "timeline", "order", "requireCompiled"}),
        ("search",): ("GET", "/api/search", {"q", "perspective", "character", "scene", "mode", "limit", "includeText", "includeHypotheses", "threadId", "timeline", "tick", "order", "allTime", "requireCompiled"}),
        ("context",): ("GET", "/api/context", {"character", "scene", "perspective", "q", "mode", "maxCharacters", "maxItems", "recallThreadId", "timeline", "tick", "order", "requireCompiled"}),
        ("conversation", "show"): ("GET", "/api/conversations/{conversation_id}", {"perspective", "character", "timeline", "tick", "order", "allTime", "requireCompiled"}),
        ("chronology", "catalog"): ("GET", "/api/chronology", {"requireCompiled"}),
        ("chronology", "format"): ("POST", "/api/chronology/format", {"requireCompiled"}),
        ("chronology", "convert"): ("POST", "/api/chronology/convert", {"requireCompiled"}),
        ("chronology", "search"): ("POST", "/api/chronology/search", {"requireCompiled"}),
        ("chronology", "story-times"): ("POST", "/api/chronology/story-times", {"requireCompiled"}),
    }

    actual = {
        contract.command: (
            contract.binding.method,
            contract.binding.path,
            {
                argument.transport_name
                for argument in contract.arguments
                if argument.transport == Transport.QUERY
            },
        )
        for contract in route_contracts()
        if contract.api_class == ApiClass.QUERY and contract.command[0] != "changeset"
    }
    assert actual == expected


def test_workflow_route_contracts_are_mounted_with_deliberate_transports() -> None:
    expected = {
        ("compile",): ("POST", "/api/compile"),
        ("changeset", "schema"): ("GET", "/api/changesets/schema"),
        ("changeset", "scaffold"): ("POST", "/api/changesets/scaffold"),
        ("changeset", "preview"): ("POST", "/api/changesets/preview"),
        ("changeset", "apply"): ("POST", "/api/changesets/apply"),
    }
    actual = {
        contract.command: (contract.binding.method, contract.binding.path)
        for contract in route_contracts()
        if contract.command[0] == "compile" or contract.command[0] == "changeset"
    }
    assert actual == expected

    for command in (("changeset", "preview"), ("changeset", "apply")):
        contract = next(item for item in route_contracts() if item.command == command)
        body = next(argument for argument in contract.arguments if argument.transport == Transport.BODY)
        assert body.dest == "file"
        assert body.transport_name == "payload"

    compile_contract = next(item for item in route_contracts() if item.command == ("compile",))
    assert all(argument.transport != Transport.BODY for argument in compile_contract.arguments)
    assert compile_contract.binding.auth == AuthPolicy.SESSION

    apply_contract = next(item for item in route_contracts() if item.command == ("changeset", "apply"))
    confirmation = next(item for item in apply_contract.arguments if item.transport == Transport.HEADER)
    assert (confirmation.dest, confirmation.transport_name) == ("confirm", "X-Wedl-Confirmation")
    assert apply_contract.binding.headers[0].dest == "confirm"


def test_production_openapi_contract_comparator_is_two_way() -> None:
    schema = _production_contract_schema()
    assert openapi_contract_errors(schema) == ()
    operations = {
        (path, method)
        for path, item in schema["paths"].items()
        for method in item
        if method in {"get", "post", "put", "patch", "delete", "head", "options", "trace"}
    }
    assert len(operations) == 31  # 29 parser contracts, session, and root.

    status_example = schema["paths"]["/api/status"]["get"]["responses"]["200"]["content"]["application/json"]["examples"]["success"]["value"]
    assert status_example["activeScenes"] == [{"id": "scene-market-day", "title": "Market day"}]
    assert status_example["currentTime"] == {"timeline": "main", "tick": 12, "order": 0}

    missing = deepcopy(schema)
    missing["paths"].pop("/api/changesets/schema")
    assert any("omits contracted operation GET /api/changesets/schema" in error for error in openapi_contract_errors(missing))

    extra = deepcopy(schema)
    extra["paths"]["/api/uncontracted"] = {"get": {"operationId": "uncontracted"}}
    assert any("uncontracted operation GET /api/uncontracted" in error for error in openapi_contract_errors(extra))

    non_api = deepcopy(schema)
    non_api["paths"]["/health"] = {"get": {"operationId": "health"}}
    assert any("uncontracted operation GET /health" in error for error in openapi_contract_errors(non_api))

    root_control = deepcopy(schema)
    root_control["paths"]["/"]["head"] = {"operationId": "unexpected_head"}
    assert any("uncontracted operation HEAD /" in error for error in openapi_contract_errors(root_control))

    operation_id = deepcopy(schema)
    operation_id["paths"]["/api/search"]["get"]["operationId"] = "wrong"
    assert any("operation ID drift" in error for error in openapi_contract_errors(operation_id))

    auth = deepcopy(schema)
    auth["paths"]["/api/compile"]["post"]["x-wedl-auth-policy"] = "public"
    assert any("auth extension drift" in error for error in openapi_contract_errors(auth))

    confirmation = deepcopy(schema)
    confirmation["paths"]["/api/changesets/apply"]["post"]["parameters"] = [
        parameter
        for parameter in confirmation["paths"]["/api/changesets/apply"]["post"]["parameters"]
        if parameter["name"] != "X-Wedl-Confirmation"
    ]
    assert any("parameter descriptor drift" in error for error in openapi_contract_errors(confirmation))

    duplicate_parameter = deepcopy(schema)
    duplicate_parameter["paths"]["/api/search"]["get"]["parameters"].append(
        deepcopy(duplicate_parameter["paths"]["/api/search"]["get"]["parameters"][0])
    )
    assert any("duplicate parameter bindings" in error for error in openapi_contract_errors(duplicate_parameter))

    parameter_shape = deepcopy(schema)
    limit = next(parameter for parameter in parameter_shape["paths"]["/api/search"]["get"]["parameters"] if parameter["name"] == "limit")
    limit["schema"]["maximum"] = 51
    assert any("parameter descriptor drift" in error for error in openapi_contract_errors(parameter_shape))

    raw_body = deepcopy(schema)
    raw_body["paths"]["/api/changesets/preview"]["post"]["requestBody"]["content"] = {
        "application/json": {"schema": {"type": "object", "properties": {"payload": {"type": "object"}}}}
    }
    assert any("raw unwrapped body descriptor drift" in error for error in openapi_contract_errors(raw_body))

    success_schema = deepcopy(schema)
    success_schema["paths"]["/api/status"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"] = "#/components/schemas/Wrong"
    assert any("success schema reference drift" in error for error in openapi_contract_errors(success_schema))

    success_example = deepcopy(schema)
    success_example["paths"]["/api/status"]["get"]["responses"]["200"]["content"]["application/json"]["examples"] = {}
    assert any("success examples drift" in error for error in openapi_contract_errors(success_example))

    error_example = deepcopy(schema)
    error_example["paths"]["/api/search"]["get"]["responses"]["400"]["content"]["application/json"]["examples"] = {}
    assert any("response descriptor drift" in error for error in openapi_contract_errors(error_example))

    components = deepcopy(schema)
    components["components"]["schemas"]["WedlError"]["properties"]["code"]["type"] = "integer"
    assert any("WEDL schemas component drift" in error for error in openapi_contract_errors(components))

    missing_response = deepcopy(schema)
    missing_response["paths"]["/api/search"]["get"]["responses"].pop("400")
    assert any("response status drift" in error for error in openapi_contract_errors(missing_response))

    extra_response = deepcopy(schema)
    extra_response["paths"]["/api/search"]["get"]["responses"]["418"] = {"description": "unexpected"}
    assert any("response status drift" in error for error in openapi_contract_errors(extra_response))

    missing_component = deepcopy(schema)
    missing_component["components"]["responses"].pop("Wedl400Error")
    assert any("WEDL responses component drift" in error for error in openapi_contract_errors(missing_component))

    extra_component = deepcopy(schema)
    extra_component["components"]["responses"]["Wedl499Error"] = {"description": "unexpected"}
    assert any("responses component allowlist drift" in error for error in openapi_contract_errors(extra_component))

    framework_component = deepcopy(schema)
    framework_component["components"]["schemas"]["HTTPValidationError"] = {"type": "object"}
    assert any("schemas component allowlist drift" in error for error in openapi_contract_errors(framework_component))


def test_parser_metadata_change_propagates_to_openapi(monkeypatch) -> None:
    original = command_parser.parser

    def changed_parser():
        root = original()
        search_action = next(action for action in root._actions if getattr(action, "dest", None) == "command")
        search = search_action.choices["search"]
        limit = next(action for action in search._actions if action.dest == "limit")
        limit.default = 7
        limit.choices = (3, 7)
        limit.help = "Parser-owned limit documentation."
        return root

    monkeypatch.setattr(command_parser, "parser", changed_parser)
    command_contracts.cache_clear()
    try:
        schema = _schema_from_contract_router()
    finally:
        command_contracts.cache_clear()

    limit = next(parameter for parameter in schema["paths"]["/api/search"]["get"]["parameters"] if parameter["name"] == "limit")
    assert limit["schema"]["default"] == 7
    assert limit["schema"]["enum"] == [3, 7]
    assert limit["description"] == "Parser-owned limit documentation."


def test_contract_inherits_global_arguments_and_rejects_unclassified_parser_input(monkeypatch) -> None:
    assert all(any(argument.dest == "compact" for argument in contract.arguments) for contract in command_contracts())
    original = command_parser.parser

    def changed_parser():
        root = original()
        search_action = next(action for action in root._actions if getattr(action, "dest", None) == "command")
        search_action.choices["search"].add_argument("--new-unclassified-option")
        return root

    monkeypatch.setattr(command_parser, "parser", changed_parser)
    command_contracts.cache_clear()
    try:
        try:
            command_contracts()
        except RuntimeError as exc:
            assert "new_unclassified_option" in str(exc)
        else:
            raise AssertionError("new parser input must be explicitly classified for HTTP")
    finally:
        command_contracts.cache_clear()


def test_generated_endpoint_forwards_parser_argument_names_without_a_repository() -> None:
    contract = contract_for_route("GET", "/api/search")
    assert contract is not None
    received: dict[str, object] = {}

    async def handler(**arguments):
        received.update(arguments)
        return {"ok": True}

    endpoint = _endpoint(contract, handler)
    values = {}
    for name, parameter in inspect.signature(endpoint).parameters.items():
        default = getattr(parameter.default, "default", parameter.default)
        values[name] = "amber" if name == "query" else default
    assert asyncio.run(endpoint(**values)) == {"ok": True}
    assert received["query"] == "amber"
    assert received["all_time"] is False
    assert received["limit"] == 20
