'use strict';

// Actual browser/CLI comparison of the selected-scene character packet.
// Optional paths: WEDL_TEST_BINARY, WEDL_TEST_CHROMIUM,
// WEDL_TEST_PLAYWRIGHT, WEDL_TEST_ARTIFACT_DIR. Fixture writes are explicit.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const {spawn, spawnSync} = require('node:child_process');
const {chromium} = require(process.env.WEDL_TEST_PLAYWRIGHT || 'playwright');

const root = path.resolve(__dirname, '..');
const binary = path.resolve(process.env.WEDL_TEST_BINARY || path.join(root, 'target', 'debug', process.platform === 'win32' ? 'wedl-rs.exe' : 'wedl-rs'));
const artifactDirectory = process.env.WEDL_TEST_ARTIFACT_DIR || os.tmpdir();
fs.mkdirSync(artifactDirectory, {recursive: true});
const runDirectory = fs.mkdtempSync(path.join(artifactDirectory, 'wedl-writing-present-'));
const world = path.join(runDirectory, 'world');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report = {runDirectory, binary, binarySha256: hash(binary), checks: [], api: [], cliReads: [], pageErrors: [], httpFailures: [], requestFailures: [], fixtureEdits: [], screenshots: []};
const ids = {world: 'world_00000000000000000000000000', actor: 'char_00000000000000000000000001', peer: 'char_00000000000000000000000002', place: 'loc_00000000000000000000000001', scene: 'scene_00000000000000000000000001', object: 'obj_00000000000000000000000001', bag: 'obj_00000000000000000000000002', hidden: 'obj_00000000000000000000000003', environment: 'env_00000000000000000000000001', relationship: 'rel_00000000000000000000000001'};
const tick = '9007199254740993', condition = '9007199254740995';
const at = {timeline: 'main', tick, order: '7'};
const point = order => '{timeline: main, tick: ' + tick + ', order: ' + order + '}';
const equal = (name, actual, expected) => { assert.deepEqual(actual, expected, name); report.checks.push(name); };
const check = (name, value) => { assert.ok(value, name); report.checks.push(name); };
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
let server, browser;

function cli(arguments_) {
  report.cliReads.push(arguments_);
  const value = spawnSync(binary, ['--compact', '--repo', world, ...arguments_], {encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, timeout: 20000});
  assert.equal(value.status, 0, value.stderr || value.stdout);
  return JSON.parse(value.stdout);
}

function record(kind, id, fields) {
  const file = path.join(world, 'story', id + '.md');
  fs.writeFileSync(file, '---\nschema: wedl/v1\nkind: ' + kind + '\nid: ' + id + '\ntitle: SOURCE_TITLE_' + id + '\n' + fields + '---\n\r\nPRIVATE_BODY_' + id + ' 🌲\r\n');
  return file;
}

function fixture() {
  fs.mkdirSync(path.join(world, 'story'), {recursive: true});
  record('world', ids.world, 'default_timeline: main\ntimelines: [{id: main, label: Main}]\ncapabilities: []\ncurrent_time: ' + point(7) + '\nrelationship_metrics: {trust: {min: -1.0, max: 1.0, default: 0.0}}\nstate_keys:\n  character:\n    location: {type: entity, kind: location, exclusive_group: placement}\n  object:\n    condition: {type: integer}\n    holder: {type: entity, kind: character, exclusive_group: placement}\n    location: {type: entity, kind: location, exclusive_group: placement}\n    container: {type: entity, kind: object, exclusive_group: placement}\n');
  record('location', ids.place, '');
  for (const id of [ids.actor, ids.peer]) record('character', id, 'initial_state: {location: {entity: ' + ids.place + '}}\n');
  record('object', ids.bag, 'initial_state: {holder: {entity: ' + ids.actor + '}}\n');
  record('object', ids.object, 'initial_state: {container: {entity: ' + ids.bag + '}, condition: ' + condition + '}\n');
  const hidden = record('object', ids.hidden, 'audience: [author]\nperspectives: [author]\ninitial_state: {location: {entity: ' + ids.place + '}, condition: 777}\n');
  record('scene', ids.scene, 'status: active\nlocation: ' + ids.place + '\ntime: {start: ' + point(0) + ', current: ' + point(7) + '}\nparticipants: [{character: ' + ids.actor + '}, {character: ' + ids.peer + '}]\nobjects: [' + [ids.object, ids.bag, ids.hidden].join(', ') + ']\nenvironments: []\nconversations: []\nobservations: []\noutcome_events: []\n');
  record('environment', ids.environment, 'status: canonical\ntargets: [' + ids.scene + ', ' + ids.place + ']\ntime: {start: ' + point(0) + ', end: ' + point(9) + '}\nconditions: {darkness: true, marker: ' + tick + '}\nsensory: ["Wind 🌲"]\n');
  record('relationship', ids.relationship, 'status: canonical\nfrom: ' + ids.actor + '\nto: ' + ids.peer + '\nrelationship_kind: trust\ntransitions:\n  - id: rt_00000000000000000000000001\n    time: ' + point(3) + '\n    metrics: {trust: 0.75}\n    facets: ["shared work 🌲"]\n    relationship_status: active\n    note: PRIVATE_RELATIONSHIP_NOTE\n');
  return hidden;
}

