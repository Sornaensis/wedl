from __future__ import annotations

import io
import json
import sys
from copy import deepcopy
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from wedl.changeset import apply, confirmation_token, preview
from wedl.cli import main, parser
from wedl.errors import ConfirmationMismatch, ConfirmationRequired, ConflictError
from wedl.server import create_app


def _payload(
    repository,
    *,
    request_id: str = "request-a",
    key: str = "confirmation-test-v1",
    tag: str = "preview-bound",
) -> dict:
    record = repository.load_world().find("Black Salt Vial", "object")
    return {
        "protocol": "wedl-changeset/v1",
        "expectedHead": repository.head(),
        "idempotencyKey": key,
        "requestId": request_id,
        "summary": "add a preview-bound test tag",
        "operations": [{"type": "entity.update", "entity": record.id, "frontmatterPatch": {"tags": [*record.tags, tag]}}],
    }


def _wedl_files(repository) -> dict[str, bytes]:
    cache = repository.root / ".wedl"
    if not cache.exists():
        return {}
    return {path.relative_to(cache).as_posix(): path.read_bytes() for path in cache.rglob("*") if path.is_file()}


def test_confirmation_is_canonical_and_binds_full_payload_and_head(ash_repo) -> None:
    payload = _payload(ash_repo)
    reordered = dict(reversed(list(payload.items())))
    plan = preview(ash_repo, payload)

    assert plan["confirmationToken"] == confirmation_token(payload, request_hash=plan["requestHash"], expected_head=ash_repo.head())
    assert preview(ash_repo, reordered)["confirmationToken"] == plan["confirmationToken"]
    assert preview(ash_repo, _payload(ash_repo, request_id="request-b"))["confirmationToken"] != plan["confirmationToken"]
    extension = _payload(ash_repo)
    extension["clientExtension"] = {"requestTrace": "different-but-valid-json"}
    assert preview(ash_repo, extension)["confirmationToken"] != plan["confirmationToken"]


def test_apply_requires_matching_preview_token_before_writes_and_replays(ash_repo) -> None:
    payload = _payload(ash_repo)
    plan = preview(ash_repo, payload)
    original_head = ash_repo.head()
    before_refusal = _wedl_files(ash_repo)

    with pytest.raises(ConfirmationRequired):
        apply(ash_repo, payload)
    assert ash_repo.head() == original_head
    assert _wedl_files(ash_repo) == before_refusal

    with pytest.raises(ConfirmationMismatch):
        apply(ash_repo, payload, confirmation_token_value="wedl-confirmation/v1:not-the-token")
    assert ash_repo.head() == original_head
    assert _wedl_files(ash_repo) == before_refusal

    receipt = apply(ash_repo, payload, confirmation_token_value=plan["confirmationToken"])
    assert receipt["idempotentReplay"] is False
    replay = apply(ash_repo, payload, confirmation_token_value=plan["confirmationToken"])
    assert replay["idempotentReplay"] is True
    changed_request_id = deepcopy(payload)
    changed_request_id["requestId"] = "request-b"
    with pytest.raises(ConflictError):
        apply(ash_repo, changed_request_id, confirmation_token_value=plan["confirmationToken"])


def test_trusted_example_scripts_apply_their_own_preview_confirmation() -> None:
    expected = 'apply(repository, payload, confirmation_token_value=plan["confirmationToken"])'
    for script in ("tools/expand_example_v04.py", "tools/finalize_example_v04.py"):
        assert expected in Path(script).read_text(encoding="utf-8")


def test_token_from_a_payload_or_head_cannot_authorize_a_different_initial_apply(ash_repo) -> None:
    payload = _payload(ash_repo)
    token = preview(ash_repo, payload)["confirmationToken"]
    changed = _payload(ash_repo, request_id="request-b")
    changed["idempotencyKey"] = "confirmation-test-b"
    with pytest.raises(ConfirmationMismatch):
        apply(ash_repo, changed, confirmation_token_value=token)

    ash_repo.commit_files(expected_head=ash_repo.head(), files={"note.md": b"# independent revision\n"}, message="advance HEAD")
    with pytest.raises(ConfirmationMismatch):
        apply(ash_repo, payload, use_current_head=True, confirmation_token_value=token)


