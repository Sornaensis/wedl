#!/usr/bin/env python3
"""Generate deterministic wedl repositories for compiler/query stress testing.

The generated worlds are intentionally synthetic and are never packaged as the
human-authored example.  They exercise source parsing, reference validation,
temporal replay, conversation/recollection projection, FTS, embeddings, and
interaction materialization at repeatable scales.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
from typing import Any

from wedl import SOURCE_SCHEMA, __version__
from wedl.ids import id_from_seed
from wedl.source import generated_path, serialize_record
from wedl.util import b64url_json

PROFILES = {
    "small": {"characters": 24, "locations": 16, "objects": 120, "events": 180, "knowledge": 180, "conversations": 24, "relationships": 48},
    "medium": {"characters": 120, "locations": 80, "objects": 900, "events": 1400, "knowledge": 1400, "conversations": 160, "relationships": 240},
    "large": {"characters": 240, "locations": 180, "objects": 2200, "events": 3600, "knowledge": 3600, "conversations": 360, "relationships": 480},
}


def point(tick: int, order: int = 0) -> dict[str, Any]:
    return {"timeline": "main", "tick": tick, "order": order}


def common(kind: str, entity_id: str, title: str, domain: str, status: str = "canonical") -> dict[str, Any]:
    return {
        "schema": SOURCE_SCHEMA,
        "kind": kind,
        "id": entity_id,
        "title": title,
        "domain": domain,
        "status": status,
        "tags": ["stress"],
        "aliases": [],
        "provenance": b64url_json({
            "v": 1,
            "tx": id_from_seed("transaction", f"stress:{entity_id}"),
            "command": "stress.generate",
            "actor": "tool:generate-stress-world",
            "generator": f"wedl/{__version__}",
            "parent": "0" * 40,
        }),
    }


def write(root: Path, fm: dict[str, Any], body: str) -> None:
    path = root / generated_path("story", fm["kind"], fm["title"], fm["id"], fm)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(serialize_record(fm, body))


def generate(path: Path, counts: dict[str, int]) -> dict[str, int]:
    if path.exists():
        shutil.rmtree(path)
    (path / "story").mkdir(parents=True)
    (path / ".gitignore").write_text(".wedl/\n", encoding="utf-8")

    world_id = id_from_seed("world", "stress:world")
    root_location = id_from_seed("location", "stress:root-location")
    world = common("world", world_id, "wedl Stress World", "world")
    world.update({
        "default_timeline": "main",
        "timelines": [{"id": "main", "label": "Main chronology"}],
        "state_keys": {
            "character": {"location": {"type": "entity", "entity_kind": "location"}, "condition": {"type": "string"}},
            "object": {
                "holder": {"type": "entity", "entity_kind": "character", "exclusive_group": "placement"},
                "location": {"type": "entity", "entity_kind": "location", "exclusive_group": "placement"},
                "condition": {"type": "string"},
            },
        },
        "relationship_metrics": {"trust": {"minimum": -1.0, "maximum": 1.0, "default": 0.0}},
        "embedding_policy": {"provider": "lsa", "model": "wedl-lsa-v1", "dimensions": 192, "max_features": 8192},
        "context_policy": {"default_budget_characters": 8000, "default_max_items": 24},
        "compilation_policy": {"source_parse_cache": True, "embedding_cache": True, "default_profile": "hybrid", "retrieval": {"fts_candidate_limit": 120, "vector_candidate_limit": 120, "hybrid_fts_weight": 1.0, "hybrid_vector_weight": 1.0, "hybrid_rrf_k": 60.0}},
    })
    write(path, world, "# wedl Stress World\n\nSynthetic data for repeatable performance tests.\n")

    location_ids = [root_location]
    fm = common("location", root_location, "Stress Atrium", "stress.location")
    fm.update({"location_type": "atrium", "parent": None, "links": [], "section_audiences": {"Stress Atrium": ["public", "author"]}})
    write(path, fm, "# Stress Atrium\n\nThe root of the synthetic location tree.\n")
    for index in range(1, counts["locations"]):
        entity_id = id_from_seed("location", f"stress:location:{index}")
        parent = location_ids[(index - 1) // 4]
        location_ids.append(entity_id)
        fm = common("location", entity_id, f"Stress Chamber {index:04d}", "stress.location")
        fm.update({"location_type": "chamber", "parent": parent, "links": [], "section_audiences": {f"Stress Chamber {index:04d}": ["public", "author"]}})
        write(path, fm, f"# Stress Chamber {index:04d}\n\nA deterministic synthetic chamber with index {index}.\n")

    character_ids: list[str] = []
    for index in range(counts["characters"]):
        entity_id = id_from_seed("character", f"stress:character:{index}")
        character_ids.append(entity_id)
        fm = common("character", entity_id, f"Stress Character {index:04d}", "stress.cast")
        fm.update({
            "pronouns": {"subject": "they", "object": "them", "possessive": "their"},
            "role": "synthetic-agent",
            "initial_state": {"location": {"entity": location_ids[index % len(location_ids)]}, "condition": "ready"},
            "section_audiences": {"Summary": ["public", "self", "author"], "Voice": ["self", "author"], "Goals": ["self", "author"]},
        })
        body = f"# Stress Character {index:04d}\n\n## Summary\n\nSynthetic actor number {index}.\n\n## Voice\n\nUses short indexed statements.\n\n## Goals\n\nPreserve record {index} and compare it with neighboring records.\n"
        write(path, fm, body)

    object_ids: list[str] = []
    for index in range(counts["objects"]):
        entity_id = id_from_seed("object", f"stress:object:{index}")
        object_ids.append(entity_id)
        fm = common("object", entity_id, f"Stress Object {index:05d}", "stress.object")
        fm.update({
            "object_type": "indexed-token",
            "initial_state": {"location": {"entity": location_ids[index % len(location_ids)]}, "condition": "indexed"},
            "capabilities": [],
            "section_audiences": {f"Stress Object {index:05d}": ["public", "author"]},
        })
        write(path, fm, f"# Stress Object {index:05d}\n\nSynthetic token {index}; checksum group {index % 37}.\n")

    event_ids: list[str] = []
    for index in range(counts["events"]):
        event_id = id_from_seed("event", f"stress:event:{index}")
        event_ids.append(event_id)
        character = character_ids[index % len(character_ids)]
        obj = object_ids[index % len(object_ids)]
        location = location_ids[(index * 7) % len(location_ids)]
        effect_id = id_from_seed("effect", f"stress:event:{index}:effect")
        fm = common("event", event_id, f"Stress Event {index:05d}", "stress.event")
        fm.update({
            "time": point(1000 + index, index % 10),
            "location": location,
            "participants": [{"character": character, "role": "actor"}],
            "causes": [event_ids[index - 1]] if index and index % 9 == 0 else [],
            "related_story_points": [],
            "effects": [{"id": effect_id, "target": obj, "key": "condition", "operation": "set", "value": f"state-{index % 19}"}],
        })
        write(path, fm, f"# Stress Event {index:05d}\n\nCharacter {index % len(character_ids)} updates object {index % len(object_ids)} in chamber {(index * 7) % len(location_ids)}.\n")

    for index in range(counts["knowledge"]):
        entity_id = id_from_seed("knowledge", f"stress:knowledge:{index}")
        knower = character_ids[index % len(character_ids)]
        source_event = event_ids[index % len(event_ids)]
        transition_id = id_from_seed("knowledge-transition", f"stress:knowledge:{index}:transition")
        fm = common("knowledge", entity_id, f"Stress Knowledge {index:05d}", f"stress.knowledge.{index % len(character_ids)}")
        fm.update({
            "knower": knower,
            "claim": {
                "key": f"stress.claim.{index:05d}",
                "statement": f"Synthetic observation {index} concerns checksum group {index % 37}.",
                "author_truth_status": "true",
                "subject": object_ids[index % len(object_ids)],
                "predicate": "has-checksum-group",
                "object": {"text": str(index % 37)},
            },
            "transitions": [{
                "id": transition_id,
                "time": point(1000 + (index % len(event_ids)), 20),
                "state": "accepted",
                "confidence": 0.8 + (index % 20) / 100,
                "acquisition": "observed",
                "causing_event": source_event,
                "source_entity": object_ids[index % len(object_ids)],
            }],
        })
        write(path, fm, f"# Stress Knowledge {index:05d}\n\nSubjective synthetic knowledge for character {index % len(character_ids)}.\n")

    # Directional inverse relationship pairs.
    pair_count = counts["relationships"] // 2
    for index in range(pair_count):
        first = character_ids[index % len(character_ids)]
        second = character_ids[(index * 13 + 1) % len(character_ids)]
        if first == second:
            second = character_ids[(index + 1) % len(character_ids)]
        first_rel = id_from_seed("relationship", f"stress:relationship:{index}:forward")
        second_rel = id_from_seed("relationship", f"stress:relationship:{index}:reverse")
        for entity_id, source, target, inverse, direction in (
            (first_rel, first, second, second_rel, "forward"),
            (second_rel, second, first, first_rel, "reverse"),
        ):
            transition_id = id_from_seed("relationship-transition", f"stress:relationship:{index}:{direction}:transition")
            fm = common("relationship", entity_id, f"Stress Relationship {index:04d} {direction}", "stress.relationship")
            fm.update({
                "from": source,
                "to": target,
                "relationship_kind": "synthetic-peer",
                "inverse": inverse,
                "transitions": [{
                    "id": transition_id,
                    "time": point(900 + index, 0),
                    "relationship_status": "active",
                    "metrics": {"trust": ((index % 21) - 10) / 10},
                    "facets": ["synthetic-peer"],
                    "causing_event": None,
                    "note": f"Synthetic relationship pair {index}.",
                }],
            })
            write(path, fm, f"# Stress Relationship {index:04d} {direction}\n\nDirectional synthetic relation.\n")

    conversation_ids: list[str] = []
    for index in range(counts["conversations"]):
        entity_id = id_from_seed("conversation", f"stress:conversation:{index}")
        conversation_ids.append(entity_id)
        first = character_ids[index % len(character_ids)]
        second = character_ids[(index + 1) % len(character_ids)]
        start_tick = 3000 + index * 2
        turns = []
        for ordinal in range(4):
            turns.append({
                "id": id_from_seed("conversation-turn", f"stress:conversation:{index}:turn:{ordinal}"),
                "at": point(start_tick, 10 + ordinal * 10),
                "speaker": first if ordinal % 2 == 0 else second,
                "text": f"Conversation {index} turn {ordinal} checksum {(index + ordinal) % 37}.",
                "audience": ["participants"],
            })
        recollections = []
        for owner, confidence in ((first, 0.91), (second, 0.83)):
            recollections.append({
                "id": id_from_seed("conversation-recollection", f"stress:conversation:{index}:recollection:{owner}"),
                "character": owner,
                "at": point(start_tick + 1, 0),
                "state": "remembered",
                "summary": f"They remember synthetic conversation {index} as an exchange about checksum {(index + 2) % 37}.",
                "interpretation": "The other participant was testing consistency.",
                "emotional_impression": "controlled attention",
                "confidence": confidence,
                "exact_turns": [turns[1]["id"]],
                "remembered_quotes": [],
            })
        fm = common("conversation", entity_id, f"Stress Conversation {index:04d}", "stress.conversation", status="closed")
        fm.update({
            "time": {"start": point(start_tick, 0), "end": point(start_tick, 50)},
            "scene": None,
            "location": location_ids[index % len(location_ids)],
            "participants": [
                {"character": first, "role": "speaker", "from": point(start_tick, 0), "to": point(start_tick, 50)},
                {"character": second, "role": "speaker", "from": point(start_tick, 0), "to": point(start_tick, 50)},
            ],
            "topics": ["stress", f"checksum-{index % 37}"],
            "turns": turns,
            "recollections": recollections,
        })
        write(path, fm, f"# Stress Conversation {index:04d}\n\nCanonical synthetic dialogue with subjective recollections.\n")

    # One active scene provides a context/search target after all historical data.
    scene_id = id_from_seed("scene", "stress:active-scene")
    scene_start = 10_000
    participants = character_ids[: min(6, len(character_ids))]
    observations = [
        {
            "id": id_from_seed("observation", f"stress:observation:{index}"),
            "at": point(scene_start, 10 + index),
            "audience": [character],
            "salience": 1.0,
            "text": f"Character {index} notices checksum group {index % 37} on the active table.",
        }
        for index, character in enumerate(participants)
    ]
    # Move every declared participant and object into the active scene before
    # validating it. Stress repositories must exercise the same physical-scene
    # invariants as authored worlds rather than relying on fixture exceptions.
    relocation_id = id_from_seed("event", "stress:active-scene-relocation")
    relocation_effects = []
    for index, character in enumerate(participants):
        relocation_effects.append({
            "id": id_from_seed("effect", f"stress:active-scene:character:{index}"),
            "target": character,
            "key": "location",
            "operation": "set",
            "value": {"entity": root_location},
        })
    for index, object_id in enumerate(object_ids[: min(12, len(object_ids))]):
        relocation_effects.append({
            "id": id_from_seed("effect", f"stress:active-scene:object:{index}"),
            "target": object_id,
            "key": "location",
            "operation": "set",
            "value": {"entity": root_location},
        })
    fm = common("event", relocation_id, "Stress Active Scene Assembly", "stress.event")
    fm.update({
        "time": point(scene_start - 1, 0),
        "location": root_location,
        "participants": [{"character": character, "role": "participant"} for character in participants],
        "causes": [],
        "related_story_points": [],
        "effects": relocation_effects,
    })
    write(path, fm, "# Stress Active Scene Assembly\n\nSynthetic relocation into the benchmark scene.\n")
    fm = common("scene", scene_id, "Stress Active Scene", "stress.scene", status="active")
    fm.update({
        "time": {"start": point(scene_start, 0), "current": point(scene_start, 100), "end": None},
        "location": root_location,
        "participants": [
            {"character": character, "role": "participant", "point_of_view": index == 0, "from": point(scene_start, 0)}
            for index, character in enumerate(participants)
        ],
        "objects": object_ids[: min(12, len(object_ids))],
        "environments": [],
        "story_points": [],
        "observations": observations,
        "conversations": [],
        "author_constraints": ["Synthetic scene used only for performance measurement."],
    })
    write(path, fm, "# Stress Active Scene\n\nA deterministic context target for search and context benchmarks.\n")

    subprocess.run(["git", "init", "-q", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.name", "wedl stress"], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.email", "stress@localhost"], check=True)
    subprocess.run(["git", "-C", str(path), "add", "story", ".gitignore"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-qm", "wedl: generate stress world"], check=True)
    return {
        "records": 1 + counts["locations"] + counts["characters"] + counts["objects"] + counts["events"] + counts["knowledge"] + counts["relationships"] + counts["conversations"] + 2,
        "activeSceneId": scene_id,
        "viewpointCharacterId": participants[0],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", type=Path)
    parser.add_argument("--profile", choices=sorted(PROFILES), default="medium")
    args = parser.parse_args()
    result = generate(args.path.resolve(), PROFILES[args.profile])
    print(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
