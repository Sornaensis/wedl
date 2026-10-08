from adrai_fixtures import current_decision, current_status

from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = current_decision("A01M48NJH26YJTG9XWA0SKCPR2Z")
VECTORS = ROOT / "tests/fixtures/architecture/multi-strand-continuity-vectors.yaml"


def test_multi_strand_adr_is_historical_and_explicitly_superseded() -> None:
    document = ADR.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "**Status:** Superseded" in document
    assert "**Superseded by:** [ADR 0002" in document
    assert "Historical decision — superseded" in document
    assert vectors == {
        "status": "withdrawn-nonnormative",
        "superseded_by": "A01M48NHJ5Z6AX617K56WRVFYWT",
        "statement": "Historical ADR 0001 vectors are withdrawn. This tombstone defines no continuity, fork, domain, horizon, projection, canon, or schema behavior.",
    }
