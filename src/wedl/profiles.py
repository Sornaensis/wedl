from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .errors import UsageError
from .model import World
from .util import canonical_json

PROFILE_NAMES = ("state", "fts", "vector", "hybrid")
VECTOR_PROVIDERS = ("lsa", "sentence-transformers", "openai-compatible", "hash-test")


@dataclass(frozen=True, slots=True)
class CompilationProfile:
    name: str
    fts_enabled: bool
    vector_enabled: bool
    vector_provider: str | None
    vector_model: str | None
    vector_dimensions: int
    vector_max_features: int
    fts_candidate_limit: int
    vector_candidate_limit: int
    hybrid_fts_weight: float
    hybrid_vector_weight: float
    hybrid_rrf_k: float

    @property
    def identifier(self) -> str:
        if not self.vector_enabled:
            return self.name
        return (
            f"{self.name}:{self.vector_provider}:{self.vector_model}:"
            f"d{self.vector_dimensions}:f{self.vector_max_features}"
        )

    def as_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "ftsEnabled": self.fts_enabled,
            "vectorEnabled": self.vector_enabled,
            "vectorProvider": self.vector_provider,
            "vectorModel": self.vector_model,
            "vectorDimensions": self.vector_dimensions,
            "vectorMaxFeatures": self.vector_max_features,
            "ftsCandidateLimit": self.fts_candidate_limit,
            "vectorCandidateLimit": self.vector_candidate_limit,
            "hybridFtsWeight": self.hybrid_fts_weight,
            "hybridVectorWeight": self.hybrid_vector_weight,
            "hybridRrfK": self.hybrid_rrf_k,
        }

    def fingerprint_material(self) -> str:
        return canonical_json(self.as_dict())


def _positive_int(value: Any, *, default: int, field: str) -> int:
    try:
        result = int(value)
    except (TypeError, ValueError):
        result = default
    if result <= 0:
        raise UsageError(f"{field} must be positive")
    return result


def _positive_float(value: Any, *, default: float, field: str) -> float:
    try:
        result = float(value)
    except (TypeError, ValueError):
        result = default
    if result <= 0:
        raise UsageError(f"{field} must be positive")
    return result


def resolve_profile(
    world: World,
    *,
    profile_name: str | None = None,
    vector_provider: str | None = None,
    vector_model: str | None = None,
    vector_dimensions: int | None = None,
    vector_max_features: int | None = None,
) -> CompilationProfile:
    compilation = world.config.get("compilation_policy") or {}
    embedding = world.config.get("embedding_policy") or {}
    name = str(profile_name or compilation.get("default_profile") or "hybrid")
    if name not in PROFILE_NAMES:
        raise UsageError(f"Unsupported compilation profile {name!r}; expected one of {', '.join(PROFILE_NAMES)}")

    fts_enabled = name in {"fts", "hybrid"}
    vector_enabled = name in {"vector", "hybrid"}
    provider: str | None = None
    model: str | None = None
    dimensions = _positive_int(
        vector_dimensions if vector_dimensions is not None else embedding.get("dimensions"),
        default=192,
        field="vector dimensions",
    )
    max_features = _positive_int(
        vector_max_features if vector_max_features is not None else embedding.get("max_features"),
        default=8192,
        field="vector max features",
    )
    if vector_enabled:
        provider = str(vector_provider or embedding.get("provider") or "lsa")
        if provider == "hash":
            # Preserve an explicit escape hatch for old tests while preventing
            # accidental claims that feature hashing is semantic retrieval.
            provider = "hash-test"
        if provider not in VECTOR_PROVIDERS:
            raise UsageError(
                f"Unsupported vector provider {provider!r}; expected one of {', '.join(VECTOR_PROVIDERS)}"
            )
        if provider == "lsa":
            model = str(vector_model or embedding.get("model") or "wedl-lsa-v1")
        elif provider == "sentence-transformers":
            model = str(
                vector_model
                or embedding.get("model")
                or "sentence-transformers/all-MiniLM-L6-v2"
            )
        elif provider == "openai-compatible":
            model = str(vector_model or embedding.get("model") or "text-embedding-3-small")
        else:
            model = str(vector_model or embedding.get("model") or "wedl-hash-test-v1")

    retrieval = compilation.get("retrieval") or {}
    return CompilationProfile(
        name=name,
        fts_enabled=fts_enabled,
        vector_enabled=vector_enabled,
        vector_provider=provider,
        vector_model=model,
        vector_dimensions=dimensions,
        vector_max_features=max_features,
        fts_candidate_limit=_positive_int(
            retrieval.get("fts_candidate_limit"), default=120, field="FTS candidate limit"
        ),
        vector_candidate_limit=_positive_int(
            retrieval.get("vector_candidate_limit"), default=120, field="vector candidate limit"
        ),
        hybrid_fts_weight=_positive_float(
            retrieval.get("hybrid_fts_weight"), default=1.0, field="hybrid FTS weight"
        ),
        hybrid_vector_weight=_positive_float(
            retrieval.get("hybrid_vector_weight"), default=1.0, field="hybrid vector weight"
        ),
        hybrid_rrf_k=_positive_float(
            retrieval.get("hybrid_rrf_k"), default=60.0, field="hybrid RRF k"
        ),
    )
