"""The mounted WEDL HTTP routes.

The parser-derived contracts provide the public command parameter grammar.
Handlers below only translate those already-parsed values into application
calls; :func:`_endpoint` gives FastAPI a generated signature so OpenAPI cannot
drift from argparse defaults, choices, aliases, or numeric constraints.
"""

from __future__ import annotations

import asyncio
import inspect
from typing import Any, Awaitable, Callable, Literal

from fastapi import APIRouter, Body, Depends, Header, Path, Query
from fastapi.responses import JSONResponse

from .api_contract import (
    ArgumentContract,
    AuthPolicy,
    CommandContract,
    Transport,
    control_endpoint,
    discovery_components,
    discovery_openapi_extra,
    discovery_request_body,
    discovery_responses,
    explorer_endpoints,
    explorer_request_body,
    route_contracts,
    validate_discovery_descriptors,
)
from .changeset import apply as apply_changeset, preview as preview_changeset, scaffold as scaffold_changeset, schema as changeset_schema
from .authoring import apply_intent, preview_intent
from .compiler import compile_world
from .context import build_context
from .chronology_api import catalog as chronology_catalog, convert_date as chronology_convert, format_date as chronology_format, search_annotations as chronology_search, story_times as chronology_story_times
from .spatial_api import execute as spatial_execute, status_code as spatial_status_code
from .spatial_explorer_api import execute as explorer_execute, status_code as explorer_status_code
from .generational_api import OPERATIONS as GENERATIONAL_OPERATIONS, execute as generational_execute, status_code as generational_status_code
from .generational_authoring import scaffold as generational_scaffold, schema as generational_schema
from .query import (
    causality,
    conversation_view,
    entity_state,
    hypotheses,
    interactions_between,
    knowledge,
    list_entities,
    search_world,
    show_entity,
    status,
    story_points,
    thread_catalog,
    thread_memberships,
    timeline,
    validation_report,
    whereabouts,
)


Authorize = Callable[..., Awaitable[None]]
Handler = Callable[..., Awaitable[Any]]


