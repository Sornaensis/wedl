# Worked retained edit

Use an absolute path to the current built `wedl-rs` executable as `EDIT_BIN`, for
example `export EDIT_BIN=/absolute/path/to/wedl-rs`. Run the three blocks in order
in the same POSIX shell with Python 3 available. They create a fresh disposable
world and request directory under the system temporary directory; no other
example or Python package is required. The target is the new world's existing
world record. All contracts remain v1.

## 1. Save unfinished work

The first patch is deliberately incomplete. Its private preview and save do not
evaluate it. A separate review reports the error while retaining the raw text.

```sh
set -eu
: "${EDIT_BIN:?Set EDIT_BIN to an absolute path to the built wedl-rs executable}"
case "$EDIT_BIN" in /*) ;; *) exit 2 ;; esac
test -x "$EDIT_BIN"
EDIT_WORK=$(mktemp -d "${TMPDIR:-/tmp}/wedl-edit-example.XXXXXX")
export EDIT_WORLD="$EDIT_WORK/world" EDIT_REQUESTS="$EDIT_WORK/requests"
mkdir "$EDIT_REQUESTS"
"$EDIT_BIN" init "$EDIT_WORLD" --title "Retained edit example" > "$EDIT_REQUESTS/initial-status.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" entity show "Retained edit example" > "$EDIT_REQUESTS/target.json"
python3 - <<'PY'
import hashlib, json, os, pathlib
p = pathlib.Path(os.environ['EDIT_REQUESTS'])
capture = json.loads((p / 'target.json').read_text())
target = capture['result']
draft = {
    'id': 'edit_00000000000000000000000001', 'title': 'Revise the world introduction',
    'worldId': target['id'], 'targetId': target['id'],
    'baseFingerprint': capture['fingerprint'],
    'baseSourceSha256': hashlib.sha256(target['rawSource'].encode()).hexdigest(),
    'baseSourcePath': target['path'], 'patch': 'kind: world\ntitle: [unfinished\n',
    'bodyMode': 'preserve', 'bodyMarkdown': '\r\n# Edited world\r\n\nRetained tree: 🌲  ',
    'phase': 'draft',
}
request = {'expectedFingerprint': capture['fingerprint'], 'expectedSha256': None, 'draft': draft}
(p / 'unfinished-save.json').write_text(json.dumps(request, ensure_ascii=False, indent=2) + '\n')
PY
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts preview --file "$EDIT_REQUESTS/unfinished-save.json" > "$EDIT_REQUESTS/unfinished-preview.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts save --file "$EDIT_REQUESTS/unfinished-save.json" > "$EDIT_REQUESTS/unfinished-save-result.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts detail edit_00000000000000000000000001 > "$EDIT_REQUESTS/unfinished-detail.json"
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['EDIT_REQUESTS'])
saved = json.loads((p / 'unfinished-detail.json').read_text())
request = json.loads((p / 'unfinished-save.json').read_text())
target = json.loads((p / 'target.json').read_text())['result']
assert saved['result']['draft'] == request['draft']
assert saved['fingerprint'] == request['expectedFingerprint']
assert (pathlib.Path(os.environ['EDIT_WORLD']) / target['path']).read_bytes() == target['rawSource'].encode()
review = {'expectedFingerprint': saved['fingerprint'], 'editId': request['draft']['id'],
          'expectedSha256': saved['result']['sourceSha256'], 'maxCharacters': 100000}
(p / 'unfinished-review.json').write_text(json.dumps(review, indent=2) + '\n')
PY
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts review --file "$EDIT_REQUESTS/unfinished-review.json" > "$EDIT_REQUESTS/unfinished-review-result.json"
```

The private file exists at `author/edits/edit_00000000000000000000000001.json`.
Canonical source and its fingerprint are unchanged. The `preserve` mode keeps the
retained prose private even when that string is nonempty.

## 2. Finish the patch and inspect the exact candidate

This update keeps the original base and uses the exact saved private-file hash.
It explicitly chooses `replace` for the retained CRLF prose, whose two trailing
spaces and absent final newline are preserved.

```sh
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['EDIT_REQUESTS'])
failed = json.loads((p / 'unfinished-review-result.json').read_text())['result']
assert failed['canApply'] is False and failed['consequences'] is None
request = json.loads((p / 'unfinished-save.json').read_text())
saved = json.loads((p / 'unfinished-detail.json').read_text())['result']
request['expectedSha256'] = saved['sourceSha256']
request['draft']['patch'] = 'kind: world\ntitle: Reviewed edit example\n'
request['draft']['bodyMode'] = 'replace'
(p / 'ready-save.json').write_text(json.dumps(request, ensure_ascii=False, indent=2) + '\n')
PY
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts preview --file "$EDIT_REQUESTS/ready-save.json" > "$EDIT_REQUESTS/ready-preview.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts save --file "$EDIT_REQUESTS/ready-save.json" > "$EDIT_REQUESTS/ready-save-result.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts detail edit_00000000000000000000000001 > "$EDIT_REQUESTS/ready-detail.json"
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['EDIT_REQUESTS'])
saved = json.loads((p / 'ready-detail.json').read_text())
review = {'expectedFingerprint': saved['fingerprint'], 'editId': saved['result']['id'],
          'expectedSha256': saved['result']['sourceSha256'], 'maxCharacters': 100000}
(p / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
PY
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts review --file "$EDIT_REQUESTS/review.json" > "$EDIT_REQUESTS/review-result.json"
cat "$EDIT_REQUESTS/review-result.json"
```

