"""Benchmark the production live author server built with snapshot-baseline."""
import argparse
from collections import deque
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import ctypes
from functools import partial
import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import platform
from queue import Empty, Queue
import re
import subprocess
from tempfile import TemporaryDirectory
from threading import Thread
from time import perf_counter
from urllib.parse import urlencode


TIERS = {"small": (64, 512), "medium": (1024, 2048), "near-limit": (4096, 7936)}
TARGET_ID = "obj_" + "0" * 25 + "1"
WORLD = b"""---
schema: wedl/v1
kind: world
id: world_00000000000000000000000000
title: Snapshot baseline
domain: world
status: canonical
capabilities: []
tags: []
aliases: []
default_timeline: main
timelines: [{id: main, label: Main}]
threads: []
current_time: {timeline: main, tick: 0, order: 0}
state_keys: {}
relationship_metrics: {}
section_audiences: {public: [public]}
---

# Snapshot baseline
"""
WORDS = b"weathered brass compass shoreline ledger harbor chalk quiet stone sky "
METRIC_PREFIX = "snapshot_baseline "
TIMEOUT = 90


def make_world(root, records, record_bytes):
    objects = root / "story" / "objects"
    objects.mkdir(parents=True)
    (root / "story" / "world.md").write_bytes(WORLD)
    source_bytes = len(WORLD)
    for number in range(1, records):
        header = (
            f"---\nschema: wedl/v1\nkind: object\nid: obj_{number:026d}\n"
            f"title: Benchmark object {number}\ndomain: baseline\nstatus: canonical\n"
            "aliases: []\ntags: []\n---\n\n"
        ).encode()
        lead = b"signalbeacon " if number == 1 else b"inventory "
        padding = record_bytes - len(header) - len(lead) - 1
        body = lead + (WORDS * math.ceil(padding / len(WORDS)))[:padding] + b"\n"
        path = objects / f"object-{number:04d}.md"
        path.write_bytes(header + body)
        source_bytes += len(header) + len(body)
    return source_bytes, objects / "object-0001.md"


def receive(queue, recent, description):
    try:
        return queue.get(timeout=TIMEOUT)
    except Empty as error:
        raise RuntimeError(f"Timed out waiting for {description}: {list(recent)}") from error


@contextmanager
def server(binary, root):
    started = perf_counter()
    process = subprocess.Popen(
        [str(binary), "--repo", str(root), "serve", "--host", "127.0.0.1", "--port", "0"],
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        encoding="utf-8", errors="replace",
    )
    ready = Queue()
    counters = Queue()
    recent = deque(maxlen=8)

    def drain():
        with process.stderr:
            for line in process.stderr:
                line = line.strip()
                address = re.match(
                    r"Ready: http://127\.0\.0\.1:(\d+) \u2014 live local-author worktree ([0-9a-f]{64}) ",
                    line,
                )
                if address:
                    ready.put((int(address[1]), address[2], (perf_counter() - started) * 1000))
                elif line.startswith(METRIC_PREFIX):
                    counters.put(json.loads(line[len(METRIC_PREFIX):]))
                else:
                    recent.append(line)
        ready.put(None)
        counters.put(None)

    reader = Thread(target=drain, daemon=True)
    reader.start()
    try:
        result = receive(ready, recent, "server readiness")
        if result is None:
            raise RuntimeError(f"Server exited before readiness: {list(recent)}")
        port, startup_pin, startup_ms = result
        yield process.pid, port, startup_pin, startup_ms, counters, recent
    finally:
        if process.poll() is None:
            process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)
        reader.join(timeout=10)


def request(port, route, **params):
    started = perf_counter()
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=TIMEOUT)
    try:
        connection.request("GET", f"/api/{route}?{urlencode(params)}")
        response = connection.getresponse()
        body = response.read()
        elapsed_ms = (perf_counter() - started) * 1000
        if response.status != 200:
            raise RuntimeError(f"{route}: HTTP {response.status}: {body[:2000]!r}")
        return elapsed_ms, json.loads(body)
    finally:
        connection.close()


def verify_entity(body, expected):
    result = body["result"]
    if (
        body["revision"] is not None
        or result["id"] != TARGET_ID
        or result["path"] != "story/objects/object-0001.md"
        or not result["bodyMarkdown"].lstrip().startswith(expected + " ")
    ):
        raise RuntimeError("Unexpected entity result or stale worktree snapshot")


def verify_search(body):
    items = body["result"]["matches"]["items"]
    if body["revision"] is not None or [item["id"] for item in items] != [TARGET_ID]:
        raise RuntimeError("Unexpected lexical result or stale worktree snapshot")


def percentile(values, percent):
    return sorted(values)[math.ceil(len(values) * percent / 100) - 1]


def measure(fetch, check, samples, workers, counters, recent):
    started = perf_counter()
    if workers == 1:
        results = [fetch() for _ in range(samples)]
    else:
        with ThreadPoolExecutor(max_workers=workers) as pool:
            results = list(pool.map(lambda _: fetch(), range(samples)))
    elapsed = perf_counter() - started
    for _, body in results:
        check(body)
    work = [receive(counters, recent, "snapshot counter receipt") for _ in results]
    if any(item is None for item in work):
        raise RuntimeError(f"Server exited before recording counters: {list(recent)}")
    times = [time for time, _ in results]
    gates = [item["gate_wait_us"] / 1000 for item in work]
    execution = [item["execution_us"] / 1000 for item in work]
    return results[-1][1]["fingerprint"], {
        "samples": samples,
        "workers": workers,
        "latency_p50_ms": round(percentile(times, 50), 3),
        "latency_p95_ms": round(percentile(times, 95), 3),
        "gate_wait_p95_ms": round(percentile(gates, 95), 3),
        "execution_p95_ms": round(percentile(execution, 95), 3),
        "requests_per_second": round(samples / elapsed, 3),
        "work_per_request": {
            name: [min(item[name] for item in work), max(item[name] for item in work)]
            for name in work[0] if name not in {"gate_wait_us", "execution_us"}
        },
    }


