"""Authenticated performance manifests and the contained authored browser stage.

Canonical hashes are SHA256 of UTF-8 JSON, sorted keys, compact separators,
ensure_ascii=False and allow_nan=False. Only resultSha256 excludes itself.
ValidateOnly/preflight proves contracts, never execution or latency.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import math
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
from urllib.parse import urlparse
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
STAGES = ("actual-100k-browser", "generational-5k-10k", "object-affordance-corpus", "packaged-example-corpus")
AFFORDANCE_NODE = "tests/test_object_affordances.py::test_legacy_corpus_affordances_survive_candidate_conversion"
PACKAGE_COUNTS = {"ash_archive_v07": 262, "frontiersmen_v07": 309, "chronology_conformance_v07": 7}
GOLDEN_AUTHORED = "924318fb126571a159be87a66ff8febd76fc19c262756beda312aa3eab69b7a5"
GOLDEN_MAPPED = "728193728c7561d41a13e20c3fc5cef0cb16eca255d46fcfc26f4920fe04ce97"
WARM_OPERATIONS = ("lineageEarly", "lineageLate", "rosterEarly", "rosterLate", "holderEarly", "holderLate")


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False,
                      allow_nan=False).encode("utf-8")


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def file_hash(path):
    with Path(path).open("rb") as stream:
        result = hashlib.file_digest(stream, "sha256").hexdigest()
    return result


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(value):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), "malformed SHA256")
    return value


def contained(path, parent, *, exists=True):
    path, parent = Path(path), Path(parent)
    require(path.is_absolute() and parent.is_absolute(), "absolute paths required")
    require(".." not in path.parts, "parent alias refused")
    absolute = Path(os.path.abspath(path))
    resolved = path.resolve(strict=exists)
    require(os.path.normcase(str(absolute)) == os.path.normcase(str(resolved)), "path alias refused")
    require(resolved != parent.resolve() and resolved.is_relative_to(parent.resolve()), "path outside containment")
    for item in (path, *path.parents):
        if not item.exists():
            continue
        info = item.lstat()
        require(not item.is_symlink() and not getattr(info, "st_file_attributes", 0) & 0x400,
                "reparse/symlink path refused")
        if item.is_file():
            require(info.st_nlink == 1, "hard-linked evidence refused")
    return resolved


def git(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True,
                          text=True, timeout=10).stdout.strip()


def load_json(path):
    def unique(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate JSON key")
            result[key] = value
        return result
    require(Path(path).stat().st_size <= 16 * 1024 * 1024, "manifest byte bound exceeded")
    return json.loads(Path(path).read_text(encoding="utf-8-sig"), object_pairs_hook=unique,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError("nonfinite JSON")))


def read_context(path):
    context = load_json(path)
    require(context.get("schemaVersion") == 1, "context schema")
    for key in ("runId", "nonce"):
        require(isinstance(context.get(key), str) and re.fullmatch(r"[0-9a-f]{32}", context[key]),
                "context run ID/nonce")
    require(context["runId"] != context["nonce"], "nonce must differ from run ID")
    require(Path(context["root"]).resolve() == ROOT, "context repository")
    require(context["head"] == git(ROOT, "rev-parse", "HEAD"), "context HEAD changed")
    scratch = Path(context["scratch"])
    require(scratch.name == "wedl-performance-" + context["runId"], "scratch/run ID mismatch")
    contained(path, scratch)
    require(context["affordanceNode"] == AFFORDANCE_NODE, "attestation node wiring")
    require(set(context["results"]) == set(STAGES), "mandatory stage roster")
    require(len(set(context["results"].values())) == 4, "duplicate result path")
    for name, result in context["results"].items():
        contained(result, scratch, exists=Path(result).exists())
        require(Path(result).name == name + ".json", "stage result path")
    require(str(path) == context["contextPath"], "context path mismatch")
    specs = context["stageSpecs"]
    require(set(specs) == set(STAGES), "unknown/missing stage specification")
    expected_commands = {"actual-100k-browser": "tools/benchmark_spatial_browser.py",
                         "generational-5k-10k": "tools/benchmark_generational.py",
                         "packaged-example-corpus": "tools/benchmark_packaged_examples.py"}
    for stage in STAGES:
        spec = specs[stage]
        require(spec["result"] == context["results"][stage], "stage result wiring")
        if stage == "object-affordance-corpus":
            require(spec.get("kind") == "pytest-attested" and spec.get("node") == AFFORDANCE_NODE
                    and not spec.get("arguments"), "once-only affordance wiring")
        else:
            require(spec.get("kind") == "executable" and spec.get("arguments")
                    and spec["arguments"][0] == expected_commands[stage], "stage command wiring")
            expected = ([expected_commands[stage], "--output", str(Path(context["scratch"]) / "evidence/generational-raw.json"),
                         "--work-dir", str(Path(context["scratch"]) / "generational-fixture"), "--repeats", "5"]
                        if stage == "generational-5k-10k" else [expected_commands[stage], "--context", context["contextPath"]])
            require(spec["arguments"] == expected, "partial/wrong stage arguments")
    require(file_hash(ROOT / "pytest.ini") == sha(context["registrySha256"]), "registry changed")
    for relative, expected in context["codeHashes"].items():
        require(not Path(relative).is_absolute() and ".." not in Path(relative).parts, "code path")
        require(file_hash(contained(ROOT / relative, ROOT)) == sha(expected), "code bytes changed: " + relative)
    for relative, expected in context["inputHashes"].items():
        require(file_hash(contained(ROOT / relative, ROOT)) == sha(expected), "input bytes changed: " + relative)
    required = {"tools/test-performance.ps1", "tools/benchmark_spatial_browser.py",
                "tools/benchmark_packaged_examples.py", "tools/benchmark_generational.py",
                "tools/generate_generational_fixture.py", "tools/benchmark_spatial_release.py",
                "tools/build_v07_packaged_examples.py", "tests/test_object_affordances.py"}
    require(required <= set(context["codeHashes"]), "incomplete code seal")
    require(context["inputHashes"], "missing package input seal")
    return context


def envelope(proof, context):
    require("resultSha256" not in proof, "self hash must be generated")
    result = {**proof, "schemaVersion": 1, "runId": context["runId"], "nonce": context["nonce"],
              "head": context["head"], "registrySha256": context["registrySha256"],
              "codeHashes": context["codeHashes"], "inputHashes": context["inputHashes"]}
    result["resultSha256"] = digest(result)
    return result


def write_manifest(path, proof, context):
    path = contained(path, context["scratch"], exists=False)
    require(not path.exists(), "result already exists")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    require(not temporary.exists(), "temporary result already exists")
    value = envelope(proof, context)
    try:
        with temporary.open("xb") as stream:
            stream.write(canonical(value) + b"\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()
    return value


def source_files_digest(files):
    material = bytearray()
    for path, value in sorted(files.items()):
        require(isinstance(path, str) and path.startswith("story/") and ".." not in Path(path).parts,
                "source path")
        material.extend(path.encode("utf-8") + b"\0" + bytes.fromhex(sha(value)) + b"\n")
    return hashlib.sha256(material).hexdigest()


def validate_affordance(value, context, outcomes):
    require(value.get("nodeId") == AFFORDANCE_NODE, "wrong attestation node")
    require(isinstance(outcomes, list) and sum(row == [AFFORDANCE_NODE, "passed"] for row in outcomes) == 1
            and sum(row[0] == AFFORDANCE_NODE for row in outcomes) == 1, "exact node not uniquely passed")
    require((value.get("fieldCount"), value.get("emptyFieldCount"), value.get("nonemptyFieldCount"),
             value.get("tokenCount")) == (53, 25, 28, 19), "affordance counts")
    rows, mapped = value["authoredRows"], value["mappedRows"]
    require(len(rows) == len(mapped) == 53 and digest(rows) == value["authoredSha256"] == GOLDEN_AUTHORED
            and digest(mapped) == value["mappedSha256"] == GOLDEN_MAPPED, "affordance golden rows")
    require(sum(not row["capabilities"] for row in rows) == 25, "empty affordance evidence")
    tokens = sorted({token for row in rows for token in row["capabilities"]})
    require(tokens == value["tokens"] and len(tokens) == 19, "affordance token evidence")
    packages = value["packages"]
    require({p["name"]: (p["recordCount"], p["fieldCount"]) for p in packages}
            == {"ash": (262, 24), "frontiersmen": (309, 29)} and len(packages) == 2, "affordance packages")
    for package in packages:
        require(len(package["sourceFiles"]) == package["recordCount"]
                and source_files_digest(package["sourceFiles"]) == package["sourceSha256"], "package source digest")
        prefix = "tests/fixtures/legacy_worlds/" + ("ash_archive" if package["name"] == "ash" else "frontiersmen") + "/"
        require(package["sourceFiles"] == {p.removeprefix(prefix): h for p, h in context["inputHashes"].items()
                                          if p.startswith(prefix)}, "affordance package input mismatch")
        require(set(package["lifecycle"]) == {"candidate", "preview", "apply", "replay", "sourceReload",
                                              "compiledDetail", "noOp", "rollback"}
                and all(flag is True for flag in package["lifecycle"].values()), "affordance lifecycle")


def validate_generational(value):
    require(value.get("kind") == "authored-generational", "generational kind")
    minimum = {"characters": 5000, "kinshipEdges": 10000, "generations": 100, "transitions": 500}
    require(all(type(value["counts"].get(key)) is int and value["counts"][key] >= count
                for key, count in minimum.items()), "generational dimensions")
    require(value["sourceSha256"] == value["independentSourceSha256"], "independent source mismatch")
    sha(value["sourceSha256"]); sha(value["answerSha256"])
    require(all(value.get(key) is True for key in ("privacyPassed", "horizonPassed", "sourceCompiledParity",
            "cacheRebuildPassed", "warmTargetPassed")) and value.get("cacheState") == "ready", "generational proof")
    require(value.get("warmStatistic") == "maximum operation p95" and value.get("warmTargetMs") == 250,
            "generational timing contract")
    operations = value["warmOperations"]
    require(set(operations) == set(WARM_OPERATIONS) == set(value["warmStates"]), "six production warm operations")
    for name, timing in operations.items():
        samples = timing["samples"]
        require(len(samples) >= 3 and all(isinstance(x, (int, float)) and math.isfinite(x) and x > 0
                                        for x in samples), "warm samples")
        ordered = sorted(samples)
        rank = (len(ordered) - 1) * .95
        lower = int(rank)
        expected_p95 = round(ordered[lower] + (ordered[min(lower + 1, len(ordered) - 1)] - ordered[lower]) * (rank - lower), 3)
        require(isinstance(timing["p95"], (int, float)) and math.isfinite(timing["p95"])
                and 0 < timing["p95"] <= 250 and abs(timing["p95"] - expected_p95) <= .001,
                "warm operation p95")
        require(value["warmStates"][name] == "available", "warm answer unavailable")
    require(value["warmQueryMs"] == max(row["p95"] for row in operations.values()), "maximum p95 mismatch")
    require(value["recordKinds"] == value["compiledRecordKinds"], "record parity")
    require(value["recordKinds"]["character"] == value["counts"]["characters"]
            and value["recordKinds"]["parentage"] == value["counts"]["kinshipEdges"], "generational count evidence")
    require(value.get("knownAnswers") and value["profile"]["repeats"] >= 3, "generational answer/repeat evidence")


def validate_package(value, context):
    require(value.get("kind") == "packaged-example-corpus" and value.get("packageCount") == 3, "package stage")
    packages = value["packages"]
    require(len(packages) == 3 and {p["name"]: p["recordCount"] for p in packages} == PACKAGE_COUNTS,
            "package names/counts")
    require(value["affordances"] == {"fields": 53, "empty": 25, "nonempty": 28}
            and len(value["tokens"]) == len(set(value["tokens"])) == 19, "package affordances")
    for package in packages:
        for field, name in (("legacyFiles", package["name"].removesuffix("_v07")), ("convertedFiles", package["name"])):
            source_root = "tests/fixtures/legacy_worlds/" if field == "legacyFiles" else "src/wedl/data/"
            prefix = source_root + name + "/"
            require(package[field] == {p.removeprefix(prefix): h for p, h in context["inputHashes"].items()
                                        if p.startswith(prefix)}, "package input mismatch")
        require(package["legacySha256"] == source_files_digest(package["legacyFiles"])
                and package["convertedSha256"] == source_files_digest(package["convertedFiles"])
                and len(package["convertedFiles"]) == package["recordCount"], "package source evidence")
        require(package["convertedSha256"] == package["regeneratedSha256"]
                and package["sourceProjection"] == package["compiledProjection"] == package["rebuiltProjection"]
                and len(package["sourceProjection"]) == package["recordCount"], "package parity evidence")
        require(all(package.get(key) is True for key in ("validated", "bodiesPreserved", "cacheDeleted", "cacheRebuilt")),
                "package lifecycle")
        require(package["projectionSha256"] == digest(package["sourceProjection"]), "projection digest")
    require(value["conversionSha256"] == digest([{key: p[key] for key in
            ("name", "legacySha256", "convertedSha256", "regeneratedSha256")} for p in packages]), "conversion digest")
    require(value["rebuildSha256"] == digest([{key: p[key] for key in
            ("name", "projectionSha256")} for p in packages]), "rebuild digest")


def source_binding(context):
    path = contained(context["sourceManifest"], context["scratch"])
    source = load_json(path)
    require(source.get("kind") == "authored-source-to-API" and source.get("baseCommit") == context["head"],
            "stale/synthetic spatial source")
    minimum = {"places": 100000, "hierarchyDepth": 128, "rootSiblings": 10000,
               "maps": 32, "routes": 250000, "portals": 100, "overlays": 10000}
    require(all(type(source["counts"].get(k)) is int and source["counts"][k] >= n for k, n in minimum.items()),
            "authored spatial dimensions")
    fixture = contained(source["fixturePath"], context["scratch"])
    require(fixture == path.parent / "fixture" and (fixture / "story").is_dir()
            and (fixture / ".git").exists() and (fixture / ".wedl/world.sqlite").is_file(), "fixture path/type")
    require(git(fixture, "rev-parse", "HEAD") == source["fixtureCommit"], "fixture commit changed")
    require(not git(fixture, "status", "--porcelain", "--untracked-files=no"), "fixture source changed")
    require(source["implementationProvenance"]["kind"] == "exact-HEAD-archive", "unpinned source code")
    phases = {item["phase"] for item in source["phaseTrace"]}
    require({"source-generation", "git-commit", "fixture-retention", "sandbox-teardown"} <= phases,
            "incomplete source lifecycle")
    return source, {"sourceFixturePath": str(fixture), "sourceManifestSha256": file_hash(path),
                    "fixtureCommit": source["fixtureCommit"], "sourceTree": git(fixture, "rev-parse", "HEAD:story")}


def validate_raw_browser(raw):
    require(raw["errors"] == [] and raw["externalDestinations"] == [], "browser errors/external requests")
    checkpoints = raw["checkpoints"]
    for name in ("boot", "children", "childrenPage2", "search"):
        require(0 < checkpoints[name]["placeCards"] <= 100, "incomplete hierarchy/search: " + name)
    require(checkpoints["search"]["placeCards"] == 1, "search did not narrow")
    for name in ("routesIncoming", "routesOutgoing"):
        require(0 < checkpoints[name]["routeCards"] <= 100, "route result missing")
    require(checkpoints["viewport"]["viewportCards"] > 0 and checkpoints["layers"]["layerCards"] > 0,
            "viewport/layer result missing")
    limit = checkpoints.get("pathLimit")
    require((limit == {"status": 422, "state": "limit", "code": "SPATIAL-LIMIT-001", "noPartialFeatureData": True}
             and "SPATIAL-LIMIT-001" in checkpoints["path"]["status"] and not checkpoints["path"]["path"])
            or (limit is None and bool(checkpoints["path"]["path"])), "path neither answer nor closed limit")
    offline = checkpoints["offlineFallback"]
    require(offline["errorVisible"] is True and offline["partialPlaceCards"] == 0
            and offline["compendiumLink"] == "/", "loaded-page offline fallback incomplete")
    requests = raw["requests"]
    require(requests and raw["spatialRequestCount"] == len(requests) and raw["responses"], "request evidence")
    require(raw["requestSha256"] == hashlib.sha256(json.dumps(requests, separators=(",", ":"),
                                                            ensure_ascii=False).encode("utf-8")).hexdigest(),
            "request digest mismatch")
    origin = urlparse(raw["url"])
    require(origin.hostname == "127.0.0.1" and origin.path == "/assets/spatial.html", "nonlocal browser")
    paths = set()
    for request in requests:
        parsed = urlparse(request["url"])
        require(parsed.scheme == "http" and parsed.netloc == origin.netloc, "external request evidence")
        require(parsed.path != "/api/entities", "full-world entity request")
        paths.add(parsed.path)
    require({"/api/spatial/explorer/places", "/api/spatial/explorer/routes", "/api/spatial/explorer/viewport",
             "/api/spatial/explorer/layers", "/api/spatial/path"} <= paths, "missing browser API actions")
    require(all(checkpoints[name]["mountedElements"] > 0 and checkpoints[name]["peakMountedElements"] > 0
                for name in ("boot", "children", "search", "viewport", "layers", "path")), "DOM evidence")


def validate_browser(value, context):
    source, binding = source_binding(context)
    require(all(value.get(key) == expected for key, expected in binding.items()), "browser source binding")
    require(value["places"] == source["counts"]["places"], "browser place count")
    raw_path = contained(value["rawProbePath"], context["scratch"])
    require(file_hash(raw_path) == sha(value["rawProbeSha256"]), "raw browser digest")
    raw = load_json(raw_path)
    validate_raw_browser(raw)
    require(set(value["checkpoints"]) == {"boot", "hierarchy", "search", "viewport", "layers", "routes", "path", "offline"}
            and all(flag is True for flag in value["checkpoints"].values()), "normalized browser checkpoints")
    require(value["requestCount"] == len(raw["requests"]) and value["networkDestinations"] == raw["externalDestinations"]
            and value["domCount"] == max(row.get("peakMountedElements", 0) for row in raw["checkpoints"].values()),
            "browser normalized measurements")
    require(value["cleanup"]["jobEmptySamples"] == [0, 0] and value["cleanup"]["sampleGapSeconds"] >= .1
            and value["cleanup"]["handlesClosed"] is True and value["cleanup"]["probeExitCode"] == 0,
            "browser resource closure")
    require(value["offlineScope"] == "loaded-page refresh failure and local compendium link", "offline scope")
    require(value["browserVersion"] == raw["browserVersion"], "browser version")


def validate_manifest(path, stage, context, outcomes=None):
    require(stage in STAGES and str(Path(path)) == context["results"][stage], "wrong stage result path")
    value = load_json(contained(path, context["scratch"]))
    sha(value.get("resultSha256"))
    require(value["resultSha256"] == digest({k: v for k, v in value.items() if k != "resultSha256"}),
            "canonical result digest mismatch")
    for key in ("schemaVersion", "runId", "nonce", "head", "registrySha256", "codeHashes", "inputHashes"):
        require(value.get(key) == context[key], "manifest context mismatch: " + key)
    if stage == "object-affordance-corpus":
        require(value.get("kind") == stage, "affordance kind")
        validate_affordance(value, context, outcomes)
    elif stage == "packaged-example-corpus":
        validate_package(value, context)
    elif stage == "generational-5k-10k":
        validate_generational(value)
        raw = contained(value["rawResultPath"], context["scratch"])
        require(file_hash(raw) == value["rawResultSha256"], "generational raw digest")
        original = load_json(raw)
        require(all(value[k] == v for k, v in original.items()), "generational payload differs from raw")
    else:
        require(value.get("kind") == "actual-browser", "browser kind")
        validate_browser(value, context)
    return value


BROWSER_PROBE = r"""
// Measured loaded-page browser flow against a contained authored fixture.
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

