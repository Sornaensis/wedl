from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from functools import lru_cache
import hashlib
import io
import json
import math
import os
from typing import Any, Iterable, Sequence
from urllib import request as urllib_request

import numpy as np
from scipy import sparse
from sklearn.decomposition import TruncatedSVD

from .errors import UsageError
from .profiles import CompilationProfile
from .lexical import STOPWORDS
from .util import TOKEN_RE, canonical_json


@dataclass(frozen=True, slots=True)
class VectorModel:
    model_id: str
    provider: str
    model_name: str
    dimensions: int
    normalized: bool
    corpus_hash: str | None
    config: dict[str, Any]
    model_blob: bytes | None = None


@dataclass(frozen=True, slots=True)
class VectorBuild:
    model: VectorModel
    vectors: dict[str, np.ndarray]
    cacheable_across_corpora: bool


def normalize_vector(values: Sequence[float] | np.ndarray) -> np.ndarray:
    vector = np.asarray(values, dtype=np.float32).reshape(-1)
    if vector.size == 0:
        raise UsageError("Embedding provider returned an empty vector")
    if not np.all(np.isfinite(vector)):
        raise UsageError("Embedding provider returned non-finite vector values")
    norm = float(np.linalg.norm(vector))
    if norm <= 1e-12:
        return np.zeros(vector.shape, dtype=np.float32)
    return np.asarray(vector / norm, dtype=np.float32)


def vectors_to_matrix(values: Iterable[Sequence[float] | np.ndarray]) -> np.ndarray:
    rows = [normalize_vector(value) for value in values]
    if not rows:
        return np.empty((0, 0), dtype=np.float32)
    dimensions = rows[0].size
    if any(row.size != dimensions for row in rows):
        raise UsageError("Embedding provider returned inconsistent vector dimensions")
    return np.vstack(rows).astype(np.float32, copy=False)


def vector_to_blob(values: Sequence[float] | np.ndarray) -> bytes:
    return normalize_vector(values).astype("<f4", copy=False).tobytes()


def blob_to_vector(blob: bytes, dimensions: int | None = None) -> np.ndarray:
    vector = np.frombuffer(blob, dtype="<f4")
    if dimensions is not None and vector.size != dimensions:
        raise UsageError(
            f"Stored vector has {vector.size} dimensions but model declares {dimensions}"
        )
    return np.asarray(vector, dtype=np.float32)


def _features(text: str) -> Counter[str]:
    tokens = [
        match.group(0).casefold()
        for match in TOKEN_RE.finditer(text)
        if match.group(0).casefold() not in STOPWORDS
    ]
    counts: Counter[str] = Counter(tokens)
    counts.update(f"{left}::{right}" for left, right in zip(tokens, tokens[1:]))
    return counts


def _corpus_hash(input_hashes: Sequence[str], profile: CompilationProfile) -> str:
    material = "\n".join(sorted(input_hashes)) + "\n" + profile.fingerprint_material()
    return hashlib.sha256(material.encode("utf-8")).hexdigest()


def _lsa_model_blob(terms: Sequence[str], idf: np.ndarray, components: np.ndarray) -> bytes:
    payload = io.BytesIO()
    np.savez_compressed(
        payload,
        terms=np.asarray(terms, dtype=np.str_),
        idf=np.asarray(idf, dtype=np.float32),
        components=np.asarray(components, dtype=np.float32),
    )
    return payload.getvalue()


@lru_cache(maxsize=8)
def _load_lsa_blob(blob: bytes) -> tuple[list[str], np.ndarray, np.ndarray]:
    with np.load(io.BytesIO(blob), allow_pickle=False) as payload:
        terms = [str(value) for value in payload["terms"].tolist()]
        idf = np.asarray(payload["idf"], dtype=np.float32)
        components = np.asarray(payload["components"], dtype=np.float32)
    return terms, idf, components


