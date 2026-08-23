from __future__ import annotations

import argparse
import json
from pathlib import Path

from wedl.cli import main, parser
from wedl.completion import render


def _commands(command_parser) -> dict[str, object]:
    action = next(action for action in command_parser._actions if action.dest == "command")
    return action.choices


def _completion_vocabulary(command_parser):
    for action in command_parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            for child in action.choices.values():
                yield from _completion_vocabulary(child)
            continue
        yield from action.option_strings
        if action.choices:
            yield from (str(choice) for choice in action.choices)


def test_completion_command_is_parser_defined_and_does_not_require_a_repository(capsys) -> None:
    arguments = parser().parse_args(["completion", "bash"])
    assert arguments.command == "completion"
    assert arguments.shell == "bash"

    assert main(["completion", "bash"]) == 0
    output = capsys.readouterr().out
    assert output.startswith("# Bash completion for wedl")
    assert "complete -F _wedl_completion wedl" in output
    assert "Repository(" not in output
    assert ".wedl/" not in output

    assert main(["--compact", "completion", "bash"]) == 2
    error = json.loads(capsys.readouterr().err)
    assert error["code"] == "usage_error"
    assert "cannot be combined" in error["message"]


def test_completion_scripts_are_deterministic_and_cover_parser_commands_and_options() -> None:
    root = parser()
    bash = render(root, "bash")
    powershell = render(root, "powershell")

    assert bash == render(root, "bash")
    assert powershell == render(root, "powershell")
    assert "Register-ArgumentCompleter -CommandName wedl" in powershell

    commands = _commands(root)
    for name in ("completion", "init", "status", "entity", "conversation", "changeset", "serve"):
        assert name in commands
        assert name in bash
        assert name in powershell
    # Every static token comes from argparse, so future parser additions cannot
    # silently leave either shell script behind.
    for token in _completion_vocabulary(root):
        assert token in bash
        assert token in powershell


def test_top_level_help_includes_completion_and_first_run_sequence() -> None:
    help_text = parser().format_help()

    assert "completion" in help_text
    assert "wedl init frontiersmen --example frontiersmen" in help_text
    assert "wedl validate --repo frontiersmen" in help_text
    assert "wedl status --repo frontiersmen" in help_text
    assert "wedl compile --repo frontiersmen --profile hybrid --vector-provider lsa" in help_text
    assert "wedl state Rhea --repo frontiersmen --tick 195" in help_text
    assert "wedl serve --repo frontiersmen" in help_text


def test_completion_docs_cover_one_session_persistent_setup_and_raw_output_contract() -> None:
    root = Path(__file__).resolve().parents[1]
    readme = (root / "README.md").read_text(encoding="utf-8")
    protocol = (root / "docs" / "LLM_AND_COMMAND_PROTOCOL.md").read_text(encoding="utf-8")

    assert 'eval "$(wedl completion bash)"' in readme
    assert "~/.bashrc" in readme
    assert '"$VIRTUAL_ENV/bin/wedl" completion bash' in readme
    assert "wedl-completion.bash" in readme
    assert "Invoke-Expression (& wedl completion powershell | Out-String)" in readme
    assert "$PROFILE" in readme
    assert "Join-Path $env:VIRTUAL_ENV 'Scripts\\wedl.exe'" in readme
    assert "wedl-completion.ps1" in readme
    assert "does not invoke a bare `wedl`" in readme
    assert "raw, sourceable shell script" in protocol
    assert "reject `--compact`" in protocol
