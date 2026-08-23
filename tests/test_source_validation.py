from __future__ import annotations

import pytest

from wedl.errors import ParseError
from wedl.source import parse_record
from wedl.validation import validate_world


def test_duplicate_yaml_keys_are_rejected() -> None:
    data = b"""---\nschema: wedl/v0.3\nkind: character\nid: char_00000000000000000000000000\ntitle: A\ntitle: B\ndomain: cast\nstatus: canonical\ntags: []\naliases: []\n---\n\n# A\n"""
    with pytest.raises(ParseError, match="duplicate YAML key"):
        parse_record(data, "story/characters/a.md")


def test_expanded_fixture_validates_and_has_conversations(ash_repo) -> None:
    world = ash_repo.load_world()
    diagnostics = validate_world(world)
    assert not [item for item in diagnostics if item["severity"] == "error"]
    assert len(world.records) == 262
    assert len(world.by_kind("conversation")) == 16
    assert len(world.by_kind("scene")) == 16
    assert world.active_scene() is None
    assert world.find("An Honest Absence", "scene").status == "closed"


def test_no_runtime_migration_feature() -> None:
    import wedl
    import wedl.cli
    assert not hasattr(wedl, "migrate")
    assert "migrate" not in wedl.cli.parser().format_help().casefold()
