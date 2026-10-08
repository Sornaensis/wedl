from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from importlib import resources
from pathlib import Path
from typing import Any

from . import __version__
from .changeset import (
    apply as apply_changeset,
    preview as preview_changeset,
    scaffold as scaffold_changeset,
    schema as changeset_schema,
)
from .authoring import apply_intent, preview_intent
from .completion import render as render_completion
from .compiler import compile_world
from .context import MIN_COMPONENT_BUDGET, build_context
from .errors import NotFound, RepositoryError, UsageError, ValidationFailed, WedlError
from .ids import KIND_PREFIX, new_id
from .profiles import PROFILE_NAMES, VECTOR_PROVIDERS
from .model import Record
from .migration import PROTOCOL as MIGRATION_PROTOCOL, apply as apply_migration, preview as preview_migration
from .query import causality, conversation_view, entity_state, hypotheses, interactions_between, knowledge, list_entities, search_world, show_entity, status, story_points, thread_catalog, thread_memberships, timeline, validation_report, whereabouts
from .chronology_api import catalog as chronology_catalog, convert_date as chronology_convert_date, format_date as chronology_format_date, search_annotations as chronology_search_annotations, story_times as chronology_story_times
from .spatial_api import execute as spatial_execute
from .consequence_verification import decode_request as consequence_decode, execute as consequence_execute, failure as consequence_failure
from .event_consequences import ProjectionFailure
from .generational_api import execute_with_viewpoint as generational_execute
from .generational_authoring import scaffold as generational_scaffold, schema as generational_schema
from .repository import Repository
from .server import local_server_url, open_local_browser, preflight_local_server, run_local_server
from .source import serialize_record
from .util import b64url_json, pretty_json
from .validation import validate_world


def _json_file(path: str) -> dict[str, Any]:
    text = sys.stdin.read() if path == "-" else Path(path).read_text(encoding="utf-8")
    value = json.loads(text)
    if not isinstance(value, dict):
        raise ValueError("JSON document must be an object")
    return value


def _write_json_file(path: str, value: dict[str, Any]) -> None:
    """Write a JSON document once, refusing accidental replacement.

    ``-`` follows the usual CLI stream convention and leaves normal rendering
    to ``main``.  A real file is opened exclusively, so a race cannot turn a
    safe scaffold command into an overwrite.
    """

    if path == "-":
        return
    destination = Path(path)
    with destination.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(pretty_json(value))
        stream.write("\n")


EXAMPLE_PACKAGES = {
    "ash-archive": "ash_archive_v07",
    "ash-archive-v07": "ash_archive_v07",
    "frontiersmen": "frontiersmen_v07",
    "frontiersmen-v07": "frontiersmen_v07",
    "tideglass": "tideglass_v07",
}



def _validate_servable_repository(repository: Repository) -> None:
    """Confirm a WEDL world exists without creating cache or session files.

    ``Repository`` deliberately accepts an arbitrary Git root. The server is
    stricter: it needs exactly one world record and no validation errors, and
    that must be established before any address lookup, socket probe,
    reservation, or session-token activity.
    """

    try:
        world = repository.load_world(cache_write=False)
        world.world_record
    except NotFound as exc:
        raise RepositoryError(
            "repository does not contain a WEDL world record",
            details={
                "repository": str(repository.root),
                "hint": "Use a repository initialized by wedl init, or choose its repository root with --repo.",
            },
        ) from exc
    diagnostics = validate_world(world)
    errors = [item for item in diagnostics if item["severity"] == "error"]
    if errors:
        raise ValidationFailed("repository validation failed before starting the server", diagnostics)


