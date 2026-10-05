from copy import deepcopy
import json

import pytest

from wedl import authoring, changeset, consequence_expectations
from wedl.errors import ConfirmationMismatch, ConflictError, RepositoryError, ValidationFailed
from wedl.repository import Repository
from wedl.transaction_recovery import TransactionJournal
from test_consequence_preview import ReadRepository, _world, _request, _snapshot


def _check(world, *, event="tmp:rescue", policy="required", value="open", tick="300", identifier="condition"):
    return {"type": "expectation.check", "event": event,
            "at": {"timeline": "main", "tick": tick, "order": "0"}, "policy": policy,
            "items": [{"id": identifier, "predicate": {"kind": "state.equals",
                "target": world.find("Brass Restricted-Vault Key").id, "key": "condition", "value": value}}]}


def test_checks_use_final_candidate_original_indexes_and_one_evaluation(monkeypatch):
    world = _world(); payload = _request(world); check = _check(world)
    payload["operations"] = [check, *payload["operations"], deepcopy(check)]
    before = deepcopy(world._cache); original = deepcopy(payload); calls = []
    evaluator = consequence_expectations.evaluate_expectations
    def observed(*args, **kwargs):
        calls.append((args, kwargs)); return evaluator(*args, **kwargs)
    monkeypatch.setattr(consequence_expectations, "evaluate_expectations", observed)
    repository = ReadRepository(world); plan = changeset.preview(repository, payload)
    report = plan["expectationChecks"]
    assert plan["valid"] and report["applyAllowed"] and len(calls) == 2
    assert [group["operationIndex"] for group in report["groups"]] == [0, 2]
    assert all(group["results"][0]["outcome"] == "pass" for group in report["groups"])
    assert plan["semanticDelta"]["expectations"] == [result for group in report["groups"] for result in group["results"]]
    assert report["candidate"] == plan["semanticDelta"]["candidate"]
    assert repository.loads == [(world.revision, False)] and world._cache == before and payload == original
    assert calls[0][1]["candidate"].request_hash == changeset._request_hash(original)


def test_required_advisory_unknown_and_unsupported_without_optional_delta():
    world = _world(); payload = _request(world); payload.pop("consequenceRequest")
    payload["operations"].append(_check(world, value="closed"))
    plan = changeset.preview(ReadRepository(world), payload)
    assert not plan["valid"] and plan["expectationChecks"]["outcome"] == "ok"
    assert plan["diagnostics"][-1]["code"] == "CONSEQUENCE-EXPECTATION-001"
    payload["operations"][-1]["policy"] = "advisory"
    assert changeset.preview(ReadRepository(world), payload)["valid"]
    predicate = payload["operations"][-1]["items"][0]["predicate"]
    predicate["target"] = "Missing evidence"
    report = changeset.preview(ReadRepository(world), payload)["expectationChecks"]
    assert report["applyAllowed"] and report["groups"][0]["results"][0]["outcome"] == "unknown"
    predicate["target"] = world.find("Brass Restricted-Vault Key").id; predicate["key"] = "undeclared"
    report = changeset.preview(ReadRepository(world), payload)["expectationChecks"]
    assert report["groups"][0]["results"][0]["outcome"] == "unsupported"
    payload["operations"][-1]["policy"] = "required"
    assert not changeset.preview(ReadRepository(world), payload)["valid"]
    payload["operations"][-1]["at"]["tick"] = "299"
    assert changeset.preview(ReadRepository(world), payload)["expectationChecks"]["outcome"] == "unavailable"


def test_closed_check_grammar_and_invalid_source_never_evaluate(monkeypatch):
    world = _world(); payload = _request(world); payload.pop("consequenceRequest")
    payload["operations"].append(_check(world))
    def forbidden(*args, **kwargs): raise AssertionError("invalid check/source folded")
    monkeypatch.setattr(consequence_expectations, "evaluate_expectations", forbidden)
    variants = [{"items": []}, {"policy": None}, {"at": {"timeline": "main", "tick": 300, "order": "0"}}, {"extra": True}]
    for patch in variants:
        invalid = deepcopy(payload); invalid["operations"][-1].update(patch)
        report = changeset.preview(ReadRepository(world), invalid)["expectationChecks"]
        assert report == {"outcome": "invalid", "code": "CONSEQUENCE-REQUEST-001", "message": "Invalid consequence request."}
    payload["operations"].insert(0, {"type": "entity.update", "entity": world.find("Brass Restricted-Vault Key").id,
                                        "frontmatterPatch": {"schema": "invalid"}})
    payload["consequenceRequest"] = _request(world)["consequenceRequest"]
    plan = changeset.preview(ReadRepository(world), payload)
    assert not plan["valid"] and plan["expectationChecks"]["code"] == "CONSEQUENCE-SOURCE-001"
    assert plan["semanticDelta"]["outcome"] == "unavailable"


