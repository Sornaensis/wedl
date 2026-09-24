"""The canonical, dispatch-free WEDL command grammar."""

from __future__ import annotations

import argparse
import sys
from typing import Any

from . import __version__
from .context import MIN_COMPONENT_BUDGET
from .errors import UsageError
from .ids import KIND_PREFIX
from .profiles import PROFILE_NAMES, VECTOR_PROVIDERS

EXAMPLE_PACKAGES = {"ash-archive": "ash_archive", "frontiersmen": "frontiersmen"}


class WedlArgumentParser(argparse.ArgumentParser):
    """An argparse parser whose invalid invocations use WEDL's JSON contract.

    ``argparse`` normally writes a human-oriented diagnostic and exits before
    :func:`main` can apply the CLI error protocol.  Help and version requests
    retain argparse's normal output and successful exit behaviour; only
    malformed input is converted into a ``UsageError``.
    """

    def error(self, message: str) -> None:
        raise UsageError(
            message,
            details={
                "context": {
                    "command": self.prog,
                    "usage": self.format_usage().strip(),
                    "help": f"{self.prog} --help",
                }
            },
        )

    def parse_args(
        self,
        args: list[str] | None = None,
        namespace: argparse.Namespace | None = None,
    ) -> argparse.Namespace:
        """Preserve the selected command's diagnostic context for late errors.

        ``argparse`` reports leftover arguments through the root parser after a
        subparser has successfully consumed its command name. That makes a
        typo such as ``wedl status --bad`` point people at ``wedl --help``.
        Reframe only those root-level leftovers to the deepest command the
        invocation actually selected; unknown commands and missing selectors
        still correctly belong to the root parser.
        """
        argument_list = list(sys.argv[1:] if args is None else args)
        try:
            return super().parse_args(argument_list, namespace)
        except UsageError as exc:
            selected = self._selected_subparser(argument_list)
            context = exc.details.get("context", {})
            if selected is not self and context.get("command") == self.prog:
                exc.details["context"] = {
                    "command": selected.prog,
                    "usage": selected.format_usage().strip(),
                    "help": f"{selected.prog} --help",
                }
            raise

    def _selected_subparser(self, arguments: list[str]) -> argparse.ArgumentParser:
        """Return the deepest explicitly selected subcommand, if any.

        This is intentionally limited to subparser selectors. It is not a
        second parser: argparse remains the authority for option values and
        validation, while this walk only supplies a better help destination.
        """
        selected: argparse.ArgumentParser = self
        index = 0
        while True:
            subparsers = next(
                (action for action in selected._actions if isinstance(action, argparse._SubParsersAction)),
                None,
            )
            if subparsers is None:
                return selected
            option_actions = {
                option: action
                for action in selected._actions
                for option in action.option_strings
            }
            child: argparse.ArgumentParser | None = None
            while index < len(arguments):
                token = arguments[index]
                if token == "--":
                    return selected
                option = option_actions.get(token)
                if option is not None:
                    # Root options currently take no values, but honoring the
                    # action shape keeps this robust as global options grow.
                    index += 1 + (0 if option.nargs == 0 else 1)
                    continue
                if token.startswith("-"):
                    index += 1
                    continue
                child = subparsers.choices.get(token)
                if child is not None:
                    index += 1
                    break
                index += 1
            if child is None:
                return selected
            selected = child


def _add_repo_argument(command: argparse.ArgumentParser) -> None:
    command.add_argument(
        "--repo",
        default=".",
        metavar="PATH",
        help="repository root; relative paths resolve from the current working directory (default: .)",
    )


def _add_require_compiled_argument(command: argparse.ArgumentParser) -> None:
    command.add_argument(
        "--require-compiled",
        action="store_true",
        help="fail with compile_required when the local compiled cache is missing, stale, or incompatible instead of rebuilding it",
    )


def integer_at_least(minimum: int) -> Any:
    """Build an argparse value parser with an actionable lower-bound error."""

    def parse(value: str) -> int:
        try:
            parsed = int(value)
        except ValueError as exc:
            raise argparse.ArgumentTypeError(f"expected an integer, got {value!r}") from exc
        if parsed < minimum:
            raise argparse.ArgumentTypeError(f"must be at least {minimum}, got {parsed}")
        return parsed

    parse.minimum = minimum
    parse.maximum = None
    parse.value_type = "integer"
    return parse


def integer_between(minimum: int, maximum: int) -> Any:
    """Build an argparse value parser with an actionable inclusive range error."""

    def parse(value: str) -> int:
        try:
            parsed = int(value)
        except ValueError as exc:
            raise argparse.ArgumentTypeError(f"expected an integer, got {value!r}") from exc
        if not minimum <= parsed <= maximum:
            raise argparse.ArgumentTypeError(f"must be between {minimum} and {maximum}, got {parsed}")
        return parsed

    parse.minimum = minimum
    parse.maximum = maximum
    parse.value_type = "integer"
    return parse


