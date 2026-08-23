from __future__ import annotations

import json

from wedl.cli import main, parser
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN


def test_cli_status_reports_parent_name_and_default_repository_equivalently(ash_repo, capsys, monkeypatch) -> None:
    monkeypatch.chdir(ash_repo.root.parent)
    assert main(["status", "--repo", ash_repo.root.name]) == 0
    parent_output = json.loads(capsys.readouterr().out)

    monkeypatch.chdir(ash_repo.root)
    assert main(["status"]) == 0
    default_output = json.loads(capsys.readouterr().out)

    assert parent_output["repositoryRoot"] == str(ash_repo.root.resolve())
    assert default_output["repositoryRoot"] == parent_output["repositoryRoot"]
    assert "repository" not in default_output


def test_cli_status_exposes_additive_unitless_time_model(ash_repo, capsys) -> None:
    assert main(["status", "--repo", str(ash_repo.root)]) == 0

    output = json.loads(capsys.readouterr().out)
    assert output["timeModel"] == {
        "coordinate": ["timeline", "tick", "order"],
        "tick": {"semantics": "unitless-ordinal", "minimum": TICK_MIN, "maximum": TICK_MAX},
        "order": {"semantics": "same-tick-order", "minimum": ORDER_MIN, "maximum": ORDER_MAX},
        "durationSemantics": "none",
        "intervalEndpoints": "inclusive",
        "origin": {"semantics": "descriptive", "setsLowerBound": False},
        "defaultTimeline": "main",
        "timelineDeclarations": [{"id": "main", "label": "Main chronology"}],
    }


def test_relative_repo_name_from_inside_repository_has_actionable_diagnosis(ash_repo, capsys, monkeypatch) -> None:
    monkeypatch.chdir(ash_repo.root)

    assert main(["status", "--repo", ash_repo.root.name]) == 2

    error = json.loads(capsys.readouterr().err)
    assert error["code"] == "repository_error"
    assert error["details"] == {
        "requested": ash_repo.root.name,
        "resolved": str((ash_repo.root / ash_repo.root.name).resolve()),
        "cwd": str(ash_repo.root.resolve()),
        "hint": f"Use --repo . (or omit --repo), or pass the absolute repository path {ash_repo.root.resolve()}.",
    }


def test_generic_repository_error_includes_path_diagnosis(ash_repo, capsys, monkeypatch) -> None:
    monkeypatch.chdir(ash_repo.root)

    assert main(["status", "--repo", "missing-world"]) == 2

    error = json.loads(capsys.readouterr().err)
    assert error["details"] == {
        "requested": "missing-world",
        "resolved": str((ash_repo.root / "missing-world").resolve()),
        "cwd": str(ash_repo.root.resolve()),
        "hint": "Use --repo . from a repository directory, or pass an absolute repository root.",
    }


def test_non_status_cli_output_schema_is_unchanged(ash_repo, capsys) -> None:
    assert main(["validate", "--repo", str(ash_repo.root)]) == 0

    output = json.loads(capsys.readouterr().out)
    assert set(output) == {"valid", "revision", "recordCount", "diagnostics"}


def test_cli_require_compiled_reports_structured_no_rebuild_diagnostic(ash_repo, capsys) -> None:
    root = str(ash_repo.root)
    cache = ash_repo.root / ".wedl"
    assert main(["entity", "list", "--repo", root, "--require-compiled"]) == 2
    diagnostic = json.loads(capsys.readouterr().err)
    assert diagnostic["code"] == "compile_required"
    assert diagnostic["details"]["cache"]["state"] == "missing"
    assert not cache.exists()

    assert main(["entity", "list", "--repo", root]) == 0
    assert cache.exists()


def test_cli_author_time_scope_versions_negative_ticks_and_conflicts(ash_repo, capsys) -> None:
    root = str(ash_repo.root)
    assert main(["search", "flood", "--repo", root, "--mode", "fts", "--tick", "-1"]) == 0
    bounded = json.loads(capsys.readouterr().out)
    assert bounded["protocol"] == "wedl-search/v5"
    assert bounded["timeScope"] == {"mode": "as-of", "at": {"timeline": "main", "tick": -1, "order": 2147483647}}

    assert main(["search", "flood", "--repo", root, "--mode", "fts", "--all-time"]) == 0
    assert json.loads(capsys.readouterr().out)["timeScope"] == {"mode": "all-time"}
    assert main(["search", "flood", "--repo", root, "--all-time", "--tick", "-1"]) == 2
    assert "cannot be combined" in json.loads(capsys.readouterr().err)["message"]
    assert main(["search", "flood", "--repo", root, "--perspective", "character", "--all-time"]) == 2
    assert "only for author" in json.loads(capsys.readouterr().err)["message"]


def test_cli_whereabouts_uses_a_name_filter_and_exact_transport(ash_repo, capsys) -> None:
    assert main(["whereabouts", "--repo", str(ash_repo.root), "--character", "Mara Vale", "--tick", "121", "--timeline", "main", "--order", "0"]) == 0

    output = json.loads(capsys.readouterr().out)
    assert output["protocol"] == "wedl-whereabouts/v1"
    assert output["characterFilter"]["title"] == "Mara Vale"
    assert output["effectiveTime"] == {"timeline": "main", "tick": "121", "order": "0"}
    assert len(output["characters"]) == 1


def test_top_level_help_includes_end_to_end_and_temporal_examples() -> None:
    help_text = parser().format_help()

    assert "wedl init frontiersmen --example frontiersmen" in help_text
    assert "wedl validate --repo frontiersmen" in help_text
    assert "wedl compile --repo frontiersmen --profile hybrid --vector-provider lsa" in help_text
    assert "wedl state Rhea --repo frontiersmen --tick 195" in help_text
    assert "Entity arguments accept IDs, titles, aliases, and slugs" in help_text
    assert "--timeline/--order" in help_text
    assert "signed ordinal ticks (negative values are valid)" in help_text
    assert "ticks do not convert to elapsed duration" in help_text
    assert "show repository and compilation status" in help_text
    command_action = next(action for action in parser()._actions if action.dest == "command")
    status_help = command_action.choices["status"].format_help()
    status_help_normalized = " ".join(status_help.split())
    assert "relative paths resolve from the current" in status_help
    assert "working directory (default: .)" in status_help
    assert "origins are descriptive, not lower bounds" in status_help_normalized
    assert "bounded intervals are inclusive" in status_help_normalized
