# Exact event and grouped consequences

This executable example builds a disposable source world. At exact tick
`9007199254740993`, the focus event changes Ana's mood while another event changes
Bob's mood at the same coordinate. Ana learns a claim at the focus time and has
a delayed focus-caused forgotten transition one tick later. A story-point tests
that the event exists; the local exclusion must keep that condition true.

Set `CONSEQUENCE_BIN` to the absolute path of the current built `wedl-rs` binary.
Run the entire block in one shell with Python 3 available. The example prepares
source/request files under a new temporary directory. Both native commands are
read-only, and the final check compares every fixture file with its original
bytes and modification time.

```sh
set -eu
: "${CONSEQUENCE_BIN:?Set CONSEQUENCE_BIN to the absolute path of the built wedl-rs binary}"
case "$CONSEQUENCE_BIN" in /*) ;; *) exit 2 ;; esac
test -x "$CONSEQUENCE_BIN"
CONSEQUENCE_WORK=$(mktemp -d "${TMPDIR:-/tmp}/wedl-consequence-example.XXXXXX")
export CONSEQUENCE_WORLD="$CONSEQUENCE_WORK/world" CONSEQUENCE_REQUESTS="$CONSEQUENCE_WORK/requests"
mkdir -p "$CONSEQUENCE_WORLD/story" "$CONSEQUENCE_REQUESTS"
python3 - <<'PY'
import hashlib, json, os, pathlib
root = pathlib.Path(os.environ['CONSEQUENCE_WORLD'])
requests = pathlib.Path(os.environ['CONSEQUENCE_REQUESTS'])
world = 'world_00000000000000000000000000'
ana = 'char_00000000000000000000000001'
bob = 'char_00000000000000000000000002'
focus = 'event_00000000000000000000000001'
parallel = 'event_00000000000000000000000002'
knowledge = 'know_00000000000000000000000001'
point = 'sp_00000000000000000000000001'
effect = 'effect_00000000000000000000000001'
tick = 9007199254740993
records = [
    ('world', world, 'Consequence example', '''capabilities: [lore-core-v1]
default_timeline: main
timelines: [{id: main, label: Main}]
profiles: []
state_keys:
  character:
    mood: {type: string}
'''),
    ('character', ana, 'Ana', 'initial_state: {mood: calm}\n'),
    ('character', bob, 'Bob', 'initial_state: {mood: quiet}\n'),
    ('event', focus, 'Focus event', f'''time: {{tick: {tick}, order: 0}}
effects: [{{id: {effect}, target: {ana}, key: mood, operation: set, value: ready}}]
'''),
    ('event', parallel, 'Parallel event', f'''time: {{tick: {tick}, order: 0}}
effects: [{{id: {effect}, target: {bob}, key: mood, operation: set, value: busy}}]
'''),
    ('knowledge', knowledge, 'Authored belief', f'''knower: {ana}
claim: {{key: report, statement: A literal report, truth: unknown}}
transitions:
- {{id: early, time: {{tick: 0}}, state: suspected, confidence: 0.5, acquisition: heard}}
- {{id: learned, time: {{tick: {tick}}}, state: accepted, confidence: 0.9, acquisition: heard, causing_event: {focus}}}
- {{id: delayed, time: {{tick: {tick+1}}}, state: forgotten, confidence: 0.9, acquisition: remembered, causing_event: {focus}}}
'''),
    ('story-point', point, 'Event-presence condition', f'''activation_policy: suggest
lifecycle: {{initial_state: dormant, transitions: []}}
when: {{event: {focus}}}
'''),
]
for kind, identifier, title, fields in records:
    text = f'---\nschema: wedl/v1\nkind: {kind}\nid: {identifier}\ntitle: {title}\n{fields}---\nLiteral authored prose.\n'
    (root / 'story' / (identifier + '.md')).write_text(text)
image = {str(path.relative_to(root)): [hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns]
         for path in root.rglob('*') if path.is_file()}
(requests / 'before.json').write_text(json.dumps(image))
PY
"$CONSEQUENCE_BIN" --repo "$CONSEQUENCE_WORLD" --compact status > "$CONSEQUENCE_REQUESTS/status.json"
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['CONSEQUENCE_REQUESTS'])
fingerprint = json.loads((p / 'status.json').read_text())['fingerprint']
focus = 'event_00000000000000000000000001'
ana = 'char_00000000000000000000000001'
effect = 'effect_00000000000000000000000001'
at = {'timeline': 'main', 'tick': '9007199254740994', 'order': '0'}
event = {'expectedFingerprint': fingerprint, 'eventId': focus, 'at': at,
         'maxItems': 100, 'maxCharacters': 100000}
patch = f'kind: event\nbody: Literal revised source.\neffects: [{{id: {effect}, target: {ana}, key: mood, operation: set, value: waiting}}]\n'
grouped = {'plan': {'expectedFingerprint': fingerprint,
                    'operations': [{'type': 'record.update', 'entityId': focus, 'patch': patch}]},
           'at': at, 'focusEventIds': [], 'maxItems': 100, 'maxCharacters': 100000}
(p / 'event.json').write_text(json.dumps(event, indent=2) + '\n')
(p / 'grouped.json').write_text(json.dumps(grouped, indent=2) + '\n')
PY
"$CONSEQUENCE_BIN" --repo "$CONSEQUENCE_WORLD" --compact event-consequences --file "$CONSEQUENCE_REQUESTS/event.json" > "$CONSEQUENCE_REQUESTS/event-report.json"
"$CONSEQUENCE_BIN" --repo "$CONSEQUENCE_WORLD" --compact grouped-consequences --file "$CONSEQUENCE_REQUESTS/grouped.json" > "$CONSEQUENCE_REQUESTS/grouped-report.json"
python3 - <<'PY'
import hashlib, json, os, pathlib
root = pathlib.Path(os.environ['CONSEQUENCE_WORLD'])
p = pathlib.Path(os.environ['CONSEQUENCE_REQUESTS'])
event = json.loads((p / 'event-report.json').read_text())['result']
grouped = json.loads((p / 'grouped-report.json').read_text())['result']
assert event['eventTime']['tick'] == '9007199254740993'
assert event['at']['tick'] == '9007199254740994'
assert event['comparison']['eventRetained'] is True
assert sorted(row['subject']['kind'] for row in event['changes']) == ['knowledge', 'state']
assert [row['atEventTime'] for row in event['causedTransitions']] == [True, False]
belief = next(row for row in event['currentAtHorizon'] if row['subject']['kind'] == 'knowledge')
assert belief['snapshot']['payload']['state'] == 'forgotten'
assert belief['supersession'] == 'unchanged'
assert len(grouped['eventGroups']) == 1
group = grouped['eventGroups'][0]
assert len(group['changes']) == 1 and group['changes'][0]['subject']['kind'] == 'state'
assert [section['section'] for row in group['recordChanges'] for section in row['sections']] == ['effects']
assert [section['section'] for row in grouped['unattributedRecordChanges'] for section in row['sections']] == ['bodyMarkdown']
for report in [event, grouped]:
    assert report['writesPerformed'] is False and report['canApply'] is False
    assert len(report['requestSha256']) == len(report['reportSha256']) == 64
image = {str(path.relative_to(root)): [hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns]
         for path in root.rglob('*') if path.is_file()}
assert image == json.loads((p / 'before.json').read_text())
print('Exact event exclusion, delayed history, grouped effects and unattributed prose verified; fixture unchanged.')
print('Reports:', p)
PY
```

The grouped preview uses the existing native source-edit codec. It changes the
focus effect in an in-memory candidate, groups that explicit source section and
its state-value difference, and leaves the independent prose edit unattributed.
It does not execute the source patch. The event-presence condition produces no
local change because the focus event remains present in both folds. The
[v1 contract](../../docs/EVENT_CONSEQUENCES.md) defines source admission, exact
typed values, complete bounds, supersession, and current native limits.
