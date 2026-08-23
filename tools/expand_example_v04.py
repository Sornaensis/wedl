from __future__ import annotations

import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from wedl import SOURCE_SCHEMA
from wedl.changeset import apply, preview
from wedl.ids import id_from_seed
from wedl.repository import Repository
from wedl.util import slugify


def ident(kind: str, name: str) -> str:
    return id_from_seed(kind, f"ash-archive-v0.4:{name}")


def point(tick: int, order: int = 0) -> dict[str, Any]:
    return {"timeline": "main", "tick": tick, "order": order}


def common(kind: str, entity_id: str, title: str, domain: str, status: str = "canonical", tags: list[str] | None = None) -> dict[str, Any]:
    return {
        "schema": SOURCE_SCHEMA,
        "kind": kind,
        "id": entity_id,
        "title": title,
        "domain": domain,
        "status": status,
        "tags": tags or [],
        "aliases": [],
    }


def operation(frontmatter: dict[str, Any], body: str) -> dict[str, Any]:
    return {"type": "entity.upsert", "value": {"frontmatter": frontmatter, "bodyMarkdown": body}}


def turn(prefix: str, index: int, tick: int, order: int, speaker: str, text: str, *, delivery: str = "", audience: list[str] | None = None) -> dict[str, Any]:
    value = {
        "id": ident("conversation-turn", f"{prefix}:{index}"),
        "at": point(tick, order),
        "speaker": speaker,
        "text": text,
        "audience": audience or ["participants"],
    }
    if delivery:
        value["delivery"] = delivery
    return value


