"""Generate deterministic shell completion from WEDL's argparse tree.

The generated scripts deliberately contain only command names, option names,
and fixed ``argparse`` choices. They do not invoke WEDL while completing, so
pressing Tab never opens a repository, builds a cache, or suggests authored
entity names.
"""

from __future__ import annotations

import argparse
import shlex
from dataclasses import dataclass
from typing import Iterable


@dataclass(frozen=True)
class _Node:
    """The completion-relevant portion of one argparse parser."""

    path: str
    commands: tuple[str, ...]
    positional_choices: tuple[str, ...]
    options: tuple[str, ...]
    value_options: tuple[tuple[str, tuple[str, ...]], ...]


def _subparser_action(parser: argparse.ArgumentParser) -> argparse._SubParsersAction | None:
    return next(
        (action for action in parser._actions if isinstance(action, argparse._SubParsersAction)),
        None,
    )


def _nodes(parser: argparse.ArgumentParser, path: tuple[str, ...] = ()) -> tuple[_Node, ...]:
    """Flatten a parser tree without inspecting any WEDL repository state."""

    subparsers = _subparser_action(parser)
    commands = tuple(sorted(subparsers.choices)) if subparsers else ()
    options: list[str] = []
    value_options: list[tuple[str, tuple[str, ...]]] = []
    positional_choices: list[str] = []
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            continue
        if not action.option_strings and action.choices:
            positional_choices.extend(sorted(str(choice) for choice in action.choices))
        options.extend(action.option_strings)
        if action.option_strings and action.nargs != 0:
            choices = tuple(sorted(str(choice) for choice in action.choices)) if action.choices else ()
            for option in action.option_strings:
                value_options.append((option, choices))
    node = _Node(
        path="/".join(path) or "root",
        commands=commands,
        positional_choices=tuple(positional_choices),
        options=tuple(sorted(options)),
        value_options=tuple(sorted(value_options)),
    )
    children: list[_Node] = []
    if subparsers:
        for name in commands:
            children.extend(_nodes(subparsers.choices[name], (*path, name)))
    return (node, *children)


def _quoted_words(words: Iterable[str]) -> str:
    return " ".join(shlex.quote(word) for word in words)


def _bash_completion(nodes: tuple[_Node, ...]) -> str:
    child_contexts = [
        (f"{node.path}:{command}", "/".join((*(() if node.path == "root" else node.path.split("/")), command)))
        for node in nodes
        for command in node.commands
    ]
    option_keys = [
        (f"{node.path}:{option}", choices)
        for node in nodes
        for option, choices in node.value_options
    ]
    command_cases = "\n".join(
        f'            {shlex.quote(key)}) _wedl_context={shlex.quote(value)} ;;'
        for key, value in child_contexts
    )
    value_cases = "\n".join(
        f'        {shlex.quote(key)}) COMPREPLY=( $(compgen -W {_quoted_words(choices)!r} -- "$cur") ); return ;;'
        for key, choices in option_keys
        if choices
    )
    value_option_cases = "\n".join(
        f'            {shlex.quote(key)}) _wedl_skip_value=1 ;;'
        for key, _ in option_keys
    )
    word_cases = "\n".join(
        f'        {shlex.quote(node.path)}) candidates={_quoted_words(node.commands or node.positional_choices)!r} ;;'
        for node in nodes
        if node.commands or node.positional_choices
    )
    option_cases = "\n".join(
        f'        {shlex.quote(node.path)}) candidates={_quoted_words(node.options)!r} ;;'
        for node in nodes
        if node.options
    )
    return f'''# Bash completion for wedl. Generated from its argparse parser; source this file or run:
# eval "$(wedl completion bash)"
_wedl_completion() {{
    local cur previous word candidates
    local _wedl_context=root _wedl_skip_value=
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    previous="${{COMP_WORDS[COMP_CWORD-1]}}"

    for word in "${{COMP_WORDS[@]:1:COMP_CWORD}}"; do
        [[ "$word" == -- ]] && break
        if [[ -n "$_wedl_skip_value" ]]; then
            _wedl_skip_value=
            continue
        fi
        case "$_wedl_context:$word" in
{value_option_cases}
{command_cases}
        esac
    done

    case "$_wedl_context:$previous" in
{value_cases}
    esac

    if [[ "$cur" == -* ]]; then
        case "$_wedl_context" in
{option_cases}
        esac
    else
        case "$_wedl_context" in
{word_cases}
        esac
    fi
    COMPREPLY=( $(compgen -W "$candidates" -- "$cur") )
}}
complete -F _wedl_completion wedl
'''


