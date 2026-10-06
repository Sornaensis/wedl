from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "docs/CONTINUITY_SCHEMA_CONTRACT.md"
VECTORS = ROOT / "architecture/adrai/examples/multi-strand-schema-contract-v04.yaml"


def test_withdrawn_continuity_schema_material_is_nonnormative_only() -> None:
    document = CONTRACT.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "Historical, nonnormative withdrawal" in document
    assert "It defines no active source schema" in document
    assert "`wedl/v0.3` is the only supported schema" in document
    assert "follow-up B" in document
    assert vectors == {
        "status": "withdrawn-nonnormative",
        "superseded_by": "../decisions/R01M/R01M48NHJBX1HWE7RG8G2S3DD9H--shared-world-concurrent-narrative-threads.decision.md",
        "historical_contract": "../../../docs/CONTINUITY_SCHEMA_CONTRACT.md",
        "source_schema": "wedl/v0.3",
        "statement": "This tombstone preserves a withdrawal notice only; it defines no v0.4 schema, vectors, diagnostics, or upgrade behavior.",
    }