def openapi_contract_errors(schema: dict[str, Any]) -> tuple[str, ...]:
    """Return normalized, two-way production OpenAPI contract mismatches."""

    missing = object()

    def parameter_value(name: str, location: str, required: bool, value_type: str | None, *, enum: Any = None, default: Any = missing, minimum: Any = None, maximum: Any = None, description: Any = None) -> dict[str, Any]:
        return {
            "name": name, "in": location, "required": required, "type": value_type,
            "enum": None if enum is None else tuple(enum), "default": default,
            "minimum": minimum, "maximum": maximum, "description": description,
        }

    def argument_value(argument: ArgumentContract) -> dict[str, Any]:
        assert argument.transport_name is not None
        location = {Transport.QUERY: "query", Transport.PATH: "path", Transport.HEADER: "header"}[argument.transport]
        default = missing if argument.required or argument.default is None else argument.default
        value_type = "array" if argument.repeated else {"boolean": "boolean", "integer": "integer", "number": "number"}.get(argument.value_type, "string")
        return parameter_value(
            argument.transport_name, location, argument.required or location == "path", value_type,
            enum=argument.choices, default=default, minimum=argument.minimum,
            maximum=argument.maximum, description=argument.transport_description,
        )

    def normalized_parameter(parameter: dict[str, Any]) -> dict[str, Any]:
        value = parameter.get("schema", {})
        alternatives = value.get("anyOf") or []
        value_type = value.get("type") or next(
            (item.get("type") for item in alternatives if item.get("type") != "null"),
            None,
        )
        enum = value.get("enum")
        if enum is None:
            enum = next((item.get("enum") for item in alternatives if item.get("enum") is not None), None)
        if enum is None and alternatives and all("const" in item for item in alternatives):
            enum = [item["const"] for item in alternatives]
        return parameter_value(
            parameter.get("name"), parameter.get("in"), bool(parameter.get("required")), value_type,
            enum=enum, default=value.get("default", missing), minimum=value.get("minimum"),
            maximum=value.get("maximum"), description=parameter.get("description", value.get("description")),
        )

    def discovery_value(descriptor: Any, *, auth: AuthPolicy, command: tuple[str, ...] | None = None) -> dict[str, Any]:
        return {
            "summary": descriptor.summary,
            "description": descriptor.description,
            "tags": list(descriptor.tags),
            "responses": {str(status): value for status, value in discovery_responses(descriptor).items()},
            "extra": discovery_openapi_extra(descriptor, command=command, auth=auth),
        }

    session = control_endpoint("GET", "/api/session")
    root = control_endpoint("GET", "/")
    expected: dict[tuple[str, str], dict[str, Any]] = {
        ("/api/session", "get"): {
            "operationId": "session_get", "command": None, "auth": session.auth.value, "parameters": [], "body": None,
            **discovery_value(session.descriptor, auth=session.auth),
        },
        ("/", "get"): {
            "operationId": "root_get", "command": None, "auth": root.auth.value, "parameters": [], "body": None,
            **discovery_value(root.descriptor, auth=root.auth),
        },
    }
    for contract in route_contracts():
        assert contract.binding is not None
        assert contract.discovery is not None
        key = (contract.binding.path, contract.binding.method.lower())
        if key in expected:
            expected[key] = {"duplicate": contract.command}
            continue
        parameters = [
            argument_value(argument)
            for argument in contract.arguments
            if argument.transport in {Transport.QUERY, Transport.PATH, Transport.HEADER}
        ]
        if contract.binding.auth == AuthPolicy.SESSION:
            parameters.append(parameter_value(
                AuthPolicy.SESSION.header_name or "", "header", False, "string",
                description=AuthPolicy.SESSION.header_description,
            ))
        body = next((argument for argument in contract.arguments if argument.transport == Transport.BODY), None)
        expected[key] = {
            "operationId": "_".join((*contract.command, key[1])).replace("-", "_"),
            "command": list(contract.command), "auth": contract.binding.auth.value,
            "parameters": parameters,
            "body": None if body is None else discovery_request_body(contract.discovery, body),
            **discovery_value(contract.discovery, auth=contract.binding.auth, command=contract.command),
        }

    for endpoint in explorer_endpoints():
        key = (endpoint.path, endpoint.method.lower())
        if key in expected:
            expected[key] = {"duplicate": key}
            continue
        params = []
        if endpoint.method == "GET":
            params = [
                parameter_value("limit", "query", False, "integer", default=20, minimum=1, maximum=100),
                parameter_value("cursor", "query", False, "string"),
                parameter_value("revision", "query", False, "string"),
                parameter_value("capabilities", "query", False, "array"),
            ]
        expected[key] = {
            "operationId": f"spatial_explorer_{endpoint.path.rsplit('/', 1)[-1]}_{key[1]}",
            "command": None, "auth": endpoint.auth.value, "parameters": params,
            "body": None if endpoint.method == "GET" else explorer_request_body(endpoint.descriptor),
            **discovery_value(endpoint.descriptor, auth=endpoint.auth),
        }

    errors: list[str] = []
    try:
        validate_discovery_descriptors()
    except RuntimeError as exc:
        errors.append(f"discovery descriptor validation failed: {exc}")
    expected_components_by_category = discovery_components()
    actual_components_by_category = schema.get("components", {})
    if set(actual_components_by_category) != set(expected_components_by_category):
        errors.append("OpenAPI component category allowlist drift")
    for category, expected_components in expected_components_by_category.items():
        actual_components = actual_components_by_category.get(category, {})
        if set(actual_components) != set(expected_components):
            errors.append(f"OpenAPI {category} component allowlist drift")
        for name, component in expected_components.items():
            if actual_components.get(name) != component:
                errors.append(f"OpenAPI WEDL {category} component drift for {name}")
    if any("duplicate" in descriptor for descriptor in expected.values()):
        errors.append("contract contains duplicate HTTP method/path bindings")
    actual: dict[tuple[str, str], dict[str, Any]] = {}
    methods = {"get", "post", "put", "patch", "delete", "head", "options", "trace"}
    for path, path_item in schema.get("paths", {}).items():
        for method, operation in path_item.items():
            if method not in methods:
                errors.append(f"OpenAPI exposes unsupported path-item control {method!r} at {path}")
                continue
            actual[(path, method)] = operation
    for key in sorted(set(actual).difference(expected)):
        errors.append(f"OpenAPI exposes uncontracted operation {key[1].upper()} {key[0]}")
    for key in sorted(set(expected).difference(actual)):
        errors.append(f"OpenAPI omits contracted operation {key[1].upper()} {key[0]}")
    for key in sorted(set(expected).intersection(actual)):
        descriptor, operation = expected[key], actual[key]
        if "duplicate" in descriptor:
            continue
        method, path = key[1].upper(), key[0]
        if operation.get("operationId") != descriptor.get("operationId"):
            errors.append(f"OpenAPI operation ID drift for {method} {path}")
        if operation.get("summary") != descriptor["summary"]:
            errors.append(f"OpenAPI summary drift for {method} {path}")
        if operation.get("description") != descriptor["description"]:
            errors.append(f"OpenAPI description drift for {method} {path}")
        if operation.get("tags", []) != descriptor["tags"]:
            errors.append(f"OpenAPI tags drift for {method} {path}")
        actual_responses = operation.get("responses", {})
        if set(actual_responses) != set(descriptor["responses"]):
            errors.append(f"OpenAPI response status drift for {method} {path}")
        for response_status, expected_response in descriptor["responses"].items():
            actual_response = actual_responses.get(response_status)
            if actual_response is None:
                errors.append(f"OpenAPI response descriptor missing for {method} {path}: {response_status}")
            elif response_status != "200":
                if actual_response != expected_response or "$ref" in actual_response:
                    errors.append(f"OpenAPI response descriptor drift for {method} {path}: {response_status}")
            elif actual_response.get("description") != expected_response["description"]:
                errors.append(f"OpenAPI response descriptor drift for {method} {path}: {response_status}")
            else:
                expected_content = expected_response.get("content", {})
                actual_content = actual_response.get("content", {})
                if set(actual_content) != set(expected_content):
                    errors.append(f"OpenAPI success media type drift for {method} {path}: {response_status}")
                    continue
                for media_type, expected_media in expected_content.items():
                    actual_media = actual_content.get(media_type, {})
                    expected_schema = expected_media.get("schema", {})
                    actual_schema = actual_media.get("schema", {})
                    schema_drift = (actual_schema != expected_schema if "oneOf" in expected_schema
                                    else actual_schema.get("$ref") != expected_schema.get("$ref"))
                    if schema_drift:
                        errors.append(f"OpenAPI success schema reference drift for {method} {path}: {response_status}")
                    if actual_media.get("examples") != expected_media.get("examples"):
                        errors.append(f"OpenAPI success examples drift for {method} {path}: {response_status}")
        for extra_name, extra_value in descriptor["extra"].items():
            if operation.get(extra_name) != extra_value:
                errors.append(f"OpenAPI {extra_name} extension drift for {method} {path}")
        if operation.get("x-wedl-auth-policy") != descriptor.get("auth"):
            errors.append(f"OpenAPI auth extension drift for {method} {path}")
        if descriptor.get("command") is not None and operation.get("x-wedl-command") != descriptor["command"]:
            errors.append(f"OpenAPI command extension drift for {method} {path}")
        if descriptor.get("command") is None and "x-wedl-command" in operation:
            errors.append(f"OpenAPI unexpected command extension for {method} {path}")
        parameters = [normalized_parameter(parameter) for parameter in operation.get("parameters", [])]
        names = [(parameter["name"], parameter["in"]) for parameter in parameters]
        if len(names) != len(set(names)):
            errors.append(f"OpenAPI contains duplicate parameter bindings for {method} {path}")
        if sorted(parameters, key=lambda item: (item["in"], item["name"])) != sorted(descriptor["parameters"], key=lambda item: (item["in"], item["name"])):
            errors.append(
                f"OpenAPI parameter descriptor drift for {method} {path}: "
                f"expected={sorted(descriptor['parameters'], key=lambda item: (item['in'], item['name']))!r}, "
                f"actual={sorted(parameters, key=lambda item: (item['in'], item['name']))!r}"
            )
        body = descriptor["body"]
        request_body = operation.get("requestBody")
        if body is None:
            if request_body is not None:
                errors.append(f"OpenAPI unexpected request body for {method} {path}")
        elif request_body is None:
            errors.append(f"OpenAPI omits raw request body for {method} {path}")
        else:
            if request_body != body:
                errors.append(f"OpenAPI raw unwrapped body descriptor drift for {method} {path}")
    return tuple(errors)


