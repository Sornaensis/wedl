# wedl 0.6.0

`wedl` is a Git- and SQLite-backed narrative-state tool for managing characters,
events, objects, locations, environments, directional relationships,
subjective knowledge, scenes, story points, and **verbatim conversations with
character-specific recollections**.

Markdown committed to Git is canonical. SQLite is a disposable,
revision-stamped read model used for temporal queries, FTS5 search, normalized dense-vector retrieval, compact LLM writing packets, and the local browser
interface.

## What works

- Strict UTF-8 Markdown/YAML records with duplicate-key rejection.
- Loading the worktree or any Git revision without checking it out.
- Blob-keyed parsed-source caching and batched Git object reads.
- Temporal replay of character, object, and location state.
- Subjective knowledge and asymmetric relationship histories.
- Timed scene participation and per-character observations.
- Historical story-point transitions with derived eligibility.
- First-class conversations containing immutable spoken turns and action beats.
- Separate recollections describing how each character remembers an exchange.
- Separate `state`, `fts`, `vector`, and `hybrid` compilation profiles.
- Perspective filtering before FTS, vector scoring, or hybrid rank fusion.
- Normalized, content-deduplicated dense vectors with independent full-corpus vector retrieval.
- Prose-first, hard-budgeted LLM writing packets with compact provenance.
- Atomic, expected-HEAD-guarded Git changesets and idempotent retries.
- SQLite compilation with exact-revision reuse, retained databases, phase
  timing, one-pass interaction projection, and content-addressed embedding reuse.
- Optional strict compiled-cache reads for automation that must not trigger an
  implicit local rebuild.
- A local FastAPI service and bundled inspection/authoring interface.
- Horizon-aware calculated character prominence in whereabouts: a disposable,
  noncanonical 40/25/20/15 scene/POV/event/relationship navigation aid that
  is never written to Markdown, frontmatter, changesets, or source schemas.
- A **262-record completed Ash Archive fixture** spanning a full mystery and epilogue.
- A **308-record open-campaign Frontiersmen fixture** spanning arrival, caravan duty, a slaughtered crossroads watch, amber manifestations, mine collapse, two days underground, the Tree King, blood-sport escape, the Drowned Waymark, and an uncertain southward road.
- Generated stress worlds at 594, 4,302, and 10,662 records.
- A local-only, preview-confirmed source migration/recovery kernel for the
  narrow v0.3-to-v0.5 and quarantined v0.4 cases; see
  [Migration and recovery](docs/MIGRATION_AND_RECOVERY.md).
- Calendar and historical chronology with explicit conversion anchors,
  qualitative uncertainty, and a 500-year packaged conformance fixture. Ticks
  order replay and never imply elapsed calendar time; see
  [Chronology rollout](docs/CHRONOLOGY_ROLLOUT.md).