def test_aggregate_check_item_and_serialized_copy_limits_are_closed():
    world = _world(); payload = _request(world); payload["operations"].append(_check(world))
    payload["consequenceRequest"]["limit"] = 1
    plan = changeset.preview(ReadRepository(world), payload)
    assert plan["expectationChecks"]["outcome"] == plan["semanticDelta"]["outcome"] == "limit"
    assert "groups" not in plan["expectationChecks"] and "eventGroups" not in plan["semanticDelta"]
    payload["consequenceRequest"]["limit"] = 1000
    payload["operations"][-1]["items"][0]["id"] = "x" * 135000
    plan = changeset.preview(ReadRepository(world), payload)
    assert not plan["valid"] and plan["expectationChecks"]["outcome"] == plan["semanticDelta"]["outcome"] == "limit"
    payload.pop("consequenceRequest")
    payload["operations"][-1]["items"] = [_check(world, identifier=str(index))["items"][0] for index in range(100)]
    payload["operations"].append(_check(world))
    assert changeset.preview(ReadRepository(world), payload)["expectationChecks"]["outcome"] == "limit"


def test_check_time_policy_and_literals_bind_confirmation_and_named_batch():
    world = _world(); payload = _request(world); payload["operations"].append(_check(world))
    token = changeset.preview(ReadRepository(world), payload)["confirmationToken"]
    for field, value in (("policy", "advisory"), ("event", "Rescue"), ("at", {"timeline": "main", "tick": "301", "order": "0"})):
        changed = deepcopy(payload); changed["operations"][-1][field] = value
        assert changeset.preview(ReadRepository(world), changed)["confirmationToken"] != token
    literal = deepcopy(payload); literal["operations"][-1]["items"][0]["predicate"]["value"] = "tmp:rescue"
    plan = changeset.preview(ReadRepository(world), literal)
    assert plan["expectationChecks"]["groups"][0]["results"][0]["comparison"]["predicate"]["value"] == "tmp:rescue"
    intent = {"action": "consequence.batch", "expectedHead": world.revision, "idempotencyKey": "named-check", "operations": deepcopy(payload["operations"])}
    intent["operations"][0]["time"].update(tick="300", order="0")
    intent["operations"][-1]["items"][0]["predicate"]["target"] = "Brass Restricted-Vault Key"
    batch = authoring.preview_intent(ReadRepository(world), intent)["preview"]
    assert batch["valid"] and batch["expectationChecks"]["applyAllowed"]


def _pure_check(repository):
    world = repository.load_world(repository.head(), cache_write=False)
    event = world.by_kind("event")[0]
    payload = _request(world); payload.pop("consequenceRequest")
    check = _check(world, event=event.id, policy="advisory")
    payload["operations"] = [check]
    return payload