def _powershell_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def _powershell_array(values: Iterable[str]) -> str:
    return "@(" + ", ".join(_powershell_quote(value) for value in values) + ")"


def _powershell_completion(nodes: tuple[_Node, ...]) -> str:
    child_contexts = [
        (f"{node.path}:{command}", "/".join((*(() if node.path == "root" else node.path.split("/")), command)))
        for node in nodes
        for command in node.commands
    ]
    option_keys = [
        (f"{node.path}:{option}", choices)
        for node in nodes
        for option, choices in node.value_options
    ]
    subcommands = "\n".join(
        f"    {_powershell_quote(node.path)} = {_powershell_array(node.commands or node.positional_choices)}"
        for node in nodes
        if node.commands or node.positional_choices
    )
    options = "\n".join(
        f"    {_powershell_quote(node.path)} = {_powershell_array(node.options)}"
        for node in nodes
        if node.options
    )
    children = "\n".join(
        f"    {_powershell_quote(key)} = {_powershell_quote(value)}"
        for key, value in child_contexts
    )
    values = "\n".join(
        f"    {_powershell_quote(key)} = {_powershell_array(choices)}"
        for key, choices in option_keys
        if choices
    )
    takes_value = _powershell_array(key for key, _ in option_keys)
    return f'''# PowerShell completion for wedl. Generated from its argparse parser; load with:
# Invoke-Expression (& wedl completion powershell | Out-String)
Register-ArgumentCompleter -CommandName wedl -ScriptBlock {{
    param($commandName, $wordToComplete, $cursorPosition, $commandAst, $fakeBoundParameters)

    $subcommands = @{{
{subcommands}
    }}
    $options = @{{
{options}
    }}
    $children = @{{
{children}
    }}
    $valueOptions = @{{
{values}
    }}
    $takesValue = {takes_value}
    $context = 'root'
    $skipValue = $false
    $startOfWord = $cursorPosition - $wordToComplete.Length
    $previousWords = @($commandAst.CommandElements |
        Where-Object {{ $_.Extent.EndOffset -le $startOfWord }} |
        ForEach-Object {{ $_.Extent.Text.Trim([char[]]@("'", '"')) }} |
        Select-Object -Skip 1)

    foreach ($word in $previousWords) {{
        if ($word -eq '--') {{ break }}
        if ($skipValue) {{ $skipValue = $false; continue }}
        $key = "$context`:$word"
        if ($children.ContainsKey($key)) {{ $context = $children[$key]; continue }}
        if ($takesValue -contains $key) {{ $skipValue = $true }}
    }}

    $candidates = @()
    if ($previousWords.Count -gt 0 -and $valueOptions.ContainsKey("$context`:$($previousWords[-1])")) {{
        $candidates = $valueOptions["$context`:$($previousWords[-1])"]
    }} elseif ($wordToComplete.StartsWith('-')) {{
        $candidates = $options[$context]
    }} else {{
        $candidates = $subcommands[$context]
    }}
    foreach ($candidate in $candidates | Where-Object {{ $_ -like "$wordToComplete*" }}) {{
        [System.Management.Automation.CompletionResult]::new($candidate, $candidate, 'ParameterValue', $candidate)
    }}
}}
'''


def render(parser: argparse.ArgumentParser, shell: str) -> str:
    """Return a sourceable completion script for ``shell``.

    ``parser`` is passed in rather than imported to keep the CLI's parser as
    the one authoritative command specification and to avoid a circular import.
    """

    nodes = _nodes(parser)
    if shell == "bash":
        return _bash_completion(nodes)
    if shell == "powershell":
        return _powershell_completion(nodes)
    raise ValueError(f"unsupported completion shell: {shell}")
