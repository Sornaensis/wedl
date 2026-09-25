"""Horizon-safe, API-neutral reads of authored generational evidence.

Transport adapters must construct ``TrustedViewerScope`` from an authenticated
principal.  Request data never supplies the viewer's identity or access sets.
"""
from __future__ import annotations

from dataclasses import dataclass
from contextlib import closing
import base64
import hashlib
import json
import re
import sqlite3
from typing import Any, Collection, Mapping

from .compiler import connect, require_database
from .conversation import scene_context_time
from .errors import CompileRequired, ValidationFailed
from .generational_index import cited_ancestors, cited_containment, cited_descendants, cited_relative_path, fold_record
from .ids import valid_id
from .model import ORDER_MAX, TICK_MAX, StoryTime
from .repository import Repository
from .util import canonical_json


PROTOCOL = "wedl-generational/v1"
CAPABILITY = "generational-core-v1"
_DECIMAL = re.compile(r"(?:0|-?[1-9][0-9]*)\Z")
_OPS = frozenset({"parents", "ancestors", "descendants", "relatives", "union",
                  "organization", "legacy", "vital", "search"})
_REQUEST_KEYS = frozenset({"operation", "subject_id", "target_id", "text",
                           "depth", "items", "cursor", "includeFormerRoles"})


@dataclass(frozen=True, slots=True)
class TrustedViewerScope:
    """Server-derived access context, kept separate from untrusted selectors."""

    revision: str
    mode: str
    timeline: str
    at: StoryTime | None
    audiences: frozenset[str]
    perspectives: frozenset[str]
    capabilities: frozenset[str]
    character_id: str | None = None

    @classmethod
    def from_trusted_adapter(
        cls, *, revision: str, mode: str, timeline: str,
        at: Mapping[str, Any] | None, server_audiences: Collection[str],
        server_perspectives: Collection[str], capabilities: Collection[str],
        authenticated_character_id: str | None = None,
    ) -> "TrustedViewerScope":
        """Parse exact signed transport only after principal authentication."""
        point = None if at is None else _point(at)
        if at is not None and point is None:
            raise ValueError("GEN-REQUEST-001")
        return cls(revision, mode, timeline, point, frozenset(server_audiences),
                   frozenset(server_perspectives), frozenset(capabilities),
                   authenticated_character_id)


def _closed(state: str, code: str | None = None) -> dict[str, Any]:
    result: dict[str, Any] = {"state": state}
    if code is not None:
        result["code"] = code
    return result


def _point(value: Any) -> StoryTime | None:
    if not isinstance(value, Mapping) or set(value) != {"timeline", "tick", "order"}:
        return None
    if not isinstance(value["timeline"], str) or not value["timeline"]:
        return None
    if not isinstance(value["tick"], str) or not _DECIMAL.fullmatch(value["tick"]):
        return None
    if not isinstance(value["order"], str) or not _DECIMAL.fullmatch(value["order"]):
        return None
    try:
        return StoryTime(value["timeline"], int(value["tick"]), int(value["order"]))
    except ValueError:
        return None


def _validated_scope(scope: TrustedViewerScope) -> tuple[StoryTime | None, dict[str, Any] | None]:
    if (not isinstance(scope, TrustedViewerScope)
            or not isinstance(scope.revision, str) or not scope.revision
            or not isinstance(scope.timeline, str) or not scope.timeline):
        return None, _closed("invalid", "GEN-REQUEST-001")
    if not scope.audiences or not scope.perspectives or any(
        not isinstance(value, str) or not value for value in (*scope.audiences, *scope.perspectives)
    ) or len(scope.audiences) > 16 or len(scope.perspectives) > 16:
        return None, _closed("invalid", "GEN-REQUEST-001")
    if CAPABILITY not in scope.capabilities:
        return None, _closed("unavailable")
    if scope.mode not in {"author-as-of", "author-all-time", "character"}:
        return None, _closed("invalid", "GEN-REQUEST-001")
    if scope.mode == "author-all-time":
        if scope.at is not None or scope.character_id is not None:
            return None, _closed("invalid", "GEN-REQUEST-001")
        return StoryTime(scope.timeline, TICK_MAX, ORDER_MAX), None
    if scope.mode == "author-as-of" and scope.at is None:
        return None, None
    if not isinstance(scope.at, StoryTime):
        return None, _closed("invalid", "GEN-REQUEST-001")
    if scope.at.timeline != scope.timeline:
        return None, _closed("invalid", "GEN-TIME-001")
    if scope.mode == "character" and not valid_id(scope.character_id, "character"):
        return None, _closed("invalid", "GEN-REQUEST-001")
    if scope.mode == "author-as-of" and scope.character_id is not None:
        return None, _closed("invalid", "GEN-REQUEST-001")
    return scope.at, None


