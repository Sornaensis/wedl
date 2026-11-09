# Apply the reviewed human alternative

Continue after [the proposal walkthrough](PROPOSALS.md) on its disposable copy
of the complete 207-request world. Keep `ACTION_BIN`, `ACTION_WORLD` and
`ACTION_REQUESTS`. The saved action is proposed, and `07-saved-preview.json`
selects its exact private bytes and active proposal. The live source still has
144 records. The instructions run from this project's root.

## Review the outcome separately

```sh
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts review-outcome --file "$ACTION_REQUESTS/07-saved-preview.json" > "$ACTION_REQUESTS/outcome-review.json"
```

Inspect the complete retained draft, proposal, exact three-file candidate,
validation and private transition. The report has a new `reviewSha256`; neither
the earlier readiness hash nor `previewSha256` is an application approval.
`canApply:true` means the valid candidate is eligible for an explicit decision.
Review itself leaves every source file unchanged.

The retained semantic analysis compares both worlds at main 12/1. Both required
reply checks must pass. They do not supply approval or prove the reported account
true; inspect the full candidate and make that decision separately.

This alternative creates the authored reply event and a **separate** knowledge
record, and replaces the bell's body with exact CRLF prose. The knowledge
transition is explicit, at main 12/1. The reported account's truth stays unknown.
The complete candidate has 146 records. No event or action prose supplies extra
learning or state effects.

## Approve that exact report and apply

After reviewing and approving this alternative, prepare the closed application
request. This step only writes a request file:

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
request = json.loads((directory / '07-saved-preview.json').read_text())
report = json.loads((directory / 'outcome-review.json').read_text())['result']
assert report['canApply'] and report['writesPerformed'] is False
assert report['privateTransition']['resultingPhase'] == 'applied'
request.update(reviewSha256=report['reviewSha256'], approved=True)
(directory / '08-apply-outcome.json').write_text(json.dumps(request, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts apply-outcome --file "$ACTION_REQUESTS/08-apply-outcome.json" > "$ACTION_REQUESTS/applied-outcome.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts detail action_00000000000000000000000001 > "$ACTION_REQUESTS/applied-detail.json"
```

Application recomputes the complete outcome under the writer lock. The three
canonical files and terminal private action share one recoverable publication.
The resulting `fingerprint` matches the reviewed candidate, and the private
action has `phase:applied` with an `appliedReceipt`. Its original base, prior
proposals, answer snapshots and private body are preserved. Canonical source
now contains 146 records; no further private save is allowed for this action.

An exact retry verifies the already applied result without writing:

```sh
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts apply-outcome --file "$ACTION_REQUESTS/08-apply-outcome.json" > "$ACTION_REQUESTS/replayed-outcome.json"
```

The result has `alreadyApplied:true` and `writesPerformed:false`. A different
request or any later canonical change refuses this retry. It never applies the
operations twice. If an attempt was interrupted, inspect publication status
and use the recovery procedure below before assuming either outcome.

## Verify exact publication

Verification pins the **current** source and applied private-file bytes:

```sh
python3 - "$ACTION_REQUESTS" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
saved = json.loads((directory / 'applied-detail.json').read_text())['result']
request = {'expectedFingerprint': saved['fingerprint'],
           'actionId': saved['draft']['id'], 'expectedSha256': saved['sourceSha256'],
           'maxCharacters': 100000}
(directory / '09-verify-outcome.json').write_text(json.dumps(request, indent=2) + '\n')
PY
"$ACTION_BIN" --repo "$ACTION_WORLD" action-drafts verify-outcome --file "$ACTION_REQUESTS/09-verify-outcome.json" > "$ACTION_REQUESTS/verified-outcome.json"
"$ACTION_BIN" --repo "$ACTION_WORLD" consequences publication-status
```

For this unchanged result, `sourceMatches`, `filesMatch` and `verified` are true,
and publication status has `pending:false`. Verification writes nothing. The
receipt checks exact source publication. The separate `semanticVerification`
also reevaluates the retained expectations on the current source at 12/1; both
pass here. Neither result infers whether the character's report is true.

For later drift, reload the current action and create a new verification
request. An unrelated canonical change makes `sourceMatches:false` while the
three recorded files may still have `filesMatch:true`.

## If publication was interrupted

Run `consequences publication-status` before loading ordinary current source.
If it reports a pending journal, inspect its files and exact `journalSha256`,
then explicitly invoke `consequences recover --expected-journal-sha256 HASH`.
Recovery restores the original three-file state and proposed action. It refuses
unrecognized edits and retains the journal on failure; it does not guess which
content to keep. Do not delete the journal as cache cleanup.

Restart the snapshot server after successful application or recovery. In the
Actions view, an applied action displays read-only history and receipt plus a
bounded current-source verification control. See [OUTCOMES.md](../../docs/OUTCOMES.md)
for the complete binding, replay, limits and recovery contract.
