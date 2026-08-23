from __future__ import annotations

import json

import pytest

from wedl.cli import main, parser
from wedl.context import MIN_COMPONENT_BUDGET
from wedl.ids import KIND_PREFIX


@pytest.mark.parametrize(
    ("arguments", "expected_command", "message"),
    [
        (("entity", "list", "--repo", "does-not-exist", "--kind", "charcter"), "wedl entity list", "invalid choice"),
        (("search", "needle", "--repo", "does-not-exist", "--limit", "0"), "wedl search", "must be between 1 and 50"),
        (("search", "needle", "--repo", "does-not-exist", "--limit", "51"), "wedl search", "must be between 1 and 50"),
        (("context", "Mara", "--repo", "does-not-exist", "--max-characters", str(MIN_COMPONENT_BUDGET - 1)), "wedl context", f"must be at least {MIN_COMPONENT_BUDGET}"),
        (("context", "Mara", "--repo", "does-not-exist", "--max-items", "0"), "wedl context", "must be at least 1"),
        (("serve", "--repo", "does-not-exist", "--port", "0"), "wedl serve", "must be between 1 and 65535"),
        (("serve", "--repo", "does-not-exist", "--port", "65536"), "wedl serve", "must be between 1 and 65535"),
    ],
)
def test_invalid_values_are_structured_usage_errors_before_runtime_work(
    arguments: tuple[str, ...], expected_command: str, message: str, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch,
) -> None:
    def no_dispatch(*_args: object) -> object:
        raise AssertionError("invalid parser input reached runtime dispatch")

    monkeypatch.setattr("wedl.cli.dispatch", no_dispatch)
    assert main(list(arguments)) == 2

    captured = capsys.readouterr()
    assert captured.out == ""
    payload = json.loads(captured.err)
    assert payload["code"] == "usage_error"
    assert message in payload["message"]
    context = payload["details"]["context"]
    assert context["command"] == expected_command
    assert context["usage"].startswith(f"usage: {expected_command}")
    assert context["help"] == f"{expected_command} --help"
    if expected_command == "wedl entity list":
        assert "charcter" in payload["message"]
        for kind in KIND_PREFIX:
            if kind == "hypothesis":
                continue
            assert repr(kind) in payload["message"]


def test_parser_accepts_exactly_the_canonical_entity_kinds() -> None:
    kind_action = next(action for action in parser()._actions if action.dest == "command").choices["entity"]
    list_action = next(action for action in kind_action._actions if action.dest == "entity_command").choices["list"]
    choices = next(action.choices for action in list_action._actions if action.dest == "kind")
    canonical_kinds = set(KIND_PREFIX) - {"hypothesis"}
    assert set(choices) == canonical_kinds

    for kind in canonical_kinds:
        assert parser().parse_args(["entity", "list", "--kind", kind]).kind == kind
    with pytest.raises(Exception):
        parser().parse_args(["entity", "list", "--kind", "hypothesis"])


def test_help_exposes_canonical_kinds_and_numeric_limits() -> None:
    command_action = next(action for action in parser()._actions if action.dest == "command")
    entity_list = next(action for action in command_action.choices["entity"]._actions if action.dest == "entity_command").choices["list"]
    entity_help = " ".join(entity_list.format_help().split())
    for kind in set(KIND_PREFIX) - {"hypothesis"}:
        assert kind in entity_help
    assert "hypothesis" not in entity_help
    assert "maximum results, from 1 to 50 (default: 20)" in " ".join(command_action.choices["search"].format_help().split())
    context_help = " ".join(command_action.choices["context"].format_help().split())
    assert f"at least {MIN_COMPONENT_BUDGET} (default: 8000)" in context_help
    assert "maximum included items; at least 1 (default: 24)" in context_help
    assert "local TCP port (default: 8765); must be from 1 to 65535" in " ".join(command_action.choices["serve"].format_help().split())


def test_parser_accepts_each_validation_boundary() -> None:
    assert parser().parse_args(["search", "needle", "--limit", "1"]).limit == 1
    assert parser().parse_args(["search", "needle", "--limit", "50"]).limit == 50
    assert parser().parse_args(["context", "Mara", "--max-characters", str(MIN_COMPONENT_BUDGET)]).max_characters == MIN_COMPONENT_BUDGET
    assert parser().parse_args(["context", "Mara", "--max-characters", "1000000"]).max_characters == 1000000
    assert parser().parse_args(["context", "Mara", "--max-items", "1"]).max_items == 1
    assert parser().parse_args(["serve", "--port", "1"]).port == 1
    assert parser().parse_args(["serve", "--port", "65535"]).port == 65535
    assert parser().parse_args(["state", "Mara", "--tick", "-1"]).tick == -1
