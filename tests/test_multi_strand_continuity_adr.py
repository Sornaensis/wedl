from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "architecture/adrai/decisions/R01M/R01M48NJHZTGCYRTBBJQ9D7XG9M--multi-strand-chronology-and-fictional-continuities.decision.md"
VECTORS = ROOT / "architecture/adrai/examples/multi-strand-continuity-vectors.yaml"


def test_multi_strand_adr_is_historical_and_explicitly_superseded() -> None:
    document = ADR.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "**Status:** Superseded" in document
    assert "**Superseded by:** [ADR 0002" in document
    assert "Historical decision — superseded" in document
    assert vectors == {
        "status": "withdrawn-nonnormative",
        "superseded_by": "../decisions/R01M/R01M48NHJBX1HWE7RG8G2S3DD9H--shared-world-concurrent-narrative-threads.decision.md",
        "statement": "Historical ADR 0001 vectors are withdrawn. This tombstone defines no continuity, fork, domain, horizon, projection, canon, or schema behavior.",
    }
