#!/usr/bin/env python3
"""Run a small Writing/search/current-time walkthrough using only the stdlib."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def identifier(prefix, number=1):
    return f"{prefix}_{number:026}"


WORLD = identifier("world")
ACTOR, OTHER = identifier("char"), identifier("char", 2)
PLACE = identifier("loc")
ACTIVE, CLOSED = identifier("scene"), identifier("scene", 2)
PIER, SHIP, LIBRARY = (identifier("obj", number) for number in (1, 2, 3))
EVENT = identifier("event")
T1, T2 = identifier("thread"), identifier("thread", 2)
WORK, SEGMENT = identifier("work"), identifier("segment")


def point(tick, order=0, wire=False):
    return {"timeline": "main", "tick": str(tick) if wire else tick,
            "order": str(order) if wire else order}


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    return path


def source(root, relative, kind, record_id, title, body, **fields):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    # JSON mappings are valid YAML; this helper needs no YAML dependency.
    header = {"schema": "wedl-author/v1" if relative.startswith("author/") else "wedl/v1",
              "kind": kind, "id": record_id, "title": title, **fields}
    path.write_text("---\n" + json.dumps(header, ensure_ascii=False, indent=2) + "\n---\n" + body)


def fixture(root):
    def canonical(kind, record_id, title, body="", **fields):
        source(root, f"story/{record_id}.md", kind, record_id, title, body, **fields)

    canonical("world", WORLD, "Writing walkthrough", capabilities=["descriptions-v1"],
              default_timeline="main", timelines=[{"id": "main", "label": "Main"}],
              current_time=point(10), threads=[{"id": T1, "label": "Harbor"},
                                             {"id": T2, "label": "Voyage"}], state_keys={})
    canonical("location", PLACE, "Landing", "The harbor landing.\n")
    canonical("character", ACTOR, "Ada", initial_state={}, descriptions=[{
        "id": identifier("description"), "heading": "Present witness",
        "markdown": "Ada waits at the landing.", "valid": {"kind": "timeless"},
        "audience": ["author", "public"], "perspectives": ["author", "ordinary"]}])
    canonical("character", OTHER, "Unselected person", "PRIVATE_LINKED_PERSON\n", initial_state={})
    canonical("object", PIER, "Pier", "harbor pier vessel\n", threads=[T1])
    canonical("object", SHIP, "Ship", "ship vessel harbor\n", threads=[T2])
    canonical("object", LIBRARY, "Library", "library book scroll\n", threads=[T1, T2])
    canonical("event", EVENT, "Later event", "futurebody futurebody futurebody\n", time=point(20))
    canonical("scene", ACTIVE, "Waiting at the landing", "A completed aggregate.\n", status="active",
              location=PLACE, time={"start": point(0), "current": point(10), "end": point(20)},
              participants=[{"character": ACTOR, "point_of_view": True}], observations=[])
    canonical("scene", CLOSED, "Earlier empty scene", "Retained closed scene.\n", status="closed",
              location=PLACE, time={"start": point(0), "current": point(5), "end": point(5)},
              participants=[], observations=[])
    source(root, f"author/segments/{SEGMENT}.md", "manuscript-segment", SEGMENT,
           "Private later segment", "PRIVATE_SEGMENT_PROSE\r\n",
           source_links=[{"kind": "record", "id": OTHER}],
           placement=point(50, 7), pov_character_id=OTHER)
    source(root, f"author/works/{WORK}.md", "work", WORK, "Private vessel draft", "PRIVATE_WORK_NOTES\n",
           duplicate_policy="allow-explicit-reuse", chapters=[{
               "id": identifier("chapter"), "title": "Private chapter",
               "segments": [{"kind": "author-record", "id": SEGMENT},
                            {"kind": "author-record", "id": SEGMENT}]}])


def source_image(root):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for folder in (root / "story", root / "author") for path in sorted(folder.rglob("*"))
            if path.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New output directory")
    args = parser.parse_args()
    binary, output = args.binary.resolve(), args.output.resolve()
    if not binary.is_file():
        parser.error("--binary must name an existing wedl-rs executable")
    output.mkdir()  # Refuse existing output rather than overwriting evidence.
    root = output / "world"
    fixture(root)
    before = source_image(root)
    commands = []

    def run(label, *arguments):
        command = [str(binary), "--compact", "--repo", str(root), *map(str, arguments)]
        completed = subprocess.run(command, text=True, capture_output=True, check=False)
        commands.append({"label": label, "argv": command, "exitCode": completed.returncode})
        write_json(output / "commands.json", commands)
        if completed.returncode:
            raise RuntimeError(f"{label}: {completed.stderr.strip()}")
        value = json.loads(completed.stdout)
        write_json(output / f"{len(commands):02}-{label}.json", value)
        return value

    assert run("validate", "validate")["result"]["valid"]
    status = run("status", "status")
    fingerprint = status["fingerprint"]
    frame = {"fingerprint": fingerprint, "work": None, "sceneId": ACTIVE, "povId": ACTOR,
             "at": point(10, wire=True), "lens": "author"}
    discovery = write_json(output / "discover-work.json", frame)
    choices = run("work-choices", "writing", "choices", "--file", discovery)["result"]
    assert [work["id"] for work in choices["works"]] == [WORK]
    author_fingerprint = choices["authorFingerprint"]
    assert len(author_fingerprint) == 64

    unbound = {**frame, "lens": "character"}
    bound = {**unbound, "work": {"workId": WORK, "authorFingerprint": author_fingerprint}}
    unbound_file = write_json(output / "frame-unbound.json", unbound)
    bound_file = write_json(output / "frame-work.json", bound)
    resolved = run("work-resolve", "writing", "resolve", "--file", bound_file)["result"]
    assert resolved == bound
    plain = run("unbound-context", "writing", "context", "--file", unbound_file,
                "--max-characters", "100000")["result"]
    selected = run("work-context", "writing", "context", "--file", bound_file,
                   "--max-characters", "100000")["result"]
    assert selected["frame"] == bound
    assert selected["characterPrompt"] == plain["characterPrompt"]
    assert selected.get("authorMargin") == plain.get("authorMargin")
    for private in ("PRIVATE_SEGMENT_PROSE", "PRIVATE_WORK_NOTES", "PRIVATE_LINKED_PERSON"):
        assert private not in json.dumps(selected)

    memberships = {PIER: [T1], SHIP: [T2], LIBRARY: [T1, T2]}
    search_counts = {}
    for mode in ("lexical", "semantic", "hybrid"):
        shared = ["--timeline", "main", "--tick", "10", "--order", "0", "--mode", mode, "--limit", "100"]
        full = run(f"{mode}-full", "search", "vessel", "--kind", "object", *shared)["result"]["matches"]
        grouped = run(f"{mode}-thread", "search", "vessel", "--kind", "object", *shared,
                      "--thread", T2)["result"]["matches"]
        expected = [row for row in full["items"] if T2 in memberships[row["id"]]]
        assert grouped["items"] == expected
        assert grouped["retrieval"] == full["retrieval"]
        assert grouped["rankedTotal"] == full["total"]
        assert grouped["total"] == len(expected)
        assert grouped["scope"] == "author-as-of" and grouped["at"] == point(10, wire=True)
        future = run(f"{mode}-future", "search", "futurebody", "--kind", "event", *shared)["result"]["matches"]
        assert future["items"] == []
        search_counts[mode] = {"full": full["total"], "thread": grouped["total"]}
    later = run("future-at-boundary", "search", "futurebody", "--kind", "event", "--mode", "lexical",
                "--timeline", "main", "--tick", "20", "--order", "0")["result"]["matches"]
    assert later["items"][0]["id"] == EVENT

    request = {"expectedFingerprint": fingerprint, "at": point(12, 2, wire=True)}
    request_file = write_json(output / "current-time-request.json", request)
    clock = run("current-time-preview", "current-time", "preview", "--file", request_file,
                "--max-characters", "100000")["result"]
    assert clock["activeSceneIds"] == [ACTIVE]
    assert clock["preview"]["validation"]["valid"] and clock["preview"]["changedFiles"] == 2
    assert [operation["entityId"] for operation in clock["plan"]["operations"]] == [WORLD, ACTIVE]
    assert {item["entityId"] for item in clock["preview"]["files"]} == {WORLD, ACTIVE}
    assert not clock["writesPerformed"] and not clock["canApply"]
    plan_file = write_json(output / "current-time-plan.json", clock["plan"])
    replay = run("exported-plan-preview", "consequences", "preview", "--file", plan_file,
                 "--max-characters", "100000")["result"]
    assert replay == clock["preview"]
    final_status = run("unchanged-status", "status")
    assert final_status["fingerprint"] == fingerprint
    assert final_status["result"]["currentTime"] == point(10, wire=True)
    assert source_image(root) == before
    assert not (root / ".wedl-rust").exists()

    summary = {"protocol": "wedl-rust/v1", "cliCommands": len(commands), "canonicalFingerprint": fingerprint,
               "authorFingerprint": author_fingerprint, "selectedWork": WORK, "frame": bound,
               "searchCounts": search_counts, "proposedTime": request["at"],
               "proposedChangedIds": [WORLD, ACTIVE], "sourceUnchanged": True, "cacheCreated": False}
    write_json(output / "summary.json", summary)
    print(json.dumps({"output": str(output), **summary}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
