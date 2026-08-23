from __future__ import annotations

import json
from importlib import resources
from pathlib import Path
import shutil

from wedl.cli import main, parser
from wedl.repository import Repository


def disposable_repository(tmp_path: Path, *, branch_default: bool = False) -> Repository:
    root = tmp_path / "ash"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    if branch_default:
        world_path = root / "story" / "world.md"
        text = world_path.read_text(encoding="utf-8")
        text = text.replace(
            "default_timeline: main\ntimelines:\n- id: main\n  label: Main chronology",
            "default_timeline: branch\ntimelines:\n- id: main\n  label: Main chronology\n- id: branch\n  label: Branch chronology",
            1,
        )
        world_path.write_text(text, encoding="utf-8")
    return Repository(root)


def _output(capsys) -> dict[str, object]:
    return json.loads(capsys.readouterr().out)


def test_temporal_commands_use_the_world_default_timeline_when_omitted(tmp_path: Path, capsys) -> None:
    repository = disposable_repository(tmp_path, branch_default=True)
    root = str(repository.root)
    world = repository.load_world()
    character = world.find("Mara Vale", "character")

    assert main(["state", character.id, "--repo", root, "--tick", "0"]) == 0
    assert _output(capsys)["at"]["timeline"] == "branch"

    assert main(["knowledge", character.id, "--repo", root, "--tick", "0"]) == 0
    assert _output(capsys)["at"]["timeline"] == "branch"

    assert main(["story-points", "--repo", root, "--tick", "0"]) == 0
    assert _output(capsys)["evaluatedAt"]["timeline"] == "branch"


def test_temporal_commands_preserve_explicit_and_scene_temporal_scopes(tmp_path: Path, capsys) -> None:
    repository = disposable_repository(tmp_path, branch_default=True)
    root = str(repository.root)
    world = repository.load_world()
    character = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")

    assert main(["state", character.id, "--repo", root, "--timeline", "main", "--tick", "0"]) == 0
    assert _output(capsys)["at"]["timeline"] == "main"
    assert main(["knowledge", character.id, "--repo", root, "--timeline", "main", "--tick", "0"]) == 0
    assert _output(capsys)["at"]["timeline"] == "main"
    assert main(["story-points", "--repo", root, "--timeline", "main", "--tick", "0"]) == 0
    assert _output(capsys)["evaluatedAt"]["timeline"] == "main"

    assert main(["story-points", "--repo", root, "--scene", scene.id]) == 0
    assert _output(capsys)["evaluatedAt"]["timeline"] == "main"


def test_story_points_rejects_an_unanchored_timeline_and_main_remains_compatible(tmp_path: Path, capsys) -> None:
    repository = disposable_repository(tmp_path, branch_default=True)
    root = str(repository.root)

    assert main(["story-points", "--repo", root, "--timeline", "branch"]) == 2
    error = json.loads(capsys.readouterr().err)
    assert error["code"] == "usage_error"
    assert "--timeline requires --tick" in error["message"]

    args = parser().parse_args(["story-points", "--repo", root, "--tick", "0"])
    assert args.timeline is None

    main_repository = disposable_repository(tmp_path / "main-default")
    main_root = str(main_repository.root)
    character = main_repository.load_world().find("Mara Vale", "character")
    assert main(["state", character.id, "--repo", main_root, "--tick", "0"]) == 0
    assert _output(capsys)["at"]["timeline"] == "main"
    assert main(["knowledge", character.id, "--repo", main_root, "--tick", "0"]) == 0
    assert _output(capsys)["at"]["timeline"] == "main"
    assert main(["story-points", "--repo", main_root, "--tick", "0"]) == 0
    assert _output(capsys)["evaluatedAt"]["timeline"] == "main"
