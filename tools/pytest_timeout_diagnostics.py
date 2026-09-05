"""Small pytest plugin used by the bounded Windows test runner.

The runner reads the JSON snapshot if it must stop pytest at its deadline.
Keeping this outside the test suite avoids making diagnostics a product
dependency while still identifying the test that was active at the deadline.
"""

from __future__ import annotations

import json
import os
from pathlib import Path


_snapshot_path = Path(os.environ["WEDL_TEST_DIAGNOSTICS"])
_active_node: str | None = None
_completed: list[dict[str, object]] = []
_durations: dict[str, float] = {}
_outcomes: dict[str, list[str]] = {}


def _write_snapshot() -> None:
    payload = {"active": _active_node, "completed": _completed[:100]}
    temporary = _snapshot_path.with_suffix(".tmp")
    temporary.write_text(json.dumps(payload), encoding="utf-8")
    temporary.replace(_snapshot_path)


def pytest_runtest_logstart(nodeid: str, location: tuple[str, int | None, str]) -> None:
    del location
    global _active_node
    _active_node = nodeid
    _durations[nodeid] = 0.0
    _outcomes[nodeid] = []
    _write_snapshot()


def pytest_runtest_logreport(report) -> None:
    if report.when not in {"setup", "call", "teardown"}:
        return
    _durations[report.nodeid] = _durations.get(report.nodeid, 0.0) + report.duration
    _outcomes.setdefault(report.nodeid, []).append(report.outcome)
    _write_snapshot()


def pytest_runtest_logfinish(
    nodeid: str, location: tuple[str, int | None, str]
) -> None:
    del location
    global _active_node
    outcomes = _outcomes.pop(nodeid, [])
    if "failed" in outcomes:
        outcome = "failed"
    elif "skipped" in outcomes:
        outcome = "skipped"
    else:
        outcome = outcomes[-1] if outcomes else "unknown"
    _completed.append(
        {
            "nodeid": nodeid,
            "duration": _durations.pop(nodeid, 0.0),
            "outcome": outcome,
        }
    )
    _completed.sort(key=lambda item: float(item["duration"]), reverse=True)
    if _active_node == nodeid:
        _active_node = None
    _write_snapshot()
