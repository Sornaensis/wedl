from __future__ import annotations

import pytest

from wedl.errors import ParseError
from wedl.source import parse_record
from wedl.validation import validate_world


def test_duplicate_yaml_keys_are_rejected() -> None:
    data = b"""---\nschema: wedl/v0.3\nkind: character\nid: char_00000000000000000000000000\ntitle: A\ntitle: B\ndomain: cast\nstatus: canonical\ntags: []\naliases: []\n---\n\n# A\n"""
    with pytest.raises(ParseError, match="duplicate YAML key"):
        parse_record(data, "story/characters/a.md")


def test_yaml_sequence_mapping_keys_are_parse_errors_not_type_errors() -> None:
    data = b"""---
? [not, a, scalar]
: value
schema: wedl/v0.3
kind: world
id: world_00000000000000000000000000
title: Key safety
domain: world
status: canonical
tags: []
aliases: []
---
"""
    with pytest.raises(ParseError, match="mapping keys must be hashable"):
        parse_record(data, "story/world.md")


def test_crlf_frontmatter_envelope_is_accepted() -> None:
    record = parse_record(
        b"---\r\nschema: wedl/v0.3\r\nkind: world\r\nid: world_00000000000000000000000000\r\ntitle: CRLF World\r\ndomain: world\r\nstatus: canonical\r\ntags: []\r\naliases: []\r\n---\r\n\r\n# CRLF World\r\n",
        "story/world.md",
    )
    assert record.title == "CRLF World"
    assert record.body == "# CRLF World\r\n"


def test_expanded_fixture_validates_and_has_conversations(ash_repo) -> None:
    world = ash_repo.load_world()
    diagnostics = validate_world(world)
    assert not [item for item in diagnostics if item["severity"] == "error"]
    assert len(world.records) == 262
    assert len(world.by_kind("conversation")) == 16
    assert len(world.by_kind("scene")) == 16
    assert world.active_scene() is None
    assert world.find("An Honest Absence", "scene").status == "closed"


def test_local_migration_is_cli_only() -> None:
    import wedl.cli

    help_text = wedl.cli.parser().format_help().casefold()
    assert "migrate" in help_text
    args = wedl.cli.parser().parse_args(["migrate", "preview", "--mode", "upgrade-v03", "--expected-head", "a" * 40, "--idempotency-key", "test"])
    assert args.command == "migrate"
    assert args.migration_command == "preview"