def _cursor_key(scope: TrustedViewerScope, request: Mapping[str, Any]) -> str:
    material = {"revision": scope.revision, "mode": scope.mode,
                "timeline": scope.timeline, "at": None if scope.at is None else scope.at.to_dict(),
                "audiences": sorted(scope.audiences), "perspectives": sorted(scope.perspectives),
                "character": scope.character_id, "capabilities": sorted(scope.capabilities),
                "request": {key: value for key, value in request.items() if key != "cursor"}}
    return hashlib.sha256(canonical_json(material).encode()).hexdigest()


def _decode_cursor(value: Any, key: str) -> str | None:
    if value is None:
        return ""
    if not isinstance(value, str) or len(value) > 1024:
        return None
    try:
        data = json.loads(base64.urlsafe_b64decode(value + "=" * (-len(value) % 4)))
    except (ValueError, UnicodeError):
        return None
    return data.get("after") if isinstance(data, dict) and data.get("scope") == key and isinstance(data.get("after"), str) else None


def _encode_cursor(key: str, after: str) -> str:
    return base64.urlsafe_b64encode(canonical_json({"scope": key, "after": after}).encode()).decode().rstrip("=")


def _applicability(row: sqlite3.Row) -> dict[str, Any]:
    kind = str(row["applicability_kind"])
    point = {"timeline": str(row["timeline"]), "tick": str(row["start_tick"]),
             "order": str(row["start_order"])}
    if kind == "inclusive-interval":
        return {"applicability_kind": kind, "first": point,
                "last": {"timeline": point["timeline"], "tick": str(row["end_tick"]),
                         "order": str(row["end_order"])}}
    return {"applicability_kind": kind, "point": point}


def _citations(connection: sqlite3.Connection, folded: Mapping[str, Any]) -> list[dict[str, Any]]:
    rows = connection.execute(
        "SELECT * FROM generational_transition WHERE record_id=? ORDER BY start_tick,start_order,source_ordinal,id",
        (folded["recordId"],),
    )
    by_section = {json.loads(str(row["citation_json"]))["section"]: row for row in rows}
    result = []
    for source in folded["citations"]:
        for item in (source, *source.get("supersededCitations", [])):
            row = by_section.get(item["section"])
            if row is not None:
                result.append({"record_id": folded["recordId"], "path": item["sourcePath"],
                               "applicability": _applicability(row)})
    return result


def _causes(connection: sqlite3.Connection, folded: Mapping[str, Any],
            at: StoryTime) -> list[dict[str, Any]]:
    result = []
    for source in folded["citations"]:
        cause = source.get("causeCitation")
        if not isinstance(cause, dict):
            continue
        event = connection.execute(
            "SELECT timeline,tick,ordering FROM event WHERE entity_id=?", (cause["entityId"],)
        ).fetchone()
        if event is None or event["timeline"] != at.timeline or (
            int(event["tick"]), int(event["ordering"])) > (at.tick, at.order):
            continue
        result.append({"eventId": cause["entityId"], "citation": {
            "record_id": cause["entityId"], "path": cause["sourcePath"],
            "applicability": {"applicability_kind": "instant", "point": {
                "timeline": str(event["timeline"]), "tick": str(event["tick"]),
                "order": str(event["ordering"])}}}})
    return result


