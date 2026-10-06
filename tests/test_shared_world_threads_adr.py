from adrai_fixtures import current_decision, current_status

from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = current_decision("A01M48NHJ5Z6AX617K56WRVFYWT")
VECTORS = ROOT / "architecture/adrai/examples/shared-world-thread-vectors.yaml"
MANIFEST = ROOT / "architecture/adrai/examples/wedl-v04-coordination-manifest.yaml"


def test_shared_world_thread_adr_governs_one_global_world_without_schema_invention() -> None:
    document = ADR.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "**Status:** Accepted" in document
    assert "**Named approver:** Sornaensis" in document
    assert "**Approval date:** 2026-08-26" in document
    assert "**Supersedes:** [ADR 0001]" in document
    assert "one global `StoryTime` ordering" in document
    assert "grouping or coordination label" in document
    assert "same characters, locations, and mutable world state" in document
    assert "one interpretation of ticks and any calendar meaning" in document
    assert "concurrent or overlap in time" in document
    assert "alternate canon, fictional\nforks, continuity/occurrence domains, replay projections, per-thread horizons,\nand per-thread or projection-specific search corpora" in document
    assert "strictly earlier" in document
    assert "one corpus and the ordinary global filters only" in document
    assert "no effect on canon, chronology, replay, causality, or search" in document
    assert "`wedl/v0.3` is the only supported source schema" in document
    assert "deferred to follow-up B" in document
    assert "nonnormative withdrawals" in document
    assert "parser, API, protocol,\ncompiler, query route, or runtime behavior" in document

    historical = current_decision("A01M48NJH26YJTG9XWA0SKCPR2Z")
    assert historical.metadata["title"] == "Multi-strand chronology and fictional continuities"
    assert "# ADR 0001:" in historical.text
    status = current_status(historical.metadata["adr"])
    assert status["state"] == "obsolete"
    assert status["replacement_adr"] == ADR.metadata["adr"]
    assert "This decision supersedes ADR 0001 in full." in document
    assert ADR.metadata["adr"] == "A01M48NHJ5Z6AX617K56WRVFYWT" and ADR.path.is_file()
    assert current_status(ADR.metadata["adr"])["state"] == "active"
    assert 'schema = "adrai/decision/v1"' in document
    assert ADR.metadata["title"] == "Shared-world concurrent narrative threads"
    assert "# ADR 0002:" in document
    assert vectors["adr"] == "A01M48NHJ5Z6AX617K56WRVFYWT"
    assert vectors["source_schema"] == "wedl/v0.3"
    assert vectors["scope"] == "governance-only"
    outcomes = {item["id"]: item["outcome"] for item in vectors["assertions"]}
    assert outcomes == {
        "one-global-story-time": "allowed",
        "thread-is-grouping-only": "allowed",
        "shared-world-resources": "allowed",
        "cross-thread-cause": "allowed",
        "alternate-canon": "forbidden",
        "thread-local-clock": "forbidden",
        "thread-local-corpus": "forbidden",
        "presentation-rewrites-canon": "forbidden",
    }
    assert vectors["withdrawal"]["superseded_adr"] == "A01M48NJH26YJTG9XWA0SKCPR2Z"


def test_coordination_manifest_withdraws_only_thread_reservation_and_preserves_other_projects() -> None:
    manifest = yaml.safe_load(MANIFEST.read_text(encoding="utf-8"))
    by_project = {owner["project"]: owner for owner in manifest["owners"]}

    assert manifest["manifest"] == "wedl-v05-shared-world-thread-reservation"
    assert manifest["status"] == "validation-slice-implemented"
    assert manifest["source_schema"] == "wedl/v0.5"
    assert manifest["release_rule"] == "v0.5 source/model/validation only; compiled database publication remains disabled"
    assert manifest["released_protocol_tokens"] == ["wedl-continuity/v1"]
    assert by_project["88d1d538-452a-4eb5-8b2e-1c42a0361ea8"] == {
        "project": "88d1d538-452a-4eb5-8b2e-1c42a0361ea8",
        "scope": "shared-world-thread-schema-and-migration",
        "contract": "A01M48RX5WAQT5ECH66KTCFVC0T",
        "gate": "reserved-by-this-contract",
        "id_prefixes": ["thread_"],
        "diagnostic_family": None,
        "protocol_tokens": [],
    }
    assert by_project["490749b5-56a3-40aa-97d2-5661b9283690"] == {
        "project": "490749b5-56a3-40aa-97d2-5661b9283690",
        "scope": "calendar-and-historical-chronology",
        "gate": "namespace-reserved",
        "id_prefixes": ["calendar_", "era_", "chronology_"],
        "diagnostic_family": "WDL-CAL-",
        "protocol_tokens": [],
    }
    assert by_project["9117cf1b-a8fb-4df3-8452-635de023de62"] == {
        "project": "9117cf1b-a8fb-4df3-8452-635de023de62",
        "scope": "generational-history",
        "gate": "namespace-reserved",
        "id_prefixes": ["organization_", "kinship_", "union_", "affiliation_", "legacy_"],
        "diagnostic_family": "WDL-GEN-",
        "protocol_tokens": [],
    }
    assert by_project["598e3653-7964-4c13-9f73-47f7fdd3d58a"] == {
        "project": "598e3653-7964-4c13-9f73-47f7fdd3d58a",
        "scope": "large-world-spatial-management",
        "gate": "namespace-reserved",
        "id_prefixes": ["map_", "route_", "overlay_", "geometry_"],
        "diagnostic_family": "WDL-SPAT-",
        "protocol_tokens": [],
    }
