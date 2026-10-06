"""Build or check explicit v0.7 copies of the three pinned packaged worlds.

Run with ``--write`` to publish the derived packages, or ``--check`` to detect
drift without writing. This tool deliberately operates on package files only;
it never migrates a user's repository.
"""

from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from wedl.migration import _v07_candidate, _world_for_candidate  # noqa: E402
from wedl.repository import Repository, Snapshot  # noqa: E402
from wedl.source import serialize_record, split_envelope  # noqa: E402
from wedl.validation import validate_world  # noqa: E402


PACKAGES = ("ash_archive", "frontiersmen", "chronology_conformance")
CAPABILITIES = ["generational-core-v1", "spatial-core-v1"]
PINNED_LEGACY = {
    "ash_archive": (262, "e627856da258becf7ce6c32f24fbcee74d0ed24b08b4f9aca1cce87e55f7cdf0"),
    "frontiersmen": (309, "c9c451eb7b0de71531974bcc7de25594509d669cd302d337958af22e4883d28c"),
    "chronology_conformance": (7, "0f8dd8e07395f4c888dff6cf24a6d583e0b8f19e76f52f129178398d89497d4b"),
}


def source_digest(files: dict[str, bytes]) -> str:
    material = bytearray()
    for path, data in sorted(files.items()):
        material.extend(path.encode("utf-8"))
        material.extend(b"\0")
        material.extend(hashlib.sha256(data).digest())
        material.extend(b"\n")
    return hashlib.sha256(material).hexdigest()


def _story_files(package: Path) -> dict[str, bytes]:
    story = package / "story"
    if not story.is_dir():
        raise ValueError(f"missing story directory: {story}")
    files = {path.relative_to(package).as_posix(): path.read_bytes() for path in story.rglob("*.md") if path.is_file()}
    if not files:
        raise ValueError(f"no Markdown records in {story}")
    return files


def assert_semantic_record(path: str, previous: dict, frontmatter: dict, previous_body: str, body: str) -> None:
    """Permit only the reviewed schema and capability envelope changes."""

    old_semantics = deepcopy(previous)
    new_semantics = deepcopy(frontmatter)
    old_semantics.pop("schema", None)
    new_semantics.pop("schema", None)
    if previous.get("kind") == "world":
        if frontmatter.get("capabilities") != CAPABILITIES:
            raise ValueError(f"{path}: invalid world capability order")
        old_semantics.pop("capabilities", None)
        new_semantics.pop("capabilities", None)
    elif "capabilities" in frontmatter:
        raise ValueError(f"{path}: non-world capabilities")
    elif previous.get("kind") == "object" and "capabilities" in previous:
        # ADR 0006 preserves the exact authored array, including [] and
        # duplicate tokens. Absence remains absence on both sides.
        old_semantics["object_affordances"] = old_semantics.pop("capabilities")
    if frontmatter.get("schema") != "wedl/v0.7" or old_semantics != new_semantics or body != previous_body:
        raise ValueError(f"{path}: conversion changed record semantics or Markdown body")


def convert_package(source: Path) -> tuple[dict[str, bytes], dict[str, str | int]]:
    """Use migration's vetted candidate transform and reject semantic drift."""

    original = _story_files(source)
    expected = PINNED_LEGACY.get(source.name)
    actual = (len(original), source_digest(original))
    if expected is None or actual != expected:
        raise ValueError(
            f"{source.name}: conversion refused: source differs from pinned legacy package "
            f"(records={actual[0]}, sha256={actual[1]}; expected={expected})"
        )
    repository = Repository(source)
    snapshot = Snapshot("PACKAGED", "PACKAGED", original, {})
    raw = [(path, *split_envelope(data, path), data) for path, data in sorted(original.items())]
    candidate, diagnostics, no_op = _v07_candidate(repository, snapshot, raw)
    if diagnostics or no_op or not candidate:
        raise ValueError(f"{source.name}: conversion refused: {diagnostics or 'source is already v0.7'}")
    validation = validate_world(_world_for_candidate(repository, snapshot.revision, snapshot.tree_oid, candidate))
    errors = [item for item in validation if item["severity"] == "error"]
    if errors:
        raise ValueError(f"{source.name}: converted source invalid: {errors[:3]}")
    converted: dict[str, bytes] = {}
    world_count = 0
    for path, frontmatter, body in candidate:
        previous, previous_body = split_envelope(original[path], path)
        if frontmatter.get("kind") == "world":
            world_count += 1
        assert_semantic_record(path, previous, frontmatter, previous_body, body)
        data = serialize_record(frontmatter, body)
        parsed, converted_body = split_envelope(data, path)
        # The canonical v0.7 serializer spells the legacy location containment
        # alias ``parent`` as ``parent_id``. The referenced ID is unchanged.
        expected = deepcopy(frontmatter)
        if expected.get("kind") == "location" and "parent" in expected:
            expected["parent_id"] = expected.pop("parent")
        if parsed != expected or converted_body != body:
            raise ValueError(f"{path}: serialization changed record semantics or Markdown body")
        converted[path] = data
    if world_count != 1 or set(converted) != set(original):
        raise ValueError(f"{source.name}: conversion changed world or record paths")
    return converted, {"records": len(converted), "legacy_sha256": source_digest(original), "v07_sha256": source_digest(converted)}


