from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from wedl import SOURCE_SCHEMA
from wedl.ids import id_from_seed

SEED = "the-frontiersmen:v1"


def eid(kind: str, slug: str) -> str:
    return id_from_seed(kind, f"{SEED}:{slug}")


def aid(kind: str, slug: str) -> str:
    return id_from_seed(kind, f"{SEED}:{slug}")


def t(tick: int, order: int = 0) -> dict[str, Any]:
    return {"timeline": "main", "tick": tick, "order": order}


def common(kind: str, entity_id: str, title: str, domain: str, *, status: str = "canonical", tags: Iterable[str] = (), aliases: Iterable[str] = ()) -> dict[str, Any]:
    return {
        "schema": SOURCE_SCHEMA,
        "kind": kind,
        "id": entity_id,
        "title": title,
        "domain": domain,
        "status": status,
        "tags": list(tags),
        "aliases": list(aliases),
    }


def upsert(frontmatter: dict[str, Any], body: str) -> dict[str, Any]:
    return {"type": "entity.upsert", "value": {"frontmatter": frontmatter, "bodyMarkdown": body.strip() + "\n"}}


def effect(slug: str, target: str, key: str, operation: str, value: Any | None = None) -> dict[str, Any]:
    result = {"id": aid("effect", slug), "target": target, "key": key, "operation": operation}
    if operation != "clear":
        result["value"] = value
    return result


def event_op(entity_id: str, title: str, tick: int, order: int, location: str, participants: list[tuple[str, str]], body: str, *, causes: Iterable[str] = (), story_points: Iterable[str] = (), effects: Iterable[dict[str, Any]] = (), tags: Iterable[str] = ()) -> dict[str, Any]:
    return {
        "type": "event.create",
        "id": entity_id,
        "title": title,
        "domain": "plot.frontiersmen",
        "status": "canonical",
        "tags": list(tags),
        "time": t(tick, order),
        "location": location,
        "participants": [{"character": character, "role": role} for character, role in participants],
        "causes": list(causes),
        "relatedStoryPoints": list(story_points),
        "effects": list(effects),
        "bodyMarkdown": f"# {title}\n\n{body.strip()}\n",
    }


def observation(slug: str, tick: int, order: int, audience: Iterable[str], text: str, *, salience: float = 1.0, until: dict[str, Any] | None = None) -> dict[str, Any]:
    result = {"id": aid("observation", slug), "at": t(tick, order), "audience": list(audience), "salience": salience, "text": text}
    if until:
        result["until"] = until
    return result


def scene_record(entity_id: str, title: str, *, status: str, start: dict[str, Any], current: dict[str, Any], end: dict[str, Any] | None, location: str, participants: list[dict[str, Any]], objects: Iterable[str] = (), environments: Iterable[str] = (), story_points: Iterable[str] = (), observations: Iterable[dict[str, Any]] = (), conversations: Iterable[str] = (), constraints: Iterable[str] = (), body: str, tags: Iterable[str] = ()) -> dict[str, Any]:
    fm = common("scene", entity_id, title, "scenes.frontiersmen", status=status, tags=tags)
    fm.update({
        "time": {"start": start, "current": current, "end": end},
        "location": location,
        "participants": participants,
        "objects": list(objects),
        "environments": list(environments),
        "story_points": list(story_points),
        "observations": list(observations),
        "author_constraints": list(constraints),
        "conversations": list(conversations),
    })
    return upsert(fm, f"# {title}\n\n{body}")


def participant(character: str, role: str, start: dict[str, Any], end: dict[str, Any] | None = None, *, pov: bool = False) -> dict[str, Any]:
    result = {"character": character, "role": role, "point_of_view": pov, "from": start}
    if end:
        result["to"] = end
    return result


def turn(slug: str, tick: int, order: int, speaker: str, text: str, *, delivery: str | None = None, audience: Iterable[str] = ("participants",), addressed_to: Iterable[str] = ()) -> dict[str, Any]:
    result = {"id": aid("conversation-turn", slug), "at": t(tick, order), "speaker": speaker, "text": text, "audience": list(audience)}
    if delivery:
        result["delivery"] = delivery
    if addressed_to:
        result["addressed_to"] = list(addressed_to)
    return result


def recollection_op(conversation_id: str, slug: str, character: str, tick: int, order: int, summary: str, interpretation: str, emotional: str, confidence: float, *, state: str = "remembered", exact_turns: Iterable[str] = (), remembered_quotes: Iterable[dict[str, Any]] = ()) -> dict[str, Any]:
    return {
        "type": "conversation.recollection.record",
        "conversationId": conversation_id,
        "recollection": {
            "character": character,
            "at": t(tick, order),
            "state": state,
            "summary": summary,
            "interpretation": interpretation,
            "emotional_impression": emotional,
            "confidence": confidence,
            "exact_turns": list(exact_turns),
            "remembered_quotes": list(remembered_quotes),
            "id": aid("conversation-recollection", f"{conversation_id}:{slug}"),
        },
    }


def conversation_op(entity_id: str, title: str, *, status: str, start: dict[str, Any], end: dict[str, Any] | None, scene: str | None, location: str | None, participants: list[dict[str, Any]], turns: list[dict[str, Any]], topics: Iterable[str], body: str) -> dict[str, Any]:
    return {
        "type": "conversation.create",
        "id": entity_id,
        "value": {
            "title": title,
            "domain": "conversations.frontiersmen",
            "status": status,
            "tags": list(topics),
            "aliases": [],
            "time": {"start": start, "end": end},
            "scene": scene,
            "location": location,
            "participants": participants,
            "topics": list(topics),
            "turns": turns,
            "recollections": [],
            "bodyMarkdown": f"# {title}\n\n{body.strip()}\n\nThe turn list is canonical verbatim provenance. Character recollections are stored separately and may disagree.\n",
        },
    }


def knowledge_record(entity_id: str, title: str, knower: str, claim_key: str, statement: str, tick: int, order: int, *, state: str = "accepted", confidence: float = 1.0, acquisition: str = "observed", truth: str = "unknown", subject: str | None = None, predicate: str | None = None, object_value: Any | None = None, causing_event: str | None = None, source_entity: str | None = None, note: str | None = None, tags: Iterable[str] = ()) -> dict[str, Any]:
    claim: dict[str, Any] = {"key": claim_key, "statement": statement, "author_truth_status": truth}
    if subject:
        claim["subject"] = subject
    if predicate:
        claim["predicate"] = predicate
    if object_value is not None:
        claim["object"] = object_value
    transition = {
        "id": aid("knowledge-transition", f"{entity_id}:0"),
        "time": t(tick, order),
        "state": state,
        "confidence": confidence,
        "acquisition": acquisition,
        "causing_event": causing_event,
        "source_entity": source_entity,
    }
    if note:
        transition["note"] = note
    fm = common("knowledge", entity_id, title, f"knowledge.{knower}", tags=tags)
    fm.update({"knower": knower, "claim": claim, "transitions": [transition]})
    return upsert(fm, f"# {title}\n\n{statement}\n\nThis is a subjective proposition held by the named knower, not a replacement for canonical event or conversation provenance.")


def relationship_transition(slug: str, tick: int, order: int, metrics: dict[str, float], facets: Iterable[str], note: str, *, causing_event: str | None = None, status: str = "active") -> dict[str, Any]:
    return {
        "id": aid("relationship-transition", slug),
        "time": t(tick, order),
        "relationship_status": status,
        "metrics": metrics,
        "facets": list(facets),
        "causing_event": causing_event,
        "note": note,
    }


def relationship_record(entity_id: str, title: str, source: str, target: str, kind: str, inverse: str, transitions: list[dict[str, Any]], body: str) -> dict[str, Any]:
    fm = common("relationship", entity_id, title, "relationships.frontiersmen", tags=[source, target])
    fm.update({"from": source, "to": target, "relationship_kind": kind, "inverse": inverse, "transitions": transitions})
    return upsert(fm, f"# {title}\n\n{body}")


def story_transition(slug: str, tick: int, order: int, state: str, causing_event: str | None, note: str) -> dict[str, Any]:
    return {"id": aid("story-point-transition", slug), "time": t(tick, order), "state": state, "causing_event": causing_event, "note": note}


def story_point_record(entity_id: str, title: str, *, initial_state: str = "dormant", transitions: Iterable[dict[str, Any]] = (), activation: str = "suggest", priority: int = 50, dependencies: dict[str, Any] | None = None, trigger: dict[str, Any] | None = None, outcomes: Iterable[str] = (), body: str, tags: Iterable[str] = ()) -> dict[str, Any]:
    fm = common("story-point", entity_id, title, "plot.frontiersmen", tags=tags)
    fm.update({
        "lifecycle": {"initial_state": initial_state, "transitions": list(transitions)},
        "activation_policy": activation,
        "priority": priority,
        "repeat_policy": "once",
        "dependencies": dependencies or {"all": []},
        "trigger": trigger or {"all": []},
        "on_activate": {"create_draft_scene": False},
        "outcome_events": list(outcomes),
    })
    return upsert(fm, f"# {title}\n\n{body}")


# Stable entity IDs ---------------------------------------------------------

WORLD = None  # filled from the initialized repository

# Main party
RHEA = eid("character", "rhea-marrow")
SYLVI = eid("character", "sylvi-ashdown")
GARRAN = eid("character", "brother-garran-holt")
VEYRA = eid("character", "veyra-kest")
PIP = eid("character", "pip-fenlock")

# Guild, road, and settlement cast
HALRIC = eid("character", "halric-doss")
NARA = eid("character", "nara-beech")
MAELA = eid("character", "maela-brigg")
EDRIK = eid("character", "sergeant-edrik-morn")
HESSA = eid("character", "reeve-hessa-noll")
TILLO = eid("character", "tillo-wick")
JORUND = eid("character", "jorund-bale")
KELLAN = eid("character", "kellan-voss")
SENNA = eid("character", "senna-reed")
ASHA = eid("character", "asha-pell")
BRAN = eid("character", "bran-tew")
NOLLY = eid("character", "nolly-dey")

# Root host
TREE_KING = eid("character", "tree-king-aldren-veyl")
MOTH = eid("character", "moth-lio-vane")
ROOTJAW = eid("character", "rootjaw")

PARTY = [RHEA, SYLVI, GARRAN, VEYRA, PIP]

# Locations
FRONTIER = eid("location", "the-frontier-peninsula")
KELDMOUTH = eid("location", "keldmouth")
KELD_DOCK = eid("location", "keldmouth-river-dock")
GUILDHALL = eid("location", "lantern-pike-guildhall")
AMBER_EXCHANGE = eid("location", "keldmouth-amber-exchange")
EAST_GATE = eid("location", "keldmouth-east-gate")
COAST_ROAD = eid("location", "east-coastal-road")
SALTMERE = eid("location", "saltmere-camp")
HARROWCROSS = eid("location", "harrowcross")
BARRACKS = eid("location", "harrowcross-barracks")
THREE_PINES = eid("location", "three-pines-inn")
SOUTH_FIELDS = eid("location", "harrowcross-south-fields")
EAST_ORCHARD = eid("location", "harrowcross-east-orchard")
MINE = eid("location", "saint-orra-amber-mine")
MINE_YARD = eid("location", "saint-orra-mine-yard")
UPPER_DRIFT = eid("location", "saint-orra-upper-drift")
AMBER_CHAMBER = eid("location", "deep-amber-chamber")
CAVEIN = eid("location", "cave-in-pocket")
LOWER_CAVERNS = eid("location", "lower-caverns")
BLACKWATER = eid("location", "blackwater-crossing")
SUNKEN_HALL = eid("location", "sunken-hall-ruins")
WRETCH_GALLERY = eid("location", "wretch-gallery")
HILLTOP = eid("location", "hilltop-sink")
ROOT_CAMP = eid("location", "root-crowned-camp")
ROOT_COURT = eid("location", "root-court")
BLOOD_RING = eid("location", "blood-ring")
PRISON_PENS = eid("location", "prison-pens")
PURSUIT_TRAIL = eid("location", "northwood-pursuit-trail")
DROWNED_WAYMARK = eid("location", "drowned-waymark")
WARDEN_ROAD = eid("location", "abandoned-warden-road")

# Objects
CONTRACT = eid("object", "lantern-pike-contract")
RHEA_BADGE = eid("object", "rhea-guild-badge")
SYLVI_BADGE = eid("object", "sylvi-guild-badge")
GARRAN_BADGE = eid("object", "garran-guild-badge")
VEYRA_BADGE = eid("object", "veyra-guild-badge")
PIP_BADGE = eid("object", "pip-guild-badge")
AMBER_WAGES = eid("object", "first-amber-wages")
RAW_AMBER = eid("object", "raw-amber-nodule")
REFINED_AMBER = eid("object", "refined-amber-discs")
DOWSING_FORK = eid("object", "tillo-dowsing-fork")
CARAVAN_WAGON = eid("object", "maela-eastroad-wagon")
WATCH_LEDGER = eid("object", "harrowcross-watch-ledger")
BROKEN_SPEAR = eid("object", "asha-broken-spear")
WRETCH_RESIDUE = eid("object", "wretch-frost-residue")
AMBER_CHEST = eid("object", "harrowcross-amber-chest")
MINE_LAMP = eid("object", "saint-orra-mine-lamp")
MINE_TALLY = eid("object", "mine-amber-tally")
AMBER_VEIN = eid("object", "living-amber-vein")
CAVE_MAP = eid("object", "jorund-cave-map")
RUIN_TABLET = eid("object", "sunken-hall-tablet")
AMBER_RELIQUARY = eid("object", "amber-reliquary")
ROOT_CROWN = eid("object", "tree-kings-root-crown")
WOOD_MASK = eid("object", "root-host-wood-mask")
BLOOD_CHAINS = eid("object", "blood-ring-chains")
MOTH_KEY = eid("object", "moths-bone-key")
GEAR_CACHE = eid("object", "party-gear-cache")
ROOT_CUDGEL = eid("object", "rootjaws-amber-cudgel")
GUILD_LOG = eid("object", "blackroot-compact-log")
BLACKROOT_COUNTERFOIL = eid("object", "blackroot-warden-counterfoil")

BADGES = {RHEA: RHEA_BADGE, SYLVI: SYLVI_BADGE, GARRAN: GARRAN_BADGE, VEYRA: VEYRA_BADGE, PIP: PIP_BADGE}

# Environments
ENV_BRINE = eid("environment", "keldmouth-brine-fog")
ENV_GUILD = eid("environment", "guildhall-smoke")
ENV_SLEET = eid("environment", "eastroad-sleet")
ENV_COAST_DUSK = eid("environment", "coastal-twilight")
ENV_MOURNING = eid("environment", "harrowcross-mourning")
ENV_QUICKENING = eid("environment", "amber-quickening")
ENV_ORCHARD = eid("environment", "orchard-frost")
ENV_MINE = eid("environment", "mine-lamp-haze")
ENV_CAVEIN = eid("environment", "cave-in-dust")
ENV_BLACKWATER = eid("environment", "blackwater-cold")
ENV_RUINS = eid("environment", "sunken-hall-resonance")
ENV_PHASE = eid("environment", "wretch-phasing")
ENV_HILL = eid("environment", "hilltop-dawn")
ENV_CAMP = eid("environment", "root-camp-smoke")
ENV_RING = eid("environment", "blood-ring-drums")
ENV_PURSUIT = eid("environment", "northwood-pursuit-rain")

# Scenes
SC_DOCK = eid("scene", "mouth-of-the-keld")
SC_GUILD = eid("scene", "lantern-pike-induction")
SC_PAY = eid("scene", "amber-at-the-pay-table")
SC_ROAD = eid("scene", "eastroad-caravan")
SC_CAMPIRE = eid("scene", "saltmere-campfire")
SC_HARROW = eid("scene", "harrowcross-at-dusk")
SC_LEFT = eid("scene", "left-at-harrowcross")
SC_WRETCH = eid("scene", "first-wretch-night")
SC_ORCHARD = eid("scene", "tracks-in-the-orchard")
SC_MINE = eid("scene", "saint-orra-mine")
SC_CAVEIN = eid("scene", "cave-in-dark")
SC_BLACKWATER = eid("scene", "blackwater-below")
SC_HALL = eid("scene", "sunken-hall")
SC_GALLERY = eid("scene", "wretch-gallery")
SC_HILL = eid("scene", "hilltop-dawn")
SC_CAPTURE = eid("scene", "camp-under-wooden-faces")
SC_AUDIENCE = eid("scene", "audience-of-the-tree-king")
SC_PENS = eid("scene", "blood-ring-pens")
SC_ESCAPE = eid("scene", "the-open-pen")
SC_PURSUIT = eid("scene", "the-hunt-begins")
SC_RETURN = eid("scene", "road-back-to-harrowcross")
SC_WAYMARK = eid("scene", "drowned-waymark")
SC_SOUTHWARD = eid("scene", "southward-cut")

# Conversations
CV_BLACKROOT = eid("conversation", "blackroot-compact")
CV_DOCK = eid("conversation", "five-at-the-dock")
CV_GUILD = eid("conversation", "new-hires-at-lantern-pike")
CV_PAY = eid("conversation", "amber-wages")
CV_CAMP = eid("conversation", "saltmere-campfire")
CV_SERGEANT = eid("conversation", "sergeants-account")
CV_DEPARTURE = eid("conversation", "maelas-departure")
CV_KELLAN = eid("conversation", "kellans-broken-watch")
CV_TILLO = eid("conversation", "tillo-and-the-fork")
CV_JORUND = eid("conversation", "jorunds-terms")
CV_CAVEIN = eid("conversation", "after-the-cave-in")
CV_HALL = eid("conversation", "the-sunken-hall-debate")
CV_BELOW = eid("conversation", "second-night-below")
CV_TREE = eid("conversation", "the-tree-kings-offer")
CV_PENS = eid("conversation", "before-the-blood-ring")
CV_PURSUIT = eid("conversation", "running-under-the-drums")
CV_WAYMARK = eid("conversation", "water-over-the-mark")
CV_SOUTHWARD = eid("conversation", "the-southward-cut")

# Events
EV_BLACKROOT_DISCOVERY = eid("event", "blackroot-amber-discovery")
EV_COMPACT = eid("event", "guild-suppresses-amber-correlation")
EV_ARRIVE = eid("event", "party-arrives-keldmouth")
EV_HIRED = eid("event", "party-hired-lantern-pike")
EV_PAID = eid("event", "party-paid-in-amber")
EV_DEPART = eid("event", "eastroad-caravan-departs")
EV_ROAD_GLIMMER = eid("event", "roadside-fox-glimmer")
EV_LAST_WATCH = eid("event", "harrowcross-last-watch")
EV_GUARD_MASSACRE = eid("event", "harrowcross-guard-massacre")
EV_CARAVAN_ARRIVES = eid("event", "caravan-arrives-harrowcross")
EV_ASSIGNED = eid("event", "party-assigned-harrowcross")
EV_CARAVAN_LEAVES = eid("event", "caravan-leaves-party")
EV_AMBER_QUICKENS = eid("event", "barracks-amber-quickens")
EV_FIRST_WRETCH = eid("event", "first-wretch-attack")
EV_WRETCH_DRIVEN = eid("event", "party-drives-off-wretch")
EV_KELLAN_TESTIFIES = eid("event", "kellan-testifies")
EV_ORCHARD_ATTACK = eid("event", "orchard-wretch-attack")
EV_TILLO_DOWSES = eid("event", "tillo-dowses-barracks")
EV_AMBER_PAYMENT = eid("event", "amber-payment-revealed")
EV_MINE_APPROACH = eid("event", "party-reaches-saint-orra")
EV_ENTER_MINE = eid("event", "party-enters-mine")
EV_VEIN_REACTS = eid("event", "living-vein-reacts")
EV_MINE_WRETCH = eid("event", "wretch-phases-through-mine-wall")
EV_CAVEIN = eid("event", "mine-cave-in")
EV_JORUND_LOST = eid("event", "jorund-lost-behind-collapse")
EV_REGROUP = eid("event", "party-regroups-underground")
EV_BLACKWATER_CROSS = eid("event", "party-crosses-blackwater")
EV_FIND_CAMP = eid("event", "party-finds-old-underground-camp")
EV_HALL_FOUND = eid("event", "party-finds-sunken-hall")
EV_TABLET_READ = eid("event", "veyra-reads-memory-resin-tablet")
EV_RELIQUARY_OPEN = eid("event", "amber-reliquary-opened")
EV_WRETCH_PACK = eid("event", "wretch-pack-manifests")
EV_GALLERY_ESCAPE = eid("event", "party-escapes-wretch-gallery")
EV_SURFACE = eid("event", "party-emerges-hilltop")
EV_SEE_CAMP = eid("event", "party-sees-root-camp")
# Preserve the canonical ID of the authored entry event while keeping the
# reconstruction deterministic with the rest of this specification.
EV_ENTER_CAMP = "event_4R6SFZP4A1C2N8W0M3K5H7D9QE"
EV_CAPTURED = eid("event", "party-captured-by-root-host")
EV_STRIPPED = eid("event", "party-gear-confiscated")
EV_TREE_AUDIENCE = eid("event", "tree-king-claims-party")
EV_BLOODSPORT = eid("event", "tree-king-decrees-blood-sport")
EV_ROOTJAW = eid("event", "rootjaw-demonstrates-amber-strength")
EV_PEN_OPEN = eid("event", "moth-opens-prison-pen")
EV_ESCAPE = eid("event", "party-escapes-root-camp")
EV_GEAR_RECOVERED = eid("event", "party-recovers-gear")
EV_ALARM = eid("event", "root-host-raises-alarm")
EV_PURSUIT = eid("event", "root-host-pursues-party")
EV_REACH_WAYMARK = eid("event", "party-reaches-drowned-waymark")
EV_BREAK_PURSUIT = eid("event", "drowned-waymark-breaks-root-host-pursuit")
EV_COUNTERFOIL = eid("event", "party-recovers-blackroot-counterfoil")
EV_TAKE_SOUTHWARD = eid("event", "party-takes-southward-cut")

# Story points
SP_JOIN = eid("story-point", "join-lantern-pike")
SP_AMBER_PAY = eid("story-point", "learn-amber-pay")
SP_CARAVAN = eid("story-point", "protect-eastroad-caravan")
SP_REACH = eid("story-point", "reach-harrowcross")
SP_REPLACE = eid("story-point", "replace-dead-watch")
SP_LAST_WATCH = eid("story-point", "reconstruct-last-watch")
SP_FIRST_WRETCH = eid("story-point", "survive-first-wretch")
SP_AMBER_LINK = eid("story-point", "connect-amber-to-wretches")
SP_MINE = eid("story-point", "trace-attacks-to-mine")
SP_ENTER_MINE = eid("story-point", "enter-saint-orra")
SP_CAVEIN = eid("story-point", "escape-cave-in")
SP_CAVERNS = eid("story-point", "cross-lower-caverns")
SP_RUINS = eid("story-point", "decode-sunken-hall")
SP_GALLERY = eid("story-point", "escape-wretch-gallery")
SP_SURFACE = eid("story-point", "reach-surface")
SP_TREE = eid("story-point", "understand-tree-king")
SP_BLOOD = eid("story-point", "escape-before-blood-sport")
SP_MOTH = eid("story-point", "identify-moth")
SP_FLEE = eid("story-point", "flee-root-host")
SP_GUILD_SECRET = eid("story-point", "expose-blackroot-compact")
SP_WHAT_AMBER = eid("story-point", "what-is-amber")
SP_RETURN = eid("story-point", "return-to-harrowcross")

# Relationships: deterministic directed pair IDs.
def rel_id(source: str, target: str) -> str:
    return eid("relationship", f"{source}-to-{target}")


def rel_pair(source: str, target: str) -> tuple[str, str]:
    return rel_id(source, target), rel_id(target, source)

CHARACTER_AUDIENCES = {
    "Summary": ["public", "self", "author"],
    "Appearance": ["public", "self", "author"],
    "Voice": ["public", "self", "author"],
    "Goals": ["self", "author"],
    "Private pressure": ["self", "author"],
    "Author notes": ["author"],
}


def character_record(entity_id: str, title: str, role: str, location: str, summary: str, appearance: str, voice: str, goals: str, pressure: str, author_notes: str, *, pronouns: tuple[str, str, str] = ("they", "them", "their"), tags: Iterable[str] = (), aliases: Iterable[str] = (), status: str = "canonical") -> dict[str, Any]:
    fm = common("character", entity_id, title, "cast.frontiersmen", status=status, tags=list(dict.fromkeys([role, *tags])), aliases=aliases)
    fm.update({
        "section_audiences": deepcopy(CHARACTER_AUDIENCES),
        "pronouns": {"subject": pronouns[0], "object": pronouns[1], "possessive": pronouns[2]},
        "role": role,
        "initial_state": {"location": {"entity": location}, "condition": "ready" if status == "canonical" else "dead"},
    })
    body = f"""
# {title}

## Summary

{summary}

## Appearance

{appearance}

## Voice

{voice}

## Goals

{goals}

## Private pressure

{pressure}

## Author notes

{author_notes}
"""
    return upsert(fm, body)


def location_record(entity_id: str, title: str, location_type: str, parent: str | None, description: str, *, tags: Iterable[str] = (), aliases: Iterable[str] = (), links: Iterable[dict[str, Any]] = ()) -> dict[str, Any]:
    fm = common("location", entity_id, title, "setting.frontier", tags=tags, aliases=aliases)
    fm.update({"location_type": location_type, "parent": parent, "links": list(links)})
    return upsert(fm, f"# {title}\n\n{description}")


def object_record(entity_id: str, title: str, object_type: str, initial_state: dict[str, Any], description: str, *, tags: Iterable[str] = (), aliases: Iterable[str] = (), capabilities: Iterable[str] = ()) -> dict[str, Any]:
    fm = common("object", entity_id, title, "props.frontiersmen", tags=tags, aliases=aliases)
    fm.update({"object_type": object_type, "initial_state": initial_state, "capabilities": list(capabilities)})
    return upsert(fm, f"# {title}\n\n{description}")


def environment_record(entity_id: str, title: str, start_tick: int, end_tick: int, targets: Iterable[str], conditions: dict[str, Any], sensory: Iterable[str], description: str, *, tags: Iterable[str] = ()) -> dict[str, Any]:
    fm = common("environment", entity_id, title, "environment.frontier", tags=tags)
    fm.update({"time": {"start": t(start_tick), "end": t(end_tick)}, "targets": list(targets), "conditions": conditions, "sensory": list(sensory)})
    return upsert(fm, f"# {title}\n\n{description}")


CHARACTERS: dict[str, dict[str, Any]] = {
    RHEA: dict(title="Rhea Marrow", role="shield-veteran", location=KELD_DOCK,
        summary="A human veteran with a caravan shield, a disciplined temper, and the habit of becoming responsible whenever everyone else hesitates.",
        appearance="Broad-shouldered, weather-browned, with an old spear scar across the left palm and a blue coat cut down from military issue.",
        voice="Short declarative sentences. She asks for names, distances, and exits before motives. Anger makes her quieter.",
        goals="Keep the new company alive, earn enough to leave old disgrace behind, and refuse any contract that requires abandoning people already under her protection.",
        pressure="Rhea left the southern legions after disobeying an order to burn a plague camp. The guild contract is her first lawful work since then, and she fears every act of command will be judged as another refusal.",
        author_notes="Primary viewpoint and practical leader. She begins wary of Veyra's fascination with amber and ends suspecting that the guild's protection business depends on the danger it conceals.", pronouns=("she","her","her"), tags=("viewpoint","fighter"), aliases=("Rhea",)),
    SYLVI: dict(title="Sylvi Ashdown", role="half-elf-tracker", location=KELD_DOCK,
        summary="A half-elven woodsrunner who reads animal absence as carefully as tracks. The Frontier's silence bothers her before its monsters do.",
        appearance="Lean, gray-eyed, with ash-brown braids, a horn bow, and a cloak patched in four different forest greens.",
        voice="Dry, spare observations followed by long silences. She dislikes speaking before she has located the wind.",
        goals="Map a road no ranger has made safe, prove she can work with people rather than around them, and learn why ordinary animals avoid amber-bearing ground.",
        pressure="Sylvi hears a second, almost animal rhythm in quickened amber. She hides it because she has spent years being treated as too elven by humans and too human by elves.",
        author_notes="She is the party's ecological intelligence. Her interpretations are often correct but couched as tracking rather than magic.", pronouns=("she","her","her"), tags=("ranger","viewpoint"), aliases=("Syl",)),
    GARRAN: dict(title="Brother Garran Holt", role="road-priest", location=KELD_DOCK,
        summary="A dwarven priest of the Hearth and Road, trained to bless thresholds, treat wounds, and ask whether a monster can still suffer.",
        appearance="Square and red-bearded, with a copper travel shrine on his pack and mail repaired more often than polished.",
        voice="Warm, patient, and stubbornly literal about mercy. He uses old road blessings when frightened.",
        goals="Protect travelers, restore a shrine somewhere that needs one, and determine whether the wretches are spirits, memories, or living beings trapped in the wrong shape.",
        pressure="Garran's first instinct is to call every apparition a restless dead. The Frontier repeatedly punishes that certainty, forcing a crisis in the theology that brought him there.",
        author_notes="He anchors the party morally and medically. His final theory—that wretches are pain remembering a body—comes closest to the truth without fully naming it.", pronouns=("he","him","his"), tags=("cleric","healer"), aliases=("Garran",)),
    VEYRA: dict(title="Veyra Kest", role="tiefling-arcanist", location=KELD_DOCK,
        summary="A tiefling hedge-arcanist with formal training in sympathetic materials and an inconvenient delight in phenomena everyone else calls cursed.",
        appearance="Tall, umber-skinned, with swept black horns, gold spectacles, and ink stains on the fingers of both hands.",
        voice="Precise until excited, then rapid and metaphor-heavy. She corrects terminology even while running.",
        goals="Understand amber's latent magic, earn recognition without a patron, and keep curiosity from becoming the reason the party dies.",
        pressure="Amber answers Veyra more readily than it should. The Tree King sees that susceptibility and offers her the clearest temptation in the story.",
        author_notes="She first identifies amber as a storage medium for impressions rather than conventional spell matter. Her fascination is useful and dangerous.", pronouns=("she","her","her"), tags=("sorcerer","arcanist","viewpoint"), aliases=("Veyra",)),
    PIP: dict(title="Pip Fenlock", role="halfling-scout", location=KELD_DOCK,
        summary="A halfling locksmith, cartographer, and opportunist who treats being underestimated as a renewable resource.",
        appearance="Small even for a halfling, quick-handed, with a red knit cap, a case of folding picks, and three maps that contradict one another.",
        voice="Light, sideways jokes that become exact technical descriptions when stakes rise.",
        goals="Draw the first useful map of the eastern Frontier, become indispensable to people who initially dismiss him, and never again be trapped behind a lock he did not inspect.",
        pressure="Pip joined because debt collectors made the mainland smaller every month. In the caves he discovers that jokes cannot make darkness less real, but they can keep the others moving.",
        author_notes="He notices guild knots on the Tree King's equipment and engineers the escape. His maps preserve spatial continuity through the underground act.", pronouns=("he","him","his"), tags=("rogue","cartographer","viewpoint"), aliases=("Pip",)),
    HALRIC: dict(title="Guildmaster Halric Doss", role="guildmaster", location=GUILDHALL,
        summary="Master of the Lantern Pike lodge in Keldmouth, a courteous administrator who can turn a dangerous omission into an employment term.",
        appearance="Silver-bearded, broad, and carefully civilian in dark wool, with an amber-headed walking cane he never lets touch blood.",
        voice="Measured, paternal, and full of phrases such as 'operationally sufficient.' He never lies when a narrower truth will do.",
        goals="Keep caravans moving, keep amber flowing, and preserve the guild's authority over a danger it helped create.",
        pressure="Halric signed the Blackroot Compact after the first confirmed amber manifestation. He believes concealment prevents panic and that monster contracts are the price of settlement.",
        author_notes="He knows fresh blood and fractured raw amber can call wretches. Secret marker: FRONTIER-SECRET-AMBER-ROOT-MEMORY-7K4M.", pronouns=("he","him","his"), tags=("guild","secret-keeper"), aliases=("Master Doss",)),
    NARA: dict(title="Nara Beech", role="guild-clerk", location=GUILDHALL,
        summary="The Lantern Pike's young contract clerk, responsible for wages, caravan rosters, and pretending newcomers already understand frontier customs.",
        appearance="Sharp-featured, ink-cuffed, and perpetually surrounded by tagged keys and wax tablets.",
        voice="Fast, practical, and embarrassed by questions she was trained not to answer.",
        goals="Keep the ledgers balanced, keep Halric's confidence, and avoid becoming responsible for the consequences hidden inside the wage schedule.",
        pressure="Nara knows amber payments correlate with emergency contracts but does not know the physical mechanism. She has learned not to ask why some ledgers are locked separately.",
        author_notes="A potential later whistleblower. Her evasions about amber are institutional habit more than malice.", pronouns=("she","her","her"), tags=("guild","clerk"), aliases=("Nara",)),
    MAELA: dict(title="Maela Brigg", role="caravan-master", location=EAST_GATE,
        summary="A hard caravan master who measures mercy in delays, spare axles, and how many mouths can still be fed at the next stop.",
        appearance="Weather-seamed, iron-gray hair tied with sail cord, two knives worn where everyone can see them.",
        voice="Blunt contractual language. She treats bad news as a change in route weight.",
        goals="Get twelve wagons east before the autumn washouts and avoid inheriting any problem not written into her bond.",
        pressure="Maela has left people behind before and survived because of it. Harrowcross becomes another decision she refuses to revisit.",
        author_notes="She is not secretly villainous; her abandonment is the Frontier's logistical cruelty made personal.", pronouns=("she","her","her"), tags=("caravan","contractual"), aliases=("Maela",)),
    EDRIK: dict(title="Sergeant Edrik Morn", role="frontier-sergeant", location=BARRACKS,
        summary="The exhausted sergeant responsible for Harrowcross's dead watch, left with one survivor and too many details that do not fit an animal attack.",
        appearance="Late forties, shaved head, one sleeve pinned above the wrist from an older hunt, uniform worn through at the elbows.",
        voice="Formal reports interrupted by flashes of personal grief. He calls the dead by rank until his composure breaks.",
        goals="Keep Harrowcross alive, learn what killed his watch, and stop the guild from classifying the deaths as ordinary frontier attrition.",
        pressure="Edrik stored the quarterly amber payment beneath the barracks because the town lacked a vault. He does not initially connect that choice to the massacre.",
        author_notes="He is culpable only through ignorance. His report gives the party its first impossible physical facts.", pronouns=("he","him","his"), tags=("harrowcross","sergeant"), aliases=("Sergeant Morn",)),
    HESSA: dict(title="Reeve Hessa Noll", role="reeve", location=HARROWCROSS,
        summary="The elected reeve of Harrowcross, a farmer-politician who understands that a village can be abandoned by paperwork before it is abandoned by people.",
        appearance="Sturdy, sun-browned, wearing a wool shawl over a chain shirt kept from an earlier militia season.",
        voice="Plain, local, and unsentimental. She repeats a promise back to the speaker so it cannot later become smaller.",
        goals="Keep the farms planted, prevent panic, and force the guild to earn the amber Harrowcross has already paid.",
        pressure="Hessa knows the village's amber chest was unusually warm on the massacre night but fears saying so will make the guild seize their only reserve.",
        author_notes="She represents the settlement's moral claim on the party after the caravan leaves.", pronouns=("she","her","her"), tags=("harrowcross","reeve"), aliases=("Reeve Noll",)),
    TILLO: dict(title="Tillo Wick", role="gnome-dowser", location=MINE_YARD,
        summary="A gnomish amber dowser whose fork twitches toward near-surface pockets and away from places where the amber has become 'quick.'",
        appearance="Compact, white-haired, with brass ear hoops, slate goggles, and a forked hazel rod bound in copper wire.",
        voice="Elliptical frontier idiom. He answers dangerous questions with measurements, songs, and warnings about what not to warm.",
        goals="Locate workable amber without waking it, protect other dowsers from guild discipline, and someday tell the whole truth without causing a rush or a purge.",
        pressure="Tillo witnessed the Blackroot manifestation and signed the compact under threat of losing his dowsing license. He recognizes Aldren beneath the Tree King's scars.",
        author_notes="He knows amber is the hardened memory-sap of the buried Heartwood Below. He will reveal only fragments to the party.", pronouns=("he","him","his"), tags=("gnome","dowser","secret-keeper"), aliases=("Tillo",)),
    JORUND: dict(title="Jorund Bale", role="mine-foreman", location=MINE_YARD,
        summary="Foreman of Saint Orra's Cut, an experienced miner who treats every magical anomaly as a ventilation problem until that becomes impossible.",
        appearance="Heavy-set, black-bearded, with yellow lamp glass sewn into his cap and old amber burns along his right forearm.",
        voice="Gruff, technical, and defensive about production. He distrusts guild hunters who arrive after miners have already died.",
        goals="Keep the mine open, find the missing lower crew, and prevent the guild from blaming his workers for phenomena management refuses to name.",
        pressure="Jorund has sealed two side drifts after wall-phasing attacks and falsified the tally to hide them from Halric.",
        author_notes="He is separated by the cave-in; his survival remains unresolved at the story's end.\n\nThe last action the party sees is to throw his lower-drift map and mine lamp across the fracture before the roof falls between them.", pronouns=("he","him","his"), tags=("mine","foreman"), aliases=("Jorund",)),
    KELLAN: dict(title="Kellan Voss", role="surviving-watchman", location=BARRACKS,
        summary="The only Harrowcross watchman to survive the barracks massacre, found beneath a collapsed bunk unable to account for how the attackers crossed the walls.",
        appearance="Young, hollow-eyed, left cheek bandaged, hands shaking hardest around warm metal.",
        voice="Fragmentary and sensory. He remembers sounds and temperatures before sequence.",
        goals="Make the others believe Asha did not open the door, remember what came through the floor, and stop sleeping beside amber.",
        pressure="Kellan heard the wretches repeat the dying guards' own voices. He fears that remembering them clearly will call them back.",
        author_notes="His recollection is incomplete but materially accurate: the first wretch emerged from the amber chest after Bran bled across it.\n\nAt the later recurrence, his reopened wound quickens the same chest and he witnesses another wretch pass through the barracks wall.", pronouns=("he","him","his"), tags=("harrowcross","survivor"), aliases=("Kellan",)),
    SENNA: dict(title="Senna Reed", role="farmer-witness", location=SOUTH_FIELDS,
        summary="A flax farmer whose orchard borders the old mine track. She saw an antlered shape pass through an apple-tree trunk without bending a branch.",
        appearance="Strong-backed, middle-aged, with an orchard knife, a green scarf, and frost-burned fingertips.",
        voice="Concrete rural speech. She distrusts explanations that make witnesses sound foolish.",
        goals="Protect her household, get the eastern orchard reopened, and make the guild admit the attacks follow amber carts.",
        pressure="Senna's husband secretly accepted raw amber instead of coin and buried it beneath their hearth.",
        author_notes="Her testimony points toward movement through matter and the coastal mine road.", pronouns=("she","her","her"), tags=("harrowcross","witness"), aliases=("Senna",)),
    TREE_KING: dict(title="The Tree King", role="root-host-king", location=ROOT_COURT,
        summary="A self-crowned sovereign of a masked woodland host who claims communion with the Frontier and demonstrates terrifying control over quickened amber.",
        appearance="Tall and gaunt beneath a crown of living roots pinned through an old guild officer's hood. Amber veins shine under scar tissue along his throat.",
        voice="Courtly, expansive, and convinced that cruelty becomes natural law when named a trial. He speaks to the forest as if it answers between words.",
        goals="Strengthen his host through blood sport, break the guild's monopoly on amber, and force Veyra to admit that the substance chooses worthy vessels.",
        pressure="He was Aldren Veyl, Halric's field-master at Blackroot. Exposure to a blooded seam let him hear the Heartwood's stored lives; he mistakes overwhelming memory for kingship.",
        author_notes="He can provoke manifestations by feeding blood and fear into carved amber lattices. His communion is real, but his interpretation is self-serving.", pronouns=("he","him","his"), tags=("antagonist","amber-bound","secret"), aliases=("Aldren Veyl","Root-Crowned Aldren")),
    MOTH: dict(title="Moth", role="masked-acolyte", location=ROOT_CAMP,
        summary="A slight masked acolyte of the Tree King, mute like the rest of the host and distinguishable only by a moth-wing notch in the left cheek of the wooden mask.",
        appearance="Wrapped in bark-cloth, barefoot despite the cold, with a whitewood mask and one finger missing from the right hand.",
        voice="Moth never speaks. Communication is limited to gestures, breath, and occasional involuntary grunts.",
        goals="Keep the Tree King from using the prisoners in the blood ring and create one unobserved opening without revealing open rebellion.",
        pressure="Moth is Lio Vane, a Harrowcross road guard taken two seasons earlier and partially bound by an amber-backed mask. Removing it may kill him.",
        author_notes="Moth leaves the pen open and the bone key visible. The party does not learn the true name during this arc.", pronouns=("he","him","his"), tags=("root-host","mute","secret"), aliases=("Lio Vane",)),
    ROOTJAW: dict(title="Rootjaw", role="masked-champion", location=BLOOD_RING,
        summary="The Tree King's largest masked champion, mute, broad as a door, and strengthened by amber set into a carved boar mask.",
        appearance="A hulking figure in layered rawhide, wearing a black-root boar mask whose tusks are capped with liquid-gold amber.",
        voice="Rootjaw does not speak, only snorts and strikes the cudgel against the earth.",
        goals="Obey the Tree King, defeat challengers in the blood ring, and feed each victory into the mask's growing strength.",
        pressure="There is little of the original person left. The mask stores every defeated opponent's panic and plays it back as strength.",
        author_notes="Rootjaw demonstrates the Tree King's method but does not fight the party before their escape.", pronouns=("he","him","his"), tags=("root-host","champion","mute"), aliases=()),
    ASHA: dict(title="Corporal Asha Pell", role="dead-watch-corporal", location=BARRACKS,
        summary="The experienced corporal who commanded the last Harrowcross watch and died without opening the barred door.",
        appearance="Remembered in a red wool sash and boiled-leather coat, with a broken spear still clenched after death.",
        voice="Crisp, local, and protective of younger guards.",
        goals="Keep the barracks secure and get Kellan through his first winter watch.",
        pressure="Asha recognized the amber chest was humming but mistook it for mine-cart resonance.",
        author_notes="Retired character used for historical events and Kellan's recollection.", pronouns=("she","her","her"), tags=("harrowcross","dead"), aliases=(), status="retired"),
    BRAN: dict(title="Bran Tew", role="dead-watchman", location=BARRACKS,
        summary="A Harrowcross watchman killed during the barracks manifestation after cutting his palm on a broken bottle beside the amber chest.",
        appearance="Large, freckled, remembered for a green scarf and a laugh that carried through the palisade.",
        voice="Loud, joking, eager to make bad nights ordinary.",
        goals="Finish the winter watch and buy his mother's field out of debt.",
        pressure="His blood was the immediate catalyst for the chest's quickening.",
        author_notes="Retired historical character.", pronouns=("he","him","his"), tags=("harrowcross","dead"), aliases=(), status="retired"),
    NOLLY: dict(title="Nolly Dey", role="dead-watchman", location=BARRACKS,
        summary="A quiet watchman killed when a wretch emerged through the bunk wall behind him.",
        appearance="Thin, dark-haired, with an oversized watch coat and a carved bird whistle.",
        voice="Soft and methodical.",
        goals="Complete enough service to join the river patrol.",
        pressure="His last words are repeated by one of the wretches in Kellan's memory.",
        author_notes="Retired historical character.", pronouns=("he","him","his"), tags=("harrowcross","dead"), aliases=(), status="retired"),
}