class _VisibleCandidates(Collection[str]):
    """Lazy authorization at each indexed edge, before fold or derivation."""

    def __init__(self, connection: sqlite3.Connection, scope: TrustedViewerScope, at: StoryTime):
        self.connection, self.scope, self.at = connection, scope, at
        self._cache: dict[str, bool] = {}

    def __contains__(self, record_id: object) -> bool:
        if not isinstance(record_id, str):
            return False
        if record_id in self._cache:
            return self._cache[record_id]
        row = self.connection.execute(
            "SELECT status,capability,timeline,audience_json,perspectives_json "
            "FROM generational_record WHERE id=?", (record_id,),
        ).fetchone()
        visible = bool(row and row["status"] == "canonical" and row["capability"] in self.scope.capabilities
                       and row["timeline"] == self.scope.timeline
                       and self.scope.audiences.intersection(json.loads(str(row["audience_json"])))
                       and self.scope.perspectives.intersection(json.loads(str(row["perspectives_json"]))))
        if visible:
            visible = self.connection.execute(
                "SELECT 1 FROM generational_candidate WHERE record_id=? AND timeline=? AND "
                "(start_tick<? OR (start_tick=? AND start_order<=?)) LIMIT 1",
                (record_id, self.scope.timeline, self.at.tick, self.at.tick, self.at.order),
            ).fetchone() is not None
        # Existing knowledge/observation/recollection grammar has no validated
        # exact structural fact reference. Neither prose nor source_entity is a
        # grant. An explicit schema/ADR change is required before opening this.
        if self.scope.mode == "character":
            visible = False
        self._cache[record_id] = visible
        return visible

    def __iter__(self):
        raise TypeError("visibility set permits indexed membership only")

    def __len__(self):
        raise TypeError("visibility set cannot reveal a global count")


def _fold(connection: sqlite3.Connection, ident: str, at: StoryTime,
          visible: _VisibleCandidates) -> dict[str, Any] | None:
    value = fold_record(connection, ident, at=at, candidate_ids=visible)
    if value is None:
        return None
    safe_value = _safe_references(connection, dict(value["value"]), visible)
    result = {"recordId": value["recordId"], "kind": value["kind"],
            "state": value["state"], "value": safe_value,
            "citations": _citations(connection, value),
            "causes": _causes(connection, value, at)}
    if visible.scope.mode == "author-all-time":
        history = []
        for row in connection.execute(
            "SELECT * FROM generational_transition WHERE record_id=? AND timeline=? "
            "ORDER BY start_tick,start_order,source_ordinal,id", (ident, at.timeline)
        ):
            source = json.loads(str(row["citation_json"]))
            history.append({"transitionId": row["id"], "kind": row["transition_kind"],
                            "payload": _safe_references(connection, json.loads(str(row["payload_json"])), visible),
                            **({"causeEventId": row["cause_event_id"]} if row["cause_event_id"] else {}),
                            "citation": {"record_id": ident, "path": source["sourcePath"],
                                         "applicability": _applicability(row)}})
        result["history"] = history
    return result


def _safe_references(connection: sqlite3.Connection, value: dict[str, Any],
                     visible: _VisibleCandidates) -> dict[str, Any]:
    for key, reference in list(value.items()):
        if key.endswith("_id") and isinstance(reference, str):
            is_generational = connection.execute(
                "SELECT 1 FROM generational_record WHERE id=?", (reference,)
            ).fetchone() is not None
            if is_generational and reference not in visible:
                value.pop(key)
        elif key == "competes_with" and isinstance(reference, list):
            value[key] = [ident for ident in reference if ident in visible]
    return value


def _bounded_rows(connection: sqlite3.Connection, sql: str, params: tuple[Any, ...],
                  visible: _VisibleCandidates, at: StoryTime, items: int,
                  states: frozenset[str] | None = None) -> tuple[list[dict[str, Any]], bool]:
    result = []
    for row in connection.execute(sql, params):
        ident = str(row[0])
        if ident not in visible:
            continue
        if _history_exceeds(connection, ident, visible.scope, items):
            return [], True
        folded = _fold(connection, ident, at, visible)
        if folded is None or (states is not None and folded["state"] not in states):
            continue
        if len(result) == items:
            return [], True
        result.append(folded)
    return result, False


def _history_exceeds(connection: sqlite3.Connection, ident: str,
                     scope: TrustedViewerScope, items: int) -> bool:
    if scope.mode != "author-all-time":
        return False
    return len(connection.execute(
        "SELECT 1 FROM generational_transition WHERE record_id=? AND timeline=? "
        "ORDER BY start_tick,start_order,source_ordinal,id LIMIT ?",
        (ident, scope.timeline, items + 1),
    ).fetchall()) > items


def _ordered_ids(table: str, field: str, condition: str = "") -> str:
    """Use the typed endpoint index, then authored applicability order."""
    return (
        f"SELECT typed.id FROM {table} AS typed "
        "JOIN generational_record AS source ON source.id=typed.id "
        "JOIN generational_transition AS initial ON initial.record_id=typed.id "
        f"WHERE typed.{field}=? AND initial.source_ordinal=0 {condition} "
        "ORDER BY initial.start_tick,initial.start_order,source.source_ordinal,typed.id"
    )