def _tfidf_matrix(
    texts: Sequence[str],
    *,
    max_features: int,
) -> tuple[sparse.csr_matrix, list[str], np.ndarray]:
    document_counts = [_features(text) for text in texts]
    document_frequency: Counter[str] = Counter()
    total_frequency: Counter[str] = Counter()
    for counts in document_counts:
        document_frequency.update(counts.keys())
        total_frequency.update(counts)
    ranked = sorted(
        document_frequency,
        key=lambda term: (-document_frequency[term], -total_frequency[term], term),
    )[:max_features]
    if not ranked:
        ranked = ["__empty__"]
    vocabulary = {term: index for index, term in enumerate(ranked)}
    n_documents = max(1, len(texts))
    idf = np.asarray(
        [math.log((1.0 + n_documents) / (1.0 + document_frequency.get(term, 0))) + 1.0 for term in ranked],
        dtype=np.float32,
    )
    rows: list[int] = []
    columns: list[int] = []
    data: list[float] = []
    for row_index, counts in enumerate(document_counts):
        row_values: list[tuple[int, float]] = []
        for term, count in counts.items():
            column = vocabulary.get(term)
            if column is None:
                continue
            value = (1.0 + math.log(float(count))) * float(idf[column])
            row_values.append((column, value))
        norm = math.sqrt(sum(value * value for _column, value in row_values))
        if norm <= 1e-12:
            continue
        for column, value in row_values:
            rows.append(row_index)
            columns.append(column)
            data.append(value / norm)
    matrix = sparse.csr_matrix(
        (np.asarray(data, dtype=np.float32), (rows, columns)),
        shape=(len(texts), len(ranked)),
        dtype=np.float32,
    )
    return matrix, ranked, idf


def _tfidf_transform(
    texts: Sequence[str],
    terms: Sequence[str],
    idf: np.ndarray,
) -> sparse.csr_matrix:
    vocabulary = {term: index for index, term in enumerate(terms)}
    rows: list[int] = []
    columns: list[int] = []
    data: list[float] = []
    for row_index, text in enumerate(texts):
        values: list[tuple[int, float]] = []
        for term, count in _features(text).items():
            column = vocabulary.get(term)
            if column is None:
                continue
            values.append((column, (1.0 + math.log(float(count))) * float(idf[column])))
        norm = math.sqrt(sum(value * value for _column, value in values))
        if norm <= 1e-12:
            continue
        for column, value in values:
            rows.append(row_index)
            columns.append(column)
            data.append(value / norm)
    return sparse.csr_matrix(
        (np.asarray(data, dtype=np.float32), (rows, columns)),
        shape=(len(texts), len(terms)),
        dtype=np.float32,
    )