def parser() -> argparse.ArgumentParser:
    root = WedlArgumentParser(
        prog="wedl",
        description="Explore and manage Git-backed interactive story worlds.",
        epilog="""First run:
  wedl init frontiersmen --example frontiersmen
  wedl validate --repo frontiersmen
  wedl status --repo frontiersmen
  wedl compile --repo frontiersmen --profile hybrid --vector-provider lsa
  wedl state Rhea --repo frontiersmen --tick 195
  wedl serve --repo frontiersmen

Explore:
  wedl entity list --repo frontiersmen --kind character
  wedl search "amber manifestations" --repo frontiersmen --timeline main --tick 195
  wedl context Rhea --repo frontiersmen --scene "The Hunt Begins" --query "escape route"

Entity arguments accept IDs, titles, aliases, and slugs. Use --tick (and, when
needed, --timeline/--order) to inspect what was true at a specific moment.

Story time uses signed ordinal ticks (negative values are valid); --order
sequences facts at one tick. Origins are descriptive, bounded intervals are
inclusive, and ticks do not convert to elapsed duration.""",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    root.add_argument(
        "--version",
        action="version",
        version=__version__,
        help="print the installed WEDL version and exit",
    )
    root.add_argument(
        "--compact",
        action="store_true",
        help="emit successful results and diagnostics as one-line JSON; place before the command",
    )
    commands = root.add_subparsers(
        dest="command", required=True, metavar="COMMAND", title="commands", parser_class=WedlArgumentParser,
    )

    completion = commands.add_parser(
        "completion",
        help="print Bash or PowerShell completion setup code",
        description=(
            "Print a self-contained shell completion script generated from this "
            "CLI parser. It completes only commands, options, and fixed option "
            "choices; it never reads a repository or suggests authored entities."
        ),
    )
    completion.add_argument(
        "shell",
        choices=("bash", "powershell"),
        help="target shell; source the emitted script in that shell",
    )

    init = commands.add_parser(
        "init", help="create a repository from an example or empty world",
        description="Create a new WEDL repository, seed it from an included example or an empty world, and compile it.",
    )
    init.add_argument("path", metavar="PATH", help="new, empty repository directory")
    init_group = init.add_mutually_exclusive_group()
    init_group.add_argument("--empty", action="store_true", help="create an empty world instead of copying an example")
    init_group.add_argument("--example", choices=sorted(EXAMPLE_PACKAGES), default="ash-archive", help="example world to copy (default: ash-archive)")
    init.add_argument("--no-git", action="store_true", help="do not initialize and commit a Git repository")
    init.add_argument("--profile", choices=PROFILE_NAMES, help="initial retrieval compilation profile")
    init.add_argument("--vector-provider", choices=VECTOR_PROVIDERS, help="initial vector provider for compilation")
    command_help = {
        "status": "show repository and compilation status",
        "validate": "validate all story records",
        "compile": "compile the story world for retrieval",
    }
    for name in ("status", "validate", "compile"):
        description = None
        if name == "status":
            description = (
                "Show repository and compilation status, including the time model. "
                "Ticks are signed ordinal coordinates (negative values are valid); "
                "order sequences facts at one tick. Timeline origins are descriptive, "
                "not lower bounds; bounded intervals are inclusive; ticks have no "
                "elapsed-duration conversion."
            )
        if description is None:
            description = {
                "validate": "Validate every authored record and report source diagnostics without changing story files.",
                "compile": "Compile the current story revision into the local retrieval cache used by search and context commands.",
            }[name]
        cmd = commands.add_parser(name, help=command_help[name], description=description)
        _add_repo_argument(cmd)
        if name == "compile":
            cmd.add_argument("--force", action="store_true", help="rebuild even when the cache is current")
            cmd.add_argument("--profile", choices=PROFILE_NAMES, help="retrieval profile to compile")
            cmd.add_argument("--vector-provider", choices=VECTOR_PROVIDERS, help="embedding/vector provider")
            cmd.add_argument("--vector-model", metavar="MODEL", help="provider-specific vector model name")
            cmd.add_argument("--vector-dimensions", metavar="DIMENSIONS", type=int, help="provider-specific vector dimensions")
            cmd.add_argument("--vector-max-features", metavar="COUNT", type=int, help="maximum lexical features for compatible providers")
    migrate = commands.add_parser("migrate", help="preview or apply a local, confirmed source schema migration", description="Run an explicit local source migration only after inspecting its dry-run diff. This command has no HTTP equivalent and never interprets quarantined v0.4 sources through normal loading.")
    migrate_sub = migrate.add_subparsers(dest="migration_command", required=True, metavar="ACTION", title="migration actions", parser_class=WedlArgumentParser)
    for name in ("preview", "apply"):
        action = "inspect its source-only diff without writes or cache activity" if name == "preview" else "write the exact confirmed source-only preview as one forward Git commit and rebuild the disposable cache"
        command = migrate_sub.add_parser(name, help=f"{name} a local source migration", description=f"{action}.")
        _add_repo_argument(command)
        command.add_argument("--mode", required=True, choices=("upgrade-v03", "upgrade-v06", "upgrade-v07", "recover-v04", "rollback"), help="explicit migration mode")
        command.add_argument("--expected-head", required=True, metavar="HEAD", help="exact current Git HEAD audited by this request")
        command.add_argument("--source-snapshot-hash", metavar="SHA256", required=name == "apply", help="exact sourceSnapshotHash from preview; required to bind an apply")
        command.add_argument("--idempotency-key", required=True, metavar="KEY", help="stable key for this exact local migration request")
        command.add_argument("--rollback-backup-ref", metavar="REF", help="backup ref to restore; required only for --mode rollback")
        if name == "apply":
            command.add_argument("--confirm", required=True, metavar="TOKEN", help="confirmationToken returned by the exact migration preview")
    entity = commands.add_parser("entity", help="list or inspect authored entities", description="List entities by kind or text, or show one complete entity by ID, title, alias, or slug.")
    entity_sub = entity.add_subparsers(dest="entity_command", required=True, metavar="ACTION", title="entity actions", parser_class=WedlArgumentParser)
    list_cmd = entity_sub.add_parser("list", help="list entities with optional filters", description="List world entities. Use --kind to narrow by canonical entity kind and --text to search titles and aliases.")
    canonical_kinds = sorted(kind for kind in KIND_PREFIX if kind != "hypothesis")
    _add_repo_argument(list_cmd); _add_require_compiled_argument(list_cmd); list_cmd.add_argument("--kind", metavar="KIND", choices=canonical_kinds, help=f"canonical entity kind; choose one of {', '.join(canonical_kinds)}"); list_cmd.add_argument("--text", metavar="TEXT", help="case-insensitive title or alias text")
    show_cmd = entity_sub.add_parser("show", help="show one complete entity", description="Show one entity by stable ID, exact title, alias, or slug.")
    show_cmd.add_argument("entity", metavar="ENTITY", help="entity ID, title, alias, or slug"); _add_repo_argument(show_cmd); _add_require_compiled_argument(show_cmd)
    state_cmd = commands.add_parser("state", help="inspect an entity at an exact story time", description="Resolve an entity's derived state at a required tick. Ticks are signed ordinals; order selects the point within a tick.")
    state_cmd.add_argument("entity", metavar="ENTITY", help="entity ID, title, alias, or slug"); _add_repo_argument(state_cmd); _add_require_compiled_argument(state_cmd); state_cmd.add_argument("--tick", metavar="TICK", type=int, required=True, help="required signed ordinal tick"); state_cmd.add_argument("--timeline", metavar="TIMELINE", help="timeline ID (defaults to the world's default timeline)"); state_cmd.add_argument("--order", metavar="ORDER", type=int, default=2_147_483_647, help="same-tick ordering coordinate (default: latest)")
    know_cmd = commands.add_parser("knowledge", help="inspect what a character knows at a story time", description="Return perspective-safe knowledge for a character at a required story-time coordinate.")
    know_cmd.add_argument("character", metavar="CHARACTER", help="character ID, title, alias, or slug"); _add_repo_argument(know_cmd); _add_require_compiled_argument(know_cmd); know_cmd.add_argument("--tick", metavar="TICK", type=int, required=True, help="required signed ordinal tick"); know_cmd.add_argument("--timeline", metavar="TIMELINE", help="timeline ID (defaults to the world's default timeline)"); know_cmd.add_argument("--order", metavar="ORDER", type=int, default=2_147_483_647, help="same-tick ordering coordinate (default: latest)")
    inter = commands.add_parser("interactions", help="show recorded interactions between two entities", description="Show relationships and interactions involving two entity references.")
    inter.add_argument("first", metavar="FIRST", help="first entity ID, title, alias, or slug"); inter.add_argument("second", metavar="SECOND", help="second entity ID, title, alias, or slug"); _add_repo_argument(inter); _add_require_compiled_argument(inter)
    sp = commands.add_parser("story-points", help="list story-point state at a scene or exact time", description="List active, eligible, and resolved story points at a selected scene cursor or an explicit tick.")
    _add_repo_argument(sp); _add_require_compiled_argument(sp); sp.add_argument("--scene", metavar="SCENE", help="scene ID, title, alias, or slug whose cursor supplies the time"); sp.add_argument("--tick", metavar="TICK", type=int, help="explicit signed ordinal tick"); sp.add_argument("--timeline", metavar="TIMELINE", help="timeline ID; requires --tick (otherwise the selected scene cursor is used)"); sp.add_argument("--order", metavar="ORDER", type=int, default=2_147_483_647, help="same-tick ordering coordinate (default: latest)")
    timeline = commands.add_parser("timeline", help="read one declared ordinal story timeline", description="Return the complete author-facing chronology for one declared timeline. Story beats are ordered by signed tick and same-tick order only; gaps never represent elapsed duration, and span endpoints are inclusive.")
    _add_repo_argument(timeline); _add_require_compiled_argument(timeline); timeline.add_argument("--timeline", metavar="TIMELINE", help="declared timeline ID (defaults to the world's default timeline)")
    chronology = commands.add_parser("chronology", help="read the public calendar chronology protocol", description="Read calendar dates, conversions, annotation relations, and explicit anchor mappings. Story ticks remain unitless ordinals and are never accepted here.")
    chronology_sub = chronology.add_subparsers(dest="chronology_command", required=True, metavar="ACTION", title="chronology actions", parser_class=WedlArgumentParser)
    catalog = chronology_sub.add_parser("catalog", help="read the chronology catalogue", description="Return the public calendar catalogue and chronology capability.")
    _add_repo_argument(catalog); _add_require_compiled_argument(catalog)
    for name, help_text in (("format", "format one civil or era date"), ("convert", "convert one exact civil or era date"), ("search", "search chronology annotations"), ("story-times", "map a chronology date through explicit anchors")):
        command = chronology_sub.add_parser(name, help=help_text, description=help_text.capitalize() + ".")
        command.add_argument("file", metavar="FILE", help="raw wedl-chronology/v1 JSON file, or - for stdin")
        _add_repo_argument(command); _add_require_compiled_argument(command)
    spatial = commands.add_parser("spatial", help="read the compiled spatial protocol", description="Read only the compiled wedl-spatial/v1 projection. Requests are raw JSON files or stdin and never select source paths.")
    spatial_sub = spatial.add_subparsers(dest="spatial_command", required=True, metavar="ACTION", title="spatial actions", parser_class=WedlArgumentParser)
    for name, help_text in (("containment", "read authored containment"), ("children", "list authored child locations"), ("bbox", "query geometry bounds"), ("nearby", "query same-map geometry"), ("adjacency", "read authored outbound edges"), ("reachability", "traverse authored directed edges"), ("path", "find an authored metric path"), ("overlay-as-of", "read authorized overlays at an exact StoryTime")):
        command = spatial_sub.add_parser(name, help=help_text, description=help_text.capitalize() + ".")
        command.add_argument("file", metavar="FILE", help="raw wedl-spatial/v1 JSON file, or - for stdin")
        _add_repo_argument(command); _add_require_compiled_argument(command)
    generational = commands.add_parser("generational", help="read cited generational history by name", description="Read private wedl-generational/v1 evidence using a selected revision and local author scope. Character mode remains closed until a trusted character identity exists.")
    generational_sub = generational.add_subparsers(dest="generational_command", required=True, metavar="ACTION", title="generational actions", parser_class=WedlArgumentParser)
    for name, help_text in (("parents", "read biological and adoptive parents"), ("ancestors", "traverse cited ancestry"), ("descendants", "traverse cited descendants"), ("relatives", "find a cited relation path"), ("union", "read a n-ary union"), ("organization", "read an organization and active roster"), ("legacy", "read tenure, holders, claims, and succession"), ("vital", "read known vital history"), ("search", "search private structural tokens"), ("context", "build bounded generational context")):
        command = generational_sub.add_parser(name, help=help_text, description=help_text.capitalize() + ".")
        command.add_argument("file", metavar="FILE", help="raw wedl-generational/v1 JSON request file, or - for stdin")
        _add_repo_argument(command); _add_require_compiled_argument(command)
    gen_scaffold = generational_sub.add_parser("scaffold", help="write a current-HEAD generational starter intent", description="Create a v0.7 organization intent bound to current HEAD. Edit the explicit choices, then use author request preview and author request apply --confirm TOKEN.")
    _add_repo_argument(gen_scaffold)
    gen_scaffold.add_argument("--output", metavar="FILE", default="-", help="JSON output path, or - for stdout")
    gen_schema = generational_sub.add_parser("schema", help="show the closed eight-kind generational intent variants", description="Emit the explicit v0.7 generational create, append, correct, and batch intent contract.")
    _add_repo_argument(gen_schema)
    threads = commands.add_parser("threads", help="list optional narrative thread labels", description="List declared narrative grouping labels for the one shared world. Thread labels never create alternate canon, time, state, or search corpora.")
    _add_repo_argument(threads); _add_require_compiled_argument(threads)
    thread_memberships = commands.add_parser("thread-memberships", help="project selected narrative membership for supplied records", description="Return only the selected narrative-group membership intersection for supplied canonical records. Narrative grouping never changes the shared world, time, state, or retrieval ranking.")
    _add_repo_argument(thread_memberships); _add_require_compiled_argument(thread_memberships)
    thread_memberships.add_argument("--record-id", dest="record_ids", metavar="RECORD", action="append", required=True, help="canonical record ID to project; repeat in sorted unique order (1 to 256)")
    thread_memberships.add_argument("--thread-id", dest="thread_ids", metavar="THREAD", action="append", required=True, help="declared narrative grouping ID; repeat in sorted unique order (1 to 32)")
    whereabouts = commands.add_parser("whereabouts", help="show character locations, journeys, and calculated prominence", description="Return a read-only author projection for canonical and retired characters. Locations and journeys use only initial state and canonical location effects; calculated prominence is a disposable, horizon-aware navigation aid derived from scene, POV, event, and relationship evidence. It never changes canonical source data or infers travel. Without --tick, the current shared cursor is used when available.")
    _add_repo_argument(whereabouts); _add_require_compiled_argument(whereabouts)
    whereabouts.add_argument("--character", metavar="CHARACTER", help="optional character ID, title, alias, or slug")
    whereabouts.add_argument("--tick", metavar="TICK", type=int, help="explicit signed ordinal author horizon")
    whereabouts.add_argument("--timeline", metavar="TIMELINE", help="timeline for --tick; defaults to the world's default timeline")
    whereabouts.add_argument("--order", metavar="ORDER", type=int, help="same-tick horizon coordinate (default: latest)")
    hypotheses = commands.add_parser("hypotheses", help="list or read non-canonical author possibilities", description="Read author hypotheses and incomplete chronology notes. Possibilities are not canonical facts, are not horizon-scoped, and never affect story state, causality, whereabouts, or character context.")
    _add_repo_argument(hypotheses); _add_require_compiled_argument(hypotheses)
    hypotheses.add_argument("hypothesis", metavar="HYPOTHESIS", nargs="?", help="optional possibility title, alias, slug, or ID")
    hypotheses.add_argument("--status", choices=["open", "adopted", "rejected"], help="optional lifecycle filter")
    hypotheses.add_argument("--text", metavar="TEXT", help="optional title, statement, or body text filter")
    causal = commands.add_parser("causal", help="trace authored event causes without inferring new canon", description="Return the explicit event causal DAG around one event. Results are clipped at the selected author horizon; only authored event.causes edges are followed.")
    causal.add_argument("event", metavar="EVENT", help="event ID, title, alias, or slug")
    _add_repo_argument(causal); _add_require_compiled_argument(causal)
    causal.add_argument("--direction", choices=["upstream", "downstream", "both"], default="both", help="causes, consequences, or both (default: both)")
    causal.add_argument("--tick", metavar="TICK", type=int, help="explicit signed ordinal author horizon")
    causal.add_argument("--timeline", metavar="TIMELINE", help="timeline for --tick; defaults to the world's default timeline")
    causal.add_argument("--order", metavar="ORDER", type=int, default=2_147_483_647, help="same-tick horizon coordinate (default: latest)")
    search = commands.add_parser("search", help="search the compiled world within a perspective and time scope", description="Search compiled story material. Author searches are as-of a scene cursor by default; character searches enforce availability and knowledge boundaries. Author --all-time returns all history, but cannot be combined with --tick, --timeline, or --order.")
    search.add_argument("query", metavar="QUERY", help="words or phrases to retrieve"); _add_repo_argument(search); _add_require_compiled_argument(search); search.add_argument("--perspective", choices=["author", "character"], default="author", help="author sees authored material; character enforces knowledge boundaries"); search.add_argument("--character", metavar="CHARACTER", help="required character reference for --perspective character"); search.add_argument("--scene", metavar="SCENE", help="scene reference supplying the default as-of time"); search.add_argument("--mode", choices=["fts", "vector", "hybrid"], default="hybrid", help="retrieval mode (default: hybrid)"); search.add_argument("--limit", metavar="COUNT", type=integer_between(1, 50), default=20, help="maximum results, from 1 to 50 (default: 20)"); search.add_argument("--include-text", action="store_true", help="include matching source text in results"); search.add_argument("--include-hypotheses", action="store_true", help="author only: include explicitly non-canonical possibilities; they are never horizon facts"); search.add_argument("--thread-id", dest="thread_ids", metavar="THREAD", action="append", help="optional narrative grouping ID; repeat in sorted order to retain matching ranked results"); search.add_argument("--tick", metavar="TICK", type=int, help="explicit signed ordinal as-of tick"); search.add_argument("--timeline", metavar="TIMELINE", help="timeline for --tick; defaults to the world's default timeline"); search.add_argument("--order", metavar="ORDER", type=int, help="same-tick ordering coordinate"); search.add_argument("--all-time", action="store_true", help="author only: return all history; conflicts with --tick, --timeline, and --order")
    context = commands.add_parser("context", help="build a perspective-limited writing context packet", description="Build a bounded context packet for a character. Character mode never relaxes authored availability, presence, audience, or time rules.")
    context.add_argument("character", metavar="CHARACTER", help="character ID, title, alias, or slug"); _add_repo_argument(context); _add_require_compiled_argument(context); context.add_argument("--scene", metavar="SCENE", help="scene reference supplying the default as-of time"); context.add_argument("--perspective", choices=["character", "author", "dramatic-irony"], default="character", help="knowledge boundary used to build the packet"); context.add_argument("--query", metavar="QUERY", help="optional focus text for ranking eligible context"); context.add_argument("--max-characters", metavar="COUNT", type=integer_at_least(MIN_COMPONENT_BUDGET), default=8000, help=f"maximum serialized packet characters; at least {MIN_COMPONENT_BUDGET} (default: 8000)"); context.add_argument("--max-items", metavar="COUNT", type=integer_at_least(1), default=24, help="maximum included items; at least 1 (default: 24)"); context.add_argument("--mode", choices=["fts", "vector", "hybrid"], default="hybrid", help="retrieval mode for optional focus (default: hybrid)"); context.add_argument("--recall-thread-id", dest="recall_thread_ids", metavar="THREAD", action="append", help="optional narrative grouping ID; repeat in sorted order to retain matching useful recall only"); context.add_argument("--tick", metavar="TICK", type=int, help="explicit signed ordinal as-of tick"); context.add_argument("--timeline", metavar="TIMELINE", help="timeline for --tick; defaults to the world's default timeline"); context.add_argument("--order", metavar="ORDER", type=int, default=2_147_483_647, help="same-tick ordering coordinate (default: latest)")
    conv = commands.add_parser("conversation", help="show a conversation transcript at a perspective and time", description="Show an authored transcript or a character-limited conversation view at an explicit or derived story moment.")
    conv_sub = conv.add_subparsers(dest="conversation_command", required=True, metavar="ACTION", title="conversation actions", parser_class=WedlArgumentParser)
    conv_show = conv_sub.add_parser("show", help="show one conversation", description="Show one conversation. Character perspective enforces participant presence and audibility. Author --all-time returns complete history, but cannot be combined with --tick, --timeline, or --order.")
    conv_show.add_argument("conversation", metavar="CONVERSATION", help="conversation ID, title, alias, or slug"); _add_repo_argument(conv_show); _add_require_compiled_argument(conv_show); conv_show.add_argument("--perspective", choices=["author", "character"], default="author", help="author transcript or character-limited view"); conv_show.add_argument("--character", metavar="CHARACTER", help="required character reference for --perspective character"); conv_show.add_argument("--tick", metavar="TICK", type=int, help="explicit signed ordinal as-of tick"); conv_show.add_argument("--timeline", metavar="TIMELINE", help="timeline for --tick; defaults to the world's default timeline"); conv_show.add_argument("--order", metavar="ORDER", type=int, help="same-tick ordering coordinate"); conv_show.add_argument("--all-time", action="store_true", help="author only: return full history; conflicts with --tick, --timeline, and --order")
    author = commands.add_parser("author", help="preview or apply simple authoring actions using names instead of IDs", description="Resolve character, location, scene, and conversation names into the existing canonical changeset protocol. Every command previews by default. Re-run the same command with --confirm TOKEN to apply its preview; --yes is a CLI-only explicit bypass.")
    author_sub = author.add_subparsers(dest="author_command", required=True, metavar="ACTION", title="authoring actions", parser_class=WedlArgumentParser)

    def author_common(command: argparse.ArgumentParser) -> None:
        _add_repo_argument(command)
        command.add_argument("--summary", metavar="TEXT", help="optional Git commit summary")
        command.add_argument("--idempotency-key", metavar="KEY", help="stable retry key; a deterministic key is used when omitted")
        confirmation = command.add_mutually_exclusive_group()
        confirmation.add_argument("--confirm", metavar="TOKEN", help="apply this exact previously previewed command")
        confirmation.add_argument("--yes", action="store_true", help="unsafe CLI-only bypass of preview confirmation")

    def author_time(command: argparse.ArgumentParser, *, required: bool = False) -> None:
        command.add_argument("--tick", metavar="TICK", type=int, required=required, help="signed ordinal tick" if required else "signed ordinal tick (uses the relevant current cursor when omitted)")
        command.add_argument("--timeline", metavar="TIMELINE", help="timeline ID (defaults to the world default)")
        command.add_argument("--order", metavar="ORDER", type=int, default=None, help="same-tick ordering coordinate (current-time set defaults to exact 0; newly authored beats use the next unused order)")

    chronology_author = author_sub.add_parser("chronology", help="replace chronology catalogue and/or record annotations", description="Preview or atomically replace complete chronology catalogue and record annotation sections.")
    chronology_author_sub = chronology_author.add_subparsers(dest="author_subject_command", required=True, parser_class=WedlArgumentParser)
    chronology_replace = chronology_author_sub.add_parser("replace", help="preview or replace complete chronology sections", description="Preview or confirm a full chronology catalogue and/or record-annotation replacement.")
    chronology_replace.add_argument("file", metavar="FILE", help="raw chronology replacement JSON file, or - for stdin")
    author_common(chronology_replace)
    chronology_replace.add_argument("--expected-head", required=True, metavar="HEAD", help="exact Git HEAD audited by this replacement")

    cursor = author_sub.add_parser("current-time", help="set the shared world cursor and advance all active fronts", description="Set the shared world cursor at an exact signed ordinal coordinate and advance every active front to it.")
    cursor_sub = cursor.add_subparsers(dest="author_subject_command", required=True, parser_class=WedlArgumentParser)
    cursor_set = cursor_sub.add_parser("set", help="preview or set the shared current time", description="Preview or set the shared current time at an exact signed ordinal coordinate.")
    author_common(cursor_set); author_time(cursor_set, required=True)

    scene = author_sub.add_parser("scene", help="create, advance, or close a scene", description="Create, advance, or close a scene through previewable authoring actions.")
    scene_sub = scene.add_subparsers(dest="author_subject_command", required=True, parser_class=WedlArgumentParser)
    scene_create = scene_sub.add_parser("create", help="create an active scene at a location with named characters", description="Create an active scene at a named location with named present characters.")
    scene_create.add_argument("title", metavar="TITLE", help="title for the new scene"); scene_create.add_argument("--location", required=True, metavar="LOCATION", help="named scene location"); scene_create.add_argument("--character", dest="characters", metavar="CHARACTER", action="append", required=True, help="present character; repeat for each independent character")
    author_common(scene_create); author_time(scene_create)
    scene_advance = scene_sub.add_parser("advance", help="advance an existing scene and the shared active-front cursor", description="Advance an active scene and the shared active-front cursor.")
    scene_advance.add_argument("scene", metavar="SCENE", nargs="?", help="active scene reference; omit only when exactly one scene is active"); scene_advance.add_argument("--location", metavar="LOCATION", help="optional new scene location"); scene_advance.add_argument("--character", dest="characters", metavar="CHARACTER", action="append", help="not accepted here; use author move --scene to reconcile an independently present character")
    author_common(scene_advance); author_time(scene_advance, required=True)
    scene_close = scene_sub.add_parser("close", help="close a scene at a story time", description="Close a named active scene at an exact story-time coordinate.")
    scene_close.add_argument("scene", metavar="SCENE", help="active scene reference"); scene_close.add_argument("--location", metavar="LOCATION", help="optional final scene location")
    author_common(scene_close); author_time(scene_close, required=True)

    move = author_sub.add_parser("move", help="record independent character location effects in one atomic event", description="Record named characters' independent location effects in one atomic event.")
    move.add_argument("--character", dest="characters", metavar="CHARACTER", action="append", required=True, help="character to move; repeat as needed")
    move.add_argument("--location", required=True, metavar="LOCATION", help="destination location")
    move.add_argument("--scene", metavar="SCENE", help="active scene whose location/presence should be reconciled atomically")
    move.add_argument("--title", metavar="TITLE", help="event title")
    author_common(move); author_time(move)

    conversation_author = author_sub.add_parser("conversation", help="create a conversation or append a spoken line or action beat", description="Create an active conversation or append a spoken line or action beat.")
    append_sub = conversation_author.add_subparsers(dest="author_subject_command", required=True, parser_class=WedlArgumentParser)
    create_conversation = append_sub.add_parser("create", help="create an active conversation in an active scene", description="Create an active conversation in a named or sole active scene.")
    create_conversation.add_argument("title", metavar="TITLE", help="conversation title")
    create_conversation.add_argument("--scene", metavar="SCENE", help="active scene reference; omit only when exactly one scene is active")
    create_conversation.add_argument("--character", dest="characters", metavar="CHARACTER", action="append", help="independently present participant; repeat to select a subset (default: all present characters)")
    author_common(create_conversation); author_time(create_conversation)
    append_line = append_sub.add_parser("append", help="append a speech line or action beat to an active conversation")
    append_line.description = "Append a speech line or action beat to an active conversation by character name. Use a raw changeset for historical or closed conversation edits."
    append_line.add_argument("conversation", metavar="CONVERSATION", help="active conversation reference"); append_line.add_argument("text", metavar="TEXT", help="speech text or action description")
    append_line.add_argument("--kind", choices=["speech", "action"], default="speech", help="beat kind (default: speech)")
    append_line.add_argument("--speaker", metavar="CHARACTER", help="speaker for speech")
    append_line.add_argument("--addressee", metavar="CHARACTER", help="optional speech addressee")
    append_line.add_argument("--actor", dest="actors", metavar="CHARACTER", action="append", help="action actor; repeat as needed")
    append_line.add_argument("--interrupt-last", action="store_true", help="mark this spoken line as interrupting the latest spoken line")
    author_common(append_line); author_time(append_line)

    hypothesis_author = author_sub.add_parser("hypothesis", help="record or resolve a non-canonical author possibility", description="Record or resolve an explicitly non-canonical author possibility.")
    hypothesis_sub = hypothesis_author.add_subparsers(dest="author_subject_command", required=True, parser_class=WedlArgumentParser)
    hypothesis_create = hypothesis_sub.add_parser("create", help="record a non-canonical possibility", description="Record a named non-canonical possibility without changing canon.")
    hypothesis_create.add_argument("title", metavar="TITLE", help="title for the possibility"); hypothesis_create.add_argument("--statement", required=True, metavar="TEXT", help="proposed non-canonical statement"); hypothesis_create.add_argument("--subject", dest="subjects", action="append", required=True, metavar="LORE", help="named subject; repeat as needed"); hypothesis_create.add_argument("--alternative", dest="alternatives", action="append", required=True, metavar="TEXT", help="possible reading; repeat as needed"); hypothesis_create.add_argument("--context", required=True, metavar="TEXT", help="why this possibility is being recorded"); hypothesis_create.add_argument("--scene", metavar="SCENE", help="optional related scene"); hypothesis_create.add_argument("--event", metavar="EVENT", help="optional related event"); hypothesis_create.add_argument("--location", metavar="LOCATION", help="optional related location"); hypothesis_create.add_argument("--timeline", metavar="TIMELINE", help="optional related timeline")
    author_common(hypothesis_create)
    hypothesis_adopt = hypothesis_sub.add_parser("adopt", help="mark a possibility adopted without changing canon", description="Mark a possibility adopted while leaving canonical records unchanged.")
    hypothesis_adopt.add_argument("hypothesis", metavar="HYPOTHESIS", help="possibility reference"); hypothesis_adopt.add_argument("--canonical-record", dest="canonical_records", action="append", required=True, metavar="LORE", help="already-settled supporting record; repeat as needed"); hypothesis_adopt.add_argument("--note", metavar="TEXT", help="optional adoption note")
    author_common(hypothesis_adopt)
    hypothesis_reject = hypothesis_sub.add_parser("reject", help="retain a rejected possibility as author reasoning", description="Retain a rejected possibility as non-canonical author reasoning.")
    hypothesis_reject.add_argument("hypothesis", metavar="HYPOTHESIS", help="possibility reference"); hypothesis_reject.add_argument("--note", metavar="TEXT", required=True, help="reason the possibility was rejected")
    author_common(hypothesis_reject)

    request = author_sub.add_parser("request", help="preview or apply an authoring intent JSON document (HTTP-equivalent)", description="Preview or apply an HTTP-equivalent authoring intent JSON document.")
    request_sub = request.add_subparsers(dest="author_subject_command", required=True, parser_class=WedlArgumentParser)
    for name in ("preview", "apply"):
        command = request_sub.add_parser(name, help=f"{name} an authoring intent JSON document", description=f"{name.capitalize()} an HTTP-equivalent authoring intent JSON document.")
        command.add_argument("file", metavar="FILE", help="authoring intent JSON file, or - to read from standard input")
        _add_repo_argument(command)
        if name == "apply":
            confirmation = command.add_mutually_exclusive_group()
            confirmation.add_argument("--confirm", metavar="TOKEN", help="confirmationToken returned by a successful authoring preview")
            confirmation.add_argument("--yes", action="store_true", help="unsafe CLI-only bypass of preview confirmation")

    changeset = commands.add_parser("changeset", help="scaffold, inspect, preview, or atomically apply canonical story changes", description="Start with a current-HEAD scaffold, inspect the supported operation schema, preview an edited JSON request, then apply its confirmationToken. Apply writes canonical Markdown, creates one Git commit, and recompiles.")
    changeset_sub = changeset.add_subparsers(dest="changeset_command", required=True, metavar="ACTION", title="changeset actions", parser_class=WedlArgumentParser)
    scaffold = changeset_sub.add_parser("scaffold", help="write a valid current-HEAD starter changeset", description="Create a valid wedl-changeset/v1 starter bound to the current repository HEAD. It contains a no-op entity.update so it can be previewed immediately; replace that operation while retaining the envelope. Start with `wedl changeset scaffold --output change.json`, inspect `wedl changeset schema`, then run `wedl changeset preview change.json`.")
    _add_repo_argument(scaffold)
    scaffold.add_argument("--output", metavar="FILE", default="-", help="new JSON file to create; use - or omit for stdout (existing files are never overwritten)")
    changeset_sub.add_parser("schema", help="show supported changeset operations and envelope fields", description="Emit concise machine-readable wedl-changeset/v1 envelope and operation guidance. Use it alongside `wedl changeset scaffold` before editing a request.")
    for name in ("preview", "apply"):
        action = "inspect its effects without writes" if name == "preview" else "write it as one Git commit and recompile"
        description = f"Read a JSON changeset and {action}."
        if name == "apply":
            description += " First run preview and pass its confirmationToken with --confirm. --yes is an explicit unsafe bypass for deliberate one-shot automation."
        cmd = changeset_sub.add_parser(name, help=f"{name} a changeset", description=description)
        cmd.add_argument("file", metavar="FILE", help="changeset JSON file, or - to read JSON from standard input"); _add_repo_argument(cmd); cmd.add_argument("--use-current-head", action="store_true", help="allow the current repository HEAD when the changeset omits an expected head; weakens expected-HEAD protection")
        if name == "apply":
            confirmation = cmd.add_mutually_exclusive_group()
            confirmation.add_argument("--confirm", metavar="TOKEN", help="confirmationToken returned by a successful changeset preview")
            confirmation.add_argument("--yes", action="store_true", help="unsafe explicit bypass of preview confirmation; intended only for deliberate automation")
    serve = commands.add_parser("serve", help="start the local browser interface", description="Serve the local WEDL web interface. The command checks the repository and local port before it blocks, then reports the exact ready URL. For safety, only loopback hosts are accepted.")
    _add_repo_argument(serve); serve.add_argument("--host", metavar="HOST", default="127.0.0.1", help="loopback bind host (default: 127.0.0.1); choose 127.0.0.1, localhost, or ::1"); serve.add_argument("--port", metavar="PORT", type=integer_between(1, 65535), default=8765, help="local TCP port (default: 8765); must be from 1 to 65535"); serve.add_argument("--open", action="store_true", help="open the local interface once after the server is ready (default: do not open a browser)")
    return root



__all__ = ["WedlArgumentParser", "integer_at_least", "integer_between", "parser"]