def initialize(
    path: Path,
    *,
    example: str | bool | None = "ash-archive",
    git: bool = True,
    profile_name: str | None = None,
    vector_provider: str | None = None,
) -> dict[str, Any]:
    path = path.expanduser().resolve()
    if path.exists() and any(path.iterdir()):
        raise ValueError(f"target is not empty: {path}")
    path.mkdir(parents=True, exist_ok=True)
    if example:
        example_name = "ash-archive" if example is True else str(example)
        package_name = EXAMPLE_PACKAGES.get(example_name)
        if package_name is None:
            raise ValueError(f"unknown example {example_name!r}; choose one of {sorted(EXAMPLE_PACKAGES)}")
        source = resources.files(f"wedl.data.{package_name}").joinpath("story")
        with resources.as_file(source) as source_path:
            shutil.copytree(source_path, path / "story", dirs_exist_ok=True)
    else:
        example_name = None
        (path / "story").mkdir(parents=True)
        world_id = new_id("world")
        frontmatter = {
            "schema": "wedl/v0.3", "kind": "world", "id": world_id, "title": path.name or "Untitled World",
            "domain": "world", "status": "canonical", "tags": [], "aliases": [], "default_timeline": "main",
            "timelines": [{"id": "main", "label": "Main chronology"}],
            "state_keys": {"character": {"location": {"type": "entity", "entity_kind": "location"}, "condition": {"type": "string"}}, "object": {"holder": {"type": "entity", "entity_kind": "character", "exclusive_group": "placement"}, "location": {"type": "entity", "entity_kind": "location", "exclusive_group": "placement"}, "container": {"type": "entity", "entity_kind": "object", "exclusive_group": "placement"}, "condition": {"type": "string"}}},
            "relationship_metrics": {"trust": {"minimum": -1.0, "maximum": 1.0, "default": 0.0}},
            "embedding_policy": {
                "provider": "lsa",
                "model": "wedl-lsa-v1",
                "dimensions": 192,
                "max_features": 8192,
            },
            "compilation_policy": {
                "default_profile": "hybrid",
                "retrieval": {
                    "fts_candidate_limit": 120,
                    "vector_candidate_limit": 120,
                    "hybrid_fts_weight": 1.0,
                    "hybrid_vector_weight": 1.0,
                    "hybrid_rrf_k": 60.0,
                },
            },
            "context_policy": {"default_budget_characters": 8000, "default_max_items": 24},
            "provenance": b64url_json({"v": 1, "tx": new_id("transaction"), "command": "repo.init", "actor": "local:wedl", "generator": f"wedl/{__version__}", "parent": "0" * 40}),
        }
        body = f"# {frontmatter['title']}\n\nAn interactive story world managed by wedl.\n"
        (path / "story" / "world.md").write_bytes(serialize_record(frontmatter, body))
    (path / ".gitignore").write_text(".wedl/\n__pycache__/\n", encoding="utf-8")
    if git:
        subprocess.run(["git", "init", "-q", str(path)], check=True)
        if os.name == "nt":
            # Example worlds can legitimately have descriptive paths longer
            # than the legacy Windows MAX_PATH limit in a user-selected root.
            subprocess.run(["git", "-C", str(path), "config", "core.longpaths", "true"], check=True)
        subprocess.run(["git", "-C", str(path), "config", "user.name", "wedl"], check=True)
        subprocess.run(["git", "-C", str(path), "config", "user.email", "wedl@localhost"], check=True)
        subprocess.run(["git", "-C", str(path), "add", "story", ".gitignore"], check=True)
        subprocess.run(["git", "-C", str(path), "commit", "-qm", "wedl: initialize world"], check=True)
    repository = Repository(path)
    report = compile_world(
        repository,
        "HEAD" if git else "WORKTREE",
        profile_name=profile_name,
        vector_provider=vector_provider,
    )
    return {"repository": str(path), "git": git, "example": example_name, "head": repository.head(), "compile": report}



# The public grammar lives at this import boundary so HTTP integrations do not
# need to import command dispatch.  Keep the CLI export for existing callers.
from .command_parser import parser