def fit_lsa(
    texts_by_hash: dict[str, str],
    profile: CompilationProfile,
    *,
    training_hashes: Sequence[str] | None = None,
    scope: str = "global",
) -> VectorBuild:
    hashes = sorted(texts_by_hash)
    selected_training = sorted(set(training_hashes or hashes))
    training_texts = [texts_by_hash[input_hash] for input_hash in selected_training]
    matrix, terms, idf = _tfidf_matrix(training_texts, max_features=profile.vector_max_features)
    max_components = max(1, min(matrix.shape[0] - 1, matrix.shape[1] - 1))
    dimensions = min(profile.vector_dimensions, max_components)
    if matrix.shape[0] == 1 or matrix.shape[1] == 1:
        # Tiny worlds still receive a real normalized TF-IDF vector model.
        components = np.eye(matrix.shape[1], dtype=np.float32)[:dimensions]
        dense = np.asarray(matrix @ components.T, dtype=np.float32)
    else:
        svd = TruncatedSVD(
            n_components=dimensions,
            algorithm="randomized",
            n_iter=7,
            random_state=0,
        )
        dense = np.asarray(svd.fit_transform(matrix), dtype=np.float32)
        components = np.asarray(svd.components_, dtype=np.float32)
    target_matrix = _tfidf_transform(
        [texts_by_hash[input_hash] for input_hash in hashes], terms, idf
    )
    dense = np.asarray(target_matrix @ components.T, dtype=np.float32)
    normalized = vectors_to_matrix(dense)
    corpus_hash = _corpus_hash(selected_training, profile)
    model_blob = _lsa_model_blob(terms, idf, components)
    model_id = f"lsa:{scope}:{profile.vector_model}:{dimensions}:{corpus_hash[:16]}"
    model = VectorModel(
        model_id=model_id,
        provider="lsa",
        model_name=str(profile.vector_model),
        dimensions=dimensions,
        normalized=True,
        corpus_hash=corpus_hash,
        config={
            "maxFeatures": profile.vector_max_features,
            "requestedDimensions": profile.vector_dimensions,
            "featureCount": len(terms),
            "trainingDocumentCount": len(training_texts),
            "embeddedDocumentCount": len(hashes),
            "scope": scope,
            "features": "word-unigram-bigram-tfidf",
            "projection": "truncated-svd",
        },
        model_blob=model_blob,
    )
    return VectorBuild(
        model=model,
        vectors={input_hash: normalized[index] for index, input_hash in enumerate(hashes)},
        cacheable_across_corpora=False,
    )


def embed_lsa_documents(
    model: VectorModel,
    texts_by_hash: dict[str, str],
) -> dict[str, np.ndarray]:
    if not model.model_blob:
        raise UsageError("Compiled LSA model is missing its projection data")
    hashes = sorted(texts_by_hash)
    terms, idf, components = _load_lsa_blob(model.model_blob)
    matrix = _tfidf_transform(
        [texts_by_hash[input_hash] for input_hash in hashes], terms, idf
    )
    dense = np.asarray(matrix @ components.T, dtype=np.float32)
    normalized = vectors_to_matrix(dense)
    return {input_hash: normalized[index] for index, input_hash in enumerate(hashes)}


def embed_lsa_query(model: VectorModel, query: str) -> np.ndarray:
    return embed_lsa_documents(model, {"query": query})["query"]


@lru_cache(maxsize=4)
def _sentence_transformer_model(model_name: str):
    try:
        from sentence_transformers import SentenceTransformer  # type: ignore
    except ImportError as exc:  # pragma: no cover - optional provider
        raise UsageError(
            "sentence-transformers provider requires `pip install wedl[sentence-transformers]`"
        ) from exc
    return SentenceTransformer(model_name)


def embed_sentence_transformers(
    texts_by_hash: dict[str, str],
    profile: CompilationProfile,
) -> VectorBuild:
    hashes = sorted(texts_by_hash)
    model_object = _sentence_transformer_model(str(profile.vector_model))
    matrix = model_object.encode(
        [texts_by_hash[input_hash] for input_hash in hashes],
        batch_size=64,
        show_progress_bar=False,
        convert_to_numpy=True,
        normalize_embeddings=True,
    )
    normalized = vectors_to_matrix(matrix)
    dimensions = int(normalized.shape[1]) if normalized.size else profile.vector_dimensions
    model_id = f"sentence-transformers:{profile.vector_model}:{dimensions}"
    model = VectorModel(
        model_id=model_id,
        provider="sentence-transformers",
        model_name=str(profile.vector_model),
        dimensions=dimensions,
        normalized=True,
        corpus_hash=None,
        config={"batchSize": 64},
    )
    return VectorBuild(
        model=model,
        vectors={input_hash: normalized[index] for index, input_hash in enumerate(hashes)},
        cacheable_across_corpora=True,
    )


def embed_sentence_transformer_query(model: VectorModel, query: str) -> np.ndarray:
    model_object = _sentence_transformer_model(model.model_name)
    value = model_object.encode(
        [query],
        show_progress_bar=False,
        convert_to_numpy=True,
        normalize_embeddings=True,
    )[0]
    return normalize_vector(value)


