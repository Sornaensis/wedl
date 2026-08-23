from __future__ import annotations

from pathlib import Path
from typing import Any, Iterator

import jsonschema
from fastapi.testclient import TestClient

from wedl.server import create_app


_ROOT = Path(__file__).resolve().parents[1]
_ROUTES = {
    ("GET", "/"), ("GET", "/api/session"), ("GET", "/api/status"), ("GET", "/api/validate"),
    ("POST", "/api/compile"), ("GET", "/api/entities"), ("GET", "/api/entities/{entity_id}"),
    ("GET", "/api/entities/{entity_id}/state"), ("GET", "/api/entities/{character_id}/knowledge"),
    ("GET", "/api/interactions"), ("GET", "/api/story-points"), ("GET", "/api/search"),
    ("GET", "/api/timeline"), ("GET", "/api/whereabouts"), ("GET", "/api/causal/{event_id}"),
    ("GET", "/api/context"), ("GET", "/api/conversations/{conversation_id}"),
    ("GET", "/api/changesets/schema"), ("POST", "/api/changesets/scaffold"),
    ("POST", "/api/changesets/preview"), ("POST", "/api/changesets/apply"),
    ("POST", "/api/authoring/preview"), ("POST", "/api/authoring/apply"),
}
_ERRORS = {
    "usage_error", "validation_failed", "confirmation_required", "confirmation_mismatch", "not_found",
    "compile_required", "authentication_required", "conflict", "stale_revision", "dirty_managed_tree",
    "repository_error", "parse_error", "protocol_error",
}
_LOCAL_ONLY = {
    "completion", "init", "serve", "--repo", "--compact", "--output", "--yes", "--use-current-head",
    "--host", "--port", "--open", "--version", "FILE",
}


def _route_table(markdown: str) -> set[tuple[str, str]]:
    rows = set()
    for line in markdown.splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) >= 4 and cells[1] in {"GET", "POST"} and cells[2].startswith("`"):
            rows.add((cells[1], cells[2].strip("`")))
    return rows


def _local_ref(document: dict[str, Any], reference: str) -> Any:
    assert reference.startswith("#/"), f"non-local OpenAPI reference: {reference}"
    target: Any = document
    for part in reference[2:].split("/"):
        target = target[part.replace("~1", "/").replace("~0", "~")]
    return target


def _walk_refs(document: dict[str, Any], value: Any, stack: tuple[str, ...] = ()) -> Iterator[str]:
    if isinstance(value, dict):
        if "$ref" in value:
            reference = value["$ref"]
            assert isinstance(reference, str)
            yield reference
            # Follow nested local references as well as checking that this
            # immediate target exists.  Repeated/cyclic references are still
            # reachable; stop descending only along the active chain.
            if reference not in stack:
                yield from _walk_refs(document, _local_ref(document, reference), (*stack, reference))
        for key, child in value.items():
            if key != "$ref":
                yield from _walk_refs(document, child, stack)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_refs(document, child, stack)


def _validate(document: dict[str, Any], schema: dict[str, Any], value: object) -> None:
    jsonschema.validate(value, {"components": document["components"], **schema})


def test_http_api_documentation_has_the_exact_public_route_error_and_local_only_inventory() -> None:
    markdown = (_ROOT / "docs" / "HTTP_API.md").read_text(encoding="utf-8")
    assert _route_table(markdown) == _ROUTES
    assert len(_route_table(markdown)) == 23
    assert all(f"`{error}`" in markdown for error in _ERRORS)
    assert all(f"`{item}`" in markdown for item in _LOCAL_ONLY)


def test_openapi_refs_examples_and_union_branches_are_client_complete(frontiersmen_repo) -> None:
    with TestClient(create_app(frontiersmen_repo.root)) as client:
        document = client.get("/openapi.json").json()

    operations = {
        (method.upper(), path): operation
        for path, item in document["paths"].items()
        for method, operation in item.items()
        if method in {"get", "post"}
    }
    assert set(operations) == _ROUTES
    references = list(_walk_refs(document, {"paths": document["paths"], "components": document["components"]}))
    assert references
    assert all(_local_ref(document, reference) is not None for reference in references)

    for _route, operation in operations.items():
        request = operation.get("requestBody")
        recipe_bodies = [example["value"]["body"] for example in operation.get("x-wedl-examples", []) if "body" in example["value"]]
        if request is None:
            assert not recipe_bodies
            continue
        media = request["content"]["application/json"]
        standard_bodies = [example["value"] for example in media["examples"].values()]
        assert standard_bodies == recipe_bodies
        for value in standard_bodies:
            _validate(document, media["schema"], value)

    schemas = document["components"]["schemas"]
    context = schemas["ContextResponse"]["oneOf"]
    assert {branch["properties"]["perspective"]["const"] for branch in context} == {"author", "character", "dramatic-irony"}
    conversation = schemas["ConversationResponse"]["oneOf"]
    assert {branch["properties"]["perspective"]["const"] for branch in conversation} == {"author", "character"}
    preview = schemas["ChangesetPreviewResponse"]["oneOf"]
    assert {branch["properties"]["valid"]["const"] for branch in preview} == {True, False}