LOCATIONS = [
    (FRONTIER, "The Frontier Peninsula", "peninsula", None, "A remote northern peninsula of black spruce, peat bog, glacial ridges, and a few low river flats suitable for farming. Roads are narrow clearings maintained by whoever most recently needed them.", ("frontier","boreal"), ()),
    (KELDMOUTH, "Keldmouth", "port-city", FRONTIER, "A timber port built where the River Keld broadens into a cold gray estuary. Storehouses stand on driven piles, and nearly every public scale is fitted to weigh amber as well as grain.", ("port","city"), ()),
    (KELD_DOCK, "Keldmouth River Dock", "dock", KELDMOUTH, "A wet stone quay at the river mouth where mainland packets unload recruits, salt, iron, and more promises than the Frontier can keep.", ("dock","arrival"), ()),
    (GUILDHALL, "Lantern Pike Guildhall", "guildhall", KELDMOUTH, "A longhouse of tarred beams, trophy skulls, contract boards, and a central hearth beneath a crossed lantern and boar spear.", ("guild","monster-hunters"), ()),
    (AMBER_EXCHANGE, "Keldmouth Amber Exchange", "exchange", KELDMOUTH, "A guarded counting house where raw nodules are weighed behind mica screens and refined amber discs circulate as local currency.", ("amber","currency"), ()),
    (EAST_GATE, "Keldmouth East Gate", "gate", KELDMOUTH, "The palisade gate opening onto the coast road. Caravan bells are checked here because the next proper smith is days away.", ("road","gate"), ()),
    (COAST_ROAD, "East Coastal Road", "road", FRONTIER, "A semi-coastal wagon route running between dark forest and glimpses of iron-gray sea, turning inland wherever cliffs break the shoreline.", ("road","caravan"), ()),
    (SALTMERE, "Saltmere Camp", "road-camp", COAST_ROAD, "A raised gravel camp beside a brackish pond, marked by three wind-bent pines and the old circles of caravan fires.", ("camp","road"), ()),
    (HARROWCROSS, "Harrowcross", "crossroads-town", FRONTIER, "A palisaded crossroads town where the coast road meets the mine track and a southern farm lane. Barley and flax survive in the low ground around it.", ("town","crossroads"), ()),
    (BARRACKS, "Harrowcross Watch Barracks", "barracks", HARROWCROSS, "A squat log barracks beside the east gate. The door bar was still set after the massacre, and clawed blood marks continue behind furniture and through solid plank walls.", ("barracks","massacre"), ()),
    (THREE_PINES, "The Three Pines Inn", "inn", HARROWCROSS, "A crowded inn with one stone chimney and a common room used for council meetings whenever the barracks are unfit.", ("inn","village"), ()),
    (SOUTH_FIELDS, "Harrowcross South Fields", "farmland", HARROWCROSS, "Low, drained fields of barley, flax, and turnip divided by alder windbreaks and raised plank paths.", ("farm","lowland"), ()),
    (EAST_ORCHARD, "Harrowcross East Orchard", "orchard", HARROWCROSS, "A neglected apple orchard along the old mine track. Frost gathers here even when the town roofs remain wet.", ("orchard","wretch"), ()),
    (MINE, "Saint Orra's Amber Mine", "mine", FRONTIER, "A shallow-cut amber mine named for a patron saint of lamps. Its earliest drifts follow near-surface pockets located by gnome dowsers.", ("mine","amber"), ()),
    (MINE_YARD, "Saint Orra Mine Yard", "mine-yard", MINE, "A churned yard of ore sledges, timber braces, wash troughs, and locked amber scales.", ("mine","yard"), ()),
    (UPPER_DRIFT, "Saint Orra Upper Drift", "mine-drift", MINE, "A timbered tunnel sloping beneath the eastern ridge, wide enough for ore sledges and wet with mineral seepage.", ("mine","tunnel"), ()),
    (AMBER_CHAMBER, "Deep Amber Chamber", "amber-pocket", MINE, "A natural chamber where liquid-gold amber glows in thin membranes between black roots fossilized into stone.", ("amber","danger"), ()),
    (CAVEIN, "Cave-In Pocket", "collapse-chamber", MINE, "A trapped pocket beyond the collapsed upper drift, filled with dust, broken braces, and a fissure descending into older natural caves.", ("cave-in","survival"), ()),
    (LOWER_CAVERNS, "Lower Caverns", "cavern-system", FRONTIER, "A sprawling system of limestone tubes, blackwater channels, and roots petrified into amber-veined pillars.", ("caverns","underground"), ()),
    (BLACKWATER, "Blackwater Crossing", "underground-river", LOWER_CAVERNS, "A waist-deep underground river crossing beneath a ceiling lost in darkness. Pale animal shapes move inside the stone banks.", ("river","underground"), ()),
    (SUNKEN_HALL, "Sunken Hall Ruins", "ancient-ruin", LOWER_CAVERNS, "A cyclopean hall tilted into the cave floor, carved with animals, hunters, and streams of gold flowing from a buried tree.", ("ruins","ancient"), ()),
    (WRETCH_GALLERY, "Wretch Gallery", "ruin-gallery", SUNKEN_HALL, "A long gallery whose reliefs show animal and human bodies overlapping. Amber lines in the walls brighten when anyone bleeds or panics.", ("ruins","wretches"), ()),
    (HILLTOP, "Hilltop Sink", "sinkhole-exit", FRONTIER, "A small grassy hill rising above endless spruce. A stone throat at its crown opens into the lower caverns.", ("surface","isolated"), ()),
    (ROOT_CAMP, "Root-Crowned Camp", "forest-camp", FRONTIER, "A hidden camp of bark shelters and pole racks beneath ancient spruce, occupied by mute figures in carved wooden masks.", ("tree-king","masked-host"), ()),
    (ROOT_COURT, "Root Court", "forest-court", ROOT_CAMP, "A circle of root benches around a split cedar throne. Amber hangs in cages over blood-dark soil.", ("tree-king","court"), ()),
    (BLOOD_RING, "Blood Ring", "arena", ROOT_CAMP, "A shallow earthen arena edged with roots, stakes, and amber shards blackened by old blood.", ("arena","blood-sport"), ()),
    (PRISON_PENS, "Root Camp Prison Pens", "prison", ROOT_CAMP, "Wattle enclosures roofed with thorn branches. The latches are bone, cord, and superstition rather than iron.", ("prison","escape"), ()),
    (PURSUIT_TRAIL, "Northwood Pursuit Trail", "forest-trail", FRONTIER, "An animal path through rain-black spruce, alder tangles, and granite ribs. Drums and hunting grunts carry farther than sight.", ("pursuit","forest"), ()),
]


OBJECTS = [
    (CONTRACT, "Lantern Pike Hiring Contract", "contract", {"location": {"entity": GUILDHALL}, "condition": "unsigned"}, "A five-name monster-hunting contract assigning the new hires to caravan protection and emergency settlement duty at guild discretion.", ("guild","contract"), ("can-be-signed",)),
    (RHEA_BADGE, "Rhea's Lantern Pike Badge", "guild-badge", {"location": {"entity": GUILDHALL}, "condition": "unissued"}, "A brass badge showing a hooded lantern crossed with a boar spear.", ("guild","badge"), ()),
    (SYLVI_BADGE, "Sylvi's Lantern Pike Badge", "guild-badge", {"location": {"entity": GUILDHALL}, "condition": "unissued"}, "A brass badge showing a hooded lantern crossed with a boar spear.", ("guild","badge"), ()),
    (GARRAN_BADGE, "Garran's Lantern Pike Badge", "guild-badge", {"location": {"entity": GUILDHALL}, "condition": "unissued"}, "A brass badge showing a hooded lantern crossed with a boar spear.", ("guild","badge"), ()),
    (VEYRA_BADGE, "Veyra's Lantern Pike Badge", "guild-badge", {"location": {"entity": GUILDHALL}, "condition": "unissued"}, "A brass badge showing a hooded lantern crossed with a boar spear.", ("guild","badge"), ()),
    (PIP_BADGE, "Pip's Lantern Pike Badge", "guild-badge", {"location": {"entity": GUILDHALL}, "condition": "unissued"}, "A brass badge showing a hooded lantern crossed with a boar spear.", ("guild","badge"), ()),
    (AMBER_WAGES, "First Amber Wages", "currency-pouch", {"location": {"entity": AMBER_EXCHANGE}, "condition": "sealed"}, "A waxed pouch containing five drilled amber discs and a pinch of powdered charcoal. The discs look like liquid gold caught inside glass.", ("amber","currency"), ("can-be-spent",)),
    (RAW_AMBER, "Raw Amber Nodule", "raw-amber", {"holder": {"entity": TILLO}, "condition": "dormant"}, "An irregular translucent nodule warm at its center and threaded with shapes like roots or veins.", ("amber","raw","magic"), ("can-quicken",)),
    (REFINED_AMBER, "Refined Amber Discs", "currency", {"location": {"entity": AMBER_EXCHANGE}, "condition": "damped"}, "Thin standardized discs of amber hardened with charcoal and tin dust for trade.", ("amber","currency"), ("can-be-spent",)),
    (DOWSING_FORK, "Tillo's Dowsing Fork", "dowsing-tool", {"holder": {"entity": TILLO}, "condition": "tuned"}, "A forked hazel rod bound in copper wire. It twists toward shallow amber and shudders away from quickened pockets.", ("gnome","amber","tool"), ("dowses-amber",)),
    (CARAVAN_WAGON, "Maela's Eastroad Wagon", "wagon", {"location": {"entity": EAST_GATE}, "condition": "roadworthy"}, "The lead wagon of Maela Brigg's twelve-wagon eastbound caravan, marked with blue axle paint.", ("caravan","wagon"), ()),
    (WATCH_LEDGER, "Harrowcross Watch Ledger", "ledger", {"location": {"entity": BARRACKS}, "condition": "blood-spattered"}, "A watch ledger ending mid-entry on the massacre night. The final line notes that the amber payment chest was humming.", ("ledger","evidence"), ("can-be-read",)),
    (BROKEN_SPEAR, "Asha Pell's Broken Spear", "weapon", {"location": {"entity": BARRACKS}, "condition": "snapped and frost-burned"}, "The corporal's spear, broken inward against a wall that shows no impact mark.", ("weapon","evidence"), ()),
    (WRETCH_RESIDUE, "Wretch Frost Residue", "residue", {"location": {"entity": BARRACKS}, "condition": "melting slowly"}, "A rime that smells of wet fur and old iron, left where a wretch passed through timber.", ("wretch","evidence"), ("can-be-sampled",)),
    (AMBER_CHEST, "Harrowcross Amber Chest", "strongbox", {"location": {"entity": BARRACKS}, "condition": "warm and cracked"}, "The town's quarterly protection payment: raw and refined amber stored together in a cedar chest beneath the watch bunks.", ("amber","payment","danger"), ("contains-amber",)),
    (MINE_LAMP, "Saint Orra Mine Lamp", "lamp", {"holder": {"entity": JORUND}, "condition": "lit"}, "A yellow-glass oil lamp designed to show color changes near amber seams.", ("mine","lamp"), ()),
    (MINE_TALLY, "Saint Orra Amber Tally", "tally-board", {"location": {"entity": MINE_YARD}, "condition": "altered"}, "A production tally with two sealed drifts omitted from the official count.", ("mine","ledger","secret"), ("can-be-read",)),
    (AMBER_VEIN, "Living Amber Vein", "amber-seam", {"location": {"entity": AMBER_CHAMBER}, "condition": "dormant"}, "A membrane of liquid-gold amber running between black petrified roots. It pulses faintly when frightened creatures approach.", ("amber","magic","secret"), ("can-quicken",)),
    (CAVE_MAP, "Jorund's Lower Drift Map", "map", {"holder": {"entity": JORUND}, "condition": "incomplete"}, "A charcoal plan of the worked mine ending at a blank marked OLD WATER.", ("map","mine"), ("can-be-read",)),
    (RUIN_TABLET, "Sunken Hall Memory Tablet", "stone-tablet", {"location": {"entity": SUNKEN_HALL}, "condition": "legible in amber light"}, "A stone tablet describing golden sap that keeps the shapes of hunted things after flesh is gone.", ("ruins","amber","lore"), ("can-be-deciphered",)),
    (AMBER_RELIQUARY, "Amber Reliquary", "reliquary", {"location": {"entity": SUNKEN_HALL}, "condition": "sealed"}, "A stone box containing an ancient amber lattice and blackened animal teeth arranged around it.", ("ruins","amber","danger"), ("can-be-opened",)),
    (ROOT_CROWN, "Root Crown", "crown", {"holder": {"entity": TREE_KING}, "condition": "living and blooded"}, "A crown of spruce roots threaded through an old guild hood, studded with raw amber that brightens when the Tree King speaks.", ("tree-king","amber","artifact"), ("amplifies-amber",)),
    (WOOD_MASK, "Root Host Wooden Mask", "mask", {"location": {"entity": ROOT_CAMP}, "condition": "amber-backed"}, "A frightening animal mask carved from green wood. A thin amber plate is fixed inside where it touches the wearer's face.", ("mask","root-host","amber"), ("binds-memory",)),
    (BLOOD_CHAINS, "Blood Ring Chains", "restraint", {"location": {"entity": BLOOD_RING}, "condition": "blood-darkened"}, "Root-bound chains used to tether captives during the Tree King's strengthening rites.", ("blood-sport","restraint"), ()),
    (MOTH_KEY, "Moth's Bone Key", "key", {"holder": {"entity": MOTH}, "condition": "concealed"}, "A carved bone hook that releases the cord-and-root latch on the prison pens.", ("escape","key"), ("opens-prison-pen",)),
    (GEAR_CACHE, "Confiscated Party Gear", "gear-cache", {"location": {"entity": ROOT_CAMP}, "condition": "bundled under guard"}, "The party's weapons, packs, badges, maps, and surviving amber tied together beneath a bark shelter.", ("party","gear"), ()),
    (ROOT_CUDGEL, "Rootjaw's Amber Cudgel", "weapon", {"holder": {"entity": ROOTJAW}, "condition": "quickened"}, "A root-club ringed with raw amber. Impacts release animal screams that do not come from the target.", ("weapon","amber","root-host"), ()),
    (GUILD_LOG, "Blackroot Compact Log", "secret-ledger", {"location": {"entity": GUILDHALL}, "condition": "locked away"}, "A sealed guild log recording the first blood-triggered amber manifestation and the decision to classify later incidents as monster activity.", ("guild","secret","amber"), ("can-be-read",)),
]


ENVIRONMENTS = [
    (ENV_BRINE, "Keldmouth Brine Fog", 8, 16, [KELD_DOCK, KELDMOUTH], {"visibility": "short", "air": "salt-wet", "bells": "muffled"}, ["Fog beads on rigging and horn tips.", "The river smells of pine pitch, fish, and iron mud."], "The party's arrival is obscured by estuary fog."),
    (ENV_GUILD, "Guildhall Hearth Smoke", 14, 26, [GUILDHALL], {"light": "firelit", "noise": "contracts and trophy stories", "privacy": "low"}, ["Boar skulls cast long shadows above the contract board.", "Amber in Halric's cane glows when the hearth pops."], "The guild presents frontier danger as organized work."),
    (ENV_SLEET, "Eastroad Sleet", 26, 38, [COAST_ROAD], {"weather": "cold sleet", "road": "muddy", "pace": "slow"}, ["Spruce branches comb the wagon covers.", "Sea wind reaches the road in sudden iron-smelling gusts."], "The weather makes the caravan dependent on its new guards."),
    (ENV_COAST_DUSK, "Coastal Twilight", 38, 50, [SALTMERE, COAST_ROAD], {"light": "long northern dusk", "animals": "unusually quiet"}, ["The sky stays pale long after sunset.", "No foxes or ravens approach the food scraps."], "The first subtle amber manifestation occurs under this quiet."),
    (ENV_MOURNING, "Harrowcross Mourning", 50, 70, [HARROWCROSS, BARRACKS, THREE_PINES], {"bells": "cloth-wrapped", "doors": "barred before dark", "work": "suspended"}, ["Black cord is tied around the east gate posts.", "Every conversation stops when the barracks creaks."], "The town is already living around an unexplained massacre."),
    (ENV_QUICKENING, "Amber Quickening", 63, 72, [BARRACKS, AMBER_CHEST], {"temperature": "rising around amber", "sound": "sub-audible pulse", "boundary": "thin"}, ["Gold light moves inside the amber without changing the room's shadows.", "Old blood darkens as if becoming wet again."], "Fresh fear and old blood wake the payment chest."),
    (ENV_ORCHARD, "East Orchard Frost", 72, 90, [EAST_ORCHARD], {"temperature": "below surrounding fields", "tracks": "end at solid trunks"}, ["Frost rims only the eastern side of each apple tree.", "The air smells of crushed leaves and wet fur."], "The orchard preserves evidence of wall-phasing attacks."),
    (ENV_MINE, "Mine Lamp Haze", 90, 108, [MINE_YARD, UPPER_DRIFT, AMBER_CHAMBER], {"light": "yellow lamp smoke", "supports": "creaking", "air": "mineral-heavy"}, ["Amber seams turn the lamp flame green at their edges.", "Every hammer strike returns as a delayed second knock."], "The mine makes amber's active properties harder to dismiss."),
    (ENV_CAVEIN, "Cave-In Dust", 108, 116, [CAVEIN], {"air": "choked with limestone", "exit": "blocked", "light": "two surviving lamps"}, ["Dust turns every breath to paste.", "The fallen timbers settle in little thunderclaps."], "The party becomes isolated from the surface and from Jorund."),
    (ENV_BLACKWATER, "Blackwater Cold", 117, 130, [LOWER_CAVERNS, BLACKWATER], {"temperature": "near freezing", "current": "slow but deep", "light": "amber reflections"}, ["The water reflects gold where no amber is visible.", "Something with antlers seems to walk inside the stone bank."], "Two days underground erode certainty and supplies."),
    (ENV_RUINS, "Sunken Hall Resonance", 127, 142, [SUNKEN_HALL], {"sound": "voices linger after speech", "amber": "responsive to emotion"}, ["Carved hunters appear to move when seen by amber light.", "Spoken names return from the walls in another voice."], "The ruins preserve the oldest direct clues to amber's nature."),
    (ENV_PHASE, "Wretch Phasing", 140, 150, [WRETCH_GALLERY], {"walls": "permeable to manifestations", "fear_response": "amplified"}, ["Paws strike from inside the relief stone.", "Human teeth appear briefly in animal muzzles."], "A pack of wretches forces the party toward the surface route."),
    (ENV_HILL, "Hilltop Dawn", 150, 160, [HILLTOP], {"weather": "clear and cold", "orientation": "unknown", "smoke": "visible northeast"}, ["The forest extends unbroken beyond every slope.", "A thin column of camp smoke rises through the spruce."], "Relief at reaching the surface becomes the temptation to approach the only visible camp."),
    (ENV_CAMP, "Root Camp Drum Smoke", 159, 178, [ROOT_CAMP, ROOT_COURT], {"smoke": "resinous", "speech": "only the king speaks", "guards": "masked and silent"}, ["Wooden faces turn in unison without a word.", "Drums under the earth answer the Tree King's root crown."], "The masked host is disciplined, uncanny, and largely mute."),
    (ENV_RING, "Blood Ring Frenzy", 175, 188, [BLOOD_RING, PRISON_PENS], {"amber": "blood-quickened", "crowd": "silent except for grunts", "stakes": "freshly sharpened"}, ["Amber shards brighten whenever the crowd stamps.", "The masks seem to breathe before the people inside them do."], "The Tree King intends violence to strengthen the host."),
    (ENV_PURSUIT, "Northwood Pursuit Rain", 188, 215, [PURSUIT_TRAIL], {"weather": "cold rain", "visibility": "one bowshot", "pursuers": "closing by drum signals"}, ["Rain strips scent from the trail but makes every root shine.", "Hunting grunts move through the trees on both sides."], "The story ends with the party running deeper into the Frontier under pursuit."),
]

BLACKROOT = eid("location", "blackroot-sink")
LOCATIONS.append((BLACKROOT, "Blackroot Sink", "abandoned-prospect", FRONTIER, "A collapsed prospecting pit west of Keldmouth where the Lantern Pike first documented blood-triggered amber manifestations twelve years before the party arrived.", ("secret","amber","history"), ()))
CHARACTERS[TREE_KING]["location"] = BLACKROOT

BASE_METRICS = {"trust": 0.0, "affinity": 0.0, "fear": 0.0, "obligation": 0.0, "respect": 0.0, "dependence": 0.0}

PARTY_PAIR_INITIALS: dict[tuple[str, str], tuple[dict[str, float], list[str], str]] = {
    (RHEA, SYLVI): ({"trust": 0.22, "affinity": 0.06, "fear": 0.0, "obligation": 0.02, "respect": 0.28, "dependence": 0.05}, ["capable-stranger", "independent"], "Rhea recognizes Sylvi's competence but expects resistance to command."),
    (SYLVI, RHEA): ({"trust": 0.12, "affinity": 0.02, "fear": 0.0, "obligation": 0.0, "respect": 0.24, "dependence": 0.02}, ["soldier", "possible-leader"], "Sylvi is prepared to follow good field decisions, not rank."),
    (RHEA, GARRAN): ({"trust": 0.34, "affinity": 0.18, "fear": 0.0, "obligation": 0.08, "respect": 0.3, "dependence": 0.12}, ["healer", "steady"], "Rhea trusts Garran's calm before she knows his doctrine."),
    (GARRAN, RHEA): ({"trust": 0.32, "affinity": 0.16, "fear": 0.0, "obligation": 0.04, "respect": 0.34, "dependence": 0.08}, ["burdened-leader"], "Garran sees someone already preparing to carry too much."),
    (RHEA, VEYRA): ({"trust": 0.02, "affinity": 0.0, "fear": 0.12, "obligation": 0.0, "respect": 0.18, "dependence": 0.05}, ["unknown-magic", "useful"], "Rhea respects Veyra's knowledge and distrusts her appetite for risk."),
    (VEYRA, RHEA): ({"trust": 0.16, "affinity": 0.06, "fear": 0.02, "obligation": 0.0, "respect": 0.22, "dependence": 0.04}, ["practical", "unimaginative-at-first"], "Veyra expects Rhea to be useful and intellectually conservative."),
    (RHEA, PIP): ({"trust": 0.18, "affinity": 0.12, "fear": 0.0, "obligation": 0.04, "respect": 0.08, "dependence": 0.02}, ["young-looking", "quick-handed"], "Rhea underestimates Pip's strategic value."),
    (PIP, RHEA): ({"trust": 0.24, "affinity": 0.18, "fear": 0.02, "obligation": 0.02, "respect": 0.18, "dependence": 0.04}, ["shield", "easy-to-tease"], "Pip decides Rhea is safest when annoyed rather than uncertain."),
    (SYLVI, GARRAN): ({"trust": 0.18, "affinity": 0.08, "fear": 0.0, "obligation": 0.0, "respect": 0.16, "dependence": 0.03}, ["priest", "loud-in-woods"], "Sylvi likes Garran and doubts his wilderness judgment."),
    (GARRAN, SYLVI): ({"trust": 0.2, "affinity": 0.14, "fear": 0.0, "obligation": 0.02, "respect": 0.22, "dependence": 0.04}, ["tracker", "guarded"], "Garran trusts Sylvi's senses and worries about her isolation."),
    (SYLVI, VEYRA): ({"trust": -0.04, "affinity": 0.02, "fear": 0.06, "obligation": 0.0, "respect": 0.18, "dependence": 0.03}, ["arcane-scent", "competitive-observer"], "Sylvi dislikes Veyra touching unknown things before reading the ground around them."),
    (VEYRA, SYLVI): ({"trust": 0.04, "affinity": 0.04, "fear": 0.0, "obligation": 0.0, "respect": 0.2, "dependence": 0.03}, ["empirical-rival"], "Veyra is fascinated by how often Sylvi reaches the right answer without formal theory."),
    (SYLVI, PIP): ({"trust": 0.3, "affinity": 0.22, "fear": 0.0, "obligation": 0.02, "respect": 0.22, "dependence": 0.08}, ["scouting-partner"], "Sylvi appreciates that Pip can move quietly and knows when not to talk."),
    (PIP, SYLVI): ({"trust": 0.3, "affinity": 0.24, "fear": 0.0, "obligation": 0.02, "respect": 0.26, "dependence": 0.08}, ["scouting-partner", "difficult-audience"], "Pip treats making Sylvi laugh as a navigational achievement."),
    (GARRAN, VEYRA): ({"trust": 0.08, "affinity": 0.06, "fear": 0.04, "obligation": 0.0, "respect": 0.18, "dependence": 0.04}, ["theological-disagreement"], "Garran respects Veyra's discipline and worries that curiosity is her form of worship."),
    (VEYRA, GARRAN): ({"trust": 0.14, "affinity": 0.1, "fear": 0.0, "obligation": 0.0, "respect": 0.2, "dependence": 0.04}, ["kind-skeptic"], "Veyra finds Garran's metaphors irritatingly useful."),
    (GARRAN, PIP): ({"trust": 0.28, "affinity": 0.32, "fear": 0.0, "obligation": 0.08, "respect": 0.18, "dependence": 0.06}, ["fond", "protective"], "Garran enjoys Pip immediately and assumes he needs more protecting than he does."),
    (PIP, GARRAN): ({"trust": 0.34, "affinity": 0.3, "fear": 0.0, "obligation": 0.04, "respect": 0.2, "dependence": 0.08}, ["safe-company", "too-earnest"], "Pip trusts Garran's hands and mocks his sermons affectionately."),
    (VEYRA, PIP): ({"trust": 0.2, "affinity": 0.16, "fear": 0.0, "obligation": 0.0, "respect": 0.22, "dependence": 0.06}, ["clever-hands", "useful-questions"], "Veyra notices that Pip asks questions other people are embarrassed to ask."),
    (PIP, VEYRA): ({"trust": 0.16, "affinity": 0.14, "fear": 0.08, "obligation": 0.0, "respect": 0.26, "dependence": 0.05}, ["dangerous-scholar", "interesting"], "Pip trusts Veyra's honesty about danger more than her restraint around it."),
}