def _annotation(argument: ArgumentContract) -> Any:
    """Return the Python annotation FastAPI should expose for an argument."""

    primitive: Any = {"boolean": bool, "integer": int, "number": float}.get(argument.value_type, str)
    # Literal supplies both a precise OpenAPI enum and FastAPI's normal value
    # coercion. Parser validation remains the final authority in middleware.
    annotation = Literal.__getitem__(argument.choices) if argument.choices is not None else primitive
    if argument.repeated:
        annotation = list[annotation]
    if not argument.required and argument.default is None:
        return annotation | None
    return annotation


def _field(argument: ArgumentContract, default: Any) -> Any:
    """Build a FastAPI field from parser metadata without duplicating it."""

    keywords: dict[str, Any] = {"description": argument.transport_description}
    if argument.minimum is not None:
        keywords["ge"] = argument.minimum
    if argument.maximum is not None:
        keywords["le"] = argument.maximum
    if argument.transport == Transport.QUERY:
        assert argument.transport_name is not None
        return Query(default, alias=argument.transport_name, **keywords)
    if argument.transport == Transport.PATH:
        return Path(..., **keywords)
    if argument.transport == Transport.BODY:
        return Body(default, **keywords)
    if argument.transport == Transport.HEADER:
        assert argument.transport_name is not None
        return Header(default, alias=argument.transport_name, **keywords)
    raise ValueError(f"cannot create an HTTP field for {argument.transport}")


