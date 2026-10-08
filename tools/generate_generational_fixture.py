"""Build a reproducible, authored v0.7 generational world.

The accepted schema vector supplies the literal record envelopes. Generated
identifiers and chronology depend only on the requested dimensions.
"""
from __future__ import annotations

import argparse
from collections import Counter
from copy import deepcopy
import hashlib
import json
from pathlib import Path

import yaml

from wedl.ids import id_from_seed
from wedl.model import Record, World
from wedl.source import KIND_DIR, serialize_record


ROOT = Path(__file__).resolve().parents[1]
VECTOR = ROOT / "tests/fixtures/architecture/generational-schema-v07.yaml"


def _point(tick: int, order: int = 0) -> dict:
    return {"applicability_kind": "instant", "point":
            {"timeline": "main", "tick": tick, "order": order}}


def build_fixture(*, generations: int = 101, width: int = 50) -> tuple[World, dict]:
    """Return a linked fixture; (101, 50) exceeds the full scale contract."""
    if generations < 3 or width < 3:
        raise ValueError("generations and width must be at least three")
    vector = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    mapping = {
        **{key: id_from_seed("character", key)
           for key in vector["reference_catalog"]["characters"]},
        **{key: id_from_seed("location", key)
           for key in vector["reference_catalog"]["locations"]},
        **{key: id_from_seed("event", key)
           for key in vector["endpoint_vectors"]["events"]},
    }

    def replace(value):
        if isinstance(value, str):
            return mapping.get(value, value)
        if isinstance(value, list):
            result = [replace(item) for item in value]
            return sorted(result) if value and all(
                isinstance(item, str) and item.startswith("character_") for item in value
            ) else result
        if isinstance(value, dict):
            return {key: replace(item) for key, item in value.items()}
        return value

    records: dict[str, Record] = {}

    def add(data: dict, *, body: str = "") -> None:
        path = "story/world.md" if data["kind"] == "world" else (
            f"story/{KIND_DIR[data['kind']]}/{data['id']}.md")
        records[data["id"]] = Record(data, body, path, b"")

    for item in vector["frontmatter_vectors"].values():
        add(replace(deepcopy(item)))
    add(replace(deepcopy(vector["capability_registry"]["envelopes"]
                         ["generational_only"]["world"])))
    for original, identifier in mapping.items():
        kind = original.split("_", 1)[0]
        data = {"schema": "wedl/v0.7", "kind": kind, "id": identifier,
                "title": original, "domain": "fixtures.core", "status": "canonical",
                "tags": [], "aliases": []}
        if kind == "event":
            data["time"] = vector["endpoint_vectors"]["events"][original]["story_time"]
        add(data)

    # One generation is one authored cohort. Two different parents in the
    # previous cohort give exactly 2 * width * (generations - 1) new edges.
    cohorts = [[id_from_seed("character", f"fixture-g{g:03d}-p{i:03d}")
                for i in range(width)] for g in range(generations)]
    parent_template = vector["frontmatter_vectors"]["parentage"]
    for generation, cohort in enumerate(cohorts):
        for index, identifier in enumerate(cohort):
            add({"schema": "wedl/v0.7", "kind": "character", "id": identifier,
                 "title": f"Chronicle {generation:03d} {index:03d}",
                 "domain": "fixtures.generations", "status": "canonical",
                 "tags": [], "aliases": []})
            if generation == 0:
                continue
            for branch, parent_index in enumerate((index, (index + 1) % width)):
                seed = f"fixture-g{generation:03d}-p{index:03d}-b{branch}"
                data = replace(deepcopy(parent_template))
                data["id"] = id_from_seed("parentage", seed)
                data["title"] = f"Chronicle lineage {seed}"
                data["child_id"] = identifier
                data["parent_id"] = cohorts[generation - 1][parent_index]
                data["initialization"]["transition_id"] = id_from_seed(
                    "generational-transition", seed + "-init")
                data["initialization"]["applicability"] = _point(-100 + generation, branch)
                basis = "adoptive" if branch and generation % 7 == 0 else "biological"
                data["initialization"]["payload"] = {"basis": basis}
                data["tags"] = [basis]
                data["transitions"] = []
                add(data)

    # Extend the vector's linked organizations and its literal tenure/claim
    # examples with stable, time-ordered transition histories.
    org_template = vector["frontmatter_vectors"]["organization_child"]
    root_org = id_from_seed("organization", "fixture-dynasty")
    previous_org = root_org
    for index in range(3):
        data = replace(deepcopy(org_template))
        data["id"] = root_org if index == 0 else id_from_seed(
            "organization", f"fixture-house-{index}")
        data["title"] = f"Chronicle {'Dynasty' if index == 0 else f'House {index}'}"
        data["organization_kind"] = "dynasty" if index == 0 else "house"
        if index == 0:
            data.pop("parent_id", None)
        else:
            data["parent_id"] = previous_org
        data.pop("location_id", None)
        data["initialization"]["transition_id"] = id_from_seed(
            "generational-transition", f"fixture-org-{index}")
        data["initialization"]["applicability"] = _point(-100, index)
        data["initialization"]["payload"] = {"title": data["title"], "aliases": []}
        add(data)
        previous_org = data["id"]

    affiliation_template = vector["frontmatter_vectors"]["affiliation"]
    for index in range(min(width, 12)):
        data = replace(deepcopy(affiliation_template))
        data["id"] = id_from_seed("affiliation", f"fixture-role-{index}")
        data["title"] = f"Chronicle role {index:03d}"
        data["character_id"] = cohorts[0][index]
        data["organization_id"] = root_org
        data["audience"] = ["public"]
        data["perspectives"] = ["ordinary"]
        data["initialization"]["transition_id"] = id_from_seed(
            "generational-transition", f"fixture-role-{index}-init")
        data["initialization"]["applicability"] = _point(-100, 0)
        data["initialization"]["payload"] = {"role": "steward"}
        data["transitions"] = [{"transition_id": id_from_seed(
            "generational-transition", f"fixture-role-{index}-change"),
            "transition_kind": "affiliation-role", "applicability": _point(-50, index),
            "payload": {"role": "elder"}}]
        add(data)

    union_template = vector["frontmatter_vectors"]["union"]
    for index in range(3):
        data = replace(deepcopy(union_template))
        data["id"] = id_from_seed("union", f"fixture-union-{index}")
        data["title"] = f"Chronicle union {index}"
        members = sorted(cohorts[0][index:index + 3])
        if len(members) == 1:
            members = sorted([cohorts[0][index], cohorts[0][(index + 1) % width]])
        data["participant_ids"] = members
        data["initialization"]["transition_id"] = id_from_seed(
            "generational-transition", f"fixture-union-{index}-init")
        data["initialization"]["applicability"] = _point(-99, 0)
        data["initialization"]["payload"] = {"participant_ids": members}
        data["transitions"] = [{"transition_id": id_from_seed(
            "generational-transition", f"fixture-union-{index}-form"),
            "transition_kind": "union-form", "applicability": _point(-99, 1),
            "payload": {"participant_ids": members}}]
        add(data)

    legacy_template = vector["frontmatter_vectors"]["legacy"]
    tenure_template = vector["frontmatter_vectors"]["tenure"]
    claim_template = vector["frontmatter_vectors"]["claim"]
    history_width = 50 if generations >= 100 and width >= 50 else 2
    office_count = 5 if history_width == 50 else 2
    for office in range(office_count):
        legacy = replace(deepcopy(legacy_template))
        legacy["id"] = id_from_seed("legacy", f"fixture-office-{office}")
        legacy["title"] = f"Chronicle office {office}"
        legacy["organization_id"] = root_org
        legacy["initialization"]["transition_id"] = id_from_seed(
            "generational-transition", f"fixture-office-{office}-init")
        legacy["initialization"]["applicability"] = _point(-100, 0)
        legacy["initialization"]["payload"] = {"title": legacy["title"], "aliases": []}
        add(legacy)
        for position in range(history_width):
            seed = f"fixture-office-{office}-tenure-{position}"
            tenure = replace(deepcopy(tenure_template))
            tenure["id"] = id_from_seed("tenure", seed)
            tenure["title"] = f"Chronicle tenure {office} {position}"
            tenure["legacy_id"] = legacy["id"]
            tenure.pop("predecessor_tenure_id", None)
            if position:
                tenure["predecessor_tenure_id"] = id_from_seed(
                    "tenure", f"fixture-office-{office}-tenure-{position - 1}")
            tenure["initialization"]["transition_id"] = id_from_seed(
                "generational-transition", seed + "-init")
            tenure["initialization"]["applicability"] = _point(-90 + position, 0)
            tenure["initialization"]["payload"] = {
                "holder_id": cohorts[0][position % width],
                "basis": "legal" if position % 2 == 0 else "de-facto"}
            tenure["transitions"] = [{"transition_id": id_from_seed(
                "generational-transition", seed + "-vacate"),
                "transition_kind": "tenure-vacate",
                "applicability": {"applicability_kind": "inclusive-interval",
                                  "first": {"timeline": "main", "tick": -89 + position,
                                            "order": 0},
                                  "last": {"timeline": "main", "tick": -89 + position,
                                           "order": 1}},
                "payload": {"holder_id": None}}]
            # The final legal and de-facto office-zero holders remain active
            # together at the late horizon; earlier records show vacancies.
            if history_width == 50 and office == 0 and position >= 48:
                tenure["transitions"] = []
            add(tenure)
        for position in range(history_width):
            seed = f"fixture-office-{office}-claim-{position}"
            claim = replace(deepcopy(claim_template))
            claim["id"] = id_from_seed("claim", seed)
            claim["title"] = f"Chronicle claim {office} {position}"
            claim["legacy_id"] = legacy["id"]
            claim["claimant_id"] = cohorts[0][position % width]
            claim["initialization"]["transition_id"] = id_from_seed(
                "generational-transition", seed + "-init")
            claim["initialization"]["applicability"] = _point(-80 + position // 2, 0)
            rival = id_from_seed("claim", f"fixture-office-{office}-claim-{position ^ 1}")
            claim["initialization"]["payload"] = {"competes_with": [rival]}
            claim["transitions"] = [{"transition_id": id_from_seed(
                "generational-transition", seed + "-dispute"),
            "transition_kind": "claim-dispute", "applicability": _point(-80 + position // 2, 1),
                "payload": {"competes_with": [rival]}}]
            add(claim)

    world = World("fixture-revision", "fixture-tree", records, ROOT)
    counts = Counter(record.kind for record in records.values())
    answers = {"first": cohorts[0][0], "last": cohorts[-1][0],
               "boundedLineage": cohorts[2][0],
               "lastParents": [cohorts[-2][0], cohorts[-2][1]],
               "dynasty": root_org, "secretChild": mapping["character_secret"],
               "futureChild": mapping["character_future_lineage"],
               "legacy": id_from_seed("legacy", "fixture-office-0"),
               "legalHolder": cohorts[0][(history_width - 2) % width],
               "deFactoHolder": cohorts[0][(history_width - 1) % width],
               "tenureChain": [id_from_seed("tenure", f"fixture-office-0-tenure-{i}")
                               for i in range(history_width)],
               "biologicalChild": mapping["character_child"],
               "adoptiveParent": mapping["character_adoptive"],
               "biologicalParent": mapping["character_biological"]}
    manifest = {"counts": {"characters": counts["character"],
                           "generations": generations,
                           "kinshipEdges": counts["parentage"],
                           "transitions": sum(len(record.frontmatter.get("transitions", []))
                                              for record in records.values() if record.kind in
                                              {"tenure", "claim"})},
                "recordKinds": dict(sorted(counts.items())), "answers": answers}
    return world, manifest


def source_digest(world: World) -> str:
    digest = hashlib.sha256()
    for record in sorted(world.records.values(), key=lambda item: item.source_path):
        digest.update(record.source_path.encode("utf-8") + b"\0")
        digest.update(serialize_record(record.frontmatter, f"# {record.title}\n"))
    return digest.hexdigest()


def write_fixture(destination: Path, world: World) -> None:
    if any(destination.iterdir()):
        raise ValueError("destination must be empty")
    for record in sorted(world.records.values(), key=lambda item: item.source_path):
        path = destination / record.source_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(record.frontmatter, f"# {record.title}\n"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--generations", type=int, default=101)
    parser.add_argument("--width", type=int, default=50)
    args = parser.parse_args()
    world, manifest = build_fixture(generations=args.generations, width=args.width)
    args.destination.mkdir(parents=True, exist_ok=True)
    write_fixture(args.destination, world)
    manifest["sourceSha256"] = source_digest(world)
    print(json.dumps(manifest, sort_keys=True))


if __name__ == "__main__":
    main()
