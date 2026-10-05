# Implemented CLI and LLM Command Protocol

This document describes the implemented WEDL 0.6 command surface. The earlier
Haskell/Elm command-envelope proposal has been archived from this document: it
described proposed verbs and HTTP envelopes, not commands accepted by the
current Python CLI. The parser and `wedl --help` are the executable source of
truth.

## Repository selection and references

`wedl consequences FILE --repo PATH` verifies a closed
`wedl-event-consequences/v1` JSON request; use `-` for stdin and `--compact`
for compact JSON. Supply an exact full `revision` SHA, an `event` reference,
explicit `at: {timeline, tick, order}` with canonical decimal strings, and
`limit` from 1 through 1000. Optional `expectations: {policy, items}` uses
`required` or `advisory` and the shared closed check predicates. Public requests
cannot provide identity, mode, audience or scope grants. Duplicate JSON members
at any depth are rejected in both files and stdin.

The read validates source and reports event-local T changes plus consequences
through H without compiling, writing source, or publishing caches/receipts.
An `ok` report exits 0 even when required checks produce `applyAllowed: false`;
invalid, unavailable and limit outcomes retain their closed JSON on stderr
and exit 2. The corresponding authenticated HTTP route is
`POST /api/events/consequences`; parser-derived discovery and OpenAPI describe
the same body and response components.

Read commands accept `--repo PATH`. A relative path is resolved from the
current working directory. Therefore, use `--repo frontiersmen` from its parent
directory, and `--repo .` (or omit the option) after entering the repository.

Entity reference positions accept a stable ID, exact title, alias, or the
slugified form of an exact title or alias. For example, `Rhea`, `Rhea Marrow`,
and `rhea-marrow` resolve to the same Frontiersmen character. Unknown
references include close suggestions; ambiguous references include candidate
IDs.

## Command invocation