def query_connection(connection: sqlite3.Connection, scope: TrustedViewerScope,
                     request: Mapping[str, Any]) -> dict[str, Any]:
    """Read a pinned compiled connection; only filtered data enters results."""
    at, failure = _validated_scope(scope)
    if failure:
        return failure
    if at is None:
        return _closed("invalid", "GEN-REQUEST-001")
    if not isinstance(request, Mapping) or set(request) - _REQUEST_KEYS:
        return _closed("invalid", "GEN-REQUEST-001")
    operation, subject = request.get("operation"), request.get("subject_id")
    if (not isinstance(operation, str) or operation not in _OPS
            or (operation != "search" and (not isinstance(subject, str) or not subject))):
        return _closed("invalid", "GEN-REQUEST-001")
    former_roles = "includeFormerRoles" in request
    if former_roles and (request["includeFormerRoles"] is not True
                         or operation != "organization" or scope.mode != "author-as-of"
                         or scope.at is None or "cursor" in request):
        return _closed("invalid", "GEN-REQUEST-001")
    target_kind = {"parents": "character", "ancestors": "character",
                   "descendants": "character", "relatives": "character", "vital": "character",
                   "union": "union", "organization": "organization", "legacy": "legacy"}
    if operation != "search" and not valid_id(subject, target_kind[operation]):
        return _closed("invalid", "GEN-REQUEST-001")
    depth, items = request.get("depth", 8), request.get("items", 100)
    if type(depth) is not int or type(items) is not int or not 0 <= depth <= 32 or not 1 <= items <= 500:
        return _closed("invalid", "GEN-REQUEST-001")
    if operation == "relatives" and not valid_id(request.get("target_id"), "character"):
        return _closed("invalid", "GEN-REQUEST-001")
    if operation == "search" and (
        not isinstance(request.get("text"), str) or not request["text"].strip()
        or re.fullmatch(r"\w{1,32}", request["text"].casefold(), re.UNICODE) is None
    ):
        return _closed("invalid", "GEN-REQUEST-001")
    key = _cursor_key(scope, request)
    after = _decode_cursor(request.get("cursor"), key)
    if after is None:
        return _closed("invalid", "GEN-REQUEST-001")
    visible = _VisibleCandidates(connection, scope, at)
    if scope.mode == "character":
        return _closed("unknown")
    if operation in {"ancestors", "descendants", "relatives"}:
        if operation == "relatives":
            result = cited_relative_path(connection, subject, request["target_id"], at=at,
                                         candidate_ids=visible, max_depth=depth,
                                         max_items=items)
        else:
            fn = cited_descendants if operation == "descendants" else cited_ancestors
            result = fn(connection, subject, at=at, candidate_ids=visible,
                        max_depth=depth, max_items=items)
        if result["status"] == "limit":
            return _closed("limit", "GEN-LIMIT-001")
        paths = result["paths"]
        relations = [{"targetId": path["targetId"], "generationDistance": len(path["edges"]),
                      "label": "relative-path" if operation == "relatives" else (
                          "descendant" if operation == "descendants" else "ancestor"),
                      "edges": [{"from": edge["from"], "to": edge["to"],
                                 "recordId": edge["edgeId"],
                                 "citations": _citations(connection, fold_record(
                                     connection, edge["edgeId"], at=at, candidate_ids=visible))}
                                for edge in path["edges"]]} for path in paths]
        return {"state": "available", "relations": relations} if relations else _closed("unknown")
    if operation == "parents":
        rows, limit = _bounded_rows(
            connection,
            _ordered_ids("generational_parentage", "child_id"),
            (subject,), visible, at, items,
            None if scope.mode == "author-all-time" else frozenset({"asserted", "confirmed"}))
        if limit:
            return _closed("limit", "GEN-LIMIT-001")
        relations = [{"targetId": item["value"]["parent_id"],
                      "label": item["value"].get("basis", "unknown") + "-parent",
                      "recordId": item["recordId"], "citations": item["citations"],
                      **({"history": item["history"]} if "history" in item else {})}
                     for item in rows if item["state"] in {"asserted", "confirmed"}
                     or scope.mode == "author-all-time"]
        return {"state": "available", "relations": relations} if relations else _closed("unknown")
    if operation == "vital":
        rows, limit = _bounded_rows(connection,
            _ordered_ids("generational_vital", "character_id", "AND typed.disclosure='known'"),
            (subject,), visible, at, items,
            frozenset({"living", "dead", "existing", "ended"}))
        if limit:
            return _closed("limit", "GEN-LIMIT-001")
        affirmative = [item for item in rows if item["state"] in {"living", "dead", "existing", "ended"}
                       and item["value"].get("disclosure") == "known"]
        return {"state": "available", "vital": affirmative[0]["state"],
                "citations": affirmative[0]["citations"],
                **({"history": affirmative[0]["history"]} if "history" in affirmative[0] else {})
                } if affirmative else _closed("unknown")
    if operation in {"union", "organization"}:
        if subject in visible and _history_exceeds(connection, subject, scope, items):
            return _closed("limit", "GEN-LIMIT-001")
        item = _fold(connection, subject, at, visible)
        if item is None or item["kind"] != operation:
            return _closed("unknown")
        if operation == "union":
            if len(item["value"].get("participant_ids", [])) > items:
                return _closed("limit", "GEN-LIMIT-001")
            return {"state": "available", "participants": item["value"].get("participant_ids", []),
                    "citations": item["citations"],
                    **({"history": item["history"]} if "history" in item else {})} if (
                        item["state"] in {"declared", "formed"} or scope.mode == "author-all-time"
                    ) else _closed("unknown")
        ancestors = cited_containment(connection, subject, at=at, candidate_ids=visible,
                                      max_depth=depth, max_items=items)
        if ancestors["status"] == "limit":
            return _closed("limit", "GEN-LIMIT-001")
        affiliations, limit = _bounded_rows(connection,
            "SELECT id FROM generational_affiliation WHERE organization_id=? ORDER BY id",
            (subject,), visible, at, items,
            None if scope.mode == "author-all-time" else
            frozenset({"active", "ended"}) if former_roles else frozenset({"active"}))
        if limit:
            return _closed("limit", "GEN-LIMIT-001")
        parent_paths = [{"targetId": path["targetId"], "edges": [
            {"from": edge["from"], "to": edge["to"], "recordId": edge["edgeId"],
             "citations": _citations(connection, fold_record(
                 connection, edge["edgeId"], at=at, candidate_ids=visible))}
            for edge in path["edges"]]} for path in ancestors["paths"]]
        return {"state": "available", "organization": item,
                "parentPath": parent_paths,
                "roles": [row for row in affiliations if row["state"] == "active"
                          or scope.mode == "author-all-time"],
                **({"formerRoles": [row for row in affiliations if row["state"] == "ended"]}
                   if former_roles else {})}
    if operation == "legacy":
        if subject in visible and _history_exceeds(connection, subject, scope, items):
            return _closed("limit", "GEN-LIMIT-001")
        legacy = _fold(connection, subject, at, visible)
        if legacy is None or legacy["kind"] != "legacy":
            return _closed("unknown")
        tenures, first_limit = _bounded_rows(connection,
            _ordered_ids("generational_tenure", "legacy_id"),
            (subject,), visible, at, items)
        claims, second_limit = _bounded_rows(connection,
            _ordered_ids("generational_claim", "legacy_id"),
            (subject,), visible, at, items)
        if first_limit or second_limit:
            return _closed("limit", "GEN-LIMIT-001")
        by_id = {row["recordId"]: row for row in tenures}
        succession = []
        for row in tenures:
            for field in ("predecessor_tenure_id", "successor_tenure_id"):
                linked = row["value"].get(field)
                if linked not in by_id:
                    continue
                first, second = ((linked, row["recordId"]) if field.startswith("predecessor")
                                 else (row["recordId"], linked))
                edge = {"from": first, "to": second, "citations": row["citations"],
                        "causes": row["causes"]}
                if edge not in succession:
                    if len(succession) == items:
                        return _closed("limit", "GEN-LIMIT-001")
                    succession.append(edge)
        return {"state": "available", "legacy": legacy, "tenures": tenures,
                "holders": [row for row in tenures if row["state"] == "holding"],
                "claims": claims, "succession": succession}
    cursor_key = None
    if after:
        try:
            value = json.loads(after)
            if (not isinstance(value, list) or len(value) != 4
                    or any(type(item) is not int for item in value[:3])
                    or not isinstance(value[3], str)):
                return _closed("invalid", "GEN-REQUEST-001")
            cursor_key = tuple(value)
        except (TypeError, ValueError):
            return _closed("invalid", "GEN-REQUEST-001")
    # Exact prefix postings are already distinct per record and normalized
    # audience/perspective lane. Every lane is an indexed, bounded range read.
    matches: dict[str, tuple[tuple[int, int, int, str], str]] = {}
    for audience in sorted(scope.audiences):
        for perspective in sorted(scope.perspectives):
            rows = connection.execute(
                "SELECT start_tick,start_order,source_ordinal,record_id,transition_id "
                "FROM generational_search_prefix INDEXED BY generational_search_lookup_idx "
                "WHERE audience=? AND perspective=? AND timeline=? AND prefix=? "
                "AND (start_tick<? OR (start_tick=? AND start_order<=?)) "
                "ORDER BY start_tick,start_order,source_ordinal,record_id LIMIT 501",
                (audience, perspective, scope.timeline, request["text"].casefold(),
                 at.tick, at.tick, at.order),
            ).fetchall()
            if len(rows) > 500:
                return _closed("limit", "GEN-LIMIT-001")
            for row in rows:
                ident = str(row["record_id"])
                sort_key = (int(row["start_tick"]), int(row["start_order"]),
                            int(row["source_ordinal"]), ident)
                if ident not in matches or sort_key < matches[ident][0]:
                    matches[ident] = (sort_key, str(row["transition_id"]))
    if len(matches) > 500:
        return _closed("limit", "GEN-LIMIT-001")
    ordered = sorted((sort_key, ident, transition_id)
                     for ident, (sort_key, transition_id) in matches.items()
                     if cursor_key is None or sort_key > cursor_key)
    selected = ordered[:items]
    page = []
    for _sort_key, ident, transition_id in selected:
        if ident not in visible:
            continue
        row = connection.execute(
            "SELECT r.kind,t.*,c.citation_json FROM generational_record AS r "
            "JOIN generational_transition AS t ON t.record_id=r.id "
            "JOIN generational_candidate AS c ON c.record_id=t.record_id AND c.transition_id=t.id "
            "WHERE r.id=? AND t.id=?", (ident, transition_id),
        ).fetchone()
        if row is None:
            continue
        source = json.loads(str(row["citation_json"]))
        page.append({"recordId": ident, "kind": row["kind"],
                     "citations": [{"record_id": ident, "path": source["sourcePath"],
                                    "applicability": _applicability(row)}]})
    cursor = None
    if len(ordered) > items and page:
        cursor = _encode_cursor(key, canonical_json(list(selected[-1][0])))
    return {"state": "available", "results": page, "cursor": cursor}