def peak_rss(pid):
    if os.name == "nt":
        from ctypes import wintypes

        class ProcessMemoryCounters(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("PageFaultCount", wintypes.DWORD),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
        kernel.K32GetProcessMemoryInfo.argtypes = (
            wintypes.HANDLE, ctypes.POINTER(ProcessMemoryCounters), wintypes.DWORD,
        )
        handle = kernel.OpenProcess(0x1000, False, pid)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            counts = ProcessMemoryCounters()
            size = ctypes.sizeof(counts)
            counts.cb = size
            if not kernel.K32GetProcessMemoryInfo(handle, ctypes.byref(counts), size):
                raise ctypes.WinError(ctypes.get_last_error())
            return counts.PeakWorkingSetSize
        finally:
            kernel.CloseHandle(handle)
    proc_status = Path(f"/proc/{pid}/status")
    if proc_status.is_file():
        for line in proc_status.read_text().splitlines():
            if line.startswith("VmHWM:"):
                return int(line.split()[1]) * 1024
    return None


def run_tier(binary, root, records, record_bytes, samples, workers):
    source_bytes, edit_path = make_world(root, records, record_bytes)
    cache = root / ".wedl-rust" / "index.sqlite3"
    with server(binary, root) as (pid, port, startup_pin, startup, counters, recent):
        entity = partial(request, port, "entity", id=TARGET_ID)
        search = partial(request, port, "search", q="signalbeacon", kind="object", limit=5)
        before = partial(verify_entity, expected="signalbeacon")

        def batch(fetch, check, count=1, parallel=1):
            return measure(fetch, check, count, parallel, counters, recent)

        first_pin, first_entity = batch(entity, before)
        if first_pin != startup_pin:
            raise RuntimeError("Entity fingerprint differs from the startup source fingerprint")
        _, warm_entity = batch(entity, before, samples)
        _, parallel_entity = batch(entity, before, samples, workers)
        if cache.exists():
            raise RuntimeError("Expected no lexical index before the first search")
        _, cold_fts = batch(search, verify_search)
        _, warm_fts = batch(search, verify_search, samples)
        cache_bytes = cache.stat().st_size

        # Same byte length and modification time, without Git or a server restart.
        old = edit_path.read_bytes()
        stamp = edit_path.stat()
        replacement = old.replace(b"signalbeacon ", b"signalrecast ", 1)
        if replacement == old or len(replacement) != len(old):
            raise RuntimeError("Expected a same-length source edit")
        temporary = edit_path.with_suffix(".tmp")
        temporary.write_bytes(replacement)
        os.utime(temporary, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        temporary.replace(edit_path)
        if edit_path.stat().st_mtime_ns != stamp.st_mtime_ns:
            raise RuntimeError("File system did not preserve the edit modification time")
        changed_pin, edited_entity = batch(entity, partial(verify_entity, expected="signalrecast"))
        if changed_pin == first_pin:
            raise RuntimeError("Same-size, same-mtime edit did not advance the fingerprint")
        search = partial(request, port, "search", q="signalrecast", kind="object", limit=5)
        search_pin, edited_fts = batch(search, verify_search)
        if search_pin != changed_pin:
            raise RuntimeError("Entity and search disagree about the edited source")
        return {
            "records": records,
            "source_bytes": source_bytes,
            "launch_to_ready_ms": round(startup, 3),
            "peak_server_rss_bytes": peak_rss(pid),
            "fts_index_bytes_before_edit": cache_bytes,
            "fts_index_bytes_after_edit": cache.stat().st_size,
            "first_entity": first_entity,
            "warm_entity": warm_entity,
            "parallel_entity": parallel_entity,
            "cold_fts": cold_fts,
            "warm_fts": warm_fts,
            "edited_entity": edited_entity,
            "edited_fts": edited_fts,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--scratch", required=True, type=Path, help="Existing scratch directory")
    parser.add_argument("--tier", choices=TIERS, action="append", help="Default is all tiers")
    parser.add_argument("--samples", type=int, default=12)
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.workers <= args.samples <= 100:
        parser.error("Require 1 <= workers <= samples <= 100")
    binary = args.binary.resolve(strict=True)
    with binary.open("rb") as file:
        binary_sha256 = hashlib.file_digest(file, "sha256").hexdigest()
    report = {
        "system": platform.platform(),
        "processor": platform.processor(),
        "logical_cpus": os.cpu_count(),
        "binary_sha256": binary_sha256,
        "tiers": {},
    }
    for name in args.tier or TIERS:
        with TemporaryDirectory(prefix=f"snapshot-{name}-", dir=args.scratch) as scratch:
            report["tiers"][name] = run_tier(
                binary, Path(scratch), *TIERS[name], args.samples, args.workers,
            )
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

