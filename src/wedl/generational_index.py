"""Disposable generational evidence and bounded internal replay primitives.

Callers must supply already visibility-filtered record IDs. These functions do
not decide which audience, perspective, or horizon a viewer is entitled to.
"""
from __future__ import annotations

import json
from heapq import merge
import sqlite3
from typing import Any, Collection

from .generational import GENERATIONAL_KINDS, generational_record
from .model import StoryTime, World
from .semantics import effective_time
from .util import canonical_json


def _citation(row: sqlite3.Row) -> dict[str, Any]:
    return json.loads(str(row["citation_json"]))


def fold_record(
    connection: sqlite3.Connection,
    record_id: str,
    *,
    at: StoryTime,
    candidate_ids: Collection[str],
) -> dict[str, Any] | None:
    """Fold one visible authored record; return None before its first instant.

    Replacement winners retain citations to the superseded literal chain.
    Inclusive vacancy is an operation at the supplied point, with a derived
    expiry after its last instant; no expiry transition is persisted.
    """
    if record_id not in candidate_ids:
        return None
    record = connection.execute(
        "SELECT kind,timeline FROM generational_record WHERE id=?", (record_id,)
    ).fetchone()
    if record is None or record["timeline"] != at.timeline:
        return None
    rows = connection.execute(
        "SELECT * FROM generational_transition "
        "WHERE record_id=? AND timeline=? AND "
        "(start_tick<? OR (start_tick=? AND start_order<=?)) "
        "ORDER BY start_tick,start_order,source_ordinal,id",
        (record_id, at.timeline, at.tick, at.tick, at.order),
    ).fetchall()
    if not rows:
        return None
    by_id = {str(row["id"]): row for row in rows}
    superseded = {str(row["replaces_transition_id"]) for row in rows if row["replaces_transition_id"]}
    state: str | None = None
    table = {
        "organization": "generational_organization", "parentage": "generational_parentage",
        "union": "generational_union", "affiliation": "generational_affiliation",
        "legacy": "generational_legacy", "tenure": "generational_tenure",
        "claim": "generational_claim", "vital-history": "generational_vital",
    }[str(record["kind"])]
    identity = connection.execute(f"SELECT * FROM {table} WHERE id=?", (record_id,)).fetchone()
    value: dict[str, Any] = {key: identity[key] for key in identity.keys() if key != "id"} if identity else {}
    citations: list[dict[str, Any]] = []
    for row in rows:
        if row["id"] in superseded:
            continue
        kind = str(row["transition_kind"])
        payload = json.loads(str(row["payload_json"]))
        citation = _citation(row)
        if row["cause_citation_json"]:
            citation["causeCitation"] = json.loads(str(row["cause_citation_json"]))
        replaced: list[dict[str, Any]] = []
        ancestor = row["replaces_transition_id"]
        while ancestor and str(ancestor) in by_id:
            prior = by_id[str(ancestor)]
            replaced.append(_citation(prior))
            ancestor = prior["replaces_transition_id"]
        if replaced:
            citation["supersededCitations"] = replaced
        citations.append(citation)
        value.update(payload)
        if kind.endswith("-initialize"):
            state = {
                "organization": "active", "parentage": "asserted",
                "union": "declared", "affiliation": "active", "legacy": "active",
                "tenure": "holding" if payload.get("holder_id") else "vacant",
                "claim": "proposed", "vital-history": "unknown",
            }[str(record["kind"])]
        else:
            state = {
                "organization-dormant": "dormant", "organization-dissolve": "dissolved",
                "parentage-confirm": "confirmed", "parentage-end": "ended",
                "union-form": "formed", "union-reconcile": "formed",
                "union-end": "ended", "union-annul": "annulled",
                "affiliation-end": "ended", "legacy-dormant": "dormant",
                "legacy-dissolve": "dissolved", "tenure-designate": "designated",
                "tenure-hold": "holding", "tenure-transfer": "vacant",
                "tenure-end": "ended", "claim-dispute": "disputed",
                "claim-recognize": "recognized", "claim-withdraw": "withdrawn",
                "claim-reject": "rejected", "vital-birth": "living",
                "vital-death": "dead", "vital-existence-start": "existing",
                "vital-existence-end": "ended",
            }.get(kind, state)
            if kind == "tenure-transfer":
                # The source tenure ceases to hold. The target tenure remains
                # governed by its own literal hold transition.
                value["holder_id"] = None
            if kind == "organization-reparent":
                value["parent_id"] = payload["parent_id"]
            if kind == "tenure-vacate":
                last = (int(row["end_tick"]), int(row["end_order"]))
                state = "vacant-interval" if (at.tick, at.order) <= last else "expired"
                value["holder_id"] = None
    # A vacancy interval that has expired is not a persisted transition. It
    # remains a closed historical result even when no later literal hold exists.
    return {"recordId": record_id, "kind": record["kind"], "state": state,
            "value": value, "citations": citations}


