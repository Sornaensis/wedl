from __future__ import annotations

from copy import deepcopy
from typing import Any

import jsonschema
from fastapi.testclient import TestClient

from wedl.server import create_app


def _validate(document: dict[str, Any], schema: dict[str, Any], value: object) -> None:
    """Validate a discovered value while retaining OpenAPI local components."""

    jsonschema.validate(value, {"components": document["components"], **schema})


def test_a_client_can_discover_and_execute_the_safe_changeset_workflow(frontiersmen_repo) -> None:
    """Exercise discovery like an external client, without contract internals."""

    with TestClient(create_app(frontiersmen_repo.root)) as client:
        session = client.get("/api/session")
        assert session.status_code == 200
        token = session.json()["token"]
        document = client.get("/openapi.json").json()

        scaffold_operation = document["paths"]["/api/changesets/scaffold"]["post"]
        assert "requestBody" not in scaffold_operation
        auth = next(
            parameter
            for parameter in scaffold_operation["parameters"]
            if parameter["in"] == "header" and parameter["name"] == "X-Wedl-Token"
        )
        assert auth["required"] is False

        scaffold = client.post("/api/changesets/scaffold", headers={"X-Wedl-Token": token})
        assert scaffold.status_code == 200
        payload = scaffold.json()

        preview_operation = document["paths"]["/api/changesets/preview"]["post"]
        request = preview_operation["requestBody"]
        assert request["required"] is True
        media = request["content"]["application/json"]
        assert media["schema"] == {"$ref": "#/components/schemas/ChangesetRequest"}
        assert "payload" not in media["schema"]
        _validate(document, media["schema"], payload)

        preview = client.post("/api/changesets/preview", headers={"X-Wedl-Token": token}, json=payload)
        assert preview.status_code == 200
        preview_value = preview.json()
        assert "_changes" not in preview_value
        _validate(document, preview_operation["responses"]["200"]["content"]["application/json"]["schema"], preview_value)

        invalid_payload = deepcopy(payload)
        invalid_payload["operations"][0]["frontmatterPatch"] = {"id": "not-the-record-key"}
        invalid_preview = client.post(
            "/api/changesets/preview", headers={"X-Wedl-Token": token}, json=invalid_payload
        )
        assert invalid_preview.status_code == 200
        assert invalid_preview.json()["valid"] is False
        _validate(
            document,
            preview_operation["responses"]["200"]["content"]["application/json"]["schema"],
            invalid_preview.json(),
        )

        unauthenticated = client.post("/api/changesets/preview", json=payload)
        assert unauthenticated.status_code == 401
        assert unauthenticated.json()["code"] == "authentication_required"
        _validate(
            document,
            preview_operation["responses"]["401"]["content"]["application/json"]["schema"],
            unauthenticated.json(),
        )

        # Deliberately stop before a confirmed apply: this documents the safe
        # client failure without creating a repository commit in the test.
        apply_operation = document["paths"]["/api/changesets/apply"]["post"]
        missing_confirmation = client.post("/api/changesets/apply", headers={"X-Wedl-Token": token}, json=payload)
        assert missing_confirmation.status_code == 400
        assert missing_confirmation.json()["code"] == "confirmation_required"
        _validate(
            document,
            apply_operation["responses"]["400"]["content"]["application/json"]["schema"],
            missing_confirmation.json(),
        )
