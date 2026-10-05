"""Disposable generational evidence and bounded internal replay primitives.

Callers must supply already visibility-filtered record IDs. These functions do
not decide which audience, perspective, or horizon a viewer is entitled to.
"""
from __future__ import annotations

import json
from heapq import merge
import re
import sqlite3
from typing import Any, Collection

from .generational import GENERATIONAL_KINDS, generational_record
from .conversation import turn_time
from .generational_knowledge import AFFIRMATIVE_STATES, GenealogyAssertion
from .model import StoryTime, World
from .semantics import effective_time
from .util import canonical_json


_STRUCTURAL_TOKEN = re.compile(r"\w+", re.UNICODE)
_MAX_SEARCH_PREFIX = 32


def _search_prefixes(structure: dict[str, Any]) -> set[str]:
    """Normalize only literal structural fields, never source prose or title."""
    tokens = _STRUCTURAL_TOKEN.findall(canonical_json(structure).casefold())
    return {token[:length] for token in tokens
            for length in range(1, min(len(token), _MAX_SEARCH_PREFIX) + 1)}


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
        # Each covering endpoint/time index yields its node's authored order.
        # Merge across the whole depth before choosing the next path.
        rows = connection.execute(
            f"SELECT id,{to_column} AS target,start_tick,start_order,source_ordinal "
            f"FROM {edge_table} "
            f"WHERE {from_column}=? AND timeline=? "
            "ORDER BY start_tick,start_order,source_ordinal,id",
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


def cited_descendants(
    connection: sqlite3.Connection, parent_id: str, *, at: StoryTime,
    candidate_ids: Collection[str], max_depth: int, max_items: int,
) -> dict[str, Any]:
    """Reverse authored parentage through the parent-side Btree."""
    return _bounded_paths(
        connection, parent_id, at=at, candidate_ids=candidate_ids,
        max_depth=max_depth, max_items=max_items,
        edge_table="generational_parentage", from_column="parent_id",
        to_column="child_id", active_states=frozenset({"asserted", "confirmed"}),
    )


def cited_relative_path(
    connection: sqlite3.Connection, first_id: str, second_id: str, *,
    at: StoryTime, candidate_ids: Collection[str], max_depth: int, max_items: int,
) -> dict[str, Any]:
    """Find one continuous cited kinship path in either authored direction."""
    if max_depth < 0 or max_items < 1:
        raise ValueError("positive item bound and nonnegative depth bound required")
    if first_id == second_id:
        return {"status": "ok", "paths": []}
    frontier: list[tuple[str, tuple[dict[str, Any], ...]]] = [(first_id, ())]
    seen = {first_id}
    expanded = 0

    def neighbor_stream(node: str, path: tuple[dict[str, Any], ...],
                        source: str, target: str, index: str):
        # The covering endpoint/time index yields the authored order without
        # sorting or folding the rest of a high-degree node's edges.
        rows = connection.execute(
            f"SELECT id,{target} AS target,start_tick,start_order,source_ordinal "
            f"FROM generational_parentage INDEXED BY {index} "
            f"WHERE {source}=? AND timeline=? "
            "ORDER BY start_tick,start_order,source_ordinal,id",
            (node, at.timeline),
        )
        for row in rows:
            edge_id = str(row["id"])
            key = (int(row["start_tick"]), int(row["start_order"]),
                   int(row["source_ordinal"]), edge_id)
            yield key, node, path, str(row["target"]), edge_id

    while frontier:
        next_frontier = []
        streams = [neighbor_stream(node, path, source, target, index)
                   for node, path in frontier
                   for source, target, index in (
                       ("child_id", "parent_id", "generational_parentage_child_time_idx"),
                       ("parent_id", "child_id", "generational_parentage_parent_time_idx"))]
        for _key, node, path, target, edge_id in merge(*streams, key=lambda item: item[0]):
            if target in seen or edge_id not in candidate_ids:
                continue
            folded = fold_record(connection, edge_id, at=at, candidate_ids=candidate_ids)
            if folded is None or folded["state"] not in {"asserted", "confirmed"}:
                continue
            if len(path) == max_depth or expanded == max_items:
                return {"status": "limit", "paths": []}
            step = {"from": node, "to": target, "edgeId": edge_id,
                    "citations": folded["citations"]}
            new_path = (*path, step)
            expanded += 1
            if target == second_id:
                return {"status": "ok", "paths": [{"targetId": target,
                                                   "edges": list(new_path)}]}
            seen.add(target)
            next_frontier.append((target, new_path))
        frontier = next_frontier
    return {"status": "ok", "paths": []}


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
        if depth == max_depth or len(result) >= max_items:
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


_DISCOVERY_INTERVAL_BASE = 1 << 40


def _insert_knowledge_assertions(connection: sqlite3.Connection, world: World) -> int:
    """Copy validated authored belief literals; never join to canonical truth."""
    count = 0
    for record in sorted(world.by_kind("knowledge"), key=lambda item: item.id):
        claim = record.frontmatter.get("claim") or {}
        if record.status != "canonical" or "genealogy" not in claim:
            continue
        value = claim["genealogy"]
        assertion = GenealogyAssertion.from_value(value)
        learned = [(StoryTime.from_value(t["time"], world.default_timeline), ordinal, t)
                   for ordinal, t in enumerate(record.frontmatter["transitions"])
                   if t["state"] in AFFIRMATIVE_STATES]
        point, _, transition = min(learned, key=lambda item: (item[0].tick, item[0].order, item[1]))
        until = assertion.valid_until
        connection.execute("INSERT INTO generational_knowledge_assertion VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
                           (record.id, record.frontmatter["knower"], assertion.kind,
                            assertion.valid_from.timeline, assertion.valid_from.tick, assertion.valid_from.order,
                            None if until is None else until.tick, None if until is None else until.order,
                            canonical_json(value["payload"]), canonical_json(value.get("labels", {})),
                            point.tick, point.order, transition["id"]))
        for field, endpoint in value["payload"].items():
            if field.endswith("_id") or field == "participant_ids":
                for ordinal, identifier in enumerate(endpoint if isinstance(endpoint, list) else [endpoint]):
                    if identifier is not None:
                        connection.execute("INSERT INTO generational_knowledge_endpoint VALUES (?,?,?,?)",
                                           (record.id, field, ordinal, identifier))
        for ordinal, evidence in enumerate(value.get("evidence", [])):
            field = next(key for key in evidence if key not in {"kind", "entity_id"})
            target = world.get(evidence["entity_id"])
            collection = {"knowledge": "transitions", "observation": "observations",
                          "turn": "turns", "recollection": "recollections"}[evidence["kind"]]
            exact = next(item for item in target.frontmatter[collection] if item["id"] == evidence[field])
            evidence_at = (turn_time(exact, target, world.default_timeline) if evidence["kind"] == "turn"
                           else StoryTime.from_value(exact["time" if evidence["kind"] == "knowledge" else "at"], world.default_timeline))
            connection.execute("INSERT INTO generational_knowledge_evidence VALUES (?,?,?,?,?,?,?,?)",
                               (record.id, ordinal, evidence["kind"], evidence["entity_id"], evidence[field],
                                evidence_at.timeline, evidence_at.tick, evidence_at.order))
        count += 1
    return count


def fold_knowledge_assertion(connection: sqlite3.Connection, knowledge_id: str, *,
                             knower_id: str, at: StoryTime,
                             include_forgotten: bool = False) -> dict[str, Any] | None:
    """Internal literal replay; held knowledge and current applicability differ.

    This primitive never admits another knower or resolves author metadata.
    Non-held states retain private history but cannot supply an affirmative edge.
    Callers own bounded selection and character-facing output projection.
    """
    assertion = connection.execute(
        "SELECT * FROM generational_knowledge_assertion WHERE knowledge_id=? AND knower_id=? AND timeline=? "
        "AND (learned_tick<? OR (learned_tick=? AND learned_order<=?))",
        (knowledge_id, knower_id, at.timeline, at.tick, at.tick, at.order)).fetchone()
    if assertion is None:
        return None
    transition = connection.execute(
        "SELECT transition_id,tick,ordering,state,confidence FROM knowledge_transition "
        "WHERE knowledge_id=? AND timeline=? AND (tick<? OR (tick=? AND ordering<=?)) "
        "ORDER BY tick DESC,ordering DESC,ordinal DESC,transition_id DESC LIMIT 1",
        (knowledge_id, at.timeline, at.tick, at.tick, at.order)).fetchone()
    if transition is None or (transition["state"] == "forgotten" and not include_forgotten):
        return None
    held = transition["state"] in AFFIRMATIVE_STATES
    coordinate = (at.tick, at.order)
    applicable = ((assertion["from_tick"], assertion["from_order"]) <= coordinate and
                  (assertion["until_tick"] is None or coordinate <= (assertion["until_tick"], assertion["until_order"])))
    evidence = [{"kind": row["kind"], "entityId": row["entity_id"], "itemId": row["item_id"],
                 "time": {"timeline": row["timeline"], "tick": row["tick"], "order": row["ordering"]}}
                for row in connection.execute(
                    "SELECT kind,entity_id,item_id,timeline,tick,ordering FROM generational_knowledge_evidence WHERE knowledge_id=? ORDER BY ordinal",
                    (knowledge_id,))]
    return {"knowledgeId": knowledge_id, "kind": assertion["kind"], "state": transition["state"],
            "learnedAt": {"timeline": assertion["timeline"], "tick": assertion["learned_tick"], "order": assertion["learned_order"]},
            "learningTransitionId": assertion["learning_transition_id"],
            "confidence": transition["confidence"], "held": held, "applicable": applicable,
            "affirmativeEdge": held and applicable, "payload": json.loads(assertion["payload_json"]),
            "labels": json.loads(assertion["labels_json"]), "evidence": evidence,
            "citation": {"knowledgeId": knowledge_id, "transitionId": transition["transition_id"],
                         "time": {"timeline": at.timeline, "tick": transition["tick"], "order": transition["ordering"]},
                         "state": transition["state"]}}


def insert_generational_index(
    connection: sqlite3.Connection, world: World, at: StoryTime | None = None,
) -> dict[str, int]:
    """Insert deterministic rows after validated entities, before SQL indexes."""
    current = at or effective_time(world)
    knowledge_count = _insert_knowledge_assertions(connection, world)
    records = [record for record in sorted(world.records.values(), key=lambda item: item.source_path)
               if record.kind in GENERATIONAL_KINDS]
    # These are disposable, private name admissions. A linked entity's source
    # title is never admitted before a canonical generational fact cites it.
    names: dict[tuple[str, str, str, str, str], tuple[int, int, int, str, str, str]] = {}
    title_changes: dict[str, list[tuple[str, int, int, bool]]] = {}
    title_instants: set[tuple[str, str, str, int, int]] = set()
    lenses: set[tuple[str, str]] = {("public", "ordinary")}
    for source_ordinal, record in enumerate(records):
        item = generational_record(record.frontmatter)
        fields = item.fields
        lenses.update((audience, perspective) for audience in fields["audience"]
                       for perspective in fields["perspectives"])
        point = item.initialization.applicability.start()
        assert point is not None
        connection.execute(
            "INSERT INTO generational_record VALUES (?,?,?,?,?,?,?,?,?,?)",
            (record.id, record.kind, source_ordinal, record.source_path, record.blob_oid,
             record.status, "generational-core-v1", point.timeline, canonical_json(fields["audience"]),
             canonical_json(fields["perspectives"])),
        )
        kind = record.kind
        postings: dict[tuple[str, str, str, str], tuple[int, int, str]] = {}
        if kind == "organization":
            connection.execute("INSERT INTO generational_organization VALUES (?,?,?,?)",
                               (record.id, fields["organization_kind"], fields.get("parent_id"), fields.get("location_id")))
        elif kind == "parentage":
            connection.execute("INSERT INTO generational_parentage VALUES (?,?,?,?,?,?,?)",
                               (record.id, fields["child_id"], fields["parent_id"],
                                point.timeline, point.tick, point.order, source_ordinal))
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
            if record.status == "canonical" and kind in {"organization", "legacy"}:
                literal_title = transition.payload.get("title")
                if isinstance(literal_title, str):
                    title_changes.setdefault(record.id, []).append(
                        (start.timeline, start.tick, start.order,
                         len(literal_title.encode("utf-8")) <= 256))
                    title_instants.update(
                        (audience, perspective, start.timeline, start.tick, start.order)
                        for audience in fields["audience"]
                        for perspective in fields["perspectives"])
            if record.status == "canonical" and not (
                kind == "vital-history" and
                (fields["disclosure"] != "known" or transition.kind == "vital-initialize")
            ):
                linked = [record.id] if kind in {"organization", "legacy"} else []
                for field in ("child_id", "parent_id", "character_id", "claimant_id",
                              "holder_id", "organization_id", "legacy_id"):
                    value = transition.payload.get(field, fields.get(field))
                    if isinstance(value, str):
                        linked.append(value)
                linked.extend(value for value in transition.payload.get("participant_ids", ())
                              if isinstance(value, str))
                if transition.cause_event_id:
                    linked.append(transition.cause_event_id)
                for entity_id in set(linked):
                    entity = world.maybe_get(entity_id)
                    if entity is None or entity.kind not in {"character", "organization", "legacy", "event"}:
                        continue
                    if entity.kind in {"organization", "legacy"} and entity_id != record.id:
                        continue
                    if entity_id == record.id and entity.kind in {"organization", "legacy"}:
                        title = transition.payload.get("title")
                        if not isinstance(title, str):
                            continue
                        labels = (title, *transition.payload.get("aliases", ()))
                    else:
                        title = entity.title
                        labels = (title, *entity.aliases)
                    if len(title.encode("utf-8")) > 256:
                        continue
                    for label in labels:
                        if (not isinstance(label, str) or not label.strip()
                                or len(label.encode("utf-8")) > 256):
                            continue
                        normalized = label.strip().casefold()
                        for audience in fields["audience"]:
                            for perspective in fields["perspectives"]:
                                key = (audience, perspective, start.timeline, normalized, entity_id)
                                posting = (start.tick, start.order, source_ordinal,
                                           entity.kind, label, title)
                                if key not in names or posting[:3] < names[key][:3]:
                                    names[key] = posting
            # Keep one earliest indexed posting per structural prefix and
            # authorized audience/perspective lane. A later transition cannot
            # make its token visible before its own applicability instant.
            if (record.status == "canonical" and
                    (kind != "vital-history" or
                     (fields["disclosure"] == "known" and transition.kind != "vital-initialize"))):
                for prefix in _search_prefixes(structure):
                    for audience in fields["audience"]:
                        for perspective in fields["perspectives"]:
                            key = (prefix, audience, perspective, start.timeline)
                            point = (start.tick, start.order, transition.id)
                            if key not in postings or point < postings[key]:
                                postings[key] = point
        if postings:
            connection.executemany(
                "INSERT INTO generational_search_prefix VALUES (?,?,?,?,?,?,?,?,?)",
                [(prefix, audience, perspective, timeline, tick, order,
                  source_ordinal, record.id, transition_id)
                 for (prefix, audience, perspective, timeline), (tick, order, transition_id)
                 in sorted(postings.items())],
            )
    candidates = frozenset(record.id for record in records)
    connection.executemany("INSERT INTO generational_discovery_lens VALUES (?,?)",
                           sorted(lenses))
    if names:
        connection.executemany(
            "INSERT INTO generational_discovery_name VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            [(audience, perspective, timeline, name_key, entity_id,
              kind, name, title, tick, order, ordinal)
             for (audience, perspective, timeline, name_key, entity_id),
                 (tick, order, ordinal, kind, name, title) in sorted(names.items())],
        )
        instants = sorted(title_instants | {
            (audience, perspective, timeline, tick, order)
            for (audience, perspective, timeline, _, _),
                (tick, order, _, _, _, _) in names.items()})
        ranks: dict[tuple[str, str, str, int, int], int] = {}
        lengths: dict[tuple[str, str, str], int] = {}
        for audience, perspective, timeline, tick, order in instants:
            lane = (audience, perspective, timeline)
            lengths[lane] = lengths.get(lane, 0) + 1
            ranks[(audience, perspective, timeline, tick, order)] = lengths[lane]
        connection.executemany("INSERT INTO generational_discovery_time VALUES (?,?,?,?,?,?)",
                               [(audience, perspective, timeline, tick, order,
                                 ranks[(audience, perspective, timeline, tick, order)])
                                for audience, perspective, timeline, tick, order in instants])
        valid_ranges = {}
        for entity_id, changes in title_changes.items():
            record = world.get(entity_id)
            item = generational_record(record.frontmatter)
            for audience in item.fields["audience"]:
                for perspective in item.fields["perspectives"]:
                    lane = (audience, perspective, changes[0][0])
                    spans = []
                    for index, (timeline, tick, order, valid) in enumerate(changes):
                        first = ranks[(audience, perspective, timeline, tick, order)]
                        last = (ranks[(audience, perspective, *changes[index + 1][:3])] - 1
                                if index + 1 < len(changes) else lengths[lane])
                        if valid and first <= last:
                            spans.append((first, last))
                    valid_ranges[(audience, perspective, entity_id)] = spans
        segments = []
        if any(length >= _DISCOVERY_INTERVAL_BASE for length in lengths.values()):
            raise ValueError("discovery time capacity exceeded")
        for (audience, perspective, timeline, name_key, entity_id), posting in sorted(names.items()):
            tick, order, _, kind, name, title = posting
            admitted = ranks[(audience, perspective, timeline, tick, order)]
            lane = (audience, perspective, timeline)
            if kind in {"organization", "legacy"}:
                for first, last in valid_ranges[(audience, perspective, entity_id)]:
                    left = _DISCOVERY_INTERVAL_BASE + max(first, admitted) - 1
                    right = _DISCOVERY_INTERVAL_BASE + last - 1
                    while left <= right:
                        if left & 1:
                            segments.append((audience, perspective, timeline, kind, left,
                                             name_key, entity_id, name, title))
                            left += 1
                        if not right & 1:
                            segments.append((audience, perspective, timeline, kind, right,
                                             name_key, entity_id, name, title))
                            right -= 1
                        left //= 2
                        right //= 2
            else:
                node = admitted
                while node <= lengths[lane]:
                    segments.append((audience, perspective, timeline, kind, node,
                                     name_key, entity_id, name, title))
                    node += node & -node
        connection.executemany(
            "INSERT INTO generational_discovery_segment VALUES (?,?,?,?,?,?,?,?,?)", segments)
    for record in records:
        folded = fold_record(connection, record.id, at=current, candidate_ids=candidates)
        if folded is not None:
            connection.execute(
                "INSERT INTO generational_current VALUES (?,?,?,?,?,?)",
                (record.id, current.timeline, current.tick, current.order,
                 folded["state"], canonical_json(folded)),
            )
    return {"generationalRecords": len(records),
            "generationalKnowledgeAssertions": knowledge_count,
            "generationalTransitions": connection.execute("SELECT COUNT(*) FROM generational_transition").fetchone()[0]}
