"""Static conformance access to CLI-managed ADR fixtures by stable identity."""

from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
import tomllib


ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class Decision:
    path: Path
    metadata: dict
    text: str

    @property
    def name(self) -> str:
        return self.path.name

    def read_text(self, *, encoding: str = 'utf-8') -> str:
        assert encoding == 'utf-8'
        return self.text


@lru_cache(maxsize=1)
def _managed_objects() -> tuple[tuple[Path, dict, str], ...]:
    objects = []
    for directory in ('decisions', 'connections'):
        for path in sorted((ROOT / 'architecture/adrai' / directory).rglob('*.md')):
            text = path.read_text(encoding='utf-8')
            sections = text.split('+++', 2)
            assert len(sections) == 3 and not sections[0].strip(), path
            metadata = tomllib.loads(sections[1])
            assert metadata['schema'] == f'adrai/{"decision" if directory == "decisions" else "connection"}/v1', path
            objects.append((path, metadata, text))
    return tuple(objects)


def current_decision(adr: str) -> Decision:
    superseded = {
        record
        for _, metadata, _ in _managed_objects()
        if metadata.get('relation') == 'amends' and metadata.get('subject_adr') == adr
        for record in metadata['to_records']
    }
    candidates = [
        Decision(path, metadata, text)
        for path, metadata, text in _managed_objects()
        if metadata.get('adr') == adr and metadata['record'] not in superseded
    ]
    assert len(candidates) == 1, (adr, [value.path for value in candidates])
    return candidates[0]


def current_status(adr: str) -> dict:
    statuses = [metadata for _, metadata, _ in _managed_objects()
                if metadata.get('relation') == 'status' and metadata.get('subject_adr') == adr]
    superseded = {parent for metadata in statuses for parent in metadata['parent_connections']}
    heads = [metadata for metadata in statuses if metadata['connection'] not in superseded]
    assert len(heads) == 1, (adr, heads)
    return heads[0]
