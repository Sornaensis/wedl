# Original retcon review and independent currentness

Run from the project root after `cargo build`. This self-contained example creates
an isolated temporary world and Git history; it never edits a maintained example.
Set `WEDL_BIN` to a built binary if the Cargo target directory is elsewhere.
Python 3 and Git are the only additional tools.

The example previews typed impact, captures immutable original analysis, saves
one private decision, and demonstrates current, stale and unknown comparison
states. Exact source integers and retained private CRLF text are asserted. The
plan is never applied by a retcon command.

```bash
export WEDL_BIN="${WEDL_BIN:-$PWD/target/debug/wedl-rs}"
python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(tempfile.mkdtemp(prefix='wedl-retcon-example-'))
world = root / 'world'
binary = os.environ['WEDL_BIN']
actor = 'char_00000000000000000000000001'
event = 'event_00000000000000000000000001'
place = 'loc_00000000000000000000000001'
review_id = 'review_00000000000000000000000001'
calls = 0

def file(name, value):
    path = root / name
    path.write_text(value if isinstance(value, str) else json.dumps(value), encoding='utf-8')
    return str(path)

def cli(*args):
    global calls
    calls += 1
    result = subprocess.run([binary, '--compact', '--repo', str(world), *map(str, args)],
                            capture_output=True, text=True, check=True)
    return json.loads(result.stdout)

def fingerprint():
    return cli('status')['fingerprint']

def write_record(name, source, record_id=None):
    source_file = file(name, source)
    prefix = ['record', 'update', record_id] if record_id else ['record', 'create']
    pinned = fingerprint()
    request = [*prefix, '--file', source_file, '--expected-fingerprint', pinned]
    preview = cli(*request)['result']
    cli(*request, '--apply', '--expected-after-sha256', preview['afterSha256'])

git_env = dict(os.environ, GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1',
               GIT_AUTHOR_NAME='Example', GIT_AUTHOR_EMAIL='example@example.invalid',
               GIT_COMMITTER_NAME='Example', GIT_COMMITTER_EMAIL='example@example.invalid')
def git(*args):
    return subprocess.run(['git', '-C', str(world), *args], env=git_env,
                          capture_output=True, text=True, check=True).stdout.strip()

def commit(label):
    git('add', '--', 'story')
    if (world / 'author').exists():
        git('add', '--', 'author')
    git('commit', '-q', '-m', label)
    return git('rev-parse', 'HEAD')

cli('init', world, '--title', 'Original retcon example')
world_id = cli('entity', 'list', '--kind', 'world')['result']['items'][0]['id']
write_record('state.yaml', 'kind: world\nstate_keys:\n  character:\n    count: {type: integer}\n', world_id)
write_record('actor.yaml', f'kind: character\nid: {actor}\ntitle: Original actor\ninitial_state: {{count: 9007199254740993}}\nbody: Original actor source.\n')
write_record('place.yaml', f'kind: location\nid: {place}\ntitle: Unrelated landing\n')
write_record('event.yaml', f'kind: event\nid: {event}\ntitle: Explicit effect\ntime: {{tick: 10}}\neffects:\n - {{id: effect_00000000000000000000000001, target: {actor}, key: count, operation: set, value: 9007199254740995}}\n')
actor_path = world / cli('entity', 'show', actor)['result']['path']
original_actor = actor_path.read_bytes()
git('init', '-q')
base = commit('Original source')
pinned = fingerprint()
plan = {'expectedFingerprint': pinned, 'operations': [
    {'type': 'record.update', 'entityId': actor, 'patch': 'kind: character\ntitle: Proposed actor\n'}]}
impact_request = {'target': {'kind': 'record', 'id': actor},
                  'at': {'timeline': 'main', 'tick': '10', 'order': '0'},
                  'depth': 32, 'maxItems': 1000}
impact = cli('retcon-impact', 'preview', '--file', file('impact.json', {
    'plan': plan, 'impactRequest': impact_request}))['result']
assert len(impact['items']) == 2
assert {item['snapshot'] for item in impact['items']} == {'base', 'candidate'}
assert not impact['writesPerformed'] and not impact['canApply']

capture = cli('retcon-reviews', 'preview', '--file', file('capture.json', {
    'baseRevision': base, 'plan': plan, 'impactRequest': impact_request}))['result']
analysis = capture['analysis']
assert any('9007199254740993' in section['valueRaw']
           for item in analysis['witness']['inputs'] for section in item['sections'])
document = {'id': review_id, 'title': 'Original actor review', 'analysis': analysis,
            'decisions': [{'itemDigest': analysis['itemDigests'][0],
                           'disposition': 'intentionally-retained',
                           'note': 'Reviewed exact effect.\r\n'}],
            'bodyMarkdown': '\r\nPrivate commentary 🌲\r\nTrailing  '}
save_request = {'expectedFingerprint': pinned, 'expectedSha256': None, 'document': document}
request_file = file('save.json', save_request)
preview = cli('retcon-reviews', 'preview-save', '--file', request_file)['result']
assert preview['canSave'] and not preview['writesPerformed']
saved = cli('retcon-reviews', 'save', '--file', request_file)['result']
assert saved['writesPerformed']
private_path = world / saved['path']
original_private = private_path.read_bytes()
assert hashlib.sha256(original_private).hexdigest() == saved['sourceSha256']
assert fingerprint() == pinned

queue_file = file('queue.json', {'revision': None, 'maxItems': 1000})
def queue_status():
    result = cli('retcon-reviews', 'queue', '--file', queue_file)['result']
    assert result['items'][0]['pendingCount'] == 1
    return result['items'][0]['currentness']['status']

assert queue_status() == 'current'
saved_revision = commit('Original private review')
write_record('unrelated.yaml', 'kind: location\ntitle: Changed unrelated landing\n', place)
assert queue_status() == 'current'
write_record('relevant.yaml', 'kind: character\ntitle: Changed relevant actor\n', actor)
assert queue_status() == 'stale'  # Native writes do not need a Git commit to stale review.
commit('Relevant source changed')
actor_path.write_bytes(original_actor)
commit('Actor reverted to original bytes')
assert queue_status() == 'stale'  # Committed first-parent history retains the change.
assert private_path.read_bytes() == original_private

# In this temporary fixture only, break live inputs to demonstrate original reads.
actor_path.write_text('invalid live canonical source', encoding='utf-8')
private_path.write_text('invalid live private source', encoding='utf-8')
detail = cli('retcon-reviews', 'detail', '--file', file('historical-detail.json', {
    'revision': saved_revision, 'id': review_id,
    'expectedSha256': saved['sourceSha256']}))['result']
assert detail['currentness']['status'] == 'unknown'
assert detail['source'].encode('utf-8') == original_private
assert detail['document'] == document and detail['blobOid']
assert detail['document']['analysis']['baseRevision'] == base
print(json.dumps({'world': str(world), 'cliCalls': calls,
                  'impactOccurrences': len(impact['items']), 'pendingItems': 1,
                  'observedCurrentness': ['current', 'stale', 'unknown'],
                  'historicalOriginalPreserved': True}, indent=2))
PY
```

An omitted decision remains pending. Updating this review requires its current
private `sourceSha256` and a current canonical fingerprint; the immutable
analysis still names the original base. A same-content save also requires the
current private hash. Saving a decision never applies its plan.

Only committed history can retain an earlier change after it is reverted.
A fully reverted uncommitted edit leaves no durable historical evidence. Missing,
invalid, over-bound or non-first-parent comparison history returns `unknown`.
See [the review contract](../../docs/RETCON_REVIEWS.md),
[typed impact](../../docs/RETCON_IMPACT.md) and
[event/grouped consequences](../../docs/EVENT_CONSEQUENCES.md).
