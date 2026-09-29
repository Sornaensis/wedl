from __future__ import annotations

from importlib import resources
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import warnings

import pytest

from wedl.repository import Repository


_INTENT_MARKERS = {"U": "normal_unit", "I": "normal_integration", "P": "performance"}
_BAD_OUTCOMES: list[str] = []
_CALL_OUTCOMES: list[tuple[str, str]] = []


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addini("wedl_registry_version", "Exact test registry version")
    parser.addini("wedl_snapshot_v2", "Exact versioned baseline, transition and overlay metadata", type="linelist")
    parser.addini("wedl_registry_v2", "Exact node IDs and audited test intent", type="linelist")
    parser.addini("wedl_active_performance_v2", "Exact active performance node IDs", type="linelist")
    parser.addoption("--wedl-suite", choices=("all", "normal", "performance"), default="all")
    parser.addoption("--wedl-strict", action="store_true", default=False)


def _suite_registry(config: pytest.Config) -> tuple[dict[str, str], set[str], dict[str, tuple[str, set[str]]], int]:
    if config.getini("wedl_registry_version") != "2":
        raise pytest.UsageError("Unsupported or missing WEDL test registry version")
    counts: dict[str, int] = {}
    added: dict[str, str] = {}
    overlays: dict[str, tuple[str, set[str]]] = {}
    baseline_digest: str | None = None
    for entry in config.getini("wedl_snapshot_v2"):
        parts = entry.split("|")
        tag = parts[0]
        if tag in {"baseline_clean", "baseline_held"} and len(parts) == 2 and parts[1].isdigit():
            if tag in counts:
                raise pytest.UsageError(f"Duplicate WEDL snapshot field: {tag}")
            counts[tag] = int(parts[1])
        elif tag == "active_performance" and len(parts) == 2 and parts[1].isdigit():
            if tag in counts:
                raise pytest.UsageError("Duplicate WEDL active performance count")
            counts[tag] = int(parts[1])
        elif tag == "intent" and len(parts) == 3 and parts[1] in _INTENT_MARKERS and parts[2].isdigit():
            key = f"intent_{parts[1]}"
            if key in counts:
                raise pytest.UsageError(f"Duplicate WEDL snapshot field: {key}")
            counts[key] = int(parts[2])
        elif tag == "baseline_registry_sha256" and len(parts) == 2 and len(parts[1]) == 64:
            if baseline_digest is not None:
                raise pytest.UsageError("Duplicate WEDL baseline digest")
            baseline_digest = parts[1]
        elif tag == "added" and len(parts) == 3 and parts[1] in _INTENT_MARKERS and parts[2].startswith("tests/") and "::" in parts[2]:
            if parts[2] in added:
                raise pytest.UsageError(f"Duplicate WEDL added node: {parts[2]}")
            added[parts[2]] = parts[1]
        elif tag == "overlay" and len(parts) >= 4 and parts[1].startswith("tests/") and parts[1].endswith(".py") and len(parts[2]) == 64:
            name, digest, *nodes = parts[1:]
            if name in overlays or len(nodes) != len(set(nodes)) or not nodes or any(node.split("::", 1)[0] != name for node in nodes):
                raise pytest.UsageError(f"Invalid WEDL overlay composition: {name}")
            overlays[name] = (digest, set(nodes))
        else:
            raise pytest.UsageError(f"Invalid WEDL snapshot entry: {entry}")
    if set(counts) != {"baseline_clean", "baseline_held", "intent_U", "intent_I", "intent_P", "active_performance"} or baseline_digest is None:
        raise pytest.UsageError("Incomplete WEDL snapshot metadata")
    overlay_ids = set().union(*(nodes for _, nodes in overlays.values()))
    if counts["baseline_held"] - counts["baseline_clean"] != len(overlay_ids):
        raise pytest.UsageError("WEDL baseline overlay count drifted")
    intent: dict[str, str] = {}
    for entry in config.getini("wedl_registry_v2"):
        kind, separator, node = entry.partition("|")
        if not separator or kind not in _INTENT_MARKERS or not node.startswith("tests/") or "::" not in node:
            raise pytest.UsageError(f"Invalid WEDL registry entry: {entry}")
        if node in intent:
            raise pytest.UsageError(f"Duplicate WEDL registry node: {node}")
        intent[node] = kind
    if any(intent.get(node) != kind for node, kind in added.items()):
        raise pytest.UsageError("WEDL reviewed transition nodes differ from registry")
    baseline = sorted(f"{kind}|{node}" for node, kind in intent.items() if node not in added)
    if hashlib.sha256(("\n".join(baseline) + "\n").encode()).hexdigest() != baseline_digest:
        raise pytest.UsageError("WEDL baseline registry digest drifted")
    if any(not nodes.issubset(intent) for _, nodes in overlays.values()) or len(intent) != counts["baseline_held"] + len(added):
        raise pytest.UsageError("WEDL registry or overlay node total drifted")
    if {kind: list(intent.values()).count(kind) for kind in _INTENT_MARKERS} != {kind: counts[f"intent_{kind}"] for kind in _INTENT_MARKERS}:
        raise pytest.UsageError("WEDL registry count or intent totals drifted; update the exact registry")
    active_lines = config.getini("wedl_active_performance_v2")
    active = set(active_lines)
    performance = {node for node, kind in intent.items() if kind == "P"}
    if len(active) != len(active_lines) or len(active) != counts["active_performance"] or active != performance:
        raise pytest.UsageError("Invalid or duplicate active performance node")
    return intent, active, overlays, counts["baseline_clean"] + len(added)