- Compiled-only `wedl-spatial/v1` reads for authored containment, geometry,
  directed routes, and horizon-authorized overlays; see the
  [spatial transport contract](docs/HTTP_API.md#spatial-reads). Seven closed
  spatial source intents use the existing preview-confirmed authoring workflow.

## Install

```bash
python -m venv .venv

# Linux or macOS
source .venv/bin/activate

# Windows PowerShell
.\.venv\Scripts\Activate.ps1

python -m pip install -e ".[dev]"
```

Python 3.11 or newer and Git 2.x are required; Python 3.13 is a supported and
regularly exercised choice. If PowerShell prevents activation, do not change a
machine-wide execution policy: invoke the venv interpreter directly instead:

```powershell
& .\.venv\Scripts\python.exe -m pip install -e ".[dev]"
& .\.venv\Scripts\wedl.exe --help
```

### Source-checkout fallback

The editable installation above is the primary way to run WEDL. If its console
script is unavailable, run the checked-out module instead. Its runtime
dependencies must still already be installed in the Python interpreter you
use; setting `PYTHONPATH` does not install them.

```powershell
$env:PYTHONPATH = 'src'
python -m wedl.cli --help
python -m wedl.cli status --repo frontiersmen
```

On some managed Windows machines, `venv` can fail during `ensurepip` with a
`PermissionError` because the system temporary directory is restricted. Create
and use a disposable directory inside this workspace for that terminal only:

```powershell
New-Item -ItemType Directory -Force .wedl-build-tmp | Out-Null
$env:TEMP = (Resolve-Path .wedl-build-tmp)
$env:TMP = $env:TEMP
py -3.13 -m venv .venv
& .\.venv\Scripts\python.exe -m pip install -e ".[dev]"
```

Use another compatible Python command instead of `py -3.13` when necessary.
This workaround neither changes global policy nor removes files outside the
workspace.

## Start with either executable story

```bash
# Completed civic-archive mystery
wedl init ash-archive --example ash-archive

# Open monster-hunting campaign on a remote peninsula
wedl init frontiersmen --example frontiersmen
```

To opt into the coordinated v0.7 source envelope for a fresh world, use
`--example ash-archive-v07` or `--example frontiersmen-v07`. These are separate
copies; the legacy names and default still use their pinned source versions.
The [migration guide](docs/MIGRATION_AND_RECOVERY.md#packaged-examples) covers
reproduction, confirmed conversion of an existing repository, and recovery.

The `--repo` path is resolved from the shell's current working directory. From
the parent directory, use `--repo frontiersmen`; after `Set-Location
frontiersmen`, use `--repo .` (or omit it). Do not pass `--repo frontiersmen`
from inside that repository: that would mean a nested `frontiersmen` path.

From the parent directory, run the first-time workflow: initialize, validate
and inspect the repository, compile the chosen retrieval profile, read one
time-bounded fact, then serve the local interface.

```bash
wedl init frontiersmen --example frontiersmen
wedl validate --repo frontiersmen
wedl status --repo frontiersmen
wedl compile --repo frontiersmen --profile hybrid --vector-provider lsa
wedl state Rhea --repo frontiersmen --timeline main --tick 210
wedl serve --repo frontiersmen
```

You can instead enter the repository before serving it locally:

```bash
cd frontiersmen
wedl serve --repo .
```

The command checks the repository and local port before it blocks, then prints
the exact ready URL (by default `http://127.0.0.1:8765`). Add `--open` to open
that URL once after the server is listening. The server accepts loopback hosts
only; choose `--host ::1` for IPv6, which is reported as `http://[::1]:8765`.
With `--compact`, readiness is a one-line JSON object such as
`{"url":"http://127.0.0.1:8765"}`. Write operations require the per-repository
token stored in `.wedl/session.json`; the same-origin UI loads it automatically.
For programmatic local integrations, see the parser-derived
[HTTP API reference](docs/HTTP_API.md), including the reviewed changeset
preview/apply workflow and its header requirements.

### Shell completion

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
New-Item -ItemType File -Path $PROFILE -Force | Out-Null
Add-Content -Path $PROFILE -Value '. "$HOME\.local\share\wedl\wedl-completion.ps1"'
. $PROFILE
```

Regenerate the saved file after upgrading the virtual environment. If it is not
active while generating the file, use a quoted absolute path instead, for
example `& 'C:\path with spaces\.venv\Scripts\wedl.exe' completion powershell`.

### Compiled-cache policy

Read commands compile a missing, stale, or incompatible local SQLite cache by
default. This keeps interactive exploration straightforward: a first `search`,
`context`, or entity read works without a separate compile step. Automation
that must never create or update `.wedl/` can add `--require-compiled` to any
read that uses the cache (entity list/show, state, knowledge, interactions,
story-points, search, context, or conversation show). Such a read fails with a
structured `compile_required` diagnostic containing the cache state, target
revision, and the command needed to compile it. `wedl status` reports
`cacheReadiness` as `ready`, `missing`, `stale`, or `incompatible` without
rebuilding the cache.

The example begins with Ilyra Sorn’s disappearance and now continues through:

- the seventh-drawer catalog mechanism;
- the Council’s defective inventory writ;
- the bell-tube route to Flood Gallery N;
- a pressure-gate escape;
- opening Ilyra’s heron letter;
- discovery of the Council listening office;
- a public ledger hearing;
- Ilyra’s return;
- the threefold-custody escape and River Gate pursuit;
- the Ember Hall reckoning;
- and the closed epilogue **An Honest Absence**, where the Archive records protected omission without preserving a central reconstruction key.

See [`docs/COMPLETED_STORY_WALKTHROUGH.md`](docs/COMPLETED_STORY_WALKTHROUGH.md) and the novella [`docs/THE_ASH_ARCHIVE.md`](docs/THE_ASH_ARCHIVE.md).

### The Frontiersmen

The second packaged world begins with five new Lantern Pike hires arriving at Keldmouth and continues through:

- unfamiliar amber wages and an eastroad caravan;
- the overnight slaughter of Harrowcross's watch;
- emergency reassignment and abandonment by the caravan;
- wretches that cross walls and answer blood, fear, broken ground, and quick amber;
- a dowsed route to Saint Orra's Mine;
- a cave-in and two days in lower caverns;
- the Sunken Hall's root-memory warning;
- emergence beyond every current map;
- capture by the mute wooden-masked Root Host;
- the Tree King's blood-sport doctrine;
- Moth's silent intervention;
- escape from the immediate pursuit at the Drowned Waymark;
- recovery of a narrowly corroborating Blackroot counterfoil;
- and the active southward journey along an abandoned warden road that is not confirmed to reach Harrowcross.

The Frontiersmen fixture contains **308 records, 19 conversations, 248 verbatim turns, and 76 recollections**. Its current scene is `Southward Cut` at `main 210:0`; it is deliberately a live campaign rather than a closed novel.

See [`docs/FRONTIERSMEN_WORLD_GUIDE.md`](docs/FRONTIERSMEN_WORLD_GUIDE.md), [`docs/FRONTIERSMEN_NARRATIVE_WALKTHROUGH.md`](docs/FRONTIERSMEN_NARRATIVE_WALKTHROUGH.md), and the campaign chronicle [`docs/THE_FRONTIERSMEN.md`](docs/THE_FRONTIERSMEN.md).

## Curated character context

```bash
wedl context "Mara Vale" \
  --scene "An Honest Absence" \
  --query "How should Mara understand custody, omission, and the reopened Archive?" \
  --max-characters 5000
```

The result is a compact Markdown writing packet, not a procedural object graph.
It prioritizes:

1. Voice and immediate intention.
2. The present physical and social situation.
3. The latest audible verbatim exchange.
4. Query-relevant beliefs and uncertainties.
5. Remembered conversations.
6. Relationship pressure.
7. A small amount of authorized retrieval.
8. A perspective boundary and compact provenance footer.

The entire serialized response obeys the requested character budget. Author and
dramatic-irony modes are explicit:

```bash
wedl context "Mara Vale" --scene "An Honest Absence" --perspective author
wedl context "Mara Vale" --scene "An Honest Absence" --perspective dramatic-irony
```

Dramatic-irony output physically separates the character packet from the author
margin. See [`docs/CONTEXT_BRIEFS.md`](docs/CONTEXT_BRIEFS.md).

### Explore the Frontiersmen from the command line

After initialization, these commands are a complete read-only tour. They use
titles and aliases deliberately: an entity argument accepts a stable ID, exact
title, alias, or the slugified form of an exact title or alias (for example,
`Rhea`, `Rhea Marrow`, and `rhea-marrow`). If a reference cannot be resolved,
WEDL returns close matching references; if one is ambiguous, it returns
candidates so you can use an ID.

```bash
# Run from the parent directory containing ./frontiersmen.
wedl status --repo frontiersmen
wedl entity list --repo frontiersmen --kind character
wedl entity show rhea-marrow --repo frontiersmen
wedl state "Amber Reliquary" --repo frontiersmen --timeline main --tick 210
wedl knowledge Rhea --repo frontiersmen --timeline main --tick 210
wedl interactions Rhea Veyra --repo frontiersmen
wedl story-points --repo frontiersmen --scene "Southward Cut"
wedl threads --repo frontiersmen
wedl conversation show "The Southward Cut" --repo frontiersmen \
  --perspective character --character Rhea --timeline main --tick 210
wedl search "Blackroot amber handling evidence" --repo frontiersmen \
  --mode hybrid --perspective character --character Rhea \
  --scene "Southward Cut" --timeline main --tick 210
wedl context Rhea --repo frontiersmen --scene "Southward Cut" \
  --query "What can Rhea justify about the evidence and the road south?" \
  --perspective character --mode hybrid --max-characters 4000
```

`--tick` selects a fictional-time snapshot; when a tick is supplied,
`--timeline` defaults to the world's declared default timeline, and `--order` distinguishes transitions within the
same tick. Author `search` uses a selected scene's (or the active scene's)
cursor when `--tick` is omitted; without a usable scene cursor it requires an
explicit `--tick`. Use its explicit author-only `--all-time` escape hatch only
when future material is intentional. `conversation show` defaults to the conversation's
natural end and is similarly time-sliced; author-only `--all-time` returns its
full history. `story-points --tick N` evaluates at that coordinate and uses the
world default timeline when `--timeline` is omitted. Without `--tick`, it uses
the selected scene's cursor (or the active scene's cursor); `story-points`
rejects `--timeline` without `--tick` rather than silently changing that
scene-derived coordinate. Context has no all-time mode. A split party uses
multiple active scenes on the same canonical timeline: they share an explicit
`world.current_time`, and every active scene's `time.current` must match it.
Generic reads select a scene explicitly when the active front is ambiguous;
character reads can infer that character's unique front.

### Story-time policy

Story time is a unitless ordinal coordinate, not a clock or calendar. Its key
is `(timeline, tick, order)`: `tick` is a signed 64-bit integer (including
negative values for pre-origin history), and `order` is a signed 32-bit
integer that deterministically orders facts sharing a tick. It has no duration
or date-conversion semantics. `wedl status` and `GET /api/status` expose this
contract in their additive `timeModel` field, including the selected default
timeline and its declared labels/origins.

An optional timeline origin is a display anchor only; it does not establish a
first legal tick or prevent earlier history. Bounded scene, participant,
observation, conversation, and environment intervals include both endpoints.
For example, a scene from `main:-20:0` through `main:5:0` includes actions at
both `-20:0` and `5:0`. Keep a real-world calendar, uncertain date, or
"three days later" explanation in authored prose rather than deriving it from
ticks.

`context --max-characters` is a hard serialized-character ceiling. Its minimum
is never below 1,800, but the required structural packet can be larger for a
specific scene or perspective; if so, the usage error reports the exact
minimum. `--query` focuses ranking of optional context and retrieval candidates
without bypassing the perspective boundary. Character mode sees only accessible
knowledge and audible turns; `author` and `dramatic-irony` are explicit modes.

## Conversation provenance

```bash
wedl conversation show "The Choice of Records"

wedl conversation show "The Choice of Records" \
  --perspective character \
  --character "Sister Ansel Marr"
```

The default author view returns the transcript and recollections available at
its effective story time (the conversation's natural end when no `--tick` is
given). Pass author-only `--all-time` to deliberately inspect the complete
history. The
character view returns only turns audible during that character’s presence
interval plus their latest applicable subjective recollection. In `The Choice of Records`, Sister Ansel joins after the opening proposal and therefore receives only the final nine turns. In `River Gate Pursuit`, Rusk enters at tick 184 and cannot retrieve the earlier escape dialogue.

Canonical transcript and memory are deliberately separate:

- speech beats preserve what was actually said, with optional addressees and
  explicit interruption links;
- action beats record visible choreography and party activity without becoming
  quoted dialogue or durable state mutation;
- recollections preserve what one character later thinks happened;
- exact remembered lines cite turn IDs;
- approximate remembered wording may disagree without altering history.

See [`docs/CONVERSATIONS.md`](docs/CONVERSATIONS.md).

## Search profiles and vector providers

Compilation is explicit about which retrieval lanes are built:

```bash
wedl compile --profile state
wedl compile --profile fts
wedl compile --profile vector --vector-provider lsa
wedl compile --profile hybrid --vector-provider lsa
```

- `state` builds temporal and narrative projections only.
- `fts` adds weighted SQLite FTS5 over title, aliases, headings, prose, domain,
  and tags, using Porter stemming plus quoted-phrase support.
- `vector` adds exact dense-vector retrieval without building FTS.
- `hybrid` builds both independent lanes and fuses their ranked unions with
  weighted reciprocal-rank fusion.

The bundled local provider is **TF-IDF plus truncated SVD (LSA)**. These are
real dense, L2-normalized latent-semantic vectors trained on the repository,
not the old feature-hash approximation. Optional providers are available for
`sentence-transformers` and OpenAI-compatible embedding endpoints. The legacy
feature hash survives only as the explicitly named `hash-test` provider.

```bash
wedl search "custody ledger alarm" --mode fts
wedl search "institutional pressure over document validity" --mode vector
wedl search "custody ledger alarm" --mode hybrid \
  --perspective character \
  --character "Mara Vale" \
  --scene "An Honest Absence"
```

FTS and vector retrieval both operate over the complete authorized corpus; the
vector lane is not restricted to FTS candidates. Hybrid mode independently
retrieves from both lanes and reports `lanes`, `ftsRank`, `vectorRank`, BM25,
cosine similarity, and fused score for inspection.

Authorization and fictional-time filtering are applied before either lane
ranks candidates. The local character LSA basis is trained only from globally
public text, then permitted private knowledge and recollections are projected
into that fixed basis. Hidden author text therefore cannot alter a character’s
vector space or visible ranking.

Vectors are stored once per `(model, normalized input hash)` and linked to any
number of documents. In the completed authored fixture, 1,171 document links use 683 unique
normalized vectors. See [`docs/SEARCH_PROFILES.md`](docs/SEARCH_PROFILES.md).

## Narrative changes

```bash
# Create an immediately previewable request tied to the current Git HEAD.
wedl changeset scaffold --output change.json
# Inspect the compact envelope and supported operation vocabulary.
wedl changeset schema
# Edit change.json, then preview its planned source diff without writing.
# Preview emits a confirmationToken bound to this exact JSON and Git HEAD.
wedl changeset preview change.json
# Copy that token out-of-band; apply is otherwise refused without writing.
wedl changeset apply change.json --confirm 'wedl-confirmation/v1:...'
```

`scaffold` writes JSON to stdout by default, or creates a new file when given
`--output`; it never replaces an existing file. The generated request has a
stable idempotency key, a current-HEAD guard, and a no-op `entity.update` so it
round-trips through preview before you replace that operation. Use
`wedl changeset schema` for the supported operations and required envelope
fields. Keep the `expectedHead` from the scaffold unless you intentionally
rebase the request; `--use-current-head` weakens that protection. `preview`
also returns `confirmationToken`, a deterministic checksum of the complete
JSON request (including `requestId` fields), its existing `requestHash`, and
the resolved Git HEAD. Pass it via `apply --confirm TOKEN`; it proves the
exact request was previewed, but is not an authorization credential. `--yes`
is an explicit unsafe bypass for non-interactive, deliberate one-shot
automation. It cannot be combined with `--confirm`.

For automation, keep stdout machine-readable and extract the token from one
compact preview response; no prompt is ever read from stdin:

```bash
preview=$(wedl --compact changeset preview change.json)
token=$(printf '%s' "$preview" | python -c "import json, sys; print(json.load(sys.stdin)['confirmationToken'])")
wedl --compact changeset apply change.json --confirm "$token"
```

Thread grouping is an optional narrative label system inside the one shared
world: it does not create a separate canon, clock, state, or search corpus.
Use target-specific, complete-list replacements in an `entity.update`
`frontmatterPatch`. World declarations use canonical source `threads`:

```json
{"entity":"world_...","frontmatterPatch":{"threads":[{"id":"thread_...","label":"Archive"}]}}
```

Ordinary non-hypothesis record memberships use public `threadIds`; this is
translated before serialization, so the resulting Markdown contains only its
canonical `threads` list. An empty list clears membership:

```json
{"entity":"character_...","frontmatterPatch":{"threadIds":[]}}
```

Each list replaces—not merges with—the previous list. A single changeset may
replace declarations and memberships together, and its complete final
candidate is validated atomically; removing a declaration still used by a
membership fails without writing either change. Hypotheses cannot carry either
grouping key. The submitted changeset itself is unchanged, so preview
confirmation and idempotency hashes always describe the exact request.

Supported operations include:

- `entity.create`, `entity.upsert`, `entity.update`, and `entity.delete`;
- `event.create`;
- `conversation.create`;
- `conversation.turn.append`;
- `conversation.recollection.record`.

Every apply validates the complete candidate world, creates one Git commit,
advances the branch with expected-HEAD protection, recompiles the resulting
revision, preserves unrelated staged files, and returns stable generated IDs.
The HTTP equivalent uses the same raw changeset object, but carries its session
token and review confirmation in headers; see [HTTP API](docs/HTTP_API.md).

### Local source migration and recovery

The local-only migration kernel is intentionally separate from the HTTP API and
browser interface. Always inspect a preview against the exact current commit,
then apply its exact hash and confirmation token:

```bash
wedl migrate preview --mode upgrade-v07 --expected-head "$(git rev-parse HEAD)" --idempotency-key schema-upgrade-1
wedl migrate apply --mode upgrade-v07 --expected-head <HEAD> --source-snapshot-hash <SHA256> --idempotency-key schema-upgrade-1 --confirm <TOKEN>
```

`upgrade-v07` is the coordinated, lossless v0.3/v0.5/v0.6-to-v0.7 transition;
its target capabilities are derived by WEDL and included in the confirmed
request. The earlier `upgrade-v03`, `upgrade-v06`, quarantined `recover-v04`,
and forward `rollback` modes remain available for their established routes. It
never migrates SQLite in place. See the
[migration and recovery runbook](docs/MIGRATION_AND_RECOVERY.md).

### Name-oriented authoring

For routine story work, `wedl author` resolves titles, aliases, and slugs and
then compiles the request to the same raw changeset format above. It never
creates a party or group entity: repeated `--character` values generate one
event with independent per-character location effects.

```bash
# Every command previews by default. Copy preview.confirmationToken and rerun
# the identical command with --confirm to apply it.
wedl author scene create "North Stair Watch" --location "North Stair" \
  --character "Mara Vale" --character "Nessa Quill" --tick 145
wedl author move --character "Mara Vale" --location "Flood Gallery N" \
  --scene "Flood Gallery N" --tick 146
wedl author conversation create "Water at the Sluice" --scene "Flood Gallery N" \
  --character "Mara Vale" --character "Nessa Quill"
wedl author conversation append "The Southward Cut" "Hold the road." \
  --speaker Rhea --addressee Pip --interrupt-last
```

The available actions are `current-time set`, `scene create|advance|close`,
`move`, and `conversation create|append`. Conversation creation uses the named
active scene's location and current time; it includes all independently present
characters unless `--character` selects a subset. When more than one scene is
active, name the scene explicitly. A scene creation or reconciled move ends
the selected characters' prior active-scene presences before adding their new
ones, and transfers held listed objects with them. `scene advance` and
`current-time set` advance every active front together, preserving the shared
canonical cursor. An automatically ordered conversation append that lands after
the cursor advances that shared horizon too. Speech appends support `--addressee` and `--interrupt-last`;
action appends use `--kind action --actor NAME` (repeat `--actor` as needed).
Authoring preview and apply responses include an `authorImpact` name-only
summary; the raw changeset payload and response fields remain available for
automation. Use `--yes` only for deliberate CLI automation; spatial source
intents always require their exact preview confirmation.

## Performance and profile benchmarking

```bash
python tools/generate_stress_world.py /tmp/wedl-medium --profile medium
PYTHONPATH=src python tools/benchmark_search_profiles.py \
  --repo /tmp/wedl-medium \
  --query "consistency checksum neighboring records" \
  --repeats 7
```

The equivalent PowerShell authoring invocation uses a semicolon-separated
`PYTHONPATH` and Windows paths:

```powershell
$env:PYTHONPATH = "src"
& .\.venv\Scripts\python.exe tools\build_frontiersmen.py .\frontiersmen-rebuilt
& .\.venv\Scripts\python.exe tools\benchmark_search_profiles.py `
  --repo .\frontiersmen-rebuilt `
  --query "consistency checksum neighboring records" `
  --repeats 7
```

Historical 0.5.0 benchmark on the 238-record pre-ending fixture:

| Profile | Cold compile | Warm forced | Exact reuse | Database |
|---|---:|---:|---:|---:|
| `state` | 54 ms | 45 ms | 5.6 ms | 1.42 MB |
| `fts` | 77 ms | 69 ms | 5.2 ms | 2.59 MB |
| `vector` | 756 ms | 95 ms | 5.9 ms | 5.89 MB |
| `hybrid` | 552 ms | 103 ms | 5.9 ms | 6.30 MB |

Median authored-fixture query latency was approximately 3.0 ms for FTS, 4.8 ms
for vector, and 5.9 ms for hybrid. On the 4,303-record stress world, medians
were approximately 11 ms, 38 ms, and 38 ms respectively. Exact vector search
scores all authorized unique vectors; no ANN extension is required at this
scale.

The exact values are environment-specific. Cold local LSA fitting dominates
vector builds; warm forced compilation reuses the cached model and normalized
vectors by content hash. Detailed measurements are in
[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md),
[`docs/SEARCH_PROFILE_BENCHMARKS.json`](docs/SEARCH_PROFILE_BENCHMARKS.json),
and [`docs/MEDIUM_SEARCH_PROFILE_BENCHMARKS.json`](docs/MEDIUM_SEARCH_PROFILE_BENCHMARKS.json).

## Tests

```bash
pytest -q
```

The suite covers parsing, validation, temporal state, historical story-point
correctness, perspective-safe retrieval, hard context budgets, conversation
presence intervals, conflicting recollections, atomic changesets, typed
conversation IDs, idempotent replay, source/embedding reuse, compiled-world
reuse, SQLite integrity, completed-story continuity, historical search boundaries, and API authorization.

## Project layout

```text
src/wedl/
  audience.py       explicit prose audiences
  conversation.py   transcript, presence, and recollection semantics
  context.py        prose-first LLM writing-packet composer
  repository.py     Git snapshots and parsed-source cache
  compiler.py       SQLite compiler and embedding cache
  search.py         independent weighted FTS/vector retrieval and hybrid fusion
  vectors.py        normalized LSA and external embedding providers
  profiles.py       state/FTS/vector/hybrid compilation profiles
  semantics.py      temporal state, knowledge, relationships, story points
  changeset.py      atomic narrative mutations
  data/ash_archive/ completed executable fixture
  data/frontiersmen/ open-campaign executable fixture

tools/
  expand_example_v04.py
  finalize_example_v04.py
  generate_stress_world.py
  benchmark.py
  benchmark_search_profiles.py
  build_frontiersmen.py
  frontiersmen_spec.py

examples/
  choice-of-records-followup.json
  conversation-changeset.json
```

Detailed design, validation, performance, and interaction notes are under
`docs/`.
