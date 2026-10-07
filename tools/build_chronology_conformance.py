"""Build a compact, reproducible chronology conformance report.

The retained v0.6 source is the release fixture.  This tool deliberately reports
calendar coverage and compiler evidence separately from StoryTime: the latter
orders scenes, it is never elapsed calendar time.
"""

from __future__ import annotations

import argparse
from importlib import resources
import json
from pathlib import Path
import shutil
from tempfile import TemporaryDirectory

from wedl.compiler import compile_world
from wedl.repository import Repository
from wedl.validation import validate_world


def run() -> dict[str, object]:
    with TemporaryDirectory(prefix="wedl-chronology-conformance-") as temporary:
        root = Path(temporary)
        source = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "legacy_worlds" / "chronology_conformance" / "story"
        with resources.as_file(source) as packaged:
            shutil.copytree(packaged, root / "story")
        repository = Repository(root)
        world = repository.load_world()
        diagnostics = validate_world(world)
        errors = [item for item in diagnostics if item["severity"] == "error"]
        if errors:
            raise RuntimeError(json.dumps(errors, ensure_ascii=False, sort_keys=True))
        compilation = compile_world(repository, "WORKTREE")
        chronology = world.world_record.frontmatter["chronology"]
        return {
            "protocol": "wedl-chronology-conformance/v1",
            "sourceSchema": world.schema,
            "years": [-249, 250],
            "calendarCount": len(chronology["calendars"]),
            "eraCount": len(chronology["eras"]),
            "annotationForms": ["civil", "era", "range", "approx", "conflict", "relative", "duration"],
            "validationErrors": 0,
            "compiled": compilation["status"],
            "cache": "disposable-rebuilt",
            "storyTimeNote": "StoryTime orders replay; tick gaps do not imply elapsed calendar time.",
        }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = run()
    text = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    else:
        print(text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