NPC_REL_INITIALS: dict[tuple[str, str], tuple[dict[str, float], list[str], str, str]] = {
    (RHEA, MAELA): ({"trust": 0.1, "affinity": -0.02, "fear": 0.0, "obligation": 0.12, "respect": 0.18, "dependence": 0.1}, ["employer", "hard-practical"], "Rhea accepts Maela's road authority while judging her priorities.", "caravan-guard-and-master"),
    (MAELA, RHEA): ({"trust": 0.2, "affinity": 0.0, "fear": 0.0, "obligation": 0.0, "respect": 0.24, "dependence": 0.12}, ["new-guard", "possible-leader"], "Maela sees a recruit who can make decisions without asking the wagons to wait.", "caravan-master-and-guard"),
    (RHEA, EDRIK): ({"trust": 0.08, "affinity": 0.0, "fear": 0.02, "obligation": 0.06, "respect": 0.2, "dependence": 0.06}, ["local-sergeant", "bereaved"], "Rhea initially sees Edrik as a damaged but competent local authority.", "guild-hunter-and-sergeant"),
    (EDRIK, RHEA): ({"trust": 0.12, "affinity": 0.0, "fear": 0.0, "obligation": 0.18, "respect": 0.14, "dependence": 0.24}, ["new-guild-hire", "needed"], "Edrik needs the guild badge more than he trusts the person wearing it.", "sergeant-and-replacement-guard"),
    (VEYRA, TILLO): ({"trust": 0.06, "affinity": 0.08, "fear": 0.04, "obligation": 0.0, "respect": 0.34, "dependence": 0.1}, ["dowser", "withholding-theory"], "Veyra knows Tillo understands amber and resents his evasions before meeting him.", "arcanist-and-dowser"),
    (TILLO, VEYRA): ({"trust": -0.08, "affinity": 0.02, "fear": 0.18, "obligation": 0.0, "respect": 0.3, "dependence": 0.04}, ["amber-sensitive", "dangerously-curious"], "Tillo recognizes the kind of scholar who may wake what she studies.", "dowser-and-arcanist"),
    (RHEA, TREE_KING): ({"trust": -0.9, "affinity": -0.7, "fear": 0.55, "obligation": 0.0, "respect": 0.16, "dependence": 0.0}, ["captor", "fanatic"], "Rhea's relationship record begins with the hostility expected once his identity becomes relevant.", "prisoner-and-captor"),
    (TREE_KING, RHEA): ({"trust": -0.5, "affinity": 0.1, "fear": 0.02, "obligation": 0.0, "respect": 0.44, "dependence": 0.0}, ["candidate-champion", "defiant"], "The Tree King values Rhea as material for trial and command.", "captor-and-challenger"),
    (MOTH, TREE_KING): ({"trust": -0.86, "affinity": -0.55, "fear": 0.88, "obligation": 0.35, "respect": -0.3, "dependence": 0.72}, ["bound-host", "terrified-rebel"], "Moth is magically and socially bound while secretly resisting.", "bound-acolyte-and-king"),
    (TREE_KING, MOTH): ({"trust": 0.1, "affinity": -0.05, "fear": 0.0, "obligation": 0.0, "respect": -0.2, "dependence": 0.02}, ["minor-acolyte", "disposable"], "The Tree King barely distinguishes Moth from the rest of the host.", "king-and-acolyte"),
    (MOTH, RHEA): ({"trust": 0.34, "affinity": 0.04, "fear": 0.18, "obligation": 0.28, "respect": 0.22, "dependence": 0.04}, ["possible-escapee", "unmasked-outsider"], "Moth invests a small hope in Rhea's refusal to submit.", "silent-helper-and-prisoner"),
    (RHEA, MOTH): ({"trust": -0.12, "affinity": 0.0, "fear": 0.22, "obligation": 0.0, "respect": 0.0, "dependence": 0.0}, ["masked-gaoler", "uncertain"], "Rhea initially sees Moth as another dangerous acolyte.", "prisoner-and-masked-acolyte"),
}


def initial_relationship_operations() -> list[dict[str, Any]]:
    operations: list[dict[str, Any]] = []
    for (source, target), (metrics, facets, note) in PARTY_PAIR_INITIALS.items():
        rid, inverse = rel_pair(source, target)
        operations.append(relationship_record(
            rid,
            f"{CHARACTERS[source]['title']}'s view of {CHARACTERS[target]['title']}",
            source,
            target,
            "new-company",
            inverse,
            [relationship_transition(f"initial:{source}:{target}", 0, 0, metrics, facets, note)],
            f"A directional relationship inside the newly formed hunting company. {note}",
        ))
    for (source, target), (metrics, facets, note, kind) in NPC_REL_INITIALS.items():
        rid, inverse = rel_pair(source, target)
        operations.append(relationship_record(
            rid,
            f"{CHARACTERS[source]['title']}'s view of {CHARACTERS[target]['title']}",
            source,
            target,
            kind,
            inverse,
            [relationship_transition(f"initial:{source}:{target}", 0, 0, metrics, facets, note)],
            f"A directional Frontier relationship. {note}",
        ))
    return operations


STORY_POINT_SPECS = [
    (SP_JOIN, "Join the Lantern Pike", 100, "Sign the guild contract and receive the badges that make the party legally responsible for frontier emergencies."),
    (SP_AMBER_PAY, "Learn What Counts as Pay", 72, "Understand that guild wages and settlement protection fees are paid in amber rather than ordinary coin."),
    (SP_CARAVAN, "Protect the Eastroad Caravan", 80, "Guard Maela Brigg's caravan from Keldmouth toward the eastern settlements."),
    (SP_REACH, "Reach Harrowcross", 78, "Bring the caravan to the crossroads town where the coast road meets the mine track."),
    (SP_REPLACE, "Replace the Dead Watch", 94, "Take over Harrowcross protection after the overnight destruction of its watch detail."),
    (SP_LAST_WATCH, "Reconstruct the Last Watch", 88, "Determine how the watch died inside a barred barracks and what Kellan actually heard."),
    (SP_FIRST_WRETCH, "Survive the First Wretch", 96, "Protect Harrowcross when an ethereal animal-wretch manifests through the barracks walls."),
    (SP_AMBER_LINK, "Connect Amber to the Wretches", 92, "Establish whether amber storage, blood, fear, or mining activity precedes the manifestations."),
    (SP_MINE, "Trace the Attacks to Saint Orra's Mine", 86, "Follow the orchard frost, mine road, and dowsing evidence toward the amber workings."),
    (SP_ENTER_MINE, "Enter Saint Orra's Mine", 82, "Investigate the sealed drifts and missing lower crew despite Jorund's resistance."),
    (SP_CAVEIN, "Escape the Cave-In", 100, "Survive the collapse, find an alternate route, and keep the company together underground."),
    (SP_CAVERNS, "Cross the Lower Caverns", 90, "Travel through two days of unmapped cavern systems with dwindling food and lamps."),
    (SP_RUINS, "Decode the Sunken Hall", 89, "Interpret the old reliefs and tablet describing amber as a vessel for remembered forms."),
    (SP_GALLERY, "Escape the Wretch Gallery", 98, "Cross a ruin gallery where wretches can emerge from every carved surface."),
    (SP_SURFACE, "Reach the Surface", 92, "Find an exit from the cavern system and establish where the party has emerged."),
    (SP_TREE, "Understand the Tree King", 95, "Learn why the masked king can harness amber and whether his communion with the Frontier is real."),
    (SP_BLOOD, "Escape Before the Blood Sport", 100, "Leave the Root Host's pens before the party is used to strengthen masked champions."),
    (SP_MOTH, "Identify the Silent Acolyte", 66, "Determine why the moth-notched acolyte helps the prisoners and what the mask conceals."),
    (SP_FLEE, "Flee the Root Host", 100, "Stay ahead of the masked host through the deep northern woods."),
    (SP_GUILD_SECRET, "Expose the Blackroot Compact", 91, "Recover evidence that the guild has long known amber extraction provokes manifestations."),
    (SP_WHAT_AMBER, "What Is Amber?", 97, "Separate frontier superstition, guild euphemism, ancient record, and the Tree King's claims into a defensible theory."),
    (SP_RETURN, "Return to Harrowcross", 84, "Find a route back to the abandoned duty post with warning and evidence."),
]


def foundation_story_points() -> list[dict[str, Any]]:
    return [
        story_point_record(entity_id, title, priority=priority, body=body, tags=("campaign",))
        for entity_id, title, priority, body in STORY_POINT_SPECS
    ]


def world_record(world_id: str, *, final: bool = False, current_summary: str | None = None) -> dict[str, Any]:
    fm = common("world", world_id, "The Frontiersmen", "world", tags=["example", "frontier-fantasy", "monster-hunters", "interactive"])
    fm.update({
        "section_audiences": {"Public premise": ["public", "author"], "Author truth": ["author"], "Current canonical state": ["author"]},
        "default_timeline": "main",
        "timelines": [{"id": "main", "label": "Main chronology"}],
        "state_keys": {
            "character": {"location": {"type": "entity", "entity_kind": "location", "exclusive": True}, "condition": {"type": "string"}},
            "object": {
                "holder": {"type": "entity", "entity_kind": "character", "exclusive_group": "placement"},
                "location": {"type": "entity", "entity_kind": "location", "exclusive_group": "placement"},
                "container": {"type": "entity", "entity_kind": "object", "exclusive_group": "placement"},
                "condition": {"type": "string"},
            },
        },
        "relationship_metrics": {
            "trust": {"minimum": -1.0, "maximum": 1.0, "default": 0.0},
            "affinity": {"minimum": -1.0, "maximum": 1.0, "default": 0.0},
            "fear": {"minimum": 0.0, "maximum": 1.0, "default": 0.0},
            "obligation": {"minimum": 0.0, "maximum": 1.0, "default": 0.0},
            "respect": {"minimum": -1.0, "maximum": 1.0, "default": 0.0},
            "dependence": {"minimum": 0.0, "maximum": 1.0, "default": 0.0},
        },
        "embedding_policy": {"provider": "lsa", "model": "wedl-lsa-v1", "dimensions": 192, "max_features": 8192},
        "context_policy": {"default_budget_characters": 8000, "default_max_items": 24, "conversation_turn_window": 6},
        "compilation_policy": {
            "default_profile": "hybrid" if final else "fts",
            "retained_revisions": 5,
            "vector_cache": True,
            "source_parse_cache": True,
            "atomic_publish": True,
            "retrieval": {"fts_candidate_limit": 120, "vector_candidate_limit": 120, "hybrid_fts_weight": 1.0, "hybrid_vector_weight": 1.0, "hybrid_rrf_k": 60.0},
        },
    })
    current = current_summary or (
        "At tick 195 the five new guild hunters are running north through rain-black spruce. The Root Host is behind them, Moth's bone key is in Pip's hand, their gear is only partly recovered, and none of them knows a safe route back to Harrowcross."
        if final
        else "The story has not yet advanced beyond the guild's suppressed Blackroot history."
    )
    body = f"""
# The Frontiersmen

## Public premise

The Frontier is a remote boreal peninsula reached through Keldmouth, a port at the mouth of the River Keld. Monster-hunting guilds protect caravans and settlements while gnome dowsers locate near-surface pockets of strange liquid-gold amber. New hires Rhea Marrow, Sylvi Ashdown, Brother Garran Holt, Veyra Kest, and Pip Fenlock arrive expecting paid caravan work and discover a frontier economy whose currency, monsters, and institutions are entangled.

## Current canonical state

{current}

## Author truth

Amber is not ordinary mineral. It is hardened memory-sap from the buried Heartwood Below, a vast ancient living network beneath the peninsula. Fresh blood, concentrated fear, and abrupt fracture can make raw amber 'quick,' releasing stored predator and victim impressions as ethereal animal-wretches that move through walls and ground. The Lantern Pike guild documented this at Blackroot and concealed the correlation while accepting amber for protection services. Former field-master Aldren Veyl rejected the concealment, survived catastrophic exposure, and became the Tree King. He correctly learned to call and strengthen manifestations with blooded amber, then mistook access to accumulated memory for divine kingship. Secret marker: FRONTIER-SECRET-AMBER-ROOT-MEMORY-7K4M.
"""
    return upsert(fm, body)


def blackroot_history_operations() -> list[dict[str, Any]]:
    turns = [
        turn("blackroot:0", 2, 10, TILLO, "The fork is turning away. Amber does not do that unless it is quick.", delivery="frightened"),
        turn("blackroot:1", 2, 20, TREE_KING, "Then we have found something more valuable than a seam.", delivery="intent"),
        turn("blackroot:2", 2, 30, HALRIC, "We have found three dead porters and no defensible report.", delivery="controlled"),
        turn("blackroot:3", 2, 40, TILLO, "Bran's blood touched the membrane. The things came out wearing the hunt.", delivery="insistent"),
        turn("blackroot:4", 2, 50, TREE_KING, "They came out because the amber remembered them.", delivery="awed"),
        turn("blackroot:5", 3, 0, HALRIC, "That sentence does not leave this pit.", delivery="formal"),
        turn("blackroot:6", 3, 10, TREE_KING, "You would sell the memory and call its teeth a separate business.", delivery="accusing"),
        turn("blackroot:7", 3, 20, HALRIC, "I would keep Keldmouth from emptying in a week.", delivery="flat"),
        turn("blackroot:8", 3, 30, TILLO, "Damp the trade pieces. Never store raw and refined together. Never let blood near a pocket.", delivery="technical"),
        turn("blackroot:9", 3, 40, HALRIC, "Those become internal handling rules. The manifestation becomes an unclassified frontier predator.", delivery="decisive"),
        turn("blackroot:10", 3, 50, TREE_KING, "Then I resign before your bookkeeping learns to hunt.", delivery="cold"),
    ]
    operations: list[dict[str, Any]] = [
        event_op(EV_BLACKROOT_DISCOVERY, "The Blackroot Amber Manifestation", 2, 5, BLACKROOT, [(HALRIC, "guildmaster"), (TREE_KING, "field-master"), (TILLO, "dowser")], "At Blackroot Sink, a porter bleeds onto a newly exposed amber membrane. Ethereal shapes of a stag, wolf, and wounded man emerge through the pit wall and kill three workers before dissolving into frost.", effects=[
            effect("blackroot-halric", HALRIC, "location", "set", {"entity": BLACKROOT}),
            effect("blackroot-aldren", TREE_KING, "location", "set", {"entity": BLACKROOT}),
            effect("blackroot-tillo", TILLO, "location", "set", {"entity": BLACKROOT}),
        ], tags=("history", "amber", "wretch")),
        conversation_op(CV_BLACKROOT, "The Blackroot Compact", status="closed", start=t(2, 10), end=t(3, 50), scene=None, location=BLACKROOT,
            participants=[participant(HALRIC, "guildmaster", t(2, 10), t(3, 50)), participant(TREE_KING, "field-master", t(2, 10), t(3, 50)), participant(TILLO, "dowser", t(2, 10), t(3, 50))],
            turns=turns, topics=("amber", "guild-secret", "blackroot"), body="The first confirmed connection between blooded amber and wretch manifestations becomes an institutional secret."),
        event_op(EV_COMPACT, "The Lantern Pike Suppresses the Amber Correlation", 4, 0, BLACKROOT, [(HALRIC, "author"), (TREE_KING, "dissenter"), (TILLO, "coerced-witness")], "Halric writes handling rules into a private log but orders public reports to classify manifestations as unrelated frontier predators. Aldren resigns and disappears into the interior.", causes=(EV_BLACKROOT_DISCOVERY,), effects=[
            effect("compact-halric-home", HALRIC, "location", "set", {"entity": GUILDHALL}),
            effect("compact-tillo-home", TILLO, "location", "set", {"entity": MINE_YARD}),
            effect("compact-aldren-root", TREE_KING, "location", "set", {"entity": ROOT_COURT}),
            effect("compact-aldren-condition", TREE_KING, "condition", "set", "amber-bound"),
        ], tags=("history", "guild-secret")),
        recollection_op(CV_BLACKROOT, "halric-blackroot", HALRIC, 5, 0, "Aldren wanted to publicize the manifestations; Tillo supplied handling rules; Halric chose controlled concealment to preserve settlement.", "The compact was a necessary administrative sin that later field officers failed to manage carefully.", "defensive certainty", 0.88, exact_turns=(turns[8]["id"], turns[9]["id"])),
        recollection_op(CV_BLACKROOT, "tillo-blackroot", TILLO, 5, 0, "Blood woke the amber and the dead shapes came through stone. Halric turned the warning into secret procedure.", "The guild understands enough to be responsible for every later amber death.", "fear hardened into shame", 0.96, exact_turns=(turns[3]["id"], turns[9]["id"])),
        recollection_op(CV_BLACKROOT, "aldren-blackroot", TREE_KING, 5, 0, "The amber remembered every hunted shape and Halric chose to sell the sap while hiring blades against its memories.", "The Frontier selected Aldren to overthrow the guild's false separation between wealth and violence.", "revelation and grandiosity", 0.99, exact_turns=(turns[4]["id"], turns[6]["id"])),
        knowledge_record(eid("knowledge", "halric-knows-blood-amber"), "Halric knows blood can quicken raw amber", HALRIC, "amber.quickens.with-blood", "Fresh blood can quicken exposed raw amber and provoke wretch manifestations.", 4, 10, truth="true", confidence=0.99, acquisition="observed", subject=RAW_AMBER, predicate="quickens-with", object_value={"text": "fresh blood and concentrated fear"}, causing_event=EV_BLACKROOT_DISCOVERY, source_entity=RAW_AMBER, tags=("secret", "amber")),
        knowledge_record(eid("knowledge", "tillo-knows-heartwood"), "Tillo knows amber stores remembered forms", TILLO, "amber.stores.remembered-forms", "Raw amber stores impressions of living and dying forms inside the buried root network.", 4, 20, truth="true", confidence=0.94, acquisition="inferred", subject=RAW_AMBER, predicate="stores", object_value={"text": "remembered living forms"}, causing_event=EV_BLACKROOT_DISCOVERY, source_entity=RAW_AMBER, tags=("secret", "amber")),
        knowledge_record(eid("knowledge", "aldren-hears-heartwood"), "Aldren believes the Heartwood chose him", TREE_KING, "heartwood.chose.aldren", "The buried Heartwood chose Aldren to embody and command the Frontier's remembered violence.", 4, 30, state="accepted", truth="contested", confidence=1.0, acquisition="experienced", subject=TREE_KING, predicate="chosen-by", object_value={"text": "Heartwood Below"}, causing_event=EV_BLACKROOT_DISCOVERY, source_entity=ROOT_CROWN, tags=("tree-king", "belief")),
    ]
    return operations


def foundation_operations(world_id: str) -> list[dict[str, Any]]:
    operations: list[dict[str, Any]] = [world_record(world_id, final=False)]
    operations.extend(character_record(entity_id, **values) for entity_id, values in CHARACTERS.items())
    operations.extend(location_record(entity_id, title, kind, parent, description, tags=tags, links=links) for entity_id, title, kind, parent, description, tags, links in LOCATIONS)
    operations.extend(object_record(entity_id, title, kind, initial, description, tags=tags, capabilities=capabilities) for entity_id, title, kind, initial, description, tags, capabilities in OBJECTS)
    operations.extend(environment_record(entity_id, title, start, end, targets, conditions, sensory, description, tags=("frontier",)) for entity_id, title, start, end, targets, conditions, sensory, description in ENVIRONMENTS)
    operations.extend(initial_relationship_operations())
    operations.extend(foundation_story_points())
    operations.extend(blackroot_history_operations())
    return operations

CV_LAST_WATCH = eid("conversation", "the-last-watch")

STORY_SPEC_BY_ID = {entity_id: (title, priority, body) for entity_id, title, priority, body in STORY_POINT_SPECS}


def story_update(entity_id: str, transitions: list[dict[str, Any]], *, outcomes: Iterable[str] = (), dependencies: dict[str, Any] | None = None, trigger: dict[str, Any] | None = None, activation: str = "suggest") -> dict[str, Any]:
    title, priority, body = STORY_SPEC_BY_ID[entity_id]
    return story_point_record(entity_id, title, transitions=transitions, outcomes=outcomes, dependencies=dependencies, trigger=trigger, activation=activation, priority=priority, body=body, tags=("campaign",))


def relationship_update(source: str, target: str, extra_transitions: list[dict[str, Any]], *, kind: str | None = None) -> dict[str, Any]:
    initial = PARTY_PAIR_INITIALS.get((source, target))
    relationship_kind = kind or "new-company"
    if initial is None:
        npc = NPC_REL_INITIALS[(source, target)]
        metrics, facets, note, relationship_kind = npc
    else:
        metrics, facets, note = initial
    rid, inverse = rel_pair(source, target)
    return relationship_record(
        rid,
        f"{CHARACTERS[source]['title']}'s view of {CHARACTERS[target]['title']}",
        source,
        target,
        relationship_kind,
        inverse,
        [relationship_transition(f"initial:{source}:{target}", 0, 0, metrics, facets, note), *extra_transitions],
        f"A directional relationship in The Frontiersmen. {note}",
    )


def party_move_effects(slug: str, location: str, condition: str | None = None) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for index, character in enumerate(PARTY):
        result.append(effect(f"{slug}:location:{index}", character, "location", "set", {"entity": location}))
        if condition:
            result.append(effect(f"{slug}:condition:{index}", character, "condition", "set", condition))
    return result


def party_condition_effects(slug: str, condition: str) -> list[dict[str, Any]]:
    return [
        effect(f"{slug}:condition:{index}", character, "condition", "set", condition)
        for index, character in enumerate(PARTY)
    ]


