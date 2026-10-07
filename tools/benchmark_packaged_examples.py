"""Convert, compile and rebuild all three pinned packaged examples in contained scratch."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

from benchmark_spatial_browser import (ROOT, PACKAGE_COUNTS, canonical, contained, digest, file_hash,
                                       read_context, require, write_manifest)


def preflight(context):
    import importlib.util
    for module in ("yaml", "numpy", "sklearn", "fastapi"):
        require(importlib.util.find_spec(module) is not None, "missing package runtime: " + module)
    require(Path(sys.executable).resolve() == Path(context["python"]["path"]).resolve()
            and file_hash(sys.executable) == context["python"]["sha256"], "package Python runtime changed")
    data = ROOT / "src/wedl/data"
    legacy = ROOT / "tests/fixtures/legacy_worlds"
    required = set()
    for name in PACKAGE_COUNTS:
        for package in (name, name.removesuffix("_v07")):
            source_root = data if package.endswith("_v07") else legacy
            story = contained(source_root / package / "story", ROOT)
            for path in story.rglob("*.md"):
                path = contained(path, ROOT)
                required.add(path.relative_to(ROOT).as_posix())
    require(required and required == set(context["inputHashes"]), "package input inventory incomplete")


def projection(world):
    return [{"id": identifier, "frontmatter": record.frontmatter, "body": record.body,
             "sourcePath": record.source_path}
            for identifier, record in sorted(world.records.items())]


def run(context):
    import build_v07_packaged_examples as builder
    from wedl.cli import initialize
    from wedl.compiler import cache_readiness, compile_world, world_from_database
    from wedl.repository import Repository
    from wedl.source import split_envelope
    from wedl.validation import validate_world

    preflight(context)
    stage = contained(Path(context["results"]["packaged-example-corpus"]).parent / "packages",
                      context["scratch"], exists=False)
    require(not stage.exists(), "package scratch already exists")
    stage.mkdir()
    data = ROOT / "src/wedl/data"
    legacy_root = builder.LEGACY_ROOT
    reports = builder.run(write=False)
    require({r["package"]: r["records"] for r in reports} == PACKAGE_COUNTS, "conversion counts")
    generated = stage / "generated"
    for package in builder.PACKAGES:
        shutil.copytree(legacy_root / package / "story", generated / package / "story")
    require(builder.run(write=True, data_root=generated, legacy_root=generated) == reports
            and builder.run(write=False, data_root=generated, legacy_root=generated) == reports, "independent conversion drift")
    counts = {"fields": 0, "empty": 0, "nonempty": 0}
    tokens = set()
    packages = []
    for package, report in zip(builder.PACKAGES, reports, strict=True):
        legacy = builder._story_files(legacy_root / package)
        converted = builder._story_files(data / (package + "_v07"))
        regenerated = builder._story_files(generated / (package + "_v07"))
        require(legacy.keys() == converted.keys() == regenerated.keys() and converted == regenerated,
                "converted paths/bytes differ")
        require(builder.source_digest(legacy) == report["legacy_sha256"]
                and builder.source_digest(converted) == report["v07_sha256"], "conversion source digest")
        for path in legacy:
            before, body = split_envelope(legacy[path], path)
            after, new_body = split_envelope(converted[path], path)
            builder.assert_semantic_record(path, before, after, body, new_body)
            if before["kind"] == "object" and "capabilities" in before:
                values = before["capabilities"]
                require(after["object_affordances"] == values, "affordance values/order changed")
                counts["fields"] += 1
                counts["empty" if not values else "nonempty"] += 1
                tokens.update(values)
        root = stage / package
        example = {"ash_archive": "ash-archive-v07", "frontiersmen": "frontiersmen-v07"}.get(package)
        bootstrap = initialize(root, example=example, profile_name="fts")
        repository = Repository(root)
        if package == "chronology_conformance":
            bootstrap_snapshot = repository.snapshot("HEAD")
            replacements = {path: None for path in bootstrap_snapshot.files if path not in converted}
            replacements.update(converted)
            repository.commit_files(expected_head=bootstrap_snapshot.revision, files=replacements,
                                    message="seed exact packaged chronology source")
        require(repository.snapshot("HEAD").files == converted, "bootstrap source bytes differ")
        compiled = compile_world(repository, profile_name="fts")
        require(compiled["recordCount"] == report["records"], "compiled record count")
        source = repository.load_world(cache_write=False)
        require(not [item for item in validate_world(source) if item["severity"] == "error"], "source validation")
        built = world_from_database(repository, Path(compiled["database"]))
        source_projection, built_projection = projection(source), projection(built)
        require(source_projection == built_projection, "source/compiled parity")
        database = contained(Path(compiled["database"]), stage)
        database.unlink()
        require(cache_readiness(repository)["state"] != "ready", "cache deletion not detected")
        rebuilt = compile_world(repository, profile_name="fts")
        require(rebuilt["recordCount"] == report["records"] and cache_readiness(repository)["state"] == "ready",
                "cache rebuild failed")
        rebuilt_projection = projection(world_from_database(repository, Path(rebuilt["database"])))
        require(rebuilt_projection == source_projection and repository.snapshot("HEAD").files == converted,
                "rebuilt projection or source drift")
        packages.append({
            "name": report["package"], "recordCount": report["records"],
            "legacySha256": report["legacy_sha256"], "convertedSha256": report["v07_sha256"],
            "regeneratedSha256": builder.source_digest(regenerated),
            "legacyFiles": {p: hashlib.sha256(b).hexdigest() for p, b in sorted(legacy.items())},
            "convertedFiles": {p: hashlib.sha256(b).hexdigest() for p, b in sorted(converted.items())},
            "sourceProjection": source_projection, "compiledProjection": built_projection,
            "rebuiltProjection": rebuilt_projection, "projectionSha256": digest(source_projection),
            "validated": True, "bodiesPreserved": True, "cacheDeleted": True, "cacheRebuilt": True,
        })
    require(counts == {"fields": 53, "empty": 25, "nonempty": 28} and len(tokens) == 19,
            "full affordance corpus changed")
    preflight(read_context(Path(context["contextPath"])))
    proof = {"kind": "packaged-example-corpus", "packageCount": 3, "packages": packages,
             "affordances": counts, "tokens": sorted(tokens),
             "conversionSha256": digest([{key: p[key] for key in
                 ("name", "legacySha256", "convertedSha256", "regeneratedSha256")} for p in packages]),
             "rebuildSha256": digest([{key: p[key] for key in ("name", "projectionSha256")} for p in packages]),
             "hashContract": "canonical normalized source/frontmatter/body/path projections; revisions and SQLite bytes excluded"}
    write_manifest(Path(context["results"]["packaged-example-corpus"]), proof, context)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--context", type=Path, required=True)
    parser.add_argument("--preflight", action="store_true")
    args = parser.parse_args()
    context = read_context(args.context)
    if args.preflight:
        preflight(context)
        print("Pinned package paths/runtime verified; no conversion or compilation executed.")
    else:
        run(context)


if __name__ == "__main__":
    main()