def dispatch(args: argparse.Namespace) -> Any:
    if args.command == "completion":
        if args.compact:
            raise UsageError("completion output cannot be combined with --compact")
        return render_completion(parser(), args.shell)
    if args.command == "init":
        return initialize(
            Path(args.path),
            example=None if args.empty else args.example,
            git=not args.no_git,
            profile_name=args.profile,
            vector_provider=args.vector_provider,
        )
    if args.command == "changeset" and args.changeset_command == "schema":
        if args.repo is None:
            if args.revision is not None:
                raise UsageError("schema --revision requires --repo")
            return changeset_schema()
        from .event_consequences import AuthorScope
        from .model import World
        from .validation import validate_world
        from .errors import ValidationFailed
        repository = Repository(args.repo)
        revision = args.revision if args.revision is not None else repository.head()
        loaded = repository.load_world(revision, cache_write=False)
        if loaded.revision != revision:
            raise UsageError("schema context revision does not match the requested revision")
        world = World(loaded.revision, loaded.tree_oid, loaded.records, loaded.root, loaded.source_root)
        diagnostics = validate_world(world)
        if any(item["severity"] == "error" for item in diagnostics):
            raise ValidationFailed("schema context source is invalid", diagnostics)
        return changeset_schema(world=world, scope=AuthorScope(world.world_record.id, frozenset(world.records)))
    if args.command == "consequences":
        raw = sys.stdin.read() if args.file == "-" else Path(args.file).read_text(encoding="utf-8")
        try:
            payload = consequence_decode(raw)
        except ProjectionFailure as failed:
            return consequence_failure(failed)
        return consequence_execute(Repository(args.repo), payload)
    repository = Repository(args.repo)
    value: Any
    if args.command == "status": value = status(repository)
    elif args.command == "validate":
        value = validation_report(repository)
    elif args.command == "compile":
        value = compile_world(
            repository,
            force=args.force,
            profile_name=args.profile,
            vector_provider=args.vector_provider,
            vector_model=args.vector_model,
            vector_dimensions=args.vector_dimensions,
            vector_max_features=args.vector_max_features,
        )
    elif args.command == "migrate":
        request = {
            "protocol": MIGRATION_PROTOCOL,
            "mode": args.mode,
            "expectedHead": args.expected_head,
            "idempotencyKey": args.idempotency_key,
            **({"sourceSnapshotHash": args.source_snapshot_hash} if args.source_snapshot_hash else {}),
            **({"rollbackBackupRef": args.rollback_backup_ref} if args.rollback_backup_ref else {}),
        }
        value = preview_migration(repository, request) if args.migration_command == "preview" else apply_migration(repository, request, confirmation_token_value=args.confirm)
        value.pop("_changes", None); value.pop("_request", None)
    elif args.command == "entity": value = list_entities(repository, args.kind, args.text, require_compiled=args.require_compiled) if args.entity_command == "list" else show_entity(repository, args.entity, require_compiled=args.require_compiled)
    elif args.command == "state": value = entity_state(repository, args.entity, args.tick, args.timeline, args.order, require_compiled=args.require_compiled)
    elif args.command == "knowledge": value = knowledge(repository, args.character, args.tick, args.timeline, args.order, require_compiled=args.require_compiled)
    elif args.command == "interactions": value = interactions_between(repository, args.first, args.second, require_compiled=args.require_compiled)
    elif args.command == "story-points": value = story_points(repository, args.scene, args.tick, args.timeline, args.order, require_compiled=args.require_compiled)
    elif args.command == "timeline": value = timeline(repository, args.timeline, require_compiled=args.require_compiled)
    elif args.command == "chronology":
        if args.chronology_command == "catalog":
            value = chronology_catalog(repository, require_compiled=args.require_compiled)
        else:
            request = _json_file(args.file)
            value = {
                "format": chronology_format_date,
                "convert": chronology_convert_date,
                "search": chronology_search_annotations,
                "story-times": chronology_story_times,
            }[args.chronology_command](repository, request, require_compiled=args.require_compiled)
    elif args.command == "spatial":
        value = spatial_execute(repository, args.spatial_command, _json_file(args.file), require_compiled=args.require_compiled)
    elif args.command == "generational":
        if args.generational_command == "scaffold":
            value = generational_scaffold(repository)
            _write_json_file(args.output, value)
            if args.output != "-":
                return None
        elif args.generational_command == "schema":
            value = generational_schema()
        else:
            value = generational_execute(repository, args.generational_command,
                                         _json_file(args.file), require_compiled=args.require_compiled,
                                         viewpoint=args.viewpoint)
    elif args.command == "threads": value = thread_catalog(repository, require_compiled=args.require_compiled)
    elif args.command == "thread-memberships": value = thread_memberships(repository, tuple(args.record_ids), tuple(args.thread_ids), require_compiled=args.require_compiled)
    elif args.command == "whereabouts": value = whereabouts(repository, args.character, args.tick, args.timeline, args.order, require_compiled=args.require_compiled)
    elif args.command == "hypotheses": value = hypotheses(repository, args.hypothesis, status=args.status, text=args.text, require_compiled=args.require_compiled)
    elif args.command == "causal": value = causality(repository, args.event, direction=args.direction, tick=args.tick, timeline=args.timeline, order=args.order, require_compiled=args.require_compiled)
    elif args.command == "search": value = search_world(repository, args.query, perspective=args.perspective, character_id=args.character, scene_id=args.scene, mode=args.mode, limit=args.limit, timeline=args.timeline, tick=args.tick, order=args.order, include_text=args.include_text, all_time=args.all_time, include_hypotheses=args.include_hypotheses, thread_ids=None if args.thread_ids is None else tuple(args.thread_ids), require_compiled=args.require_compiled)
    elif args.command == "context": value = build_context(repository, character_id=args.character, scene_id=args.scene, perspective=args.perspective, query=args.query, max_characters=args.max_characters, max_items=args.max_items, search_mode=args.mode, timeline=args.timeline, tick=args.tick, order=args.order, _thread_filter_ids=None if args.recall_thread_ids is None else tuple(args.recall_thread_ids), require_compiled=args.require_compiled)
    elif args.command == "conversation": value = conversation_view(repository, args.conversation, perspective=args.perspective, character_id=args.character, timeline=args.timeline, tick=args.tick, order=args.order, all_time=args.all_time, require_compiled=args.require_compiled)
    elif args.command == "author":
        if args.author_command == "request":
            intent = _json_file(args.file)
            if args.author_subject_command == "preview":
                return preview_intent(repository, intent)
            if args.yes and isinstance(intent, dict) and str(intent.get("action", "")).startswith(("spatial.", "generational.")):
                raise UsageError("--yes cannot bypass preview confirmation for spatial or generational authoring")
            if args.yes and isinstance(intent, dict) and intent.get("action") == "consequence.batch":
                raise UsageError("--yes cannot bypass preview confirmation for consequence authoring")
            value = apply_intent(repository, intent, confirmation_token_value=args.confirm, allow_unconfirmed=args.yes)
            return value
        action = {
            ("current-time", "set"): "current-time.set",
            ("scene", "create"): "scene.create",
            ("scene", "advance"): "scene.advance",
            ("scene", "close"): "scene.close",
            ("move", None): "character.move",
            ("conversation", "create"): "conversation.create",
            ("conversation", "append"): "conversation.append",
            ("hypothesis", "create"): "hypothesis.create",
            ("hypothesis", "adopt"): "hypothesis.adopt",
            ("hypothesis", "reject"): "hypothesis.reject",
            ("chronology", "replace"): "chronology.replace",
        }.get((args.author_command, getattr(args, "author_subject_command", None)))
        if action is None:
            raise UsageError("unsupported authoring command")
        if action == "chronology.replace":
            change = _json_file(args.file)
            intent = {"action": action, "expectedHead": args.expected_head, "change": change}
            if args.summary is not None:
                intent["summary"] = args.summary
            if args.idempotency_key is not None:
                intent["idempotencyKey"] = args.idempotency_key
            value = apply_intent(repository, intent, confirmation_token_value=args.confirm, allow_unconfirmed=args.yes) if (args.confirm or args.yes) else preview_intent(repository, intent)
            return value
        is_hypothesis_action = action.startswith("hypothesis.")
        if not is_hypothesis_action and getattr(args, "tick", None) is None and (getattr(args, "timeline", None) is not None or getattr(args, "order", None) is not None):
            raise UsageError("--timeline and --order require --tick for authoring commands")
        time = None if getattr(args, "tick", None) is None else {"tick": args.tick, **({"timeline": args.timeline} if args.timeline else {}), **({"order": args.order} if args.order is not None else {})}
        intent: dict[str, Any] = {"action": action, "time": time, "summary": args.summary, "idempotencyKey": args.idempotency_key}
        for name in ("title", "scene", "event", "location", "timeline", "characters", "conversation", "hypothesis", "text", "kind", "speaker", "addressee", "actors", "statement", "subjects", "alternatives", "context", "canonical_records", "note"):
            if hasattr(args, name) and getattr(args, name) is not None:
                # Semantic authoring uses camelCase only at its HTTP boundary;
                # flags remain readable Python names.
                intent[{"canonical_records": "canonicalRecords"}.get(name, name)] = getattr(args, name)
        if getattr(args, "interrupt_last", False):
            intent["interruptLast"] = True
        if args.confirm or args.yes:
            value = apply_intent(repository, intent, confirmation_token_value=args.confirm, allow_unconfirmed=args.yes)
        else:
            value = preview_intent(repository, intent)
    elif args.command == "changeset":
        if args.changeset_command == "scaffold":
            value = scaffold_changeset(repository)
            _write_json_file(args.output, value)
            if args.output != "-":
                return None
        else:
            payload = _json_file(args.file)
            if args.changeset_command == "preview":
                value = preview_changeset(repository, payload, use_current_head=args.use_current_head)
                value.pop("_changes", None)
            else:
                value = apply_changeset(repository, payload, use_current_head=args.use_current_head, confirmation_token_value=args.confirm, allow_unconfirmed=args.yes)
    elif args.command == "serve":
        _validate_servable_repository(repository)
        url = preflight_local_server(args.host, args.port)

        def announce_ready() -> None:
            if args.open:
                open_local_browser(url)
            if args.compact:
                print(json.dumps({"url": url}, ensure_ascii=False, separators=(",", ":")), flush=True)
            else:
                print(f"Ready: {url} (press Ctrl+C to stop)", flush=True)

        run_local_server(repository.root, host=args.host, port=args.port, on_ready=announce_ready)
        return None
    else:
        raise ValueError(args.command)
    return value