def _openai_endpoint(config: dict[str, Any]) -> str:
    return str(
        config.get("endpoint")
        or os.environ.get("WEDL_EMBEDDING_ENDPOINT")
        or "http://127.0.0.1:8000/v1/embeddings"
    )


def _openai_api_key(config: dict[str, Any]) -> str | None:
    environment_name = str(config.get("api_key_env") or "WEDL_EMBEDDING_API_KEY")
    return os.environ.get(environment_name)


def _openai_embeddings(texts: Sequence[str], model_name: str, config: dict[str, Any]) -> np.ndarray:
    endpoint = _openai_endpoint(config)
    headers = {"Content-Type": "application/json"}
    if api_key := _openai_api_key(config):
        headers["Authorization"] = f"Bearer {api_key}"
    body = json.dumps({"model": model_name, "input": list(texts)}).encode("utf-8")
    request = urllib_request.Request(endpoint, data=body, headers=headers)
    try:
        with urllib_request.urlopen(request, timeout=float(config.get("timeout_seconds", 120))) as response:  # noqa: S310 - explicit configured endpoint
            payload = json.load(response)
    except Exception as exc:  # pragma: no cover - external integration
        raise UsageError(f"OpenAI-compatible embedding request failed: {exc}") from exc
    rows = sorted(payload.get("data") or [], key=lambda item: int(item.get("index", 0)))
    if len(rows) != len(texts):
        raise UsageError("OpenAI-compatible embedding endpoint returned the wrong number of vectors")
    return vectors_to_matrix([row.get("embedding") or [] for row in rows])


def embed_openai_compatible(
    texts_by_hash: dict[str, str],
    profile: CompilationProfile,
    config: dict[str, Any],
) -> VectorBuild:
    hashes = sorted(texts_by_hash)
    batch_size = int(config.get("batch_size", 64))
    matrices: list[np.ndarray] = []
    for start in range(0, len(hashes), batch_size):
        batch_hashes = hashes[start : start + batch_size]
        matrices.append(
            _openai_embeddings(
                [texts_by_hash[input_hash] for input_hash in batch_hashes],
                str(profile.vector_model),
                config,
            )
        )
    matrix = np.vstack(matrices) if matrices else np.empty((0, profile.vector_dimensions), dtype=np.float32)
    dimensions = int(matrix.shape[1]) if matrix.size else profile.vector_dimensions
    model = VectorModel(
        model_id=f"openai-compatible:{profile.vector_model}:{dimensions}:{hashlib.sha256(_openai_endpoint(config).encode()).hexdigest()[:8]}",
        provider="openai-compatible",
        model_name=str(profile.vector_model),
        dimensions=dimensions,
        normalized=True,
        corpus_hash=None,
        config={
            "endpoint": _openai_endpoint(config),
            "apiKeyEnv": str(config.get("api_key_env") or "WEDL_EMBEDDING_API_KEY"),
            "batchSize": batch_size,
        },
    )
    return VectorBuild(
        model=model,
        vectors={input_hash: matrix[index] for index, input_hash in enumerate(hashes)},
        cacheable_across_corpora=True,
    )


def embed_openai_query(model: VectorModel, query: str) -> np.ndarray:
    return _openai_embeddings([query], model.model_name, model.config)[0]


def hash_test_embedding(text: str, dimensions: int) -> np.ndarray:
    vector = np.zeros(dimensions, dtype=np.float32)
    counts = _features(text)
    for token, count in counts.items():
        digest = hashlib.blake2b(token.encode("utf-8"), digest_size=16).digest()
        index = int.from_bytes(digest[:8], "big") % dimensions
        sign = 1.0 if digest[8] & 1 else -1.0
        vector[index] += sign * (1.0 + math.log(float(count)))
    return normalize_vector(vector)