def _bounded_paths(
    connection: sqlite3.Connection,
    start_id: str,
    *,
    at: StoryTime,
    candidate_ids: Collection[str],
    max_depth: int,
    max_items: int,
    edge_table: str,
    from_column: str,
    to_column: str,
    active_states: frozenset[str],
) -> dict[str, Any]:
    if max_depth < 0 or max_items < 1:
        raise ValueError("positive item bound and nonnegative depth bound required")
    frontier: list[tuple[str, tuple[dict[str, Any], ...], frozenset[str]]] = [
        (start_id, (), frozenset({start_id}))
    ]
    expanded = {start_id}
    result: list[dict[str, Any]] = []

    def ordered_edges(node: str, path: tuple[dict[str, Any], ...], nodes: frozenset[str]):
        # Each cursor uses its indexed source node. Merge the cursors at the
        # whole depth so a later frontier node can still have an earlier
        # authored applicability point.
        rows = connection.execute(
            f"SELECT edge.id,edge.{to_column} AS target,"
            "initial.start_tick,initial.start_order,source.source_ordinal "
            f"FROM {edge_table} AS edge "
            "JOIN generational_record AS source ON source.id=edge.id "
            "JOIN generational_transition AS initial "
            "ON initial.record_id=edge.id AND initial.source_ordinal=0 "
            f"WHERE edge.{from_column}=? AND initial.timeline=? "
            "ORDER BY initial.start_tick,initial.start_order,source.source_ordinal,edge.id",
            (node, at.timeline),
        )
        for row in rows:
            key = (int(row["start_tick"]), int(row["start_order"]),
                   int(row["source_ordinal"]), str(row["id"]))
            yield key, node, path, nodes, str(row["id"]), str(row["target"])

    for depth in range(max_depth + 1):
        next_frontier: list[tuple[str, tuple[dict[str, Any], ...], frozenset[str]]] = []
        streams = [ordered_edges(node, path, nodes) for node, path, nodes in frontier]
        for _, node, path, nodes, edge_id, target in merge(
            *streams, key=lambda item: item[0]
        ):
            if edge_id not in candidate_ids or target in nodes:
                continue
            folded = fold_record(connection, edge_id, at=at, candidate_ids=candidate_ids)
            if folded is None or folded["state"] not in active_states:
                continue
            if depth == max_depth or len(result) >= max_items:
                return {"status": "limit", "paths": []}
            step = {"from": node, "to": target, "edgeId": edge_id,
                    "citations": folded["citations"]}
            target_path = (*path, step)
            result.append({"targetId": target, "edges": list(target_path)})
            if target not in expanded:
                expanded.add(target)
                next_frontier.append((target, target_path, nodes | {target}))
        frontier = next_frontier
        if not frontier:
            break
    return {"status": "ok", "paths": result}


def cited_ancestors(
    connection: sqlite3.Connection, child_id: str, *, at: StoryTime,
    candidate_ids: Collection[str], max_depth: int, max_items: int,
) -> dict[str, Any]:
    """Follow only authored, currently asserted/confirmed parent edges."""
    return _bounded_paths(
        connection, child_id, at=at, candidate_ids=candidate_ids,
        max_depth=max_depth, max_items=max_items,
        edge_table="generational_parentage", from_column="child_id",
        to_column="parent_id", active_states=frozenset({"asserted", "confirmed"}),
    )


def cited_containment(
    connection: sqlite3.Connection, organization_id: str, *, at: StoryTime,
    candidate_ids: Collection[str], max_depth: int, max_items: int,
) -> dict[str, Any]:
    """Walk exact folded reparenting, including removal of a parent."""
    if max_depth < 0 or max_items < 1:
        raise ValueError("positive item bound and nonnegative depth bound required")
    node = organization_id
    seen = {node}
    result: list[dict[str, Any]] = []
    steps: list[dict[str, Any]] = []
    for depth in range(max_depth + 1):
        folded = fold_record(connection, node, at=at, candidate_ids=candidate_ids)
        if folded is None or folded["state"] not in {"active", "dormant"}:
            break
        parent = folded["value"].get("parent_id")
        if parent is None or parent not in candidate_ids:
            break
        if parent in seen:
            return {"status": "limit", "paths": []}
        parent_fold = fold_record(connection, parent, at=at, candidate_ids=candidate_ids)
        if parent_fold is None or parent_fold["state"] not in {"active", "dormant"}:
            break
        if depth == max_depth or len(seen) >= max_items:
            return {"status": "limit", "paths": []}
        seen.add(parent)
        steps.append({"from": node, "to": parent, "edgeId": node,
                      "citations": folded["citations"]})
        result.append({"targetId": parent, "edges": list(steps)})
        node = parent
    return {"status": "ok", "paths": result}


def current_holders(
    connection: sqlite3.Connection, legacy_id: str, *, at: StoryTime,
    candidate_ids: Collection[str],
) -> list[dict[str, Any]]:
    """Keep legal and de-facto literal holds separate from all claims."""
    if legacy_id not in candidate_ids:
        return []
    rows = connection.execute(
        "SELECT id FROM generational_tenure WHERE legacy_id=? ORDER BY id", (legacy_id,)
    ).fetchall()
    result = []
    for row in rows:
        folded = fold_record(connection, str(row["id"]), at=at, candidate_ids=candidate_ids)
        if folded is not None and folded["state"] == "holding" and folded["value"].get("holder_id"):
            result.append(folded)
    return result