def query_generational(repository: Repository, scope: TrustedViewerScope,
                       request: Mapping[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    """Revision-bound repository entry point for a trusted transport adapter."""
    _at, failure = _validated_scope(scope)
    if failure:
        return failure
    if (scope.mode == "author-as-of" and scope.at is None
            and isinstance(request, Mapping) and "includeFormerRoles" in request):
        return _closed("invalid", "GEN-REQUEST-001")
    try:
        if repository.resolve(scope.revision) != scope.revision:
            return _closed("invalid", "GEN-REQUEST-001")
        world, database = require_database(repository, scope.revision,
                                           require_compiled=require_compiled)
    except (ValueError, OSError, CompileRequired, ValidationFailed):
        return _closed("unavailable")
    if world.revision != scope.revision or CAPABILITY not in (world.world_record.frontmatter.get("capabilities") or []):
        return _closed("unavailable")
    if scope.timeline not in world.timeline_ids:
        return _closed("invalid", "GEN-TIME-001")
    effective_scope = scope
    if scope.mode == "author-as-of" and scope.at is None:
        cursor = world.current_time
        if cursor is None:
            scene = world.active_scene()
            if scene is None:
                return _closed("invalid", "GEN-REQUEST-001")
            cursor = scene_context_time(scene, world.default_timeline)
        if cursor.timeline != scope.timeline:
            return _closed("invalid", "GEN-TIME-001")
        effective_scope = TrustedViewerScope(scope.revision, scope.mode, scope.timeline,
                                            cursor, scope.audiences,
                                            scope.perspectives, scope.capabilities)
    with closing(connect(database, True)) as connection:
        return query_connection(connection, effective_scope, request)