Read the complete report: retained draft, exact before/after source, diff,
validation and private transition. The top-level result should have
`canApply:true`, `approvalRequired:true` and `writesPerformed:false`. Its nested
consequence preview has `canApply:false` because that preview cannot publish.

## 3. Approve this report, apply, and retry exactly

Run this block **after approving the complete report above**. It records that
decision as `approved:true` together with this review's exact digest, then uses
the identical request for retry. Refreshing its source or private hashes would
create a different request.

```sh
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['EDIT_REQUESTS'])
request = json.loads((p / 'review.json').read_text())
report = json.loads((p / 'review-result.json').read_text())['result']
assert report['canApply'] is True and report['validation']['valid'] is True
request.update(reviewSha256=report['reviewSha256'], approved=True)
(p / 'approved.json').write_text(json.dumps(request, indent=2) + '\n')
PY
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts apply --file "$EDIT_REQUESTS/approved.json" > "$EDIT_REQUESTS/applied.json"
python3 - <<'PY'
import hashlib, json, os, pathlib
world, p = pathlib.Path(os.environ['EDIT_WORLD']), pathlib.Path(os.environ['EDIT_REQUESTS'])
image = {str(f.relative_to(world)): [hashlib.sha256(f.read_bytes()).hexdigest(), f.stat().st_mtime_ns]
         for f in world.rglob('*') if f.is_file()}
(p / 'after-image.json').write_text(json.dumps(image))
PY
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts apply --file "$EDIT_REQUESTS/approved.json" > "$EDIT_REQUESTS/retry.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" edit-drafts detail edit_00000000000000000000000001 > "$EDIT_REQUESTS/applied-detail.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" validate > "$EDIT_REQUESTS/validation.json"
"$EDIT_BIN" --repo "$EDIT_WORLD" consequences publication-status > "$EDIT_REQUESTS/publication.json"
python3 - <<'PY'
import hashlib, json, os, pathlib
world, p = pathlib.Path(os.environ['EDIT_WORLD']), pathlib.Path(os.environ['EDIT_REQUESTS'])
def result(name): return json.loads((p / name).read_text())['result']
applied, retry, detail = result('applied.json'), result('retry.json'), result('applied-detail.json')
retained = json.loads((p / 'ready-save.json').read_text())['draft']
assert applied['writesPerformed'] is True and applied['changedFiles'] == 1
assert applied['canonicalNoOp'] is False and applied['alreadyApplied'] is False
assert retry['alreadyApplied'] is True and retry['writesPerformed'] is False
assert detail['fingerprint'] == applied['fingerprint']
assert detail['draft']['phase'] == 'applied'
terminal = dict(detail['draft'])
receipt = terminal.pop('appliedReceipt')
terminal['phase'] = 'draft'
assert terminal == retained and receipt == applied['receipt']
assert (world / retained['baseSourcePath']).read_bytes().endswith(retained['bodyMarkdown'].encode())
image = {str(f.relative_to(world)): [hashlib.sha256(f.read_bytes()).hexdigest(), f.stat().st_mtime_ns]
         for f in world.rglob('*') if f.is_file()}
assert image == json.loads((p / 'after-image.json').read_text())
assert result('validation.json')['valid'] is True and result('publication.json')['pending'] is False
print(json.dumps({'canonicalRecords': len(list((world / 'story').rglob('*.md'))),
    'privateEdits': len(list((world / 'author/edits').glob('edit_*.json'))),
    'changedFiles': applied['changedFiles'], 'phase': detail['draft']['phase'],
    'retryWritesPerformed': retry['writesPerformed'], 'requests': str(p)}, indent=2))
PY
```

Expected summary: one canonical record, one applied private edit, one changed
canonical file, and a retry with no writes. The byte hashes and modification
times of every file in the world are unchanged by retry and subsequent reads.
The reports and requests remain under `EDIT_REQUESTS` for inspection.

If publication is interrupted, inspect `consequences publication-status` before
retrying. A pending journal requires explicit guarded recovery using the exact
reported journal hash; successful recovery restores the draft and target before
images. See [the edit contract](../../docs/EDITS.md) for reconciliation, receipts,
recovery and historical reads. This example deliberately creates no Git history;
authors who track `story/` and `author/edits/` can inspect original saved edits
with `--revision COMMIT edit-drafts detail ID`. Their `comparedFingerprint` and
`currentness` separately describe the current worktree comparison.
