from __future__ import annotations

from difflib import get_close_matches
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

from .errors import NotFound
from .util import slugify


TICK_MIN = -(2**63)
TICK_MAX = 2**63 - 1
ORDER_MIN = -(2**31)
ORDER_MAX = 2**31 - 1


@dataclass(frozen=True, order=True, slots=True)
class StoryTime:
    timeline: str
    tick: int
    order: int = 0

    def __post_init__(self) -> None:
        if not isinstance(self.timeline, str) or not self.timeline.strip():
            raise ValueError("timeline must be a non-empty string")
        self._validate_integer("tick", self.tick, TICK_MIN, TICK_MAX)
        self._validate_integer("order", self.order, ORDER_MIN, ORDER_MAX)

    @staticmethod
    def _validate_integer(field: str, value: Any, minimum: int, maximum: int) -> None:
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError(f"{field} must be an integer")
        if not minimum <= value <= maximum:
            raise ValueError(f"{field} must be between {minimum} and {maximum}")

    @classmethod
    def from_value(cls, value: Any, default_timeline: str = "main") -> "StoryTime":
        if isinstance(value, StoryTime):
            return value
        if not isinstance(value, dict) or "tick" not in value:
            raise ValueError(f"invalid story time: {value!r}")
        return cls(value.get("timeline", default_timeline), value["tick"], value.get("order", 0))

    def to_dict(self) -> dict[str, Any]:
        return {"timeline": self.timeline, "tick": self.tick, "order": self.order}

    def not_after(self, other: "StoryTime") -> bool:
        return self.timeline == other.timeline and (self.tick, self.order) <= (other.tick, other.order)


@dataclass(slots=True)
class Record:
    frontmatter: dict[str, Any]
    body: str
    source_path: str
    raw_bytes: bytes
    blob_oid: str | None = None
    revision: str | None = None

    @property
    def id(self) -> str:
        return str(self.frontmatter.get("id", ""))

    @property
    def kind(self) -> str:
        return str(self.frontmatter.get("kind", ""))

    @property
    def title(self) -> str:
        return str(self.frontmatter.get("title", self.id))

    @property
    def domain(self) -> str:
        return str(self.frontmatter.get("domain", ""))

    @property
    def status(self) -> str:
        return str(self.frontmatter.get("status", "canonical"))

    @property
    def tags(self) -> list[str]:
        return [str(value) for value in self.frontmatter.get("tags") or []]

    @property
    def aliases(self) -> list[str]:
        return [str(value) for value in self.frontmatter.get("aliases") or []]

    def citation(self, revision: str, section: str) -> dict[str, Any]:
        return {
            "entityId": self.id,
            "kind": self.kind,
            "sourcePath": self.source_path,
            "blobOid": self.blob_oid,
            "revision": revision,
            "section": section,
        }


