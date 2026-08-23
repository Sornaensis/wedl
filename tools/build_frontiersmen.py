from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Any, Callable

from wedl.repository import Repository
from wedl.source import split_envelope

import frontiersmen_spec as spec


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def run_cli(source_root: Path, args: list[str]) -> dict[str, Any]:
    env = {**os.environ, "PYTHONPATH": str(source_root / "src")}
    process = subprocess.run(
        [sys.executable, "-m", "wedl.cli", "--compact", *args],
        cwd=source_root,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if process.returncode != 0:
        raise RuntimeError(f"wedl {' '.join(args)} failed ({process.returncode}):\n{process.stderr}\n{process.stdout}")
    return json.loads(process.stdout)


def current_world_id(repository: Path) -> str:
    fm, _body = split_envelope((repository / "story" / "world.md").read_bytes(), "story/world.md")
    return str(fm["id"])


def apply_act(source_root: Path, repository: Path, output_dir: Path, number: int, slug: str, summary: str, operations: list[dict[str, Any]]) -> dict[str, Any]:
    head = Repository(repository).head()
    payload = {
        "protocol": "wedl-changeset/v1",
        "expectedHead": head,
        "idempotencyKey": f"frontiersmen-act-{number}-{slug}-v1",
        "summary": summary,
        "actor": {"kind": "author", "id": "local:frontiersmen-builder"},
        "operations": operations,
    }
    json_path = output_dir / f"act-{number:02d}-{slug}.json"
    write_json(json_path, payload)
    preview = run_cli(source_root, ["changeset", "preview", str(json_path), "--repo", str(repository)])
    write_json(output_dir / f"act-{number:02d}-{slug}-preview.json", preview)
    if not preview.get("valid"):
        raise RuntimeError(f"act {number} invalid:\n" + json.dumps(preview.get("diagnostics"), ensure_ascii=False, indent=2))
    result = run_cli(source_root, ["changeset", "apply", str(json_path), "--repo", str(repository), "--confirm", preview["confirmationToken"]])
    write_json(output_dir / f"act-{number:02d}-{slug}-apply.json", result)
    validation = run_cli(source_root, ["validate", "--repo", str(repository)])
    write_json(output_dir / f"act-{number:02d}-{slug}-validation.json", validation)
    if not validation.get("valid"):
        raise RuntimeError(f"act {number} post-apply validation failed")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("repository", type=Path)
    parser.add_argument("--source-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--start", type=int, default=0, help="first act index to apply")
    parser.add_argument("--through", type=int, default=0, help="exclusive act index; 0 means all")
    args = parser.parse_args()
    source_root = args.source_root.resolve()
    repository = args.repository.resolve()
    output_dir = (args.output_dir or source_root / "examples" / "frontiersmen").resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    world_id = current_world_id(repository)
    acts: list[tuple[str, str, Callable[[], list[dict[str, Any]]]]] = [
        ("foundation", "Establish the Frontier, cast, amber economy, and concealed Blackroot history", lambda: spec.foundation_operations(world_id)),
        ("arrival-eastroad", "Bring the new hires to Keldmouth, bind them to Lantern Pike, travel east, and reach the Harrowcross catastrophe", spec.act1_operations),
        ("harrowcross-investigation", "Abandon the party at Harrowcross, repeat the amber trigger, survive the first wretch, and trace the danger to Saint Orra Mine", spec.act2_operations),
        ("under-the-frontier", "Enter Saint Orra Mine, survive the cave-in, spend two days below, read the Sunken Hall, and surface beyond the maps", spec.act3_operations),
        ("tree-king-and-pursuit", "Approach the masked camp, endure the Tree King's blood-sport sentence, escape through Moth's intervention, and flee the Root Host", lambda: spec.act4_operations(world_id)),
        ("pursuit-perspectives", "Separate audible pursuit from physical presence, record the heroes' immediate recollections, and tighten the final context", spec.act5_operations),
        ("drowned-waymark", "Break the immediate Root Host pursuit at a drowned waymark, recover bounded Blackroot evidence, and open the abandoned warden road south", lambda: spec.act6_operations(world_id)),
    ]
    if args.start < 0 or args.start > len(acts):
        parser.error(f"--start must be between 0 and {len(acts)}")
    through = args.through or len(acts)
    if through < args.start or through > len(acts):
        parser.error(f"--through must be between --start and {len(acts)}")
    for index in range(args.start, through):
        slug, summary, factory = acts[index]
        print(f"Applying act {index}: {slug}", file=sys.stderr)
        apply_act(source_root, repository, output_dir, index, slug, summary, factory())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
