# Worked human proposals and consequence previews

First complete the resolution-needed save in [the action guide](README.md),
using a disposable copy of the complete 207-request authoring world. Retain its
`ACTION_BIN`, `ACTION_WORLD` and `ACTION_REQUESTS` variables. This continuation
adds private proposal history while the 144 canonical records remain unchanged.
The commands run from this archive's project root. Use fresh example IDs for an
already modified private history.

The explicit [operation list](proposal-operations.json) creates a proposed
event and a separate knowledge record, then replaces the bell's body with an
exact CRLF string. The knowledge operation deliberately precedes its causing
event; the final candidate validates them together. The account's truth stays
unknown. Neither action intent nor the event automatically creates learning:
the knowledge record and its transition are authored explicitly.

## Prepare and inspect the complete alternative

Load the exact saved private document and append a proposal locally. The helper
below creates request files only. Its answer snapshot includes every present
answer in current question order, including an explicitly empty answer.

```sh
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts detail action_00000000000000000000000001 > "$ACTION_REQUESTS/ready-detail.json"
python3 - "$ACTION_REQUESTS" <<'PY'
import copy, json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
saved = json.loads((directory / 'ready-detail.json').read_text())['result']
draft = copy.deepcopy(saved['draft'])
assert draft['phase'] == 'resolution-needed' and draft['proposals'] == []
frame = json.loads((directory / 'frame.json').read_text())
operations = json.loads(pathlib.Path('examples/actions/proposal-operations.json').read_text())
analysis = {
    'at': {'timeline': 'main', 'tick': '12', 'order': '1'},
    'policy': 'required',
    'expectations': [
        {'id': 'reply-is-held', 'predicate': {'kind': 'knowledge.state',
         'knowledgeId': 'know_00000000000000000000000050', 'state': 'accepted'}},
        {'id': 'reply-has-explicit-cause', 'predicate': {'kind': 'transition.caused',
         'recordId': 'know_00000000000000000000000050', 'transitionId': 'heard-reply',
         'eventId': 'event_00000000000000000000000050'}},
    ],
}
plan = {'expectedFingerprint': frame['fingerprint'], 'operations': operations, 'analysis': analysis}
check = {'expectedFingerprint': frame['fingerprint'], 'analysis': analysis, 'maxCharacters': 100000}
draft['proposals'].append({
    'id': 'proposal_00000000000000000000000001',
    'summary': 'Rowan gives a reply; Ana retains it as an uncertain account.',
    'contextFingerprint': frame['fingerprint'],
    'questionAnswers': [{'questionId': q['id'], 'answer': q['answer']}
                        for q in draft['questions'] if 'answer' in q],
    'resolver': 'human',
    'evidence': [],
    'consequences': plan,
})
draft['phase'] = 'proposed'
review = {'draft': draft, 'frame': frame, 'briefs': [], 'families': {}, 'maxCharacters': 100000}
for name, value in [('plan.json', plan), ('current-check.json', check), ('05-proposal-review.json', review)]:
    (directory / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" consequences check --file "$ACTION_REQUESTS/current-check.json" > "$ACTION_REQUESTS/current-expectations.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" consequences preview --file "$ACTION_REQUESTS/plan.json" --max-characters 100000 > "$ACTION_REQUESTS/consequences.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts review --file "$ACTION_REQUESTS/05-proposal-review.json" > "$ACTION_REQUESTS/proposal-review.json"
```

Inspect the full context, exact before/after source, three diffs and validation
diagnostics. This example's valid candidate contains 146 records, with two
creates and one changed existing file. The live world still contains 144.
`writesPerformed:false` and `canApply:false` remain explicit. No preview
approves the alternative.

The current-source check reports both expectations as `unknown`, because the
reply event and knowledge record do not yet exist. The candidate's
`semanticAnalysis` reports both as `pass` at 12/1 and shows the knowledge change
at that same horizon. The body-only bell edit stays visible in the exact source
diff without creating a semantic state change. `accepted` describes Ana's
authored belief; the claim's truth remains unknown. With `policy:required`, both
checks must pass before the later approved outcome can apply.

The analysis horizon, policy and literal predicates are retained in immutable
proposal history and covered by its complete review. Changing the horizon to
12/0 makes the reply checks unknown; it never moves the authored transition.
See [semantic consequences](../../docs/SEMANTIC_CONSEQUENCES.md) for all seven
predicates, typed operands and the separate source/semantic verification scopes.