function snapshot() {
  const files = [];
  function visit(directory) {
    for (const entry of fs.readdirSync(directory, {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(file);
      else if (entry.isFile()) {
        const stat = fs.statSync(file, {bigint: true});
        files.push({path: path.relative(world, file), bytes: stat.size.toString(), mtimeNs: stat.mtimeNs.toString(), sha256: hash(file)});
      }
    }
  }
  visit(world);
  return files;
}

async function freePort() {
  const listener = net.createServer();
  await new Promise((resolve, reject) => { listener.once('error', reject); listener.listen(0, '127.0.0.1', resolve); });
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  return port;
}

function nativeContext(fingerprint, lens) {
  const frame = {fingerprint, work: null, sceneId: ids.scene, povId: ids.actor, at, lens};
  const file = path.join(runDirectory, 'frame-' + lens + '.json');
  fs.writeFileSync(file, JSON.stringify(frame));
  return {frame, value: cli(['writing', 'context', '--file', file, '--max-characters', '100000']).result};
}

async function readContext(page) {
  await page.locator('#writing-context').click();
  await page.waitForFunction(() => document.querySelector('#writing-status').textContent === 'Complete context ready.');
  return JSON.parse(await page.locator('#writing-character-prompt').textContent());
}

async function main() {
  const hidden = fixture();
  const before = snapshot(); report.sourceBefore = before;
  const status = cli(['status']);
  const expected = nativeContext(status.fingerprint, 'character');
  const dramatic = nativeContext(status.fingerprint, 'dramatic-irony');
  const present = expected.value.characterPrompt.present;
  check('Native character packet includes selected-scene present facts', present && Array.isArray(present.objects) && Array.isArray(present.environments) && Array.isArray(present.relationships));
  equal('Exactly admitted physically present objects', present.objects.map(item => item.objectId).sort(), [ids.object, ids.bag].sort());
  equal('Exact large integer condition', present.objects.find(item => item.objectId === ids.object).state.condition, condition);
  equal('Active direct-target environment appears without a listed environment grant', present.environments.map(item => item.environmentId), [ids.environment]);
  equal('Literal sensory text survives', present.environments[0].sensory, ['Wind 🌲']);
  equal('Own outgoing relationship appears', present.relationships.map(item => item.relationshipId), [ids.relationship]);
  equal('Exact literal relationship metric', present.relationships[0].metrics.trust, 0.75);
  equal('Dramatic and character lenses have identical character components', dramatic.value.characterPrompt, expected.value.characterPrompt);
  const serialized = JSON.stringify(expected.value.characterPrompt);
  check('Character component omits hidden IDs, titles, bodies and relationship notes', ![ids.hidden, 'SOURCE_TITLE_', 'PRIVATE_BODY_', 'PRIVATE_RELATIONSHIP_NOTE', 'sourcePath', 'fingerprint', 'revision', 'blobOid'].some(value => serialized.includes(value)));
  const ordinaryBefore = cli(['character', 'context', ids.actor, '--timeline', at.timeline, '--tick', at.tick, '--order', at.order, '--max-characters', '100000']);
  equal('Ordinary character context gains no selected-scene component', Object.hasOwn(ordinaryBefore.result, 'present'), false);

  const origin = 'http://127.0.0.1:' + await freePort(); report.origin = origin;
  server = spawn(binary, ['--repo', world, 'serve', '--host', '127.0.0.1', '--port', new URL(origin).port], {stdio: ['ignore', 'pipe', 'pipe']});
  for (const stream of [server.stdout, server.stderr]) stream.on('data', chunk => { report.serverLog = (report.serverLog || '') + chunk; });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error('Server stopped: ' + report.serverLog);
    try { const response = await fetch(origin + '/api/status'); if (response.ok) { equal('Server captured the exact fixture', (await response.json()).fingerprint, status.fingerprint); ready = true; break; } } catch (error) { if (error.code === 'ERR_ASSERTION') throw error; }
    await pause(100);
  }
  check('Server started', ready);
  browser = await chromium.launch({headless: true, ...(process.env.WEDL_TEST_CHROMIUM ? {executablePath: process.env.WEDL_TEST_CHROMIUM} : {}), args: ['--no-sandbox']});
  const context = await browser.newContext({viewport: {width: 1280, height: 900}});
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('request', request => { const url = new URL(request.url()); if (url.pathname.startsWith('/api/')) report.api.push({method: request.method(), path: url.pathname}); });
  page.on('response', response => { const url = new URL(response.url()); if (url.pathname.startsWith('/api/') && !response.ok()) report.httpFailures.push({path: url.pathname, status: response.status()}); });
  page.on('requestfailed', request => report.requestFailures.push(request.failure()?.errorText));
  await page.goto(origin + '/legacy-author');
  await page.waitForFunction(fingerprint => document.querySelector('#writing-fingerprint').value === fingerprint, status.fingerprint);
  await page.locator('#writing-tab').click();
  for (const [id, value] of [['writing-scene', ids.scene], ['writing-pov', ids.actor], ['writing-timeline', at.timeline], ['writing-tick', at.tick], ['writing-order', at.order], ['writing-budget', '100000']]) await page.locator('#' + id).fill(value);
  await page.locator('#writing-lens').selectOption('character');
  equal('Rendered character context exactly matches native output', await readContext(page), expected.value.characterPrompt);
  equal('Browser keeps the exact selected tick', await page.locator('#writing-tick').inputValue(), tick);
  await page.locator('#writing-lens').selectOption('dramatic-irony');
  equal('Rendered dramatic character component stays separate', await readContext(page), expected.value.characterPrompt);
  equal('Rendered author margin exactly matches native output', JSON.parse(await page.locator('#writing-author-margin').textContent()), dramatic.value.authorMargin);
  await page.locator('#writing-lens').selectOption('character');
  await readContext(page);
  for (const [name, width] of [['desktop', 1280], ['mobile', 390]]) {
    await page.setViewportSize({width, height: 900});
    const file = path.join(runDirectory, name + '-writing-present.png'); await page.screenshot({path: file, fullPage: true}); report.screenshots.push(file);
    const layout = await page.evaluate(() => ({viewport: window.innerWidth, document: document.documentElement.scrollWidth,
      overflowing: [...document.querySelectorAll('body *')].filter(element => element.getBoundingClientRect().right > window.innerWidth).map(element => ({tag: element.tagName, id: element.id, right: element.getBoundingClientRect().right}))}));
    (report.layouts ||= {})[name] = layout;
    check(name + ' has no document overflow', layout.document <= layout.viewport);
  }
  await page.setViewportSize({width: 1280, height: 900});
  equal('Reads preserve all initial source bytes and nanosecond mtimes', snapshot(), before);

  const old = fs.readFileSync(hidden, 'utf8'); const changed = old.replace('condition: 777', 'condition: 778');
  assert.notEqual(changed, old); fs.writeFileSync(hidden, changed);
  report.fixtureEdits.push({path: path.relative(world, hidden), reason: 'Explicit hidden condition change for stale pin and noninterference checks'});
  const afterEdit = snapshot(); report.sourceAfterFixtureEdit = afterEdit;
  const nextStatus = cli(['status']); check('Fixture edit changes the native source fingerprint', nextStatus.fingerprint !== status.fingerprint);
  const next = nativeContext(nextStatus.fingerprint, 'character');
  equal('Hidden object edit cannot affect the complete character component', next.value.characterPrompt, expected.value.characterPrompt);
  equal('Ordinary character context remains identical after hidden edit', cli(['character', 'context', ids.actor, '--timeline', at.timeline, '--tick', at.tick, '--order', at.order, '--max-characters', '100000']), ordinaryBefore);
  await page.locator('#writing-context').click();
  await page.waitForFunction(() => document.querySelector('#writing-status').classList.contains('error'));
  check('Stale browser frame is refused without repinning', await page.locator('#writing-fingerprint').inputValue() === status.fingerprint);
  await page.locator('#writing-capture').click();
  await page.waitForFunction(fingerprint => document.querySelector('#writing-fingerprint').value === fingerprint, nextStatus.fingerprint);
  equal('Explicitly recaptured browser frame keeps the same admitted facts', await readContext(page), next.value.characterPrompt);
  equal('Reads preserve the explicitly changed source image', snapshot(), afterEdit);
  equal('No browser JavaScript errors', report.pageErrors, []);
  equal('Only deliberate stale frame refusal fails at HTTP level', report.httpFailures, [{path: '/api/writing/context', status: 404}]);
  equal('No failed requests', report.requestFailures, []);
  check('All browser API requests are reads', report.api.every(item => item.method === 'GET'));
  equal('Binary unchanged during browser verification', hash(binary), report.binarySha256);
  report.passed = true;
}

const deadline = setTimeout(() => { report.failure = 'Browser regression exceeded 180 seconds'; fs.writeFileSync(path.join(runDirectory, 'report.json'), JSON.stringify(report, null, 2) + '\n'); if (server) server.kill('SIGTERM'); process.exit(1); }, 180000);
main().catch(error => { report.failure = error.stack || String(error); process.exitCode = 1; }).finally(async () => {
  clearTimeout(deadline);
  if (browser) await browser.close();
  if (server) server.kill('SIGTERM');
  fs.writeFileSync(path.join(runDirectory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({passed: report.passed || false, checks: report.checks.length, apiReads: report.api.length, cliReads: report.cliReads.length, sourceFiles: report.sourceBefore?.length, runDirectory, ...(report.failure ? {failure: report.failure} : {})}, null, 2));
});