def test_checked_receipt_replays_before_later_head_planning_without_cache_or_index_writes(ash_repo, monkeypatch):
    repository = ash_repo; root = repository.root; payload = _pure_check(repository)
    plan = changeset.preview(repository, payload); before = _snapshot(root); index = (root / ".git/index").read_bytes()
    def forbidden(*args, **kwargs): raise AssertionError("check-only path compiled or committed")
    with monkeypatch.context() as patch:
        patch.setattr(changeset, "_AUTHORING_MAX_RECEIPT", 200)
        with pytest.raises(RepositoryError, match="idempotency receipt exceeds authoring byte limit"):
            changeset.apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
    assert _snapshot(root) == before and not TransactionJournal.pending(root)
    with monkeypatch.context() as patch:
        for name in ("_authoring_cache_preflight", "_authoring_cache_enrollments"):
            patch.setattr(changeset, name, forbidden)
        patch.setattr(repository, "commit_files", forbidden)
        result = changeset.apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "checked" and result["newHead"] == payload["expectedHead"] == repository.head()
    assert result["compile"] == {"status": "not-required", "revision": repository.head()} and result["touchedEntityIds"] == []
    assert (root / ".git/index").read_bytes() == index and not TransactionJournal.pending(root)
    after = _snapshot(root)
    assert {key: value for key, value in after.items() if key != ".wedl/idempotency.json"} == before
    assert json.loads((root / ".wedl/idempotency.json").read_text())[payload["idempotencyKey"]]["result"] == result
    world = repository.load_world(repository.head(), cache_write=False)
    cancelled = deepcopy(payload); cancelled["idempotencyKey"] = "cancelled-allocation"
    frontmatter = deepcopy(world.find("Brass Restricted-Vault Key").frontmatter); frontmatter.pop("id")
    cancelled["operations"][:0] = [{"type": "entity.create", "temporaryId": "tmp:cancel", "value": {"frontmatter": frontmatter}},
                                  {"type": "entity.delete", "entity": "tmp:cancel"}]
    cancelled_plan = changeset.preview(repository, cancelled)
    assert cancelled_plan["files"] == [] and cancelled_plan["generatedIds"]["tmp:cancel"]
    cancelled_result = changeset.apply(repository, cancelled, confirmation_token_value=cancelled_plan["confirmationToken"])
    assert cancelled_result["status"] == "checked" and cancelled_result["generatedIds"] == cancelled_plan["generatedIds"]
    assert cancelled_result["touchedEntityIds"] == [] and (root / ".git/index").read_bytes() == index
    write = _request(world, [{"type": "entity.update", "entity": world.find("Brass Restricted-Vault Key").id, "bodyMarkdown": "Later HEAD\n"}]); write.pop("consequenceRequest")
    write["idempotencyKey"] = "later-head"
    write_plan = changeset.preview(repository, write)
    changeset.apply(repository, write, confirmation_token_value=write_plan["confirmationToken"])
    later = _snapshot(root)
    with monkeypatch.context() as patch:
        patch.setattr(changeset, "preview", forbidden)
        replay = changeset.apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
    assert replay == {**result, "idempotentReplay": True} and _snapshot(root) == later
    changed = deepcopy(payload); changed["operations"][0]["policy"] = "required"
    with pytest.raises(ConflictError): changeset.apply(repository, changed, confirmation_token_value=plan["confirmationToken"])


def test_check_refusal_confirmation_and_normal_write_record_exact_report(ash_repo, monkeypatch):
    repository = ash_repo; root = repository.root; world = repository.load_world(repository.head(), cache_write=False)
    payload = _request(world); payload.pop("consequenceRequest"); payload["operations"].append(_check(world, value="closed"))
    before = _snapshot(root)
    with pytest.raises(ValidationFailed) as failure:
        changeset.apply(repository, payload, confirmation_token_value="unused")
    assert failure.value.message == "Consequence checks block apply." and failure.value.details["expectationChecks"]["outcome"] == "ok"
    assert _snapshot(root) == before
    payload["operations"][-1]["items"][0]["predicate"]["value"] = "open"
    plan = changeset.preview(repository, payload)
    changed = deepcopy(payload); changed["operations"][-1]["policy"] = "advisory"
    with pytest.raises(ConfirmationMismatch): changeset.apply(repository, changed, confirmation_token_value=plan["confirmationToken"])
    with monkeypatch.context() as patch:
        patch.setattr(changeset, "_authoring_cache_preflight", lambda *args: False)
        result = changeset.apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "committed" and result["expectationChecks"] == plan["expectationChecks"]
    assert result["compile"]["status"] == "deferred-to-restart"
    assert result["expectationChecks"]["candidate"]["baseRevision"] == payload["expectedHead"]
    replay = changeset.apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
    assert replay == {**result, "idempotentReplay": True}


@pytest.mark.parametrize("checkpoint,retained", [("receipt-before-completion", False), ("receipt-after-completion", True)])
def test_receipt_completion_boundary_recovers_equal_head_and_retries_exact_result(ash_repo, monkeypatch, checkpoint, retained):
    import wedl.repository as repository_module
    repository = ash_repo; root = repository.root; payload = _pure_check(repository); plan = changeset.preview(repository, payload)
    class InterruptedProcess(BaseException): pass
    def interrupted(name):
        if name == checkpoint:
            lock = repository._canonical_lock
            assert lock is not None and lock[2].owns_canonical_lock(lock[0], pid=__import__("os").getpid(), token=lock[1])
            raise InterruptedProcess("injected receipt interruption")
    with monkeypatch.context() as patch:
        patch.setattr(repository_module, "mutation_checkpoint", interrupted)
        with pytest.raises(InterruptedProcess, match="injected receipt interruption"):
            changeset.apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
    restarted = Repository(root); restarted.recover_authoring_transactions()
    assert restarted.head() == payload["expectedHead"] and not TransactionJournal.pending(root)
    receipt = root / ".wedl/idempotency.json"
    assert receipt.exists() is retained
    result = changeset.apply(restarted, payload, confirmation_token_value=plan["confirmationToken"])
    assert result["status"] == "checked" and result["idempotentReplay"] is retained
    assert result["expectationChecks"] == plan["expectationChecks"]
