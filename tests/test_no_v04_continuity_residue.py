from __future__ import annotations

from pathlib import Path


_ROOT = Path(__file__).resolve().parents[1]
_RUNTIME = _ROOT / "src" / "wedl"
_FORBIDDEN = (
    "wedl/v0." + "4",
    "WDL-" + "CONT",
    "wedl-" + "continuity/v1",
    "Replay" + "Projection",
    "record_" + "occurrence",
    "continuity_" + "scope",
)


def test_runtime_has_no_withdrawn_v04_continuity_residue() -> None:
    residues = [
        f"{path.relative_to(_ROOT)}: {marker}"
        for path in sorted(_RUNTIME.rglob("*.py"))
        for marker in _FORBIDDEN
        if marker in path.read_text(encoding="utf-8")
    ]

    # v0.4 remains visible only in the normal-path quarantine message and the
    # separately gated raw-envelope migration recovery kernel. Neither module
    # exposes the withdrawn continuity model to ordinary loading or queries.
    assert residues == [
        "src\\wedl\\api_contract.py: wedl/v0.4",
        "src\\wedl\\migration.py: wedl/v0.4",
    ]