def recollection(prefix: str, index: int, character: str, tick: int, order: int, summary: str, interpretation: str, emotion: str, confidence: float, *, exact: list[str] | None = None, approximate: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    return {
        "id": ident("conversation-recollection", f"{prefix}:{index}:{character}"),
        "character": character,
        "at": point(tick, order),
        "state": "remembered",
        "summary": summary,
        "interpretation": interpretation,
        "emotional_impression": emotion,
        "confidence": confidence,
        "exact_turns": exact or [],
        "remembered_quotes": approximate or [],
    }


def effect(prefix: str, target: str, key: str, operation_name: str, value: Any | None = None) -> dict[str, Any]:
    result = {"id": ident("effect", prefix), "target": target, "key": key, "operation": operation_name}
    if operation_name != "clear":
        result["value"] = value
    return result


def event(entity_id: str, title: str, tick: int, order: int, location: str, participants: list[tuple[str, str]], body: str, *, causes: list[str] | None = None, story_points: list[str] | None = None, effects: list[dict[str, Any]] | None = None, tags: list[str] | None = None) -> tuple[dict[str, Any], str]:
    fm = common("event", entity_id, title, "plot.ash-archive.second-act", tags=tags)
    fm.update({
        "time": point(tick, order),
        "location": location,
        "participants": [{"character": character, "role": role} for character, role in participants],
        "causes": causes or [],
        "related_story_points": story_points or [],
        "effects": effects or [],
    })
    return fm, f"# {title}\n\n{body}\n"


def knowledge(entity_id: str, title: str, knower: str, claim_key: str, statement: str, tick: int, order: int, state: str, confidence: float, acquisition: str, *, cause: str | None = None, source: str | None = None, truth: str = "unknown", subject: str | None = None, predicate: str | None = None, object_value: Any | None = None, note: str | None = None) -> tuple[dict[str, Any], str]:
    claim: dict[str, Any] = {"key": claim_key, "statement": statement, "author_truth_status": truth}
    if subject:
        claim["subject"] = subject
    if predicate:
        claim["predicate"] = predicate
    if object_value is not None:
        claim["object"] = object_value
    transition = {
        "id": ident("knowledge-transition", f"{entity_id}:0"),
        "time": point(tick, order),
        "state": state,
        "confidence": confidence,
        "acquisition": acquisition,
        "causing_event": cause,
        "source_entity": source,
    }
    if note:
        transition["note"] = note
    fm = common("knowledge", entity_id, title, f"knowledge.{slugify(knower)}", tags=["second-act"])
    fm.update({"knower": knower, "claim": claim, "transitions": [transition]})
    return fm, f"# {title}\n\n{statement}\n\nA subjective knowledge record for the named character.\n"


def relationship(entity_id: str, title: str, source: str, target: str, inverse: str, kind: str, transitions: list[dict[str, Any]], body: str) -> tuple[dict[str, Any], str]:
    fm = common("relationship", entity_id, title, "relationships.second-act", tags=[source, target])
    fm.update({"from": source, "to": target, "relationship_kind": kind, "inverse": inverse, "transitions": transitions})
    return fm, f"# {title}\n\n{body}\n"


def rel_transition(prefix: str, tick: int, order: int, trust: float, affinity: float, fear: float, obligation: float, facets: list[str], note: str, cause: str | None = None) -> dict[str, Any]:
    return {
        "id": ident("relationship-transition", prefix),
        "time": point(tick, order),
        "relationship_status": "active",
        "metrics": {"trust": trust, "affinity": affinity, "fear": fear, "obligation": obligation},
        "facets": facets,
        "causing_event": cause,
        "note": note,
    }


def story_point(entity_id: str, title: str, priority: int, trigger: dict[str, Any], body: str, *, transitions: list[dict[str, Any]] | None = None, outcomes: list[str] | None = None, dependencies: list[dict[str, Any]] | None = None, status: str = "dormant") -> tuple[dict[str, Any], str]:
    fm = common("story-point", entity_id, title, "plot.ash-archive.second-act", tags=["second-act"])
    fm.update({
        "lifecycle": {"initial_state": status, "transitions": transitions or []},
        "activation_policy": "manual",
        "priority": priority,
        "repeat_policy": "once",
        "dependencies": {"all": dependencies or []},
        "trigger": trigger,
        "on_activate": {"create_draft_scene": False},
        "outcome_events": outcomes or [],
    })
    return fm, f"# {title}\n\n{body}\n"


def spt(prefix: str, tick: int, order: int, state: str, event_id: str, note: str) -> dict[str, Any]:
    return {"id": ident("story-point-transition", prefix), "time": point(tick, order), "state": state, "causing_event": event_id, "note": note}


def character(entity_id: str, title: str, role: str, location: str, summary: str, voice: str, goals: str, author_notes: str) -> tuple[dict[str, Any], str]:
    fm = common("character", entity_id, title, "cast.cindervale", tags=[role])
    fm.update({
        "pronouns": {"subject": "she" if title != "Halver Rook" else "he", "object": "her" if title != "Halver Rook" else "him", "possessive": "her" if title != "Halver Rook" else "his"},
        "role": role,
        "initial_state": {"location": {"entity": location}, "condition": "alert"},
        "section_audiences": {
            "Summary": ["public", "self", "author"],
            "Appearance": ["public", "self", "author"],
            "Voice": ["public", "self", "author"],
            "Goals": ["self", "author"],
            "Author notes": ["author"],
        },
    })
    body = f"# {title}\n\n## Summary\n\n{summary}\n\n## Appearance\n\nPractical civic clothing marked by the tools and obligations of the role.\n\n## Voice\n\n{voice}\n\n## Goals\n\n{goals}\n\n## Author notes\n\n{author_notes}\n"
    return fm, body


def location(entity_id: str, title: str, location_type: str, parent: str, description: str) -> tuple[dict[str, Any], str]:
    fm = common("location", entity_id, title, "setting.cindervale.second-act", tags=[location_type])
    fm.update({"location_type": location_type, "parent": parent, "links": []})
    return fm, f"# {title}\n\n{description}\n"


def object_record(entity_id: str, title: str, object_type: str, initial_state: dict[str, Any], description: str) -> tuple[dict[str, Any], str]:
    fm = common("object", entity_id, title, "props.ash-archive.second-act", tags=[object_type])
    fm.update({"object_type": object_type, "initial_state": initial_state, "capabilities": []})
    return fm, f"# {title}\n\n{description}\n"


def environment(entity_id: str, title: str, start: int, end: int, targets: list[str], conditions: dict[str, Any], sensory: list[str], description: str) -> tuple[dict[str, Any], str]:
    fm = common("environment", entity_id, title, "environment.second-act", tags=["second-act"])
    fm.update({"time": {"start": point(start), "end": point(end)}, "targets": targets, "conditions": conditions, "sensory": sensory})
    return fm, f"# {title}\n\n{description}\n"


def observation(prefix: str, tick: int, order: int, audience: list[str], text: str, salience: float = 1.0) -> dict[str, Any]:
    return {"id": ident("observation", prefix), "at": point(tick, order), "audience": audience, "salience": salience, "text": text}


def _point_value(value: int | tuple[int, int] | None) -> dict[str, Any] | None:
    if value is None:
        return None
    return point(*value) if isinstance(value, tuple) else point(value)


def scene(entity_id: str, title: str, status: str, start: int, current: tuple[int, int], end: int | tuple[int, int] | None, location_id: str, participants: list[dict[str, Any]], objects: list[str], environments: list[str], story_points: list[str], conversations: list[str], observations: list[dict[str, Any]], constraints: list[str], body: str) -> tuple[dict[str, Any], str]:
    fm = common("scene", entity_id, title, "scenes.ash-archive.second-act", status=status, tags=[status, "second-act"])
    fm.update({
        "time": {"start": point(start), "current": point(*current), "end": _point_value(end)},
        "location": location_id,
        "participants": participants,
        "objects": objects,
        "environments": environments,
        "story_points": story_points,
        "observations": observations,
        "conversations": conversations,
        "author_constraints": constraints,
    })
    return fm, f"# {title}\n\n{body}\n"


def conversation(entity_id: str, title: str, status: str, start: int, end: int | tuple[int, int] | None, scene_id: str, location_id: str, participants: list[dict[str, Any]], turns: list[dict[str, Any]], recollections: list[dict[str, Any]], body: str, topics: list[str]) -> tuple[dict[str, Any], str]:
    fm = common("conversation", entity_id, title, "conversations.ash-archive.second-act", status=status, tags=topics)
    fm.update({
        "time": {"start": point(start), "end": _point_value(end)},
        "scene": scene_id,
        "location": location_id,
        "participants": participants,
        "topics": topics,
        "turns": turns,
        "recollections": recollections,
    })
    return fm, f"# {title}\n\n{body}\n\nThe turn list is canonical verbatim provenance. Recollections are character-owned interpretations and may diverge.\n"


def build(repository: Repository) -> dict[str, Any]:
    world = repository.load_world("HEAD")
    by_title = {record.title: record for record in world.records.values()}

    # Existing IDs.
    MARA = by_title["Mara Vale"].id
    NESSA = by_title["Nessa Quill"].id
    YSABET = by_title["Ysabet Crane"].id
    RUSK = by_title["Captain Tomas Rusk"].id
    CALDRIN = by_title["Councillor Caldrin Vey"].id
    ILYRA = by_title["Ilyra Sorn"].id
    EDRIN = by_title["Brother Edrin Holt"].id
    SABLE = by_title["Sable Fen"].id
    FLOOD = by_title["Flood Gallery N"].id
    ARCHIVE = by_title["Ash Archive"].id
    COUNCIL = by_title["Council House Ember Hall"].id
    RIVER_GATE = by_title["River Gate"].id
    VAULT = by_title["Restricted Vault"].id
    LETTER = by_title["Sealed Heron Letter"].id
    REGISTER = by_title["Flood Register N-7B"].id
    RIBBON = by_title["Ilyra's Graphite Ribbon"].id
    COUNTERSEAL = by_title["Archive Counterseal Fragment"].id
    CURRENT_SCENE = by_title["Flood Gallery N"].id
    CURRENT_CONV = by_title["Whispers in Flood Gallery N"].id
    SP_TRUST = by_title["Decide Whether to Trust Ysabet"].id
    SP_ESCAPE = by_title["Escape Before Rusk Opens the North Door"].id
    SP_BREAK = by_title["Break the Seal"].id
    SP_FIND = by_title["Find Ilyra Sorn"].id

    ANSEL = ident("character", "sister-ansel-marr")
    HALVER = ident("character", "halver-rook")
    MIRA = ident("character", "mira-sen")

    FLOOD_STAIR = ident("location", "south-bank-flood-stair")
    PRESSURE_GATE = ident("location", "pressure-gate-seven")
    LISTENING_OFFICE = ident("location", "council-listening-office")
    EVIDENCE_CLOISTER = ident("location", "evidence-cloister")
    UNDERCROFT = ident("location", "vault-undercroft")
    SHUTTER_CHAMBER = ident("location", "north-shutter-chamber")

    EVIDENCE_TUBE = ident("object", "notarial-evidence-tube")
    LISTENING_LEDGER = ident("object", "listening-office-ledger")
    REQUISITION_RING = ident("object", "rusk-requisition-ring")
    PRESSURE_KEY = ident("object", "pressure-gate-key")
    UNDERCROFT_LEDGER = ident("object", "undercroft-descendant-ledger")
    WITNESS_COPY = ident("object", "witness-copy-flood-register")
    COURIER_CORD = ident("object", "erased-wing-courier-cord")

    ENV_SURGE = ident("environment", "flood-stair-surge")
    ENV_DAWN = ident("environment", "dawn-through-grates")
    ENV_OFFICE = ident("environment", "listening-office-echo")
    ENV_HEARING = ident("environment", "ember-hall-hearing-crowd")
    ENV_UNDERCROFT = ident("environment", "undercroft-condensation")
    ENV_ALARM = ident("environment", "vault-fire-alarm")

    SC_EVIDENCE = ident("scene", "the-evidence-choice")
    SC_GATE = ident("scene", "pressure-gate-seven")
    SC_OFFICE = ident("scene", "the-dead-bell-office")
    SC_HEARING = by_title["The Ledger Hearing"].id
    SC_UNDER = ident("scene", "under-the-vault")
    SC_RECORDS = ident("scene", "the-choice-of-records")
    SC_PURSUIT = ident("scene", "river-gate-pursuit")
    SC_RECKONING = ident("scene", "ember-hall-reckoning")

    CONV_EVIDENCE = ident("conversation", "the-evidence-choice")
    CONV_GATE = ident("conversation", "the-pressure-gate")
    CONV_LETTER = ident("conversation", "the-letter-under-water")
    CONV_CLERK = ident("conversation", "the-clerks-ledger")
    CONV_OFFICE = ident("conversation", "the-dead-bell-office")
    CONV_HEARING = ident("conversation", "the-ledger-hearing")
    CONV_DEPOSITION = ident("conversation", "rusks-deposition")
    CONV_ILYRA = ident("conversation", "the-chief-archivists-account")
    CONV_RECORDS = ident("conversation", "the-choice-of-records")
    CONV_ANSEL = ident("conversation", "ansels-warning")

    E_SEAL = ident("event", "ysabet-seals-flood-register")
    E_COPY = ident("event", "nessa-copies-requisition-ciphers")
    E_RUSK_DOOR = ident("event", "rusk-reaches-north-door")
    E_ESCAPE = ident("event", "mara-escapes-through-flood-stair")
    E_GATE = ident("event", "ansel-opens-pressure-gate")
    E_LETTER = ident("event", "mara-opens-heron-letter")
    E_OFFICE = ident("event", "listening-office-entered")
    E_LEDGER = ident("event", "listening-ledger-recovered")
    E_HALVER = ident("event", "halver-gives-testimony")
    E_RING = ident("event", "rusk-ring-matches-cipher")
    E_HEARING = ident("event", "ledger-hearing-convenes")
    E_CALDRIN = ident("event", "caldrin-disavows-rusk")
    E_RUSK = ident("event", "rusk-accuses-caldrin")
    E_RULE = ident("event", "ysabet-rules-chain-admissible")
    E_ROUTE = ident("event", "letter-route-opens-undercroft")
    E_FIND = ident("event", "mara-finds-ilyra")
    E_ACCOUNT = ident("event", "ilyra-explains-descendant-ledger")
    E_COPY_LEDGER = ident("event", "nessa-makes-witness-copy")
    E_ALARM = ident("event", "vault-fire-alarm-sounds")
    E_CHOICE = ident("event", "records-dispute-begins")

    SP_CUSTODY = ident("story-point", "choose-custodian-of-register")
    SP_GATE = ident("story-point", "pass-pressure-gate-seven")
    SP_OPEN = ident("story-point", "open-letter-under-witness")
    SP_OFFICE = ident("story-point", "expose-listening-office")
    SP_HALVER = ident("story-point", "test-halvers-testimony")
    SP_HEARING = ident("story-point", "conduct-ledger-hearing")
    SP_CHAIN = ident("story-point", "establish-evidence-chain")
    SP_LEDGER = ident("story-point", "decide-fate-descendant-ledger")
    SP_ALARM = ident("story-point", "survive-vault-alarm")

    operations: list[dict[str, Any]] = []
    def add(value: tuple[dict[str, Any], str]) -> None:
        fm, body = value
        operations.append(operation(fm, body))

    # Characters.
    add(character(ANSEL, "Sister Ansel Marr", "hydraulic-keeper", FLOOD_STAIR,
        "A lay sister who maintains the south-bank pressure gates and treats water levels as civic testimony.",
        "Low, spare, and rhythmically deliberate. She counts before answering dangerous questions.",
        "Keep the lower gates intact, decide whether Mara's evidence justifies opening Gate Seven, and prevent the Archive foundations from flooding.",
        "Ansel helped Ilyra inspect the undercroft months ago, but she does not know Ilyra remained there."))
    add(character(HALVER, "Halver Rook", "watch-records-clerk", LISTENING_OFFICE,
        "A junior watch clerk who copied requisition ciphers without understanding the physical routes behind them.",
        "Careful, apologetic, and over-specific when frightened. He remembers ledger columns better than faces.",
        "Survive the hearing, prove which entries he copied, and avoid being made the sole author of Captain Rusk's paperwork.",
        "Halver changed one date after Rusk threatened his sister's ferry license. He will admit it only under direct documentary contradiction."))
    add(character(MIRA, "Mira Sen", "acoustic-engineer", LISTENING_OFFICE,
        "Varo Pell's former apprentice and the engineer assigned to decommission the Council listening diaphragms.",
        "Fast, technical, and impatient with metaphor. She answers a political question by describing a mechanism.",
        "Prove the listening office remained active after its closure order and keep Varo from being blamed for the Council retrofit.",
        "Mira retained a private calibration strip showing which office heard which bell-tube branch."))

    # Locations.
    add(location(FLOOD_STAIR, "South-Bank Flood Stair", "service-stair", RIVER_GATE, "A barred stair descending from the embankment to the city's oldest pressure gates. The seventh landing shares brickwork with the Archive undercroft."))
    add(location(PRESSURE_GATE, "Pressure Gate Seven", "hydraulic-gate", FLOOD_STAIR, "A wheel-operated flood gate whose brass index uses the same seven-B notation as the missing catalog cards."))
    add(location(LISTENING_OFFICE, "Council Listening Office", "inspection-office", COUNCIL, "A decommissioned inspection room above the Archive. Felt-lined speaking tubes terminate behind a locked ledger desk."))
    add(location(EVIDENCE_CLOISTER, "Evidence Cloister", "legal-cloister", COUNCIL, "A narrow stone walk where witnesses and sealed objects wait outside Ember Hall under notarial supervision."))
    add(location(UNDERCROFT, "Vault Undercroft", "undercroft", VAULT, "A dry chamber between the Restricted Vault and the old pressure galleries, reachable by a false north shelf and Gate Seven's maintenance passage."))
    add(location(SHUTTER_CHAMBER, "North Shutter Chamber", "fire-shutter-room", UNDERCROFT, "A circular chamber containing the Archive's original fire shutters and a hand bell connected to every upper floor."))

    # Objects.
    add(object_record(EVIDENCE_TUBE, "Notarial Evidence Tube", "sealed-container", {"holder": {"entity": YSABET}, "condition": "empty and sealed-ready"}, "A brass-ended tube whose numbered wax collar can establish custody without revealing the document inside."))
    add(object_record(LISTENING_LEDGER, "Listening Office Ledger", "ledger", {"location": {"entity": LISTENING_OFFICE}, "condition": "locked in desk"}, "A log of bell-tube activations, copied phrases, and the Council officials who requested each listening session."))
    add(object_record(REQUISITION_RING, "Rusk Requisition Ring", "cipher-ring", {"holder": {"entity": RUSK}, "condition": "worn"}, "A rotating brass ring used to stamp watch requisition ciphers. One tooth carries the same defect found in Flood Register N-7B."))
    add(object_record(PRESSURE_KEY, "Pressure Gate Seven Key", "hydraulic-key", {"holder": {"entity": ANSEL}, "condition": "oiled"}, "A long forked key that disengages the flood gate's pressure lock after the water count is verified."))
    add(object_record(UNDERCROFT_LEDGER, "Undercroft Descendant Ledger", "private-ledger", {"location": {"entity": UNDERCROFT}, "condition": "wrapped in red waxcloth"}, "Ilyra's complete list of living descendants evacuated through illegal river channels. Preserving it protects history and endangers every person named."))
    add(object_record(WITNESS_COPY, "Witness Copy of Flood Register N-7B", "document-copy", {"location": {"entity": FLOOD}, "condition": "unfinished"}, "Nessa's line-for-line copy of the surviving flood-register page, including spacing, abbreviations, and ink breaks."))
    add(object_record(COURIER_CORD, "Erased-Wing Courier Cord", "courier-token", {"holder": {"entity": ILYRA}, "condition": "frayed"}, "A red-and-gray cord from the emergency courier network, knotted in the sequence used to authenticate an undercroft route."))

    # Environments.
    add(environment(ENV_SURGE, "Flood-Stair Surge", 143, 150, [FLOOD_STAIR, PRESSURE_GATE], {"water_level": "rising", "safe_cycles": 3}, ["Water strikes the lower stair in timed pulses.", "The pressure wheel groans before each surge."], "The stair is passable only between pressure cycles."))
    add(environment(ENV_DAWN, "Dawn Through the River Grates", 147, 151, [FLOOD_STAIR, PRESSURE_GATE], {"light": "cold dawn", "visibility": "striped"}, ["Pale river light cuts through iron grates.", "Every wet footprint becomes visible."], "Dawn makes escape easier to navigate and harder to conceal."))
    add(environment(ENV_OFFICE, "Listening-Office Echo", 152, 159, [LISTENING_OFFICE], {"sound": "amplified whispers", "privacy": "false"}, ["A whisper at one tube returns from two others.", "Old felt releases coal dust when touched."], "The room makes every conversation feel overheard even when the network is disconnected."))
    add(environment(ENV_HEARING, "Ember Hall Hearing Crowd", 160, 167, [SC_HEARING, COUNCIL], {"audience": "full galleries", "procedure": "public record"}, ["Clerks sharpen pens whenever a witness pauses.", "The galleries murmur at every named requisition."], "Public procedure turns every exact phrase into evidence and performance."))
    add(environment(ENV_UNDERCROFT, "Undercroft Condensation", 169, 181, [UNDERCROFT, SHUTTER_CHAMBER], {"air": "cold and wet", "paper_risk": "high"}, ["Moisture beads beneath the false shelf.", "Waxcloth crackles louder than speech."], "The hidden chamber protects people poorly and paper worse."))
    add(environment(ENV_ALARM, "Vault Fire Alarm", 177, 184, [SC_RECORDS, UNDERCROFT, SHUTTER_CHAMBER], {"bells": "continuous", "shutters": "closing"}, ["The old hand bell shakes dust from every joint.", "Fire shutters descend one tooth at a time."], "The alarm imposes a final time limit on the argument over the ledger."))

    # IDs for new conversations are referenced by scenes and vice versa.
    # Extend and close current Flood Gallery conversation/scene.
    current_conv = deepcopy(world.get(CURRENT_CONV).frontmatter)
    current_conv["status"] = "closed"
    current_conv["time"]["end"] = point(142, 40)
    extra_turns = [
        turn("flood-choice", 0, 140, 0, MARA, "The register goes under your seal. Nessa keeps the ribbon.", delivery="decisive"),
        turn("flood-choice", 1, 140, 10, YSABET, "Then I can prove what was removed, but not who left the route.", delivery="measured"),
        turn("flood-choice", 2, 140, 20, NESSA, "That is the point. Evidence for the hall; clue for the search.", delivery="impatient"),
        turn("flood-choice", 3, 141, 0, YSABET, "Captain Rusk is at the north door.", delivery="low"),
        turn("flood-choice", 4, 141, 10, MARA, "Seal it. We leave by the water stairs.", delivery="formal"),
        turn("flood-choice", 5, 142, 20, RUSK, "Open this door in the name of the watch.", delivery="through iron", audience=[RUSK]),
    ]
    current_conv.setdefault("turns", []).extend(extra_turns)
    current_conv.setdefault("participants", []).append({"character": RUSK, "role": "outside-door", "from": point(142, 20), "to": point(142, 40)})
    current_conv.setdefault("recollections", []).extend([
        recollection("flood-choice", 0, MARA, 143, 0, "Ysabet accepted the register while Nessa retained Ilyra's ribbon; Rusk reached the door after the choice was made.", "The notary chose an admissible chain, not personal loyalty.", "relief under pressure", 0.91, exact=[extra_turns[1]["id"], extra_turns[4]["id"]]),
        recollection("flood-choice", 1, NESSA, 143, 0, "Mara divided proof from clue and trusted Ysabet with only the proof.", "Mara understood Nessa's objection before she voiced it.", "vindicated", 0.88, exact=[extra_turns[0]["id"], extra_turns[2]["id"]]),
        recollection("flood-choice", 2, YSABET, 143, 0, "Mara surrendered the most admissible object and retained the most dangerous lead.", "The division was rational and legally risky.", "professional respect", 0.94, exact=[extra_turns[0]["id"]]),
        recollection("flood-choice", 3, RUSK, 143, 0, "Voices stopped behind the door before he ordered it opened.", "The archivists fled with evidence while the notary delayed him.", "controlled anger", 0.62, approximate=[{"source_turn": extra_turns[5]["id"], "speaker": RUSK, "text": "Open in the name of the watch.", "fidelity": "approximate"}]),
    ])
    add((current_conv, world.get(CURRENT_CONV).body + "\nThe conversation closes with the register sealed and Rusk outside the north door.\n"))

    current_scene = deepcopy(world.get(CURRENT_SCENE).frontmatter)
    current_scene["status"] = "closed"
    current_scene["time"]["current"] = point(142, 40)
    current_scene["time"]["end"] = point(142, 40)
    for participant in current_scene.get("participants") or []:
        participant["to"] = point(142, 40)
    current_scene["participants"].append({"character": RUSK, "role": "outside-door", "point_of_view": False, "from": point(142, 20), "to": point(142, 40)})
    current_scene.setdefault("objects", []).extend([EVIDENCE_TUBE, WITNESS_COPY])
    current_scene.setdefault("observations", []).extend([
        observation("flood-register-sealed", 140, 30, [MARA, NESSA, YSABET], "Ysabet's numbered wax collar closes around the flood register without exposing its pages.", 1.4),
        observation("flood-rusk-door", 142, 20, [MARA, NESSA, YSABET], "Rusk's requisition ring strikes the north door twice before his voice follows.", 1.3),
        observation("flood-rusk-only", 142, 20, [RUSK], "The room beyond the door falls silent, but water continues through a lower passage.", 1.1),
    ])
    add((current_scene, world.get(CURRENT_SCENE).body + "\nThe scene closes after Mara assigns the register to Ysabet and escapes with Nessa through the flood stair.\n"))

    # Events 140-142.
    add(event(E_SEAL, "Ysabet Seals Flood Register N-7B", 140, 30, FLOOD, [(YSABET, "notary"), (MARA, "custodian"), (NESSA, "witness")], "Ysabet places the original flood register in the numbered evidence tube and records Mara as the delivering custodian.", story_points=[SP_CUSTODY, SP_TRUST], effects=[effect("register-to-ysabet", REGISTER, "holder", "set", {"entity": YSABET}), effect("tube-to-ysabet", EVIDENCE_TUBE, "holder", "set", {"entity": YSABET})], tags=["evidence", "custody"]))
    add(event(E_COPY, "Nessa Completes the Witness Copy", 140, 40, FLOOD, [(NESSA, "copyist"), (MARA, "witness")], "Nessa copies every surviving line and ink break before Ysabet closes the evidence tube.", causes=[E_SEAL], effects=[effect("copy-to-nessa", WITNESS_COPY, "holder", "set", {"entity": NESSA}), effect("copy-condition", WITNESS_COPY, "condition", "set", "complete")], tags=["copy", "provenance"]))
    add(event(E_RUSK_DOOR, "Rusk Reaches the North Door", 142, 20, FLOOD, [(RUSK, "watch-captain"), (MARA, "fleeing-archivist"), (NESSA, "fleeing-assistant"), (YSABET, "notary")], "Rusk reaches the sealed north door after the evidence decision and orders it opened.", causes=[E_SEAL], story_points=[SP_ESCAPE], effects=[effect("rusk-flood", RUSK, "location", "set", {"entity": FLOOD})], tags=["pursuit"]))
    add(event(E_ESCAPE, "Mara and Nessa Enter the Flood Stair", 142, 40, FLOOD_STAIR, [(MARA, "fugitive"), (NESSA, "guide"), (YSABET, "covering-notary")], "Mara and Nessa take the lower passage while Ysabet remains at the north door with the sealed register.", causes=[E_RUSK_DOOR], story_points=[SP_ESCAPE, SP_GATE], effects=[effect("mara-stair", MARA, "location", "set", {"entity": FLOOD_STAIR}), effect("nessa-stair", NESSA, "location", "set", {"entity": FLOOD_STAIR})], tags=["escape", "flood-stair"]))

    # Pressure gate scene and conversations.
    gate_turns = [
        turn("pressure-gate", 0, 143, 10, ANSEL, "Stop at the dry mark. The next surge takes anyone below it.", delivery="counting"),
        turn("pressure-gate", 1, 143, 20, MARA, "We need Gate Seven open before the watch reaches the lower stair.", delivery="formal"),
        turn("pressure-gate", 2, 143, 30, ANSEL, "Need is not a water level. Show me why the gate should know you.", delivery="flat"),
        turn("pressure-gate", 3, 144, 0, NESSA, "Token seven-B, Varo's repair route, and Ilyra's ribbon.", delivery="rapid"),
        turn("pressure-gate", 4, 144, 10, ANSEL, "Ilyra said the ribbon would arrive with someone who distrusted its answer.", delivery="quiet"),
        turn("pressure-gate", 5, 144, 20, MARA, "Then she described me accurately.", delivery="dry"),
        turn("pressure-gate", 6, 145, 0, ANSEL, "Three breaths after the wheel drops. Do not touch the black rail.", delivery="instruction"),
        turn("pressure-gate", 7, 145, 20, NESSA, "What happens if I touch it?", delivery="breathless"),
        turn("pressure-gate", 8, 145, 30, ANSEL, "The river corrects you.", delivery="matter-of-fact"),
    ]
    add(conversation(CONV_GATE, "The Pressure Gate", "closed", 143, 145, SC_GATE, PRESSURE_GATE,
        [{"character": MARA, "role": "petitioner", "from": point(143), "to": point(145, 30)}, {"character": NESSA, "role": "assistant", "from": point(143), "to": point(145, 30)}, {"character": ANSEL, "role": "keeper", "from": point(143, 10), "to": point(145, 30)}],
        gate_turns,
        [
            recollection("pressure-gate", 0, MARA, 146, 0, "Ansel recognized Ilyra's ribbon and opened Gate Seven after giving exact surge instructions.", "Ilyra prepared the route for a skeptical successor.", "reluctant reassurance", 0.89, exact=[gate_turns[4]["id"], gate_turns[6]["id"]]),
            recollection("pressure-gate", 1, NESSA, 146, 0, "Ansel trusted the ribbon before she trusted Mara and treated the river as a stricter authority than the watch.", "The keeper knew Ilyra expected them.", "awed urgency", 0.86, exact=[gate_turns[0]["id"], gate_turns[8]["id"]]),
            recollection("pressure-gate", 2, ANSEL, 146, 0, "Mara answered caution with caution rather than claiming Ilyra's authority.", "She may be the archivist Ilyra intended to reach.", "guarded approval", 0.77, exact=[gate_turns[5]["id"]]),
        ],
        "Sister Ansel tests the evidence route before opening Pressure Gate Seven.", ["flood", "ansel", "escape"]))
    letter_turns = [
        turn("letter-water", 0, 147, 10, NESSA, "The seal is soft again. The water warmed the wax through your coat.", delivery="technical"),
        turn("letter-water", 1, 147, 20, MARA, "Ansel, witness that the seal is intact before I break it.", delivery="formal"),
        turn("letter-water", 2, 147, 30, ANSEL, "Intact. Erased left wing. Open it before the next surge.", delivery="counting"),
        turn("letter-water", 3, 148, 0, MARA, "Ilyra is alive beneath the Restricted Vault.", delivery="reading"),
        turn("letter-water", 4, 148, 10, NESSA, "Alive?", delivery="disbelieving"),
        turn("letter-water", 5, 148, 20, MARA, "The north shelf opens from Gate Seven. Trust no intact heron seal.", delivery="reading"),
        turn("letter-water", 6, 148, 30, ANSEL, "Then the Council signet is a warning, not a key.", delivery="quiet"),
        turn("letter-water", 7, 149, 0, MARA, "Or a warning someone expected us to misunderstand.", delivery="controlled"),
    ]
    add(conversation(CONV_LETTER, "The Letter Under Water", "closed", 147, 149, SC_GATE, PRESSURE_GATE,
        [{"character": MARA, "role": "reader", "from": point(147), "to": point(149)}, {"character": NESSA, "role": "listener", "from": point(147), "to": point(149)}, {"character": ANSEL, "role": "witness", "from": point(147), "to": point(149)}],
        letter_turns,
        [
            recollection("letter-water", 0, MARA, 150, 0, "The letter says Ilyra is alive beneath the vault and warns against intact heron seals.", "The warning is genuine but may be designed to redirect suspicion.", "relief constrained by method", 0.96, exact=[letter_turns[3]["id"], letter_turns[5]["id"]]),
            recollection("letter-water", 1, NESSA, 150, 0, "Mara read that Ilyra is alive and that Gate Seven reaches the north shelf.", "Mara's first response was suspicion rather than relief.", "hope and frustration", 0.94, exact=[letter_turns[3]["id"], letter_turns[7]["id"]]),
            recollection("letter-water", 2, ANSEL, 150, 0, "The erased-wing letter identifies the undercroft route and rejects intact heron authority.", "Ilyra expected the Council signet to be used as false authentication.", "old fear confirmed", 0.9, exact=[letter_turns[5]["id"], letter_turns[6]["id"]]),
        ],
        "Mara breaks the heron seal under witness and reads Ilyra's route aloud.", ["letter", "ilyra", "witness"]))
    add(scene(SC_GATE, "Pressure Gate Seven", "closed", 143, (149, 20), 150, PRESSURE_GATE,
        [{"character": MARA, "role": "viewpoint", "point_of_view": True, "from": point(143), "to": point(150)}, {"character": NESSA, "role": "assistant", "point_of_view": False, "from": point(143), "to": point(150)}, {"character": ANSEL, "role": "keeper", "point_of_view": False, "from": point(143, 10), "to": point(150)}],
        [LETTER, RIBBON, PRESSURE_KEY], [ENV_SURGE, ENV_DAWN], [SP_GATE, SP_OPEN, SP_FIND], [CONV_GATE, CONV_LETTER],
        [observation("gate-dry-mark", 143, 10, [MARA, NESSA], "A white mineral line marks the highest safe step above the surge.", 1.2), observation("letter-open", 148, 0, [MARA], "Ilyra's hand is unmistakable in the first line, even where the ink bled through damp paper.", 1.4), observation("ansel-signet", 148, 30, [ANSEL], "Ansel recognizes Caldrin's intact heron signet as the exact emblem Ilyra warned against.", 1.3)],
        ["The letter contents become known only to the three present witnesses.", "Opening the letter resolves its seal but does not prove every assertion inside it."],
        "Mara and Nessa meet Sister Ansel, pass the pressure gate, and open Ilyra's letter under witness."))
    add(event(E_GATE, "Ansel Opens Pressure Gate Seven", 145, 10, PRESSURE_GATE, [(ANSEL, "keeper"), (MARA, "petitioner"), (NESSA, "assistant")], "Ansel verifies the route tokens and opens Gate Seven between flood surges.", causes=[E_ESCAPE], story_points=[SP_GATE], effects=[effect("ansel-gate", ANSEL, "location", "set", {"entity": PRESSURE_GATE}), effect("mara-gate", MARA, "location", "set", {"entity": PRESSURE_GATE}), effect("nessa-gate", NESSA, "location", "set", {"entity": PRESSURE_GATE})], tags=["gate", "escape"]))
    add(event(E_LETTER, "Mara Opens the Heron Letter", 148, 0, PRESSURE_GATE, [(MARA, "reader"), (NESSA, "witness"), (ANSEL, "witness")], "Mara breaks the erased-wing seal and reads Ilyra's route and warning aloud.", causes=[E_GATE], story_points=[SP_BREAK, SP_FIND, SP_OPEN], effects=[effect("letter-opened", LETTER, "condition", "set", "opened and damp")], tags=["letter", "revelation"]))

    # Listening office.
    clerk_turns = [
        turn("clerk-ledger", 0, 152, 20, MIRA, "The office was ordered closed, but the seventh diaphragm has fresh graphite.", delivery="technical"),
        turn("clerk-ledger", 1, 152, 30, HALVER, "I copied requests. I did not seat diaphragms.", delivery="defensive"),
        turn("clerk-ledger", 2, 153, 0, MARA, "Who signed request N-seven-B?", delivery="precise"),
        turn("clerk-ledger", 3, 153, 10, HALVER, "Captain Rusk's cipher. Councillor Vey's office supplied the date.", delivery="quiet"),
        turn("clerk-ledger", 4, 153, 20, YSABET, "Those are different assertions. Which did you personally copy?", delivery="notarial"),
        turn("clerk-ledger", 5, 153, 30, HALVER, "Rusk's cipher. Vey's date arrived already written.", delivery="careful"),
        turn("clerk-ledger", 6, 154, 0, NESSA, "And the altered date in the margin?", delivery="blunt"),
        turn("clerk-ledger", 7, 154, 10, HALVER, "I changed it after Rusk threatened my sister's ferry license.", delivery="breaking"),
    ]
    add(conversation(CONV_CLERK, "The Clerk's Ledger", "closed", 152, (154, 20), SC_OFFICE, LISTENING_OFFICE,
        [{"character": MARA, "role": "questioner", "from": point(152), "to": point(154, 20)}, {"character": NESSA, "role": "document-reader", "from": point(152), "to": point(154, 20)}, {"character": YSABET, "role": "notary", "from": point(152), "to": point(154, 20)}, {"character": MIRA, "role": "engineer", "from": point(152), "to": point(154, 20)}, {"character": HALVER, "role": "clerk", "from": point(152, 20), "to": point(154, 20)}],
        clerk_turns,
        [recollection("clerk-ledger", 0, MARA, 155, 0, "Halver copied Rusk's cipher, received Caldrin's date already written, and later changed one margin date under threat.", "Rusk controlled the requisition mechanism; Caldrin's office shaped its chronology.", "clarity without exoneration", 0.93, exact=[clerk_turns[3]["id"], clerk_turns[7]["id"]]), recollection("clerk-ledger", 1, HALVER, 155, 0, "The archivist separated what he copied from what he inferred, while the notary forced him to identify the altered date.", "They may believe him if the ledger survives.", "shame and relief", 0.76, exact=[clerk_turns[4]["id"], clerk_turns[7]["id"]]), recollection("clerk-ledger", 2, YSABET, 155, 0, "Halver distinguishes Rusk's cipher from Vey's supplied date and admits one coerced alteration.", "His testimony is usable only with the original listening ledger and cipher ring.", "professional focus", 0.97, exact=[clerk_turns[5]["id"], clerk_turns[7]["id"]])],
        "Halver's testimony separates Rusk's requisition cipher from Caldrin's supplied dates." , ["testimony", "listening-office"]))
    office_turns = [
        turn("dead-office", 0, 155, 20, MIRA, "This line marks every time the seventh branch was opened.", delivery="technical"),
        turn("dead-office", 1, 155, 30, NESSA, "There are entries after the closure order.", delivery="reading"),
        turn("dead-office", 2, 156, 0, MARA, "And one during Ilyra's night inventory.", delivery="quiet"),
        turn("dead-office", 3, 156, 10, YSABET, "Copy the line spacing. Do not remove the ledger until I number the pages.", delivery="formal"),
        turn("dead-office", 4, 157, 0, MIRA, "The receiver at Vey's office was active. Rusk's ring only requested the session.", delivery="certain"),
        turn("dead-office", 5, 157, 10, MARA, "So one man ordered the listening and another office heard it.", delivery="precise"),
    ]
    add(conversation(CONV_OFFICE, "The Dead Bell Office", "closed", 155, (157, 20), SC_OFFICE, LISTENING_OFFICE,
        [{"character": MARA, "role": "investigator", "from": point(155), "to": point(157, 20)}, {"character": NESSA, "role": "copyist", "from": point(155), "to": point(157, 20)}, {"character": YSABET, "role": "notary", "from": point(155), "to": point(157, 20)}, {"character": MIRA, "role": "engineer", "from": point(155), "to": point(157, 20)}],
        office_turns,
        [recollection("dead-office", 0, MARA, 158, 0, "The ledger proves the closed listening office heard the seventh branch during Ilyra's inventory; Rusk requested it and Vey's office received it.", "Rusk and Caldrin performed distinct parts of the same surveillance chain.", "grim confirmation", 0.95, exact=[office_turns[4]["id"], office_turns[5]["id"]]), recollection("dead-office", 1, MIRA, 158, 0, "Mara understood that request and reception are mechanically distinct.", "The evidence may finally clear Varo of maintaining the illegal listening system.", "vindication", 0.9, exact=[office_turns[5]["id"]])],
        "Mira demonstrates how Rusk's requisition and Caldrin's receiver formed one listening session." , ["mechanism", "surveillance"]))
    add(scene(SC_OFFICE, "The Dead Bell Office", "closed", 152, (158, 0), 158, LISTENING_OFFICE,
        [{"character": MARA, "role": "viewpoint", "point_of_view": True, "from": point(152), "to": point(158)}, {"character": NESSA, "role": "copyist", "point_of_view": False, "from": point(152), "to": point(158)}, {"character": YSABET, "role": "notary", "point_of_view": False, "from": point(152), "to": point(158)}, {"character": MIRA, "role": "engineer", "point_of_view": False, "from": point(152), "to": point(158)}, {"character": HALVER, "role": "clerk", "point_of_view": False, "from": point(152, 20), "to": point(154, 20)}],
        [LISTENING_LEDGER, WITNESS_COPY, REQUISITION_RING], [ENV_OFFICE], [SP_OFFICE, SP_HALVER, SP_CHAIN], [CONV_CLERK, CONV_OFFICE],
        [observation("office-fresh-graphite", 152, 20, [MIRA], "Fresh graphite lies under the seventh diaphragm despite the closure order.", 1.3), observation("office-ilyra-line", 156, 0, [MARA, NESSA, YSABET], "The listening ledger records a session during Ilyra's final night inventory.", 1.4)],
        ["Halver's admission is coerced-testimony evidence and requires documentary corroboration.", "Mira can establish the mechanism but not who listened in person."],
        "The group enters the supposedly dead listening office, obtains Halver's testimony, and proves the surveillance chain remained active."))
    add(event(E_OFFICE, "The Council Listening Office Is Entered", 152, 10, LISTENING_OFFICE, [(YSABET, "notary"), (MARA, "archive-witness"), (NESSA, "copyist"), (MIRA, "engineer")], "Ysabet uses the defective writ's evidence clause to open the decommissioned listening office.", causes=[E_LETTER], story_points=[SP_OFFICE], effects=[effect("mara-office", MARA, "location", "set", {"entity": LISTENING_OFFICE}), effect("nessa-office", NESSA, "location", "set", {"entity": LISTENING_OFFICE}), effect("ysabet-office", YSABET, "location", "set", {"entity": LISTENING_OFFICE}), effect("mira-office", MIRA, "location", "set", {"entity": LISTENING_OFFICE})], tags=["office", "investigation"]))
    add(event(E_HALVER, "Halver Gives Qualified Testimony", 154, 10, LISTENING_OFFICE, [(HALVER, "witness"), (YSABET, "notary"), (MARA, "questioner"), (NESSA, "copyist")], "Halver distinguishes Rusk's cipher from Caldrin's supplied date and admits one coerced alteration.", causes=[E_OFFICE], story_points=[SP_HALVER], effects=[effect("halver-office", HALVER, "location", "set", {"entity": LISTENING_OFFICE})], tags=["testimony"]))
    add(event(E_LEDGER, "The Listening Ledger Is Numbered as Evidence", 156, 30, LISTENING_OFFICE, [(YSABET, "notary"), (MARA, "witness"), (NESSA, "copyist"), (MIRA, "engineer")], "Ysabet numbers the ledger pages while Nessa copies the activation line from Ilyra's last inventory.", causes=[E_HALVER], story_points=[SP_OFFICE, SP_CHAIN], effects=[effect("ledger-to-ysabet", LISTENING_LEDGER, "holder", "set", {"entity": YSABET}), effect("ledger-condition", LISTENING_LEDGER, "condition", "set", "numbered and sealed")], tags=["evidence", "listening-ledger"]))
    add(event(E_RING, "Rusk's Ring Matches the Requisition Cipher", 157, 20, LISTENING_OFFICE, [(MIRA, "examiner"), (YSABET, "notary"), (RUSK, "absent-subject")], "Mira compares the ledger stamp pattern with an earlier impression of Rusk's requisition ring and identifies the same broken tooth.", causes=[E_LEDGER], story_points=[SP_CHAIN], effects=[], tags=["cipher", "corroboration"]))

    # Hearing and deposition.
    hearing_turns = [
        turn("ledger-hearing", 0, 160, 10, YSABET, "This hearing concerns custody, not guilt. Every answer will identify its source.", delivery="formal"),
        turn("ledger-hearing", 1, 160, 20, MARA, "The flood register came from Gallery N and entered the notarial tube before Captain Rusk reached the door.", delivery="precise"),
        turn("ledger-hearing", 2, 161, 0, CALDRIN, "A hidden register found by an accused archivist is not evidence against the Council.", delivery="warm"),
        turn("ledger-hearing", 3, 161, 10, NESSA, "The witness copy predates the seal, and the ink breaks match the original.", delivery="firm"),
        turn("ledger-hearing", 4, 162, 0, HALVER, "I copied Captain Rusk's cipher. Councillor Vey's date was already on the request.", delivery="shaking"),
        turn("ledger-hearing", 5, 162, 20, RUSK, "I requisitioned listening sessions under lawful fire-security orders.", delivery="controlled"),
        turn("ledger-hearing", 6, 163, 0, CALDRIN, "No order from my office authorized surveillance of archivists.", delivery="measured"),
        turn("ledger-hearing", 7, 163, 20, MIRA, "Your receiver was active. Rusk's ring requested the branch. Those are separate mechanical facts.", delivery="technical"),
        turn("ledger-hearing", 8, 164, 0, YSABET, "The chain is admissible. The allocation of responsibility remains contested.", delivery="ruling"),
    ]
    add(conversation(CONV_HEARING, "The Ledger Hearing", "closed", 160, (164, 10), SC_HEARING, COUNCIL,
        [{"character": MARA, "role": "witness", "from": point(160), "to": point(164, 10)}, {"character": NESSA, "role": "copyist", "from": point(160), "to": point(164, 10)}, {"character": YSABET, "role": "notary", "from": point(160), "to": point(164, 10)}, {"character": CALDRIN, "role": "councillor", "from": point(160), "to": point(164, 10)}, {"character": RUSK, "role": "watch-captain", "from": point(160), "to": point(164, 10)}, {"character": HALVER, "role": "clerk", "from": point(162), "to": point(164, 10)}, {"character": MIRA, "role": "engineer", "from": point(162, 20), "to": point(164, 10)}],
        hearing_turns,
        [recollection("ledger-hearing", 0, MARA, 165, 0, "Ysabet admitted the evidence chain but left responsibility contested between Rusk and Caldrin.", "The hearing preserved the documents while warning both men how much remains unproven.", "frustrated success", 0.92, exact=[hearing_turns[0]["id"], hearing_turns[8]["id"]]), recollection("ledger-hearing", 1, CALDRIN, 165, 0, "The notary admitted a chain built by hostile archivists but did not assign responsibility.", "He can still isolate Rusk if the undercroft is never reached.", "strategic relief", 0.78, exact=[hearing_turns[8]["id"]]), recollection("ledger-hearing", 2, RUSK, 165, 0, "Mira separated his requests from Vey's receiver, and Halver named the supplied dates.", "Caldrin intends to sacrifice him for following broad security orders.", "contained fury", 0.81, exact=[hearing_turns[4]["id"], hearing_turns[7]["id"]]), recollection("ledger-hearing", 3, NESSA, 165, 0, "The copy and original survived public challenge, but nobody answered why Ilyra was targeted.", "The truth is below the vault, not in Ember Hall.", "impatient resolve", 0.9, exact=[hearing_turns[3]["id"], hearing_turns[8]["id"]])],
        "A public hearing preserves the evidence chain while leaving Rusk and Caldrin's respective responsibility unresolved.", ["hearing", "evidence", "testimony"]))
    deposition_turns = [
        turn("rusk-deposition", 0, 165, 10, YSABET, "Did you order session N-seven-B?", delivery="formal"),
        turn("rusk-deposition", 1, 165, 20, RUSK, "I ordered a fire-security listen on the north branch.", delivery="controlled"),
        turn("rusk-deposition", 2, 165, 30, MARA, "Did you know the branch terminated in the catalog rail?", delivery="precise"),
        turn("rusk-deposition", 3, 165, 40, RUSK, "Not until after Ilyra disappeared.", delivery="flat"),
        turn("rusk-deposition", 4, 166, 0, NESSA, "Then why does your cipher appear on removals through Flood Gallery N?", delivery="sharp"),
        turn("rusk-deposition", 5, 166, 10, RUSK, "Because Caldrin's office told the watch those crates were fire hazards.", delivery="angry"),
    ]
    add(conversation(CONV_DEPOSITION, "Rusk's Deposition", "closed", 165, (166, 20), SC_HEARING, EVIDENCE_CLOISTER,
        [{"character": RUSK, "role": "deponent", "from": point(165), "to": point(166, 20)}, {"character": YSABET, "role": "notary", "from": point(165), "to": point(166, 20)}, {"character": MARA, "role": "questioner", "from": point(165), "to": point(166, 20)}, {"character": NESSA, "role": "witness", "from": point(165), "to": point(166, 20)}],
        deposition_turns,
        [recollection("rusk-deposition", 0, MARA, 167, 0, "Rusk admits ordering the listening session but says Caldrin's office classified the removed crates as fire hazards.", "He may be both perpetrator and instrument.", "unsettled", 0.84, exact=[deposition_turns[1]["id"], deposition_turns[5]["id"]]), recollection("rusk-deposition", 1, RUSK, 167, 0, "He admitted the listen and named Caldrin's fire-hazard classification.", "The archivists will use his candor to reach the undercroft and leave him exposed.", "resentful calculation", 0.88, exact=[deposition_turns[5]["id"]])],
        "Rusk admits the listening order and redirects the origin of the removals toward Caldrin's office.", ["rusk", "deposition"]))
    add(scene(SC_HEARING, "The Ledger Hearing", "closed", 160, (166, 20), (166, 30), COUNCIL,
        [{"character": MARA, "role": "witness", "point_of_view": True, "from": point(160), "to": point(166, 30)}, {"character": NESSA, "role": "copyist", "point_of_view": False, "from": point(160), "to": point(166, 30)}, {"character": YSABET, "role": "notary", "point_of_view": False, "from": point(160), "to": point(166, 30)}, {"character": CALDRIN, "role": "councillor", "point_of_view": False, "from": point(160), "to": point(164, 10)}, {"character": RUSK, "role": "watch-captain", "point_of_view": False, "from": point(160), "to": point(166, 30)}, {"character": HALVER, "role": "clerk", "point_of_view": False, "from": point(162), "to": point(164, 10)}, {"character": MIRA, "role": "engineer", "point_of_view": False, "from": point(162, 20), "to": point(164, 10)}],
        [EVIDENCE_TUBE, REGISTER, WITNESS_COPY, LISTENING_LEDGER, REQUISITION_RING], [ENV_HEARING], [SP_HEARING, SP_CHAIN, SP_FIND], [CONV_HEARING, CONV_DEPOSITION],
        [observation("hearing-caldrin-signet", 161, 0, [MARA], "Caldrin keeps his intact heron signet beneath his folded hand whenever the erased-wing letter is mentioned.", 1.2), observation("hearing-rusk-ring", 162, 20, [MIRA, YSABET], "The missing tooth on Rusk's ring repeats the defect in the listening-ledger stamp.", 1.4)],
        ["The hearing proves the chain and mechanism, not final intent.", "Mara has not yet revealed the full contents of Ilyra's letter publicly."],
        "The evidence survives public challenge. Rusk admits the listening order; Caldrin denies authorizing surveillance; responsibility remains divided."))
    add(event(E_HEARING, "The Ledger Hearing Convenes", 160, 0, COUNCIL, [(YSABET, "notary"), (MARA, "witness"), (NESSA, "copyist"), (CALDRIN, "councillor"), (RUSK, "watch-captain")], "Ysabet opens a public custody hearing over the flood register and listening ledger.", causes=[E_LEDGER], story_points=[SP_HEARING, SP_CHAIN], effects=[effect("mara-council", MARA, "location", "set", {"entity": COUNCIL}), effect("nessa-council", NESSA, "location", "set", {"entity": COUNCIL}), effect("ysabet-council", YSABET, "location", "set", {"entity": COUNCIL})], tags=["hearing"]))
    add(event(E_CALDRIN, "Caldrin Disavows Rusk's Surveillance", 163, 0, COUNCIL, [(CALDRIN, "councillor"), (RUSK, "watch-captain"), (YSABET, "notary")], "Caldrin denies authorizing surveillance and frames Rusk's listening sessions as unauthorized security zeal.", causes=[E_HEARING], story_points=[SP_HEARING], effects=[], tags=["testimony"]))
    add(event(E_RUSK, "Rusk Names Caldrin's Fire-Hazard Orders", 166, 10, EVIDENCE_CLOISTER, [(RUSK, "deponent"), (MARA, "questioner"), (YSABET, "notary")], "Rusk states that Caldrin's office classified the removed crates as fire hazards and supplied the dates.", causes=[E_CALDRIN], story_points=[SP_HEARING], effects=[], tags=["deposition"]))
    add(event(E_RULE, "Ysabet Rules the Evidence Chain Admissible", 164, 0, COUNCIL, [(YSABET, "notary"), (MARA, "custodian"), (NESSA, "copyist")], "Ysabet admits the flood register, witness copy, listening ledger, and testimony as one documented chain while reserving judgment on responsibility.", causes=[E_HEARING, E_RING], story_points=[SP_CHAIN, SP_HEARING], effects=[], tags=["ruling", "evidence"]))

    # Ilyra and the final active scene.
    ilyra_turns = [
        turn("ilyra-account", 0, 169, 20, ILYRA, "You opened the letter after finding the route. Good.", delivery="tired"),
        turn("ilyra-account", 1, 169, 30, MARA, "You could have told me the ledger was a list of living people.", delivery="formal"),
        turn("ilyra-account", 2, 170, 0, ILYRA, "If Caldrin heard the list existed, he would use the Archive to find every name.", delivery="quiet"),
        turn("ilyra-account", 3, 170, 10, NESSA, "Rusk already moved crates through the flood gallery.", delivery="sharp"),
        turn("ilyra-account", 4, 170, 20, ILYRA, "Rusk moved what Caldrin labelled. He knew the method, not the history.", delivery="precise"),
        turn("ilyra-account", 5, 171, 0, YSABET, "That distinction may reduce guilt. It does not restore custody.", delivery="notarial"),
        turn("ilyra-account", 6, 171, 10, MARA, "What do you want done with the descendant ledger?", delivery="direct"),
        turn("ilyra-account", 7, 171, 20, ILYRA, "Destroy the addresses. Preserve the evacuation proof. Never keep both in one record again.", delivery="decisive"),
        turn("ilyra-account", 8, 172, 0, NESSA, "That means rewriting the Archive.", delivery="uneasy"),
        turn("ilyra-account", 9, 172, 10, ILYRA, "It means admitting that one perfect record can be a weapon.", delivery="flat"),
    ]
    add(conversation(CONV_ILYRA, "The Chief Archivist's Account", "closed", 169, (172, 20), SC_UNDER, UNDERCROFT,
        [{"character": ILYRA, "role": "chief-archivist", "from": point(169), "to": point(172, 20)}, {"character": MARA, "role": "successor", "from": point(169), "to": point(172, 20)}, {"character": NESSA, "role": "assistant", "from": point(169), "to": point(172, 20)}, {"character": YSABET, "role": "notary", "from": point(169), "to": point(172, 20)}],
        ilyra_turns,
        [recollection("ilyra-account", 0, MARA, 173, 0, "Ilyra hid the descendant ledger because its addresses could be used against living families and wants proof separated from identities.", "Her mentor is asking Mara to preserve history by destroying part of the record.", "relief turning into moral anger", 0.96, exact=[ilyra_turns[7]["id"], ilyra_turns[9]["id"]]), recollection("ilyra-account", 1, NESSA, 173, 0, "Ilyra says Rusk moved what Caldrin labelled and wants the addresses destroyed while proof survives.", "The Archive's completeness is part of the danger.", "intellectual shock", 0.92, exact=[ilyra_turns[4]["id"], ilyra_turns[8]["id"]]), recollection("ilyra-account", 2, YSABET, 173, 0, "Ilyra proposes separating identity-bearing addresses from evidentiary evacuation records.", "The proposal can be lawful only if the transformation itself has provenance and independent witnesses.", "professional fascination", 0.95, exact=[ilyra_turns[7]["id"]]), recollection("ilyra-account", 3, ILYRA, 173, 0, "Mara reached the undercroft with an admissible chain and immediately challenged the cost of concealment.", "Mara is ready to succeed her because she refuses the easy form of Ilyra's answer.", "exhausted pride", 0.84, exact=[ilyra_turns[1]["id"], ilyra_turns[6]["id"]])],
        "Ilyra explains why the descendant ledger cannot remain both complete and safely centralized.", ["ilyra", "ledger", "ethics"]))
    records_turns = [
        turn("choice-records", 0, 176, 10, MARA, "We will not destroy names without recording what was removed and why.", delivery="formal"),
        turn("choice-records", 1, 176, 20, ILYRA, "A redaction ledger is another map to the names.", delivery="tired"),
        turn("choice-records", 2, 176, 30, YSABET, "Not if custody is divided and reconstruction requires independent warrants.", delivery="precise"),
        turn("choice-records", 3, 177, 0, NESSA, "Then make three partial records: proof, route, and identity. No one office keeps two.", delivery="rapid"),
        turn("choice-records", 4, 177, 10, MARA, "And the original?", delivery="quiet"),
        turn("choice-records", 5, 177, 20, ILYRA, "The original is why we are all underground.", delivery="flat"),
        turn("choice-records", 6, 178, 0, ANSEL, "The shutters are closing. Argue while walking.", delivery="from the gate"),
    ]
    add(conversation(CONV_RECORDS, "The Choice of Records", "active", 176, None, SC_RECORDS, SHUTTER_CHAMBER,
        [{"character": MARA, "role": "viewpoint", "from": point(176)}, {"character": ILYRA, "role": "chief-archivist", "from": point(176)}, {"character": NESSA, "role": "designer", "from": point(176)}, {"character": YSABET, "role": "notary", "from": point(176)}, {"character": ANSEL, "role": "gate-keeper", "from": point(178)}],
        records_turns,
        [recollection("choice-records", 0, MARA, 178, 10, "Nessa proposed splitting proof, route, and identity among independent custodians while Ilyra still wants the original destroyed.", "A distributed record may preserve history without recreating the weapon, but no custody design is settled.", "urgent concentration", 0.82, exact=[records_turns[2]["id"], records_turns[3]["id"]]), recollection("choice-records", 1, ILYRA, 178, 10, "Mara refuses unrecorded destruction; Nessa proposes three incomplete records.", "They may build a safer Archive than the one Ilyra protected, if they survive the alarm.", "fear mixed with hope", 0.7, exact=[records_turns[0]["id"], records_turns[3]["id"]])],
        "The active conversation turns archival design into the immediate dramatic conflict while the fire shutters close.", ["active", "records", "custody"]))
    add(scene(SC_UNDER, "Under the Vault", "closed", 169, (174, 0), 174, UNDERCROFT,
        [{"character": MARA, "role": "viewpoint", "point_of_view": True, "from": point(169), "to": point(174)}, {"character": NESSA, "role": "assistant", "point_of_view": False, "from": point(169), "to": point(174)}, {"character": YSABET, "role": "notary", "point_of_view": False, "from": point(169), "to": point(174)}, {"character": ILYRA, "role": "chief-archivist", "point_of_view": False, "from": point(169), "to": point(174)}],
        [UNDERCROFT_LEDGER, COURIER_CORD, LETTER], [ENV_UNDERCROFT], [SP_FIND, SP_LEDGER], [CONV_ILYRA],
        [observation("undercroft-ilyra", 169, 20, [MARA], "Ilyra is thinner and exhausted but standing without support when Mara first sees her.", 1.5), observation("undercroft-ledger", 171, 20, [MARA, NESSA, YSABET], "The red waxcloth ledger is indexed by living family rather than by event or accession.", 1.4)],
        ["Finding Ilyra resolves her physical disappearance but opens the unresolved custody problem.", "The descendant ledger's living addresses remain author-sensitive unless explicitly learned."],
        "Mara finds Ilyra alive beneath the vault and learns why the complete descendant ledger is both historical proof and a present weapon."))
    add(scene(SC_RECORDS, "The Choice of Records", "active", 176, (178, 10), None, SHUTTER_CHAMBER,
        [{"character": MARA, "role": "viewpoint", "point_of_view": True, "from": point(176)}, {"character": ILYRA, "role": "chief-archivist", "point_of_view": False, "from": point(176)}, {"character": NESSA, "role": "designer", "point_of_view": False, "from": point(176)}, {"character": YSABET, "role": "notary", "point_of_view": False, "from": point(176)}, {"character": ANSEL, "role": "gate-keeper", "point_of_view": False, "from": point(178)}],
        [UNDERCROFT_LEDGER, WITNESS_COPY, COURIER_CORD], [ENV_UNDERCROFT, ENV_ALARM], [SP_LEDGER, SP_ALARM], [CONV_RECORDS],
        [observation("records-shutters", 177, 30, [MARA, NESSA, ILYRA, YSABET], "The north fire shutter drops one tooth each time the bell strikes.", 1.4), observation("records-mara-ledger", 176, 10, [MARA], "Ilyra's hand tightens on the waxcloth whenever the word provenance is used.", 1.3), observation("records-ansel-count", 178, 0, [ANSEL], "Only two pressure cycles remain before the south passage floods.", 1.5)],
        ["No custody plan is canonical yet.", "The active choice concerns both the original ledger and the three proposed partial records.", "The fire alarm creates a real time limit but does not determine the moral outcome."],
        "Mara, Ilyra, Nessa, and Ysabet dispute whether to destroy, preserve, or distribute the descendant ledger as the old fire shutters close."))
    add(scene(SC_PURSUIT, "River Gate Pursuit", "planned", 182, (182, 0), 188, RIVER_GATE,
        [{"character": MARA, "role": "viewpoint", "point_of_view": True, "from": point(182), "to": point(188)}, {"character": SABLE, "role": "guide", "point_of_view": False, "from": point(182), "to": point(188)}, {"character": RUSK, "role": "pursuer", "point_of_view": False, "from": point(184), "to": point(188)}],
        [UNDERCROFT_LEDGER, COURIER_CORD], [], [SP_ALARM], [], [], ["This is a future branch, not current experience."], "A possible escape branch in which the records leave Cindervale by river."))
    add(scene(SC_RECKONING, "Ember Hall Reckoning", "planned", 190, (190, 0), 198, COUNCIL,
        [{"character": MARA, "role": "viewpoint", "point_of_view": True, "from": point(190), "to": point(198)}, {"character": ILYRA, "role": "chief-archivist", "point_of_view": False, "from": point(190), "to": point(198)}, {"character": YSABET, "role": "notary", "point_of_view": False, "from": point(190), "to": point(198)}, {"character": CALDRIN, "role": "councillor", "point_of_view": False, "from": point(190), "to": point(198)}, {"character": RUSK, "role": "watch-captain", "point_of_view": False, "from": point(190), "to": point(198)}],
        [UNDERCROFT_LEDGER, LISTENING_LEDGER, REGISTER], [], [SP_LEDGER], [], [], ["This is a future branch, not current experience."], "A possible final public adjudication after the records' custody has been chosen."))
    add(event(E_ROUTE, "The Letter Route Opens the Undercroft", 169, 10, UNDERCROFT, [(MARA, "reader"), (NESSA, "assistant"), (YSABET, "notary")], "Mara follows the letter's Gate Seven instruction and releases the false north shelf from below.", causes=[E_LETTER, E_RULE], story_points=[SP_FIND], effects=[effect("mara-undercroft", MARA, "location", "set", {"entity": UNDERCROFT}), effect("nessa-undercroft", NESSA, "location", "set", {"entity": UNDERCROFT}), effect("ysabet-undercroft", YSABET, "location", "set", {"entity": UNDERCROFT})], tags=["route", "undercroft"]))
    add(event(E_FIND, "Mara Finds Ilyra Alive", 169, 20, UNDERCROFT, [(MARA, "finder"), (ILYRA, "missing-chief"), (NESSA, "witness"), (YSABET, "witness")], "Mara finds Ilyra alive in the vault undercroft with the descendant ledger and courier cord.", causes=[E_ROUTE], story_points=[SP_FIND], effects=[effect("ilyra-undercroft", ILYRA, "location", "set", {"entity": UNDERCROFT}), effect("ilyra-condition", ILYRA, "condition", "set", "exhausted but alive")], tags=["ilyra", "reunion"]))
    add(event(E_ACCOUNT, "Ilyra Explains the Descendant Ledger", 171, 20, UNDERCROFT, [(ILYRA, "chief-archivist"), (MARA, "successor"), (NESSA, "assistant"), (YSABET, "notary")], "Ilyra explains that the ledger's complete addresses are historical evidence and a ready-made targeting list.", causes=[E_FIND], story_points=[SP_LEDGER], effects=[], tags=["ledger", "ethics"]))
    add(event(E_COPY_LEDGER, "Nessa Proposes Three Partial Records", 177, 0, SHUTTER_CHAMBER, [(NESSA, "designer"), (MARA, "archivist"), (ILYRA, "chief-archivist"), (YSABET, "notary")], "Nessa proposes separating proof, route, and identity so no single custodian can reconstruct the living list.", causes=[E_ACCOUNT], story_points=[SP_LEDGER], effects=[], tags=["records", "design"]))
    add(event(E_ALARM, "The Vault Fire Alarm Sounds", 177, 30, SHUTTER_CHAMBER, [(ANSEL, "keeper"), (MARA, "archivist"), (ILYRA, "chief-archivist")], "The old alarm begins closing the north shutters while flood pressure rises in the south passage.", causes=[E_COPY_LEDGER], story_points=[SP_ALARM], effects=[effect("ansel-shutter", ANSEL, "location", "set", {"entity": SHUTTER_CHAMBER})], tags=["alarm", "deadline"]))
    add(event(E_CHOICE, "The Records Dispute Begins", 178, 0, SHUTTER_CHAMBER, [(MARA, "archivist"), (ILYRA, "chief-archivist"), (NESSA, "designer"), (YSABET, "notary"), (ANSEL, "keeper")], "The group argues over destruction, redaction, and distributed custody while the shutters close.", causes=[E_ACCOUNT, E_ALARM], story_points=[SP_LEDGER, SP_ALARM], effects=[], tags=["choice", "active"]))

    # Story points and updates.
    add(story_point(SP_CUSTODY, "Choose the Custodian of Flood Register N-7B", 94, {"event": {"event": E_SEAL}}, "Choose whether the original register remains with Mara or enters a notarial custody chain.", transitions=[spt("custody-active", 139, 30, "active", by_title["Ysabet Warns That Rusk Is Approaching"].id, "Ysabet can carry one item."), spt("custody-resolved", 140, 30, "resolved", E_SEAL, "Mara assigns the original register to Ysabet.")], outcomes=[E_SEAL]))
    add(story_point(SP_GATE, "Pass Pressure Gate Seven", 90, {"event": {"event": E_ESCAPE}}, "Reach and safely pass Gate Seven before Rusk finds the lower route.", transitions=[spt("gate-active", 142, 40, "active", E_ESCAPE, "The flood stair becomes the escape route."), spt("gate-resolved", 145, 10, "resolved", E_GATE, "Ansel opens the gate.")], outcomes=[E_GATE]))
    add(story_point(SP_OPEN, "Open the Letter Under Witness", 88, {"event": {"event": E_GATE}}, "Break the heron seal with an independent witness and preserve its contents as provenance.", transitions=[spt("open-active", 145, 10, "active", E_GATE, "The group reaches a defensible place to open the letter."), spt("open-resolved", 148, 0, "resolved", E_LETTER, "Mara opens and reads the letter.")], outcomes=[E_LETTER]))
    add(story_point(SP_OFFICE, "Expose the Listening Office", 92, {"event": {"event": E_LETTER}}, "Prove that the supposedly decommissioned office continued listening to the Archive.", transitions=[spt("office-active", 150, 0, "active", E_LETTER, "Ilyra's warning makes the listening route actionable."), spt("office-resolved", 156, 30, "resolved", E_LEDGER, "The active ledger is numbered as evidence.")], outcomes=[E_LEDGER]))
    add(story_point(SP_HALVER, "Test Halver's Testimony", 80, {"event": {"event": E_HALVER}}, "Separate what Halver copied, inferred, and altered under threat.", transitions=[spt("halver-active", 152, 20, "active", E_OFFICE, "Halver is found in the office."), spt("halver-resolved", 154, 10, "resolved", E_HALVER, "His qualified admission is recorded.")], outcomes=[E_HALVER]))
    add(story_point(SP_HEARING, "Conduct the Ledger Hearing", 86, {"event": {"event": E_LEDGER}}, "Preserve the evidence chain through public challenge without overstating what it proves.", transitions=[spt("hearing-active", 160, 0, "active", E_HEARING, "The public hearing begins."), spt("hearing-resolved", 164, 0, "resolved", E_RULE, "Ysabet admits the chain.")], outcomes=[E_RULE]))
    add(story_point(SP_CHAIN, "Establish the Surveillance Chain", 93, {"event": {"event": E_RING}}, "Connect Rusk's requisition, Caldrin's receiver, Halver's dates, and the live listening ledger.", transitions=[spt("chain-active", 154, 10, "active", E_HALVER, "The testimony identifies distinct actors."), spt("chain-resolved", 164, 0, "resolved", E_RULE, "The chain is admitted while responsibility stays contested.")], outcomes=[E_RULE]))
    add(story_point(SP_LEDGER, "Decide the Fate of the Descendant Ledger", 100, {"event": {"event": E_ACCOUNT}}, "Choose destruction, redaction, distributed custody, or another design for a record that is both proof and weapon.", transitions=[spt("ledger-active", 171, 20, "active", E_ACCOUNT, "Ilyra explains the ledger's danger.")], outcomes=[]))
    add(story_point(SP_ALARM, "Survive the Vault Alarm", 100, {"event": {"event": E_ALARM}}, "Reach a safe exit before the fire shutters and flood pressure close both routes.", transitions=[spt("alarm-active", 177, 30, "active", E_ALARM, "The shutters begin closing.")], outcomes=[]))

    # Update existing points.
    for title, transition in [
        ("Break the Seal", spt("break-resolved-v04", 148, 0, "resolved", E_LETTER, "Mara opens the letter under witness.")),
        ("Find Ilyra Sorn", spt("find-resolved-v04", 169, 20, "resolved", E_FIND, "Mara finds Ilyra alive beneath the vault.")),
        ("Decide Whether to Trust Ysabet", spt("trust-resolved-v04", 140, 30, "resolved", E_SEAL, "Mara entrusts Ysabet with the original register.")),
        ("Escape Before Rusk Opens the North Door", spt("escape-resolved-v04", 142, 40, "resolved", E_ESCAPE, "Mara and Nessa take the lower passage before the door opens.")),
    ]:
        record = world.find(title, "story-point")
        fm = deepcopy(record.frontmatter)
        transitions = fm.setdefault("lifecycle", {}).setdefault("transitions", [])
        if transition["id"] not in {item.get("id") for item in transitions if isinstance(item, dict)}:
            transitions.append(transition)
        fm["outcome_events"] = list(dict.fromkeys([*(fm.get("outcome_events") or []), transition["causing_event"]]))
        add((fm, record.body + f"\nThe second act advances this thread at tick {transition['time']['tick']}.\n"))

    # Knowledge records.
    knowledge_specs = [
        ("mara-letter-contents-v04", MARA, "letter.ilyra.alive", "Ilyra's letter says she is alive beneath the Restricted Vault.", 148, 5, "accepted", .99, "read", E_LETTER, LETTER),
        ("nessa-letter-route-v04", NESSA, "letter.route.gate-seven", "The heron letter says Gate Seven opens the north shelf route to the undercroft.", 148, 25, "accepted", .97, "heard", E_LETTER, LETTER),
        ("ansel-letter-warning-v04", ANSEL, "letter.rejects.intact-heron", "Ilyra's letter warns that an intact heron seal must not be trusted.", 148, 35, "accepted", .98, "heard", E_LETTER, LETTER),
        ("mara-halver-v04", MARA, "halver.distinguishes.cipher.date", "Halver says he copied Rusk's cipher while Caldrin's office supplied the date.", 154, 15, "accepted", .93, "heard", E_HALVER, HALVER),
        ("ysabet-halver-alteration-v04", YSABET, "halver.altered.margin.date", "Halver admits changing one margin date after Rusk threatened his sister's ferry license.", 154, 15, "accepted", .99, "heard", E_HALVER, HALVER),
        ("mira-office-active-v04", MIRA, "listening.office.remained.active", "The Council listening office remained active after its closure order.", 155, 25, "accepted", .99, "observed", E_LEDGER, LISTENING_LEDGER),
        ("mara-chain-v04", MARA, "surveillance.chain.split", "Rusk's ring requested the seventh branch while Caldrin's office received it.", 157, 15, "accepted", .96, "inferred", E_RING, LISTENING_LEDGER),
        ("rusk-caldrin-fire-orders-v04", RUSK, "caldrin.classified.crates.fire-hazards", "Caldrin's office classified the removed archive crates as fire hazards.", 166, 15, "accepted", .9, "remembered", E_RUSK, CALDRIN),
        ("mara-rusk-instrument-v04", MARA, "rusk.may-be.instrument", "Rusk may have enforced Caldrin's classifications without knowing the descendant history.", 166, 20, "suspected", .68, "inferred", E_RUSK, RUSK),
        ("caldrin-chain-admitted-v04", CALDRIN, "evidence.chain.admitted", "Ysabet admitted the flood register and listening ledger as one evidence chain.", 164, 5, "accepted", .99, "heard", E_RULE, YSABET),
        ("mara-ilyra-ledger-danger-v04", MARA, "descendant.ledger.dual-use", "The complete descendant ledger proves illegal evacuations and exposes living families.", 171, 25, "accepted", .98, "told", E_ACCOUNT, UNDERCROFT_LEDGER),
        ("nessa-three-records-v04", NESSA, "distributed.records.proposal", "Proof, route, and identity can be split among independent custodians.", 177, 5, "accepted", .94, "inferred", E_COPY_LEDGER, UNDERCROFT_LEDGER),
        ("ysabet-distributed-custody-v04", YSABET, "distributed.custody.may-be-lawful", "A distributed record can be lawful if reconstruction requires independent warrants.", 176, 35, "suspected", .82, "inferred", E_CHOICE, UNDERCROFT_LEDGER),
        ("ilyra-mara-successor-v04", ILYRA, "mara.ready.successor", "Mara is ready to succeed Ilyra because she challenges easy concealment.", 173, 5, "suspected", .84, "inferred", E_ACCOUNT, MARA),
        ("ansel-alarm-cycles-v04", ANSEL, "vault.alarm.two-cycles", "Only two pressure cycles remain before the south passage floods.", 178, 5, "accepted", .99, "observed", E_ALARM, PRESSURE_GATE),
    ]
    for name, knower, claim_key, statement, tick_value, order_value, state, confidence, acquisition, cause, source in knowledge_specs:
        add(knowledge(ident("knowledge", name), statement, knower, claim_key, statement, tick_value, order_value, state, confidence, acquisition, cause=cause, source=source, truth="unknown"))

    # Relationship pairs.
    relationship_pairs = [
        ("mara-ansel", MARA, ANSEL, "hydraulic-ally", [(0, 0, 0.0, 0.0, 0.0, 0.0, ["unknown"], "No established relationship.", None), (149, 10, .48, .08, .08, .22, ["trusted-route-keeper", "ilyra-contact"], "Ansel opens the gate and witnesses the letter without claiming authority over it.", E_LETTER)]),
        ("ansel-mara", ANSEL, MARA, "tested-successor", [(0, 0, 0.0, 0.0, 0.0, 0.0, ["unknown"], "No established relationship.", None), (149, 10, .55, .12, .05, .18, ["skeptical", "capable"], "Mara responds to the route test with caution rather than entitlement.", E_LETTER)]),
        ("mara-halver", MARA, HALVER, "witness-questioner", [(0, 0, 0.0, 0.0, 0.0, 0.0, ["unknown"], "No established relationship.", None), (154, 20, .1, -.05, .04, .0, ["useful-witness", "coerced-alterer"], "Halver's admission is useful but compromised.", E_HALVER)]),
        ("halver-mara", HALVER, MARA, "frightened-witness", [(0, 0, 0.0, 0.0, 0.0, 0.0, ["unknown"], "No established relationship.", None), (154, 20, .34, .02, .38, .12, ["precise-questioner", "possible-protector"], "Mara distinguishes his copied facts from his inferences.", E_HALVER)]),
        ("ysabet-rusk", YSABET, RUSK, "notary-deponent", [(0, 0, .05, -.05, .02, .0, ["watch-captain"], "Professional familiarity.", None), (166, 20, -.22, -.18, .12, .0, ["admitted-listener", "contested-agent"], "Rusk admits the listening order but redirects the classifications to Caldrin.", E_RUSK)]),
        ("rusk-ysabet", RUSK, YSABET, "deponent-notary", [(0, 0, .04, -.04, .03, .0, ["council-notary"], "Professional familiarity.", None), (166, 20, -.38, -.22, .3, .0, ["dangerous-record-maker", "procedural-adversary"], "Ysabet records his admissions without accepting his defense.", E_RUSK)]),
    ]
    for name, source, target, kind, specs in relationship_pairs:
        inverse_name = name.split("-", 1)
        reverse = f"{inverse_name[1]}-{inverse_name[0]}" if len(inverse_name) == 2 else f"inverse-{name}"
        rel_id = ident("relationship", name)
        inverse_id = ident("relationship", reverse)
        transitions = [rel_transition(f"{name}:{tick_value}:{order_value}", tick_value, order_value, trust, affinity, fear, obligation, facets, note, cause) for tick_value, order_value, trust, affinity, fear, obligation, facets, note, cause in specs]
        add(relationship(rel_id, f"{world.get(source).title if source in world.records else _name(source, by_title)}'s view of {_name(target, by_title)}", source, target, inverse_id, kind, transitions, "A directional relationship shaped by the second act."))

    # Append transitions to existing Mara/Ilyra records instead of relying only on duplicate semantic pair records.
    for title, transition in [
        ("Mara's view of Ilyra", rel_transition("mara-ilyra-existing-v04", 174, 0, .62, .55, .12, .35, ["found-alive", "moral-disagreement"], "Relief is tempered by anger over the ledger plan.", E_ACCOUNT)),
        ("Ilyra's view of Mara", rel_transition("ilyra-mara-existing-v04", 174, 0, .9, .58, .18, .68, ["chosen-successor", "independent-judgment"], "Mara's challenge confirms her readiness to succeed Ilyra.", E_ACCOUNT)),
    ]:
        try:
            record = world.find(title, "relationship")
        except Exception:
            continue
        fm = deepcopy(record.frontmatter)
        if transition["id"] not in {item.get("id") for item in fm.get("transitions") or [] if isinstance(item, dict)}:
            fm.setdefault("transitions", []).append(transition)
        add((fm, record.body + "\nThe reunion adds a new directional transition at tick 174.\n"))

    # World synopsis.
    world_record = world.world_record
    fm = deepcopy(world_record.frontmatter)
    fm["context_policy"] = {"default_budget_characters": 8000, "default_max_items": 24, "conversation_turn_window": 6}
    fm["compilation_policy"] = {"retained_revisions": 5, "embedding_cache": True, "source_parse_cache": True, "atomic_publish": True}
    body = """# The Ash Archive

Cindervale's Ash Archive preserves fires, inheritances, civic debts, and evidence powerful people would rather route elsewhere. Chief Archivist Ilyra Sorn disappeared after tracing a gap in the accession catalog.

## Current canonical state

At tick 178:10, Mara, Ilyra, Nessa, Ysabet, and Sister Ansel are in the North Shutter Chamber. The public evidence chain survives, Ilyra has been found alive, and the complete descendant ledger remains a present danger. The group is arguing over destruction, redaction, and distributed custody while the old fire shutters close.

## Story extent

The authored example now runs from Ilyra's original audit assignment through the catalog gap, Oren's letter, the seventh drawer, Flood Gallery N, Pressure Gate Seven, the Council listening office, a public ledger hearing, Ilyra's return, and the active dispute over the Archive's future design.

## Fixture purpose

The expanded fixture exercises temporal state, hidden and public truth, verbatim dialogue, late participant admission, conflicting recollections, evidentiary provenance, asymmetric relationships, multi-stage story points, perspective-safe retrieval, hard context budgets, source-parse caching, embedding reuse, and compilation over a substantially longer narrative.
"""
    add((fm, body))

    # Helper resolves titles for newly generated characters.
    payload = {
        "protocol": "wedl-changeset/v1",
        "expectedHead": repository.head(),
        "idempotencyKey": "ash-archive-v04-second-act-v1",
        "summary": "Expand Ash Archive through the choice of records",
        "operations": operations,
    }
    return payload


def _name(entity_id: str, by_title: dict[str, Any]) -> str:
    for record in by_title.values():
        if record.id == entity_id:
            return record.title
    generated = {
        ident("character", "sister-ansel-marr"): "Sister Ansel Marr",
        ident("character", "halver-rook"): "Halver Rook",
        ident("character", "mira-sen"): "Mira Sen",
    }
    return generated.get(entity_id, entity_id)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True)
    parser.add_argument("--output")
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    repository = Repository(args.repo)
    payload = build(repository)
    if args.output:
        Path(args.output).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    plan = preview(repository, payload)
    print(json.dumps({key: value for key, value in plan.items() if key not in {"_changes", "diff"}}, ensure_ascii=False, indent=2))
    if not plan["valid"]:
        return 2
    if args.apply:
        receipt = apply(repository, payload, confirmation_token_value=plan["confirmationToken"])
        print(json.dumps(receipt, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