def _check_overlay(config: pytest.Config, intent: dict[str, str],
                   overlays: dict[str, tuple[str, set[str]]], clean_total: int) -> set[str]:
    root = config.rootpath
    present = {name for name in overlays if (root / name).is_file()}
    if present and present != set(overlays):
        raise pytest.UsageError("Partial test overlay; both hash-bound files are required")
    if present:
        for name, (expected, _) in overlays.items():
            actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
            if actual != expected:
                raise pytest.UsageError(f"Test overlay hash mismatch: {name}")
        if (root / ".git").exists():
            for name in overlays:
                tracked = subprocess.run(
                    ["git", "-C", str(root), "ls-files", "--error-unmatch", "--", name],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
                )
                if tracked.returncode == 0:
                    raise pytest.UsageError("Tracked overlay transition requires a registry rebaseline")
    overlay_ids = set().union(*(nodes for _, nodes in overlays.values()))
    expected = set(intent) if present else set(intent) - overlay_ids
    if not present and len(expected) != clean_total:
        raise pytest.UsageError("WEDL clean snapshot count drifted")
    return expected


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    intent, active, overlays, clean_total = _suite_registry(config)
    expected = _check_overlay(config, intent, overlays, clean_total)
    observed = [item.nodeid.replace("\\", "/") for item in items]
    if len(observed) != len(set(observed)):
        raise pytest.UsageError("Duplicate collected WEDL node ID")
    unknown = set(observed) - expected
    if unknown:
        raise pytest.UsageError(f"Unknown collected WEDL node ID: {sorted(unknown)[0]}")
    if list(config.args) == ["tests"] and set(observed) != expected:
        raise pytest.UsageError("Full WEDL collection differs from the exact registry snapshot")
    for item, node in zip(items, observed):
        declared = {marker.name for marker in item.iter_markers()} & set(_INTENT_MARKERS.values())
        required = _INTENT_MARKERS[intent[node]]
        if declared and declared != {required}:
            raise pytest.UsageError(f"WEDL marker disagrees with exact registry: {node}")
        item.add_marker(required)
        if config.getoption("--wedl-strict") and any(any(item.iter_markers(name=name)) for name in ("skip", "skipif", "xfail")):
            raise pytest.UsageError(f"Skip or expected failure is forbidden in WEDL suite: {node}")
    suite = config.getoption("--wedl-suite")
    if suite != "all":
        selected = [item for item in items if (item.nodeid.replace("\\", "/") in active) == (suite == "performance")]
        deselected = [item for item in items if item not in selected]
        if deselected:
            config.hook.pytest_deselected(items=deselected)
        items[:] = selected