def test_cli_confirmation_contract_stdin_compact_and_parser_errors(ash_repo, capsys, monkeypatch) -> None:
    payload = _payload(ash_repo)
    token = preview(ash_repo, payload)["confirmationToken"]
    root = str(ash_repo.root)
    original_head = ash_repo.head()
    before_refusal = _wedl_files(ash_repo)
    monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps(payload)))
    assert main(["--compact", "changeset", "apply", "-", "--repo", root]) == 2
    captured = capsys.readouterr()
    assert captured.out == ""
    assert json.loads(captured.err)["code"] == "confirmation_required"
    assert ash_repo.head() == original_head
    assert _wedl_files(ash_repo) == before_refusal

    monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps(payload)))
    assert main(["--compact", "changeset", "apply", "-", "--repo", root, "--confirm", "wedl-confirmation/v1:wrong"]) == 2
    captured = capsys.readouterr()
    assert captured.out == ""
    assert json.loads(captured.err)["code"] == "confirmation_mismatch"
    assert ash_repo.head() == original_head
    assert _wedl_files(ash_repo) == before_refusal

    monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps(payload)))

    assert main(["--compact", "changeset", "apply", "-", "--repo", root, "--confirm", token]) == 0
    captured = capsys.readouterr()
    assert captured.err == ""
    assert json.loads(captured.out)["status"] == "committed"

    assert main(["changeset", "apply", "request.json", "--confirm", "token", "--yes"]) == 2
    error = json.loads(capsys.readouterr().err)
    assert error["code"] == "usage_error"
    assert "changeset apply" in error["details"]["context"]["command"]
    assert parser().parse_args(["changeset", "apply", "request.json", "--yes"]).yes is True

    bypass = _payload(ash_repo, key="confirmation-test-yes-v1", tag="preview-bound-yes")
    monkeypatch.setattr(sys, "stdin", io.StringIO(json.dumps(bypass)))
    assert main(["--compact", "changeset", "apply", "-", "--repo", root, "--yes"]) == 0
    captured = capsys.readouterr()
    assert captured.err == ""
    assert json.loads(captured.out)["status"] == "committed"


def test_http_apply_requires_authentication_and_confirmation_header(ash_repo) -> None:
    payload = _payload(ash_repo)
    with TestClient(create_app(ash_repo.root)) as client:
        session = client.get("/api/session").json()
        assert client.post("/api/changesets/apply", json=payload).status_code == 401
        headers = {"X-Wedl-Token": session["token"]}
        plan = client.post("/api/changesets/preview", json=payload, headers=headers)
        assert plan.status_code == 200
        token = plan.json()["confirmationToken"]
        original_head = ash_repo.head()
        before_refusal = _wedl_files(ash_repo)
        missing = client.post("/api/changesets/apply", json=payload, headers=headers)
        assert missing.status_code == 400
        assert missing.json()["code"] == "confirmation_required"
        assert ash_repo.head() == original_head
        assert _wedl_files(ash_repo) == before_refusal
        wrong = client.post("/api/changesets/apply", json=payload, headers={**headers, "X-Wedl-Confirmation": "wedl-confirmation/v1:wrong"})
        assert wrong.status_code == 400
        assert wrong.json()["code"] == "confirmation_mismatch"
        assert ash_repo.head() == original_head
        assert _wedl_files(ash_repo) == before_refusal
        applied = client.post("/api/changesets/apply", json=payload, headers={**headers, "X-Wedl-Confirmation": token})
        assert applied.status_code == 200
        assert applied.json()["status"] == "committed"