def _parameters(
    contract: CommandContract,
    extra: tuple[inspect.Parameter, ...] = (),
) -> tuple[inspect.Parameter, ...]:
    """Synthesize route parameters from its exposed parser arguments.

    Path argument names intentionally use the route placeholder rather than
    their CLI destination; the wrapper maps them back before calling a handler.
    """

    parameters: list[inspect.Parameter] = []
    for argument in contract.arguments:
        if argument.transport not in {Transport.QUERY, Transport.PATH, Transport.BODY, Transport.HEADER}:
            continue
        assert argument.transport_name is not None
        name = argument.transport_name if argument.transport in {Transport.PATH, Transport.BODY} else argument.dest
        default = ... if argument.required else argument.default
        parameters.append(
            inspect.Parameter(
                name,
                inspect.Parameter.KEYWORD_ONLY,
                default=_field(argument, default),
                annotation=dict[str, Any] if argument.transport == Transport.BODY else _annotation(argument),
            )
        )
    return (*parameters, *extra)


def _endpoint(
    contract: CommandContract,
    handler: Handler,
    *,
    extra: tuple[inspect.Parameter, ...] = (),
) -> Handler:
    """Wrap a behaviour-only handler in the parser-derived FastAPI signature."""

    exposed = tuple(argument for argument in contract.arguments if argument.transport in {Transport.QUERY, Transport.PATH, Transport.BODY, Transport.HEADER})

    async def endpoint(**values: Any) -> Any:
        arguments: dict[str, Any] = {}
        for argument in exposed:
            assert argument.transport_name is not None
            name = argument.transport_name if argument.transport in {Transport.PATH, Transport.BODY} else argument.dest
            arguments[argument.dest] = values[name]
        arguments.update({parameter.name: values[parameter.name] for parameter in extra})
        return await handler(**arguments)

    endpoint.__name__ = "_".join(contract.command).replace("-", "_") + "_endpoint"
    endpoint.__signature__ = inspect.Signature(
        _parameters(contract, extra),
        return_annotation=inspect.signature(handler).return_annotation,
    )
    return endpoint