def insert_generational_index(
    connection: sqlite3.Connection, world: World, at: StoryTime | None = None,
) -> dict[str, int]:
    """Insert deterministic rows after validated entities, before SQL indexes."""
    current = at or effective_time(world)
    records = [record for record in sorted(world.records.values(), key=lambda item: item.source_path)
               if record.kind in GENERATIONAL_KINDS]
    for source_ordinal, record in enumerate(records):
        item = generational_record(record.frontmatter)
        fields = item.fields
        point = item.initialization.applicability.start()
        assert point is not None
        connection.execute(
            "INSERT INTO generational_record VALUES (?,?,?,?,?,?,?,?,?,?)",
            (record.id, record.kind, source_ordinal, record.source_path, record.blob_oid,
             record.status, "generational-core-v1", point.timeline, canonical_json(fields["audience"]),
             canonical_json(fields["perspectives"])),
        )
        kind = record.kind
        if kind == "organization":
            connection.execute("INSERT INTO generational_organization VALUES (?,?,?,?)",
                               (record.id, fields["organization_kind"], fields.get("parent_id"), fields.get("location_id")))
        elif kind == "parentage":
            connection.execute("INSERT INTO generational_parentage VALUES (?,?,?)",
                               (record.id, fields["child_id"], fields["parent_id"]))
        elif kind == "union":
            connection.execute("INSERT INTO generational_union VALUES (?)", (record.id,))
        elif kind == "affiliation":
            connection.execute("INSERT INTO generational_affiliation VALUES (?,?,?)",
                               (record.id, fields["character_id"], fields["organization_id"]))
        elif kind == "legacy":
            connection.execute("INSERT INTO generational_legacy VALUES (?,?,?)",
                               (record.id, fields["legacy_kind"], fields.get("organization_id")))
        elif kind == "tenure":
            connection.execute("INSERT INTO generational_tenure VALUES (?,?,?,?)",
                               (record.id, fields["legacy_id"], fields.get("predecessor_tenure_id"), fields.get("successor_tenure_id")))
        elif kind == "claim":
            connection.execute("INSERT INTO generational_claim VALUES (?,?,?)",
                               (record.id, fields["legacy_id"], fields["claimant_id"]))
        elif kind == "vital-history":
            connection.execute("INSERT INTO generational_vital VALUES (?,?,?)",
                               (record.id, fields["character_id"], fields["disclosure"]))
        for ordinal, transition in enumerate((item.initialization, *item.transitions)):
            applicability = transition.applicability
            start = applicability.start()
            assert start is not None
            end = applicability.last
            section = "initialization" if ordinal == 0 else f"transitions[{ordinal - 1}]"
            citation = record.citation(world.revision, section)
            cause_citation = (
                world.get(transition.cause_event_id).citation(world.revision, "time")
                if transition.cause_event_id else None
            )
            connection.execute(
                "INSERT INTO generational_transition VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (transition.id, record.id, ordinal, transition.kind, applicability.kind,
                 start.timeline, start.tick, start.order, end.tick if end else None,
                 end.order if end else None, canonical_json(dict(transition.payload)),
                 transition.cause_event_id,
                 canonical_json(cause_citation) if cause_citation else None,
                 transition.replaces_transition_id,
                 canonical_json(citation)),
            )
            if kind == "union" and "participant_ids" in transition.payload:
                connection.executemany(
                    "INSERT INTO generational_union_participant VALUES (?,?,?,?)",
                    [(record.id, transition.id, participant, index)
                     for index, participant in enumerate(transition.payload["participant_ids"])],
                )
            # Record-level labels can summarize a later transition. Keep only
            # this literal transition's fields in the private temporal row.
            structure = {"kind": kind, "recordId": record.id,
                         "transitionKind": transition.kind, "payload": dict(transition.payload),
                         "causeEventId": transition.cause_event_id,
                         "replacesTransitionId": transition.replaces_transition_id}
            connection.execute(
                "INSERT INTO generational_candidate VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (record.id, transition.id, ordinal, "generational-core-v1",
                 start.timeline, start.tick, start.order,
                 end.tick if end else None, end.order if end else None,
                 canonical_json(fields["audience"]), canonical_json(fields["perspectives"]),
                 canonical_json(citation), canonical_json(structure)),
            )
    candidates = frozenset(record.id for record in records)
    for record in records:
        folded = fold_record(connection, record.id, at=current, candidate_ids=candidates)
        if folded is not None:
            connection.execute(
                "INSERT INTO generational_current VALUES (?,?,?,?,?,?)",
                (record.id, current.timeline, current.tick, current.order,
                 folded["state"], canonical_json(folded)),
            )
    return {"generationalRecords": len(records),
            "generationalTransitions": connection.execute("SELECT COUNT(*) FROM generational_transition").fetchone()[0]}
