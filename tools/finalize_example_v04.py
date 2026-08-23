#!/usr/bin/env python3
"""Finalize long-arc continuity in the authored Ash Archive example.

This is an example-authoring script, not a repository migration feature. It is
kept so the packaged fixture can be regenerated from the 0.3 example plus the
second-act expansion in a reproducible sequence.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import json
from pathlib import Path

from wedl.changeset import apply, preview
from wedl.ids import id_from_seed
from wedl.repository import Repository


def point(tick: int, order: int = 0) -> dict[str, object]:
    return {"timeline": "main", "tick": tick, "order": order}


def transition(seed: str, tick: int, order: int, state: str, event_id: str, note: str) -> dict[str, object]:
    return {
        "id": id_from_seed("story-point-transition", f"ash-v04-continuity:{seed}"),
        "time": point(tick, order),
        "state": state,
        "causing_event": event_id,
        "note": note,
    }


def build(repository: Repository) -> dict[str, object]:
    world = repository.load_world("HEAD")
    operations: list[dict[str, object]] = []

    def upsert(record, frontmatter=None, body=None):
        operations.append({
            "type": "entity.upsert",
            "value": {
                "frontmatter": deepcopy(frontmatter or record.frontmatter),
                "bodyMarkdown": body if body is not None else record.body,
            },
        })

    event_drawer = world.find("Token 7B Releases the Seventh Drawer", "event").id
    event_letter = world.find("Mara Opens the Heron Letter", "event").id
    event_hearing = world.find("The Ledger Hearing Convenes", "event").id
    event_account = world.find("Ilyra Explains the Descendant Ledger", "event").id

    search_gap = world.find("Search the Catalog Gap", "story-point")
    fm = deepcopy(search_gap.frontmatter)
    fm["lifecycle"] = {
        "initial_state": "dormant",
        "transitions": [
            transition("search-gap-resolved", 126, 10, "resolved", event_drawer, "Token 7B opens the missing-card rail and turns the gap into a physical route."),
        ],
    }
    fm["outcome_events"] = [event_drawer]
    upsert(search_gap, fm, search_gap.body + "\n\nThe search is resolved when Token 7B opens the seventh-drawer rail at tick 126.\n")

    trace = world.find("Trace the Erased-Wing Heron", "story-point")
    fm = deepcopy(trace.frontmatter)
    fm["lifecycle"] = {
        "initial_state": "dormant",
        "transitions": [
            transition("trace-heron-active", 148, 0, "active", event_letter, "Opening Ilyra's letter confirms that the mark belongs to a live contingency network."),
            transition("trace-heron-resolved", 171, 20, "resolved", event_account, "Ilyra explains how she reactivated the network and why its records were divided."),
        ],
    }
    fm["outcome_events"] = [event_letter, event_account]
    upsert(trace, fm, trace.body + "\n\nThe thread resolves when Ilyra accounts for the reactivated network at tick 171.\n")

    confront = world.find("Confront Caldrin Vey", "story-point")
    fm = deepcopy(confront.frontmatter)
    fm["lifecycle"] = {
        "initial_state": "dormant",
        "transitions": [
            transition("confront-caldrin-active", 160, 0, "active", event_hearing, "The ledger hearing puts Caldrin's account under public challenge."),
        ],
    }
    fm["outcome_events"] = []
    upsert(confront, fm, confront.body + "\n\nThe confrontation is active after the hearing, but final responsibility remains contested.\n")

    character_bodies = {
        "Mara Vale": """# Mara Vale

## Summary

A meticulous archivist in her early thirties who notices documentary inconsistencies before she understands the human motives behind them. Ilyra trained her to distrust elegant explanations.

## Appearance

Compact, ink-stained, usually dressed in a gray archive coat with narrow interior pockets for pencils and accession slips.

## Voice

Precise and restrained. She asks short, concrete questions when frightened and becomes formally polite when angry.

## Goals

Protect people without making evidence disappear, keep the Archive from becoming a centralized weapon, and make every irreversible choice defensible through provenance.

## Author notes

Mara opens Ilyra's letter at tick 148 under witness. Historical contexts before that event must not expose its contents. Her evidence against Rusk and Caldrin remains strong but does not collapse their distinct responsibility into one simple answer.
""",
        "Nessa Quill": """# Nessa Quill

## Summary

An apprentice archivist with a talent for remembering physical arrangements. She found the catalog gap because the dust line around the missing cards was too clean.

## Voice

Fast, associative, and candid with Mara. She disguises fear as practical questions.

## Goals

Preserve provenance while redesigning dangerous records, keep Mara honest about what she withholds, and prove that arrangement and custody can matter as much as completeness.

## Author notes

Nessa enters the Reading Room at tick 122, helps open the seventh-drawer route, and proposes splitting proof, route, and identity among independent custodians at tick 177.
""",
        "Ilyra Sorn": """# Ilyra Sorn

## Summary

Chief Archivist of Cindervale and Mara's mentor. She treats records as civic infrastructure rather than neutral history.

## Voice

Dry, patient, and exacting. She rarely raises her voice; refusal from Ilyra sounds like a procedural conclusion.

## Goals

Keep identity-bearing records out of centralized custody, preserve proof of the illegal evacuations, and accept that Mara may build a safer Archive by rejecting part of Ilyra's solution.

## Author notes

Ilyra is concealed beneath the Restricted Vault from tick 91 until Mara finds her at tick 169. Marker: ASH-SECRET-ILYRA-UNDERCROFT-9K2M. Historical contexts must obey that timeline even though the current story has found her alive.
""",
        "Ysabet Crane": """# Ysabet Crane

## Summary

A Council notary who makes coercion look like neutral procedure and notices seals, dates, and who benefits from technical validity.

## Appearance

Practical Cindervale clothes marked by the tools and obligations of the office.

## Voice

Dry and exact. She offers choices in the grammar of rulings rather than favors.

## Goals

Turn the surviving evidence chain into lawful distributed custody, prevent procedural doubt from becoming an excuse for destruction, and keep her own intervention independently reviewable.

## Author notes

Ysabet is loyal to admissible evidence, not secretly loyal to Mara. She knows more about Rusk's requisition habits than she initially discloses, but every character-facing context must rely on her timed statements and recollections rather than this note.
""",
        "Sister Ansel Marr": """# Sister Ansel Marr

## Summary

A lay sister who maintains the south-bank pressure gates and treats water levels as civic testimony.

## Appearance

Practical civic clothing marked by the tools and obligations of the role.

## Voice

Low, spare, and rhythmically deliberate. She counts before answering dangerous questions.

## Goals

Move the group beyond the closing shutters, preserve the pressure route, and make sure the argument over custody does not drown the people or evidence it is meant to protect.

## Author notes

Ansel helped Ilyra inspect the undercroft months earlier. She joins the active Choice of Records conversation only at tick 178 and hears none of its preceding turns.
""",
    }
    for title, body in character_bodies.items():
        upsert(world.find(title, "character"), body=body)

    return {
        "protocol": "wedl-changeset/v1",
        "expectedHead": repository.head(),
        "idempotencyKey": "ash-v04-continuity-cleanup-v1",
        "summary": "Resolve completed story points and refresh enduring character goals",
        "operations": operations,
    }


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
        print(json.dumps(apply(repository, payload, confirmation_token_value=plan["confirmationToken"]), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