def main(argv: list[str] | None = None) -> int:
    argument_list = list(sys.argv[1:] if argv is None else argv)
    try:
        args = parser().parse_args(argument_list)
        value = dispatch(args)
        if value is not None:
            # Spatial semantic failures are valid protocol envelopes rather
            # than malformed CLI invocations, but remain non-successful CLI
            # outcomes.  Keep their exact envelope on stderr for parity with
            # the HTTP status mapping instead of pretending an empty result
            # succeeded.
            if args.command == "consequences" and value["outcome"] != "ok":
                print(json.dumps(value, ensure_ascii=False, separators=(",", ":")) if args.compact else pretty_json(value), file=sys.stderr)
                return 2
            if args.command in {"spatial", "generational"} and isinstance(value, dict) and value.get("state") not in {"ok", "available", "unknown"}:
                print(json.dumps(value, ensure_ascii=False, separators=(",", ":")) if args.compact else pretty_json(value), file=sys.stderr)
                return 2
            if args.command == "completion":
                print(value, end="")
            else:
                print(json.dumps(value, ensure_ascii=False, separators=(",", ":")) if args.compact else pretty_json(value))
        return 0
    except (WedlError, ValueError, OSError, json.JSONDecodeError) as exc:
        payload = exc.as_dict() if isinstance(exc, WedlError) else {"code": "error", "message": str(exc)}
        compact = "--compact" in argument_list
        print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")) if compact else pretty_json(payload), file=sys.stderr)
        return 2


def server_main() -> int:
    return main(["serve", *sys.argv[1:]])


if __name__ == "__main__":
    raise SystemExit(main())
