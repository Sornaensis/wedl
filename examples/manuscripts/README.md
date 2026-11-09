# Manuscript source and literal work order

Run the single block below with the built `wedl-rs` on `PATH`. It creates a
throwaway world, saves two private segments and an explicitly reused work, checks
exact bytes and canonical separation, then compares current and historical work
resolution. Git supplies original committed source; the manuscript writer itself
does not create commits. See [MANUSCRIPT.md](../../docs/MANUSCRIPT.md).

```sh
set -eu
manuscript_example_dir="$(mktemp -d)"
python3 - "$manuscript_example_dir" <<'PY'
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
world = root / 'world'
calls = 0

def cli(*args):
    global calls
    calls += 1
    result = subprocess.run(['wedl-rs', '--compact', '--repo', str(world), *map(str, args)],
                            capture_output=True, text=True, check=True)
    return json.loads(result.stdout)

def request(name, operation, value):
    path = root / (name + '.json')
    path.write_text(json.dumps(value), encoding='utf-8')
    return cli('manuscripts', operation, '--file', path)['result']

def catalogue(revision=None):
    return request('list', 'list', {'revision': revision, 'kind': None, 'maxItems': 1024})

def detail(identifier):
    return request('detail', 'detail', {'revision': None, 'id': identifier,
                                      'expectedSha256': None})['document']

def save(name, source, previous=None):
    value = {'expectedFingerprint': fingerprint,
             'expectedAuthorFingerprint': catalogue()['authorFingerprint'],
             'expectedSha256': previous, 'source': source}
    preview = request(name, 'preview-save', value)
    assert preview['afterSource'] == source and not preview['writesPerformed']
    saved = request(name, 'save', value)
    assert saved['afterSource'] == source and saved['canApply'] is False
    return saved

def resolve(revision=None):
    return request('resolve', 'resolve', {'revision': revision, 'workId': work_id,
                   'expectedAuthorFingerprint': catalogue(revision)['authorFingerprint'],
                   'maxOccurrences': 4096, 'maxCharacters': 100000})

cli('init', world, '--title', 'Private manuscript example')
fingerprint = cli('status')['fingerprint']
first = 'segment_00000000000000000000000001'
second = 'segment_00000000000000000000000002'
work_id = 'work_00000000000000000000000001'
chapter = 'chapter_00000000000000000000000001'

def segment(identifier, placement=''):
    return ('---\r\nschema: wedl-author/v1\r\nkind: manuscript-segment\r\n'
            f'id: {identifier}\r\ntitle: Private segment\r\nsource_links: []\r\n'
            + placement + '---\r\nExact private prose 🌲\r\nTrailing  ')

first_source = segment(first, 'placement: {timeline: main, tick: 10, order: 0}\r\n')
second_source = segment(second)
save('first', first_source)
save('second', second_source)
work_source = (f'---\nschema: wedl-author/v1\nkind: work\nid: {work_id}\n'
               'title: Literal authored order\nduplicate_policy: allow-explicit-reuse\n'
               f'chapters:\n  - id: {chapter}\n    title: Opening\n    segments:\n'
               + ''.join(f'      - {{kind: author-record, id: {identifier}}}\n'
                         for identifier in [first, second, first])
               + '---\nPrivate work notes.\n')
save('work', work_source)
assert detail(first)['rawSource'] == first_source
original = resolve()
assert [item['segmentId'] for item in original['chapters'][0]['occurrences']] == [first, second, first]
assert original['occurrenceCount'] == 3 and original['uniqueSegmentCount'] == 2
assert original['readerAuthorized'] is False
assert original['segments'][0]['placement']['tick'] == '10'
assert cli('status')['fingerprint'] == fingerprint

def git(*args):
    return subprocess.check_output(['git', '-C', str(world), *args], text=True).strip()

git('init', '-q')
git('add', '--', 'story', 'author')
git('-c', 'user.name=Example', '-c', 'user.email=example@invalid',
    '-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'Original private manuscript')
base = git('rev-parse', 'HEAD')
old = detail(first)
save('changed', first_source + '\r\nChanged private ending.', old['sourceSha256'])
current = resolve()
historical = resolve(base)
assert current['authorFingerprint'] != historical['authorFingerprint']
assert historical['revision'] == base
assert historical['segments'][0]['blobOid'] is not None
assert historical['segments'][0]['bodyMarkdown'] == original['segments'][0]['bodyMarkdown']
assert current['segments'][0]['bodyMarkdown'].endswith('Changed private ending.')
assert cli('status')['fingerprint'] == fingerprint
print(f'Passed manuscript source, reuse and history checks ({calls} CLI calls).')
print(f'Example world: {world}')
PY
```

The two appearances of the first segment have distinct occurrence positions but
share its source. Placement does not sort a work. Private prose and static links
do not add character knowledge or grant reader disclosure. The separate
[reader example](../readers/README.md) adds occurrence-bound reveal plans and
explicitly installed committed reader policies.

The Manuscripts browser tab offers the same catalogue, exact source, complete work
resolution, preview and request downloads. Save with the CLI. The Retcons tab
accepts typed `author-record` targets and includes admitted manuscript source links,
POV references and work occurrences in dependency analysis.