The primary installation is the editable install described in the
[README](../README.md#install), which provides the `wedl` console script. For a
checked-out source tree whose console script is unavailable, use this
PowerShell fallback:

```powershell
$env:PYTHONPATH = 'src'
python -m wedl.cli --help
python -m wedl.cli status --repo frontiersmen
```

This fallback only makes the source package importable. Its runtime dependencies
must still be installed in the Python interpreter; `PYTHONPATH` does not install
them.

Run `wedl --help` first to discover the command families, then use (for
example) `wedl search --help` or `wedl conversation show --help` before
automating a specific command. Help is deliberately human-readable and calls
out perspective and temporal constraints. `--compact` is a global option and
must appear before the command:

```text
wedl --compact status --repo frontiersmen
wedl --compact search "amber manifestations" --repo frontiersmen --tick 195
```

## Read-only CLI surface

```text
wedl init PATH [--empty | --example ash-archive|frontiersmen] [--no-git]
  [--profile state|fts|vector|hybrid] [--vector-provider ...]
wedl completion bash|powershell
wedl status [--repo PATH]
wedl validate [--repo PATH]
wedl compile [--repo PATH] [--force] [--profile state|fts|vector|hybrid]
  [--vector-provider ...] [--vector-model ...] [--vector-dimensions N]
  [--vector-max-features N]
wedl entity list [--repo PATH] [--require-compiled] [--kind KIND] [--text TEXT]
wedl entity show ENTITY [--repo PATH] [--require-compiled]
wedl state ENTITY [--repo PATH] [--require-compiled] --tick N [--timeline NAME] [--order N]
wedl knowledge CHARACTER [--repo PATH] [--require-compiled] --tick N [--timeline NAME] [--order N]
wedl interactions FIRST SECOND [--repo PATH] [--require-compiled]
wedl story-points [--repo PATH] [--require-compiled] [--scene SCENE] [--tick N]
  [--timeline NAME] [--order N]
wedl search QUERY [--repo PATH] [--require-compiled] [--mode fts|vector|hybrid]
  [--perspective author|character] [--character CHARACTER] [--scene SCENE]
  [--limit N] [--include-text] [--tick N] [--timeline NAME] [--order N]
  [--all-time]
wedl context CHARACTER [--repo PATH] [--require-compiled] [--scene SCENE]
  [--perspective character|author|dramatic-irony] [--query TEXT]
  [--max-characters N] [--max-items N] [--mode fts|vector|hybrid]
  [--tick N] [--timeline NAME] [--order N]
wedl conversation show CONVERSATION [--repo PATH] [--require-compiled]
  [--perspective author|character] [--character CHARACTER] [--tick N]
  [--timeline NAME] [--order N] [--all-time]
wedl chronology catalog [--repo PATH] [--require-compiled]
wedl chronology format|convert|search|story-times FILE [--repo PATH] [--require-compiled]
wedl spatial containment|children|bbox|nearby|adjacency|reachability|path|overlay-as-of FILE [--repo PATH] [--require-compiled]
wedl serve [--repo PATH] [--host LOOPBACK] [--port N]
```

`wedl spatial` reads one raw closed `wedl-spatial/v1` JSON document from FILE
or standard input and prints the same result as its matching local HTTP POST.
The document carries revision/capabilities/limit/cursor plus only that action's
members; StoryTime tick/order are signed decimal strings. Semantic spatial
states are `ok` (exit 0), `invalid`, `unavailable`, `forbidden`, or `limit`
(stderr and exit 2). See the HTTP contract for the exact action/result matrix.
For explicit overlay-ID reads, a hidden overlay and an unknown ID both return
the same `forbidden` `SPATIAL-OVERLAY-001` envelope (HTTP 403; CLI exit 2).
Catalogue reads without an explicit ID still return an empty `ok` result when
no overlay is visible.
When `--require-compiled` finds a missing or stale cache, the ordinary
`compile_required` error keeps its normal exit classification but exposes only
the compile hint; it never includes a local cache or repository path.

The parser rejects invalid option values before it opens a repository or starts
the local server. `entity list --kind` accepts only canonical entity kinds:
`world`, `character`, `knowledge`, `event`, `object`, `environment`, `location`,
`relationship`, `story-point`, `scene`, and `conversation`. `search --limit`
is 1 through 50. `context --max-characters` is at least 1,800 (a particular
packet can still require a larger structural minimum), and `--max-items` is at
least 1. `serve --port` is 1 through 65,535.

`--tick` selects a fictional-time state. When a tick is supplied, `--timeline`
defaults to the world's declared default timeline where
available, and `--order` disambiguates multiple transitions at one tick. An
author search is bounded by default and derives its moment from a selected or
active scene cursor when `--tick` is omitted; it requires `--tick` only when no
usable cursor exists. `--all-time` is the explicit author-only opt-in and
conflicts with every temporal flag. Character search rejects `--all-time`.
Author conversation views default to the
conversation's natural end when no time is supplied, while author `--all-time`
returns its full transcript/recollection history. Search, context, and
conversation views apply both time and perspective constraints before
presenting or ranking data. `story-points --tick N` evaluates at that
coordinate, using the world default timeline if `--timeline` is omitted.
Without `--tick`, `story-points` uses the selected scene cursor (or active
scene cursor); it rejects `--timeline` without `--tick` instead of silently
changing the scene-derived coordinate.

### Compiled-cache reads

The cache-backed reads above rebuild a missing, stale, or incompatible local
compiled cache by default. Add `--require-compiled` when automation must not
create or update `.wedl/`: it returns a structured `compile_required` error
instead. The error's `details.cache` reports `missing`, `stale`, or
`incompatible`, the database path, the requested target revision/tree, and any
available compiled metadata. Run `wedl status` first when deciding whether to
compile; its side-effect-free `cacheReadiness` object uses the same states.

## Story-time contract

Read `wedl status` (or `GET /api/status`) before automating temporal queries.
Its additive `timeModel` object reports the default timeline, timeline
declarations and any optional origins, coordinate bounds, and interval policy.
The CLI and API return the same object.

`tick` is a signed 64-bit, unitless ordinal coordinate; negative ticks are
valid for pre-origin history. `order` is a signed 32-bit coordinate that
orders facts occurring at the same tick. `durationSemantics` is `none`: never
infer hours, days, dates, or elapsed time from the numeric difference between
ticks. Timeline origins are descriptive display anchors, not lower bounds.
All bounded temporal intervals are inclusive at both endpoints.

WEDL v0.6 also supports an independent validated chronology model: calendar,
era, and explicit anchor declarations on the world record, plus chronology
annotations on records. Civil, era, range, approximate, conflict, relative,
and display-only duration values belong to that model—not to StoryTime tick
arithmetic. Only explicit anchors can map a chronology date to a StoryTime;
there is no interpolation or tick-to-duration conversion. Use the public
`wedl chronology` read commands or the documented chronology authoring flow;
the precise wire and replacement contract is
[CHRONOLOGY_API_CONTRACT.md](CHRONOLOGY_API_CONTRACT.md).

## Context and conversation boundaries

Character context and character conversation views are not author summaries.
They include only material accessible to the requested character at the
effective story time; conversation output also respects presence and audibility
intervals. `author` and `dramatic-irony` context modes are explicit.

Context is a prose writing packet with a hard serialized `--max-characters`
budget. The minimum is at least 1,800 characters, although structural
requirements for a particular packet can raise it; the CLI reports the exact
minimum. `--query` affects optional context/retrieval ranking only. It cannot
override availability, presence, audience, or temporal rules.

The current JSON output protocols are `wedl-context/v3`, `wedl-search/v5`, and
`wedl-conversation/v2`. Search v5 and conversation v2 add `timeScope`: either
`{"mode":"as-of","at":{...}}` or the explicit `{"mode":"all-time"}`.
Context v3 adds a `focus` object alongside the existing
packet fields. It reports the effective query terms, whether focus was applied,
and—in character packets—the perspective-safe retrieval eligibility and mode.
`focus` describes optional-selection ranking; it does not relax the same
availability, presence, audience, or temporal boundaries.

## Canonical mutations

The CLI mutation interface accepts a JSON changeset file; the local HTTP API
accepts the same changeset object directly as documented below:

```text
wedl changeset preview FILE [--repo PATH] [--use-current-head]
wedl changeset apply FILE [--repo PATH] [--use-current-head] --confirm TOKEN
wedl author chronology replace FILE --repo PATH --expected-head HEAD
  [--summary TEXT] [--idempotency-key KEY] [--confirm TOKEN | --yes]
```

`preview` validates a candidate change and reports the resulting effects without
committing it. A successful preview returns `confirmationToken`: a versioned,
deterministic checksum of the canonical complete JSON payload, `requestHash`,
and the resolved Git HEAD. Supply that exact value out-of-band through
`apply --confirm TOKEN`; a missing, changed-payload, or stale-HEAD token is
rejected before writes. The checksum is proof of preview, not authorization.
`--yes` is the mutually-exclusive explicit unsafe bypass for deliberate
non-interactive one-shot automation, except for all `spatial.*` authoring
intents: those always require their exact preview confirmation. `apply` validates, writes canonical
Markdown, creates one Git commit, and recompiles the resulting revision. See the current changeset
examples under [`examples/`](../examples/) for the file format. A changeset file
uses the current `wedl-changeset/v1` input document version. The older
`wedl-command/v1` and `wedl-query/v1` command-envelope proposal is not accepted
by the 0.6 CLI. Use `--use-current-head` only when the changeset deliberately
omits an expected head: it weakens the normal expected-HEAD protection against
applying a change to an unintended revision.

The additive consequence operations and event/preview semantic reports are
specified in [EVENT_CONSEQUENCES_CONTRACT.md](EVENT_CONSEQUENCES_CONTRACT.md).
That contract defines downstream support; use installed schema discovery and
CLI help to determine which variants are executable. It preserves the current
changeset/source versions and distinguishes event-local views from a base/candidate
comparison at one explicit horizon.

`wedl author chronology replace` is a complete catalog and/or per-record
annotation replacement, not granular chronology CRUD. It previews by default,
requires the exact audited `--expected-head`, and accepts the normal
`--confirm TOKEN` replay of that preview (or CLI-only `--yes`). The resulting
candidate uses the same validation, atomic commit, compilation, and receipt
replay guarantees as other authoring actions. See
[CHRONOLOGY_API_CONTRACT.md](CHRONOLOGY_API_CONTRACT.md) for accepted values,
temporary IDs, legacy upgrade behavior, and HTTP parity.

Automation must preserve the exact previewed JSON and pass the token out of
band, without an interactive prompt. For example:

```bash
preview=$(wedl --compact changeset preview change.json)
token=$(printf '%s' "$preview" | python -c "import json, sys; print(json.load(sys.stdin)['confirmationToken'])")
wedl --compact changeset apply change.json --confirm "$token"
```

The confirmation checksum does not replace the authenticated
`X-Wedl-Token` required by the HTTP mutation endpoints.

## Local HTTP workflow

`wedl serve` also exposes a parser-derived local HTTP API. Its complete route,
parameter, authentication, and OpenAPI contract is documented in
[HTTP_API.md](HTTP_API.md). HTTP changesets are the raw `wedl-changeset/v1`
JSON object, never a wrapper containing a file path or `payload` property.
`POST /api/changesets/scaffold` and `preview` require `X-Wedl-Token`; `apply`
also requires the reviewed preview token in `X-Wedl-Confirmation`. The header
is the only HTTP confirmation channel: body values, CLI `--yes`, and CLI
`--use-current-head` are intentionally excluded. The server plans against its
current HEAD and serializes writers. Review the preview result before applying
the same canonical JSON value; member order is immaterial, but any canonical
payload change invalidates confirmation. The token proves that review, not
authorization.

## Output and automation

`wedl changeset schema` is repository-free concrete discovery. Add `--repo PATH`
for validated world declarations at current HEAD, or also `--revision SHA` to pin
an exact lowercase 40-character revision. The selected source is loaded once
without publishing a cache. `--revision` requires `--repo`.

Preview a `consequence.batch` intent with `wedl author request preview FILE --repo
PATH`, then apply the unchanged intent with `wedl author request apply FILE
--repo PATH --confirm TOKEN`. FILE may be `-` for standard input. The batch uses
the installed operation schemas, explicit times and declared references;
`expectation.check` checks the complete final candidate. Automation also requires
that exact preview token; `--yes` cannot bypass batch confirmation. OpenAPI
publishes the same unwrapped body through the existing authoring routes.

Data commands print formatted JSON by default; pass global `--compact` before
the subcommand for compact JSON. `serve` writes a human-ready URL line when it
has bound successfully, or its one-line JSON readiness object with `--compact`.
`completion bash` and `completion powershell` are the intentional exception:
they write a raw, sourceable shell script to stdout so callers can evaluate it.
They reject `--compact` with the normal structured `usage_error` diagnostic;
the script itself is never JSON-encoded.

Every malformed invocation—including an unknown command, an omitted required
argument, an invalid option value, and conflicting options—writes one JSON
diagnostic to stderr and exits with status 2 instead of using argparse's
plain-text error output. The stable parser-error shape is:

```json
{
  "code": "usage_error",
  "message": "the parser's actionable explanation",
  "details": {
    "context": {
      "command": "wedl search",
      "usage": "usage: wedl search ...",
      "help": "wedl search --help"
    }
  }
}
```

`code` identifies the error class, `message` is the parser explanation, and
`details.context` identifies the command whose grammar failed, its current
usage line, and the exact command-specific help invocation. The stable
`details.context.help` value is always `<command> --help` (for example,
`wedl --help` for root grammar failures and `wedl search --help` for a nested
command). Other runtime diagnostics retain their established JSON error shapes
and may provide operation-specific details. Automation should parse JSON from
stdout for JSON-producing commands and from stderr on failure; completion
output must instead be treated as shell source. Use `wedl --help` or relevant
subcommand help to discover accepted flags rather than relying on the older
command-envelope proposal.
