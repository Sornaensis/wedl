# Work-bound Writing, dated search, and shared current time

This compact walkthrough creates a new ten-record world and two private author
documents, then runs **19 CLI reads and previews**. It uses native `wedl/v1`,
`wedl-author/v1`, and `wedl-rust/v1` contracts. Python is only the standard-library
fixture and request helper; the runtime operations use the Rust executable.

Run this block from the project root. Each run chooses a new temporary directory:

```sh
cargo build --locked --offline
WRITING_BIN="${CARGO_TARGET_DIR:-target}/debug/wedl-rs"
WRITING_EXAMPLE="$(mktemp -d)/walkthrough"
python3 examples/writing/make_requests.py --binary "$WRITING_BIN" --output "$WRITING_EXAMPLE"
```

The output directory contains the generated world, exact request files, every
CLI response, `commands.json`, and `summary.json`. The helper refuses an existing
output directory. It checks complete results and stops on any failed command or
assertion.

## One explicitly selected frame

`discover-work.json` uses the author lens to obtain a minimal work choice and the
private author fingerprint. `frame-work.json` then binds that exact work and
author fingerprint alongside the captured canonical fingerprint. Its independent
coordinates are the active landing scene, Ada as POV, and main **10/0**, with a
character lens.

The work deliberately repeats a segment whose authored placement is **50/7** and
whose POV and source link name another character. Resolving the frame retains
the selected scene, Ada, and 10/0. The bound and unbound contexts have identical
character prompts and author margins. The private work/segment prose and linked
person's body do not enter the context. There is no Writing occurrence selector.

The executable requests are:

```sh
"$WRITING_BIN" --repo "$WRITING_EXAMPLE/world" writing resolve --file "$WRITING_EXAMPLE/frame-work.json"
"$WRITING_BIN" --repo "$WRITING_EXAMPLE/world" writing context --file "$WRITING_EXAMPLE/frame-work.json" --max-characters 100000
```

## A dated corpus and a stable thread subsequence

The helper searches `vessel` at main 10/0 in lexical, semantic, and hybrid modes,
first across object prose, then with the exact Voyage thread ID. It verifies
that grouped rows equal the corresponding subsequence of the full ranked
result, including scores, snippets, lane metadata, and training-derived settings.
The output limit is applied after grouping over the complete bounded candidate
window.

A separate event's body starts at 20/0. Searching its unique body word at 10/0
returns no results in all three modes; lexical search finds it at the exact
20/0 boundary. Source identity titles remain timeless. These requests exercise
native dated prose selection, not automatic structured payload search.

For example:

```sh
"$WRITING_BIN" --repo "$WRITING_EXAMPLE/world" search vessel --kind object --timeline main --tick 10 --order 0 --mode hybrid --thread thread_00000000000000000000000002
```

## One complete shared-current-time proposal

`current-time-request.json` asks for main **12/2** against the same canonical
fingerprint. The preview returns `current-time-plan.json`, a standard consequence
plan containing the world and every active scene. In this fixture it changes two
records: the world clock and the active landing scene cursor. The closed scene,
event, characters, objects, and private work remain outside the plan.

The helper runs the exported plan through the ordinary consequence preview and
checks that it produces the same full candidate preview. It also checks valid
candidate invariants, no performed writes, unchanged original source hashes,
the original current time of 10/0, and no created FTS cache.

```sh
"$WRITING_BIN" --repo "$WRITING_EXAMPLE/world" current-time preview --file "$WRITING_EXAMPLE/current-time-request.json" --max-characters 100000
"$WRITING_BIN" --repo "$WRITING_EXAMPLE/world" consequences preview --file "$WRITING_EXAMPLE/current-time-plan.json" --max-characters 100000
```

The generated request, full candidate source, and exported plan are available
for inspection. No application or publication is part of this walkthrough.
