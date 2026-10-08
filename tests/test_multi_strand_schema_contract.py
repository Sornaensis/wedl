from adrai_fixtures import current_decision

from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = current_decision("A01M48NJH26YJTG9XWA0SKCPR2Z")
VECTORS = ROOT / "tests/fixtures/architecture/multi-strand-schema-contract-v04.yaml"


def test_withdrawn_continuity_schema_material_is_nonnormative_only() -> None:
    document = CONTRACT.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "Historical, nonnormative withdrawal" in document
    assert "It defines no active source schema" in document
    assert "`wedl/v0.3` is the only supported schema" in document
    assert "follow-up B" in document
    assert vectors == {
        "status": "withdrawn-nonnormative",
        "superseded_by": "A01M48NHJ5Z6AX617K56WRVFYWT",
        "historical_contract": "A01M48NJH26YJTG9XWA0SKCPR2Z",
        "source_schema": "wedl/v0.3",
        "statement": "This tombstone preserves a withdrawal notice only; it defines no v0.4 schema, vectors, diagnostics, or upgrade behavior.",
    }
