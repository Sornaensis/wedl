from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

import pytest

from wedl.cli import main, parser


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def invoke_cli(*arguments: str) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    source = str(PROJECT_ROOT / "src")
    environment["PYTHONPATH"] = source + os.pathsep + environment.get("PYTHONPATH", "")
    return subprocess.run(
        [sys.executable, "-m", "wedl.cli", *arguments],
        cwd=PROJECT_ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )


def test_help_remains_human_readable_and_explains_global_compact() -> None:
    result = invoke_cli("--help")

    assert result.returncode == 0
    assert result.stderr == ""
    assert "Explore and manage Git-backed interactive story worlds." in result.stdout
    assert "--compact" in result.stdout
    assert "search the compiled world within a perspective and time" in result.stdout
    assert "scope" in result.stdout
    assert "print the installed WEDL version and exit" in result.stdout


def test_nested_help_explains_time_and_perspective_constraints() -> None:
    result = invoke_cli("conversation", "show", "--help")
    help_text = " ".join(result.stdout.split())

    assert result.returncode == 0
    assert result.stderr == ""
    assert "Character perspective enforces participant presence and audibility" in help_text
    assert "author only: return full history; conflicts with --tick, --timeline, and --order" in help_text
    assert "timeline for --tick; defaults to the world's default timeline" in help_text
    assert "conflicts with --tick, --timeline, and --order" in help_text


@pytest.mark.parametrize("arguments", [("search", "--help"), ("conversation", "show", "--help")])
def test_temporal_help_explains_all_time_conflicts(arguments: tuple[str, ...]) -> None:
    result = invoke_cli(*arguments)
    help_text = " ".join(result.stdout.split())

    assert result.returncode == 0
    assert "--all-time" in help_text
    assert "conflicts with --tick, --timeline, and --order" in help_text


def test_changeset_help_explains_current_head_tradeoff() -> None:
    result = invoke_cli("changeset", "apply", "--help")

    assert result.returncode == 0
    assert "weakens expected-HEAD protection" in result.stdout


def test_serve_help_uses_meaningful_value_labels_and_correct_defaults() -> None:
    result = invoke_cli("serve", "--help")

    assert result.returncode == 0
    assert result.stderr == ""
    assert "--host HOST" in result.stdout
    assert "loopback bind host (default: 127.0.0.1)" in result.stdout
    assert "--port PORT" in result.stdout
    assert "local TCP port (default: 8765)" in result.stdout


def _command_parsers(command_parser: argparse.ArgumentParser) -> list[argparse.ArgumentParser]:
    parsers = [command_parser]
    for action in command_parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            for child in action.choices.values():
                parsers.extend(_command_parsers(child))
    return parsers


def test_every_command_parser_has_structured_human_help() -> None:
    command_parsers = _command_parsers(parser())

    assert {command.prog for command in command_parsers} >= {
        "wedl", "wedl search", "wedl conversation show", "wedl changeset apply",
    }
    for command in command_parsers:
        help_text = command.format_help()
        assert command.description, command.prog
        assert "usage:" in help_text, command.prog
        assert "-h, --help" in help_text, command.prog
        for action in command._actions:
            if (
                isinstance(action, argparse._SubParsersAction)
                or isinstance(action, argparse._HelpAction)
                or action.help == argparse.SUPPRESS
            ):
                continue
            assert action.help and action.help.strip(), (command.prog, action.dest)
            if action.nargs != 0 and not action.choices:
                assert action.metavar and action.metavar.strip(), (command.prog, action.dest)


def test_cache_backed_read_commands_advertise_strict_no_rebuild_policy() -> None:
    commands = {command.prog: command for command in _command_parsers(parser())}
    expected = {
        "wedl entity list",
        "wedl entity show",
        "wedl state",
        "wedl knowledge",
        "wedl interactions",
        "wedl story-points",
        "wedl search",
        "wedl context",
        "wedl conversation show",
    }
    for name in expected:
        action = next(action for action in commands[name]._actions if action.dest == "require_compiled")
        assert "compile_required" in action.help
        assert action.option_strings == ["--require-compiled"]


@pytest.mark.parametrize(
    ("arguments", "expected_command", "message_fragment"),
    [
        ((), "wedl", "the following arguments are required: COMMAND"),
        (("entity",), "wedl entity", "the following arguments are required: ACTION"),
        (("search",), "wedl search", "the following arguments are required: QUERY"),
        (("status", "--not-an-option"), "wedl status", "unrecognized arguments: --not-an-option"),
        (("conversation", "show", "C1", "--not-an-option"), "wedl conversation show", "unrecognized arguments: --not-an-option"),
        (("search", "needle", "--mode", "unknown"), "wedl search", "invalid choice"),
        (("state", "Rhea", "--tick", "not-an-integer"), "wedl state", "invalid int value"),
        (("state", "Rhea"), "wedl state", "the following arguments are required: --tick"),
        (("not-a-command",), "wedl", "invalid choice"),
    ],
)
def test_parser_failure_matrix_is_structured_and_points_to_exact_help(
    arguments: tuple[str, ...], expected_command: str, message_fragment: str,
) -> None:
    result = invoke_cli(*arguments)

    assert result.returncode == 2
    assert result.stdout == ""
    payload, parsed_length = json.JSONDecoder().raw_decode(result.stderr)
    assert not result.stderr[parsed_length:].strip()
    assert set(payload) == {"code", "message", "details"}
    context = payload["details"]["context"]
    assert payload["code"] == "usage_error"
    assert message_fragment in payload["message"]
    assert context["command"] == expected_command
    assert context["usage"].startswith(f"usage: {expected_command}")
    assert context["help"] == f"{expected_command} --help"


def test_root_parser_failure_keeps_help_hint_in_compact_json() -> None:
    result = invoke_cli("--compact", "not-a-command")

    assert result.returncode == 2
    assert result.stdout == ""
    assert result.stderr.count("\n") == 1
    payload = json.loads(result.stderr)
    assert payload["details"]["context"]["help"] == "wedl --help"


def test_direct_entrypoint_keeps_successful_version_exit(capsys) -> None:
    with pytest.raises(SystemExit) as result:
        main(["--version"])
    assert result.value.code == 0
    assert capsys.readouterr().out.strip() == "0.6.0"
