# Committed reader disclosure

Run this single block with the built `wedl-rs`, Git and Python 3 on `PATH`.
It creates a throwaway world and uses the native manuscript preview/save commands
for every private source. It then checks cumulative record/section/text disclosure,
ordinary public scope, exact committed source, paging, Unicode search, and explicit
author simulation. Operator policies and request files stay outside `story/` and
`author/`. See [REVEAL_PLANS.md](../../docs/REVEAL_PLANS.md) for source grammar and
[REVEALS.md](../../docs/REVEALS.md) for the projection boundary.

```sh
set -eu
reader_example_dir="$(mktemp -d)"
python3 - "$reader_example_dir" <<'PY'
import json
import pathlib
import secrets
import shlex
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
world = root / 'world'
operator = root / 'operator'
requests = root / 'requests'
operator.mkdir()
requests.mkdir()
calls = 0
protocol = 'wedl-reader-presentation/v1'
world_id = 'world_00000000000000000000000000'
place = 'loc_00000000000000000000000001'
first = 'segment_00000000000000000000000001'
second = 'segment_00000000000000000000000002'
work_id = 'work_00000000000000000000000001'
chapter = 'chapter_00000000000000000000000001'
plan_id = 'reveal_00000000000000000000000001'
section_id = 'description_00000000000000000000000001'
private_id = 'description_00000000000000000000000002'
future_id = 'description_00000000000000000000000003'
text_id = 'revealtext_00000000000000000000000001'

def cli(*args, expected=0):
    global calls
    calls += 1
    result = subprocess.run(['wedl-rs', '--compact', '--repo', str(world), *map(str, args)],
                            capture_output=True, text=True)
    assert result.returncode == expected, result.stderr or result.stdout
    return json.loads(result.stdout)

def file(name, value, directory=requests):
    path = directory / (name + '.json')
    path.write_text(json.dumps(value, ensure_ascii=False), encoding='utf-8')
    return path

def request(name, group, operation, value):
    return cli(group, operation, '--file', file(name, value))['result']

def catalogue():
    return request('catalogue', 'manuscripts', 'list',
                   {'revision': None, 'kind': None, 'maxItems': 1024})

def detail(identifier):
    return request('detail', 'manuscripts', 'detail',
                   {'revision': None, 'id': identifier, 'expectedSha256': None})['document']

def save(name, source, previous=None):
    value = {'expectedFingerprint': cli('status')['fingerprint'],
             'expectedAuthorFingerprint': catalogue()['authorFingerprint'],
             'expectedSha256': previous, 'source': source}
    preview = request(name, 'manuscripts', 'preview-save', value)
    assert preview['afterSource'] == source and not preview['writesPerformed']
    saved = request(name, 'manuscripts', 'save', value)
    assert saved['afterSource'] == source and saved['canApply'] is False

def markdown(fields, body):
    # JSON is a YAML mapping, so this needs no Python YAML dependency.
    return '---\n' + json.dumps(fields, ensure_ascii=False) + '\n---\n' + body

def time(tick):
    return {'timeline': 'main', 'tick': tick, 'order': 0}

def description(identifier, heading, words, audience, valid):
    return {'id': identifier, 'heading': heading, 'markdown': words, 'valid': valid,
            'audience': audience, 'perspectives': ['ordinary']}

cli('init', world, '--title', 'Reader example')
(world / 'story' / 'world.md').write_text(markdown({
    'schema': 'wedl/v1', 'kind': 'world', 'id': world_id, 'title': 'Reader example',
    'capabilities': ['descriptions-v1'], 'default_timeline': 'main',
    'timelines': [{'id': 'main', 'label': 'Main'}], 'state_keys': {}
}, 'Private world notes.\n'), encoding='utf-8')
place_source = markdown({
    'schema': 'wedl/v1', 'kind': 'location', 'id': place, 'title': 'Owner spoiler title',
    'descriptions': [
        description(section_id, 'First glimpse', 'A Straße crosses the square.',
                    ['public'], {'kind': 'timeless'}),
        description(private_id, 'Private explanation', 'PRIVATE_SECTION_SECRET',
                    ['author'], {'kind': 'timeless'}),
        description(future_id, 'Later account', 'FUTURE_PUBLIC_ACCOUNT', ['public'],
                    {'kind': 'interval', 'timeline': 'main', 'start': time(50)})]
}, '# Public\nA public square.\n\n# Author notes\nPRIVATE_BODY_SECRET\n')
place_path = world / 'story' / 'place.md'
place_path.write_text(place_source, encoding='utf-8')
fingerprint = cli('status')['fingerprint']

for identifier, tick in [(first, 20), (second, -20)]:
    source = markdown({'schema': 'wedl-author/v1', 'kind': 'manuscript-segment',
                       'id': identifier, 'title': 'PRIVATE_SEGMENT_TITLE',
                       'source_links': [{'kind': 'section', 'owner_id': place,
                                         'id': private_id}], 'placement': time(tick)},
                      '\r\nPRIVATE_MANUSCRIPT_PROSE 🌲\r\nTrailing  ')
    save(identifier, source)
    assert detail(identifier)['rawSource'] == source

work_source = markdown({'schema': 'wedl-author/v1', 'kind': 'work', 'id': work_id,
                       'title': 'PRIVATE_WORK_TITLE',
                       'duplicate_policy': 'allow-explicit-reuse',
                       'chapters': [{'id': chapter, 'title': 'PRIVATE_CHAPTER_TITLE',
                                     'segments': [{'kind': 'author-record', 'id': identifier}
                                                  for identifier in [first, second, first]]}]},
                      'PRIVATE_WORK_NOTES\r\n')
save('work', work_source)
literal = '\r\n  Explicit reveal text æ🌲\r\nTrailing  '
admissions = [
    {'kind': 'section', 'target': {'kind': 'section', 'owner_id': place, 'id': section_id}},
    {'kind': 'record', 'target': {'kind': 'record', 'id': place}},
    {'kind': 'text', 'id': text_id, 'markdown': literal}]
plan_source = markdown({'schema': 'wedl-author/v1', 'kind': 'reveal-plan', 'id': plan_id,
                       'title': 'PRIVATE_PLAN_TITLE',
                       'work': {'kind': 'author-record', 'id': work_id},
                       'entries': [{'milestone': {'chapter_id': chapter, 'segment_index': index},
                                    'admissions': [admission]}
                                   for index, admission in enumerate(admissions)]},
                      '\r\nPRIVATE_PLAN_NOTES 🌲\r\nTrailing  ')
save('plan', plan_source)
assert detail(plan_id)['rawSource'] == plan_source
assert cli('status')['fingerprint'] == fingerprint
resolved = request('resolve', 'reveal-plans', 'resolve',
                   {'revision': None, 'planId': plan_id, 'expectedAuthorFingerprint': None,
                    'maxOccurrences': 4096, 'maxCharacters': 100000})
assert resolved['occurrenceCount'] == 3 and not resolved['readerAuthorized']
assert [item['segmentId'] for item in resolved['chapters'][0]['occurrences']] == [first, second, first]

def git(*args):
    return subprocess.check_output(['git', '-C', str(world), *args], text=True).strip()

git('init', '-q')
git('add', '--', 'story', 'author')
git('-c', 'user.name=Example', '-c', 'user.email=example@invalid',
    '-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'Original reader edition')
revision = git('rev-parse', 'HEAD')
scope = {'worldId': world_id, 'recordIds': [place], 'sections': None}
common = {'schema': 'wedl-reader-policy/v1', 'revision': revision, 'workId': work_id,
          'planId': plan_id, 'canonicalScope': scope,
          'generation': 'example-1', 'maxOccurrence': 2}
public_policy = file('public-policy', dict(common, kind='public', publicationId='example',
    cursorSecret=secrets.token_hex(32), at={'timeline': 'main', 'tick': '10', 'order': '0'},
    textPolicy='all-at-pinned-plan'), operator)
simulation_policy = file('simulation-policy', dict(common, kind='author-simulation',
    cursorSecret=secrets.token_hex(32), principalId='example-author',
    authorPermissions=scope), operator)
public_policy.chmod(0o600)
simulation_policy.chmod(0o600)

def public(occurrence, *, limit=100, cursor=None, query=None, policy=public_policy):
    value = {'protocol': protocol, 'publicationId': 'example', 'revision': revision,
             'occurrence': occurrence, 'limit': limit, 'cursor': cursor}
    operation = 'page' if query is None else 'search'
    if query is not None:
        value['query'] = query
    path = file('reader-' + operation, value)
    args = ['reader', operation, '--file', path]
    if policy is not None:
        args += ['--policy', policy]
    return cli(*args, expected=0 if policy is not None else 2)

assert public(0, policy=None)['outcome'] == 'unavailable'
opening = public(0)
assert opening['count'] == 1 and opening['items'][0]['kind'] == 'section'
assert opening['items'][0]['title'] == 'First glimpse'
assert opening['items'][0]['ownerId'] == place
assert 'Owner spoiler title' not in json.dumps(opening)
middle = public(1)
assert [item['kind'] for item in middle['items']] == ['section', 'record']
complete = public(2)
assert [item['kind'] for item in complete['items']] == ['section', 'record', 'text']
assert complete['items'][-1]['markdown'] == literal
assert complete['navigation']['nextOccurrence'] is None
for hidden in ['PRIVATE_', 'FUTURE_PUBLIC_ACCOUNT', work_id, plan_id, first, chapter,
               'fingerprint', 'blobOid', 'sourcePath']:
    assert hidden not in json.dumps(complete, ensure_ascii=False), hidden
assert public(2, query='STRASSE')['items'] == opening['items']
assert public(2, query='PRIVATE_SECTION_SECRET')['count'] == 0

page_one = public(2, limit=1)
page_two = public(2, limit=1, cursor=page_one['nextCursor'])
page_three = public(2, limit=1, cursor=page_two['nextCursor'])
assert page_one['count'] == page_two['count'] == page_three['count'] == 3
assert page_one['items'] + page_two['items'] + page_three['items'] == complete['items']
assert page_three['nextCursor'] is None

def preview(tick, text_policy):
    value = {'protocol': protocol, 'revision': revision, 'workId': work_id,
             'planId': plan_id, 'occurrence': 2, 'operation': 'page',
             'at': {'timeline': 'main', 'tick': str(tick), 'order': '0'},
             'textPolicy': text_policy, 'limit': 100, 'cursor': None}
    return cli('reader', 'preview', '--policy', simulation_policy,
               '--file', file('preview', value))

assert preview(10, 'none')['items'] == middle['items']
later = preview(50, 'none')
assert later['mode'] == 'author-preview' and later['count'] == 3
assert later['items'][-1]['markdown'] == 'FUTURE_PUBLIC_ACCOUNT'
assert all(item['kind'] != 'text' for item in later['items'])
assert 'PRIVATE_' not in json.dumps(later)
assert preview(50, 'all-at-pinned-plan')['items'][-1]['markdown'] == literal

# Live source changes do not change the installed original commit.
save('changed-plan', plan_source + '\r\nDIRTY_PRIVATE_NOTE', detail(plan_id)['sourceSha256'])
place_path.write_text(place_source.replace('A public square.', 'DIRTY_LIVE_PUBLIC_BODY'), encoding='utf-8')
assert cli('status')['fingerprint'] != fingerprint
assert public(2) == complete
assert public(2, limit=1, cursor=page_one['nextCursor']) == page_two

evidence = {'passed': True, 'cliCalls': calls, 'revision': revision,
            'occurrences': 3, 'counts': [opening['count'], middle['count'], complete['count']],
            'previewLaterCount': later['count'], 'fullCasefoldSearch': True,
            'sectionOnlyOwnerTitleExcluded': True, 'exactText': True,
            'originalCommitIgnoresDirtySource': True, 'cursorContinuation': True,
            'noPolicyUnavailable': True, 'world': str(world),
            'publicPolicy': str(public_policy), 'simulationPolicy': str(simulation_policy)}
evidence_path = root / 'reader-example-evidence.json'
evidence_path.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
print(f'Passed reader disclosure checks ({calls} CLI calls).')
print(f'Evidence: {evidence_path}')
print('Isolated reader:', shlex.join(['wedl-rs', '--repo', str(world), 'serve-reader',
                                      '--port', '8766', '--policy', str(public_policy)]))
print('Author simulation:', shlex.join(['wedl-rs', '--repo', str(world), 'serve',
                                        '--port', '8765', '--reader-simulation-policy',
                                        str(simulation_policy)]))
PY
```

The printed `serve-reader` command starts a separate loopback reader origin on
port 8766. Its page/search routes use only the installed public policy and expose
no author source or session routes. The printed `serve --reader-simulation-policy`
command starts the author browser on port 8765 with an independent simulation
policy. Its preview request explicitly selects both story time and text policy;
ordinary permissions still intersect the two installed scopes. Neither command
implicitly grants access to a plan. Policies are loaded at process startup, so
editing a policy file does not change an already running service.

The original commit remains the edition pin even after the example dirties the
working files. Keep operator policies private; their freshly generated cursor
secrets are not reader inputs. Public requests select only the publication,
revision, occurrence, page size, cursor and optional search query. They cannot
choose a plan, source path, story horizon, scope or text policy.

An embedding revokes a running native service with
`PresentationService::install_public(None)` or `install_simulation(None)`.
Replacement/revocation also invalidates an in-flight prepared result before
emission. No HTTP policy installation or revocation endpoint exists. A standalone
CLI/server process is stopped and restarted with the intended policy; starting
`serve-reader` without one remains unavailable.