@dataclass(slots=True)
class World:
    revision: str
    tree_oid: str
    records: dict[str, Record]
    root: Path
    source_root: str = "story"
    is_worktree: bool = False
    diagnostics: list[dict[str, Any]] = field(default_factory=list)
    _cache: dict[str, Any] = field(default_factory=dict, repr=False)

    def __iter__(self) -> Iterable[Record]:
        return iter(self.records.values())

    @property
    def world_record(self) -> Record:
        values = self.by_kind("world")
        if len(values) != 1:
            raise NotFound(f"expected exactly one world record, found {len(values)}")
        return values[0]

    @property
    def config(self) -> dict[str, Any]:
        return self.world_record.frontmatter

    @property
    def default_timeline(self) -> str:
        return str(self.config.get("default_timeline", "main"))

    @property
    def timeline_ids(self) -> frozenset[str]:
        timelines = self.config.get("timelines")
        if not isinstance(timelines, list):
            return frozenset()
        return frozenset(
            value["id"]
            for value in timelines
            if isinstance(value, dict) and isinstance(value.get("id"), str) and value["id"].strip()
        )

    def story_time(self, tick: int, timeline: str | None = None, order: int = 0) -> StoryTime:
        point = StoryTime(timeline if timeline is not None else self.default_timeline, tick, order)
        if point.timeline not in self.timeline_ids:
            raise ValueError(f"unknown timeline {point.timeline!r}")
        return point

    @property
    def current_time(self) -> StoryTime | None:
        """Return the explicitly authored world cursor, when one is present.

        Validation owns diagnostics for malformed cursors.  Keeping this
        accessor nullable lets read paths retain their useful legacy fallback
        while a source author is correcting a draft world.
        """
        value = self.config.get("current_time")
        if value is None:
            return None
        try:
            point = StoryTime.from_value(value, self.default_timeline)
        except (TypeError, ValueError):
            return None
        return point if point.timeline in self.timeline_ids else None

    def get(self, entity_id: str) -> Record:
        try:
            return self.records[entity_id]
        except KeyError as exc:
            raise NotFound(f"unknown entity {entity_id}") from exc

    def maybe_get(self, entity_id: str | None) -> Record | None:
        return self.records.get(entity_id or "")

    def by_kind(self, kind: str) -> list[Record]:
        key = f"kind:{kind}"
        if key not in self._cache:
            self._cache[key] = tuple(
                sorted(
                    (record for record in self.records.values() if record.kind == kind),
                    key=lambda record: (record.title.casefold(), record.id),
                )
            )
        return list(self._cache[key])

    def find(self, value: str, kind: str | None = None) -> Record:
        if value in self.records:
            record = self.records[value]
            if kind is None or record.kind == kind:
                return record
        folded = value.casefold()
        records = self.by_kind(kind) if kind else sorted(self.records.values(), key=lambda record: (record.title.casefold(), record.id))
        name_matches = [
            record
            for record in records
            if folded in self._name_references(record)
        ]
        if name_matches:
            return self._resolve_exact_matches(value, name_matches)

        slug_matches = [record for record in records if folded in self._slug_references(record)]
        if slug_matches:
            return self._resolve_exact_matches(value, slug_matches)

        suggestions = self._reference_suggestions(value, records)
        message = f"could not resolve {value!r}"
        if suggestions:
            references = ", ".join(repr(item["reference"]) for item in suggestions)
            message += f"; did you mean {references}?"
        raise NotFound(message, details={"suggestions": suggestions} if suggestions else None)

    @staticmethod
    def _name_references(record: Record) -> set[str]:
        names = (record.title, *record.aliases)
        return {name.casefold() for name in names}

    @staticmethod
    def _slug_references(record: Record) -> set[str]:
        return {slugify(name).casefold() for name in (record.title, *record.aliases)}

    def _resolve_exact_matches(self, value: str, matches: list[Record]) -> Record:
        if len(matches) == 1:
            return matches[0]
        candidates = self._reference_details(matches)
        raise NotFound(
            f"ambiguous entity name {value!r}",
            details={"candidates": candidates, "suggestions": candidates},
        )

    @staticmethod
    def _reference_details(records: Iterable[Record]) -> list[dict[str, str]]:
        return [
            {"id": record.id, "kind": record.kind, "title": record.title, "reference": record.title}
            for record in records
        ]

    def _reference_suggestions(self, value: str, records: Iterable[Record]) -> list[dict[str, str]]:
        candidates: dict[str, tuple[str, list[Record]]] = {}
        for record in records:
            for name in (record.title, *record.aliases):
                for key, reference in ((name.casefold(), name), (slugify(name).casefold(), slugify(name))):
                    if key not in candidates:
                        candidates[key] = (reference, [record])
                    elif all(candidate.id != record.id for candidate in candidates[key][1]):
                        candidates[key][1].append(record)

        suggestions: list[dict[str, str]] = []
        seen_ids: set[str] = set()
        for candidate in get_close_matches(value.casefold(), candidates, n=6, cutoff=0.6):
            reference, matches = candidates[candidate]
            if len(matches) != 1:
                continue
            record = matches[0]
            if record.id in seen_ids:
                continue
            seen_ids.add(record.id)
            suggestions.append({"id": record.id, "kind": record.kind, "title": record.title, "reference": reference})
            if len(suggestions) == 3:
                break
        return suggestions

    def active_scenes(self) -> list[Record]:
        """Return every active front in a deterministic author-facing order."""
        return [record for record in self.by_kind("scene") if record.status == "active"]

    def active_scene(self) -> Record | None:
        """Return the active scene only when the legacy singleton is unambiguous."""
        active = self.active_scenes()
        return active[0] if len(active) == 1 else None
