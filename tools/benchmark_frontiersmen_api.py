from __future__ import annotations

from fastapi.testclient import TestClient
from importlib import resources
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time
from typing import Any, Callable

from wedl.compiler import compile_world
from wedl.repository import Repository
from wedl.server import create_app


def git(root: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def make_repo(root: Path) -> Repository:
    root.mkdir()
    source = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "legacy_worlds" / "frontiersmen" / "story"
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl api benchmark")
    git(root, "config", "user.email", "api@wedl.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed")
    repository = Repository(root)
    compile_world(repository, force=True, profile_name="hybrid", vector_provider="lsa")
    return repository


def measure(fn: Callable[[], Any], repeats: int = 25) -> dict[str, Any]:
    values: list[float] = []
    response: Any = None
    for _ in range(repeats):
        started = time.perf_counter()
        response = fn()
        values.append((time.perf_counter() - started) * 1000.0)
        assert response.status_code == 200, response.text
    ordered = sorted(values)
    return {
        "medianMs": round(statistics.median(values), 3),
        "meanMs": round(statistics.mean(values), 3),
        "p95Ms": round(ordered[min(len(ordered)-1, int(repeats*0.95))], 3),
        "minimumMs": round(min(values), 3),
        "maximumMs": round(max(values), 3),
        "repeats": repeats,
        "responseBytes": len(response.content),
    }


def main(output: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="wedl-frontiersmen-api-") as temporary:
        repository = make_repo(Path(temporary) / "world")
        world = repository.load_world()
        rhea = world.find("Rhea", "character")
        rootjaw = world.find("Rootjaw", "character")
        scene = world.find("The Hunt Begins", "scene")
        pursuit = world.find("Running Under the Drums", "conversation")
        with TestClient(create_app(repository.root)) as client:
            session = client.get("/api/session").json()
            results = {
                "status": measure(lambda: client.get("/api/status")),
                "characterList": measure(lambda: client.get("/api/entities", params={"kind": "character"})),
                "ftsSearch": measure(lambda: client.get("/api/search", params={"q": "blood quickens amber", "mode": "fts", "scene": scene.id})),
                "vectorSearch": measure(lambda: client.get("/api/search", params={"q": "memory of wounded animals taking shape", "mode": "vector", "scene": scene.id})),
                "hybridRheaSearch": measure(lambda: client.get("/api/search", params={"q": "escape masked host bone key", "mode": "hybrid", "perspective": "character", "character": rhea.id, "scene": scene.id})),
                "rheaContext5000": measure(lambda: client.get("/api/context", params={"character": rhea.id, "scene": scene.id, "q": "escape masked host through spruce", "maxCharacters": 5000, "maxItems": 18})),
                "rootjawConversation": measure(lambda: client.get(f"/api/conversations/{pursuit.id}", params={"perspective": "character", "character": rootjaw.id})),
                "storyPoints": measure(lambda: client.get("/api/story-points", params={"scene": scene.id})),
                "unauthorizedCompileStatus": client.post("/api/compile").status_code,
                "authorizedCompileStatus": client.post("/api/compile", headers={"X-Wedl-Token": session["token"]}).status_code,
            }
    output.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    main(parser.parse_args().output)