def pytest_runtest_logreport(report: pytest.TestReport) -> None:
    if report.when == "call":
        _CALL_OUTCOMES.append((report.nodeid.replace("\\", "/"), report.outcome))
    if report.skipped or getattr(report, "wasxfail", None):
        _BAD_OUTCOMES.append(f"{report.nodeid}: {report.outcome}")


def pytest_sessionfinish(session: pytest.Session, exitstatus: int) -> None:
    outcome_path = os.environ.get("WEDL_TEST_OUTCOMES")
    if outcome_path:
        Path(outcome_path).write_text(json.dumps(_CALL_OUTCOMES), encoding="utf-8")
    if session.config.getoption("--wedl-strict") and _BAD_OUTCOMES:
        session.exitstatus = 1
        warnings.warn(pytest.PytestWarning("WEDL suite forbids skipped/xfail/xpass outcomes: " + "; ".join(_BAD_OUTCOMES[:3])))


def git(root: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", "-C", str(root), *args], check=check, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if os.name == "nt" and args and args[0] == "init":
        subprocess.run(["git", "-C", str(root), "config", "core.longpaths", "true"], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return result


@pytest.fixture()
def ash_repo(tmp_path: Path) -> Repository:
    root = tmp_path / "ash"
    root.mkdir()
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed")
    return Repository(root)


def _task91_tree(root: Path) -> dict[str, bytes]:
    """Capture the seed's complete byte image for isolation checks."""

    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file()
    }


def _task91_is_reparse_point(path: Path) -> bool:
    attributes = getattr(os.lstat(path), "st_file_attributes", 0)
    return bool(attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))


def _assert_task91_seed_is_clean(root: Path) -> None:
    assert (root / ".git").is_dir()
    assert not (root / ".git" / "index.lock").exists()
    assert not (root / ".wedl").exists()


def _assert_task91_copy_is_independent(seed: Path, copy: Path) -> None:
    assert not os.path.samefile(seed, copy)
    assert _task91_tree(copy) == _task91_tree(seed)
    for source in seed.rglob("*"):
        target = copy / source.relative_to(seed)
        assert target.exists()
        assert not source.is_symlink()
        assert not target.is_symlink()
        assert not _task91_is_reparse_point(source)
        assert not _task91_is_reparse_point(target)
        assert not os.path.samefile(source, target)


@pytest.fixture(scope="session")
def _task91_seed(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("task91-seed") / "repository"
    root.mkdir()
    world = resources.files("wedl.data.ash_archive").joinpath("story", "world.md")
    (root / "story").mkdir()
    with resources.as_file(world) as world_path:
        shutil.copyfile(world_path, root / "story" / "world.md")
    (root / ".gitignore").write_text(".wedl/\n")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed")
    _assert_task91_seed_is_clean(root)
    return root


@pytest.fixture()
def task91_repo(tmp_path: Path, _task91_seed: Path) -> Repository:
    root = tmp_path / "task91"
    seed_before = _task91_tree(_task91_seed)
    shutil.copytree(_task91_seed, root, copy_function=shutil.copy2)
    _assert_task91_seed_is_clean(_task91_seed)
    _assert_task91_seed_is_clean(root)
    _assert_task91_copy_is_independent(_task91_seed, root)
    # A byte copy necessarily gives the worktree new filesystem timestamps.
    # Refresh only this private copy before tests take their index snapshots.
    git(root, "update-index", "--refresh")
    _assert_task91_seed_is_clean(_task91_seed)
    _assert_task91_seed_is_clean(root)
    repository = Repository(root)
    yield repository
    assert _task91_tree(_task91_seed) == seed_before
    _assert_task91_seed_is_clean(_task91_seed)


@pytest.fixture()
def frontiersmen_repo(tmp_path: Path) -> Repository:
    root = tmp_path / "frontiersmen"
    root.mkdir()
    source = resources.files("wedl.data.frontiersmen").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed Frontiersmen")
    return Repository(root)
