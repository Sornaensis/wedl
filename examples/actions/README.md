# Worked private action and requirement requests

Use the valid world produced by all **207 numbered requests** in
[the authoring walkthrough](../authoring/README.md). This example adds no
canonical story records. It uses Ana, the closed harbor scene and the middle
magic practice at main 12/0. The complete source remains 144 records.

The optional request-building helper uses only Python's standard library;
Python is not required by the Rust application. Run from this source archive's
project root, with an existing parent for the new request directory:

```sh
ACTION_BIN=/absolute/path/to/wedl-rs
ACTION_WORLD=/absolute/path/to/complete-example-world
ACTION_REQUESTS=/tmp/wedl-private-action-requests
python3 examples/actions/make_requests.py --binary "$ACTION_BIN" --repo "$ACTION_WORLD" --output "$ACTION_REQUESTS"
```

The helper reads the current world ID and source fingerprint. It creates
`01-save.json`, `02-review.json` and `frame.json` in a new directory. It refuses
an existing output directory. It saves no action and makes no sufficiency
assessment. Review the generated private intent and target before saving.

## Create and inspect one private draft

```sh
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts list
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts preview --file "$ACTION_REQUESTS/01-save.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts save --file "$ACTION_REQUESTS/01-save.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts detail action_00000000000000000000000001 > "$ACTION_REQUESTS/saved.json"
```

Preview shows the complete candidate source and its hash. Save reproduces those
bytes if the canonical source and expected private hash still match. Creation
uses `expectedSha256:null`. An existing action with this ID is a conflict; choose
a new stable ID or explicitly load and edit the existing document.

The saved document is `author/actions/action_00000000000000000000000001.json`.
Its Markdown retains the example's CRLF. Its question has no `answer` field,
which is different from an explicitly empty answer. Canonical source, character
learning, state, search and the source fingerprint are unchanged.

## Review at the action's saved horizon

```sh
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts review --file "$ACTION_REQUESTS/02-review.json" > "$ACTION_REQUESTS/initial-review.json"
```

The selected writing frame has clock 11/0, but the draft's saved horizon is
12/0. The response reports 12/0 and preserves dramatic context as a separate
character prompt and author margin. It reports `requiredAnswered:false`.
Loading this packet does not assess its sufficiency.

To prepare a resolution-needed review, explicitly answer the question and set
that phase **before** requesting the review. For example:

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
request = json.loads((directory / '02-review.json').read_text())
request['draft']['phase'] = 'resolution-needed'
request['draft']['questions'][0]['answer'] = (
    'Ana retains the rejected no-witness report at 12/0. The author margin '
    'preserves separately admitted canonical source. No outcome follows from either account.'
)
(directory / '03-ready-review.json').write_text(json.dumps(request, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts review --file "$ACTION_REQUESTS/03-ready-review.json" > "$ACTION_REQUESTS/ready-review.json"
```

Read the resulting context and question answer. If it is insufficient, keep or
request `context-needed`, mark stale, or cancel. Only after you assess it as
sufficient, compose the exact save confirmation:

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
saved = json.loads((directory / 'saved.json').read_text())['result']
request = json.loads((directory / '03-ready-review.json').read_text())
reviewed = json.loads((directory / 'ready-review.json').read_text())['result']
confirmation = {key: request[key] for key in ('frame', 'briefs', 'families', 'maxCharacters')}
confirmation.update(reviewSha256=reviewed['reviewSha256'], sufficient=True)
save = {
    'expectedFingerprint': request['frame']['fingerprint'],
    'expectedSha256': saved['sourceSha256'],
    'draft': request['draft'],
    'review': confirmation,
}
(directory / '04-ready-save.json').write_text(json.dumps(save, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts preview --file "$ACTION_REQUESTS/04-ready-save.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts save --file "$ACTION_REQUESTS/04-ready-save.json"
```

Editing the answer, body, intent, phase, horizon, selected context or budget
after review makes its hash stale. Setting `sufficient:false`, omitting the
review, or leaving a required answer absent refuses the resolution-needed save.
The saved phase records private readiness. Continue with the
[human proposal and consequence guide](PROPOSALS.md) to append an explicit
alternative, review its complete context and inspect exact candidate source.

Reload detail to obtain the new hash before another edit. Cancellation uses the
same document with `phase:"cancelled"` and `review:null`. Cancellation is
terminal, including an unchanged save. There is no private deletion command.
An exact retry with the prior file hash after a successful changed save is stale;
inspect current bytes if a save response was lost.

## Compare explicit literals separately

[magic-selection.json](magic-selection.json) selects only the middle practice's
restriction and exception fields. It deliberately follows no optional links,
so other missing role selections cannot be mistaken for complete linkage.
[field-requirement.json](field-requirement.json) compares the complete authored
restriction object, including the exact integer `9007199254740993`, decimal
`1e0`, and a Ref-shaped **literal** object.

Prepare an author frame without changing the captured source or time:

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
frame = json.loads((directory / 'frame.json').read_text())
frame['lens'] = 'author'
(directory / 'author-frame.json').write_text(json.dumps(frame, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-requirements capture --file "$ACTION_REQUESTS/author-frame.json" --families "$(cat examples/actions/magic-selection.json)" --max-characters 100000
"$ACTION_BIN" --repo "$ACTION_WORLD" action-requirements compare --file "$ACTION_REQUESTS/author-frame.json" --families "$(cat examples/actions/magic-selection.json)" --requirement "$(cat examples/actions/field-requirement.json)" --max-characters 100000
"$ACTION_BIN" --repo "$ACTION_WORLD" action-requirements compare --file "$ACTION_REQUESTS/author-frame.json" --families "$(cat examples/actions/magic-selection.json)" --requirement "$(cat examples/actions/assertion-requirement.json)" --max-characters 100000
```

Both author comparisons return `matches`. The restriction's source ratio `1.0`
equals the explicit `1e0`. Changing only the expected integer to
`9007199254740992` returns `differs`. The assertion request selects correction
20: “The witnessed vow must be spoken at the harbor.” Selecting replaced
assertion 19 instead returns `unknown` at this horizon.

The generated `frame.json` retains the dramatic lens:

```sh
"$ACTION_BIN" --repo "$ACTION_WORLD" action-requirements capture --file "$ACTION_REQUESTS/frame.json" --families "$(cat examples/actions/magic-selection.json)" --max-characters 100000
"$ACTION_BIN" --repo "$ACTION_WORLD" action-requirements compare --file "$ACTION_REQUESTS/frame.json" --families "$(cat examples/actions/magic-selection.json)" --requirement "$(cat examples/actions/field-requirement.json)" --max-characters 100000
```

This separate supplement shows only Ana's learned account: she was told that
middle calling needs no witness, and she retains that account after rejection
at 12/0. It does not substitute the canonical correction. The comparison is
`unknown`, with null actual operand and no author citations. The character lens
has the same learned-only boundary. Ordinary dramatic action review still has
its explicit separate author margin.

Requirement results never appear inside a draft or save confirmation. Matching
literal prose does not calculate a cost, prove feasibility, mark evidence
sufficient or authorize an effect. See [ACTIONS.md](../../docs/ACTIONS.md) for
the closed grammars, complete limits and remaining workflow scope.