def act1_operations() -> list[dict[str, Any]]:
    operations: list[dict[str, Any]] = []

    dock_turns = [
        turn("dock:0", 10, 10, PIP, "Which one of you looks most like five competent hires from a distance?", delivery="cheerful"),
        turn("dock:1", 10, 20, RHEA, "The one carrying the contract tube.", delivery="flat"),
        turn("dock:2", 10, 30, VEYRA, "That is a fishing permit.", delivery="correcting"),
        turn("dock:3", 10, 40, PIP, "Then I am carrying local color.", delivery="unbothered"),
        turn("dock:4", 11, 0, GARRAN, "Brother Garran Holt. Hearth and Road. I mend people before contracts, when allowed.", delivery="warm"),
        turn("dock:5", 11, 10, SYLVI, "Sylvi Ashdown. The gulls will not land on the east pilings.", delivery="watchful"),
        turn("dock:6", 11, 20, VEYRA, "Veyra Kest. Those pilings are capped with amber nails.", delivery="interested"),
        turn("dock:7", 11, 30, RHEA, "Rhea Marrow. We find the guild before we study the dock.", delivery="decisive"),
        turn("dock:8", 11, 40, PIP, "Pip Fenlock. I study exits while walking toward orders.", delivery="light"),
        turn("dock:9", 12, 0, SYLVI, "Then study the forest side. Nothing living is watching the city from there.", delivery="quiet"),
    ]
    operations.extend([
        event_op(EV_ARRIVE, "The New Hires Arrive at Keldmouth", 10, 0, KELD_DOCK, [(member, "new-hire") for member in PARTY], "A mainland packet unloads five strangers into brine fog at the mouth of the River Keld. They identify one another while searching for the Lantern Pike lodge.", effects=party_move_effects("arrival", KELD_DOCK, "newly-arrived"), story_points=(SP_JOIN,), tags=("arrival", "party")),
        conversation_op(CV_DOCK, "Five at the Dock", status="closed", start=t(10, 10), end=t(12), scene=SC_DOCK, location=KELD_DOCK, participants=[participant(member, "new-hire", t(10, 10), t(12)) for member in PARTY], turns=dock_turns, topics=("arrival", "party-formation"), body="The party's first conversation establishes voice, competence, and the Frontier's unnatural animal silence."),
        scene_record(SC_DOCK, "The Mouth of the Keld", status="closed", start=t(10), current=t(13), end=t(13), location=KELD_DOCK, participants=[participant(member, "new-hire", t(10), t(13), pov=(member == RHEA)) for member in PARTY], environments=(ENV_BRINE,), conversations=(CV_DOCK,), observations=(
            observation("dock-fog", 10, 0, ("participants",), "Brine fog hides the upper city while amber-capped pilings glow beneath the quay water."),
            observation("dock-sylvi-birds", 11, 5, (SYLVI,), "Gulls crowd ordinary timber but refuse every piling capped with amber." , salience=1.25),
            observation("dock-veyra-hum", 11, 25, (VEYRA,), "The amber nail heads answer Veyra's presence with a pressure behind her teeth." , salience=1.2),
        ), constraints=("The party does not yet know amber is local currency or that it causes manifestations.",), body="Five strangers meet at the river mouth and begin forming a company before any of them has seen a guild badge.", tags=("arrival",)),
    ])
    for index, character in enumerate(PARTY):
        operations.append(recollection_op(CV_DOCK, f"dock-{index}", character, 13, index, {
            RHEA: "The other four are undisciplined but observant; Sylvi notices the wrong things are absent and Veyra notices the wrong things are magical.",
            SYLVI: "Rhea assumes command, Garran offers help without bargaining, Veyra hears the amber nails, and Pip jokes when uncertain.",
            GARRAN: "Rhea organizes the group before anyone asks, while Sylvi and Veyra independently notice that the dock is wrong.",
            VEYRA: "The party includes a soldier, a tracker, a priest, and a locksmith; the dock amber responds faintly to magic or bloodline.",
            PIP: "Rhea takes the invisible job of deciding where they go, and everyone reveals more by correcting him than by introducing themselves.",
        }[character], {
            RHEA: "They may be usable as a unit if she gives them clear work.",
            SYLVI: "The Frontier begins where animals choose not to stand.",
            GARRAN: "The company is already listening to different kinds of danger.",
            VEYRA: "Amber is active material, whatever the locals call it.",
            PIP: "The easiest way into the group is to let Rhea think she has organized him.",
        }[character], "first impressions under fog", 0.82, exact_turns=(dock_turns[7]["id"], dock_turns[9]["id"]) if character in {RHEA, SYLVI} else (dock_turns[index + 4]["id"],)))

    guild_turns = [
        turn("guild:0", 15, 10, HALRIC, "The Lantern Pike does not hire heroes. It hires people who return with the caravan, the witness, or an honest account of why neither returned.", delivery="measured"),
        turn("guild:1", 15, 20, RHEA, "What is the first assignment?", delivery="direct"),
        turn("guild:2", 15, 30, HALRIC, "Eastroad caravan. Twelve wagons. Keldmouth to the farming and amber settlements. You answer to Maela Brigg until emergency reassignment.", delivery="administrative"),
        turn("guild:3", 15, 40, PIP, "How broad is emergency?", delivery="innocent"),
        turn("guild:4", 15, 50, NARA, "Broader than the road and narrower than refusing it.", delivery="rehearsed"),
        turn("guild:5", 16, 0, GARRAN, "Monster categories?", delivery="professional"),
        turn("guild:6", 16, 10, HALRIC, "Beast, revenant, witch-work, giant-kind, and unclassified frontier predator.", delivery="smooth"),
        turn("guild:7", 16, 20, VEYRA, "Unclassified by whom?", delivery="interested"),
        turn("guild:8", 16, 30, HALRIC, "By whoever survived long enough to file.", delivery="final"),
        turn("guild:9", 17, 0, SYLVI, "What does the guild know east of Harrowcross?", delivery="testing"),
        turn("guild:10", 17, 10, HALRIC, "Enough to hire you and not enough to send you alone.", delivery="paternal"),
        turn("guild:11", 18, 0, RHEA, "Five names. One company. We sign together.", delivery="decisive"),
    ]
    hire_effects = party_move_effects("hire", GUILDHALL, "guild-hired")
    hire_effects.append(effect("contract-signed", CONTRACT, "condition", "set", "signed-by-five"))
    for index, member in enumerate(PARTY):
        hire_effects.extend([
            effect(f"badge-clear:{index}", BADGES[member], "location", "clear"),
            effect(f"badge-holder:{index}", BADGES[member], "holder", "set", {"entity": member}),
            effect(f"badge-condition:{index}", BADGES[member], "condition", "set", "issued"),
        ])
    operations.extend([
        event_op(EV_HIRED, "The Lantern Pike Hires the Five", 18, 10, GUILDHALL, [(HALRIC, "guildmaster"), (NARA, "clerk"), *[(member, "new-hire") for member in PARTY]], "The five sign one contract, receive badges, and accept emergency reassignment language they do not yet understand.", causes=(EV_ARRIVE,), story_points=(SP_JOIN, SP_CARAVAN), effects=hire_effects, tags=("guild", "contract")),
        conversation_op(CV_GUILD, "New Hires at Lantern Pike", status="closed", start=t(15, 10), end=t(18), scene=SC_GUILD, location=GUILDHALL, participants=[participant(HALRIC, "guildmaster", t(15, 10), t(18)), participant(NARA, "clerk", t(15, 10), t(18)), *[participant(member, "new-hire", t(15, 10), t(18)) for member in PARTY]], turns=guild_turns, topics=("contract", "guild", "emergency-duty"), body="Halric defines the guild's terms narrowly enough to sound honest and broadly enough to abandon the party later."),
        scene_record(SC_GUILD, "Lantern Pike Induction", status="closed", start=t(14), current=t(22), end=t(22), location=GUILDHALL, participants=[participant(HALRIC, "guildmaster", t(14), t(22)), participant(NARA, "clerk", t(14), t(22)), *[participant(member, "new-hire", t(14), t(22), pov=(member == RHEA)) for member in PARTY]], objects=(CONTRACT, *BADGES.values()), environments=(ENV_GUILD,), story_points=(SP_JOIN, SP_CARAVAN), conversations=(CV_GUILD,), observations=(
            observation("guild-cane", 15, 5, (VEYRA,), "Amber in Halric's cane brightens when the hearth spits and goes dull when he closes his hand around it.", salience=1.15),
            observation("guild-contract", 17, 20, ("participants",), "The emergency reassignment clause is written in larger script than the pay rate."),
            observation("guild-rhea-badges", 18, 20, (RHEA,), "The five badges are numbered consecutively, as if the guild already decided they are one unit."),
        ), constraints=("Halric's Blackroot knowledge remains author-only.", "The phrase unclassified frontier predator is meaningful but unexplained to the party."), body="Inside the trophy-lined guildhall, Halric Doss binds the strangers into one emergency-response company.", tags=("guild", "contract")),
        story_update(SP_JOIN, [story_transition("join-resolved", 18, 10, "resolved", EV_HIRED, "The five sign and receive guild badges.")], outcomes=(EV_HIRED,)),
        story_update(SP_CARAVAN, [story_transition("caravan-active", 18, 20, "active", EV_HIRED, "The party accepts Maela Brigg's Eastroad caravan assignment.")]),
    ])
    for index, character in enumerate(PARTY):
        operations.extend([
            knowledge_record(eid("knowledge", f"{character}-guild-contract"), f"{CHARACTERS[character]['title']} knows the guild emergency clause", character, "guild.contract.emergency-reassignment", "The Lantern Pike may reassign its hunters to settlement emergencies without renegotiating the road contract.", 18, 20 + index, truth="true", confidence=0.99, acquisition="read", subject=CONTRACT, predicate="permits", object_value={"text": "emergency settlement reassignment"}, causing_event=EV_HIRED, source_entity=CONTRACT, tags=("guild", "contract")),
            recollection_op(CV_GUILD, f"guild-{index}", character, 22, index, {
                RHEA: "Halric hired them as one company and made emergency reassignment part of the bargain.",
                SYLVI: "The guild knows the road only in reports and uses survivors to classify what it does not understand.",
                GARRAN: "The guild recognizes unclassified predators but offers no doctrine for what that means.",
                VEYRA: "Halric avoided naming the authority behind monster categories and his amber cane reacted to heat.",
                PIP: "The largest print in the contract explains how the guild can change the job after the party is already far from port.",
            }[character], {
                RHEA: "The clause is broad, but a signed duty remains a duty.",
                SYLVI: "The guild's ignorance may be curated rather than accidental.",
                GARRAN: "They are being hired for uncertainty, not merely combat.",
                VEYRA: "Unclassified is an institutional category worth investigating.",
                PIP: "The contract's trap is not hidden; it is simply printed where desperate people will accept it.",
            }[character], "professional caution", 0.9, exact_turns=(guild_turns[2]["id"], guild_turns[4]["id"])),
        ])

    pay_turns = [
        turn("pay:0", 23, 10, NARA, "One drilled disc each now, four after Harrowcross, plus board on caravan days.", delivery="routine"),
        turn("pay:1", 23, 20, PIP, "This is amber.", delivery="stating-the-obvious"),
        turn("pay:2", 23, 30, NARA, "Yes.", delivery="equally-obvious"),
        turn("pay:3", 23, 40, PIP, "I was promised money.", delivery="patient"),
        turn("pay:4", 23, 50, NARA, "You were promised local wages.", delivery="precise"),
        turn("pay:5", 24, 0, VEYRA, "Why the charcoal powder in the pouch?", delivery="technical"),
        turn("pay:6", 24, 10, NARA, "Keeps the discs from sweating in heat.", delivery="rehearsed"),
        turn("pay:7", 24, 20, SYLVI, "Amber sweats?", delivery="skeptical"),
        turn("pay:8", 24, 30, NARA, "Everything sweats here eventually.", delivery="closing"),
        turn("pay:9", 24, 40, RHEA, "What buys food east of the gate?", delivery="practical"),
        turn("pay:10", 24, 50, NARA, "Amber. Salt. Iron. Promises, once per customer.", delivery="dry"),
    ]
    pay_effects = party_move_effects("pay", AMBER_EXCHANGE)
    pay_effects.extend([
        effect("wages-clear-location", AMBER_WAGES, "location", "clear"),
        effect("wages-holder", AMBER_WAGES, "holder", "set", {"entity": RHEA}),
        effect("wages-open", AMBER_WAGES, "condition", "set", "opened and divided"),
    ])
    operations.extend([
        event_op(EV_PAID, "The Party Is Paid in Amber", 24, 55, AMBER_EXCHANGE, [(NARA, "pay-clerk"), *[(member, "guild-hunter") for member in PARTY]], "Nara issues the first wages as drilled amber discs and offers no explanation beyond their purchasing power.", causes=(EV_HIRED,), story_points=(SP_AMBER_PAY,), effects=pay_effects, tags=("amber", "currency")),
        conversation_op(CV_PAY, "Amber Wages", status="closed", start=t(23, 10), end=t(24, 50), scene=SC_PAY, location=AMBER_EXCHANGE, participants=[participant(NARA, "clerk", t(23, 10), t(24, 50)), *[participant(member, "new-hire", t(23, 10), t(24, 50)) for member in PARTY]], turns=pay_turns, topics=("amber", "wages", "frontier-custom"), body="The party learns that amber is both pay and a subject locals have practiced not explaining."),
        scene_record(SC_PAY, "Amber at the Pay Table", status="closed", start=t(23), current=t(25), end=t(25), location=AMBER_EXCHANGE, participants=[participant(NARA, "clerk", t(23), t(25)), *[participant(member, "new-hire", t(23), t(25), pov=(member == VEYRA)) for member in PARTY]], objects=(AMBER_WAGES, REFINED_AMBER), conversations=(CV_PAY,), observations=(
            observation("pay-liquid-gold", 23, 15, ("participants",), "Each amber disc holds a moving gold sheen that remains level when the disc tilts."),
            observation("pay-veyra-pressure", 24, 5, (VEYRA,), "The opened wage pouch creates a pressure in Veyra's teeth like the amber-capped dock pilings, only stronger.", salience=1.3),
            observation("pay-sylvi-smell", 24, 25, (SYLVI,), "The amber has no scent, but the clerk's pulse changes when Sylvi lifts a disc near the hearth brazier.", salience=1.1),
        ), constraints=("No party member yet knows why amber is damped with charcoal.",), body="The Frontier's currency arrives without a lesson, and the lack of explanation becomes its own warning.", tags=("amber", "wages")),
        story_update(SP_AMBER_PAY, [story_transition("amber-pay-resolved", 24, 55, "resolved", EV_PAID, "The party receives and can spend amber wages.")], outcomes=(EV_PAID,)),
    ])
    for index, character in enumerate(PARTY):
        operations.append(knowledge_record(eid("knowledge", f"{character}-amber-currency"), f"{CHARACTERS[character]['title']} knows amber is frontier currency", character, "amber.functions.as-currency", "Refined amber discs function as ordinary wages and exchange value across the Frontier.", 24, 56 + index, truth="true", confidence=0.98, acquisition="told", subject=REFINED_AMBER, predicate="functions-as", object_value={"text": "frontier currency"}, causing_event=EV_PAID, source_entity=NARA, tags=("amber", "currency")))
    operations.extend([
        knowledge_record(eid("knowledge", "veyra-amber-active-material"), "Veyra suspects amber is magically active", VEYRA, "amber.is.magically-active", "Amber is an active magical material rather than inert gemstone or resin.", 24, 65, state="suspected", truth="true", confidence=0.7, acquisition="observed", subject=REFINED_AMBER, predicate="is", object_value={"text": "magically active material"}, causing_event=EV_PAID, source_entity=AMBER_WAGES, tags=("amber", "magic")),
        recollection_op(CV_PAY, "pay-pip", PIP, 25, 0, "The guild pays in amber because the Frontier treats it as currency, and Nara called that local wages rather than money.", "The distinction benefits whoever controls the scales.", "amusement covering suspicion", 0.94, exact_turns=(pay_turns[3]["id"], pay_turns[4]["id"])),
        recollection_op(CV_PAY, "pay-veyra", VEYRA, 25, 1, "The discs carry active pressure and are packed with charcoal powder the clerk describes as anti-sweat handling.", "The powder suppresses a magical or physical reaction locals consider routine.", "focused curiosity", 0.88, exact_turns=(pay_turns[5]["id"], pay_turns[6]["id"])),
        recollection_op(CV_PAY, "pay-rhea", RHEA, 25, 2, "Amber buys food and passage east of the gate; the party cannot insist on mainland coin without making the contract useless.", "The currency is strange but currently a logistical fact rather than a mystery she can afford.", "practical reservation", 0.92, exact_turns=(pay_turns[9]["id"], pay_turns[10]["id"])),
    ])

    road_effects = party_move_effects("road-depart", COAST_ROAD, "on-caravan-duty")
    road_effects.extend([
        effect("maela-road", MAELA, "location", "set", {"entity": COAST_ROAD}),
        effect("wagon-road", CARAVAN_WAGON, "location", "set", {"entity": COAST_ROAD}),
    ])
    operations.extend([
        event_op(EV_DEPART, "The Eastroad Caravan Departs", 26, 10, EAST_GATE, [(MAELA, "caravan-master"), *[(member, "guard") for member in PARTY]], "Maela leads twelve wagons through Keldmouth's east gate and assigns the new company to rotate between lead, flank, and rear guard.", causes=(EV_HIRED, EV_PAID), story_points=(SP_CARAVAN,), effects=road_effects, tags=("caravan", "road")),
        event_op(EV_ROAD_GLIMMER, "A Fox-Shaped Glimmer Crosses the Road", 34, 20, COAST_ROAD, [(SYLVI, "witness"), (VEYRA, "witness"), (PIP, "partial-witness")], "A transparent fox-shaped figure runs from one milestone into another without crossing the open ground between them. It leaves no tracks, and the amber wage pouch is briefly warm.", causes=(EV_DEPART,), effects=[], tags=("wretch", "foreshadowing")),
        scene_record(SC_ROAD, "Eastroad Caravan", status="closed", start=t(26), current=t(38), end=t(38), location=COAST_ROAD, participants=[participant(MAELA, "caravan-master", t(26), t(38)), *[participant(member, "guard", t(26), t(38), pov=(member == SYLVI)) for member in PARTY]], objects=(CARAVAN_WAGON, AMBER_WAGES, *BADGES.values()), environments=(ENV_SLEET,), story_points=(SP_CARAVAN,), observations=(
            observation("road-spruce", 28, 0, ("participants",), "The road runs between black spruce and abrupt views of a cold iron sea."),
            observation("road-no-deer", 31, 0, (SYLVI,), "Deer tracks approach the road repeatedly and turn away before crossing the wagon ruts.", salience=1.15),
            observation("road-fox", 34, 20, (SYLVI, VEYRA, PIP), "A transparent fox-shape enters one stone milestone and exits the next without touching the road between them.", salience=1.35),
            observation("road-wages-warm", 34, 25, (RHEA, VEYRA), "The amber wage pouch warms for three breaths while the fox-shape is visible.", salience=1.3),
        ), constraints=("The apparition is a weak wretch manifestation but the party has no name for it.", "Maela does not see the fox clearly and dismisses the report."), body="The caravan travels east through sleet. The first manifestation is subtle enough to remain deniable.", tags=("caravan", "foreshadowing")),
        knowledge_record(eid("knowledge", "sylvi-fox-no-tracks"), "Sylvi knows the fox-shape left no tracks", SYLVI, "road.apparition.left-no-tracks", "The transparent fox-shape crossed between milestones without leaving tracks in sleet or mud.", 34, 30, truth="true", confidence=1.0, acquisition="observed", subject=EV_ROAD_GLIMMER, predicate="left", object_value={"text": "no physical tracks"}, causing_event=EV_ROAD_GLIMMER, source_entity=COAST_ROAD, tags=("wretch", "tracking")),
        knowledge_record(eid("knowledge", "veyra-amber-warmed-with-glimmer"), "Veyra links the warm wage pouch to the road apparition", VEYRA, "amber.warmed.during-apparition", "The party's amber wages became warm during the fox-shaped apparition.", 34, 35, truth="true", confidence=0.93, acquisition="observed", subject=AMBER_WAGES, predicate="warmed-during", object_value={"entity": EV_ROAD_GLIMMER}, causing_event=EV_ROAD_GLIMMER, source_entity=AMBER_WAGES, tags=("amber", "wretch")),
    ])

    camp_turns = [
        turn("camp:0", 40, 10, MAELA, "First rule east of Keldmouth: wagons make a circle before anyone makes tea.", delivery="routine"),
        turn("camp:1", 40, 20, RHEA, "Second rule?", delivery="cooperative"),
        turn("camp:2", 40, 30, MAELA, "If the woods go quiet, count people before horses.", delivery="flat"),
        turn("camp:3", 40, 40, SYLVI, "They have been quiet since the gate.", delivery="serious"),
        turn("camp:4", 40, 50, MAELA, "Then count us continuously and call it professionalism.", delivery="dry"),
        turn("camp:5", 41, 0, VEYRA, "The wage amber warmed when the fox appeared.", delivery="testing"),
        turn("camp:6", 41, 10, MAELA, "Amber warms in coats, sunlight, fever, kitchens, arguments, and stories told by new hires.", delivery="dismissive"),
        turn("camp:7", 41, 20, GARRAN, "That is an impressive number of explanations for a stone.", delivery="gentle"),
        turn("camp:8", 41, 30, PIP, "It is not a stone. Nara was very clear that it is money.", delivery="helpful"),
        turn("camp:9", 42, 0, RHEA, "Maela. If the thing returns, do we fight it?", delivery="direct"),
        turn("camp:10", 42, 10, MAELA, "If it can bite a horse, yes. If it can only frighten scholars, keep the caravan moving.", delivery="final"),
    ]
    operations.extend([
        conversation_op(CV_CAMP, "Saltmere Campfire", status="closed", start=t(40, 10), end=t(42, 10), scene=SC_CAMPIRE, location=SALTMERE, participants=[participant(MAELA, "caravan-master", t(40, 10), t(42, 10)), *[participant(member, "guard", t(40, 10), t(42, 10)) for member in PARTY]], turns=camp_turns, topics=("road", "amber", "apparition"), body="The party raises the road apparition with Maela and learns how aggressively experienced travelers normalize frontier warnings."),
        scene_record(SC_CAMPIRE, "Saltmere Campfire", status="closed", start=t(39), current=t(44), end=t(44), location=SALTMERE, participants=[participant(MAELA, "caravan-master", t(39), t(44)), *[participant(member, "guard", t(39), t(44), pov=(member == RHEA)) for member in PARTY]], objects=(CARAVAN_WAGON, AMBER_WAGES), environments=(ENV_COAST_DUSK,), story_points=(SP_CARAVAN,), conversations=(CV_CAMP,), observations=(
            observation("camp-no-scavengers", 39, 20, (SYLVI,), "No fox, raven, or mouse approaches the caravan scraps despite the long dusk."),
            observation("camp-amber-cooled", 41, 15, (VEYRA,), "The wage discs are cold again before Maela finishes listing ordinary reasons they might have warmed."),
            observation("camp-rhea-circle", 42, 15, (RHEA,), "Maela places the new company on the side of the wagon circle nearest the forest rather than the sea."),
        ), constraints=("Maela knows amber incidents are treated as road folklore, not the Blackroot Compact.",), body="At the first road camp, the party discovers that experienced Frontier travelers survive partly by refusing to connect their observations.", tags=("campfire", "party")),
        recollection_op(CV_CAMP, "camp-rhea", RHEA, 44, 0, "Maela has rules for unnatural quiet but refuses to connect amber warmth to the fox-shaped apparition.", "The caravan master knows enough to manage risk and not enough—or not willingly enough—to explain it.", "irritated caution", 0.84, exact_turns=(camp_turns[2]["id"], camp_turns[10]["id"])),
        recollection_op(CV_CAMP, "camp-veyra", VEYRA, 44, 1, "Maela produced many ordinary explanations for warm amber without testing any of them.", "The Frontier has social defenses against asking what amber does.", "intellectual frustration", 0.89, exact_turns=(camp_turns[5]["id"], camp_turns[6]["id"])),
        recollection_op(CV_CAMP, "camp-sylvi", SYLVI, 44, 2, "The woods have been quiet since Keldmouth and Maela treats that as something to count around rather than investigate.", "The road's animal absence is established danger, not Sylvi's private unease.", "vindication without comfort", 0.92, exact_turns=(camp_turns[2]["id"], camp_turns[4]["id"])),
    ])

    last_turns = [
        turn("lastwatch:0", 47, 10, ASHA, "Bar the east door. The amber chest is humming again.", delivery="command"),
        turn("lastwatch:1", 47, 20, BRAN, "It is a box, Corporal. Boxes do not hum.", delivery="joking"),
        turn("lastwatch:2", 47, 30, KELLAN, "This one does.", delivery="quiet"),
        turn("lastwatch:3", 47, 40, NOLLY, "Something is scratching under the floor.", delivery="listening"),
        turn("lastwatch:4", 47, 50, ASHA, "Nobody opens the door. Bran, move the chest away from the bunks.", delivery="controlled"),
        turn("lastwatch:5", 48, 0, BRAN, "Bottle broke. Give me a cloth.", delivery="annoyed"),
        turn("lastwatch:6", 48, 5, KELLAN, "Your blood is running toward the chest.", delivery="alarmed"),
        turn("lastwatch:7", 48, 10, NOLLY, "Asha, there is a deer in the wall.", delivery="disbelieving"),
        turn("lastwatch:8", 48, 12, ASHA, "Spear line. Do not look at the floor.", delivery="command"),
        turn("lastwatch:9", 48, 14, NOLLY, "It has Bran's teeth.", delivery="terrified"),
    ]
    slaughter_effects = [
        effect("massacre-asha", ASHA, "condition", "set", "dead"),
        effect("massacre-bran", BRAN, "condition", "set", "dead"),
        effect("massacre-nolly", NOLLY, "condition", "set", "dead"),
        effect("massacre-kellan", KELLAN, "condition", "set", "wounded and traumatized"),
        effect("massacre-chest", AMBER_CHEST, "condition", "set", "quickened and cracked"),
    ]
    operations.extend([
        conversation_op(CV_LAST_WATCH, "The Last Watch", status="closed", start=t(47, 10), end=t(48, 14), scene=None, location=BARRACKS, participants=[participant(ASHA, "corporal", t(47, 10), t(48, 14)), participant(BRAN, "watchman", t(47, 10), t(48, 14)), participant(NOLLY, "watchman", t(47, 10), t(48, 14)), participant(KELLAN, "watchman", t(47, 10), t(48, 14))], turns=last_turns, topics=("massacre", "amber", "wretch"), body="The canonical final words of the Harrowcross watch. Only Kellan survives to remember them."),
        event_op(EV_LAST_WATCH, "The Harrowcross Watch Hears the Amber Chest", 47, 55, BARRACKS, [(ASHA, "corporal"), (BRAN, "watchman"), (NOLLY, "watchman"), (KELLAN, "watchman")], "The barred watch notices the amber payment chest humming and scratching beneath the floor before Bran cuts his palm on a bottle.", tags=("harrowcross", "history")),
        event_op(EV_GUARD_MASSACRE, "The Harrowcross Guard Detail Is Torn Apart", 48, 20, BARRACKS, [(ASHA, "defender"), (BRAN, "catalyst"), (NOLLY, "defender"), (KELLAN, "survivor")], "Blood reaches the cracked amber chest. Antlered and wolf-like wretches emerge through floor and walls, repeating the guards' voices while killing three of them. Kellan survives beneath a collapsed bunk.", causes=(EV_LAST_WATCH,), story_points=(SP_LAST_WATCH, SP_AMBER_LINK), effects=slaughter_effects, tags=("massacre", "wretch", "amber")),
    ])

    arrival_effects = party_move_effects("harrow-arrival", HARROWCROSS, "arrived-at-harrowcross")
    arrival_effects.extend([
        effect("maela-harrow", MAELA, "location", "set", {"entity": HARROWCROSS}),
        effect("wagon-harrow", CARAVAN_WAGON, "location", "set", {"entity": HARROWCROSS}),
        effect("edrik-harrow", EDRIK, "location", "set", {"entity": HARROWCROSS}),
        effect("kellan-harrow", KELLAN, "location", "set", {"entity": HARROWCROSS}),
        effect("hessa-harrow", HESSA, "location", "set", {"entity": HARROWCROSS}),
    ])
    sergeant_turns = [
        turn("sergeant:0", 52, 10, EDRIK, "The east watch was torn apart overnight.", delivery="formal"),
        turn("sergeant:1", 52, 20, RHEA, "By what?", delivery="direct"),
        turn("sergeant:2", 52, 30, EDRIK, "If I knew, I would have named it before I asked a caravan for hunters.", delivery="strained"),
        turn("sergeant:3", 52, 40, MAELA, "My bond carries freight east. It does not replace village watchmen.", delivery="contractual"),
        turn("sergeant:4", 53, 0, HESSA, "Harrowcross has paid the Lantern Pike through winter. The badges standing here are what arrived.", delivery="plain"),
        turn("sergeant:5", 53, 10, KELLAN, "The door never opened.", delivery="shaking"),
    ]
    operations.extend([
        event_op(EV_CARAVAN_ARRIVES, "The Caravan Reaches Harrowcross", 50, 20, HARROWCROSS, [(MAELA, "caravan-master"), (EDRIK, "sergeant"), (HESSA, "reeve"), *[(member, "caravan-guard") for member in PARTY]], "The caravan enters a town with mourning cord around the gate and no watch detail standing inspection.", causes=(EV_DEPART, EV_GUARD_MASSACRE), story_points=(SP_REACH, SP_REPLACE), effects=arrival_effects, tags=("harrowcross", "arrival")),
        conversation_op(CV_SERGEANT, "The Sergeant's Account", status="active", start=t(52, 10), end=None, scene=SC_HARROW, location=HARROWCROSS, participants=[participant(EDRIK, "sergeant", t(52, 10)), participant(MAELA, "caravan-master", t(52, 10)), participant(HESSA, "reeve", t(52, 10)), participant(KELLAN, "survivor", t(53, 10)), *[participant(member, "guild-hunter", t(52, 10)) for member in PARTY]], turns=sergeant_turns, topics=("massacre", "assignment", "harrowcross"), body="Edrik opens his report while Maela and Hessa argue over who inherits responsibility for the undefended town."),
        scene_record(SC_HARROW, "Harrowcross at Dusk", status="active", start=t(50), current=t(55), end=None, location=HARROWCROSS, participants=[participant(MAELA, "caravan-master", t(50)), participant(EDRIK, "sergeant", t(50)), participant(HESSA, "reeve", t(50)), participant(KELLAN, "survivor", t(53, 10)), *[participant(member, "guild-hunter", t(50), pov=(member == RHEA)) for member in PARTY]], objects=(CARAVAN_WAGON, AMBER_WAGES, *BADGES.values()), environments=(ENV_MOURNING,), story_points=(SP_REACH, SP_REPLACE, SP_LAST_WATCH), conversations=(CV_SERGEANT,), observations=(
            observation("harrow-black-cord", 50, 0, ("participants",), "Black mourning cord is tied around the east gate posts and every watch bell has been wrapped in cloth."),
            observation("harrow-no-watch", 50, 10, (RHEA,), "No armed guard meets the caravan, although the gate stands open."),
            observation("harrow-kellan", 53, 10, ("participants",), "Kellan's bandaged hands tremble hardest when Maela's wagon amber passes near him."),
            observation("harrow-maela-count", 54, 0, (PIP,), "Maela counts wagons twice while Edrik speaks and never looks toward the barracks."),
        ), constraints=("The full massacre mechanism remains unknown to the party.", "The conversation is intentionally active at the act boundary so later turns and recollections can be appended through wedl."), body="At the crossroads town, the caravan meets a catastrophe large enough to activate the guild contract's emergency clause.", tags=("harrowcross", "active")),
        story_update(SP_REACH, [story_transition("reach-resolved", 50, 20, "resolved", EV_CARAVAN_ARRIVES, "The caravan reaches Harrowcross.")], outcomes=(EV_CARAVAN_ARRIVES,)),
    ])
    operations.extend([
        relationship_update(RHEA, SYLVI, [relationship_transition("road-rhea-sylvi", 44, 0, {"trust": 0.38, "affinity": 0.1, "fear": 0.0, "obligation": 0.04, "respect": 0.48, "dependence": 0.12}, ["trusted-tracker", "independent"], "Sylvi's warning about animal silence is supported by the road apparition.", causing_event=EV_ROAD_GLIMMER)]),
        relationship_update(SYLVI, RHEA, [relationship_transition("road-sylvi-rhea", 44, 1, {"trust": 0.28, "affinity": 0.06, "fear": 0.0, "obligation": 0.0, "respect": 0.4, "dependence": 0.08}, ["field-leader", "listens-to-evidence"], "Rhea does not dismiss the apparition even when Maela does.", causing_event=EV_ROAD_GLIMMER)]),
        relationship_update(RHEA, VEYRA, [relationship_transition("road-rhea-veyra", 44, 2, {"trust": 0.1, "affinity": 0.02, "fear": 0.14, "obligation": 0.02, "respect": 0.3, "dependence": 0.1}, ["useful-arcanist", "risk-of-fascination"], "Veyra notices a connection between the apparition and amber warmth.", causing_event=EV_ROAD_GLIMMER)]),
        relationship_update(VEYRA, SYLVI, [relationship_transition("road-veyra-sylvi", 44, 3, {"trust": 0.2, "affinity": 0.08, "fear": 0.0, "obligation": 0.0, "respect": 0.44, "dependence": 0.1}, ["empirical-rival", "corroborating-witness"], "Sylvi's tracking observation gives Veyra's magical hypothesis a physical check.", causing_event=EV_ROAD_GLIMMER)]),
        relationship_update(PIP, RHEA, [relationship_transition("road-pip-rhea", 44, 4, {"trust": 0.38, "affinity": 0.22, "fear": 0.02, "obligation": 0.04, "respect": 0.34, "dependence": 0.1}, ["accepted-leader", "still-teasable"], "Rhea asks for Pip's map corrections rather than treating him as baggage.", causing_event=EV_DEPART)]),
    ])
    return operations