def embed_hash_test(texts_by_hash: dict[str, str], profile: CompilationProfile) -> VectorBuild:
    model = VectorModel(
        model_id=f"hash-test:{profile.vector_model}:{profile.vector_dimensions}",
        provider="hash-test",
        model_name=str(profile.vector_model),
        dimensions=profile.vector_dimensions,
        normalized=True,
        corpus_hash=None,
        config={"warning": "lexical feature hash; test/diagnostic provider only"},
    )
    return VectorBuild(
        model=model,
        vectors={
            input_hash: hash_test_embedding(text, profile.vector_dimensions)
            for input_hash, text in texts_by_hash.items()
        },
        cacheable_across_corpora=True,
    )


def build_vectors(
    texts_by_hash: dict[str, str],
    profile: CompilationProfile,
    provider_config: dict[str, Any],
    *,
    training_hashes: Sequence[str] | None = None,
    scope: str = "global",
) -> VectorBuild:
    if profile.vector_provider == "lsa":
        return fit_lsa(
            texts_by_hash,
            profile,
            training_hashes=training_hashes,
            scope=scope,
        )
    if profile.vector_provider == "sentence-transformers":
        return embed_sentence_transformers(texts_by_hash, profile)
    if profile.vector_provider == "openai-compatible":
        return embed_openai_compatible(texts_by_hash, profile, provider_config)
    if profile.vector_provider == "hash-test":
        return embed_hash_test(texts_by_hash, profile)
    raise UsageError(f"Unsupported vector provider {profile.vector_provider!r}")


def embed_documents(
    model: VectorModel,
    texts_by_hash: dict[str, str],
) -> dict[str, np.ndarray]:
    if not texts_by_hash:
        return {}
    if model.provider == "lsa":
        return embed_lsa_documents(model, texts_by_hash)
    if model.provider == "sentence-transformers":
        profile = type("Profile", (), {"vector_model": model.model_name})()
        hashes = sorted(texts_by_hash)
        model_object = _sentence_transformer_model(model.model_name)
        matrix = vectors_to_matrix(
            model_object.encode(
                [texts_by_hash[input_hash] for input_hash in hashes],
                batch_size=64,
                show_progress_bar=False,
                convert_to_numpy=True,
                normalize_embeddings=True,
            )
        )
        return {input_hash: matrix[index] for index, input_hash in enumerate(hashes)}
    if model.provider == "openai-compatible":
        hashes = sorted(texts_by_hash)
        matrix = _openai_embeddings(
            [texts_by_hash[input_hash] for input_hash in hashes],
            model.model_name,
            model.config,
        )
        return {input_hash: matrix[index] for index, input_hash in enumerate(hashes)}
    if model.provider == "hash-test":
        return {
            input_hash: hash_test_embedding(text, model.dimensions)
            for input_hash, text in texts_by_hash.items()
        }
    raise UsageError(f"Unsupported compiled vector provider {model.provider!r}")


def embed_query(model: VectorModel, query: str) -> np.ndarray:
    if model.provider == "lsa":
        return embed_lsa_query(model, query)
    if model.provider == "sentence-transformers":
        return embed_sentence_transformer_query(model, query)
    if model.provider == "openai-compatible":
        return embed_openai_query(model, query)
    if model.provider == "hash-test":
        return hash_test_embedding(query, model.dimensions)
    raise UsageError(f"Unsupported compiled vector provider {model.provider!r}")


def model_from_row(row: Any) -> VectorModel:
    return VectorModel(
        model_id=str(row["model_id"]),
        provider=str(row["provider"]),
        model_name=str(row["model_name"]),
        dimensions=int(row["dimensions"]),
        normalized=bool(row["normalized"]),
        corpus_hash=str(row["corpus_hash"]) if row["corpus_hash"] else None,
        config=json.loads(row["config_json"] or "{}"),
        model_blob=bytes(row["model_blob"]) if row["model_blob"] is not None else None,
    )