const require = createRequire(import.meta.url);
const playwright = require(process.env.PLAYWRIGHT_CORE_PATH || 'playwright-core');
const [url, output] = process.argv.slice(2);
const startedUtc = new Date().toISOString();
const started = performance.now();
if (!url || !output) throw new Error('usage: node task6d-browser-probe.mjs URL OUTPUT');
if (!/^http:\/\/127\.0\.0\.1:\d+\/assets\/spatial\.html$/.test(url)) throw new Error('local spatial entrypoint only');
const browser = await playwright.chromium.launch({
  headless: true,
  executablePath: process.env.CHROME_PATH || 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  args: ['--no-first-run'],
});
const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, reducedMotion: 'reduce' });
const page = await context.newPage();
const requests = [];
const responses = [];
const responseReads = [];
const checkpoints = {};
const errors = [];
let spatialRequestCount = null;
page.on('request', request => requests.push({ method: request.method(), url: request.url(), bodyBytes: request.postDataBuffer()?.length || 0 }));
page.on('response', response => responseReads.push((async () => {
  if (spatialRequestCount !== null) {
    responses.push({ status: response.status(), url: response.url(), bodyBytes: null, phase: 'lore-handoff' });
    return;
  }
  try { responses.push({ status: response.status(), url: response.url(), bodyBytes: (await response.body()).length }); }
  catch (error) { errors.push(`response body ${response.url()}: ${error}`); }
})()));
page.on('pageerror', error => errors.push(`page: ${error.message}`));
await page.addInitScript(() => {
  globalThis.__domPeak = 0;
  const count = () => { globalThis.__domPeak = Math.max(globalThis.__domPeak, document.querySelectorAll('*').length); };
  document.addEventListener('DOMContentLoaded', () => {
    count();
    new MutationObserver(count).observe(document, { childList: true, subtree: true });
  }, { once: true });
});
async function checkpoint(name) {
  checkpoints[name] = await page.evaluate(() => ({
    mountedElements: document.querySelectorAll('*').length,
    peakMountedElements: globalThis.__domPeak,
    placeCards: document.querySelectorAll('#place-list li').length,
    coordinateFreeCards: [...document.querySelectorAll('#place-list li')].filter(card => card.textContent?.includes('No authored coordinates')).length,
    routeCards: document.querySelectorAll('#route-list li').length,
    viewportCards: document.querySelectorAll('#map-features > *').length,
    layerCards: document.querySelectorAll('#layer-list li').length,
    status: document.querySelector('#spatial-status')?.textContent,
    path: document.querySelector('#path-result')?.textContent,
  }));
}
async function waitApi(path, action) {
  const response = page.waitForResponse(r => new URL(r.url()).pathname === path, { timeout: 120000 });
  await action();
  const result = await response;
  if (!result.ok()) {
    const body = await result.json();
    if (!(path === '/api/spatial/path' && result.status() === 422 &&
          body.state === 'limit' && body.code === 'SPATIAL-LIMIT-001' && body.result == null)) {
      throw new Error(`${path} HTTP ${result.status()}: ${JSON.stringify(body)}`);
    }
    checkpoints.pathLimit = { status: result.status(), state: body.state, code: body.code, noPartialFeatureData: body.result == null };
  }
  return result;
}
try {
  await page.goto(url, { waitUntil: 'networkidle', timeout: 120000 });
  await page.locator('#place-list li').first().waitFor({ timeout: 120000 });
  await checkpoint('boot');
  await waitApi('/api/spatial/explorer/places', () => page.locator('#place-list li button').filter({ hasText: 'Children' }).first().click());
  await checkpoint('children');
  if (await page.locator('#place-more').isVisible()) {
    await waitApi('/api/spatial/explorer/places', () => page.locator('#place-more').click());
    await checkpoint('childrenPage2');
    await waitApi('/api/spatial/explorer/places', () => page.locator('#place-roots').click());
    await waitApi('/api/spatial/explorer/places', () => page.locator('#place-list li button').filter({ hasText: 'Children' }).first().click());
  }
  await waitApi('/api/spatial/explorer/routes', () => page.locator('#place-list li button').filter({ hasText: 'Inspect' }).first().click());
  await checkpoint('inspect');
  await page.locator('#place-query').fill('Scale place 000001');
  await waitApi('/api/spatial/explorer/places', () => page.locator('#place-search button[type=submit]').click());
  await checkpoint('search');
  await waitApi('/api/spatial/explorer/routes', () => page.locator('#route-outgoing').click());
  await checkpoint('routesOutgoing');
  await waitApi('/api/spatial/explorer/routes', () => page.locator('#route-incoming').click());
  await checkpoint('routesIncoming');
  await page.locator('#map-select').selectOption('map:scale-0');
  for (const [id, value] of Object.entries({ 'min-x': '-50000', 'min-y': '-50000', 'max-x': '-49990', 'max-y': '-49900' })) await page.locator(`#${id}`).fill(value);
  await waitApi('/api/spatial/explorer/viewport', () => page.locator('#viewport-form button[type=submit]').click());
  await checkpoint('viewport');
  for (const [id, value] of Object.entries({ 'story-timeline': 'main', 'story-tick': '0', 'story-order': '0' })) await page.locator(`#${id}`).fill(value);
  await waitApi('/api/spatial/explorer/layers', () => page.locator('#layer-form button[type=submit]').click());
  await checkpoint('layers');
  await page.locator('#path-target').fill('location:scale-2');
  await waitApi('/api/spatial/path', () => page.locator('#path-submit').click());
  await checkpoint('path');
  await page.setViewportSize({ width: 360, height: 740 });
  await page.emulateMedia({ reducedMotion: 'reduce', forcedColors: 'active' });
  checkpoints.narrow = await page.evaluate(() => ({ width: innerWidth, scrollWidth: document.documentElement.scrollWidth, focusedTag: document.activeElement?.tagName }));
  await page.keyboard.press('Tab');
  checkpoints.keyboard = await page.evaluate(() => ({ focusedTag: document.activeElement?.tagName, focusedId: document.activeElement?.id, outline: getComputedStyle(document.activeElement).outlineStyle }));
  if (process.argv.includes('--offline')) {
    await context.setOffline(true);
    await page.locator('#refresh-catalog').click();
    await page.waitForFunction(() => document.querySelector('#spatial-status')?.classList.contains('error'), null, { timeout: 15000 });
    checkpoints.offlineFallback = await page.evaluate(() => ({
      status: document.querySelector('#spatial-status')?.textContent,
      errorVisible: document.querySelector('#spatial-status')?.classList.contains('error'),
      partialPlaceCards: document.querySelectorAll('#place-list li').length,
      compendiumLink: document.querySelector('nav a')?.getAttribute('href'),
      mountedElements: document.querySelectorAll('*').length,
    }));
    await context.setOffline(false);
  }
  if (process.argv.includes('--lore')) {
    spatialRequestCount = requests.length;
    const lore = page.locator('#place-list a').filter({ hasText: 'Read lore' }).first();
    const href = await lore.getAttribute('href');
    await Promise.all([page.waitForURL(target => new URL(target).pathname === '/', { timeout: 120000 }), lore.click()]);
    await page.waitForFunction(() => !sessionStorage.getItem('wedl.spatial.lore.once'), null, { timeout: 120000 });
    checkpoints.loreHandoff = { href, destinationPath: new URL(page.url()).pathname,
      tokenCleared: await page.evaluate(() => !sessionStorage.getItem('wedl.spatial.lore.once')) };
  }
} catch (error) {
  errors.push(`flow: ${error.stack || error}`);
} finally {
  await Promise.allSettled(responseReads);
  await browser.close();
}
const origin = new URL(url).origin;
const externalDestinations = [...new Set(requests.map(r => new URL(r.url).origin).filter(x => x !== origin))];
spatialRequestCount ??= requests.length;
const artifact = {
  url, startedUtc, flowElapsedMs: Math.round((performance.now() - started) * 1000) / 1000,
  browserVersion: browser.version(), checkpoints, spatialRequestCount, requests, responses,
  requestBodyBytes: requests.reduce((sum, r) => sum + r.bodyBytes, 0),
  responseBodyBytes: responses.reduce((sum, r) => sum + r.bodyBytes, 0),
  externalDestinations, errors,
  requestSha256: createHash('sha256').update(JSON.stringify(requests)).digest('hex'),
  limitations: ['Playwright request body bytes exclude URL and HTTP headers', 'MutationObserver peak starts at DOMContentLoaded'],
};
writeFileSync(output, JSON.stringify(artifact, null, 2) + '\n');
console.log(JSON.stringify({ output, checkpoints: Object.keys(checkpoints), requests: requests.length, externalDestinations, errors: errors.length }));
if (errors.length || externalDestinations.length || requests.slice(0, spatialRequestCount).some(r => new URL(r.url).pathname === '/api/entities')) process.exitCode = 1;
"""




def runtime_preflight(context):
    runtime = context["browserRuntime"]
    for key in ("node", "browser", "playwrightPackage", "playwrightEntry"):
        path = Path(runtime[key]["path"])
        require(path.is_absolute() and path.is_file(), "missing browser runtime: " + key)
        require(file_hash(path) == sha(runtime[key]["sha256"]), "browser runtime hash changed: " + key)
    package = load_json(runtime["playwrightPackage"]["path"])
    require(package["name"] == "playwright-core" and package["version"] == runtime["playwrightVersion"],
            "playwright version changed")
    require(os.name == "nt", "browser stage requires reviewed Windows Job cleanup")
    for relative, expected in runtime["libraryFiles"].items():
        library = Path(runtime["playwrightPackage"]["path"]).parent
        require(file_hash(contained(library / relative, library)) == sha(expected), "playwright library changed")
    return runtime


def prepare_code(context):
    import io
    import tarfile
    target = contained(context["codeRoot"], context["scratch"], exists=False)
    require(not target.exists(), "code archive destination exists")
    archive = subprocess.run(["git", "-C", str(ROOT), "archive", "--format=tar", context["head"], "src/wedl"],
                             check=True, capture_output=True, timeout=20).stdout
    require(git(ROOT, "rev-parse", "HEAD") == context["head"], "HEAD moved while archiving")
    target.mkdir()
    inventory = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        for member in tar:
            within = ((member.name in ("src", "src/", "src/wedl", "src/wedl/") and member.isdir())
                      or member.name.startswith("src/wedl/"))
            require(within and not member.issym() and not member.islnk()
                    and (member.isdir() or member.isfile()), "unsafe code archive entry")
            path = contained(target / member.name, target, exists=False)
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True)
            else:
                stream = tar.extractfile(member)
                require(stream is not None, "archive file missing")
                data = stream.read()
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
                inventory[member.name] = file_hash(path)
    require(not (target / ".git").exists(), "code archive contains Git")
    for asset in ("spatial.html", "spatial_app.mjs", "spatial_api.mjs", "spatial_renderer.mjs",
                  "spatial.css", "favicon.svg", "index.html", "app.js"):
        require("src/wedl/static/" + asset in inventory, "missing archived browser asset")
    proof = {"head": context["head"], "archiveSha256": hashlib.sha256(archive).hexdigest(),
             "codeRoot": str(target), "files": inventory}
    (target.parent / "code-archive.json").write_bytes(canonical(proof) + b"\n")
    return proof


class WindowsJob:
    """Assign suspended children before execution; retain exact process handles."""
    def __init__(self):
        require(os.name == "nt", "Windows Job required")
        from ctypes import wintypes as w
        self.k = ctypes.WinDLL("kernel32", use_last_error=True)
        self.handles = []
        self.children = []
        self.closed = False

        class IO(ctypes.Structure):
            _fields_ = [(name, ctypes.c_ulonglong) for name in
                        ("ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
                         "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]
        class Limits(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_longlong), ("PerJobUserTimeLimit", ctypes.c_longlong),
                        ("LimitFlags", w.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                        ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", w.DWORD),
                        ("Affinity", ctypes.c_size_t), ("PriorityClass", w.DWORD), ("SchedulingClass", w.DWORD)]
        class Extended(ctypes.Structure):
            _fields_ = [("BasicLimitInformation", Limits), ("IoInfo", IO),
                        ("ProcessMemoryLimit", ctypes.c_size_t), ("JobMemoryLimit", ctypes.c_size_t),
                        ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]
        class Accounting(ctypes.Structure):
            _fields_ = [("TotalUserTime", ctypes.c_longlong), ("TotalKernelTime", ctypes.c_longlong),
                        ("ThisPeriodTotalUserTime", ctypes.c_longlong), ("ThisPeriodTotalKernelTime", ctypes.c_longlong),
                        ("TotalPageFaultCount", w.DWORD), ("TotalProcesses", w.DWORD),
                        ("ActiveProcesses", w.DWORD), ("TotalTerminatedProcesses", w.DWORD)]
        class Startup(ctypes.Structure):
            _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                        ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                        ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                        ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD),
                        ("wShowWindow", w.WORD), ("cbReserved2", w.WORD), ("lpReserved2", ctypes.c_void_p),
                        ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]
        class StartupEx(ctypes.Structure):
            _fields_ = [("StartupInfo", Startup), ("lpAttributeList", ctypes.c_void_p)]
        class ProcessInfo(ctypes.Structure):
            _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE),
                        ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]
        self.Accounting, self.StartupEx, self.ProcessInfo = Accounting, StartupEx, ProcessInfo
        prototypes = {
            "CreateJobObjectW": ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
            "SetInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
            "QueryInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
            "AssignProcessToJobObject": ([w.HANDLE, w.HANDLE], w.BOOL),
            "CreateProcessW": ([w.LPCWSTR, w.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, w.BOOL, w.DWORD,
                                ctypes.c_void_p, w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
            "InitializeProcThreadAttributeList": ([ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.POINTER(ctypes.c_size_t)], w.BOOL),
            "UpdateProcThreadAttribute": ([ctypes.c_void_p, w.DWORD, ctypes.c_size_t, ctypes.c_void_p,
                                           ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
            "DeleteProcThreadAttributeList": ([ctypes.c_void_p], None),
            "ResumeThread": ([w.HANDLE], w.DWORD),
            "WaitForSingleObject": ([w.HANDLE, w.DWORD], w.DWORD),
            "GetExitCodeProcess": ([w.HANDLE, ctypes.POINTER(w.DWORD)], w.BOOL),
            "TerminateProcess": ([w.HANDLE, w.UINT], w.BOOL),
            "TerminateJobObject": ([w.HANDLE, w.UINT], w.BOOL),
            "CloseHandle": ([w.HANDLE], w.BOOL),
        }
        for name, (args, result) in prototypes.items():
            getattr(self.k, name).argtypes = args
            getattr(self.k, name).restype = result
        self.job = self.k.CreateJobObjectW(None, None)
        require(bool(self.job), "CreateJobObject failed")
        self.handles.append(self.job)
        limit = Extended()
        limit.BasicLimitInformation.LimitFlags = 0x2000 | 0x8  # kill on close, active process cap
        limit.BasicLimitInformation.ActiveProcessLimit = 32
        try:
            self.check(self.k.SetInformationJobObject(self.job, 9, ctypes.byref(limit), ctypes.sizeof(limit)))
        except BaseException:
            self.close()
            raise

    def check(self, result):
        if not result:
            raise ctypes.WinError(ctypes.get_last_error())

    def launch(self, arguments, directory, environment, log):
        import msvcrt
        require(len(self.children) < 2, "new child bound exceeded")
        stdin = open(os.devnull, "rb")
        stdout = open(log.with_suffix(".stdout.txt"), "xb")
        stderr = open(log.with_suffix(".stderr.txt"), "xb")
        streams = [stdin, stdout, stderr]
        attributes = None
        initialized = False
        process = self.ProcessInfo()
        try:
            handles = (wintypes.HANDLE * 3)(*[msvcrt.get_osfhandle(s.fileno()) for s in streams])
            for stream in streams:
                os.set_handle_inheritable(msvcrt.get_osfhandle(stream.fileno()), True)
            size = ctypes.c_size_t()
            self.k.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
            attributes = ctypes.create_string_buffer(size.value)
            self.check(self.k.InitializeProcThreadAttributeList(attributes, 1, 0, ctypes.byref(size)))
            initialized = True
            self.check(self.k.UpdateProcThreadAttribute(attributes, 0, 0x20002, handles,
                                                       ctypes.sizeof(handles), None, None))
            startup = self.StartupEx()
            startup.StartupInfo.cb = ctypes.sizeof(startup)
            startup.StartupInfo.dwFlags = 0x100  # STARTF_USESTDHANDLES
            startup.StartupInfo.hStdInput, startup.StartupInfo.hStdOutput, startup.StartupInfo.hStdError = handles
            startup.lpAttributeList = ctypes.cast(attributes, ctypes.c_void_p)
            block = ctypes.create_unicode_buffer("\0".join(k + "=" + v for k, v in sorted(environment.items())) + "\0\0")
            command = ctypes.create_unicode_buffer(subprocess.list2cmdline([str(a) for a in arguments]))
            self.check(self.k.CreateProcessW(str(arguments[0]), command, None, None, True,
                                            0x4 | 0x400 | 0x80000 | 0x8000000, block, str(directory),
                                            ctypes.byref(startup), ctypes.byref(process)))
            self.handles.extend((process.hProcess, process.hThread))
            try:
                self.check(self.k.AssignProcessToJobObject(self.job, process.hProcess))
                require(self.k.ResumeThread(process.hThread) != 0xffffffff, "ResumeThread failed")
                self.children.append(process)
            except BaseException:
                self.check(self.k.TerminateProcess(process.hProcess, 1))
                require(self.k.WaitForSingleObject(process.hProcess, 5000) == 0, "unassigned child closure")
                raise
            return process
        finally:
            if initialized:
                self.k.DeleteProcThreadAttributeList(attributes)
            for stream in streams:
                os.set_handle_inheritable(msvcrt.get_osfhandle(stream.fileno()), False)
                stream.close()

    def exit_code(self, process):
        if self.k.WaitForSingleObject(process.hProcess, 0) != 0:
            return None
        code = wintypes.DWORD()
        self.check(self.k.GetExitCodeProcess(process.hProcess, ctypes.byref(code)))
        return code.value

    def accounting(self):
        value = self.Accounting()
        self.check(self.k.QueryInformationJobObject(self.job, 1, ctypes.byref(value), ctypes.sizeof(value), None))
        require(value.TotalProcesses <= 64, "browser cumulative process bound exceeded")
        return value.ActiveProcesses, value.TotalProcesses

    def empty(self, deadline):
        samples = []
        while time.monotonic() < deadline:
            active, total = self.accounting()
            if active == 0:
                now = time.monotonic()
                samples.append(now)
                if len(samples) == 2:
                    return {"jobEmptySamples": [0, 0], "sampleGapSeconds": samples[1] - samples[0],
                            "totalProcesses": total}
                time.sleep(.11)
            else:
                samples.clear()
                time.sleep(.05)
        raise TimeoutError("owned browser Job did not become empty")

    def close(self):
        errors = []
        for handle in reversed(self.handles):
            if not self.k.CloseHandle(handle):
                errors.append(ctypes.get_last_error())
        self.handles.clear()
        self.closed = not errors
        require(self.closed, "owned kernel handle close failed")


def browser_stage(context, timeout):
    runtime = runtime_preflight(context)
    source, binding = source_binding(context)
    code_root = contained(context["codeRoot"], context["scratch"])
    require((code_root / "src/wedl").is_dir() and not (code_root / ".git").exists(), "code archive missing")
    output = Path(context["results"]["actual-100k-browser"]).parent / "browser"
    contained(output, context["scratch"], exists=False).mkdir()
    helper = output / "probe.mjs"
    helper.write_text(BROWSER_PROBE, encoding="utf-8")
    raw_path = output / "raw.json"
    with socket.socket() as reserved:
        reserved.bind(("127.0.0.1", 0))
        port = reserved.getsockname()[1]
    url = f"http://127.0.0.1:{port}/assets/spatial.html"
    environment = dict(os.environ, WEDL_CODE_ROOT=str(code_root),
                       PYTHONPATH=str(code_root / "src") + os.pathsep + str(ROOT / "tools"),
                       PYTHONDONTWRITEBYTECODE="1",
                       PLAYWRIGHT_CORE_PATH=str(Path(runtime["playwrightPackage"]["path"]).parent),
                       CHROME_PATH=runtime["browser"]["path"])
    deadline = time.monotonic() + timeout
    work_deadline = deadline - 15
    job = WindowsJob()
    cleanup = None
    try:
        server = job.launch([sys.executable, "-m", "wedl.cli", "--compact", "serve", "--repo",
                             binding["sourceFixturePath"], "--port", str(port)], ROOT, environment, output / "server")
        ready = False
        while time.monotonic() < work_deadline:
            require(job.exit_code(server) is None, "server exited before browser")
            try:
                with urlopen(url, timeout=min(2, max(.1, work_deadline - time.monotonic()))) as response:
                    ready = response.status == 200
            except OSError:
                pass
            if ready:
                break
            time.sleep(.1)
        require(ready, "server readiness deadline")
        probe = job.launch([runtime["node"]["path"], helper, url, raw_path, "--offline"],
                           ROOT, environment, output / "probe")
        while job.exit_code(probe) is None and time.monotonic() < work_deadline:
            job.accounting()
            time.sleep(.1)
        require(job.exit_code(probe) == 0, "browser failed or deadline reached")
        raw = load_json(raw_path)
        validate_raw_browser(raw)
        # Uvicorn is a persistent owned server. Stop its retained handle after the probe closes Chrome.
        job.check(job.k.TerminateProcess(server.hProcess, 0))
        require(job.k.WaitForSingleObject(server.hProcess, 5000) == 0, "owned server did not stop")
        cleanup = job.empty(deadline)
        cleanup.update(probeExitCode=0, serverStopped=True)
    except BaseException:
        job.k.TerminateJobObject(job.job, 1)
        try:
            job.empty(deadline)
        finally:
            job.close()
        raise
    else:
        job.close()
    cleanup["handlesClosed"] = job.closed
    runtime_preflight(context)
    require(source_binding(context)[1] == binding, "fixture changed during browser stage")
    proof = {"kind": "actual-browser", **binding, "places": source["counts"]["places"],
             "checkpoints": {key: True for key in ("boot", "hierarchy", "search", "viewport", "layers", "routes", "path", "offline")},
             "domCount": max(row.get("peakMountedElements", 0) for row in raw["checkpoints"].values()),
             "requestCount": len(raw["requests"]), "networkDestinations": raw["externalDestinations"],
             "browserVersion": raw["browserVersion"], "runtime": runtime, "cleanup": cleanup,
             "rawProbePath": str(raw_path), "rawProbeSha256": file_hash(raw_path),
             "probeCodeSha256": file_hash(helper),
             "offlineScope": "loaded-page refresh failure and local compendium link"}
    write_manifest(Path(context["results"]["actual-100k-browser"]), proof, context)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--context", type=Path, required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--preflight", action="store_true")
    modes.add_argument("--prepare-code", action="store_true")
    modes.add_argument("--validate-manifest", type=Path)
    modes.add_argument("--seal-generational", type=Path)
    parser.add_argument("--stage", choices=STAGES)
    parser.add_argument("--outcomes", type=Path)
    parser.add_argument("--timeout", type=float, default=240)
    args = parser.parse_args()
    context = read_context(args.context)
    if args.preflight:
        runtime_preflight(context)
        print("Browser runtime and four manifest contracts verified; no benchmark executed.")
    elif args.prepare_code:
        prepare_code(context)
    elif args.validate_manifest:
        require(args.stage is not None, "stage required")
        outcomes = load_json(contained(args.outcomes, context["scratch"])) if args.outcomes else None
        validate_manifest(args.validate_manifest, args.stage, context, outcomes)
    elif args.seal_generational:
        raw = contained(args.seal_generational, context["scratch"])
        value = load_json(raw)
        validate_generational(value)
        write_manifest(Path(context["results"]["generational-5k-10k"]),
                       {**value, "rawResultPath": str(raw), "rawResultSha256": file_hash(raw)}, context)
    else:
        require(math.isfinite(args.timeout) and 20 <= args.timeout <= 1800, "browser deadline range")
        browser_stage(context, args.timeout)


if __name__ == "__main__":
    main()

