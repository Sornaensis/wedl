from __future__ import annotations

import json

import numpy as np
import pytest

from wedl.compiler import compile_world, connect
from wedl.errors import UsageError
from wedl.profiles import CompilationProfile
from wedl.query import search_world
from wedl.vectors import embed_lsa_query, fit_lsa


def test_lsa_dense_vectors_recover_nonlexical_neighbour() -> None:
    profile = CompilationProfile(
        name="vector",
        fts_enabled=False,
        vector_enabled=True,
        vector_provider="lsa",
        vector_model="test-lsa",
        vector_dimensions=2,
        vector_max_features=100,
        fts_candidate_limit=20,
        vector_candidate_limit=20,
        hybrid_fts_weight=1.0,
        hybrid_vector_weight=1.0,
        hybrid_rrf_k=60.0,
    )
    build = fit_lsa(
        {
            "target": "pressure gate chamber shutters",
            "bridge": "hydraulic river pressure gate",
            "unrelated": "archive catalog ledger",
        },
        profile,
        scope="test",
    )
    query = embed_lsa_query(build.model, "hydraulic river")
    assert np.isclose(np.linalg.norm(query), 1.0, atol=1e-5)
    assert np.isclose(np.linalg.norm(build.vectors["target"]), 1.0, atol=1e-5)
    assert float(query @ build.vectors["target"]) > float(query @ build.vectors["unrelated"])


def test_compilation_profiles_enable_only_declared_lanes(ash_repo) -> None:
    state = compile_world(ash_repo, force=True, profile_name="state")
    assert state["searchDocumentCount"] == 0
    with pytest.raises(UsageError):
        search_world(ash_repo, "counterseal", mode="fts")

    fts = compile_world(ash_repo, force=True, profile_name="fts")
    assert fts["ftsDocumentCount"] == fts["searchDocumentCount"] > 0
    assert fts["vectorDocumentCount"] == 0
    assert search_world(ash_repo, "counterseal", mode="fts", all_time=True)["results"]
    with pytest.raises(UsageError):
        search_world(ash_repo, "counterseal", mode="vector")

    vector = compile_world(ash_repo, force=True, profile_name="vector")
    assert vector["ftsDocumentCount"] == 0
    assert vector["vectorDocumentCount"] == vector["searchDocumentCount"] > 0
    assert search_world(ash_repo, "legal authenticity coercion", mode="vector", all_time=True)["results"]
    with pytest.raises(UsageError):
        search_world(ash_repo, "counterseal", mode="fts")

    hybrid = compile_world(ash_repo, force=True, profile_name="hybrid")
    assert hybrid["ftsDocumentCount"] == hybrid["searchDocumentCount"]
    assert hybrid["vectorDocumentCount"] == hybrid["searchDocumentCount"]


def test_vector_storage_is_normalized_and_content_deduplicated(ash_repo) -> None:
    report = compile_world(ash_repo, force=True, profile_name="hybrid")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    with connect(database, True) as connection:
        nonzero = connection.execute(
            "SELECT COUNT(*) FROM vector_embedding WHERE norm > 0"
        ).fetchone()[0]
        bad = connection.execute(
            "SELECT COUNT(*) FROM vector_embedding WHERE norm > 0 AND abs(norm - 1.0) > 0.001"
        ).fetchone()[0]
        unique_vectors = connection.execute("SELECT COUNT(*) FROM vector_embedding").fetchone()[0]
        links = connection.execute("SELECT COUNT(*) FROM document_vector").fetchone()[0]
        models = [dict(row) for row in connection.execute("SELECT * FROM vector_model ORDER BY scope")]
    assert nonzero > 0
    assert bad == 0
    assert unique_vectors == report["uniqueVectorCount"]
    assert links == report["vectorLinkCount"]
    assert unique_vectors < links
    assert {model["scope"] for model in models} == {"author", "character"}
    assert all(model["normalized"] for model in models)


def test_hybrid_runs_full_independent_lanes_and_fuses_union(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="hybrid")
    query = "legal authenticity coercion"
    fts = search_world(ash_repo, query, mode="fts", limit=20, all_time=True)["results"]
    vector = search_world(ash_repo, query, mode="vector", limit=20, all_time=True)["results"]
    hybrid = search_world(ash_repo, query, mode="hybrid", limit=20, all_time=True)["results"]

    fts_ids = {item["documentId"] for item in fts}
    vector_only = [item for item in vector if item["documentId"] not in fts_ids]
    assert vector_only, "vector lane should recover documents outside lexical results"
    hybrid_by_id = {item["documentId"]: item for item in hybrid}
    assert any(item["documentId"] in hybrid_by_id for item in vector_only)
    assert any(set(item["lanes"]) == {"fts", "vector"} for item in hybrid)
    assert all("ftsRank" in item for item in fts)
    assert all("vectorRank" in item for item in vector)


def test_search_response_reports_profile_and_models(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="hybrid")
    response = search_world(ash_repo, "flood register", mode="hybrid", limit=5, all_time=True)
    state = response["searchState"]
    assert state["profile"]["name"] == "hybrid"
    assert state["fts"]["available"] is True
    assert state["vector"]["available"] is True
    assert {model["scope"] for model in state["vector"]["models"]} == {"author", "character"}


def test_fts_indexes_aliases_tags_and_structured_fields(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="fts")
    alias_results = search_world(ash_repo, '"Chief Sorn"', mode="fts", limit=10, all_time=True)["results"]
    assert any(item["metadata"]["title"] == "Ilyra Sorn" for item in alias_results)

    tag_results = search_world(ash_repo, '"hydraulic key"', mode="fts", limit=10, all_time=True)["results"]
    assert any(item["metadata"]["title"] == "Pressure Gate Seven Key" for item in tag_results)


def test_vector_input_uses_title_alias_heading_domain_tags_and_text(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="vector")
    results = search_world(ash_repo, "Chief Sorn", mode="vector", limit=20, all_time=True)["results"]
    assert any(item["metadata"]["title"] == "Ilyra Sorn" for item in results)


def test_openai_compatible_provider_normalizes_dense_vectors(monkeypatch) -> None:
    import io
    import json
    from wedl.vectors import embed_openai_compatible, embed_openai_query

    payloads = [
        {"data": [{"index": 0, "embedding": [3.0, 4.0]}, {"index": 1, "embedding": [0.0, 5.0]}]},
        {"data": [{"index": 0, "embedding": [6.0, 8.0]}]},
    ]

    def fake_urlopen(_request, timeout=120):
        assert timeout == 120
        return io.BytesIO(json.dumps(payloads.pop(0)).encode("utf-8"))

    monkeypatch.setattr("wedl.vectors.urllib_request.urlopen", fake_urlopen)
    profile = CompilationProfile(
        name="vector",
        fts_enabled=False,
        vector_enabled=True,
        vector_provider="openai-compatible",
        vector_model="fake-dense-model",
        vector_dimensions=2,
        vector_max_features=100,
        fts_candidate_limit=20,
        vector_candidate_limit=20,
        hybrid_fts_weight=1.0,
        hybrid_vector_weight=1.0,
        hybrid_rrf_k=60.0,
    )
    build = embed_openai_compatible(
        {"a": "first", "b": "second"},
        profile,
        {"endpoint": "http://127.0.0.1:9999/v1/embeddings"},
    )
    assert all(np.isclose(np.linalg.norm(value), 1.0) for value in build.vectors.values())
    query = embed_openai_query(build.model, "query")
    assert np.isclose(np.linalg.norm(query), 1.0)
