from __future__ import annotations

from dataclasses import dataclass
import sqlite3
from typing import Any

from . import THREAD_SOURCE_SCHEMA
from .errors import UsageError
from .ids import valid_id


@dataclass(frozen=True, slots=True, init=False)
class ThreadFilter:
    """Validated internal selector for records in one or more narrative threads."""

    thread_ids: tuple[str, ...]

    @classmethod
    def _from_normalized(cls, thread_ids: tuple[str, ...]) -> ThreadFilter:
        result = object.__new__(cls)
        object.__setattr__(result, "thread_ids", thread_ids)
        return result


def resolve_thread_filter(
    connection: sqlite3.Connection,
    thread_ids: tuple[str, ...] | None,
) -> ThreadFilter | None:
    """Validate a private thread selector against the compiled v0.5 world."""

    if thread_ids is None:
        return None
    normalized_ids = tuple(thread_ids)
    if not normalized_ids:
        raise UsageError("thread filter must contain at least one thread id")
    for thread_id in normalized_ids:
        if not valid_id(thread_id, "thread"):
            raise UsageError("thread filter id must use the thread_<26 Crockford> format")
    if len(set(normalized_ids)) != len(normalized_ids):
        raise UsageError("thread filter thread ids must be unique")
    if tuple(sorted(normalized_ids)) != normalized_ids:
        raise UsageError("thread filter thread ids must be sorted")

    revision = connection.execute("SELECT source_schema FROM revision LIMIT 1").fetchone()
    if revision is None or revision[0] != THREAD_SOURCE_SCHEMA:
        raise UsageError("thread filtering requires a validated wedl/v0.5 world")

    placeholders = ",".join("?" for _ in normalized_ids)
    declared = {
        str(row[0])
        for row in connection.execute(
            f"SELECT id FROM narrative_thread WHERE id IN ({placeholders})", normalized_ids
        ).fetchall()
    }
    for thread_id in normalized_ids:
        if thread_id not in declared:
            raise UsageError("thread filter id is not declared by the world")
    return ThreadFilter._from_normalized(normalized_ids)


def filter_ranked_candidates(
    connection: sqlite3.Connection,
    candidates: list[dict[str, Any]],
    thread_filter: ThreadFilter,
) -> list[dict[str, Any]]:
    """Return the stable ANY-thread subsequence using one bounded lookup."""

    entity_ids = tuple(dict.fromkeys(str(item["entityId"]) for item in candidates))
    if not entity_ids:
        return []
    thread_placeholders = ",".join("?" for _ in thread_filter.thread_ids)
    entity_placeholders = ",".join("?" for _ in entity_ids)
    rows = connection.execute(
        f"SELECT DISTINCT record_id FROM record_thread "
        f"WHERE thread_id IN ({thread_placeholders}) "
        f"AND record_id IN ({entity_placeholders})",
        [*thread_filter.thread_ids, *entity_ids],
    ).fetchall()
    included = {str(row[0]) for row in rows}
    return [item for item in candidates if str(item["entityId"]) in included]


def resolve_membership_record_ids(record_ids: tuple[str, ...]) -> tuple[str, ...]:
    """Validate the public bounded record projection selector."""

    normalized_ids = tuple(record_ids)
    if not normalized_ids:
        raise UsageError("thread membership projection requires at least one record id")
    if len(normalized_ids) > 256:
        raise UsageError("thread membership projection accepts at most 256 record ids")
    if any(not valid_id(record_id) for record_id in normalized_ids):
        raise UsageError("thread membership record ids must use stable entity ID format")
    if len(set(normalized_ids)) != len(normalized_ids):
        raise UsageError("thread membership record ids must be unique")
    if tuple(sorted(normalized_ids)) != normalized_ids:
        raise UsageError("thread membership record ids must be sorted")
    return normalized_ids


def project_thread_memberships(
    connection: sqlite3.Connection,
    record_ids: tuple[str, ...],
    thread_filter: ThreadFilter,
) -> list[dict[str, Any]]:
    """Return the bounded selected-thread intersection for canonical records.

    Candidate ids are intentionally joined to public canonical records before
    any membership data is read.  Unknown, hypothesis, and unavailable ids
    therefore have the same empty result and cannot become a membership oracle.
    """

    record_placeholders = ",".join("?" for _ in record_ids)
    thread_placeholders = ",".join("?" for _ in thread_filter.thread_ids)
    rows = connection.execute(
        f"SELECT record_thread.record_id, record_thread.thread_id "
        f"FROM record_thread JOIN entity ON entity.id = record_thread.record_id "
        f"WHERE record_thread.record_id IN ({record_placeholders}) "
        f"AND record_thread.thread_id IN ({thread_placeholders}) "
        # Keep this SQL equivalent to validation.is_adoptable_canonical_record:
        # settled scenes/conversations use their active/closed lifecycle rather
        # than the generic canonical record status vocabulary.
        f"AND entity.kind != 'hypothesis' AND ("
        f"(entity.kind IN ('scene', 'conversation') AND entity.status IN ('active', 'closed', 'retired')) "
        f"OR (entity.kind NOT IN ('scene', 'conversation') AND entity.status IN ('canonical', 'retired'))"
        f") "
        f"ORDER BY record_thread.record_id, record_thread.thread_id",
        [*record_ids, *thread_filter.thread_ids],
    ).fetchall()
    grouped: dict[str, list[str]] = {}
    for row in rows:
        grouped.setdefault(str(row[0]), []).append(str(row[1]))
    return [
        {"recordId": record_id, "threadIds": thread_ids}
        for record_id, thread_ids in sorted(grouped.items())
    ]