def package_files(source: Path) -> tuple[dict[str, bytes], dict[str, str | int]]:
    converted, report = convert_package(source)
    package_name = source.name + "_v07"
    files = {"__init__.py": f'"""Explicit v0.7 copy of {source.name}."""\n'.encode("utf-8"), **converted}
    return files, {"package": package_name, **report}


def _redirected(path: Path) -> bool:
    return path.is_symlink() or (hasattr(path, "is_junction") and path.is_junction())


def _assert_safe_target(data_root: Path, target: Path) -> None:
    """Never follow a destination link into a pinned source or another tree."""

    current = target
    while current != data_root:
        if _redirected(current):
            raise ValueError(f"symlinked destination is not allowed: {current}")
        if current.is_file() and current.stat().st_nlink > 1:
            raise ValueError(f"hard-linked destination is not allowed: {current}")
        current = current.parent
    if _redirected(data_root) or not target.resolve(strict=False).is_relative_to(data_root.resolve(strict=True)):
        raise ValueError(f"destination escapes data root: {target}")


def _expected_bytecode(relative: Path, files: dict[str, bytes]) -> bool:
    """Recognize disposable caches for expected modules across supported Python versions."""

    if len(relative.parts) != 2 or relative.parts[0] != "__pycache__":
        return False
    parts = relative.name.split(".")
    if len(parts) not in (3, 4) or parts[-1] != "pyc":
        return False
    if len(parts) == 4 and parts[2] not in ("opt-1", "opt-2"):
        return False
    tag = parts[1]
    version = re.fullmatch(r"cpython-3([1-9][0-9]+)", tag)
    if tag != sys.implementation.cache_tag and not (version and int(version[1]) >= 11):
        return False
    try:
        source = importlib.util.source_from_cache(str(relative))
    except ValueError:
        return False
    return source in files and Path(source).suffix == ".py"


def run(*, write: bool, names: tuple[str, ...] = PACKAGES, data_root: Path = ROOT / "src" / "wedl" / "data") -> list[dict[str, str | int]]:
    prepared = []
    for name in names:
        if name not in PACKAGES:
            raise ValueError(f"unknown package: {name}")
        files, report = package_files(data_root / name)
        destination = data_root / f"{name}_v07"
        _assert_safe_target(data_root, destination)
        existing = {}
        if destination.exists():
            for path in destination.rglob("*"):
                _assert_safe_target(data_root, path)
                relative = path.relative_to(destination)
                if path.is_file():
                    if not _expected_bytecode(relative, files):
                        existing[relative.as_posix()] = path.read_bytes()
                elif "__pycache__" in relative.parts and relative != Path("__pycache__"):
                    # Only the direct cache directory is allowed, including when empty.
                    existing[relative.as_posix()] = b""
        unexpected = sorted(set(existing) - set(files))
        if unexpected:
            raise ValueError(f"{destination}: unexpected files: {unexpected[:5]}")
        drift = sorted(path for path in files if existing.get(path) != files[path])
        prepared.append((destination, files, report, drift))
    if not write:
        stale = [(str(destination), drift[:5]) for destination, _files, _report, drift in prepared if drift]
        if stale:
            raise ValueError(f"packaged v0.7 copies differ from conversion: {stale}")
    else:
        for destination, files, _report, drift in prepared:
            for path in drift:
                target = destination / path
                _assert_safe_target(data_root, target)
                target.parent.mkdir(parents=True, exist_ok=True)
                _assert_safe_target(data_root, target)
                target.write_bytes(files[path])
    return [report for _destination, _files, report, _drift in prepared]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="write only derived v0.7 package files")
    mode.add_argument("--check", action="store_true", help="verify derived packages without writing")
    parser.add_argument("--package", action="append", choices=PACKAGES, help="limit to named source packages")
    args = parser.parse_args()
    try:
        report = run(write=args.write, names=tuple(args.package or PACKAGES))
    except (ValueError, OSError) as exc:
        parser.exit(1, f"v0.7 package conversion failed: {exc}\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