The complete proposed draft is reviewed **after** adding its plan. An earlier
resolution-needed review does not bind this proposal. Changes to the raw YAML,
summary, answer snapshots, source, context selectors or budget require a fresh
review. The original draft `baseFingerprint` is preserved.

## Confirm and save private history

After assessing the complete packet as sufficient, bind that decision to its
exact review hash. If the packet is insufficient, keep the saved private draft
and revise the local request before reviewing again.

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
saved = json.loads((directory / 'ready-detail.json').read_text())['result']
request = json.loads((directory / '05-proposal-review.json').read_text())
reviewed = json.loads((directory / 'proposal-review.json').read_text())['result']
confirmation = {key: request[key] for key in ('frame', 'briefs', 'families', 'maxCharacters')}
confirmation.update(reviewSha256=reviewed['reviewSha256'], sufficient=True)
save = {'expectedFingerprint': request['frame']['fingerprint'],
        'expectedSha256': saved['sourceSha256'], 'draft': request['draft'], 'review': confirmation}
(directory / '06-proposal-save.json').write_text(json.dumps(save, ensure_ascii=False, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts preview --file "$ACTION_REQUESTS/06-proposal-save.json" > "$ACTION_REQUESTS/proposal-save-preview.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts save --file "$ACTION_REQUESTS/06-proposal-save.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts detail action_00000000000000000000000001 > "$ACTION_REQUESTS/proposed-detail.json"
```

The saved phase is `proposed`, with one active proposal. Only the private action
file changes. The earlier history is immutable: editing a summary, answer,
context pin or even YAML whitespace in a stored proposal is refused. Plans
whose constructed candidate has validation errors can be retained for diagnosis;
malformed or unconstructible plans cannot be appended.

## Preview the exact saved proposal

The saved preview takes compact selectors, not an imported plan or result. It
binds the current source, exact private-file hash and selected active proposal.

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
envelope = json.loads((directory / 'proposed-detail.json').read_text())
saved = envelope['result']
request = {'expectedFingerprint': envelope['fingerprint'],
           'actionId': saved['draft']['id'], 'expectedSha256': saved['sourceSha256'],
           'proposalId': 'proposal_00000000000000000000000001', 'maxCharacters': 100000}
(directory / '07-saved-preview.json').write_text(json.dumps(request, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts preview-proposal --file "$ACTION_REQUESTS/07-saved-preview.json" > "$ACTION_REQUESTS/saved-consequences.json"
```

The nested consequence report agrees with the standalone preview for the same
plan and budget. Its outer hash additionally binds the exact saved proposal and
private source. A changed source fingerprint, private hash, inactive proposal,
stale effective phase or exhausted complete budget refuses the preview.

## Revise without rewriting history

To change proposed intention or answers, first save a document with `phase:stale`
and `review:null`, preserving every proposal verbatim and using the latest
private hash. A phase-only rejection can instead use `resolution-needed` with
a fresh readiness review. Cancellation is terminal. None of these transitions
executes the retained plan.

From `stale` or `resolution-needed`, append a new stable proposal ID with a new
current answer snapshot and current source pin. Set optional `supersedes` to
the earlier proposal ID only when explicitly replacing that alternative. Review
the complete proposed draft and confirm it afresh before saving. Omitting
`supersedes` leaves both alternatives active; they are previewed independently,
never composed into one plan.

A canonical source edit makes authored `proposed` history effectively stale
when **any unsuperseded proposal** retains an older context. A new parallel
alternative does not hide that old active context. Explicit supersession can
retire it while the original draft base and every prior proposal stay unchanged.
Load a fresh current world and private hash before making that follow-up save.

For a deliberate no-change alternative, use one explicit typed update such as
`{"type":"record.update","entityId":"char_00000000000000000000000001","patch":"kind: character\n"}`.
An empty operation list is invalid. See [ACTIONS.md](../../docs/ACTIONS.md) and
[CONSEQUENCES.md](../../docs/CONSEQUENCES.md) for complete phase, construction and
bound rules. Continue with [reviewed outcome application](OUTCOMES.md) to
approve and publish the exact saved alternative through its separate workflow.
