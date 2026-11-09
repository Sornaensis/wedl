#!/usr/bin/env python3
"""Prepare private requests for the complete numbered authoring example world.

This helper reads source through the CLI and writes only a new request directory.
It neither saves an action nor assesses evidence as sufficient.
"""
import argparse
import copy
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--binary", default="wedl-rs")
    args = parser.parse_args()
    inventory = json.loads(subprocess.run(
        [args.binary, "--repo", str(args.repo), "--compact", "entity", "list", "--kind", "world"],
        check=True, text=True, capture_output=True,
    ).stdout)
    worlds = inventory["result"]["items"]
    if len(worlds) != 1 or inventory["revision"] is not None:
        raise SystemExit("Use the current complete numbered authoring example world.")
    fingerprint = inventory["fingerprint"]
    draft = {
        "id": "action_00000000000000000000000001",
        "title": "Ask about the witnessed vow",
        "worldId": worlds[0]["id"],
        "baseFingerprint": fingerprint,
        "horizon": {"timeline": "main", "tick": "12", "order": "0"},
        "intent": "Ana asks which account of the witnessed vow she can rely on.",
        "actorRefs": [{"kind": "record", "id": "char_00000000000000000000000001"}],
        "targetRefs": [{"kind": "record", "id": "concept_00000000000000000000000010"}],
        "hypothesisRefs": [],
        "evidence": [],
        "questions": [{
            "id": "question_00000000000000000000000001",
            "text": "What does Ana actually know, and what remains only in the author margin?",
            "required": True,
        }],
        "phase": "draft",
        "proposals": [],
        "bodyMarkdown": "Private planning only.\r\nDo not turn the rejected account into a canonical effect.\r\n",
    }
    frame = {
        "fingerprint": fingerprint,
        "work": None,
        "sceneId": "scene_00000000000000000000000002",
        "povId": "char_00000000000000000000000001",
        "at": copy.deepcopy(draft["horizon"]),
        "lens": "dramatic-irony",
    }
    review_frame = copy.deepcopy(frame)
    review_frame["at"]["tick"] = "11"  # The saved action's 12/0 takes precedence.
    files = {
        "01-save.json": {"expectedFingerprint": fingerprint, "expectedSha256": None, "draft": draft, "review": None},
        "02-review.json": {"draft": draft, "frame": review_frame, "briefs": [], "families": {}, "maxCharacters": 100000},
        "frame.json": frame,
    }
    args.output.mkdir(parents=False, exist_ok=False)
    for name, value in files.items():
        (args.output / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Prepared {len(files)} request files in {args.output}. No draft was saved or approved.")


if __name__ == "__main__":
    main()
