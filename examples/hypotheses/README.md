# Review author possibilities

This example creates a disposable world, saves one canonical object and one
open hypothesis through the existing guarded record writer, then reads and
searches that hypothesis. Adoption and rejection remain independent previews
against the same saved source. Each exported Plan is checked by the ordinary
consequences preview; the final assertions verify unchanged source bytes and
modification times after discovery and resolution previews.

Run this entire block from the project root with Python 3 available. `WEDL_BIN`
defaults to `target/debug/wedl-rs`; it may instead name an absolute built binary
or a wrapper executable. The block makes 14 CLI calls and retains its temporary
world, requests, and reports for inspection.

```bash
set -eu
export WEDL_BIN="${WEDL_BIN:-target/debug/wedl-rs}"
test -x "$WEDL_BIN"
export HYPOTHESIS_WORK
HYPOTHESIS_WORK=$(mktemp -d "${TMPDIR:-/tmp}/wedl-hypothesis-example.XXXXXX")
python3 - <<'PY'
import hashlib, json, os, pathlib, subprocess

binary = str(pathlib.Path(os.environ['WEDL_BIN']).resolve())
work = pathlib.Path(os.environ['HYPOTHESIS_WORK'])
root = work / 'world'
files = work / 'requests-and-reports'
files.mkdir()
subject = 'obj_00000000000000000000000001'
hypothesis = 'hyp_00000000000000000000000001'
calls = 0

def run(name, *arguments):
    global calls
    calls += 1
    completed = subprocess.run([binary, '--compact', *map(str, arguments)],
                               text=True, capture_output=True, check=True)
    (files / (name + '.json')).write_text(completed.stdout)
    return json.loads(completed.stdout)

def cli(name, *arguments):
    return run(name, '--repo', root, *arguments)

def save(name, value):
    path = files / name
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
    return path

def source_image():
    return {str(p.relative_to(root)):
            (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mtime_ns)
            for p in root.rglob('*.md')}

# Initialize and explicitly review/save the canonical subject.
initialized = run('initialized', 'init', root, '--title', 'Possibility example')
initial_image = source_image()
draft = files / 'subject.yaml'
draft.write_text(f'kind: object\nid: {subject}\ntitle: Repair log\n'
                 'body: A literal canonical repair log.\n')
subject_preview = cli('subject-preview', 'record', 'create', '--file', draft,
                      '--expected-fingerprint', initialized['fingerprint'])['result']
assert subject_preview['validation']['valid'] is True
assert subject_preview['writesPerformed'] is False
subject_saved = cli('subject-saved', 'record', 'create', '--file', draft,
                    '--expected-fingerprint', initialized['fingerprint'],
                    '--apply', '--expected-after-sha256',
                    subject_preview['afterSha256'])['result']
assert subject_saved['writesPerformed'] is True
assert all(source_image()[path] == image for path, image in initial_image.items())
subject_image = source_image()

# Preview a closed request, then save its exact exported record operation.
create = save('create.json', {
    'expectedFingerprint': subject_saved['fingerprint'],
    'action': {
        'type': 'create', 'id': hypothesis, 'title': 'Possible copied repair entry',
        'statement': 'The repair entry may have been copied.',
        'subjects': [subject],
        'alternatives': ['A copied entry', 'An incomplete repair'],
        'context': 'Compare the log headings.', 'placement': {'timeline': 'main'}
    }
})
created = cli('create-preview', 'hypotheses', 'preview', '--file', create)['result']
assert created['complete'] is True and created['nonCanonical'] is True
assert created['canApply'] is False and created['writesPerformed'] is False
assert created['preview']['validation']['valid'] is True
assert source_image() == subject_image
operation = created['plan']['operations'][0]
assert operation['type'] == 'record.create'
draft = files / 'hypothesis.yaml'
draft.write_text(operation['draft'])
created_file = created['preview']['files'][0]
saved = cli('hypothesis-saved', 'record', 'create', '--file', draft,
            '--expected-fingerprint', subject_saved['fingerprint'], '--apply',
            '--expected-after-sha256', created_file['afterSha256'])['result']
assert saved['writesPerformed'] is True
assert all(source_image()[path] == image for path, image in subject_image.items())
assert (root / saved['sourcePath']).read_bytes() == created_file['afterSource'].encode()
before = source_image()
assert len(before) == 3
fingerprint = saved['fingerprint']

# Complete author reads and three independently ranked canonical search modes.
catalogue = cli('list', 'hypotheses', 'list', '--status', 'open',
                '--text', 'COPIED')['result']
detail = cli('detail', 'hypotheses', 'detail', hypothesis)['result']
assert catalogue['hypotheses'] == [detail['hypothesis']]
assert detail['hypothesis']['status'] == 'open'
assert detail['hypothesis']['subjects'][0]['id'] == subject
assert detail['hypothesis']['citation']['fingerprint'] == fingerprint
possibility_hits = None
for mode in ['lexical', 'semantic', 'hybrid']:
    search = cli('search-' + mode, 'search', 'repair', '--mode', mode,
                 '--include-hypotheses')['result']
    assert search['matches']['hypothesesIncluded'] is False
    assert all(row['kind'] != 'hypothesis' for row in search['matches']['items'])
    lane = search['hypothesisMatches']
    assert lane['included'] is True and lane['nonCanonical'] is True
    assert [row['id'] for row in lane['items']] == [hypothesis]
    if possibility_hits is not None:
        assert lane['items'] == possibility_hits
    possibility_hits = lane['items']

# Adoption links an existing record; rejection records a literal author reason.
# These are two alternatives for review, with no resolution written to source.
actions = {
    'adopt': {'type': 'adopt', 'hypothesisId': hypothesis,
              'canonicalRecords': [subject],
              'note': 'Explicit author choice, supported by the existing log.'},
    'reject': {'type': 'reject', 'hypothesisId': hypothesis,
               'note': 'Keep the uncertainty unresolved in the story.'}
}
for name, action in actions.items():
    request = save(name + '.json', {'expectedFingerprint': fingerprint, 'action': action})
    report = cli(name + '-preview', 'hypotheses', 'preview', '--file', request)['result']
    plan = save(name + '-plan.json', report['plan'])
    generic = cli(name + '-consequences', 'consequences', 'preview', '--file', plan)['result']
    assert generic == report['preview']
    assert generic['validation']['valid'] is True
    assert report['complete'] is True and report['nonCanonical'] is True
    assert report['writesPerformed'] is False and report['canApply'] is False
    assert len(report['plan']['operations']) == len(generic['files']) == 1
    assert report['plan']['operations'][0]['entityId'] == hypothesis
    expected_status = 'adopted' if name == 'adopt' else 'rejected'
    assert '\nstatus: ' + expected_status + '\n' in generic['files'][0]['afterSource']
    assert generic['files'][0]['beforeSource'] == created_file['afterSource']

assert calls == 14
assert source_image() == before
print('Verified 14 CLI calls: reviewed creation, exact reads, three search modes, and two reusable resolution Plans.')
print('The saved hypothesis remains open; discovery and resolution previews preserved all source bytes and mtimes.')
print('World and reports:', work)
PY
```

Canonical search may rebuild its disposable index. The assertions deliberately
check the authored Markdown source. The hypothesis lane never joins that
canonical corpus, and its occurrence scores are kept separate from canonical
search scores. The [contract](../../docs/HYPOTHESES.md) defines source grammar,
native projection differences, limits, and the settled-record adoption rule.
