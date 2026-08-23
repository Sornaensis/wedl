from __future__ import annotations

import json
from pathlib import Path
import re
from unittest.mock import MagicMock, patch

import pytest

from wedl.changeset import CHANGESET_OPERATION_TYPES, preview
from wedl.cli import dispatch, main, parser
from wedl.repository import Repository


def _repository(tmp_path: Path) -> Repository:
    root = tmp_path / "world"
    (root / "story").mkdir(parents=True)
    (root / "story" / "world.md").write_text(
        "---\n"
        "schema: wedl/v0.3\n"
        "kind: world\n"
        "id: world_00000000000000000000000000\n"
        "title: Scaffold Test World\n"
        "domain: world\n"
        "status: canonical\n"
        "tags: []\n"
        "aliases: []\n"
        "default_timeline: main\n"
        "timelines:\n"
        "  - id: main\n"
        "    label: Main chronology\n"
        "---\n"
        "# Scaffold Test World\n",
        encoding="utf-8",
    )
    return Repository(root)


def test_scaffold_is_current_head_bound_and_immediately_previewable(tmp_path: Path) -> None:
    repository = _repository(tmp_path)

    payload = dispatch(parser().parse_args(["changeset", "scaffold", "--repo", str(repository.root)]))

    assert payload["protocol"] == "wedl-changeset/v1"
    assert payload["expectedHead"] == "WORKTREE"
    assert payload["idempotencyKey"] == "wedl-scaffold-WORKTREE"
    assert payload["summary"]
    assert payload["operations"] == [{"type": "entity.update", "entity": "world_00000000000000000000000000", "frontmatterPatch": {}}]
    assert preview(repository, payload)["valid"] is True


def test_scaffold_noop_preserves_noncanonical_authored_bytes(tmp_path: Path) -> None:
    root = tmp_path / "world"
    story = root / "story"
    story.mkdir(parents=True)
    source = story / "world.md"
    source.write_text(
        "---\n"
        "title: 'Scaffold Test World'  # deliberately authored formatting\n"
        "id: world_00000000000000000000000000\n"
        "kind: world\n"
        "schema: wedl/v0.3\n"
        "domain: world\n"
        "status: canonical\n"
        "tags: [ ]\n"
        "aliases: [ ]\n"
        "default_timeline: main\n"
        "timelines: [ { id: main, label: 'Main chronology' } ]\n"
        "---\n"
        "# Scaffold Test World\n",
        encoding="utf-8",
    )
    repository = Repository(root)

    result = preview(repository, dispatch(parser().parse_args(["changeset", "scaffold", "--repo", str(root)])))

    assert result["valid"] is True
    assert result["files"] == []
    assert result["diff"] == ""
    assert source.read_text(encoding="utf-8").startswith("---\ntitle: 'Scaffold Test World'")


def test_scaffold_stdout_is_machine_readable_json(tmp_path: Path, capsys) -> None:
    repository = _repository(tmp_path)

    assert main(["changeset", "scaffold", "--repo", str(repository.root)]) == 0

    captured = capsys.readouterr()
    assert captured.err == ""
    assert json.loads(captured.out)["expectedHead"] == "WORKTREE"


def test_scaffold_writes_new_json_file_without_stdout_value_or_overwrite(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    output = tmp_path / "change.json"
    arguments = parser().parse_args(["changeset", "scaffold", "--repo", str(repository.root), "--output", str(output)])

    assert dispatch(arguments) is None
    payload = json.loads(output.read_text(encoding="utf-8"))
    assert payload["expectedHead"] == "WORKTREE"
    assert preview(repository, payload)["valid"] is True

    try:
        dispatch(arguments)
    except FileExistsError:
        pass
    else:
        raise AssertionError("scaffold silently replaced an existing output file")


def test_changeset_schema_and_help_make_the_edit_preview_path_discoverable() -> None:
    schema = dispatch(parser().parse_args(["changeset", "schema"]))
    commands = next(action for action in parser()._actions if action.dest == "command")
    changeset = commands.choices["changeset"]
    actions = next(action for action in changeset._actions if action.dest == "changeset_command")
    help_text = actions.choices["scaffold"].format_help()

    assert schema["protocol"] == "wedl-changeset-schema/v1"
    assert schema["required"] == ["protocol", "expectedHead", "idempotencyKey", "summary", "operations"]
    expected = {
        "entity.create", "entity.upsert", "entity.update", "entity.delete", "event.create",
        "conversation.create", "conversation.turn.append", "conversation.recollection.record",
    }
    assert set(CHANGESET_OPERATION_TYPES) == expected
    assert {operation["type"] for operation in schema["operations"]} == expected
    readme = Path("README.md").read_text(encoding="utf-8")
    section = readme.split("Supported operations include:", maxsplit=1)[1].split("Every apply", maxsplit=1)[0]
    assert set(re.findall(r"`([a-z]+(?:[.-][a-z]+)+)`", section)) == expected
    assert "changeset schema" in help_text
    assert "changeset preview" in help_text
    assert "files are never overwritten" in help_text


def test_scaffold_workflow_runs_without_pytest_temp_paths(capsys) -> None:
    """Keep a complete scaffold contract check runnable in locked-down CI.

    The packaged Frontiersmen repository supplies a real committed HEAD; file
    output is mocked so this test does not depend on ``tmp_path`` or the host
    temporary-directory ACLs.
    """

    repository = Repository(Path("frontiersmen"))
    payload = dispatch(parser().parse_args(["changeset", "scaffold", "--repo", str(repository.root)]))
    result = preview(repository, payload)

    assert payload["protocol"] == "wedl-changeset/v1"
    assert payload["expectedHead"] == repository.head()
    assert result["valid"] is True
    assert result["files"] == []
    assert result["diff"] == ""
    assert tuple(item["type"] for item in dispatch(parser().parse_args(["changeset", "schema"]))["operations"]) == CHANGESET_OPERATION_TYPES

    assert main(["--compact", "changeset", "scaffold", "--repo", str(repository.root)]) == 0
    assert json.loads(capsys.readouterr().out) == payload

    output = MagicMock()
    stream = MagicMock()
    output.open.return_value.__enter__.return_value = stream
    arguments = parser().parse_args(["changeset", "scaffold", "--repo", str(repository.root), "--output", "request.json"])
    with patch("wedl.cli.Path", return_value=output):
        assert dispatch(arguments) is None
    output.open.assert_called_once_with("x", encoding="utf-8", newline="\n")

    output.open.side_effect = FileExistsError("request.json")
    with patch("wedl.cli.Path", return_value=output), pytest.raises(FileExistsError):
        dispatch(arguments)