def create_api_router(runtime: Any, authorize: Authorize) -> APIRouter:
    """Create the current HTTP surface from the parser-derived route matrix."""

    router = APIRouter()
    bindings = {
        contract.command: contract
        for contract in route_contracts()
        if contract.binding is not None
    }

    def operation(contract: CommandContract) -> str:
        assert contract.binding is not None
        return "_".join((*contract.command, contract.binding.method.lower())).replace("-", "_")

    def mount(
        command: tuple[str, ...],
        handler: Handler,
    ) -> None:
        contract = bindings[command]
        assert contract.binding is not None
        assert contract.discovery is not None
        dependencies = [Depends(authorize)] if contract.binding.auth == AuthPolicy.SESSION else []
        extra = discovery_openapi_extra(
            contract.discovery,
            command=contract.command,
            auth=contract.binding.auth,
        )
        body = next((argument for argument in contract.arguments if argument.transport == Transport.BODY), None)
        if body is not None:
            extra["requestBody"] = discovery_request_body(contract.discovery, body)
        router.add_api_route(
            contract.binding.path,
            _endpoint(contract, handler),
            methods=[contract.binding.method],
            dependencies=dependencies,
            operation_id=operation(contract),
            summary=contract.discovery.summary,
            description=contract.discovery.description,
            tags=list(contract.discovery.tags),
            responses=discovery_responses(contract.discovery),
            openapi_extra=extra,
        )

    session_contract = control_endpoint("GET", "/api/session")

    @router.get(
        "/api/session",
        operation_id="session_get",
        summary=session_contract.descriptor.summary,
        description=session_contract.descriptor.description,
        tags=list(session_contract.descriptor.tags),
        responses=discovery_responses(session_contract.descriptor),
        openapi_extra=discovery_openapi_extra(session_contract.descriptor, auth=session_contract.auth),
    )
    async def session() -> dict[str, Any]:
        return {"token": runtime.token, "head": runtime.repository.head()}

    async def status_handler() -> dict[str, Any]:
        return await asyncio.to_thread(status, runtime.repository)

    async def validate_handler() -> dict[str, Any]:
        return await asyncio.to_thread(validation_report, runtime.repository)

    async def compile_handler(**arguments: Any) -> dict[str, Any]:
        async with runtime.write_lock:
            return await asyncio.to_thread(
                compile_world,
                runtime.repository,
                "HEAD",
                force=arguments["force"],
                profile_name=arguments["profile"],
                vector_provider=arguments["vector_provider"],
                vector_model=arguments["vector_model"],
                vector_dimensions=arguments["vector_dimensions"],
                vector_max_features=arguments["vector_max_features"],
            )

    async def entities_handler(**arguments: Any) -> list[dict[str, Any]]:
        return await asyncio.to_thread(
            list_entities,
            runtime.repository,
            arguments["kind"],
            arguments["text"],
            require_compiled=arguments["require_compiled"],
        )

    async def entity_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            show_entity,
            runtime.repository,
            arguments["entity"],
            require_compiled=arguments["require_compiled"],
        )

    async def state_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            entity_state,
            runtime.repository,
            arguments["entity"],
            arguments["tick"],
            arguments["timeline"],
            arguments["order"],
            require_compiled=arguments["require_compiled"],
        )

    async def knowledge_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            knowledge,
            runtime.repository,
            arguments["character"],
            arguments["tick"],
            arguments["timeline"],
            arguments["order"],
            require_compiled=arguments["require_compiled"],
        )

    async def interactions_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            interactions_between,
            runtime.repository,
            arguments["first"],
            arguments["second"],
            require_compiled=arguments["require_compiled"],
        )

    async def search_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            search_world,
            runtime.repository,
            arguments["query"],
            perspective=arguments["perspective"],
            character_id=arguments["character"],
            scene_id=arguments["scene"],
            mode=arguments["mode"],
            limit=arguments["limit"],
            timeline=arguments["timeline"],
            tick=arguments["tick"],
            order=arguments["order"],
            all_time=arguments["all_time"],
            include_text=arguments["include_text"],
            include_hypotheses=arguments["include_hypotheses"],
            thread_ids=None if arguments["thread_ids"] is None else tuple(arguments["thread_ids"]),
            require_compiled=arguments["require_compiled"],
        )

    async def context_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            build_context,
            runtime.repository,
            character_id=arguments["character"],
            scene_id=arguments["scene"],
            perspective=arguments["perspective"],
            query=arguments["query"],
            search_mode=arguments["mode"],
            max_characters=arguments["max_characters"],
            max_items=arguments["max_items"],
            timeline=arguments["timeline"],
            tick=arguments["tick"],
            order=arguments["order"],
            _thread_filter_ids=None if arguments["recall_thread_ids"] is None else tuple(arguments["recall_thread_ids"]),
            require_compiled=arguments["require_compiled"],
        )

    async def conversation_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            conversation_view,
            runtime.repository,
            arguments["conversation"],
            perspective=arguments["perspective"],
            character_id=arguments["character"],
            timeline=arguments["timeline"],
            tick=arguments["tick"],
            order=arguments["order"],
            all_time=arguments["all_time"],
            require_compiled=arguments["require_compiled"],
        )

    async def story_points_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            story_points,
            runtime.repository,
            arguments["scene"],
            arguments["tick"],
            arguments["timeline"],
            arguments["order"],
            require_compiled=arguments["require_compiled"],
        )

    async def timeline_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            timeline,
            runtime.repository,
            arguments["timeline"],
            require_compiled=arguments["require_compiled"],
        )

    async def chronology_catalog_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(chronology_catalog, runtime.repository, require_compiled=arguments["require_compiled"])

    async def chronology_format_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(chronology_format, runtime.repository, arguments["file"], require_compiled=arguments["require_compiled"])

    async def chronology_convert_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(chronology_convert, runtime.repository, arguments["file"], require_compiled=arguments["require_compiled"])

    async def chronology_search_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(chronology_search, runtime.repository, arguments["file"], require_compiled=arguments["require_compiled"])

    async def chronology_story_times_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(chronology_story_times, runtime.repository, arguments["file"], require_compiled=arguments["require_compiled"])

    async def spatial_handler(operation: str, **arguments: Any) -> Any:
        value = await asyncio.to_thread(spatial_execute, runtime.repository, operation, arguments["file"], require_compiled=arguments["require_compiled"])
        return JSONResponse(value, status_code=spatial_status_code(value))

    async def generational_handler(operation: str, **arguments: Any) -> Any:
        value = await asyncio.to_thread(generational_execute, runtime.repository, operation,
                                        arguments["file"], require_compiled=arguments["require_compiled"])
        return JSONResponse(value, status_code=generational_status_code(value))

    async def generational_scaffold_handler() -> dict[str, Any]:
        async with runtime.write_lock:
            return await asyncio.to_thread(generational_scaffold, runtime.repository)

    async def generational_schema_handler() -> dict[str, Any]:
        return generational_schema()

    async def threads_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            thread_catalog,
            runtime.repository,
            require_compiled=arguments["require_compiled"],
        )

    async def thread_memberships_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            thread_memberships,
            runtime.repository,
            tuple(arguments["record_ids"]),
            tuple(arguments["thread_ids"]),
            require_compiled=arguments["require_compiled"],
        )

    async def whereabouts_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            whereabouts,
            runtime.repository,
            arguments["character"],
            arguments["tick"],
            arguments["timeline"],
            arguments["order"],
            require_compiled=arguments["require_compiled"],
        )

    async def hypotheses_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            hypotheses, runtime.repository, arguments["hypothesis"],
            status=arguments["status"], text=arguments["text"],
            require_compiled=arguments["require_compiled"],
        )

    async def causal_handler(**arguments: Any) -> dict[str, Any]:
        return await asyncio.to_thread(
            causality,
            runtime.repository,
            arguments["event"],
            direction=arguments["direction"],
            tick=arguments["tick"],
            timeline=arguments["timeline"],
            order=arguments["order"],
            require_compiled=arguments["require_compiled"],
        )

    async def preview_handler(**arguments: Any) -> dict[str, Any]:
        async with runtime.write_lock:
            result = await asyncio.to_thread(preview_changeset, runtime.repository, arguments["file"], use_current_head=True)
            result.pop("_changes", None)
            return result

    async def scaffold_handler() -> dict[str, Any]:
        async with runtime.write_lock:
            return await asyncio.to_thread(scaffold_changeset, runtime.repository)

    async def schema_handler() -> dict[str, Any]:
        return changeset_schema()

    async def apply_handler(**arguments: Any) -> dict[str, Any]:
        async with runtime.write_lock:
            result = await asyncio.to_thread(
                apply_changeset,
                runtime.repository,
                arguments["file"],
                use_current_head=True,
                confirmation_token_value=arguments["confirm"],
            )
            if not result["idempotentReplay"]:
                runtime.last_head = result["newHead"]
                await runtime.broadcast(
                    {"type": "revision", "head": result["newHead"], "compile": result["compile"]}
                )
            return result

    async def author_preview_handler(**arguments: Any) -> dict[str, Any]:
        async with runtime.write_lock:
            return await asyncio.to_thread(preview_intent, runtime.repository, arguments["file"])

    async def author_apply_handler(**arguments: Any) -> dict[str, Any]:
        async with runtime.write_lock:
            result = await asyncio.to_thread(
                apply_intent, runtime.repository, arguments["file"],
                confirmation_token_value=arguments["confirm"],
            )
            if not result["idempotentReplay"]:
                runtime.last_head = result["newHead"]
                await runtime.broadcast({"type": "revision", "head": result["newHead"], "compile": result["compile"]})
            return result

    behaviors: dict[tuple[str, ...], Handler] = {
        ("status",): status_handler,
        ("validate",): validate_handler,
        ("compile",): compile_handler,
        ("entity", "list"): entities_handler,
        ("entity", "show"): entity_handler,
        ("state",): state_handler,
        ("knowledge",): knowledge_handler,
        ("interactions",): interactions_handler,
        ("search",): search_handler,
        ("context",): context_handler,
        ("conversation", "show"): conversation_handler,
        ("story-points",): story_points_handler,
        ("timeline",): timeline_handler,
        ("chronology", "catalog"): chronology_catalog_handler,
        ("chronology", "format"): chronology_format_handler,
        ("chronology", "convert"): chronology_convert_handler,
        ("chronology", "search"): chronology_search_handler,
        ("chronology", "story-times"): chronology_story_times_handler,
        ("spatial", "containment"): lambda **arguments: spatial_handler("containment", **arguments),
        ("spatial", "children"): lambda **arguments: spatial_handler("children", **arguments),
        ("spatial", "bbox"): lambda **arguments: spatial_handler("bbox", **arguments),
        ("spatial", "nearby"): lambda **arguments: spatial_handler("nearby", **arguments),
        ("spatial", "adjacency"): lambda **arguments: spatial_handler("adjacency", **arguments),
        ("spatial", "reachability"): lambda **arguments: spatial_handler("reachability", **arguments),
        ("spatial", "path"): lambda **arguments: spatial_handler("path", **arguments),
        ("spatial", "overlay-as-of"): lambda **arguments: spatial_handler("overlay-as-of", **arguments),
        **{("generational", action): (lambda action=action, **arguments: generational_handler(action, **arguments)) for action in GENERATIONAL_OPERATIONS},
        ("generational", "scaffold"): generational_scaffold_handler,
        ("generational", "schema"): generational_schema_handler,
        ("threads",): threads_handler,
        ("thread-memberships",): thread_memberships_handler,
        ("whereabouts",): whereabouts_handler,
        ("hypotheses",): hypotheses_handler,
        ("causal",): causal_handler,
        ("changeset", "schema"): schema_handler,
        ("changeset", "scaffold"): scaffold_handler,
        ("changeset", "preview"): preview_handler,
        ("changeset", "apply"): apply_handler,
        ("author", "request", "preview"): author_preview_handler,
        ("author", "request", "apply"): author_apply_handler,
    }
    if set(behaviors) != set(bindings):
        raise RuntimeError(
            f"router behavior registry does not match mounted contracts; "
            f"missing={sorted(set(bindings).difference(behaviors))!r}, "
            f"stale={sorted(set(behaviors).difference(bindings))!r}"
        )
    for command, handler in behaviors.items():
        mount(command, handler)

    def explorer_options(endpoint: Any) -> dict[str, Any]:
        return {"methods": [endpoint.method],
                "operation_id": f"spatial_explorer_{endpoint.path.rsplit('/', 1)[-1]}_{endpoint.method.lower()}",
                "summary": endpoint.descriptor.summary,
                "description": endpoint.descriptor.description,
                "tags": list(endpoint.descriptor.tags),
                "responses": discovery_responses(endpoint.descriptor),
                "openapi_extra": discovery_openapi_extra(endpoint.descriptor, auth=endpoint.auth)}

    for endpoint in explorer_endpoints():
        action = endpoint.path.rsplit("/", 1)[-1]
        if action == "catalog":
            async def explorer_catalog(limit: int = Query(20, ge=1, le=100),
                                       cursor: str | None = Query(None),
                                       revision: str | None = Query(None),
                                       capabilities: list[str] | None = Query(None)) -> Any:
                request: dict[str, Any] = {"protocol": "wedl-spatial-explorer/v1", "limit": limit}
                if cursor is not None: request["cursor"] = cursor
                if revision is not None: request["revision"] = revision
                if capabilities is not None: request["capabilities"] = capabilities
                value = await asyncio.to_thread(explorer_execute, runtime.repository, "catalog", request)
                return JSONResponse(value, status_code=explorer_status_code(value))
            router.add_api_route(endpoint.path, explorer_catalog, **explorer_options(endpoint))
        else:
            def explorer_post(operation: str) -> Handler:
                async def handler(payload: dict[str, Any] = Body(...)) -> Any:
                    value = await asyncio.to_thread(explorer_execute, runtime.repository, operation, payload)
                    return JSONResponse(value, status_code=explorer_status_code(value))
                return handler
            router.add_api_route(endpoint.path, explorer_post(action), **explorer_options(endpoint))

    return router
