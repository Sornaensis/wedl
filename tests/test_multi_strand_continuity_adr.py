from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "docs/decisions/0001-multi-strand-chronology-and-fictional-continuities.md"
VECTORS = ROOT / "docs/decisions/examples/multi-strand-continuity-vectors.yaml"


def test_multi_strand_adr_is_historical_and_explicitly_superseded() -> None:
    document = ADR.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "**Status:** Superseded" in document
    assert "**Superseded by:** [ADR 0002" in document
    assert "Historical decision — superseded" in document
    assert vectors == {
        "status": "withdrawn-nonnormative",
        "superseded_by": "../0002-shared-world-concurrent-narrative-threads.md",
        "statement": "Historical ADR 0001 vectors are withdrawn. This tombstone defines no continuity, fork, domain, horizon, projection, canon, or schema behavior.",
    }
