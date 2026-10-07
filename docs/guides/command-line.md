# Command-line usage and automation

Install from the [source README](../../README.md#install), then run `wedl --help`.
Use command-specific help before assembling requests, for example:

```text
wedl search --help
wedl conversation show --help
wedl generational parents --help
wedl migrate preview --help
```

## Starting a world

`wedl init my-world` starts the completed Ash Archive in v0.7. Choose
Frontiersmen for an open campaign:

```text
wedl init my-world --example frontiersmen
```

The `ash-archive-v07` and `frontiersmen-v07` names remain aliases for the
same examples. Use `wedl init my-world --empty` for a blank v0.3 world.
Initialization requires an empty destination; it does not upgrade an
existing world.

## Running from a source checkout

The [installation instructions](../../README.md#install) provide the `wedl`
console script. If PowerShell blocks virtual environment activation, invoke
that environment directly:

```powershell
& .\.venv\Scripts\python.exe -m pip install -e .
& .\.venv\Scripts\wedl.exe --help
```

If the console script is unavailable, run the module from the source checkout.
Install its runtime dependencies in the interpreter first; `PYTHONPATH` only
makes the source importable.

```powershell
$env:PYTHONPATH = 'src'
python -m wedl.cli --help
python -m wedl.cli status --repo PATH
```

On managed Windows machines, a restricted temporary directory can make `venv`
fail during `ensurepip`. Use a disposable directory for the current terminal,
keeping the previous TEMP and TMP values so you can restore them afterward:

```powershell
$previousTemp = $env:TEMP
$previousTmp = $env:TMP
New-Item -ItemType Directory -Path .wedl-build-tmp -ErrorAction Stop | Out-Null
$env:TEMP = (Resolve-Path .wedl-build-tmp).Path
$env:TMP = $env:TEMP
try {
  python -m venv .venv
  & .\.venv\Scripts\python.exe -m pip install -e .
} finally {
  $env:TEMP = $previousTemp
  $env:TMP = $previousTmp
}
```

Remove only the disposable build directory you created once it is no longer
needed. See the [testing guide](testing.md#temporary-and-retained-outputs) for
scratch ownership and retention.

## Repository and output selection

Use `--repo PATH` to select a world repository. Relative paths start at your
current working directory; use `--repo .` after entering that repository.
Entity arguments accept a stable ID, exact title, alias or title/alias slug.
Use global `--compact` before the subcommand for compact JSON:

```text
wedl status --repo PATH
wedl --compact status --repo PATH
wedl --compact search "amber manifestations" --repo PATH --tick 195
```

Data commands normally print JSON. Read JSON diagnostics from stderr on failure.
Completion commands instead print a raw, sourceable shell script and
reject `--compact`; evaluate their output as shell source.

## Shell completion

Completion is generated from the installed CLI parser, so command and option
suggestions stay aligned with the version you run. It is deterministic and
only suggests commands, options, and fixed option choices—never repository
paths, caches, or authored entity names.

The one-session commands below require `wedl` to be available in that shell,
normally after activating the project's virtual environment.

For the current Bash session:

```bash
eval "$(wedl completion bash)"
```

For a persistent Bash setup, create a stable script while the intended virtual
environment is active, then have `~/.bashrc` source that saved file. The profile
does not invoke a bare `wedl`, so it remains loadable when that environment is
not active:

```bash
completion_file="$HOME/.local/share/wedl/wedl-completion.bash"
mkdir -p "$(dirname "$completion_file")"
"$VIRTUAL_ENV/bin/wedl" completion bash > "$completion_file"
printf '\nsource "$HOME/.local/share/wedl/wedl-completion.bash"\n' >> "$HOME/.bashrc"
source "$HOME/.bashrc"
```

Regenerate the saved file after upgrading the virtual environment. If it is not
active while generating the file, replace `"$VIRTUAL_ENV/bin/wedl"` with the
quoted absolute path to that environment's `wedl` executable.

For the current PowerShell session:

```powershell
Invoke-Expression (& wedl completion powershell | Out-String)
```

For a persistent PowerShell setup, create the script while the intended virtual
environment is active, then have the profile source that saved script. The
profile does not need `wedl` on `PATH` when it later loads:

```powershell
$completionFile = Join-Path $HOME '.local\share\wedl\wedl-completion.ps1'
New-Item -ItemType Directory -Path (Split-Path -Parent $completionFile) -Force | Out-Null
& (Join-Path $env:VIRTUAL_ENV 'Scripts\wedl.exe') completion powershell |
  Set-Content -Path $completionFile -Encoding utf8
if (-not (Test-Path -LiteralPath $PROFILE)) {
  New-Item -ItemType File -Path $PROFILE -Force | Out-Null
}
Add-Content -Path $PROFILE -Value '. "$HOME\.local\share\wedl\wedl-completion.ps1"'
. $PROFILE
```

Regenerate the saved file after upgrading the virtual environment. If it is not
active while generating the file, use a quoted absolute path instead, for
example `& 'C:\path with spaces\.venv\Scripts\wedl.exe' completion powershell`.

## Find the right family

| Need | Command/help entry |
| --- | --- |
| New world | `wedl init --help` (`--empty`, `ash-archive`, `frontiersmen`, `ash-archive-v07`, `frontiersmen-v07`) |
| Validation/cache | `wedl validate --help`, `wedl compile --help`, `wedl status --help` |
| Entities/state/knowledge | `wedl entity list --help`, `wedl entity show --help`, `wedl state --help`, `wedl knowledge --help` |
| Conversation/context/search | `wedl conversation show --help`, `wedl context --help`, `wedl search --help` |
| Story navigation | `wedl interactions --help`, `wedl story-points --help`, `wedl timeline --help`, `wedl whereabouts --help`, `wedl causal --help`, `wedl hypotheses --help` |
| Narrative grouping | `wedl threads --help`, `wedl thread-memberships --help`; search `--thread-id`, context `--recall-thread-id` |
| Calendar chronology | `wedl chronology --help` and [chronology usage](chronology.md) |
| Spatial reads | `wedl spatial --help`; use the operation's JSON request file or `-` for stdin |
| Generational reads/authoring | `wedl generational --help`, [generational usage](generational.md) |
| Consequences | `wedl consequences --help`, [consequence walkthrough](event-consequence-preview.md) |
| Source changes | `wedl changeset --help`, `wedl author --help` |
| Migration/rollback | `wedl migrate --help`, [migration and recovery](migration-and-recovery.md) |
| Local browser/API | `wedl serve --help`, [HTTP usage](http-api.md) |

`entity list --kind` includes world, character, knowledge, event, object,
environment, location, relationship, story-point, scene, conversation,
organization, parentage, union, affiliation, legacy, tenure, claim and
vital-history. Use `wedl hypotheses` for author possibilities.

## Choose a time and viewpoint

Supply `--tick`, and optionally `--timeline` and `--order`, for an explicit
story moment. `wedl status` reports the world's time model. Author search
without `--tick` uses a selected or active scene; supply a tick if no cursor
is available. Use author `--all-time` only without temporal flags. For character
search, select `--perspective character --character CHARACTER`. Context defaults
to character perspective; use explicit author or dramatic-irony options when
needed. Conversation author views without a time use that conversation's end.

`wedl generational ACTION FILE --repo PATH` takes a raw JSON request with an
explicit mode. For character mode also pass `--viewpoint CHARACTER`, chosen by
the local author. Public actions are parents, ancestors, descendants, relatives,
union, organization, legacy, vital, search, context, discover, labels,
character-unions and organization-legacies. Internal knowledge-history is not
a command. Use scaffold/schema to discover authoring inputs.

Thread catalogue/filter commands currently admit v0.5/v0.6 repositories.
Repeat sorted unique declared `--thread-id` or `--recall-thread-id` values.
Their current public admission does not include v0.7, whose canonical source
still retains the grouping declarations and memberships.

Add `--require-compiled` to cache-backed reads when automation must stop for
a missing, stale or incompatible cache. Run `wedl status` to inspect readiness
and `wedl compile --repo PATH` when rebuilding is intended.

## Preview changes before applying

Create a current-HEAD starter and inspect the installed schema:

```text
wedl changeset scaffold --repo PATH --output change.json
wedl changeset schema --repo PATH
wedl changeset preview change.json --repo PATH
wedl changeset apply change.json --repo PATH --confirm TOKEN
```

Edit the starter before previewing; review the preview's changes, diagnostics
and confirmation token. Apply the unchanged request with that exact token.
For semantic intents, use:

```text
wedl author request preview intent.json --repo PATH
wedl author request apply intent.json --repo PATH --confirm TOKEN
```

Keep the audited expected head, idempotency key and payload. Any edited request
needs a new preview. Spatial and generational intents and `consequence.batch`
always require their preview token; `--yes` cannot bypass it. Other commands
that expose `--yes` label it an unsafe bypass; prefer confirmed preview. Use
`--use-current-head` only when intentionally accepting its weaker head guard.

From the WEDL development/source checkout, read the command/output contract:
`adrai --repo WEDL_SOURCE_CHECKOUT show A01M494PM59C6BG1W7K1CXXYR0H --json`.

## Read or write architecture with ADRAI

Run ADRAI against the WEDL development/source checkout. Search by the affected
repository-relative path, then read the returned IDs:

```text
adrai --repo WEDL_SOURCE_CHECKOUT search --mode fts --file .hmem.workspace --include-obsolete --limit 100 --json
adrai --repo WEDL_SOURCE_CHECKOUT search --mode fts --file src/wedl/cli.py --include-obsolete --limit 100 --json
adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json
```

Read applicable decisions before changing architecture. Draft the decision body
in a temporary owned file, inspect `adrai create --help` or `adrai amend --help`,
and supply truthful actor/model attribution and the intended domains/path scope.
For an amendment, copy the exact current `state_token` from `show` into
`--expect`. Review the body and complete mutation sequence before invoking the
write: ADRAI can create Git commits automatically. Check its returned record,
commit and status afterward. Use CLI mutations for decision changes; editing
managed record files bypasses that workflow. Keep architecture in ADRAI rather
than adding an architectural document or duplicate decision index under docs.
