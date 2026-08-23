from __future__ import annotations

from copy import deepcopy

from fastapi.testclient import TestClient

from wedl import changeset
from wedl.server import Runtime, create_app


def _payload(repository, *, key: str, tag: str) -> dict:
    record = next(item for item in repository.load_world().by_kind("object"))
    return {
        "protocol": "wedl-changeset/v1",
        "expectedHead": repository.head(),
        "idempotencyKey": key,
        "requestId": "api-workflow-request-1",
        "summary": "exercise the HTTP changeset workflow",
        "operations": [
            {
                "type": "entity.update",
                "entity": record.id,
                "frontmatterPatch": {"tags": [*record.tags, tag]},
            }
        ],
    }


def test_schema_is_public_and_scaffold_requires_the_session_token(frontiersmen_repo, monkeypatch) -> None:
    broadcasts: list[dict] = []

    async def capture_broadcast(_runtime, payload: dict) -> None:
        broadcasts.append(payload)

    monkeypatch.setattr(Runtime, "broadcast", capture_broadcast)
    with TestClient(create_app(frontiersmen_repo.root)) as client:
        schema = client.get("/api/changesets/schema")
        assert schema.status_code == 200
        assert schema.json()["protocol"] == "wedl-changeset-schema/v1"

        assert client.post("/api/changesets/scaffold").status_code == 401
        token = client.get("/api/session").json()["token"]
        scaffold = client.post("/api/changesets/scaffold", headers={"X-Wedl-Token": token})
        assert scaffold.status_code == 200
        assert scaffold.json()["protocol"] == "wedl-changeset/v1"
        assert scaffold.json()["expectedHead"] == frontiersmen_repo.head()

        # Compile uses parser-derived query parameters, requires the same
        # session token, and deliberately does not announce a revision.
        assert client.post("/api/compile").status_code == 401
        assert client.post("/api/compile", headers={"X-Wedl-Token": token}).status_code == 200
        assert broadcasts == []


def test_raw_http_changeset_confirmation_replay_and_broadcast(frontiersmen_repo, monkeypatch) -> None:
    broadcasts: list[dict] = []
    receipt_writes = []
    atomic_write = changeset.atomic_write

    async def capture_broadcast(_runtime, payload: dict) -> None:
        assert (frontiersmen_repo.root / ".wedl" / "idempotency.json").exists()
        broadcasts.append(payload)

    def capture_receipt(path, data) -> None:
        receipt_writes.append(path)
        atomic_write(path, data)

    monkeypatch.setattr(Runtime, "broadcast", capture_broadcast)
    monkeypatch.setattr(changeset, "atomic_write", capture_receipt)
    payload = _payload(frontiersmen_repo, key="api-workflow-replay-v1", tag="api-workflow-route")

    with TestClient(create_app(frontiersmen_repo.root)) as client:
        session = client.get("/api/session").json()
        headers = {"X-Wedl-Token": session["token"]}
        assert client.post("/api/changesets/preview", json=payload).status_code == 401
        preview = client.post("/api/changesets/preview", json=payload, headers=headers)
        assert preview.status_code == 200
        plan = preview.json()
        assert plan["valid"] is True
        assert "_changes" not in plan

        # A body field is only an untrusted extension; the header remains the
        # sole HTTP confirmation channel.
        body_confirmation = {**payload, "confirmationToken": plan["confirmationToken"]}
        missing = client.post("/api/changesets/apply", json=body_confirmation, headers=headers)
        assert missing.status_code == 400
        assert missing.json()["code"] == "confirmation_required"

        wrong = client.post(
            "/api/changesets/apply",
            json=payload,
            headers={**headers, "X-Wedl-Confirmation": "wedl-confirmation/v1:wrong"},
        )
        assert wrong.status_code == 400
        assert wrong.json()["code"] == "confirmation_mismatch"

        applied = client.post(
            "/api/changesets/apply",
            json=payload,
            headers={**headers, "X-Wedl-Confirmation": plan["confirmationToken"]},
        )
        assert applied.status_code == 200
        result = applied.json()
        assert result["status"] == "committed"
        assert result["idempotentReplay"] is False
        assert receipt_writes == [frontiersmen_repo.root / ".wedl" / "idempotency.json"]
        assert broadcasts == [{"type": "revision", "head": result["newHead"], "compile": result["compile"]}]

        replay = client.post(
            "/api/changesets/apply",
            json=payload,
            headers={**headers, "X-Wedl-Confirmation": plan["confirmationToken"]},
        )
        assert replay.status_code == 200
        assert replay.json() == {**result, "idempotentReplay": True}
        assert broadcasts == [{"type": "revision", "head": result["newHead"], "compile": result["compile"]}]

        changed = deepcopy(payload)
        changed["requestId"] = "api-workflow-request-2"
        conflict = client.post(
            "/api/changesets/apply",
            json=changed,
            headers={**headers, "X-Wedl-Confirmation": plan["confirmationToken"]},
        )
        assert conflict.status_code == 409
        assert conflict.json()["code"] == "conflict"


def test_invalid_preview_is_reported_without_becoming_an_apply(frontiersmen_repo) -> None:
    payload = _payload(frontiersmen_repo, key="api-workflow-invalid-v1", tag="api-workflow-invalid")
    payload["operations"][0]["frontmatterPatch"] = {"id": "not-the-record-key"}

    with TestClient(create_app(frontiersmen_repo.root)) as client:
        token = client.get("/api/session").json()["token"]
        headers = {"X-Wedl-Token": token}
        preview = client.post("/api/changesets/preview", json=payload, headers=headers)
        assert preview.status_code == 200
        assert preview.json()["valid"] is False

        apply = client.post(
            "/api/changesets/apply",
            json=payload,
            headers={**headers, "X-Wedl-Confirmation": preview.json()["confirmationToken"]},
        )
        assert apply.status_code == 400
        assert apply.json()["code"] == "validation_failed"
