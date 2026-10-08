from adrai_fixtures import current_decision

from pathlib import Path
import re

import yaml


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = current_decision("A01M48RX5WAQT5ECH66KTCFVC0T")
VECTORS = ROOT / "tests/fixtures/architecture/shared-world-thread-schema-v05.yaml"
MANIFEST = ROOT / "tests/fixtures/architecture/wedl-v04-coordination-manifest.yaml"


THREAD_ID = re.compile(r"^thread_[0-9A-HJKMNP-TV-Z]{26}$")


def test_v05_thread_reservation_has_one_global_timeline_and_grouping_only_membership() -> None:
    document = CONTRACT.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))

    assert "v0.5 source validation and the local-only migration/recovery kernel are available" in document
    assert "Query, search, API, and browser delivery remain separately" in document
    assert "exactly one `timelines` item" in document
    assert "`default_timeline` must equal that item" in document
    assert "one global cursor" in document
    assert "Characters, locations, mutable world state" in document
    assert "strictly\nearlier than its effect" in document
    assert "id` and `label`; it has no\nextra fields" in document
    assert "no thread,\none thread, or many threads" in document
    assert "Hypotheses reject thread membership" in document
    assert "`parent`, `status`, `canon`, `time`, `horizon`, `presentation`, `search`," in document
    assert "`timeline`, `domain`,\n`fork`, `continuity`, `strand`, `sync`, `retcon`, `projection`, and\n`author-all`" in document
    assert "may infer a thread" in document

    assert vectors["contract"] == "A01M48RX5WAQT5ECH66KTCFVC0T"
    assert vectors["status"] == "validation-slice-implemented"
    assert vectors["source_schema"] == "wedl/v0.5"
    assert vectors["supported_schemas"] == ["wedl/v0.3", "wedl/v0.5"]
    assert vectors["global_model"] == {
        "timelines": "exactly-one",
        "default_timeline": "matches-only-timeline",
        "current_time": "optional-global",
        "semantics": "shared characters, locations, mutable state, StoryTime, tick/calendar interpretation, causes, and cursor",
    }
    assert vectors["thread_membership"]["hypothesis"] == "rejected"
    assert vectors["thread_membership"]["semantics"] == "zero-to-many grouping only; no canon, time, state, causality, cursor, or corpus effect"
    assert vectors["prohibited"]["thread_fields"] == ["parent", "status", "canon", "time", "horizon", "presentation", "search", "model", "cursor"]
    assert vectors["prohibited"]["thread_scoped_fields"] == ["timeline", "domain", "fork", "continuity", "strand", "sync", "retcon", "projection", "author-all"]
    assert vectors["prohibited"]["inference"] == "prohibited"

    cases = {item["id"]: item for item in vectors["vectors"]}
    assert list(cases) == ["zero-membership", "one-membership", "many-membership", "duplicate-membership", "global-cause-state-cursor"]
    assert cases["zero-membership"]["memberships"] == []
    assert all(THREAD_ID.fullmatch(value) for value in cases["one-membership"]["memberships"])
    assert len(cases["many-membership"]["memberships"]) == 2
    assert len(set(cases["many-membership"]["memberships"])) == 2
    assert cases["duplicate-membership"]["expected"] == "rejected-duplicate"
    assert cases["global-cause-state-cursor"]["expected"] == "strict-global-story-time-only"
    assert vectors["validation"]["v04"] == "v04_superseded; see ADRAI in the WEDL development/source checkout: adrai --repo WEDL_SOURCE_CHECKOUT show A01M48RX5WAQT5ECH66KTCFVC0T --json (section 4: Quarantined v0.4 recovery)"
    assert vectors["validation"]["schema"]["mixed"]["code"] == "WDL-SRC-008"
    assert vectors["validation"]["timeline"]["code"] == "WDL-TIMELINE-012"
    assert list(vectors["validation"]["threads"]) == [f"WDL-THREAD-00{index}" for index in range(1, 10)]
    assert "## 6. Validation diagnostics" in document
    assert "thread declaration diagnostics precede record\nmembership diagnostics" in document


def test_v05_migration_and_quarantined_v04_recovery_are_narrow_and_noninferential() -> None:
    document = CONTRACT.read_text(encoding="utf-8")
    vectors = yaml.safe_load(VECTORS.read_text(encoding="utf-8"))
    manifest = yaml.safe_load(MANIFEST.read_text(encoding="utf-8"))

    for requirement in (
        "exact expected Git\nHEAD", "clean working tree", "homogeneous v0.3", "source hash",
        "exactly one timeline", "default_timeline` equals it", "atomic, confirmed write",
        "creates a backup", "It does not infer or create\nthreads", "multi-timeline source blocks",
        "idempotent", "mixed source schemas are rejected", "Disposable caches rebuild",
        "`wedl/v0.4`, `wedl-continuity/v1`, and `WDL-CONT` continuity IDs are withdrawn",
        "never be silently treated as threads", "stable `v04_superseded` diagnostic",
        "not a normal v0.4 topology read", "exactly one root\ncontinuity/domain", "status `primary`", "`default_continuity` names it",
        "alternate, experimental, retired, or mismatched-default sole continuity\nblocks automatic recovery", "empty strands",
        "scalar horizon may map to global `current_time`", "author-supplied mapping",
        "exact expected HEAD,\nclean tree, dry-run hash, and backup", "every source\nrecord's schema marker to homogeneous `wedl/v0.5`", "backup supplies recoverable rollback", "partial or mixed sources\nare never left behind", "No v0.4 recovery may infer threads",
    ):
        assert requirement in document

    assert vectors["migration"] == {
        "v03_automatic": "exact-head-clean-dry-hash-confirm-backup; one timeline and matching default only; world threads []; record memberships omitted; schemas atomic",
        "v03_multitimeline": "blocked",
        "v03_mixed": "rejected",
        "v03_rerun": "idempotent",
        "inference": "prohibited",
    }
    assert vectors["recovery"] == {
        "v04_normal_path": "v04_superseded; stop before topology or reinterpretation",
        "automatic_only": "exactly one root continuity/domain with primary status and matching default_continuity; no parent/fork/retcon/sync/handoff/vector-or-synchronized-horizon/frame; global empty-strand membership; coordinates map global timeline",
        "scalar_horizon": "may map global current_time",
        "alternate_sole_continuity": "blocked",
        "experimental_sole_continuity": "blocked",
        "retired_sole_continuity": "blocked",
        "mismatched_default_continuity": "blocked",
        "conversion": "atomic all-schema-markers to homogeneous wedl/v0.5; drop wrappers; world threads []; record memberships omitted; backup rollback; no partial sources",
        "otherwise": "author-supplied-mapping-required",
        "safeguards": "exact-head-clean-dry-hash-confirm-backup",
    }
    assert manifest["released_protocol_tokens"] == ["wedl-continuity/v1"]
    assert manifest["status"] == "validation-slice-implemented"
    assert manifest["owners"][0]["id_prefixes"] == ["thread_"]
    assert manifest["owners"][0]["protocol_tokens"] == []