def act2_operations() -> list[dict[str, Any]]:
    """Harrowcross investigation: abandonment, first wretch, dowser, and mine trail."""
    operations: list[dict[str, Any]] = []

    # Finish the active sergeant interview through append operations so the
    # provenance history records the act boundary rather than replacing it.
    sergeant_more = [
        turn("sergeant:6", 53, 20, GARRAN, "Show me the survivor's wounds before the room.", delivery="gentle"),
        turn("sergeant:7", 53, 30, KELLAN, "The cold is inside them. I keep dreaming my hands belong to the thing that bit Bran.", delivery="fragmented"),
        turn("sergeant:8", 53, 40, VEYRA, "What was stored beneath the bunks?", delivery="precise"),
        turn("sergeant:9", 53, 50, EDRIK, "Quarterly guild payment. Amber from Saint Orra, raw and struck.", delivery="reluctant"),
        turn("sergeant:10", 54, 0, SYLVI, "Any animal tracks around the barracks?", delivery="quiet"),
        turn("sergeant:11", 54, 10, EDRIK, "None. Frost on the inner walls. Splinters pointing into the room.", delivery="controlled"),
        turn("sergeant:12", 54, 20, PIP, "So the door stayed barred, the walls were not broken, and the spears snapped inward.", delivery="technical"),
        turn("sergeant:13", 54, 30, HESSA, "That is why the town paid for monster hunters before the caravan ever saw our gate.", delivery="plain"),
        turn("sergeant:14", 55, 0, MAELA, "And that is why the emergency clause now applies.", delivery="final"),
    ]
    for value in sergeant_more:
        operations.append({"type": "conversation.turn.append", "conversationId": CV_SERGEANT, "turn": value})
    operations.append({
        "type": "entity.update",
        "entityId": CV_SERGEANT,
        "frontmatterPatch": {"status": "closed", "time": {"start": t(52, 10), "end": t(55)}},
    })
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "The guards died inside a barred room containing the town's amber payment; Maela intends to invoke emergency reassignment.", "The assignment was decided before the caravan reached the gate.", "anger becoming duty", 0.9, (sergeant_more[7]["id"], sergeant_more[8]["id"])),
        (SYLVI, "The barracks has no animal approach tracks, frost on its inner walls, and debris driven inward.", "Whatever killed the watch did not enter like a living predator.", "cold certainty", 0.96, (sergeant_more[4]["id"], sergeant_more[5]["id"])),
        (GARRAN, "Kellan experiences cold inside wounds made by something that borrowed familiar anatomy.", "The killer may be a suffering impression rather than an ordinary beast or ghost.", "compassion under dread", 0.82, (sergeant_more[0]["id"], sergeant_more[1]["id"])),
        (VEYRA, "The slaughter room contained mixed raw and refined amber stored beneath the bunks.", "The chest is a material variable the locals are reluctant to discuss.", "focused suspicion", 0.93, (sergeant_more[2]["id"], sergeant_more[3]["id"])),
        (PIP, "The physical evidence points inward: no opened door, no breached wall, and weapons broken toward the center.", "Locks and walls did not fail; the threat disregarded them.", "professional unease", 0.91, (sergeant_more[6]["id"],)),
        (EDRIK, "The new hunters immediately separated wounds, tracks, stored amber, and structural evidence instead of asking for a monster name.", "They may survive long enough to discover what his old report missed.", "relief mixed with shame", 0.76, (sergeant_more[0]["id"], sergeant_more[6]["id"])),
        (KELLAN, "The strangers believe the barred room matters and did not call his account cowardice.", "Garran may understand that the cold remains in him.", "fragile relief", 0.7, (sergeant_more[0]["id"],)),
    ]):
        operations.append(recollection_op(CV_SERGEANT, f"sergeant-act2-{index}", character, 55, 10 + index, summary, interpretation, emotional, confidence, exact_turns=exact))

    operations.append(scene_record(
        SC_HARROW,
        "Harrowcross at Dusk",
        status="closed",
        start=t(50), current=t(55), end=t(55), location=HARROWCROSS,
        participants=[
            participant(MAELA, "caravan-master", t(50), t(55)),
            participant(EDRIK, "sergeant", t(50), t(55)),
            participant(HESSA, "reeve", t(50), t(55)),
            participant(KELLAN, "survivor", t(53, 10), t(55)),
            *[participant(member, "guild-hunter", t(50), t(55), pov=(member == RHEA)) for member in PARTY],
        ],
        objects=(CARAVAN_WAGON, AMBER_WAGES, *BADGES.values()), environments=(ENV_MOURNING,),
        story_points=(SP_REACH, SP_REPLACE, SP_LAST_WATCH), conversations=(CV_SERGEANT,),
        observations=(
            observation("harrow-black-cord", 50, 0, ("participants",), "Black mourning cord is tied around the east gate posts and every watch bell has been wrapped in cloth."),
            observation("harrow-no-watch", 50, 10, (RHEA,), "No armed guard meets the caravan, although the gate stands open."),
            observation("harrow-kellan", 53, 10, ("participants",), "Kellan's bandaged hands tremble hardest when Maela's wagon amber passes near him."),
            observation("harrow-maela-count", 54, 0, (PIP,), "Maela counts wagons twice while Edrik speaks and never looks toward the barracks."),
            observation("harrow-inward", 54, 25, (PIP, SYLVI), "The barracks shutter splinters lie inside the room despite no opening large enough for an animal."),
        ),
        constraints=("The party knows the amber chest was present but not yet that blood quickened it.",),
        body="The first report gives the party a crime scene without a physical entrance and a contract clause Maela is ready to use.",
        tags=("harrowcross", "closed"),
    ))

    # The caravan's departure is deliberately abrupt and socially ugly.
    departure_turns = [
        turn("departure:0", 56, 0, MAELA, "Lantern Pike badges remain in Harrowcross. The caravan leaves at the next bell.", delivery="announcing"),
        turn("departure:1", 56, 10, RHEA, "We were hired for twelve wagons.", delivery="controlled"),
        turn("departure:2", 56, 20, MAELA, "You were hired under a guild bond. The bond transfers you to a paid settlement emergency.", delivery="contractual"),
        turn("departure:3", 56, 30, PIP, "Paid in what? The reeve did not put silver on your table.", delivery="too-casual"),
        turn("departure:4", 56, 40, HESSA, "Amber. Three measures from Saint Orra each quarter, plus stamped discs for expenses.", delivery="plain"),
        turn("departure:5", 56, 50, VEYRA, "Raw amber is a protection fee?", delivery="sharp"),
        turn("departure:6", 57, 0, HESSA, "Amber buys most things here. Protection is only the thing we need most often.", delivery="tired"),
        turn("departure:7", 57, 10, GARRAN, "And if we refuse?", delivery="quiet"),
        turn("departure:8", 57, 20, MAELA, "Your badges, first wages, and return passage are forfeit. More urgently, the east road loses a village tonight.", delivery="hard"),
        turn("departure:9", 57, 30, SYLVI, "You knew before we arrived that you would leave us.", delivery="certain"),
        turn("departure:10", 57, 40, MAELA, "I knew the town had sent a red request. I did not know what had answered it.", delivery="honest at last"),
        turn("departure:11", 58, 0, RHEA, "Go, then. Leave the spare lamp oil and every map east of here.", delivery="command"),
        turn("departure:12", 58, 10, MAELA, "Done. Try to be alive when I come back west.", delivery="almost apologetic"),
    ]
    operations.extend([
        conversation_op(CV_DEPARTURE, "Maela's Departure", status="closed", start=t(56), end=t(58, 10), scene=SC_LEFT, location=HARROWCROSS,
            participants=[participant(MAELA, "caravan-master", t(56), t(58, 10)), participant(HESSA, "reeve", t(56), t(58, 10)), *[participant(member, "reassigned-hunter", t(56), t(58, 10)) for member in PARTY]],
            turns=departure_turns, topics=("contract", "abandonment", "amber-payment"),
            body="The caravan invokes the emergency clause, reveals that Harrowcross pays Lantern Pike in amber, and leaves the new company behind."),
        event_op(EV_ASSIGNED, "The Party Is Reassigned to Harrowcross", 57, 25, HARROWCROSS,
            [(MAELA, "assigning-caravan-master"), (HESSA, "contracting-reeve"), *[(member, "reassigned-hunter") for member in PARTY]],
            "Maela invokes the emergency clause and Hessa accepts the five badges as Harrowcross's replacement watch and investigation detail.",
            causes=(EV_CARAVAN_ARRIVES,), story_points=(SP_REPLACE, SP_CARAVAN, SP_AMBER_PAY),
            effects=[*party_move_effects("assignment", HARROWCROSS, "stationed-at-harrowcross")], tags=("assignment", "harrowcross")),
        event_op(EV_CARAVAN_LEAVES, "The Eastroad Caravan Leaves the Frontiersmen", 58, 20, HARROWCROSS,
            [(MAELA, "departing-caravan-master"), *[(member, "abandoned-hunter") for member in PARTY]],
            "The wagons roll east without ceremony. Maela leaves lamp oil and her annotated road map, but takes the reserve guards and every experienced teamster.",
            causes=(EV_ASSIGNED,), story_points=(SP_CARAVAN, SP_REPLACE), effects=[
                effect("leave-maela", MAELA, "location", "set", {"entity": COAST_ROAD}),
                effect("leave-wagon", CARAVAN_WAGON, "location", "set", {"entity": COAST_ROAD}),
            ], tags=("abandonment", "caravan")),
        scene_record(SC_LEFT, "Left at Harrowcross", status="closed", start=t(56), current=t(59), end=t(59), location=HARROWCROSS,
            participants=[participant(MAELA, "caravan-master", t(56), t(58, 20)), participant(HESSA, "reeve", t(56), t(59)), *[participant(member, "reassigned-hunter", t(56), t(59), pov=(member == RHEA)) for member in PARTY]],
            objects=(CONTRACT, AMBER_WAGES, *BADGES.values()), environments=(ENV_MOURNING,),
            story_points=(SP_CARAVAN, SP_REPLACE, SP_AMBER_PAY), conversations=(CV_DEPARTURE,),
            observations=(
                observation("departure-wheels", 58, 20, ("participants",), "The wagon bells diminish eastward while the town's wrapped watch bells remain silent."),
                observation("departure-rhea-map", 58, 25, (RHEA,), "Maela's road map has Saint Orra Mine circled twice in different inks."),
                observation("departure-pip-ledger", 56, 45, (PIP,), "Hessa's amber tally uses weight marks where mainland contracts would use coin columns."),
            ),
            constraints=("The party now knows amber pays for guild service, but no one has explained extraction or magical risk.",),
            body="The contractual transition is made personal: the old caravan leaves, the new company inherits a frightened town, and amber becomes a debt rather than a curiosity.", tags=("abandonment",)),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "Maela transferred the party under a valid emergency clause and admitted Harrowcross pays in raw amber.", "The contract is coercive, but leaving the town undefended would be worse than accepting it.", "fury disciplined into command", 0.95, (departure_turns[2]["id"], departure_turns[8]["id"])),
        (SYLVI, "Maela knew a red request waited at Harrowcross and brought inexperienced hires anyway.", "The guild uses the road to move replaceable bodies toward danger it has already measured.", "betrayal", 0.88, (departure_turns[9]["id"], departure_turns[10]["id"])),
        (GARRAN, "Harrowcross pays the guild in amber because protection is its most urgent recurring need.", "The settlement's danger and its economy may be the same wound.", "moral unease", 0.8, (departure_turns[4]["id"], departure_turns[6]["id"])),
        (VEYRA, "Raw amber is transferred directly from Saint Orra Mine to the guild as payment.", "The guild has institutional access to the material it refuses to explain.", "vindicated suspicion", 0.94, (departure_turns[4]["id"], departure_turns[5]["id"])),
        (PIP, "Hessa's ledger measures protection fees by amber weight and Maela was prepared to invoke reassignment.", "The Frontier's contracts convert local danger into guild-controlled material flow.", "admiration for the trap", 0.9, (departure_turns[3]["id"], departure_turns[8]["id"])),
    ]):
        operations.append(recollection_op(CV_DEPARTURE, f"departure-{index}", character, 59, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    # Reconstructing the barracks repeats the original trigger under witnesses.
    kellan_turns = [
        turn("kellan:0", 61, 0, KELLAN, "Asha heard the chest humming before Bran cut his hand.", delivery="halting"),
        turn("kellan:1", 61, 10, VEYRA, "The chest was already warm?", delivery="precise"),
        turn("kellan:2", 61, 20, KELLAN, "Warm enough that the snow beneath it melted. Then Bran's blood ran uphill toward the crack.", delivery="shaking"),
        turn("kellan:3", 61, 30, GARRAN, "What did the first shape resemble?", delivery="gentle"),
        turn("kellan:4", 61, 40, KELLAN, "A deer until it turned its head. Then it had Bran's teeth and Asha's voice.", delivery="distant"),
        turn("kellan:5", 61, 50, SYLVI, "Did it leave a print? Hair? Blood?", delivery="quiet"),
        turn("kellan:6", 62, 0, KELLAN, "Only cold. It went through the wall when Asha struck it.", delivery="certain"),
        turn("kellan:7", 62, 10, PIP, "And the wall remained a wall.", delivery="confirming"),
        turn("kellan:8", 62, 20, RHEA, "We move the chest into the open yard and keep every blade covered.", delivery="command"),
        turn("kellan:9", 62, 30, EDRIK, "The guild collector forbade moving it before tally.", delivery="ashamed"),
        turn("kellan:10", 62, 40, RHEA, "The collector is not here.", delivery="final"),
    ]
    operations.extend([
        conversation_op(CV_KELLAN, "Kellan's Broken Watch", status="closed", start=t(61), end=t(62, 40), scene=SC_WRETCH, location=BARRACKS,
            participants=[participant(KELLAN, "surviving-watchman", t(61), t(62, 40)), participant(EDRIK, "sergeant", t(61), t(62, 40)), *[participant(member, "investigator", t(61), t(62, 40)) for member in PARTY]],
            turns=kellan_turns, topics=("massacre", "amber", "wretch"), body="Kellan supplies the causal sequence the dead guards could not: warm amber, blood moving toward it, and a wretch borrowing familiar anatomy and voices."),
        recollection_op(CV_LAST_WATCH, "kellan-last-watch", KELLAN, 62, 45,
            "Asha ordered a spear line after Bran's blood moved toward the humming amber chest. The first wretch wore a deer's body, Bran's teeth, and Asha's voice.",
            "Kellan believes the chest called the thing and that his own survival was accidental rather than brave.", "survivor's terror", 0.86,
            exact_turns=(aid("conversation-turn", "lastwatch:0"), aid("conversation-turn", "lastwatch:6"), aid("conversation-turn", "lastwatch:9"))),
    ])

    quicken_effects = [
        effect("quicken-chest", AMBER_CHEST, "condition", "set", "bright, humming, and blood-responsive"),
        effect("quicken-kellan", KELLAN, "condition", "set", "wound reopened"),
        *party_move_effects("barracks-party", BARRACKS, "under-attack"),
        effect("barracks-edrik", EDRIK, "location", "set", {"entity": BARRACKS}),
    ]
    first_wretch_effects = [
        effect("wretch-garran", GARRAN, "condition", "set", "frost-bitten left arm"),
        effect("wretch-spear-clear", BROKEN_SPEAR, "location", "clear"),
        effect("wretch-spear", BROKEN_SPEAR, "holder", "set", {"entity": RHEA}),
        effect("wretch-residue-clear", WRETCH_RESIDUE, "location", "clear"),
        effect("wretch-residue", WRETCH_RESIDUE, "holder", "set", {"entity": VEYRA}),
    ]
    operations.extend([
        event_op(EV_AMBER_QUICKENS, "The Harrowcross Amber Chest Quickens Again", 63, 10, BARRACKS,
            [(KELLAN, "bleeding-survivor"), (VEYRA, "observer"), *[(member, "investigator") for member in PARTY]],
            "While the company prepares to move the chest, Kellan's bandage opens. A thread of blood creeps against the floor's slope and disappears into the cracked seam. The amber brightens from within and repeats the watch bell's muffled note.",
            causes=(EV_GUARD_MASSACRE, EV_ASSIGNED), story_points=(SP_FIRST_WRETCH, SP_AMBER_LINK), effects=quicken_effects, tags=("amber", "quickening")),
        event_op(EV_FIRST_WRETCH, "A Wretch Comes Through the Barracks Wall", 63, 20, BARRACKS,
            [(RHEA, "shield"), (SYLVI, "archer"), (GARRAN, "warder"), (VEYRA, "arcanist"), (PIP, "flanker"), (KELLAN, "witness")],
            "An antlered, dog-limbed shape steps through the north wall without disturbing plaster. Its mouth alternates between Asha's command voice and Bran's laugh. Iron passes through it until Garran's threshold blessing and Veyra's heat force it partly solid.",
            causes=(EV_AMBER_QUICKENS,), story_points=(SP_FIRST_WRETCH, SP_AMBER_LINK), effects=first_wretch_effects, tags=("wretch", "combat")),
        event_op(EV_WRETCH_DRIVEN, "The Frontiersmen Drive Off Their First Wretch", 64, 10, BARRACKS,
            [(RHEA, "shield"), (SYLVI, "archer"), (GARRAN, "warder"), (VEYRA, "arcanist"), (PIP, "flanker")],
            "Rhea pins the half-solid creature against the amber chest with Asha's broken spear while Sylvi severs the antler-shadow from its body. Pip rolls the chest into the yard and Veyra smothers the crack under wet charcoal. The wretch collapses into frost and runs as a pale outline through the ground toward the east orchard.",
            causes=(EV_FIRST_WRETCH,), story_points=(SP_FIRST_WRETCH, SP_AMBER_LINK), effects=[
                effect("driven-party-ready", RHEA, "condition", "set", "bruised but steady"),
                effect("driven-sylvi", SYLVI, "condition", "set", "ready"),
                effect("driven-veyra", VEYRA, "condition", "set", "excited and shaken"),
                effect("driven-pip", PIP, "condition", "set", "ready"),
                effect("driven-chest", AMBER_CHEST, "location", "set", {"entity": SOUTH_FIELDS}),
                effect("driven-chest-state", AMBER_CHEST, "condition", "set", "charcoal-damped but warm"),
            ], tags=("wretch", "victory")),
        scene_record(SC_WRETCH, "First Wretch Night", status="closed", start=t(61), current=t(68), end=t(68), location=BARRACKS,
            participants=[participant(EDRIK, "sergeant", t(61), t(64, 20)), participant(KELLAN, "survivor", t(61), t(64, 20)), *[participant(member, "guild-hunter", t(61), t(68), pov=(member == RHEA)) for member in PARTY]],
            objects=(AMBER_CHEST, BROKEN_SPEAR, WRETCH_RESIDUE, *BADGES.values()), environments=(ENV_MOURNING, ENV_QUICKENING),
            story_points=(SP_LAST_WATCH, SP_FIRST_WRETCH, SP_AMBER_LINK), conversations=(CV_KELLAN,),
            observations=(
                observation("wretch-blood-uphill", 63, 10, ("participants",), "Kellan's blood crawls uphill in a narrow thread toward the cracked amber chest.", salience=1.5),
                observation("wretch-wall", 63, 20, ("participants",), "An antlered body emerges through intact plaster as though the wall were fog.", salience=1.5),
                observation("wretch-rhea-voice", 63, 30, (RHEA,), "The creature says 'hold the line' in the dead corporal's exact command cadence.", salience=1.35),
                observation("wretch-sylvi-no-weight", 63, 35, (SYLVI,), "The floor dust bends away from the creature but never carries its weight.", salience=1.3),
                observation("wretch-garran-pain", 63, 40, (GARRAN,), "The wretch recoils from the threshold prayer as if remembering an older wound.", salience=1.3),
                observation("wretch-veyra-chest", 63, 45, (VEYRA,), "Every change in the creature's shape follows a pulse from the chest's raw amber.", salience=1.45),
                observation("wretch-pip-ground", 64, 10, (PIP,), "When the chest leaves the room, the wretch's pale outline flees east through the packed ground.", salience=1.35),
            ),
            constraints=("The party has witnessed correlation, not yet learned the deeper nature of amber.", "The dead guards' voices are exact stored impressions, not conscious ghosts."),
            body="The story's first direct monster encounter proves that walls and conventional defenses do not define the threat. The party survives by treating the amber chest as part of the battlefield.", tags=("wretch", "combat")),
    ])
    for index, (character, title, key, statement, state, confidence, acquisition, subject, predicate, obj, source) in enumerate([
        (RHEA, "Rhea links blood, amber, and the wretch", "amber.blood.precedes-wretch", "Kellan's blood moved toward the cracked amber immediately before the wretch emerged.", "accepted", 0.96, "observed", AMBER_CHEST, "quickened-after", {"text": "Kellan's blood"}, AMBER_CHEST),
        (SYLVI, "Sylvi knows the wretch carried no weight", "wretch.leaves-no-weight-track", "The wretch disturbed dust and air but never bore weight on the barracks floor.", "accepted", 0.98, "observed", EV_FIRST_WRETCH, "left", {"text": "no weight-bearing track"}, WRETCH_RESIDUE),
        (GARRAN, "Garran suspects the wretch remembers pain", "wretch.responds-as-memory", "The wretch reacted to a threshold blessing as if remembering an older injury rather than suffering a new one.", "suspected", 0.7, "inferred", EV_FIRST_WRETCH, "behaves-like", {"text": "a remembered wound"}, BROKEN_SPEAR),
        (VEYRA, "Veyra observes amber pulses shaping the wretch", "amber.pulses-shape-wretch", "Pulses from the raw amber chest corresponded to the wretch's changing borrowed anatomy.", "accepted", 0.94, "observed", AMBER_CHEST, "shaped", {"entity": EV_FIRST_WRETCH}, WRETCH_RESIDUE),
        (PIP, "Pip knows removing the chest weakened the wretch", "moving-amber-weakened-wretch", "Moving the quickened amber chest out of the barracks weakened the wretch and drove its outline east through the ground.", "accepted", 0.92, "observed", AMBER_CHEST, "proximity-strengthened", {"entity": EV_FIRST_WRETCH}, AMBER_CHEST),
    ]):
        operations.append(knowledge_record(eid("knowledge", f"act2-{index}-{key}"), title, character, key, statement, 64, 20 + index, state=state, confidence=confidence, acquisition=acquisition, truth="true" if state == "accepted" else "unknown", subject=subject, predicate=predicate, object_value=obj, causing_event=EV_WRETCH_DRIVEN, source_entity=source, tags=("wretch", "amber")))

    # Orchard pursuit and Tillo's partial explanation.
    orchard_turns = [
        turn("tillo:0", 71, 0, TILLO, "Do not bring that chest another step toward my fork.", delivery="alarmed"),
        turn("tillo:1", 71, 10, VEYRA, "You can feel it before you see it.", delivery="interested"),
        turn("tillo:2", 71, 20, TILLO, "I can dowse for shallow amber. This is not shallow. This is quick.", delivery="correcting"),
        turn("tillo:3", 71, 30, RHEA, "Define quick.", delivery="command"),
        turn("tillo:4", 71, 40, TILLO, "Warm without fire. Pulls the fork backward. Makes old shapes where no animal stands.", delivery="careful"),
        turn("tillo:5", 71, 50, GARRAN, "Old shapes of the dead?", delivery="testing"),
        turn("tillo:6", 72, 0, TILLO, "Old shapes. Do not put a soul in the answer because you want one there.", delivery="blunt"),
        turn("tillo:7", 72, 10, SYLVI, "The outline fled toward the orchard and the mine road.", delivery="quiet"),
        turn("tillo:8", 72, 20, TILLO, "Then Saint Orra has opened something it should have left sleeping.", delivery="grim"),
        turn("tillo:9", 72, 30, PIP, "And Harrowcross pays the guild with what the mine wakes.", delivery="flat"),
        turn("tillo:10", 72, 40, TILLO, "Yes. That is the Frontier's cleverest circle.", delivery="bitter"),
    ]
    operations.extend([
        event_op(EV_ORCHARD_ATTACK, "A Second Wretch Hunts the East Orchard", 70, 10, EAST_ORCHARD,
            [(SENNA, "orchard-keeper"), (SYLVI, "tracker"), (RHEA, "shield"), (GARRAN, "warder"), (VEYRA, "arcanist"), (PIP, "scout")],
            "At dawn the party follows frost-rime through the ground to Senna Reed's orchard. A boar-shaped wretch rises beneath a grafted apple tree and repeats the squeal of a pig slaughtered there years earlier. Charcoal-damped amber on Rhea's cloak remains quiet until the creature strikes the tree's fresh sap wound.",
            causes=(EV_WRETCH_DRIVEN,), story_points=(SP_AMBER_LINK, SP_MINE), effects=[*party_move_effects("orchard-party", EAST_ORCHARD, "tracking-wretch"), effect("orchard-senna", SENNA, "location", "set", {"entity": EAST_ORCHARD})], tags=("wretch", "orchard")),
        event_op(EV_TILLO_DOWSES, "Tillo Dowses the Quickenings Toward Saint Orra", 72, 45, EAST_ORCHARD,
            [(TILLO, "dowser"), *[(member, "witness") for member in PARTY]],
            "Tillo's copper-bound fork twists away from the damped chest, then pulls east toward the mine road. He identifies the chest as quickened and admits that gnome dowsers locate amber pockets by the pressure they exert on tuned wood and metal.",
            causes=(EV_ORCHARD_ATTACK, EV_AMBER_QUICKENS), story_points=(SP_AMBER_LINK, SP_MINE, SP_WHAT_AMBER), effects=[effect("tillo-orchard", TILLO, "location", "set", {"entity": EAST_ORCHARD})], tags=("dowsing", "amber")),
        event_op(EV_AMBER_PAYMENT, "Harrowcross Opens Its Amber Protection Ledger", 74, 0, HARROWCROSS,
            [(HESSA, "reeve"), (EDRIK, "sergeant"), *[(member, "guild-hunter") for member in PARTY]],
            "Hessa produces the town ledger: Saint Orra owes Harrowcross a production share, and Harrowcross transfers the first three measures each quarter to Lantern Pike in exchange for a permanent watch and emergency hunting. The most recent chest mixed raw nodules with struck discs against Tillo's handling rules.",
            causes=(EV_ASSIGNED, EV_TILLO_DOWSES), story_points=(SP_AMBER_PAY, SP_GUILD_SECRET, SP_WHAT_AMBER), effects=[*party_move_effects("ledger-party", HARROWCROSS, "investigating-amber-system")], tags=("ledger", "amber-payment")),
        conversation_op(CV_TILLO, "Tillo and the Fork", status="closed", start=t(71), end=t(72, 40), scene=SC_ORCHARD, location=EAST_ORCHARD,
            participants=[participant(TILLO, "dowser", t(71), t(72, 40)), *[participant(member, "guild-hunter", t(71), t(72, 40)) for member in PARTY]],
            turns=orchard_turns, topics=("amber", "dowsing", "mine"), body="Tillo gives the party its first operational vocabulary for raw, refined, and quick amber while still withholding the Blackroot Compact."),
        scene_record(SC_ORCHARD, "Tracks in the Orchard", status="closed", start=t(69), current=t(76), end=t(76), location=EAST_ORCHARD,
            participants=[participant(SENNA, "orchard-keeper", t(69), t(71)), participant(TILLO, "dowser", t(71), t(73)), *[participant(member, "guild-hunter", t(69), t(76), pov=(member == SYLVI)) for member in PARTY]],
            objects=(DOWSING_FORK, WRETCH_RESIDUE, AMBER_CHEST), environments=(ENV_ORCHARD,),
            story_points=(SP_AMBER_LINK, SP_MINE, SP_WHAT_AMBER), conversations=(CV_TILLO,),
            observations=(
                observation("orchard-rime", 69, 10, ("participants",), "A line of white rime travels through buried roots without any print between trees."),
                observation("orchard-sylvi-birds", 69, 20, (SYLVI,), "Birds feed in the western rows but refuse every branch above the rime line."),
                observation("orchard-veyra-sap", 70, 15, (VEYRA,), "The wretch strengthens when fresh tree sap touches amber-colored soil, even without blood."),
                observation("orchard-tillo-fork", 72, 45, ("participants",), "Tillo's fork pulls toward Saint Orra hard enough to bend its copper binding."),
            ),
            constraints=("Tillo knows much more than he says and is ashamed of the guild compact.", "The party now has a defensible mine lead but not the full metaphysical explanation."),
            body="The investigation leaves the human crime scene and becomes ecological: the same cold route passes through orchard roots toward the amber mine.", tags=("orchard", "dowsing")),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "Tillo calls bloodless warmth, backward dowsing, and bodiless old shapes signs of quick amber; the fork points to Saint Orra.", "The mine is the next actionable source even if Tillo is concealing institutional history.", "impatient focus", 0.9, (orchard_turns[2]["id"], orchard_turns[8]["id"])),
        (SYLVI, "The wretch trail moves through roots and animal absence rather than across the ground; Tillo's fork confirms the mineward direction.", "Amber-bearing ground changes the behavior of the forest before a manifestation appears.", "ecological alarm", 0.94, (orchard_turns[7]["id"], orchard_turns[8]["id"])),
        (GARRAN, "Tillo refuses to call the old shapes souls and distinguishes a remembered form from a conscious dead person.", "The wretches may be repeated suffering without a surviving self to release.", "theological disorientation", 0.72, (orchard_turns[5]["id"], orchard_turns[6]["id"])),
        (VEYRA, "Gnome dowsing senses amber through pressure on tuned materials, and quick amber creates old shapes without living bodies.", "Amber stores or transmits structured impressions rather than ordinary spell energy.", "dangerous fascination", 0.93, (orchard_turns[1]["id"], orchard_turns[4]["id"])),
        (PIP, "Harrowcross pays Lantern Pike with amber mined at Saint Orra, and the same material is implicated in the attacks.", "The guild's revenue and monster work may be one closed commercial loop.", "grim amusement", 0.9, (orchard_turns[9]["id"], orchard_turns[10]["id"])),
    ]):
        operations.append(recollection_op(CV_TILLO, f"tillo-{index}", character, 73, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    # Common knowledge acquired from ledger and dowser.
    for index, character in enumerate(PARTY):
        operations.extend([
            knowledge_record(eid("knowledge", f"{character}-amber-extracted"), f"{CHARACTERS[character]['title']} knows gnomes dowse for amber pockets", character, "amber.pockets.dowsed-by-gnomes", "Gnome dowsers locate shallow amber pockets using tuned forks that pull toward the material.", 73, index, confidence=0.96, acquisition="told", truth="true", subject=TILLO, predicate="dowses-for", object_value={"entity": RAW_AMBER}, causing_event=EV_TILLO_DOWSES, source_entity=DOWSING_FORK, tags=("amber", "dowsing")),
            knowledge_record(eid("knowledge", f"{character}-guild-paid-amber"), f"{CHARACTERS[character]['title']} knows Harrowcross pays Lantern Pike in amber", character, "guild.services.paid-in-amber", "Harrowcross transfers a quarterly share of Saint Orra's amber production to Lantern Pike in exchange for guards and monster-hunting service.", 74, 10 + index, confidence=0.99, acquisition="read", truth="true", subject=HARROWCROSS, predicate="pays", object_value={"entity": HALRIC}, causing_event=EV_AMBER_PAYMENT, source_entity=WATCH_LEDGER, tags=("amber", "guild")),
            knowledge_record(eid("knowledge", f"{character}-quick-amber"), f"{CHARACTERS[character]['title']} knows the chest contained quick amber", character, "harrowcross.chest.quick-amber", "The Harrowcross payment chest contained raw amber that had become warm, backward-pulling, and capable of supporting wretch manifestations.", 74, 20 + index, confidence=0.9, acquisition="inferred", truth="true", subject=AMBER_CHEST, predicate="contained", object_value={"text": "quick amber"}, causing_event=EV_AMBER_PAYMENT, source_entity=AMBER_CHEST, tags=("amber", "wretch")),
        ])

    # Mine approach and active act boundary.
    jorund_turns = [
        turn("jorund:0", 81, 0, JORUND, "The upper drift is closed. Take your guild badges back to town.", delivery="hostile"),
        turn("jorund:1", 81, 10, RHEA, "Harrowcross lost three guards and two wretches led us to your road.", delivery="direct"),
        turn("jorund:2", 81, 20, JORUND, "Then kill them outside. My miners are not bait for a lodge report.", delivery="angry"),
        turn("jorund:3", 81, 30, VEYRA, "Your tally omits two drifts and the amber under this yard is warm.", delivery="observant"),
        turn("jorund:4", 81, 40, JORUND, "You have been here one minute.", delivery="alarmed"),
        turn("jorund:5", 81, 50, PIP, "Long minute.", delivery="mild"),
        turn("jorund:6", 82, 0, SYLVI, "Nothing nests within half a mile of the spoil heaps.", delivery="quiet"),
        turn("jorund:7", 82, 10, JORUND, "Animals are smarter than contracts.", delivery="bitter"),
    ]
    operations.extend([
        event_op(EV_MINE_APPROACH, "The Frontiersmen Follow the Quickenings to Saint Orra", 80, 20, MINE_YARD,
            [(TILLO, "dowser"), (JORUND, "foreman"), *[(member, "guild-hunter") for member in PARTY]],
            "Tillo guides the company along the mine road until his fork pulls downward toward the sealed upper drift. Foreman Jorund Bale meets them with a lit yellow-glass lamp and refuses entry before they ask.",
            causes=(EV_TILLO_DOWSES, EV_AMBER_PAYMENT), story_points=(SP_MINE, SP_ENTER_MINE, SP_WHAT_AMBER), effects=[*party_move_effects("mine-approach", MINE_YARD, "at-saint-orra"), effect("mine-tillo", TILLO, "location", "set", {"entity": MINE_YARD})], tags=("mine", "investigation")),
        conversation_op(CV_JORUND, "Jorund's Terms", status="active", start=t(81), end=None, scene=SC_MINE, location=MINE_YARD,
            participants=[participant(JORUND, "foreman", t(81)), participant(TILLO, "dowser", t(81)), *[participant(member, "guild-hunter", t(81)) for member in PARTY]],
            turns=jorund_turns, topics=("mine", "sealed-drift", "guild"), body="Jorund tries to bar the party from the drift while accidentally confirming that the mine has concealed workings and an animal-absence problem."),
        scene_record(SC_MINE, "Saint Orra Mine", status="active", start=t(80), current=t(85), end=None, location=MINE_YARD,
            participants=[participant(JORUND, "foreman", t(80)), participant(TILLO, "dowser", t(80)), *[participant(member, "guild-hunter", t(80), pov=(member == VEYRA)) for member in PARTY]],
            objects=(DOWSING_FORK, MINE_LAMP, MINE_TALLY, CAVE_MAP, *BADGES.values()), environments=(ENV_MINE,),
            story_points=(SP_MINE, SP_ENTER_MINE, SP_WHAT_AMBER), conversations=(CV_JORUND,),
            observations=(
                observation("mine-no-birds", 80, 20, (SYLVI,), "No bird, hare, or insect crosses the boundary between spruce litter and amber spoil."),
                observation("mine-veyra-warm", 81, 25, (VEYRA,), "The omitted drifts on Jorund's tally correspond to the two places where the ground presses against her teeth."),
                observation("mine-pip-tally", 81, 35, (PIP,), "Fresh knife cuts have removed two drift totals from the official tally board."),
                observation("mine-garran-lamp", 82, 5, (GARRAN,), "The Saint Orra lamp changes from yellow to green whenever Jorund points it toward the sealed drift."),
                observation("mine-rhea-ready", 84, 0, (RHEA,), "Jorund keeps his lower-drift map inside his coat and one hand on the drift bell rope."),
            ),
            constraints=("Jorund knows the sealed drift breached a living vein but has not confessed it.", "The party is at the threshold of the mine; the cave-in has not yet occurred."),
            body="The first investigation act ends at an unwilling mine: every independent clue points below, and the man responsible for the workers is more afraid of guild scrutiny than of the visible threat.", tags=("mine", "active")),
    ])

    # Party cohesion after first combat.
    complete = lambda **kw: {"trust": kw.get("trust", 0.0), "affinity": kw.get("affinity", 0.0), "fear": kw.get("fear", 0.0), "obligation": kw.get("obligation", 0.0), "respect": kw.get("respect", 0.0), "dependence": kw.get("dependence", 0.0)}
    operations.extend([
        relationship_update(RHEA, GARRAN, [relationship_transition("wretch-rhea-garran", 68, 0, complete(trust=0.5, affinity=0.1, fear=0.02, obligation=0.12, respect=0.58, dependence=0.26), ["trusted-warder", "wounded-protector"], "Garran's threshold blessing makes the wretch vulnerable despite his injury.", causing_event=EV_WRETCH_DRIVEN)]),
        relationship_update(GARRAN, RHEA, [relationship_transition("wretch-garran-rhea", 68, 1, complete(trust=0.54, affinity=0.14, fear=0.0, obligation=0.08, respect=0.56, dependence=0.18), ["shield-at-threshold", "leader"], "Rhea holds the creature in place long enough for a mercy-shaped prayer to matter.", causing_event=EV_WRETCH_DRIVEN)]),
        relationship_update(RHEA, PIP, [relationship_transition("wretch-rhea-pip", 68, 2, complete(trust=0.46, affinity=0.18, fear=0.02, obligation=0.08, respect=0.46, dependence=0.2), ["decisive-scout", "moves-the-danger"], "Pip recognizes that the chest is the battlefield and removes it under attack.", causing_event=EV_WRETCH_DRIVEN)]),
        relationship_update(VEYRA, GARRAN, [relationship_transition("wretch-veyra-garran", 68, 3, complete(trust=0.34, affinity=0.1, fear=0.04, obligation=0.08, respect=0.5, dependence=0.16), ["complementary-methods", "prayer-made-material"], "Garran's blessing produces a repeatable physical change in the manifestation.", causing_event=EV_WRETCH_DRIVEN)]),
        relationship_update(SYLVI, VEYRA, [relationship_transition("wretch-sylvi-veyra", 68, 4, complete(trust=0.3, affinity=0.1, fear=0.04, obligation=0.04, respect=0.52, dependence=0.14), ["cross-checks-magic", "shared-theory"], "Veyra's pulse observations agree with Sylvi's trackless movement evidence.", causing_event=EV_WRETCH_DRIVEN)]),
    ])

    # Story lifecycle through the mine threshold.
    operations.extend([
        story_update(SP_CARAVAN, [story_transition("caravan-active", 26, 10, "active", EV_DEPART, "The company begins eastroad duty."), story_transition("caravan-resolved", 57, 25, "resolved", EV_ASSIGNED, "Emergency reassignment ends the caravan contract early.")], outcomes=(EV_ASSIGNED,)),
        story_update(SP_REPLACE, [story_transition("replace-active", 57, 25, "active", EV_ASSIGNED, "The party becomes Harrowcross's temporary watch and investigation detail.")]),
        story_update(SP_LAST_WATCH, [story_transition("last-watch-active", 53, 10, "active", EV_CARAVAN_ARRIVES, "Kellan begins giving the surviving account."), story_transition("last-watch-resolved", 64, 10, "resolved", EV_WRETCH_DRIVEN, "Kellan's testimony and the repeated trigger reconstruct the attack sequence.")], outcomes=(EV_GUARD_MASSACRE, EV_AMBER_QUICKENS)),
        story_update(SP_FIRST_WRETCH, [story_transition("first-wretch-active", 63, 20, "active", EV_FIRST_WRETCH, "A manifestation attacks the investigators."), story_transition("first-wretch-resolved", 64, 10, "resolved", EV_WRETCH_DRIVEN, "The party weakens and disperses the manifestation.")], outcomes=(EV_WRETCH_DRIVEN,)),
        story_update(SP_AMBER_LINK, [story_transition("amber-link-active", 63, 10, "active", EV_AMBER_QUICKENS, "Blood and the chest precede a second manifestation."), story_transition("amber-link-resolved", 74, 0, "resolved", EV_AMBER_PAYMENT, "Witnessed quickening, dowsing, and ledger provenance establish an operational amber link.")], outcomes=(EV_AMBER_QUICKENS, EV_TILLO_DOWSES, EV_AMBER_PAYMENT)),
        story_update(SP_MINE, [story_transition("mine-trace-active", 72, 45, "active", EV_TILLO_DOWSES, "The dowsing fork and wretch route point to Saint Orra."), story_transition("mine-trace-resolved", 80, 20, "resolved", EV_MINE_APPROACH, "The party reaches the sealed drift that draws the fork.")], outcomes=(EV_MINE_APPROACH,)),
        story_update(SP_ENTER_MINE, [], dependencies={"all": [{"story_point": SP_MINE, "state_in": ["resolved"]}]}, trigger={"scene": {"location": MINE_YARD, "participant": RHEA}}, activation="manual"),
        story_update(SP_WHAT_AMBER, [story_transition("what-amber-active", 72, 45, "active", EV_TILLO_DOWSES, "Tillo supplies a partial operational vocabulary for quick amber.")]),
        story_update(SP_GUILD_SECRET, [story_transition("guild-secret-active", 74, 0, "active", EV_AMBER_PAYMENT, "The protection ledger makes guild dependence on amber explicit.")]),
    ])
    return operations


def act3_operations() -> list[dict[str, Any]]:
    """Saint Orra collapse and the two-day underground passage to the hilltop."""
    operations: list[dict[str, Any]] = []

    # Complete Jorund's negotiation and close the mine-yard scene.
    jorund_more = [
        turn("jorund:8", 82, 20, RHEA, "Open the drift or tell us what happened inside it.", delivery="controlled"),
        turn("jorund:9", 82, 30, JORUND, "We followed a new seam through black rootstone. Three cutters said the wall was warm. Then the tally bell rang from underground.", delivery="defeated"),
        turn("jorund:10", 82, 40, TILLO, "You cut a membrane after I marked it sleeping.", delivery="furious"),
        turn("jorund:11", 82, 50, JORUND, "I marked it unproductive. The guild collector marked it overdue.", delivery="bitter"),
        turn("jorund:12", 83, 0, GARRAN, "Are miners still below?", delivery="urgent"),
        turn("jorund:13", 83, 10, JORUND, "One pump tender never came out. The others refuse to go past the green lamp line.", delivery="quiet"),
        turn("jorund:14", 83, 20, VEYRA, "The line is green here in daylight.", delivery="observant"),
        turn("jorund:15", 83, 30, JORUND, "That is why I am not opening the gate.", delivery="flat"),
        turn("jorund:16", 84, 0, PIP, "The gate bolt was drawn this morning. Someone opened it already.", delivery="technical"),
        turn("jorund:17", 84, 10, JORUND, "Then we are finished arguing.", delivery="afraid"),
        turn("jorund:18", 84, 20, SYLVI, "No. We are finished pretending the danger is behind the gate.", delivery="quiet"),
        turn("jorund:19", 84, 30, RHEA, "You lead us to the green line. Tillo stays above and rings three times if the fork turns away.", delivery="command"),
    ]
    for value in jorund_more:
        operations.append({"type": "conversation.turn.append", "conversationId": CV_JORUND, "turn": value})
    operations.append({"type": "entity.update", "entityId": CV_JORUND, "frontmatterPatch": {"status": "closed", "time": {"start": t(81), "end": t(84, 30)}}})
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "Jorund breached a warm seam under production pressure, lost a pump tender, and discovered the sealed gate had been opened again.", "The mine foreman concealed danger to protect workers and output at the same time; he is culpable but necessary.", "contained anger", 0.91, (jorund_more[1]["id"], jorund_more[9]["id"])),
        (SYLVI, "Animals avoid the spoil boundary and someone reopened the dangerous drift after Jorund sealed it.", "The threat is already moving independently of the mine's formal gates.", "predatory focus", 0.9, (jorund_more[9]["id"], jorund_more[10]["id"])),
        (GARRAN, "A pump tender remains missing below a warm seam while miners refuse to cross the lamp's green line.", "Rescue is still possible and gives moral weight to entering despite the risk.", "urgent compassion", 0.84, (jorund_more[4]["id"], jorund_more[5]["id"])),
        (VEYRA, "The lamp reacts at the yard and the warm seam lies through black rootstone rather than ordinary ore.", "The mine has intersected a larger active structure rather than an isolated amber pocket.", "awe restrained by fear", 0.88, (jorund_more[1]["id"], jorund_more[6]["id"])),
        (PIP, "The sealed gate's bolt was operated recently despite Jorund's refusal to reopen it.", "Someone inside or outside the crew is maintaining access to the concealed drift.", "suspicious concentration", 0.94, (jorund_more[8]["id"], jorund_more[9]["id"])),
        (JORUND, "The guild hunters found every omission and mechanical contradiction before entering the drift.", "If they die, his concealment will be harder to excuse than the original breach.", "shame and dread", 0.86, (jorund_more[0]["id"], jorund_more[11]["id"])),
        (TILLO, "Jorund cut a membrane the dowser had marked sleeping because the guild collection was overdue.", "The Blackroot handling rules failed because production pressure rewards violating them.", "old guilt renewed", 0.97, (jorund_more[2]["id"], jorund_more[3]["id"])),
    ]):
        operations.append(recollection_op(CV_JORUND, f"jorund-act3-{index}", character, 85, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    operations.append(scene_record(
        SC_MINE, "Saint Orra Mine", status="closed", start=t(80), current=t(85), end=t(85), location=MINE_YARD,
        participants=[participant(JORUND, "foreman", t(80), t(85)), participant(TILLO, "dowser", t(80), t(85)), *[participant(member, "guild-hunter", t(80), t(85), pov=(member == VEYRA)) for member in PARTY]],
        objects=(DOWSING_FORK, MINE_LAMP, MINE_TALLY, CAVE_MAP, *BADGES.values()), environments=(ENV_MINE,),
        story_points=(SP_MINE, SP_ENTER_MINE, SP_WHAT_AMBER), conversations=(CV_JORUND,),
        observations=(
            observation("mine-no-birds", 80, 20, (SYLVI,), "No bird, hare, or insect crosses the boundary between spruce litter and amber spoil."),
            observation("mine-veyra-warm", 81, 25, (VEYRA,), "The omitted drifts on Jorund's tally correspond to the two places where the ground presses against her teeth."),
            observation("mine-pip-tally", 81, 35, (PIP,), "Fresh knife cuts have removed two drift totals from the official tally board."),
            observation("mine-garran-lamp", 82, 5, (GARRAN,), "The Saint Orra lamp changes from yellow to green whenever Jorund points it toward the sealed drift."),
            observation("mine-rhea-ready", 84, 0, (RHEA,), "Jorund keeps his lower-drift map inside his coat and one hand on the drift bell rope."),
            observation("mine-pip-bolt", 84, 0, (PIP,), "Fresh graphite on the gate bolt shows it was drawn after Jorund claims to have sealed it."),
        ),
        constraints=("Jorund still does not know the Blackroot Compact.", "The party enters for rescue and investigation, not because they understand amber."),
        body="The mine negotiation strips away the last procedural excuse. A worker may remain below, the sealed gate has been used, and every instrument points into the omitted drift.", tags=("mine", "closed"),
    ))

    # Entry, vein reaction, wretch encounter, and collapse.
    operations.extend([
        event_op(EV_ENTER_MINE, "The Frontiersmen Enter Saint Orra's Sealed Drift", 86, 0, UPPER_DRIFT,
            [(JORUND, "guide"), *[(member, "guild-hunter") for member in PARTY]],
            "Jorund opens the drift and guides the company past abandoned cutting stations. Amber seams brighten behind black rootstone whenever the party speaks, as if sound is arriving before them.",
            causes=(EV_MINE_APPROACH,), story_points=(SP_ENTER_MINE, SP_CAVEIN), effects=[
                *party_move_effects("enter-mine", UPPER_DRIFT, "inside-sealed-drift"),
                effect("enter-jorund", JORUND, "location", "set", {"entity": UPPER_DRIFT}),
            ], tags=("mine", "descent")),
        event_op(EV_VEIN_REACTS, "The Living Amber Vein Answers the Party", 88, 20, AMBER_CHAMBER,
            [(JORUND, "guide"), (VEYRA, "observer"), (GARRAN, "warder"), *[(member, "witness") for member in PARTY]],
            "The drift opens onto a membrane of liquid-gold amber threaded between petrified roots. It repeats the party's footfalls a heartbeat late and briefly shows five other silhouettes walking inside it.",
            causes=(EV_ENTER_MINE,), story_points=(SP_WHAT_AMBER, SP_CAVEIN), effects=[
                *party_move_effects("vein-party", AMBER_CHAMBER, "before-living-vein"),
                effect("vein-jorund", JORUND, "location", "set", {"entity": AMBER_CHAMBER}),
                effect("vein-state", AMBER_VEIN, "condition", "set", "awake and imitating nearby forms"),
            ], tags=("amber", "living-vein")),
        event_op(EV_MINE_WRETCH, "A Mine Wretch Phases Out of the Living Vein", 89, 10, AMBER_CHAMBER,
            [(RHEA, "shield"), (SYLVI, "archer"), (GARRAN, "warder"), (VEYRA, "arcanist"), (PIP, "scout"), (JORUND, "guide")],
            "A long-bodied wretch emerges from the vein wearing the pump tender's lamp face and the claws of a cave bear. It repeats Jorund's orders before he speaks them, striking each place the foreman is about to indicate.",
            causes=(EV_VEIN_REACTS,), story_points=(SP_CAVEIN, SP_WHAT_AMBER), effects=[effect("mine-wretch-jorund", JORUND, "condition", "set", "concussed and bleeding")], tags=("wretch", "mine")),
        event_op(EV_CAVEIN, "The Saint Orra Drift Collapses", 91, 0, AMBER_CHAMBER,
            [(RHEA, "shield"), (SYLVI, "archer"), (GARRAN, "warder"), (VEYRA, "arcanist"), (PIP, "scout"), (JORUND, "foreman")],
            "Veyra scorches the membrane to force the wretch solid. The living vein contracts through the rootstone, snapping old support timbers in sequence. Jorund throws his map and mine lamp across the fracture before the roof comes down between him and the party.",
            causes=(EV_MINE_WRETCH,), story_points=(SP_CAVEIN, SP_CAVERNS), effects=[
                *party_move_effects("cavein-party", CAVEIN, "trapped underground"),
                effect("cavein-jorund", JORUND, "location", "set", {"entity": UPPER_DRIFT}),
                effect("cavein-jorund-state", JORUND, "condition", "set", "separated behind collapse; fate unknown"),
                effect("cave-map-clear", CAVE_MAP, "holder", "clear"),
                effect("cave-map-pip", CAVE_MAP, "holder", "set", {"entity": PIP}),
                effect("mine-lamp-clear", MINE_LAMP, "holder", "clear"),
                effect("mine-lamp-garran", MINE_LAMP, "holder", "set", {"entity": GARRAN}),
                effect("vein-collapse", AMBER_VEIN, "condition", "set", "contracted behind collapse"),
            ], tags=("cave-in", "disaster")),
        event_op(EV_JORUND_LOST, "Jorund Is Lost Behind the Collapse", 91, 10, CAVEIN,
            [(JORUND, "separated-foreman"), *[(member, "trapped-hunter") for member in PARTY]],
            "The party hears Jorund strike the far side of the rock three times, then one final blow farther away. No voice follows. The main drift and surface bell rope are buried.",
            causes=(EV_CAVEIN,), story_points=(SP_CAVEIN,), effects=[], tags=("separation", "mine")),
    ])

    cave_turns = [
        turn("cavein:0", 92, 0, RHEA, "Names and injuries. Quickly.", delivery="command"),
        turn("cavein:1", 92, 10, SYLVI, "Sylvi. Shoulder cut. Air is moving from below us.", delivery="controlled"),
        turn("cavein:2", 92, 20, GARRAN, "Garran. Old frost wound, new bruising. The lamp survived.", delivery="steady"),
        turn("cavein:3", 92, 30, VEYRA, "Veyra. Hearing the vein through the stone. I would prefer not to.", delivery="rapid"),
        turn("cavein:4", 92, 40, PIP, "Pip. Uninjured in all morally significant ways. I have Jorund's map.", delivery="breathless"),
        turn("cavein:5", 92, 50, RHEA, "Jorund?", delivery="loud"),
        turn("cavein:6", 93, 0, PIP, "Three strikes. Then one moving away. Nothing now.", delivery="quiet"),
        turn("cavein:7", 93, 10, GARRAN, "We dig until the air fails or he answers.", delivery="firm"),
        turn("cavein:8", 94, 0, SYLVI, "The dust is settling toward the lower fissure. The surface side has no breath.", delivery="certain"),
        turn("cavein:9", 94, 10, VEYRA, "The map ends at OLD WATER beneath us. There are older passages beyond the mine work.", delivery="focused"),
        turn("cavein:10", 94, 20, RHEA, "We mark this chamber, take the lower air, and return for Jorund from outside.", delivery="decisive"),
        turn("cavein:11", 94, 30, GARRAN, "Say it as a promise, not a plan.", delivery="low"),
        turn("cavein:12", 94, 40, RHEA, "We come back for him.", delivery="formal"),
    ]
    operations.extend([
        conversation_op(CV_CAVEIN, "After the Cave-In", status="closed", start=t(92), end=t(94, 40), scene=SC_CAVEIN, location=CAVEIN,
            participants=[participant(member, "trapped-hunter", t(92), t(94, 40)) for member in PARTY], turns=cave_turns,
            topics=("cave-in", "survival", "jorund"), body="The party inventories itself, attempts to hear Jorund, and makes the consequential decision to follow lower airflow rather than die digging toward a sealed surface."),
        event_op(EV_REGROUP, "The Frontiersmen Choose the Lower Air", 101, 0, CAVEIN,
            [(RHEA, "leader"), (SYLVI, "air-finder"), (GARRAN, "healer"), (VEYRA, "reader"), (PIP, "mapper")],
            "After hours of digging and no further answer, Sylvi proves the lower fissure carries breathable air. Pip copies the collapse chamber onto Jorund's map, Garran marks the return promise, and Rhea leads the company into passages older than Saint Orra.",
            causes=(EV_CAVEIN, EV_JORUND_LOST), story_points=(SP_CAVEIN, SP_CAVERNS), effects=[*party_move_effects("regroup-lower", LOWER_CAVERNS, "exhausted but moving")], tags=("survival", "caverns")),
        scene_record(SC_CAVEIN, "Cave-In Dark", status="closed", start=t(91), current=t(101), end=t(101), location=CAVEIN,
            participants=[participant(member, "trapped-hunter", t(91), t(101), pov=(member == RHEA)) for member in PARTY],
            objects=(CAVE_MAP, MINE_LAMP, BROKEN_SPEAR, WRETCH_RESIDUE, *BADGES.values()), environments=(ENV_CAVEIN,),
            story_points=(SP_CAVEIN, SP_CAVERNS), conversations=(CV_CAVEIN,),
            observations=(
                observation("cavein-air", 92, 10, (SYLVI,), "Dust threads flatten toward a black fissure below while the collapsed drift side remains still."),
                observation("cavein-veyra-pulse", 92, 30, (VEYRA,), "The living vein repeats their heartbeats through several yards of stone with one extra beat among them."),
                observation("cavein-pip-map", 92, 40, (PIP,), "Jorund's map ends at a lower annotation marked OLD WATER and a passage he never inked."),
                observation("cavein-garran-strikes", 93, 0, (GARRAN,), "Three distant blows answer from behind the collapse; the fourth arrives from farther away."),
                observation("cavein-rhea-choice", 94, 20, (RHEA,), "The party has one day of lamp oil and less than two days of food if rationed."),
            ),
            constraints=("Jorund's fate remains unknown.", "The party believes the lower route may return toward the surface; it does not."),
            body="The catastrophe converts investigation into expedition. The party has no surface route, limited light, and a promise it may not be able to keep.", tags=("cave-in", "survival")),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "The surface drift had no air; the lower fissure did. The party promised to return for Jorund and chose movement over a fatal dig.", "Leadership below means selecting which obligation can be postponed without pretending it is discharged.", "guilt made operational", 0.96, (cave_turns[10]["id"], cave_turns[12]["id"])),
        (SYLVI, "Airflow proved the only viable route was down into older water passages.", "The mine is built across a preexisting underground ecology the surface crews do not understand.", "claustrophobic focus", 0.98, (cave_turns[1]["id"], cave_turns[8]["id"])),
        (GARRAN, "Jorund answered three times before moving away; Rhea promised the party would return.", "A promise to the absent is still a threshold the living can violate.", "grief held in ritual", 0.87, (cave_turns[6]["id"], cave_turns[12]["id"])),
        (VEYRA, "The vein repeated six heartbeats for five people and the old map points to passages below mine construction.", "The amber network may preserve or simulate forms independently of visible wretches.", "fascination turning invasive", 0.78, (cave_turns[3]["id"], cave_turns[9]["id"])),
        (PIP, "The lower map annotation and airflow offered a route; the collapse chamber is now the first reliable point on his underground survey.", "Maps become promises when people depend on the return line.", "fear controlled by measurement", 0.92, (cave_turns[4]["id"], cave_turns[10]["id"])),
    ]):
        operations.append(recollection_op(CV_CAVEIN, f"cavein-{index}", character, 102, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    # Two days below: black water, old camp, and the party's first honest night.
    below_turns = [
        turn("below:0", 111, 0, PIP, "We have moved eleven map lengths east and three down. That is either excellent progress or a very specific grave.", delivery="tired"),
        turn("below:1", 111, 10, RHEA, "Eat before the jokes become strategy.", delivery="dry"),
        turn("below:2", 111, 20, GARRAN, "I have half a road blessing left and no road. We may need to rename it.", delivery="weary"),
        turn("below:3", 111, 30, SYLVI, "There are blind fish in the black water. Real ones. The amber has not emptied everything.", delivery="quietly relieved"),
        turn("below:4", 111, 40, VEYRA, "The roots under the water glow when we remember something aloud.", delivery="careful"),
        turn("below:5", 111, 50, RHEA, "Then we remember silently.", delivery="immediate"),
        turn("below:6", 112, 0, PIP, "I remember owing three men money. The roots remain unimpressed.", delivery="testing"),
        turn("below:7", 112, 10, SYLVI, "Say something that hurt.", delivery="flat"),
        turn("below:8", 112, 20, PIP, "I once locked myself in a customs vault for six hours because I had forged the key backward.", delivery="confessing"),
        turn("below:9", 112, 30, VEYRA, "They brightened.", delivery="unhappy"),
        turn("below:10", 112, 40, RHEA, "No more experiments with our own fear.", delivery="command"),
        turn("below:11", 113, 0, GARRAN, "That may be the first sensible rule anyone has made about amber.", delivery="soft"),
        turn("below:12", 113, 10, RHEA, "The guild had rules. It kept them from us.", delivery="cold"),
        turn("below:13", 113, 20, VEYRA, "And Tillo knew the word quick before the chest moved.", delivery="quiet"),
        turn("below:14", 114, 0, SYLVI, "When we surface, the guild answers before we take another contract.", delivery="certain"),
    ]
    operations.extend([
        event_op(EV_BLACKWATER_CROSS, "The Frontiersmen Cross the Black Water", 108, 20, BLACKWATER,
            [(RHEA, "rope-anchor"), (SYLVI, "route-finder"), (GARRAN, "lamp-bearer"), (VEYRA, "amber-watcher"), (PIP, "mapper")],
            "The lower route becomes a flooded throat. Rhea anchors a rope across, Sylvi follows blind fish toward moving water, and the party crosses while amber threads beneath the pool imitate the shadows of swimming animals that are not present.",
            causes=(EV_REGROUP,), story_points=(SP_CAVERNS,), effects=[*party_move_effects("blackwater-party", BLACKWATER, "cold and rationing")], tags=("caverns", "crossing")),
        event_op(EV_FIND_CAMP, "The Party Finds an Old Underground Camp", 116, 0, LOWER_CAVERNS,
            [(RHEA, "leader"), (SYLVI, "tracker"), (GARRAN, "healer"), (VEYRA, "reader"), (PIP, "mapper")],
            "Beyond the black water they find a centuries-old camp: stone fire ring, bone needles, and soot marks deliberately placed over amber-lit root cracks. Someone lived below long enough to learn darkness was safer than memory-light.",
            causes=(EV_BLACKWATER_CROSS,), story_points=(SP_CAVERNS, SP_RUINS, SP_WHAT_AMBER), effects=[*party_move_effects("old-camp-party", LOWER_CAVERNS, "second-day-underground")], tags=("camp", "ruins")),
        conversation_op(CV_BELOW, "Second Night Below", status="closed", start=t(111), end=t(114), scene=SC_BLACKWATER, location=BLACKWATER,
            participants=[participant(member, "stranded-hunter", t(111), t(114)) for member in PARTY], turns=below_turns,
            topics=("fear", "amber", "guild"), body="The party tests the glowing roots by speaking memories, then recognizes that the guild possessed operational rules it withheld."),
        scene_record(SC_BLACKWATER, "Blackwater Below", status="closed", start=t(102), current=t(120), end=t(120), location=BLACKWATER,
            participants=[participant(member, "underground-traveler", t(102), t(120), pov=(member == PIP)) for member in PARTY],
            objects=(CAVE_MAP, MINE_LAMP, BROKEN_SPEAR, WRETCH_RESIDUE, *BADGES.values()), environments=(ENV_BLACKWATER,),
            story_points=(SP_CAVERNS, SP_RUINS, SP_WHAT_AMBER, SP_GUILD_SECRET), conversations=(CV_BELOW,),
            observations=(
                observation("blackwater-fish", 107, 20, (SYLVI,), "Blind white fish occupy the flowing channel but avoid every amber-lit root."),
                observation("blackwater-shadows", 108, 20, ("participants",), "Animal-shaped shadows swim beneath the crossing although the water around the rope is empty."),
                observation("blackwater-memory", 112, 30, (VEYRA,), "The submerged roots brighten when Pip describes a humiliating frightened memory and dim when he jokes without believing it."),
                observation("blackwater-old-camp", 116, 0, ("participants",), "Ancient soot has been packed over every amber crack surrounding the abandoned camp."),
                observation("blackwater-rhea-rations", 118, 0, (RHEA,), "The party has one full meal left and enough lamp oil for perhaps another day."),
            ),
            constraints=("The roots respond to emotionally salient memory, but the party cannot yet prove whether they store or merely attract it.", "The party's growing suspicion of the guild remains inference, not knowledge of the Blackroot Compact."),
            body="Two days underground make the group into a real company. Hunger and darkness reduce their theories to the observations they are willing to trust.", tags=("caverns", "survival")),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "Amber-lit roots brightened when Pip recalled genuine fear, and Tillo and the guild already had rules for quick amber.", "The guild concealed practical hazard knowledge from workers and hunters who were expected to handle the material.", "anger becoming purpose", 0.88, (below_turns[10]["id"], below_turns[12]["id"])),
        (SYLVI, "Real blind fish avoid amber roots while emotionally charged speech brightens them.", "Amber changes behavior across living ecosystems before it produces visible monsters.", "relief at living tracks, then alarm", 0.91, (below_turns[3]["id"], below_turns[14]["id"])),
        (GARRAN, "The roots answered painful memory, not ordinary words, and old inhabitants covered them in soot.", "Whatever amber stores or calls, the danger is tied to remembered suffering more closely than to death itself.", "faith under revision", 0.79, (below_turns[8]["id"], below_turns[11]["id"])),
        (VEYRA, "The roots brighten for emotionally real memories and respond weakly to fabricated statements.", "Amber may be a medium for structured lived impressions rather than a source of free magical energy.", "intellectual hunger tempered by consent", 0.9, (below_turns[4]["id"], below_turns[9]["id"])),
        (PIP, "His genuine humiliation brightened the roots, proving that the underground system distinguishes performed speech from felt recollection.", "The caves are reading them, which makes mapping space alone insufficient.", "exposure and defiance", 0.87, (below_turns[6]["id"], below_turns[8]["id"])),
    ]):
        operations.append(recollection_op(CV_BELOW, f"below-{index}", character, 115, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    # The Sunken Hall supplies the oldest surviving theory, then punishes curiosity.
    hall_turns = [
        turn("hall:0", 123, 0, PIP, "These stairs were built from below upward. The tool marks face the wrong way for a mine entrance.", delivery="technical"),
        turn("hall:1", 123, 10, SYLVI, "No animal bone in the hall. Only teeth placed around the stone box.", delivery="quiet"),
        turn("hall:2", 123, 20, VEYRA, "The tablet calls amber 'golden sap after the root has remembered the wound.'", delivery="translating"),
        turn("hall:3", 123, 30, GARRAN, "Remembered by whom?", delivery="strained"),
        turn("hall:4", 123, 40, VEYRA, "Not whom. The grammar makes the root the subject.", delivery="awed"),
        turn("hall:5", 124, 0, RHEA, "Read the warning before the poetry.", delivery="firm"),
        turn("hall:6", 124, 10, VEYRA, "'Do not give the sap fresh terror, for it returns the hunter and the hunted without knowing the difference.'", delivery="careful"),
        turn("hall:7", 124, 20, GARRAN, "Then the wretches are not dead souls.", delivery="quiet"),
        turn("hall:8", 124, 30, VEYRA, "They may be memories with enough structure to move.", delivery="precise"),
        turn("hall:9", 125, 0, PIP, "The stone box has a pressure latch and absolutely no written suggestion that opening it is wise.", delivery="dry"),
        turn("hall:10", 125, 10, RHEA, "We leave it closed.", delivery="immediate"),
        turn("hall:11", 125, 20, VEYRA, "The tablet also says the lattice can quiet an awakened route.", delivery="reluctant"),
        turn("hall:12", 125, 30, SYLVI, "Or wake every shape stored around it.", delivery="flat"),
        turn("hall:13", 126, 0, RHEA, "We copy the tablet and leave the box.", delivery="decisive"),
        turn("hall:14", 127, 0, PIP, "The floor behind us has started breathing.", delivery="very quiet"),
    ]
    operations.extend([
        event_op(EV_HALL_FOUND, "The Frontiersmen Find the Sunken Hall", 122, 20, SUNKEN_HALL,
            [(RHEA, "leader"), (SYLVI, "tracker"), (GARRAN, "priest"), (VEYRA, "reader"), (PIP, "mapper")],
            "Beyond the old camp, dressed stone emerges from the cavern wall. A stair descends into a hall built around amber-bearing roots, with murals showing hunters and prey dissolving into the same golden lattice.",
            causes=(EV_FIND_CAMP,), story_points=(SP_RUINS, SP_WHAT_AMBER), effects=[*party_move_effects("hall-party", SUNKEN_HALL, "inside-sunken-hall")], tags=("ruins", "amber")),
        event_op(EV_TABLET_READ, "Veyra Reads the Memory-Sap Tablet", 124, 15, SUNKEN_HALL,
            [(VEYRA, "translator"), (GARRAN, "theological-witness"), (RHEA, "decision-maker"), (SYLVI, "ecological-witness"), (PIP, "copyist")],
            "Veyra translates the hall tablet: amber is golden sap after a buried root has remembered a wound; fresh terror can return hunter and hunted without distinguishing them. Garran rejects his earlier soul theory, while Pip copies every line onto the back of Jorund's map.",
            causes=(EV_HALL_FOUND,), story_points=(SP_RUINS, SP_WHAT_AMBER), effects=[], tags=("lore", "amber")),
        event_op(EV_RELIQUARY_OPEN, "The Amber Reliquary Opens Under Pressure", 127, 10, SUNKEN_HALL,
            [(PIP, "locksmith"), (VEYRA, "arcanist"), (RHEA, "leader"), (SYLVI, "watcher"), (GARRAN, "warder")],
            "The breathing floor closes the route back. Pip opens the reliquary because its lattice is the only mechanism named for quieting an awakened path. The amber lattice inside unfolds like roots in water and absorbs the party's nearest shadows.",
            causes=(EV_TABLET_READ,), story_points=(SP_RUINS, SP_GALLERY), effects=[
                effect("reliquary-clear", AMBER_RELIQUARY, "location", "clear"),
                effect("reliquary-veyra", AMBER_RELIQUARY, "holder", "set", {"entity": VEYRA}),
                effect("reliquary-state", AMBER_RELIQUARY, "condition", "set", "open and carrying five fresh shadow-impressions"),
            ], tags=("reliquary", "choice")),
        conversation_op(CV_HALL, "The Sunken Hall Debate", status="closed", start=t(123), end=t(127), scene=SC_HALL, location=SUNKEN_HALL,
            participants=[participant(member, "underground-investigator", t(123), t(127)) for member in PARTY], turns=hall_turns,
            topics=("amber", "memory", "reliquary"), body="Ancient grammar gives the party a theory of amber as root memory. They decide against opening the reliquary until the hall itself blocks retreat."),
        scene_record(SC_HALL, "Sunken Hall", status="closed", start=t(121), current=t(135), end=t(135), location=SUNKEN_HALL,
            participants=[participant(member, "ruin-explorer", t(121), t(135), pov=(member == VEYRA)) for member in PARTY],
            objects=(CAVE_MAP, MINE_LAMP, RUIN_TABLET, AMBER_RELIQUARY, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_RUINS,),
            story_points=(SP_RUINS, SP_WHAT_AMBER, SP_GALLERY), conversations=(CV_HALL,),
            observations=(
                observation("hall-mural", 122, 20, ("participants",), "The same gold root is painted through hunters, prey, and human mourners without separating their bodies."),
                observation("hall-sylvi-teeth", 123, 10, (SYLVI,), "Every animal tooth around the reliquary was cut from a jaw after death and arranged by species."),
                observation("hall-veyra-grammar", 123, 20, (VEYRA,), "The oldest inscription treats the buried root as the acting mind and people as passing impressions."),
                observation("hall-garran-no-souls", 124, 20, (GARRAN,), "The warning describes returned forms as unable to distinguish hunter from hunted, not as conscious dead."),
                observation("hall-pip-breath", 127, 0, (PIP,), "Dust rises and falls through cracks behind the party in the rhythm of a sleeping chest."),
            ),
            constraints=("The tablet is reliable ancient evidence but not a complete scientific account.", "Opening the reliquary is a coerced tactical choice, not Veyra casually ignoring Rhea."),
            body="The ruins provide the campaign's first defensible ontology: amber is remembered form in a buried living network. The immediate need to escape keeps that revelation from becoming safe scholarship.", tags=("ruins", "lore")),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "The tablet says amber is root memory and fresh terror returns hunter and hunted without distinction. The party opened the reliquary only after retreat closed.", "The practical question is no longer whether amber causes wretches but who knew the handling rules and kept mining.", "clarity sharpened into accusation", 0.9, (hall_turns[6]["id"], hall_turns[13]["id"])),
        (SYLVI, "The hall treats animal, human, hunter, and prey as impressions in one root system; the reliquary is ringed by deliberately catalogued teeth.", "The Frontier's animal silence may be avoidance of a memory ecology older than settlement.", "awe without comfort", 0.88, (hall_turns[1]["id"], hall_turns[12]["id"])),
        (GARRAN, "The inscription says returned forms cannot distinguish hunter from hunted, contradicting his belief that wretches are conscious dead souls.", "Mercy must address repeated pain without pretending a person remains inside every shape.", "faith broken open", 0.84, (hall_turns[6]["id"], hall_turns[7]["id"])),
        (VEYRA, "Ancient grammar names the root as the remembering subject and amber as golden sap retaining wounded forms.", "The Heartwood may be an enormous distributed memory rather than a conventional magical creature.", "wonder approaching temptation", 0.94, (hall_turns[2]["id"], hall_turns[4]["id"])),
        (PIP, "The party chose not to open the reliquary until the return floor began moving; its lattice is designed to quiet routes but also records nearby shadows.", "Ancient safety devices require inputs that can become new hazards.", "technical fascination under terror", 0.9, (hall_turns[9]["id"], hall_turns[14]["id"])),
    ]):
        operations.append(recollection_op(CV_HALL, f"hall-{index}", character, 136, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    operations.extend([
        event_op(EV_WRETCH_PACK, "The Sunken Hall Releases a Pack of Remembered Beasts", 136, 10, WRETCH_GALLERY,
            [(RHEA, "shield"), (SYLVI, "route-finder"), (GARRAN, "warder"), (VEYRA, "reliquary-bearer"), (PIP, "door-finder")],
            "The opened lattice quiets the hall behind them but wakes a gallery ahead. Pale forms of cave lion, elk, wolf, miner, and something winged phase through pillars, changing prey and predator anatomy whenever the party shows fear.",
            causes=(EV_RELIQUARY_OPEN,), story_points=(SP_GALLERY, SP_WHAT_AMBER), effects=[*party_move_effects("gallery-party", WRETCH_GALLERY, "pursued underground")], tags=("wretch-pack", "ruins")),
        event_op(EV_GALLERY_ESCAPE, "The Frontiersmen Escape the Wretch Gallery", 151, 20, WRETCH_GALLERY,
            [(RHEA, "rear-guard"), (SYLVI, "route-finder"), (GARRAN, "warder"), (VEYRA, "reliquary-bearer"), (PIP, "locksmith")],
            "Pip finds an air shaft disguised as a carved tree trunk. Garran uses the reliquary lattice as a moving threshold while Rhea and Sylvi retreat without feeding the pack fresh panic. Veyra closes the lattice only after their shadows stop multiplying.",
            causes=(EV_WRETCH_PACK,), story_points=(SP_GALLERY, SP_SURFACE), effects=[
                *party_move_effects("gallery-escape", HILLTOP, "surfaced, starved, and exhausted"),
                effect("lamp-spent", MINE_LAMP, "condition", "set", "empty but intact"),
                effect("reliquary-closed", AMBER_RELIQUARY, "condition", "set", "closed; contains five shadow-impressions"),
            ], tags=("escape", "wretch-pack")),
        event_op(EV_SURFACE, "The Party Emerges on a Hilltop Deep in the Frontier", 158, 0, HILLTOP,
            [(RHEA, "leader"), (SYLVI, "tracker"), (GARRAN, "healer"), (VEYRA, "arcanist"), (PIP, "mapper")],
            "After climbing the shaft for hours, the party pushes through root mat into cold dawn. No road, field smoke, or river landmark is visible—only unbroken boreal forest and unfamiliar ridgelines.",
            causes=(EV_GALLERY_ESCAPE,), story_points=(SP_SURFACE, SP_TREE, SP_RETURN), effects=[*party_move_effects("surface-party", HILLTOP, "isolated in deep forest")], tags=("surface", "isolation")),
        event_op(EV_SEE_CAMP, "The Frontiersmen See Smoke from a Masked Camp", 160, 0, HILLTOP,
            [(SYLVI, "spotter"), (PIP, "scout"), (RHEA, "decision-maker"), (GARRAN, "healer"), (VEYRA, "observer")],
            "From the hilltop Sylvi sees thin smoke two valleys north. Pip's glass reveals wooden animal masks moving around a palisade and a single unmasked man wearing a crown of living roots. With no food and no known direction to Harrowcross, the party approaches under a white cloth.",
            causes=(EV_SURFACE,), story_points=(SP_TREE, SP_RETURN), effects=[], tags=("root-host", "camp")),
        event_op(EV_ENTER_CAMP, "The Frontiersmen Enter Root-Crowned Camp", 160, 1, ROOT_CAMP,
            [(member, "approacher") for member in PARTY],
            "With no food, road, or safer human refuge in sight, the five frontiersmen descend under a white cloth into the masked camp. The host watches without answering.",
            causes=(EV_SEE_CAMP,), story_points=(SP_TREE,), effects=party_move_effects("approach-camp", ROOT_CAMP), tags=("root-host", "camp", "approach")),
        scene_record(SC_GALLERY, "Wretch Gallery", status="closed", start=t(136), current=t(152), end=t(152), location=WRETCH_GALLERY,
            participants=[participant(member, "fleeing-explorer", t(136), t(152), pov=(member == GARRAN)) for member in PARTY],
            objects=(CAVE_MAP, AMBER_RELIQUARY, MINE_LAMP, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_PHASE,),
            story_points=(SP_GALLERY, SP_SURFACE), conversations=(),
            observations=(
                observation("gallery-forms", 136, 10, ("participants",), "Each wretch shifts between predator and prey anatomy while retaining the same moving amber outline."),
                observation("gallery-rhea-fear", 140, 0, (RHEA,), "When Rhea imagines losing the others, the nearest wretch grows five human hands around its throat."),
                observation("gallery-sylvi-route", 144, 0, (SYLVI,), "A cold draft carries spruce pollen through a vertical carving shaped like a tree trunk."),
                observation("gallery-garran-threshold", 148, 0, (GARRAN,), "The open reliquary lattice makes the pack hesitate at an invisible moving boundary."),
                observation("gallery-veyra-shadows", 151, 0, (VEYRA,), "Every fresh panic adds a new shadow-layer inside the lattice; controlled breathing stops the multiplication."),
            ),
            constraints=("The pack responds to felt fear, not merely spoken fear.", "The party escapes by emotional discipline and spatial ingenuity rather than killing every manifestation."),
            body="The gallery stress-tests the new amber theory under pursuit. Knowledge helps only when the party changes how it feels and moves, not when it becomes exposition.", tags=("wretch-pack", "escape")),
        scene_record(SC_HILL, "Hilltop Dawn", status="active", start=t(158), current=t(160), end=None, location=HILLTOP,
            participants=[participant(member, "stranded-frontiersman", t(158), pov=(member == SYLVI)) for member in PARTY],
            objects=(CAVE_MAP, AMBER_RELIQUARY, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_HILL,),
            story_points=(SP_SURFACE, SP_TREE, SP_RETURN, SP_WHAT_AMBER), conversations=(),
            observations=(
                observation("hill-no-landmark", 158, 0, ("participants",), "The hill overlooks forest in every direction without road smoke, farm clearing, or the River Keld."),
                observation("hill-sylvi-wrong-stars", 158, 20, (SYLVI,), "The dawn angle places them far north and east of every mapped road in Pip's case."),
                observation("hill-pip-map", 159, 0, (PIP,), "Jorund's map and Pip's underground route cannot be reconciled with any known surface contour."),
                observation("hill-smoke", 160, 0, ("participants",), "A narrow column of smoke rises two valleys north from a camp where animal-faced figures move in silence."),
                observation("hill-veyra-crown", 160, 10, (VEYRA,), "The unmasked figure's root crown contains amber that pulses in time with the reliquary under her coat."),
            ),
            constraints=("The party does not know the Tree King's identity or Moth's role.", "Approaching the camp is a survival decision under hunger and isolation, not implausible trust."),
            body="The underground arc ends with apparent escape and a worse isolation. The first human camp in sight belongs to someone already resonating with what the party carries.", tags=("surface", "active")),
    ])

    # Knowledge acquired below.
    underground_knowledge = [
        (RHEA, "amber.root-memory-warning", "The Sunken Hall tablet says amber is golden sap through which a buried root remembers wounds, and fresh terror can return hunter and hunted without distinction.", "accepted", 0.92),
        (SYLVI, "animals.avoid-amber-network", "Living cave animals avoid amber-lit roots while remembered animal shapes move within them.", "accepted", 0.95),
        (GARRAN, "wretches.not-conscious-dead", "Wretches are repeated forms unable to distinguish hunter from hunted, not necessarily surviving conscious souls.", "suspected", 0.82),
        (VEYRA, "amber.stores-lived-impressions", "Amber stores structured lived impressions inside a larger root network and can project them as moving forms.", "accepted", 0.93),
        (PIP, "amber-routes-react-to-memory", "Amber routes react to emotionally genuine recollection and can be quieted or redirected by ancient lattice mechanisms.", "accepted", 0.9),
    ]
    for index, (character, key, statement, state, confidence) in enumerate(underground_knowledge):
        operations.append(knowledge_record(eid("knowledge", f"underground-{character}"), f"{CHARACTERS[character]['title']} forms an underground amber theory", character, key, statement, 152, index, state=state, confidence=confidence, acquisition="inferred", truth="true" if state == "accepted" else "unknown", subject=AMBER_RELIQUARY, predicate="reveals", object_value={"text": "the buried memory-root network"}, causing_event=EV_GALLERY_ESCAPE, source_entity=RUIN_TABLET, tags=("amber", "ruins")))
    operations.extend([
        knowledge_record(eid("knowledge", "rhea-guild-withheld-rules"), "Rhea suspects the guild concealed amber safety rules", RHEA, "guild.concealed.amber-rules", "Lantern Pike possessed operational handling rules for quick amber but did not provide them to the hunters, watch, or mine crews.", 116, 10, state="suspected", confidence=0.88, acquisition="inferred", truth="true", subject=HALRIC, predicate="concealed", object_value={"text": "quick-amber handling rules"}, causing_event=EV_FIND_CAMP, source_entity=DOWSING_FORK, tags=("guild-secret",)),
        knowledge_record(eid("knowledge", "party-isolated-hill"), "Rhea knows the party is isolated beyond mapped roads", RHEA, "party.isolated.deep-frontier", "The party emerged far beyond every mapped road and cannot identify a direct route back to Harrowcross.", 159, 0, confidence=0.98, acquisition="observed", truth="true", subject=HILLTOP, predicate="isolates", object_value={"text": "the party from mapped settlements"}, causing_event=EV_SURFACE, source_entity=CAVE_MAP, tags=("isolation",)),
    ])

    # Relationship changes under shared survival.
    full = lambda **kw: {"trust": kw.get("trust", 0.0), "affinity": kw.get("affinity", 0.0), "fear": kw.get("fear", 0.0), "obligation": kw.get("obligation", 0.0), "respect": kw.get("respect", 0.0), "dependence": kw.get("dependence", 0.0)}
    operations.extend([
        relationship_update(RHEA, PIP, [relationship_transition("below-rhea-pip", 152, 0, full(trust=0.68, affinity=0.28, fear=0.02, obligation=0.16, respect=0.7, dependence=0.46), ["trusted-mapper", "keeps-exits-real"], "Pip preserves a navigable route and finds the surface shaft while frightened.", causing_event=EV_GALLERY_ESCAPE)]),
        relationship_update(PIP, RHEA, [relationship_transition("below-pip-rhea", 152, 1, full(trust=0.64, affinity=0.34, fear=0.04, obligation=0.12, respect=0.62, dependence=0.32), ["leader-keeps-promises", "uses-his-map"], "Rhea treats the map as a shared promise rather than Pip's private trick.", causing_event=EV_GALLERY_ESCAPE)]),
        relationship_update(RHEA, VEYRA, [relationship_transition("below-rhea-veyra", 152, 2, full(trust=0.38, affinity=0.08, fear=0.26, obligation=0.1, respect=0.58, dependence=0.34), ["essential-arcanist", "amber-susceptible"], "Veyra decodes the ruins and carries the reliquary, but the material answers her too readily.", causing_event=EV_RELIQUARY_OPEN)]),
        relationship_update(VEYRA, RHEA, [relationship_transition("below-veyra-rhea", 152, 3, full(trust=0.58, affinity=0.18, fear=0.08, obligation=0.12, respect=0.64, dependence=0.3), ["protective-skeptic", "limits-experiment"], "Rhea prevents experimentation until survival makes the reliquary necessary.", causing_event=EV_RELIQUARY_OPEN)]),
        relationship_update(GARRAN, VEYRA, [relationship_transition("below-garran-veyra", 136, 0, full(trust=0.5, affinity=0.2, fear=0.02, obligation=0.08, respect=0.62, dependence=0.22), ["theory-changed-faith", "truthful-translator"], "Veyra translates evidence that forces Garran to abandon the easy soul explanation.", causing_event=EV_TABLET_READ)]),
        relationship_update(SYLVI, PIP, [relationship_transition("below-sylvi-pip", 152, 4, full(trust=0.56, affinity=0.22, fear=0.0, obligation=0.08, respect=0.6, dependence=0.32), ["air-and-map-partners", "reliable-under-dark"], "Sylvi's airflow and Pip's map become one route rather than competing methods.", causing_event=EV_GALLERY_ESCAPE)]),
    ])

    # Story-point history through apparent surface escape.
    operations.extend([
        story_update(SP_ENTER_MINE, [story_transition("enter-mine-active", 84, 30, "active", EV_MINE_APPROACH, "The party wins access to the sealed drift."), story_transition("enter-mine-resolved", 86, 0, "resolved", EV_ENTER_MINE, "The company crosses the green lamp line.")], outcomes=(EV_ENTER_MINE,)),
        story_update(SP_CAVEIN, [story_transition("cavein-active", 91, 0, "active", EV_CAVEIN, "The mine collapses between the party and the surface."), story_transition("cavein-resolved", 101, 0, "resolved", EV_REGROUP, "The party finds lower air and a viable route away from the collapse.")], outcomes=(EV_REGROUP,)),
        story_update(SP_CAVERNS, [story_transition("caverns-active", 101, 0, "active", EV_REGROUP, "The party enters unmapped lower caverns."), story_transition("caverns-resolved", 122, 20, "resolved", EV_HALL_FOUND, "The lower route reaches constructed ruins and a new navigational frame.")], outcomes=(EV_BLACKWATER_CROSS, EV_FIND_CAMP, EV_HALL_FOUND)),
        story_update(SP_RUINS, [story_transition("ruins-active", 122, 20, "active", EV_HALL_FOUND, "The party begins reading the Sunken Hall."), story_transition("ruins-resolved", 124, 15, "resolved", EV_TABLET_READ, "Veyra translates the central amber warning.")], outcomes=(EV_TABLET_READ,)),
        story_update(SP_GALLERY, [story_transition("gallery-active", 136, 10, "active", EV_WRETCH_PACK, "The reliquary wakes a gallery of remembered beasts."), story_transition("gallery-resolved", 151, 20, "resolved", EV_GALLERY_ESCAPE, "The party reaches the surface shaft.")], outcomes=(EV_GALLERY_ESCAPE,)),
        story_update(SP_SURFACE, [story_transition("surface-active", 151, 20, "active", EV_GALLERY_ESCAPE, "A cold shaft offers the first surface route."), story_transition("surface-resolved", 158, 0, "resolved", EV_SURFACE, "The party emerges onto the isolated hilltop.")], outcomes=(EV_SURFACE,)),
        story_update(SP_TREE, [story_transition("tree-active", 160, 0, "active", EV_SEE_CAMP, "The only nearby human camp belongs to a masked host and an amber-crowned man.")]),
        story_update(SP_RETURN, [story_transition("return-active", 158, 0, "active", EV_SURFACE, "The party is alive but isolated beyond mapped roads.")]),
        story_update(SP_WHAT_AMBER, [story_transition("what-amber-active", 72, 45, "active", EV_TILLO_DOWSES, "Tillo supplies a partial operational vocabulary for quick amber."), story_transition("what-amber-resolved", 124, 15, "resolved", EV_TABLET_READ, "The tablet and underground behavior support a root-memory theory.")], outcomes=(EV_TABLET_READ,)),
    ])
    return operations


def act4_operations(world_id: str) -> list[dict[str, Any]]:
    """Root Host captivity, Tree King blood sport, silent rescue, and pursuit."""
    operations: list[dict[str, Any]] = []

    # Remove Moth's author-secret true name from public aliases. The canonical
    # source keeps it in author-only prose, not character-safe identity metadata.
    moth_values = dict(CHARACTERS[MOTH])
    moth_values["aliases"] = ()
    operations.append(character_record(MOTH, **moth_values))

    # The hilltop watch hands off to the camp approach after the smoke sighting.
    # Keep the scene boundary location-bounded: the party cannot be in the
    # hilltop scene and the Root Camp scene at the same story coordinate.
    operations.append(scene_record(
        SC_HILL, "Hilltop Dawn", status="closed", start=t(158), current=t(160), end=t(160), location=HILLTOP,
        participants=[participant(member, "stranded-frontiersman", t(158), t(160), pov=(member == SYLVI)) for member in PARTY],
        objects=(CAVE_MAP, AMBER_RELIQUARY, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_HILL,),
        story_points=(SP_SURFACE, SP_TREE, SP_RETURN, SP_WHAT_AMBER), conversations=(),
        observations=(
            observation("hill-no-landmark", 158, 0, ("participants",), "The hill overlooks forest in every direction without road smoke, farm clearing, or the River Keld."),
            observation("hill-sylvi-wrong-stars", 158, 20, (SYLVI,), "The dawn angle places them far north and east of every mapped road in Pip's case."),
            observation("hill-pip-map", 159, 0, (PIP,), "Jorund's map and Pip's underground route cannot be reconciled with any known surface contour."),
            observation("hill-smoke", 160, 0, ("participants",), "A narrow column of smoke rises two valleys north from a camp where animal-faced figures move in silence."),
            observation("hill-veyra-crown", 160, 0, (VEYRA,), "The unmasked figure's root crown contains amber that pulses in time with the reliquary under her coat."),
        ),
        constraints=("The party approaches because it lacks food, direction, and another human option.", "Moth's identity remains author-only."),
        body="The apparent escape ends with a rational but perilous approach to the only visible camp. Silence, animal masks, and amber resonance foreshadow capture without giving the party a plausible alternative.", tags=("surface", "closed"),
    ))

    # Capture and confiscation.
    captured_items = [CONTRACT, AMBER_WAGES, WRETCH_RESIDUE, CAVE_MAP, AMBER_RELIQUARY, MINE_LAMP, BROKEN_SPEAR, *BADGES.values()]
    confiscation_effects: list[dict[str, Any]] = []
    for index, item in enumerate(captured_items):
        confiscation_effects.extend([
            effect(f"confiscate-{index}-holder", item, "holder", "clear"),
            effect(f"confiscate-{index}-location", item, "location", "clear"),
            effect(f"confiscate-{index}-container", item, "container", "clear"),
            effect(f"confiscate-{index}-cache", item, "container", "set", {"entity": GEAR_CACHE}),
        ])
    operations.extend([
        event_op(EV_CAPTURED, "The Root Host Captures the Frontiersmen", 163, 20, ROOT_CAMP,
            [(ROOTJAW, "ambusher"), (MOTH, "masked-acolyte"), *[(member, "captured-outsider") for member in PARTY]],
            "The company descends under a white cloth. Masked figures rise from spruce wells and moss-covered pits without speech or warning. Rootjaw strikes the earth once; root cords snap around ankles and wrists while Moth catches the reliquary before it hits stone.",
            causes=(EV_ENTER_CAMP,), story_points=(SP_TREE, SP_BLOOD, SP_MOTH), effects=party_condition_effects("capture-party", "bound and exhausted"), tags=("capture", "root-host")),
        event_op(EV_STRIPPED, "The Root Host Confiscates the Party's Gear", 164, 10, ROOT_CAMP,
            [(ROOTJAW, "gaoler"), (MOTH, "silent-sorter"), *[(member, "prisoner") for member in PARTY]],
            "The masked host removes weapons, maps, badges, amber, and the Sunken Hall reliquary. They sort the gear by smell and resonance rather than ownership. Moth hides the bone key while placing Pip's map on top of the bundle.",
            causes=(EV_CAPTURED,), story_points=(SP_MOTH, SP_BLOOD), effects=[*confiscation_effects, effect("gear-cache-full", GEAR_CACHE, "condition", "set", "contains the party's confiscated equipment")], tags=("confiscation", "root-host")),
        scene_record(SC_CAPTURE, "Camp Under Wooden Faces", status="closed", start=t(160, 1), current=t(166), end=t(166), location=ROOT_CAMP,
            participants=[participant(ROOTJAW, "masked-champion", t(163), t(166)), participant(MOTH, "masked-acolyte", t(163), t(166)), *[participant(member, "captive", t(160, 1), t(166), pov=(member == SYLVI)) for member in PARTY]],
            objects=(GEAR_CACHE, WOOD_MASK, ROOT_CUDGEL), environments=(ENV_CAMP,),
            story_points=(SP_TREE, SP_BLOOD, SP_MOTH), conversations=(),
            observations=(
                observation("capture-silent-host", 160, 30, ("participants",), "Dozens of wooden animal faces turn toward the descending party, but not one camp voice answers the white cloth."),
                observation("capture-mask-terror", 163, 20, ("participants",), "The masks exaggerate prey eyes and predator mouths into expressions no living animal could hold."),
                observation("capture-rootjaw-grunt", 163, 25, ("participants",), "Rootjaw gives one low grunt and six hidden acolytes move at once."),
                observation("capture-sylvi-no-camp", 164, 0, (SYLVI,), "The camp has human tracks but no ordinary domestic pattern: no children, livestock, loose tools, or unmasked sleepers."),
                observation("capture-pip-knot", 164, 10, (PIP,), "The cord around the gear bundle uses an obsolete Lantern Pike field knot."),
                observation("capture-veyra-resonance", 164, 10, (VEYRA,), "The amber behind every mask brightens when the Sunken Hall reliquary passes nearby."),
                observation("capture-moth-hand", 164, 20, (RHEA, PIP), "The moth-notched acolyte is missing one finger and hesitates before tightening Rhea's wrist cord."),
            ),
            constraints=("The Root Host remains mute except for nonverbal grunts.", "The party has not yet heard the Tree King's name or purpose."),
            body="The capture is spatially earned by the host's prepared pits and the party's exhaustion. The camp itself is not a settlement but a military organism built around masks and silence.", tags=("capture", "root-host")),
    ])

    # The Tree King's court and blood-sport decree.
    tree_turns = [
        turn("tree:0", 167, 0, TREE_KING, "Welcome, Lantern Pike. Halric sends better questions than soldiers now.", delivery="courtly"),
        turn("tree:1", 167, 10, RHEA, "He did not send us to you.", delivery="flat"),
        turn("tree:2", 167, 20, TREE_KING, "He sends everyone to me when he sends them after what amber remembers.", delivery="pleased"),
        turn("tree:3", 167, 30, PIP, "You know the guild knot.", delivery="careful"),
        turn("tree:4", 167, 40, TREE_KING, "I tied it before your lodge-master learned to make silence look like policy. Aldren Veyl, then. Tree King, now.", delivery="grand"),
        turn("tree:5", 168, 0, VEYRA, "The crown is routing impressions from the masks into you.", delivery="fascinated despite herself"),
        turn("tree:6", 168, 10, TREE_KING, "Communion. The Frontier lends me every hunted life the guild grinds into coin.", delivery="reverent"),
        turn("tree:7", 168, 20, GARRAN, "A chorus is not consent. Memory is not a crown.", delivery="firm"),
        turn("tree:8", 168, 30, ROOTJAW, "[a low warning grunt]", delivery="nonverbal"),
        turn("tree:9", 168, 40, TREE_KING, "The masks do not require consent from what is already dead.", delivery="mild"),
        turn("tree:10", 169, 0, SYLVI, "Some of it is not dead. The fish avoided the roots. The forest avoids your camp.", delivery="quiet"),
        turn("tree:11", 169, 10, TREE_KING, "The weak always call attention to wisdom when they mean fear.", delivery="dismissive"),
        turn("tree:12", 169, 20, RHEA, "What do you want from us?", delivery="direct"),
        turn("tree:13", 169, 30, TREE_KING, "Blood, terror, contest. The survivor strengthens my host. The fallen strengthen it more completely.", delivery="ceremonial"),
        turn("tree:14", 169, 40, GARRAN, "You make murder into a tool and call the tool a kingdom.", delivery="cold"),
        turn("tree:15", 170, 0, TREE_KING, "I make strength from what the guild wastes. You will enter the ring tomorrow and teach the masks five new shapes.", delivery="final"),
        turn("tree:16", 170, 10, VEYRA, "And if the amber chooses a different vessel?", delivery="testing"),
        turn("tree:17", 170, 20, TREE_KING, "Then I will finally have an heir worth fearing.", delivery="delighted"),
        turn("tree:18", 170, 30, RHEA, "No.", delivery="quiet"),
        turn("tree:19", 170, 40, TREE_KING, "The ring is more patient than you are.", delivery="amused"),
    ]
    operations.extend([
        event_op(EV_TREE_AUDIENCE, "The Tree King Claims Communion with the Frontier", 167, 0, ROOT_COURT,
            [(TREE_KING, "self-crowned-king"), (ROOTJAW, "champion"), (MOTH, "silent-acolyte"), *[(member, "captive") for member in PARTY]],
            "The captives are brought before a throne grown from living spruce roots. The Tree King identifies himself as former Lantern Pike field-master Aldren Veyl, accuses Halric of selling damped memory as currency, and demonstrates that his root crown resonates with the party's reliquary.",
            causes=(EV_CAPTURED, EV_STRIPPED, EV_COMPACT), story_points=(SP_TREE, SP_GUILD_SECRET, SP_WHAT_AMBER), effects=[
                *party_move_effects("audience-party", ROOT_COURT, "bound before tree king"),
                effect("audience-tree", TREE_KING, "location", "set", {"entity": ROOT_COURT}),
                effect("audience-rootjaw", ROOTJAW, "location", "set", {"entity": ROOT_COURT}),
                effect("audience-moth", MOTH, "location", "set", {"entity": ROOT_COURT}),
            ], tags=("tree-king", "guild-secret")),
        event_op(EV_BLOODSPORT, "The Tree King Decrees Blood Sport", 169, 30, ROOT_COURT,
            [(TREE_KING, "decreeing-king"), (ROOTJAW, "champion"), *[(member, "condemned-captive") for member in PARTY]],
            "Aldren orders the five outsiders into the Blood Ring. He states that fear and injury strengthen amber-backed masks, while death leaves a fuller impression. The party is to fight Rootjaw and whatever the ring calls until one side no longer rises.",
            causes=(EV_TREE_AUDIENCE,), story_points=(SP_BLOOD, SP_TREE), effects=[], tags=("blood-sport", "sentence")),
        event_op(EV_ROOTJAW, "Rootjaw Demonstrates the Amber Mask", 171, 20, BLOOD_RING,
            [(ROOTJAW, "champion"), (TREE_KING, "observer"), *[(member, "forced-witness") for member in PARTY]],
            "Rootjaw cuts a captive boar, presses the blood to the amber plate inside his mask, and fights the wretch that rises from the animal's panic. When he crushes it, the mask plays the boar's scream and his shoulders swell beneath the rawhide.",
            causes=(EV_BLOODSPORT,), story_points=(SP_BLOOD, SP_WHAT_AMBER), effects=[
                effect("rootjaw-strength", ROOTJAW, "condition", "set", "further strengthened by blood-ring impression"),
                effect("wood-mask-clear", WOOD_MASK, "location", "clear"),
                effect("wood-mask-rootjaw", WOOD_MASK, "holder", "set", {"entity": ROOTJAW}),
                effect("wood-mask-state", WOOD_MASK, "condition", "set", "holding fresh boar terror"),
            ], tags=("blood-ring", "demonstration")),
        conversation_op(CV_TREE, "The Tree King's Offer", status="closed", start=t(167), end=t(170, 40), scene=SC_AUDIENCE, location=ROOT_COURT,
            participants=[participant(TREE_KING, "king", t(167), t(170, 40)), participant(ROOTJAW, "champion", t(167), t(170, 40)), participant(MOTH, "silent-acolyte", t(167), t(170, 40)), *[participant(member, "captive", t(167), t(170, 40)) for member in PARTY]],
            turns=tree_turns, topics=("tree-king", "amber", "blood-sport", "guild-secret"), body="Aldren names his former guild role, offers his theology of communion, tempts Veyra, and condemns the party to blood sport."),
        scene_record(SC_AUDIENCE, "Audience of the Tree King", status="closed", start=t(167), current=t(172), end=t(172), location=ROOT_COURT,
            participants=[participant(TREE_KING, "king", t(167), t(172)), participant(ROOTJAW, "champion", t(167), t(172)), participant(MOTH, "silent-acolyte", t(167), t(172)), *[participant(member, "captive", t(167), t(172), pov=(member == VEYRA)) for member in PARTY]],
            objects=(ROOT_CROWN, ROOT_CUDGEL, WOOD_MASK, BLOOD_CHAINS, GEAR_CACHE), environments=(ENV_CAMP, ENV_RING),
            story_points=(SP_TREE, SP_BLOOD, SP_GUILD_SECRET, SP_WHAT_AMBER), conversations=(CV_TREE,),
            observations=(
                observation("audience-crown", 167, 0, ("participants",), "Raw amber in the root crown brightens in different places as masked acolytes shift behind the prisoners."),
                observation("audience-pip-knot", 167, 35, (PIP,), "Aldren's old hood carries the same field knot as the Lantern Pike gear bundle, worn and re-tied many times."),
                observation("audience-veyra-route", 168, 0, (VEYRA,), "The crown draws pressure from each mask through root cords under the court and returns a stronger pulse."),
                observation("audience-garran-voices", 168, 20, (GARRAN,), "Several masks exhale animal panic when Aldren calls their stored forms communion."),
                observation("audience-sylvi-moth", 170, 30, (SYLVI,), "The moth-notched acolyte flinches when Rhea refuses, then deliberately looks away from the pen gate."),
                observation("audience-rhea-count", 171, 20, (RHEA,), "Rootjaw's demonstration leaves seventeen armed masks around the ring and only four guarding the gear shelter."),
            ),
            constraints=("Aldren's communion is materially real but his claim of chosenness remains interpretation.", "Moth never speaks.", "The party receives testimony about the Blackroot concealment but no documentary proof."),
            body="The antagonist articulates a coherent indictment of the guild and a monstrous conclusion: because amber stores violence, he believes more violence is the proper means of sovereignty.", tags=("tree-king", "blood-sport")),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "The Tree King is former Lantern Pike field-master Aldren Veyl. He uses blood sport to feed terror and defeated forms into amber-backed masks and plans to use the party tomorrow.", "Aldren knows real guild secrets but has turned that knowledge into a system of domination.", "controlled hatred", 0.96, (tree_turns[4]["id"], tree_turns[13]["id"], tree_turns[18]["id"])),
        (SYLVI, "Aldren claims the Frontier lends him hunted lives, but the living forest avoids his camp and Moth reacted to Rhea's refusal.", "His communion is extraction from the forest, not mutual relation with it.", "ecological revulsion", 0.9, (tree_turns[6]["id"], tree_turns[10]["id"])),
        (GARRAN, "Aldren treats remembered forms as dead matter that cannot withhold consent and strengthens masks through blooded combat.", "He has confused access to pain with moral authority over it.", "righteous anger", 0.94, (tree_turns[7]["id"], tree_turns[9]["id"], tree_turns[14]["id"])),
        (VEYRA, "The root crown routes impressions from every mask into Aldren, and he believes amber may choose her as a stronger vessel.", "His offer proves amber can distribute memory through a controlled lattice, but accepting his premise would make people into inputs.", "temptation recognized as predation", 0.86, (tree_turns[5]["id"], tree_turns[16]["id"], tree_turns[17]["id"])),
        (PIP, "Aldren Veyl tied Lantern Pike field knots before Halric and openly describes Halric's concealment as policy.", "The former field-master is a living witness to the guild secret, though not a trustworthy one.", "fear sharpened into evidence", 0.9, (tree_turns[3]["id"], tree_turns[4]["id"])),
        (TREE_KING, "The new Lantern Pike company carries a living reliquary, a translator the amber answers, and a leader who refuses ceremonial fear.", "Their resistance will either produce unusually strong masks or a worthy successor.", "grandiose anticipation", 0.99, (tree_turns[5]["id"], tree_turns[18]["id"])),
        (MOTH, "Rhea refused the Tree King quietly after learning the ring would consume the party's fear and dead shapes.", "An unmasked outsider may still choose something the mask cannot predict.", "terrified hope", 0.72, (tree_turns[13]["id"], tree_turns[18]["id"])),
    ]):
        operations.append(recollection_op(CV_TREE, f"tree-{index}", character, 173, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    # Captive planning and the silent acolyte's intervention.
    pens_turns = [
        turn("pens:0", 175, 0, PIP, "The pen latch is root cord around a bone tooth. I could open it if I had the tooth or a smaller hand than mine.", delivery="quiet"),
        turn("pens:1", 175, 10, RHEA, "Gear first, north palisade second. We do not enter the ring.", delivery="command"),
        turn("pens:2", 175, 20, SYLVI, "The moth mask changes guard at moonrise and never tightens the left hinge.", delivery="observant"),
        turn("pens:3", 175, 30, VEYRA, "The reliquary is under the bark shelter. The crown pulls at it even closed.", delivery="controlled"),
        turn("pens:4", 175, 40, GARRAN, "If the masks store fear, panic during escape arms the pursuit.", delivery="low"),
        turn("pens:5", 175, 50, PIP, "Good. I was worried calm had no tactical use.", delivery="whispered"),
        turn("pens:6", 176, 0, RHEA, "We leave together. No challenge, no revenge, no experiments.", delivery="firm"),
        turn("pens:7", 176, 10, VEYRA, "I agree so emphatically that I resent the order.", delivery="dry"),
        turn("pens:8", 176, 20, ROOTJAW, "[two warning grunts from beyond the pen]", delivery="nonverbal", audience=(ROOTJAW, *PARTY)),
        turn("pens:9", 176, 30, SYLVI, "Moth is watching the gear shelter, not us.", delivery="barely audible"),
        turn("pens:10", 177, 0, GARRAN, "Then someone else may also be choosing.", delivery="quiet"),
    ]
    operations.extend([
        conversation_op(CV_PENS, "Before the Blood Ring", status="closed", start=t(175), end=t(177), scene=SC_PENS, location=PRISON_PENS,
            participants=[participant(ROOTJAW, "gaoler", t(176, 20), t(176, 20)), *[participant(member, "prisoner", t(175), t(177)) for member in PARTY]],
            turns=pens_turns, topics=("escape-plan", "moth", "masks"), body="The party plans around the root latch, gear shelter, mask resonance, and the need to control fear. Moth remains outside the transcript because he never speaks."),
        event_op(EV_PEN_OPEN, "Moth Leaves the Prison Pen Open", 184, 0, PRISON_PENS,
            [(MOTH, "silent-helper"), (PIP, "watching-prisoner"), (SYLVI, "watching-prisoner")],
            "At the final dark watch, Moth replaces the root cord without seating it, leaves the carved bone key in the hinge, and gives one involuntary grunt when Pip notices. He walks away without looking back.",
            causes=(EV_BLOODSPORT,), story_points=(SP_MOTH, SP_BLOOD, SP_FLEE), effects=[
                effect("moth-key-clear", MOTH_KEY, "holder", "clear"),
                effect("moth-key-pip", MOTH_KEY, "holder", "set", {"entity": PIP}),
                effect("moth-key-state", MOTH_KEY, "condition", "set", "left in prison latch; taken by Pip"),
            ], tags=("moth", "escape")),
        scene_record(SC_PENS, "Blood Ring Pens", status="closed", start=t(173), current=t(184), end=t(184), location=PRISON_PENS,
            participants=[participant(ROOTJAW, "gaoler", t(173), t(176, 20)), participant(MOTH, "silent-acolyte", t(173), t(184)), *[participant(member, "prisoner", t(173), t(184), pov=(member == PIP)) for member in PARTY]],
            objects=(BLOOD_CHAINS, MOTH_KEY, GEAR_CACHE), environments=(ENV_RING,),
            story_points=(SP_BLOOD, SP_MOTH, SP_FLEE), conversations=(CV_PENS,),
            observations=(
                observation("pens-root-latch", 173, 10, (PIP,), "The pen is held by braided living root around a carved bone tooth, not by metal lockwork."),
                observation("pens-mask-cries", 174, 0, (GARRAN,), "Masks hanging near the ring breathe fragments of previous captives' panic whenever the drums begin."),
                observation("pens-moth-hinge", 175, 20, (SYLVI,), "Moth checks every binding except the left hinge and repeats the omission on three rounds."),
                observation("pens-veyra-pull", 175, 30, (VEYRA,), "The root crown's distant pulse tugs at the closed reliquary through the bark shelter wall."),
                observation("pens-rhea-guards", 176, 0, (RHEA,), "Only four masked guards remain between the pen, gear shelter, and north palisade during ring preparation."),
                observation("pens-open", 184, 0, (PIP, SYLVI), "Moth leaves the bone tooth unseated and the root cord resting loose across it."),
            ),
            constraints=("Moth's action is intentional, but the party cannot know why or who he was.", "The party's escape plan prioritizes cohesion over attacking Aldren."),
            body="The prison act makes the escape a product of observation, discipline, and one silent internal rebellion rather than convenient negligence.", tags=("prison", "blood-ring")),
    ])
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate([
        (RHEA, "The party agreed to recover essential gear and leave together without entering the ring or attacking Aldren.", "Fear control is now a material part of tactics, not only morale.", "command under restraint", 0.97, (pens_turns[1]["id"], pens_turns[6]["id"])),
        (SYLVI, "Moth repeatedly left the left hinge loose and watched the gear shelter rather than the prisoners.", "The acolyte may be resisting the host from inside the mask's constraints.", "cautious hope", 0.82, (pens_turns[2]["id"], pens_turns[9]["id"])),
        (GARRAN, "The masks replay prior captives' panic, so uncontrolled fear during escape could strengthen the pursuit.", "Calm has become an act of mercy toward one another and denial toward the host.", "grim purpose", 0.9, (pens_turns[4]["id"], pens_turns[10]["id"])),
        (VEYRA, "Aldren's crown continues pulling at the reliquary while it is closed and out of sight.", "The artifacts belong to compatible parts of one network; distance reduces but does not sever the link.", "temptation under discipline", 0.84, (pens_turns[3]["id"], pens_turns[7]["id"])),
        (PIP, "The root latch needs the bone tooth, Moth deliberately leaves one hinge loose, and four guards cover the gear route.", "The escape is being offered by someone who cannot speak openly and may pay for discovery.", "technical clarity and guilt", 0.91, (pens_turns[0]["id"], pens_turns[9]["id"])),
    ]):
        operations.append(recollection_op(CV_PENS, f"pens-{index}", character, 183, index, summary, interpretation, emotional, confidence, exact_turns=exact))

    # Escape and active pursuit.
    recovered = [(RHEA_BADGE, RHEA), (SYLVI_BADGE, SYLVI), (GARRAN_BADGE, GARRAN), (VEYRA_BADGE, VEYRA), (PIP_BADGE, PIP), (CAVE_MAP, PIP), (AMBER_RELIQUARY, VEYRA), (BROKEN_SPEAR, RHEA)]
    recovery_effects: list[dict[str, Any]] = []
    for index, (item, holder) in enumerate(recovered):
        recovery_effects.extend([
            effect(f"recover-{index}-container", item, "container", "clear"),
            effect(f"recover-{index}-location", item, "location", "clear"),
            effect(f"recover-{index}-holder", item, "holder", "clear"),
            effect(f"recover-{index}-set", item, "holder", "set", {"entity": holder}),
        ])
    operations.extend([
        event_op(EV_ESCAPE, "The Frontiersmen Slip Out of the Blood Ring Pens", 184, 20, PRISON_PENS,
            [(PIP, "locksmith"), (SYLVI, "watcher"), (RHEA, "leader"), (GARRAN, "warder"), (VEYRA, "arcanist")],
            "Pip lifts the unseated bone tooth and peels the living cord from the hinge. The party exits one at a time between drumbeats, breathing in count so the nearby masks do not brighten.",
            causes=(EV_PEN_OPEN,), story_points=(SP_BLOOD, SP_MOTH, SP_FLEE), effects=[*party_move_effects("escape-pen", ROOT_CAMP, "free but unarmed")], tags=("escape", "prison")),
        event_op(EV_GEAR_RECOVERED, "The Party Recovers Essential Gear", 187, 0, ROOT_CAMP,
            [(RHEA, "shield"), (SYLVI, "watcher"), (GARRAN, "carrier"), (VEYRA, "reliquary-bearer"), (PIP, "locksmith")],
            "They reach the bark shelter and cut only the outer bundle. Badges, map, broken spear, and reliquary are recovered; the signed contract, amber wages, frost sample, and empty mine lamp remain beneath the guard rack when Rootjaw returns early.",
            causes=(EV_ESCAPE, EV_STRIPPED), story_points=(SP_BLOOD, SP_FLEE), effects=[*recovery_effects, effect("gear-cache-rifled", GEAR_CACHE, "condition", "set", "rifled; contract, wages, frost sample, and empty lamp remain")], tags=("gear", "escape")),
        event_op(EV_ALARM, "The Root Host Raises the Pursuit Alarm", 189, 10, ROOT_CAMP,
            [(ROOTJAW, "alarm-champion"), (TREE_KING, "pursuing-king"), *[(member, "fleeing-prisoner") for member in PARTY]],
            "Rootjaw discovers the open pen and strikes his quickened cudgel against the root court. Every mask in the camp releases a different animal cry. Aldren answers through the crown, sending the host north and east in practiced hunting lines.",
            causes=(EV_GEAR_RECOVERED,), story_points=(SP_FLEE, SP_MOTH), effects=[effect("alarm-tree", TREE_KING, "location", "set", {"entity": ROOT_CAMP}), effect("alarm-rootjaw", ROOTJAW, "location", "set", {"entity": ROOT_CAMP})], tags=("alarm", "root-host")),
        event_op(EV_PURSUIT, "The Root Host Pursues the Frontiersmen", 191, 0, PURSUIT_TRAIL,
            [(RHEA, "rear-guard"), (SYLVI, "route-finder"), (GARRAN, "warder"), (VEYRA, "reliquary-bearer"), (PIP, "mapper"), (TREE_KING, "distant-pursuer"), (ROOTJAW, "pursuit-champion")],
            "The party crosses the north palisade and enters rain-black spruce. Behind them the host advances without spoken orders, guided by drumbeats and the brightening reliquary. Sylvi chooses a flooded game trail where masks cannot keep formation, but Rootjaw's cudgel calls wretch cries from the ground.",
            causes=(EV_ESCAPE, EV_ALARM), story_points=(SP_FLEE, SP_RETURN, SP_MOTH), effects=[
                *party_move_effects("pursuit-party", PURSUIT_TRAIL, "running under pursuit"),
                effect("pursuit-rootjaw", ROOTJAW, "location", "set", {"entity": PURSUIT_TRAIL}),
            ], tags=("pursuit", "forest")),
        scene_record(SC_ESCAPE, "The Open Pen", status="closed", start=t(184, 1), current=t(190), end=t(190), location=ROOT_CAMP,
            participants=[participant(MOTH, "silent-helper", t(184, 1), t(184, 5)), participant(ROOTJAW, "returning-gaoler", t(188), t(190)), *[participant(member, "escaping-prisoner", t(184, 1), t(190), pov=(member == PIP)) for member in PARTY]],
            objects=(MOTH_KEY, GEAR_CACHE, AMBER_RELIQUARY, CAVE_MAP, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_CAMP, ENV_RING),
            story_points=(SP_BLOOD, SP_MOTH, SP_FLEE), conversations=(),
            observations=(
                observation("escape-moth-grunt", 184, 1, (PIP, SYLVI), "Moth gives one involuntary grunt when Pip takes the bone key, then turns his mask toward the ring."),
                observation("escape-root-breath", 184, 20, ("participants",), "The loosened root cord curls back toward the empty pen as if trying to remember its shape."),
                observation("escape-gear-count", 186, 30, (RHEA,), "Four guards cover the gear shelter; two face the ring whenever the drums peak."),
                observation("escape-pip-contract", 187, 0, (PIP,), "The contract and amber wages are tied beneath the heaviest guard rack and cannot be reached without waking the camp."),
                observation("escape-veyra-reliquary", 187, 5, (VEYRA,), "The reliquary pulls toward Aldren's crown until Veyra wraps it in wet bark and charcoal cloth."),
                observation("escape-alarm", 189, 10, ("participants",), "Every wooden mask cries in a different animal voice when Rootjaw strikes the earth."),
            ),
            constraints=("Moth remains behind and his fate is unknown.", "The party recovers enough gear to survive, not every possession or piece of evidence."),
            body="The escape pays off the party's observations, emotional discipline, and Moth's silent choice. Leaving possessions behind makes the flight materially costly.", tags=("escape", "root-host")),
    ])

    pursuit_turns = [
        turn("pursuit:0", 191, 0, RHEA, "Direction.", delivery="running"),
        turn("pursuit:1", 191, 10, SYLVI, "North ridge, then east into water. The masks cannot spread in the flooded spruce.", delivery="breathless"),
        turn("pursuit:2", 191, 20, PIP, "My map has achieved complete freedom from the landscape.", delivery="running"),
        turn("pursuit:3", 191, 30, GARRAN, "Save the complaint for ground that is not trying to remember our feet.", delivery="strained"),
        turn("pursuit:4", 191, 40, VEYRA, "The reliquary is pulling west. Aldren is using it as a compass.", delivery="alarmed"),
        turn("pursuit:5", 191, 50, RHEA, "Can you break the link without opening it?", delivery="direct"),
        turn("pursuit:6", 192, 0, VEYRA, "Charcoal, running water, and no terror. I can offer two of those.", delivery="grim"),
        turn("pursuit:7", 192, 10, SYLVI, "Stream ahead. Thirty breaths.", delivery="certain"),
        turn("pursuit:8", 192, 20, PIP, "Moth's key has three notches. One is shaped like a road marker.", delivery="observing"),
        turn("pursuit:9", 192, 30, GARRAN, "Then remember the helper without inventing the reason.", delivery="firm"),
        turn("pursuit:10", 193, 0, RHEA, "We owe him an escape, not a name.", delivery="controlled"),
        turn("pursuit:11", 193, 10, ROOTJAW, "[a distant boar-like bellow through the rain]", delivery="nonverbal", audience=(ROOTJAW, *PARTY)),
        turn("pursuit:12", 194, 0, SYLVI, "They have cut south of us. The host knows the water route.", delivery="urgent"),
        turn("pursuit:13", 194, 10, PIP, "Then the third notch may be theirs, not ours.", delivery="suddenly serious"),
        turn("pursuit:14", 195, 0, RHEA, "Keep it. Keep moving. We learn what it opens when we are not being hunted.", delivery="command"),
    ]
    operations.extend([
        conversation_op(CV_PURSUIT, "Running Under the Drums", status="active", start=t(191), end=None, scene=SC_PURSUIT, location=PURSUIT_TRAIL,
            participants=[participant(ROOTJAW, "distant-pursuer", t(193, 10)), *[participant(member, "fleeing-frontiersman", t(191)) for member in PARTY]],
            turns=pursuit_turns, topics=("pursuit", "moth-key", "reliquary"), body="The active escape conversation keeps route choice, artifact resonance, and the unknown helper in motion while the host closes around the party."),
        scene_record(SC_PURSUIT, "The Hunt Begins", status="active", start=t(191), current=t(195), end=None, location=PURSUIT_TRAIL,
            participants=[participant(ROOTJAW, "distant-pursuer", t(193, 10)), *[participant(member, "fleeing-frontiersman", t(191), pov=(member == RHEA)) for member in PARTY]],
            objects=(MOTH_KEY, AMBER_RELIQUARY, CAVE_MAP, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_PURSUIT,),
            story_points=(SP_FLEE, SP_MOTH, SP_RETURN, SP_GUILD_SECRET), conversations=(CV_PURSUIT,),
            observations=(
                observation("pursuit-rain", 191, 0, ("participants",), "Cold rain erases ordinary tracks but every reliquary pulse leaves a brief gold reflection in puddles."),
                observation("pursuit-sylvi-lines", 191, 10, (SYLVI,), "Masked pursuit lines remain unnaturally straight until forced into flooded spruce."),
                observation("pursuit-pip-key", 192, 20, (PIP,), "Moth's bone key has three notches: pen tooth, triangular road mark, and an unfamiliar hooked cut."),
                observation("pursuit-veyra-compass", 191, 40, (VEYRA,), "The closed reliquary tugs west in time with distant root-crown pulses."),
                observation("pursuit-garran-voices", 193, 10, (GARRAN,), "Rootjaw's distant bellow carries several older animal cries under the boar voice."),
                observation("pursuit-rhea-cutoff", 194, 0, (RHEA,), "A second drumline sounds south of the party, proving the host knows or predicts the water route."),
            ),
            constraints=("The Root Host is actively pursuing; the party has not escaped the campaign threat.", "Moth's identity and the key's other uses remain unresolved.", "The route back to Harrowcross remains unknown."),
            body="The requested story ends in motion: the company has escaped captivity but not the Frontier, and the enemy's silent host is adapting to every route choice.", tags=("active", "pursuit")),
    ])

    # Knowledge and recollection after capture.
    shared_truths = [
        ("tree-king-aldren", "tree-king.identity.aldren-veyl", "The Tree King says he was Aldren Veyl, a former Lantern Pike field-master who opposed Halric after Blackroot.", TREE_KING, "identifies-as", {"text": "Aldren Veyl, former Lantern Pike field-master"}, EV_TREE_AUDIENCE),
        ("bloodsport-strengthens", "root-host.bloodsport.strengthens-masks", "The Tree King's blood sport uses fear, injury, and defeated manifestations to strengthen amber-backed masks and their wearers.", BLOOD_RING, "strengthens", {"entity": WOOD_MASK}, EV_ROOTJAW),
        ("moth-opened-pen", "moth.left-pen-open", "The moth-notched acolyte deliberately left the prison latch unseated and the bone key visible.", MOTH, "enabled", {"entity": EV_ESCAPE}, EV_PEN_OPEN),
    ]
    for character_index, character in enumerate(PARTY):
        for truth_index, (slug, key, statement, subject, predicate, obj, cause) in enumerate(shared_truths):
            operations.append(knowledge_record(eid("knowledge", f"{character}-{slug}"), f"{CHARACTERS[character]['title']} records {slug.replace('-', ' ')}", character, key, statement, 190, character_index * 10 + truth_index, state="accepted" if slug != "moth-opened-pen" else "suspected", confidence=0.96 if slug != "moth-opened-pen" else 0.86, acquisition="heard" if cause in {EV_TREE_AUDIENCE, EV_ROOTJAW} else "inferred", truth="true", subject=subject, predicate=predicate, object_value=obj, causing_event=cause, source_entity=MOTH_KEY if slug == "moth-opened-pen" else ROOT_CROWN, tags=("root-host",)))
    operations.extend([
        knowledge_record(eid("knowledge", "veyra-crown-network"), "Veyra understands the root crown as a mask network", VEYRA, "root-crown.routes-mask-impressions", "Aldren's root crown routes stored impressions from amber-backed masks into his body and can track compatible amber artifacts at distance.", 190, 60, confidence=0.92, acquisition="observed", truth="true", subject=ROOT_CROWN, predicate="routes-and-tracks", object_value={"entity": AMBER_RELIQUARY}, causing_event=EV_ALARM, source_entity=AMBER_RELIQUARY, tags=("amber", "root-host")),
        knowledge_record(eid("knowledge", "pip-key-road-notch"), "Pip notices a road-marker notch on Moth's key", PIP, "moth-key.has-road-notch", "Moth's bone key includes a triangular notch resembling a Frontier road marker in addition to the prison tooth.", 192, 25, confidence=0.94, acquisition="observed", truth="unknown", subject=MOTH_KEY, predicate="bears", object_value={"text": "triangular road-marker notch"}, causing_event=EV_PURSUIT, source_entity=MOTH_KEY, tags=("moth", "route")),
        knowledge_record(eid("knowledge", "rhea-guild-blackroot-testimony"), "Rhea records Aldren's accusation against Halric", RHEA, "aldren.accuses-halric.blackroot-coverup", "Aldren accuses Halric of concealing the Blackroot manifestation while continuing to sell and collect damped amber.", 173, 10, state="suspected", confidence=0.72, acquisition="told", truth="true", subject=HALRIC, predicate="concealed", object_value={"entity": EV_BLACKROOT_DISCOVERY}, causing_event=EV_TREE_AUDIENCE, source_entity=TREE_KING, tags=("guild-secret",)),
    ])

    # Relationship records beginning at actual encounter times prevent future-NPC leakage.
    full = lambda **kw: {"trust": kw.get("trust", 0.0), "affinity": kw.get("affinity", 0.0), "fear": kw.get("fear", 0.0), "obligation": kw.get("obligation", 0.0), "respect": kw.get("respect", 0.0), "dependence": kw.get("dependence", 0.0)}
    rhea_tree, tree_rhea = rel_pair(RHEA, TREE_KING)
    rhea_moth, moth_rhea = rel_pair(RHEA, MOTH)
    veyra_tree, tree_veyra = rel_pair(VEYRA, TREE_KING)
    pip_moth, moth_pip = rel_pair(PIP, MOTH)
    operations.extend([
        relationship_record(rhea_tree, "Rhea's view of the Tree King", RHEA, TREE_KING, "captive-and-tyrant", tree_rhea, [
            relationship_transition("rhea-tree-audience", 170, 35, full(trust=-0.94, affinity=-0.78, fear=0.48, obligation=0.0, respect=0.28, dependence=0.0), ["captor", "former-guild", "amber-tyrant"], "Aldren combines real evidence with blood-sport domination.", causing_event=EV_BLOODSPORT),
            relationship_transition("rhea-tree-pursuit", 191, 10, full(trust=-1.0, affinity=-0.84, fear=0.62, obligation=0.0, respect=0.34, dependence=0.0), ["pursuing-tyrant", "strategic-enemy"], "The Root Host hunts the party through terrain Aldren understands better.", causing_event=EV_PURSUIT),
        ], "Rhea's hostility begins only after actual encounter and grows into strategic fear during pursuit."),
        relationship_record(tree_rhea, "The Tree King's view of Rhea", TREE_KING, RHEA, "tyrant-and-challenger", rhea_tree, [
            relationship_transition("tree-rhea-audience", 170, 35, full(trust=-0.5, affinity=0.08, fear=0.02, obligation=0.0, respect=0.52, dependence=0.0), ["candidate-champion", "defiant-leader"], "Rhea's quiet refusal makes her valuable blood-ring material.", causing_event=EV_BLOODSPORT),
            relationship_transition("tree-rhea-escape", 189, 20, full(trust=-0.82, affinity=-0.06, fear=0.16, obligation=0.0, respect=0.66, dependence=0.0), ["escaped-challenger", "must-be-recaptured"], "Her cohesive escape proves the host did not fully master her fear.", causing_event=EV_ALARM),
        ], "Aldren values Rhea as a challenger and later as a threat to the authority of the ring."),
        relationship_record(rhea_moth, "Rhea's view of Moth", RHEA, MOTH, "escaped-prisoner-and-unknown-helper", moth_rhea, [
            relationship_transition("rhea-moth-capture", 164, 25, full(trust=-0.18, affinity=0.0, fear=0.24, obligation=0.0, respect=0.02, dependence=0.0), ["masked-gaoler", "hesitating"], "Moth hesitates while binding her but remains part of the captor host.", causing_event=EV_STRIPPED),
            relationship_transition("rhea-moth-open", 190, 0, full(trust=0.34, affinity=0.06, fear=0.16, obligation=0.52, respect=0.42, dependence=0.0), ["unknown-helper", "left-behind"], "The deliberate open pen creates a debt without revealing motive or identity.", causing_event=EV_PEN_OPEN),
        ], "Rhea moves from suspicion to indebted uncertainty after the silent rescue."),
        relationship_record(moth_rhea, "Moth's view of Rhea", MOTH, RHEA, "bound-acolyte-and-outsider", rhea_moth, [
            relationship_transition("moth-rhea-audience", 170, 35, full(trust=0.42, affinity=0.08, fear=0.3, obligation=0.4, respect=0.5, dependence=0.08), ["possible-escapee", "refuses-king"], "Rhea refuses the blood ring without bargaining for herself.", causing_event=EV_BLOODSPORT),
            relationship_transition("moth-rhea-open", 184, 5, full(trust=0.58, affinity=0.1, fear=0.44, obligation=0.66, respect=0.56, dependence=0.1), ["escape-bearer", "last-chance"], "Moth risks the key on a company that may expose or end the host.", causing_event=EV_PEN_OPEN),
        ], "Moth's hope is recorded from his own perspective without giving the party his hidden identity."),
        relationship_record(veyra_tree, "Veyra's view of the Tree King", VEYRA, TREE_KING, "arcanist-and-amber-tyrant", tree_veyra, [
            relationship_transition("veyra-tree-audience", 170, 35, full(trust=-0.72, affinity=0.04, fear=0.54, obligation=0.0, respect=0.58, dependence=0.0), ["dangerous-teacher", "predatory-theorist"], "Aldren demonstrates real network control while offering Veyra a role that requires domination.", causing_event=EV_TREE_AUDIENCE),
        ], "Veyra recognizes valuable knowledge inside a fundamentally predatory practice."),
        relationship_record(tree_veyra, "The Tree King's view of Veyra", TREE_KING, VEYRA, "king-and-possible-heir", veyra_tree, [
            relationship_transition("tree-veyra-audience", 170, 35, full(trust=0.08, affinity=0.24, fear=0.08, obligation=0.0, respect=0.7, dependence=0.04), ["possible-heir", "amber-responsive"], "The reliquary and crown both answer Veyra's presence.", causing_event=EV_TREE_AUDIENCE),
        ], "Aldren interprets Veyra's susceptibility as evidence that amber chooses vessels."),
        relationship_record(pip_moth, "Pip's view of Moth", PIP, MOTH, "escapee-and-silent-helper", moth_pip, [
            relationship_transition("pip-moth-open", 184, 5, full(trust=0.48, affinity=0.08, fear=0.18, obligation=0.56, respect=0.46, dependence=0.0), ["silent-helper", "key-giver"], "Pip sees the deliberate latch error and takes the bone key.", causing_event=EV_PEN_OPEN),
        ], "Pip treats Moth's action as a designed mechanism rather than an accident."),
        relationship_record(moth_pip, "Moth's view of Pip", MOTH, PIP, "silent-helper-and-locksmith", pip_moth, [
            relationship_transition("moth-pip-open", 184, 5, full(trust=0.52, affinity=0.06, fear=0.38, obligation=0.44, respect=0.48, dependence=0.08), ["notices-openings", "takes-the-key"], "Moth selects the prisoner most likely to understand the unseated latch.", causing_event=EV_PEN_OPEN),
        ], "Moth's choice of Pip is practical, not random."),
    ])

    # Story lifecycle at the requested ending.
    operations.extend([
        story_update(SP_TREE, [story_transition("tree-active", 160, 0, "active", EV_SEE_CAMP, "The party approaches the only visible camp."), story_transition("tree-resolved", 170, 20, "resolved", EV_TREE_AUDIENCE, "Aldren reveals his guild past, amber practice, and blood-sport purpose.")], outcomes=(EV_TREE_AUDIENCE, EV_BLOODSPORT)),
        story_update(SP_BLOOD, [story_transition("blood-active", 169, 30, "active", EV_BLOODSPORT, "The party is condemned to the ring."), story_transition("blood-resolved", 184, 20, "resolved", EV_ESCAPE, "The party leaves the pens before the contest begins.")], outcomes=(EV_ESCAPE,)),
        story_update(SP_MOTH, [story_transition("moth-active", 184, 0, "active", EV_PEN_OPEN, "The moth-notched acolyte deliberately enables escape.")]),
        story_update(SP_FLEE, [story_transition("flee-active", 189, 10, "active", EV_ALARM, "The Root Host raises the camp-wide pursuit alarm.")]),
        story_update(SP_GUILD_SECRET, [story_transition("guild-secret-active", 74, 0, "active", EV_AMBER_PAYMENT, "The protection ledger makes guild dependence on amber explicit."), story_transition("guild-secret-testimony", 167, 40, "active", EV_TREE_AUDIENCE, "Aldren identifies himself as a former field-master and accuses Halric of the Blackroot concealment.")]),
        story_update(SP_RETURN, [story_transition("return-active", 158, 0, "active", EV_SURFACE, "The party is isolated beyond mapped roads and later flees farther north.")]),
    ])

    # Final world summary/profile and hybrid compilation default.
    operations.append(world_record(world_id, final=True))
    return operations


def act5_operations() -> list[dict[str, Any]]:
    """Final perspective cleanup and current chase recollections."""
    pursuit_turns = [turn(f"pursuit:{index}", tick, order, speaker, text, delivery=delivery, audience=audience) for index, (tick, order, speaker, text, delivery, audience) in enumerate([
        (191, 0, RHEA, "Direction.", "running", ("participants",)),
        (191, 10, SYLVI, "North ridge, then east into water. The masks cannot spread in the flooded spruce.", "breathless", ("participants",)),
        (191, 20, PIP, "My map has achieved complete freedom from the landscape.", "running", ("participants",)),
        (191, 30, GARRAN, "Save the complaint for ground that is not trying to remember our feet.", "strained", ("participants",)),
        (191, 40, VEYRA, "The reliquary is pulling west. Aldren is using it as a compass.", "alarmed", ("participants",)),
        (191, 50, RHEA, "Can you break the link without opening it?", "direct", ("participants",)),
        (192, 0, VEYRA, "Charcoal, running water, and no terror. I can offer two of those.", "grim", ("participants",)),
        (192, 10, SYLVI, "Stream ahead. Thirty breaths.", "certain", ("participants",)),
        (192, 20, PIP, "Moth's key has three notches. One is shaped like a road marker.", "observing", ("participants",)),
        (192, 30, GARRAN, "Then remember the helper without inventing the reason.", "firm", ("participants",)),
        (193, 0, RHEA, "We owe him an escape, not a name.", "controlled", ("participants",)),
        (193, 10, ROOTJAW, "[a distant boar-like bellow through the rain]", "nonverbal", (ROOTJAW, *PARTY)),
        (194, 0, SYLVI, "They have cut south of us. The host knows the water route.", "urgent", ("participants",)),
        (194, 10, PIP, "Then the third notch may be theirs, not ours.", "suddenly serious", ("participants",)),
        (195, 0, RHEA, "Keep it. Keep moving. We learn what it opens when we are not being hunted.", "command", ("participants",)),
    ])]
    operations: list[dict[str, Any]] = [
        scene_record(SC_PURSUIT, "The Hunt Begins", status="active", start=t(191), current=t(195), end=None, location=PURSUIT_TRAIL,
            participants=[participant(member, "fleeing-frontiersman", t(191), pov=(member == RHEA)) for member in PARTY],
            objects=(MOTH_KEY, AMBER_RELIQUARY, CAVE_MAP, BROKEN_SPEAR, *BADGES.values()), environments=(ENV_PURSUIT,),
            story_points=(SP_FLEE, SP_MOTH, SP_RETURN, SP_GUILD_SECRET), conversations=(CV_PURSUIT,),
            observations=(
                observation("pursuit-rain", 191, 0, ("participants",), "Cold rain erases ordinary tracks but every reliquary pulse leaves a brief gold reflection in puddles."),
                observation("pursuit-sylvi-lines", 191, 10, (SYLVI,), "Masked pursuit lines remain unnaturally straight until forced into flooded spruce."),
                observation("pursuit-pip-key", 192, 20, (PIP,), "Moth's bone key has three notches: pen tooth, triangular road mark, and an unfamiliar hooked cut."),
                observation("pursuit-veyra-compass", 191, 40, (VEYRA,), "The closed reliquary tugs west in time with distant root-crown pulses."),
                observation("pursuit-garran-voices", 193, 10, (GARRAN,), "Rootjaw's distant bellow carries several older animal cries under the boar voice."),
                observation("pursuit-rhea-cutoff", 194, 0, (RHEA,), "A second drumline sounds south of the party, proving the host knows or predicts the water route."),
            ),
            constraints=("Rootjaw is audible and pursuing at distance, not physically co-present with the party.", "Moth's identity and the key's other uses remain unresolved.", "The route back to Harrowcross remains unknown."),
            body="The requested story ends in motion: five exhausted hunters are physically together, while the masked host remains an audible and spatially inferred pursuit rather than a co-present scene participant.", tags=("active", "pursuit")),
    ]
    memories = [
        (RHEA, "The host anticipated the flooded route, the reliquary acts as Aldren's compass, and Moth's unknown key may encode another road.", "Rhea must keep the party moving without mistaking debt to Moth for knowledge of his plan.", "command narrowed to the next thirty breaths", 0.88, (pursuit_turns[4]["id"], pursuit_turns[10]["id"], pursuit_turns[14]["id"])),
        (SYLVI, "Flooded spruce disrupts mask formations, but a second drumline cut south before the party reached the stream.", "The host understands the terrain and may be reading likely prey routes rather than tracking footprints.", "hunted focus", 0.93, (pursuit_turns[1]["id"], pursuit_turns[12]["id"])),
        (GARRAN, "The party can deny the masks fresh strength by controlling terror, and Rootjaw's bellow contains several older captured animal cries.", "Moth should be remembered for the action actually witnessed, not turned into a comforting story.", "mercy under pursuit", 0.86, (pursuit_turns[9]["id"], pursuit_turns[11]["id"])),
        (VEYRA, "Aldren tracks the closed reliquary through root-crown resonance; charcoal and running water may interrupt the link.", "The artifact is both their best defense against wretches and the signal guiding the host.", "technical panic", 0.95, (pursuit_turns[4]["id"], pursuit_turns[6]["id"])),
        (PIP, "Moth's key includes a possible road-marker notch, but the host knew the same water route the notch may indicate.", "The key may open a route, identify one, or be deliberately legible to the host; he cannot choose among those yet.", "curiosity restrained by immediate danger", 0.84, (pursuit_turns[8]["id"], pursuit_turns[13]["id"])),
    ]
    for index, (character, summary, interpretation, emotional, confidence, exact) in enumerate(memories):
        operations.append(recollection_op(CV_PURSUIT, f"pursuit-current-{index}", character, 195, 10 + index, summary, interpretation, emotional, confidence, exact_turns=exact))
    return operations


def act6_operations(world_id: str) -> list[dict[str, Any]]:
    """Break the immediate chase and open a bounded southward continuation."""
    waymark_turns = [
        turn("waymark:0", 196, 10, RHEA, "Down. Water if it holds us, stone if it does not.", delivery="command"),
        turn("waymark:1", 196, 20, SYLVI, "Stone under it. Cut face. This was a road mark before the bog took the road.", delivery="low"),
        turn("waymark:2", 197, 0, PIP, "Triangle is the same hand as the notch on Moth's key. The hooked cut is not.", delivery="exact"),
        turn("waymark:3", 197, 10, RHEA, "Then leave the hook alone.", delivery="flat"),
        turn("waymark:4", 198, 0, VEYRA, "The current is making the closed reliquary answer from every flooded channel.", delivery="intent"),
        turn("waymark:5", 198, 10, GARRAN, "Closed remains the important word.", delivery="warning"),
        turn("waymark:6", 198, 20, VEYRA, "Closed remains the condition.", delivery="controlled"),
        turn("waymark:7", 201, 0, SYLVI, "The drums split at the alder fork. They are following three trails and none is ours.", delivery="listening"),
        turn("waymark:8", 202, 10, RHEA, "How long before they mend the mistake?", delivery="quiet"),
        turn("waymark:9", 202, 20, SYLVI, "Long enough to stop running. Not long enough to be safe.", delivery="certain"),
        turn("waymark:10", 203, 0, PIP, "There is a survey seam behind the triangle. Iron pin. No bone key required.", delivery="working"),
        turn("waymark:11", 204, 0, PIP, "Blackroot recovery, three dead, raw amber under floodwater, raw and refined kept apart. Initialed H. Doss.", delivery="reading"),
        turn("waymark:12", 204, 10, GARRAN, "It proves the guild knew what required caution after the deaths. It does not prove every silence Aldren named.", delivery="careful"),
        turn("waymark:13", 204, 20, RHEA, "Then we carry exactly what it proves.", delivery="decisive"),
        turn("waymark:14", 205, 0, PIP, "The margin gives an old warden road south. Abandoned, but drawn by someone who had met the ground.", delivery="hopeful"),
    ]
    southward_turns = [
        turn("southward:0", 207, 10, RHEA, "Report.", delivery="walking"),
        turn("southward:1", 207, 20, SYLVI, "No drum for four hundred breaths. The alder closes behind us. No fresh boot sign ahead.", delivery="measured"),
        turn("southward:2", 208, 0, VEYRA, "The reliquary is closed, charcoal-wrapped, and quiet. I am not improving that result.", delivery="dry"),
        turn("southward:3", 208, 10, PIP, "The counterfoil names Blackroot, the dead, the handling rule, and Halric's hand. It does not name a compact.", delivery="precise"),
        turn("southward:4", 208, 20, GARRAN, "Proof with its edges left on. A rare honest object.", delivery="approving"),
        turn("southward:5", 209, 0, RHEA, "Destination.", delivery="direct"),
        turn("southward:6", 209, 10, PIP, "South by the warden cuts. Harrowcross is not on the surviving line.", delivery="reluctant"),
        turn("southward:7", 209, 20, SYLVI, "Safer than crossing the host's northward search. Not safe.", delivery="plain"),
        turn("southward:8", 210, 0, RHEA, "Southward cut. We do not call it home until it brings us there.", delivery="command"),
    ]

    full = lambda **kw: {"trust": kw.get("trust", 0.0), "affinity": kw.get("affinity", 0.0), "fear": kw.get("fear", 0.0), "obligation": kw.get("obligation", 0.0), "respect": kw.get("respect", 0.0), "dependence": kw.get("dependence", 0.0)}
    operations: list[dict[str, Any]] = [
        {
            "type": "entity.update",
            "entity": SC_PURSUIT,
            "frontmatterPatch": {
                "status": "closed",
                "tags": ["pursuit"],
                "time": {"start": t(191), "current": t(195, 99), "end": t(195, 99)},
                "participants": [participant(member, "fleeing-frontiersman", t(191), t(195, 99), pov=(member == RHEA)) for member in PARTY],
                "author_constraints": [
                    "The pursuit remains a historical chase, not a defeat of the Tree King or the Root Host as a campaign antagonist.",
                    "Moth's identity, intent beyond the witnessed escape, and fate remain unresolved.",
                    "The hooked notch on Moth's bone key remains unused and unexplained.",
                ],
            },
            "bodyMarkdown": "# The Hunt Begins\n\nThe first pursuit ends at drowned stone: the five remain together and reach running water before the host's southward line can close. Nothing here defeats Aldren or determines what became of those left in his camp.\n",
        },
        {
            "type": "entity.update",
            "entity": CV_PURSUIT,
            "frontmatterPatch": {
                "status": "closed",
                "time": {"start": t(191), "end": t(195, 99)},
                "participants": [
                    participant(ROOTJAW, "distant-pursuer", t(193, 10), t(195, 99)),
                    *[participant(member, "fleeing-frontiersman", t(191), t(195, 99)) for member in PARTY],
                ],
            },
            "bodyMarkdown": "# Running Under the Drums\n\nThe escape exchange closes when the company drops from the northwood trail into the flooded works around an old waymark. Rootjaw's earlier bellow remains distant historical provenance; he is not present in the scenes that follow.\n\nThe turn list is canonical verbatim provenance. Character recollections are stored separately and may disagree.\n",
        },
        location_record(WARDEN_ROAD, "Abandoned Warden Road", "abandoned-road", FRONTIER,
            "A raised strip of fitted stone and corduroy timber running south beneath alder, peat, and decades of flood wash. Its surviving cuts were made for wardens on foot, not wagons, and no current map promises where it ends.",
            tags=("road", "abandoned", "southward")),
        location_record(DROWNED_WAYMARK, "Drowned Waymark", "flooded-waymark", WARDEN_ROAD,
            "A shoulder-high road stone drowned to its carved triangle in a blackwater braid. Running channels divide around it, and a rusted survey slot survives behind the old route mark.",
            tags=("waymark", "flooded", "warden-road")),
        object_record(BLACKROOT_COUNTERFOIL, "Blackroot Warden Counterfoil", "route-counterfoil",
            {"location": {"entity": DROWNED_WAYMARK}, "condition": "sealed in the rusted survey slot"},
            "A water-stained Lantern Pike route counterfoil headed 'Blackroot Recovery 7.' Dated after three recorded deaths and initialed H. Doss, it orders raw amber carried under floodwater and charcoal and kept apart from refined pieces. It does not name wretches, the report classification, or any compact.",
            tags=("blackroot", "guild", "bounded-evidence"), capabilities=("can-be-read",)),
        event_op(EV_REACH_WAYMARK, "The Frontiersmen Reach the Drowned Waymark", 196, 0, DROWNED_WAYMARK,
            [(RHEA, "leader"), (SYLVI, "route-finder"), (GARRAN, "warder"), (VEYRA, "reliquary-bearer"), (PIP, "mapper")],
            "The company drops from the closing spruce line into a flooded road cutting. Beneath the brown current Sylvi finds worked stone, and Pip recognizes the triangular warden mark without fitting the bone key to anything.",
            causes=(EV_PURSUIT,), story_points=(SP_FLEE, SP_RETURN),
            effects=party_move_effects("waymark-party", DROWNED_WAYMARK, "soaked, exhausted, and still moving"), tags=("pursuit", "waymark")),
        event_op(EV_BREAK_PURSUIT, "The Drowned Waymark Breaks the Immediate Pursuit", 202, 20, DROWNED_WAYMARK,
            [(RHEA, "leader"), (SYLVI, "tracker"), (GARRAN, "warder"), (VEYRA, "arcanist"), (PIP, "line-tender")],
            "With the reliquary still closed, Veyra lowers its charcoal-wrapped weight into the racing junction on Pip's cord. The cold channels throw its answering pulse along three flooded cuts. Drums divide north and east; the party retrieves the box and hears the search pass away from the drowned stone.",
            causes=(EV_REACH_WAYMARK, EV_PURSUIT), story_points=(SP_FLEE,), effects=[
                *party_move_effects("pursuit-broken-party", DROWNED_WAYMARK, "soaked and exhausted; immediate trail lost"),
                effect("pursuit-broken-reliquary", AMBER_RELIQUARY, "condition", "set", "closed; charcoal-wrapped; resonance quieted by cold running water"),
            ], tags=("pursuit", "escape", "reliquary")),
        event_op(EV_COUNTERFOIL, "Pip Recovers the Blackroot Warden Counterfoil", 204, 30, DROWNED_WAYMARK,
            [(PIP, "reader"), (RHEA, "evidence-bearer"), (GARRAN, "witness"), (SYLVI, "watcher"), (VEYRA, "corroborating-arcanist")],
            "Pip lifts an ordinary iron survey pin and unfolds the counterfoil behind it. The paper corroborates that Halric's field office imposed special amber precautions after deaths at Blackroot. It does not record the manifestation, the false public category, or a conspiracy agreement.",
            causes=(EV_REACH_WAYMARK, EV_COMPACT), story_points=(SP_GUILD_SECRET,), effects=[
                effect("counterfoil-location-clear", BLACKROOT_COUNTERFOIL, "location", "clear"),
                effect("counterfoil-rhea", BLACKROOT_COUNTERFOIL, "holder", "set", {"entity": RHEA}),
                effect("counterfoil-condition", BLACKROOT_COUNTERFOIL, "condition", "set", "water-stained but legible; folded dry"),
            ], tags=("blackroot", "evidence", "guild")),
        event_op(EV_TAKE_SOUTHWARD, "The Frontiersmen Take the Southward Cut", 207, 0, WARDEN_ROAD,
            [(RHEA, "leader"), (SYLVI, "route-finder"), (GARRAN, "warder"), (VEYRA, "reliquary-bearer"), (PIP, "mapper")],
            "Once the drums have remained absent, the five leave the drowned marker by the alder-covered warden road. Its old cuts run south, but no surviving line connects them to Harrowcross.",
            causes=(EV_BREAK_PURSUIT, EV_COUNTERFOIL), story_points=(SP_RETURN, SP_GUILD_SECRET),
            effects=party_move_effects("southward-party", WARDEN_ROAD, "moving south under concealment"), tags=("road", "southward", "active")),
        conversation_op(CV_WAYMARK, "Water Over the Mark", status="closed", start=t(196, 10), end=t(205), scene=SC_WAYMARK, location=DROWNED_WAYMARK,
            participants=[participant(member, "escaped-frontiersman", t(196, 10), t(205)) for member in PARTY], turns=waymark_turns,
            topics=("pursuit", "drowned-waymark", "blackroot-evidence"),
            body="Under cold water and receding drums, the company distinguishes a usable road sign from an unknown key, a bounded document from Aldren's larger accusation, and a pause in pursuit from safety."),
        conversation_op(CV_SOUTHWARD, "The Southward Cut", status="active", start=t(207, 10), end=None, scene=SC_SOUTHWARD, location=WARDEN_ROAD,
            participants=[participant(member, "southbound-frontiersman", t(207, 10)) for member in PARTY], turns=southward_turns,
            topics=("southward", "blackroot-evidence", "harrowcross"),
            body="The company takes inventory while walking: one immediate chase escaped, one narrow piece of evidence preserved, and no honest claim that the abandoned road reaches Harrowcross."),
        scene_record(SC_WAYMARK, "Drowned Waymark", status="closed", start=t(196), current=t(206), end=t(206), location=DROWNED_WAYMARK,
            participants=[participant(member, "escaped-frontiersman", t(196), t(206), pov=(member == PIP)) for member in PARTY],
            objects=(MOTH_KEY, AMBER_RELIQUARY, BLACKROOT_COUNTERFOIL, CAVE_MAP, BROKEN_SPEAR, *BADGES.values()),
            story_points=(SP_FLEE, SP_GUILD_SECRET, SP_RETURN, SP_MOTH), conversations=(CV_WAYMARK,), observations=(
                observation("waymark-triangle", 197, 0, (PIP,), "The triangular notch on the bone key matches the drowned stone's route mark; the hooked notch matches nothing used here."),
                observation("waymark-water", 198, 0, (VEYRA,), "Cold running water divides the closed reliquary's answering pull among three channels without opening its lid."),
                observation("waymark-drums", 201, 0, (SYLVI,), "The pursuit drums divide north and east, then recede beyond reliable hearing."),
                observation("waymark-counterfoil", 204, 0, ("participants",), "The counterfoil records three Blackroot deaths, Halric's initials, and special separation and damping rules for amber."),
            ), constraints=(
                "The immediate Root Host pursuit is broken; the Tree King and his larger host remain unresolved antagonists.",
                "Rootjaw is not present in this scene or its plan.",
                "The reliquary remains closed throughout the maneuver.",
                "The hooked key notch is neither used nor explained; Moth's identity, intent beyond the witnessed escape, and fate remain unknown.",
                "The counterfoil corroborates post-Blackroot handling knowledge only; it does not prove the entire compact or Aldren's full account.",
                "Nothing establishes Jorund's fate or reaches Harrowcross.",
            ), body="Floodwater has buried the road but preserved its argument: the carved triangle still directs travelers, while a forgotten paper proves only the narrow institutional fact written on it. The party earns breathing room through restraint rather than conquest.", tags=("waymark", "escape", "evidence")),
        scene_record(SC_SOUTHWARD, "Southward Cut", status="active", start=t(207), current=t(210), end=None, location=WARDEN_ROAD,
            participants=[participant(member, "southbound-frontiersman", t(207), pov=(member == RHEA)) for member in PARTY],
            objects=(MOTH_KEY, AMBER_RELIQUARY, BLACKROOT_COUNTERFOIL, CAVE_MAP, BROKEN_SPEAR, *BADGES.values()),
            story_points=(SP_GUILD_SECRET, SP_RETURN, SP_MOTH), conversations=(CV_SOUTHWARD,), observations=(
                observation("southward-alder", 207, 20, (SYLVI,), "Wet alder folds across the abandoned road after the party passes, hiding movement without promising protection."),
                observation("southward-map", 209, 10, (PIP,), "The surviving warden cuts run south, but neither Jorund's mine map nor the counterfoil connects them to Harrowcross."),
                observation("southward-evidence", 208, 10, (RHEA,), "The counterfoil is legible enough to confront guild authority with one precise contradiction, not a complete case."),
            ), constraints=(
                "The scene remains active at main 210:0 on an uncertain southward road.",
                "Rootjaw is absent from the scene and from the party's private route plan.",
                "Harrowcross has not been reached and the road is not confirmed to lead there.",
                "The reliquary remains closed and the hooked notch on Moth's key remains unused.",
                "Do not identify Moth or establish the fate of Moth or Jorund.",
                "The Blackroot counterfoil remains a narrow corroborating document, not resolution of the guild conspiracy.",
            ), body="The old road is less a rescue than a direction. Five exhausted hunters walk south with a closed danger, an unknown key, and one piece of paper whose restraint makes it harder to dismiss.", tags=("active", "southward", "warden-road")),
        knowledge_record(eid("knowledge", "pip-matches-key-waymark-triangle"), "Pip matches the key's triangle to the drowned waymark", PIP,
            "moth-key.triangle.matches-warden-waymark", "The triangular notch on Moth's bone key matches the route mark carved on the Drowned Waymark, but the unfamiliar hooked notch remains unused and unexplained.",
            197, 5, confidence=0.98, acquisition="observed", truth="true", subject=MOTH_KEY, predicate="matches-route-mark-on", object_value={"entity": DROWNED_WAYMARK}, causing_event=EV_REACH_WAYMARK, source_entity=DROWNED_WAYMARK, tags=("route", "moth-key")),
        knowledge_record(eid("knowledge", "veyra-running-water-diffuses-reliquary"), "Veyra knows cold running water diffuses the reliquary signal", VEYRA,
            "reliquary.resonance.diffused-by-running-water", "Cold running water and charcoal can divide the closed reliquary's answering resonance across branching channels without opening it.",
            202, 25, confidence=0.94, acquisition="tested", truth="true", subject=AMBER_RELIQUARY, predicate="signal-diffused-by", object_value={"text": "cold running water and charcoal"}, causing_event=EV_BREAK_PURSUIT, source_entity=AMBER_RELIQUARY, tags=("reliquary", "pursuit")),
        knowledge_record(eid("knowledge", "rhea-counterfoil-bounds-blackroot-proof"), "Rhea understands the limit of the Blackroot counterfoil", RHEA,
            "blackroot-counterfoil.proves-bounded-guild-knowledge", "Halric's field office imposed special amber handling rules after three deaths at Blackroot; the counterfoil does not itself prove the manifestation classification or the full concealment Aldren alleged.",
            204, 35, confidence=0.99, acquisition="read", truth="true", subject=BLACKROOT_COUNTERFOIL, predicate="corroborates", object_value={"text": "bounded post-Blackroot guild knowledge"}, causing_event=EV_COUNTERFOIL, source_entity=BLACKROOT_COUNTERFOIL, tags=("blackroot", "evidence")),
        relationship_update(RHEA, PIP, [
            relationship_transition("below-rhea-pip", 152, 0, full(trust=0.68, affinity=0.28, fear=0.02, obligation=0.16, respect=0.7, dependence=0.46), ["trusted-mapper", "keeps-exits-real"], "Pip preserves a navigable route and finds the surface shaft while frightened.", causing_event=EV_GALLERY_ESCAPE),
            relationship_transition("waymark-rhea-pip", 205, 10, full(trust=0.78, affinity=0.32, fear=0.02, obligation=0.2, respect=0.82, dependence=0.5), ["trusted-mapper", "evidence-reader", "states-limits"], "Pip finds both the southward road and the counterfoil, then distinguishes what each can and cannot promise.", causing_event=EV_COUNTERFOIL),
        ]),
        relationship_update(PIP, RHEA, [
            relationship_transition("below-pip-rhea", 152, 1, full(trust=0.64, affinity=0.34, fear=0.04, obligation=0.12, respect=0.62, dependence=0.32), ["leader-keeps-promises", "uses-his-map"], "Rhea treats the map as a shared promise rather than Pip's private trick.", causing_event=EV_GALLERY_ESCAPE),
            relationship_transition("waymark-pip-rhea", 205, 11, full(trust=0.74, affinity=0.38, fear=0.03, obligation=0.14, respect=0.74, dependence=0.34), ["leader-keeps-promises", "protects-evidence", "refuses-false-certainty"], "Rhea carries the counterfoil without inflating it and takes the road without pretending it reaches home.", causing_event=EV_COUNTERFOIL),
        ]),
        story_update(SP_FLEE, [
            story_transition("flee-active", 189, 10, "active", EV_ALARM, "The Root Host raises the camp-wide pursuit alarm."),
            story_transition("flee-resolved", 202, 20, "resolved", EV_BREAK_PURSUIT, "Running water divides the reliquary signal and the immediate hunting lines pass away from the party."),
        ], outcomes=(EV_BREAK_PURSUIT,)),
        story_update(SP_GUILD_SECRET, [
            story_transition("guild-secret-active", 74, 0, "active", EV_AMBER_PAYMENT, "The protection ledger makes guild dependence on amber explicit."),
            story_transition("guild-secret-testimony", 167, 40, "active", EV_TREE_AUDIENCE, "Aldren identifies himself as a former field-master and accuses Halric of the Blackroot concealment."),
            story_transition("guild-secret-counterfoil", 204, 30, "active", EV_COUNTERFOIL, "The counterfoil corroborates special guild precautions after Blackroot but does not yet expose the full compact."),
        ]),
        world_record(world_id, final=True, current_summary=(
            "At tick 210 the five hunters are moving south along an abandoned warden road after the Drowned Waymark split the immediate Root Host pursuit. Rhea carries a narrowly corroborating Blackroot counterfoil; Veyra's reliquary remains closed and quiet under charcoal; Pip still holds Moth's bone key with its hooked notch unused. The road is not confirmed to reach Harrowcross, and neither Moth's nor Jorund's fate is known."
        )),
    ]
    return operations
