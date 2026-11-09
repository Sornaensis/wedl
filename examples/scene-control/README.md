# Coordinated scenes and carried objects

This example previews all four controls against one disposable world. Ana and
Bo occupy different active scenes. Ana carries a bag containing a listed object;
a stationary prop stays behind when she joins Bo. The handoff occurs at exact
tick `9007199254740993`, order `8`, immediately after old presence through order
`7`. Each request is an independent alternative against the same source.

Set `SCENE_CONTROL_BIN` to the absolute path of the built `wedl-rs` binary, then
run the complete block with Python 3 available. It exports an ordinary Plan and
replays its preview. It applies no source changes.

```sh
set -eu
: "${SCENE_CONTROL_BIN:?Set SCENE_CONTROL_BIN to the absolute path of wedl-rs}"
case "$SCENE_CONTROL_BIN" in /*) ;; *) exit 2 ;; esac
test -x "$SCENE_CONTROL_BIN"
SCENE_CONTROL_WORK=$(mktemp -d "${TMPDIR:-/tmp}/wedl-scene-control-example.XXXXXX")
export SCENE_CONTROL_WORLD="$SCENE_CONTROL_WORK/world" SCENE_CONTROL_REQUESTS="$SCENE_CONTROL_WORK/requests"
mkdir -p "$SCENE_CONTROL_WORLD/story" "$SCENE_CONTROL_REQUESTS"
python3 - <<'PY'
import hashlib, json, os, pathlib
root = pathlib.Path(os.environ['SCENE_CONTROL_WORLD'])
requests = pathlib.Path(os.environ['SCENE_CONTROL_REQUESTS'])
def identifier(prefix, number): return prefix + '_' + str(number).zfill(26)
world, ana, bo = identifier('world', 1), identifier('char', 1), identifier('char', 2)
origin, destination = identifier('loc', 1), identifier('loc', 2)
bag, contents, prop = [identifier('obj', n) for n in (1, 2, 3)]
scene, other = identifier('scene', 1), identifier('scene', 2)
tick = 9007199254740993
def point(order): return {'timeline': 'main', 'tick': tick, 'order': order}
def entity(kind): return {'type': 'entity', 'kind': kind}
def record(kind, rid, title, **fields):
    data = {'schema': 'wedl/v1', 'kind': kind, 'id': rid, 'title': title,
            'status': 'canonical', **fields}
    path = root / 'story' / (rid + '.md')
    path.write_text('---\n' + json.dumps(data, ensure_ascii=False) + '\n---\nExact authored prose.\n')
record('world', world, 'Scene-control example', capabilities=[], threads=[],
       default_timeline='main', timelines=[{'id': 'main', 'label': 'Main'}],
       current_time=point(7), state_keys={
           'character': {'location': entity('location')},
           'object': {'location': entity('location'), 'holder': entity('character'),
                      'container': entity('object')}})
record('location', origin, 'Harbor')
record('location', destination, 'Hill')
record('character', ana, 'Ana', initial_state={'location': {'entity': origin}})
record('character', bo, 'Bo', initial_state={'location': {'entity': destination}})
record('object', bag, 'Carried bag', initial_state={'holder': {'entity': ana}})
record('object', contents, 'Bag contents', initial_state={'container': {'entity': bag}})
record('object', prop, 'Stationary prop', initial_state={'location': {'entity': origin}})
for rid, title, place, cast, objects in [
        (scene, 'Harbor watch', origin, ana, [bag, contents, prop]),
        (other, 'Hill watch', destination, bo, [])]:
    record('scene', rid, title, status='active', location=place,
           time={'start': point(0), 'current': point(7), 'end': None},
           participants=[{'character': cast, 'from': point(0)}], objects=objects,
           environments=[], story_points=[], outcome_events=[], observations=[], conversations=[])
image = {str(p.relative_to(root)): [hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mtime_ns]
         for p in root.rglob('*') if p.is_file()}
(requests / 'before.json').write_text(json.dumps(image))
PY
"$SCENE_CONTROL_BIN" --compact --repo "$SCENE_CONTROL_WORLD" validate > "$SCENE_CONTROL_REQUESTS/validation.json"
"$SCENE_CONTROL_BIN" --compact --repo "$SCENE_CONTROL_WORLD" status > "$SCENE_CONTROL_REQUESTS/status.json"
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['SCENE_CONTROL_REQUESTS'])
assert json.loads((p / 'validation.json').read_text())['result']['valid'] is True
fingerprint = json.loads((p / 'status.json').read_text())['fingerprint']
def identifier(prefix, number): return prefix + '_' + str(number).zfill(26)
ana, scene, other = identifier('char', 1), identifier('scene', 1), identifier('scene', 2)
destination = identifier('loc', 2)
event = {'id': identifier('event', 9), 'title': 'Ana arrives'}
actions = {
    'create': {'type': 'scene.create', 'sceneId': identifier('scene', 9),
               'title': 'New hill scene', 'locationId': destination,
               'characterIds': [ana], 'event': event},
    'advance': {'type': 'scene.advance', 'sceneId': scene},
    'close': {'type': 'scene.close', 'sceneId': scene},
    'move': {'type': 'character.move', 'sceneId': other,
             'characterIds': [ana], 'locationId': destination, 'event': event},
}
for name, action in actions.items():
    request = {'expectedFingerprint': fingerprint,
               'at': {'timeline': 'main', 'tick': '9007199254740993', 'order': '8'},
               'action': action}
    (p / (name + '.json')).write_text(json.dumps(request, indent=2) + '\n')
PY
for control in create advance close move; do
  "$SCENE_CONTROL_BIN" --compact --repo "$SCENE_CONTROL_WORLD" scene-control preview --file "$SCENE_CONTROL_REQUESTS/$control.json" > "$SCENE_CONTROL_REQUESTS/$control-report.json"
done
python3 - <<'PY'
import json, os, pathlib
p = pathlib.Path(os.environ['SCENE_CONTROL_REQUESTS'])
for name in ('create', 'advance', 'close', 'move'):
    report = json.loads((p / (name + '-report.json')).read_text())['result']
    assert report['request'] == json.loads((p / (name + '.json')).read_text())
    assert report['complete'] is True and report['writesPerformed'] is False and report['canApply'] is False
    assert report['preview']['validation']['valid'] is True
move = json.loads((p / 'move-report.json').read_text())['result']
assert len(move['plan']['operations']) == 4
(p / 'move-plan.json').write_text(json.dumps(move['plan'], indent=2) + '\n')
PY
"$SCENE_CONTROL_BIN" --compact --repo "$SCENE_CONTROL_WORLD" consequences preview --file "$SCENE_CONTROL_REQUESTS/move-plan.json" --max-characters 100000 > "$SCENE_CONTROL_REQUESTS/replayed.json"
python3 - <<'PY'
import hashlib, json, os, pathlib
root = pathlib.Path(os.environ['SCENE_CONTROL_WORLD'])
p = pathlib.Path(os.environ['SCENE_CONTROL_REQUESTS'])
move = json.loads((p / 'move-report.json').read_text())['result']
assert move['preview'] == json.loads((p / 'replayed.json').read_text())['result']
image = {str(path.relative_to(root)): [hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns]
         for path in root.rglob('*') if path.is_file()}
assert image == json.loads((p / 'before.json').read_text())
print('All four controls and ordinary Plan replay passed; source bytes and mtimes are unchanged.')
print('Requests and reports:', p)
PY
```

The [contract](../../docs/SCENE_CONTROL.md) describes inclusive attendance,
stationary-object refusal, exact times and complete candidate reconciliation.
Publish a chosen exported Plan only through the existing reviewed outcome flow;
these alternative previews are not sequential instructions to apply together.
