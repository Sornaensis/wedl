'use strict';

// Run with Node and Playwright installed. Optional paths:
// WEDL_TEST_BINARY, WEDL_TEST_CHROMIUM, WEDL_TEST_PLAYWRIGHT,
// WEDL_TEST_ARTIFACT_DIR. All fixture source belongs to this test's scratch folder.
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
const runDirectory = fs.mkdtempSync(path.join(artifactDirectory, 'wedl-browser-writing-history-'));
const world = path.join(runDirectory, 'world');
const hash = filename => crypto.createHash('sha256').update(fs.readFileSync(filename)).digest('hex');
const report = {runDirectory, binary, binarySha256: hash(binary), checks: [], api: [], pageErrors: [], httpFailures: [], sourcePhases: []};
const equal = (name, actual, expected) => { assert.deepEqual(actual, expected, name); report.checks.push(name); };
const check = (name, value) => { assert.ok(value, name); report.checks.push(name); };
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const key = 'wedlWritingFrame';
const ids = {world: 'world_00000000000000000000000000', character: 'char_00000000000000000000000001', location: 'loc_00000000000000000000000001', scene: 'scene_00000000000000000000000001', work: 'work_00000000000000000000000001'};
const unusualTimeline = 'line\nwith CR\r\nand 🌲';
let server, browser, expectedSource;

function cli(arguments_) {
  const result = spawnSync(binary, ['--compact', '--repo', world, ...arguments_], {encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, timeout: 20000});
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

function setup() {
  fs.mkdirSync(path.join(world, 'story'), {recursive: true});
  const put = (kind, id, fields) => fs.writeFileSync(path.join(world, 'story', id + '.md'), '---\nschema: wedl/v1\nkind: ' + kind + '\nid: ' + id + '\ntitle: History ' + kind + '\n' + fields + '---\nPrivate source body 🌲\r\n');
  put('world', ids.world, 'capabilities: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: ' + JSON.stringify(unusualTimeline) + ', label: Exact}]\ncurrent_time: {timeline: main, tick: 9007199254740993, order: 1}\nstate_keys: {}\nrelationship_metrics: {}\n');
  put('character', ids.character, 'initial_state: {}\n');
  put('location', ids.location, '');
  put('scene', ids.scene, 'status: closed\nlocation: ' + ids.location + '\ntime: {start: {timeline: main, tick: 9007199254740992, order: 0}, current: {timeline: main, tick: 9007199254740993, order: 1}, end: {timeline: main, tick: 9007199254740994, order: 0}}\nparticipants: [{character: ' + ids.character + ', point_of_view: true}]\n');
  fs.mkdirSync(path.join(world, 'author', 'works'), {recursive: true});
  fs.writeFileSync(path.join(world, 'author', 'works', ids.work + '.md'), '---\nschema: wedl-author/v1\nkind: work\nid: ' + ids.work + '\ntitle: Private work\nduplicate_policy: reject\nchapters: []\n---\nPrivate work notes 🌲\r\n');
}

function snapshot() {
  const files = [];
  function walk(directory) {
    for (const entry of fs.readdirSync(directory, {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))) {
      const filename = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(filename);
      else if (entry.isFile()) { const stat = fs.statSync(filename, {bigint: true}); files.push({path: path.relative(world, filename), bytes: stat.size.toString(), mtimeNs: stat.mtimeNs.toString(), sha256: hash(filename)}); }
    }
  }
  walk(world); return files;
}

function frameFile(name, frame) {
  const file = path.join(runDirectory, name + '.json'); fs.writeFileSync(file, JSON.stringify(frame)); return file;
}

async function unusedPort() {
  const listener = net.createServer();
  await new Promise((resolve, reject) => { listener.once('error', reject); listener.listen(0, '127.0.0.1', resolve); });
  const port = listener.address().port; await new Promise(resolve => listener.close(resolve)); return port;
}

async function pageFor(context) {
  const page = await context.newPage(); page.setDefaultTimeout(15000);
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('request', request => { const url = new URL(request.url()); if (url.pathname.startsWith('/api/')) report.api.push({method: request.method(), path: url.pathname, ...(url.pathname.startsWith('/api/writing/') ? {frame: JSON.parse(url.searchParams.get('frame'))} : {})}); });
  page.on('response', response => { const url = new URL(response.url()); if (url.pathname.startsWith('/api/') && !response.ok()) report.httpFailures.push({path: url.pathname, status: response.status()}); });
  return page;
}

const history = page => page.evaluate(() => ({length: window.history.length, state: window.history.state}));
const resolved = page => page.evaluate(() => { const text = document.getElementById('writing-frame-json').textContent; return text ? JSON.parse(text) : null; });
const inputs = page => page.evaluate(() => Object.fromEntries(['fingerprint', 'work-id', 'work-author-fingerprint', 'scene', 'pov', 'timeline', 'tick', 'order', 'lens'].map(name => [name, document.getElementById('writing-' + name).value])));

async function waitValidated(page, frame) {
  await page.waitForFunction(expected => {
    const button = document.getElementById('writing-download'), text = document.getElementById('writing-frame-json').textContent;
    const ordered = value => value && typeof value === 'object' && !Array.isArray(value) ? Object.fromEntries(Object.keys(value).sort().map(key => [key, ordered(value[key])])) : value;
    return !button.disabled && text && JSON.stringify(ordered(JSON.parse(text))) === JSON.stringify(ordered(expected));
  }, frame);
}

async function importFrame(page, frame) {
  await page.locator('#writing-file').focus();
  await page.locator('#writing-file').setInputFiles({name: 'captured-frame.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(frame))});
  await waitValidated(page, frame);
}

async function hold(page, endpoint) {
  let release, start;
  const started = new Promise(resolve => { start = resolve; }), gate = new Promise(resolve => { release = resolve; });
  let count = 0;
  const handler = async route => { const response = await route.fetch(), body = await response.body(); count++; start(); await gate; await route.fulfill({response, body}); };
  await page.route('**/api/' + endpoint + '*', handler);
  return {
    started: async () => { let timer; try { await Promise.race([started, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Held ' + endpoint + ' timed out')), 15000); })]); } finally { clearTimeout(timer); } },
    release: async () => { release(); await page.waitForLoadState('networkidle'); await page.unroute('**/api/' + endpoint + '*', handler); },
    count: () => count,
  };
}

async function main() {
  setup(); expectedSource = snapshot(); report.sourceBefore = expectedSource;
  const status = cli(['status']);
  const frameA = {fingerprint: status.fingerprint, work: null, sceneId: null, povId: null, at: {timeline: 'main', tick: '-9007199254740993', order: '-7'}, lens: 'author'};
  const choices = cli(['writing', 'choices', '--file', frameFile('frame-a', frameA)]).result;
  const frameB = {fingerprint: status.fingerprint, work: {workId: ids.work, authorFingerprint: choices.authorFingerprint}, sceneId: ids.scene, povId: ids.character, at: {timeline: 'main', tick: '9007199254740993', order: '1'}, lens: 'dramatic-irony'};
  const frameC = {...frameA, at: {timeline: unusualTimeline, tick: '-9223372036854775808', order: '-2147483648'}};
  equal('Native private work frame is valid', cli(['writing', 'resolve', '--file', frameFile('frame-b', frameB)]).result, frameB);
  equal('Native exact multiline timeline and signed limits are valid', cli(['writing', 'resolve', '--file', frameFile('frame-c', frameC)]).result, frameC);
  const origin = 'http://127.0.0.1:' + await unusedPort();
  server = spawn(binary, ['--repo', world, 'serve', '--host', '127.0.0.1', '--port', new URL(origin).port], {stdio: ['ignore', 'pipe', 'pipe']});
  let log = '';
  for (const stream of [server.stdout, server.stderr]) stream.on('data', chunk => { log += chunk; report.serverLog = log; });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error('Server stopped: ' + log);
    try { const response = await fetch(origin + '/api/status'); if (response.ok) { ready = true; break; } } catch {}
    await pause(100);
  }
  check('Server started', ready);
  browser = await chromium.launch({headless: true, ...(process.env.WEDL_TEST_CHROMIUM ? {executablePath: process.env.WEDL_TEST_CHROMIUM} : {}), args: process.platform === 'linux' ? ['--no-sandbox'] : []});
  const context = await browser.newContext({viewport: {width: 1280, height: 900}, acceptDownloads: true});
  const page = await pageFor(context);
  const legacy = origin + '/legacy-author';
  await page.goto(legacy, {waitUntil: 'networkidle'}); await page.locator('#writing-tab').click();
  await page.evaluate(() => window.history.replaceState({unrelated: {note: 'Preserve this tab state'}}, ''));
  const initialHistory = await history(page);
  await page.locator('#writing-tick').fill(frameA.at.tick); await page.locator('#writing-order').fill(frameA.at.order); await page.locator('#writing-resolve').click(); await waitValidated(page, frameA);
  const firstHistory = await history(page);
  equal('First accepted frame adds one navigation entry', firstHistory.length, initialHistory.length + 1);
  equal('History stores the exact six-field frame', firstHistory.state[key], frameA);
  equal('Unrelated history state survives', firstHistory.state.unrelated, initialHistory.state.unrelated);
  await page.locator('#writing-resolve').click(); await waitValidated(page, frameA);
  equal('Repeated validation creates no duplicate entry', (await history(page)).length, firstHistory.length);

  await page.locator('#writing-tick').fill('9007199254740993');
  const draft = await history(page);
  equal('First selection edit creates one unselected draft entry', [draft.length, draft.state[key]], [firstHistory.length + 1, null]);
  equal('Editing immediately clears download approval', await page.locator('#writing-download').isDisabled(), true);
  await page.locator('#writing-order').fill('1');
  equal('Further edits reuse that draft entry', (await history(page)).length, draft.length);
  await importFrame(page, frameB);
  equal('Successful import replaces its draft entry', (await history(page)).length, draft.length);
  equal('Private work retains both original source pins', (await history(page)).state[key], frameB);
  const unsent = 'Unsent private draft 🌲\nTrailing spaces  ';
  await page.locator('#consequence-tab').click(); await page.locator('#grouped-consequence-plan').fill(unsent);
  await page.locator('#writing-tab').click(); await page.locator('#writing-briefs').fill('Unsent brief selection');
  await page.goBack(); await waitValidated(page, frameA);
  equal('Back restores and revalidates the previous exact frame', await resolved(page), frameA);
  equal('Back leaves unsent Plan text intact', await page.locator('#grouped-consequence-plan').inputValue(), unsent);
  equal('Back leaves unrelated local context selection intact', await page.locator('#writing-briefs').inputValue(), 'Unsent brief selection');
  await page.goForward(); await waitValidated(page, frameB);
  equal('Forward restores the original private work and lens', await resolved(page), frameB);
  equal('Navigation does not append history', (await history(page)).length, draft.length);
  equal('History carries no unsent Plan or brief contents', JSON.stringify((await history(page)).state).includes(unsent) || JSON.stringify((await history(page)).state).includes('Unsent brief selection'), false);

  const beforeReloadRequests = report.api.filter(item => item.path === '/api/writing/resolve').length;
  const heldStatus = await hold(page, 'status'); await page.reload({waitUntil: 'domcontentloaded'}); await heldStatus.started();
  equal('Reload restores original inputs before delayed startup status', await inputs(page), {fingerprint: frameB.fingerprint, 'work-id': ids.work, 'work-author-fingerprint': frameB.work.authorFingerprint, scene: ids.scene, pov: ids.character, timeline: 'main', tick: frameB.at.tick, order: '1', lens: 'dramatic-irony'});
  equal('Reload awaits source readiness before revalidation', report.api.filter(item => item.path === '/api/writing/resolve').length, beforeReloadRequests);
  await heldStatus.release(); await waitValidated(page, frameB);
  equal('Reload revalidates without an extra history entry', (await history(page)).length, draft.length);
  equal('Reload does not restore draft Plan prose', await page.locator('#grouped-consequence-plan').inputValue(), '');
  equal('Reload does not restore context brief input', await page.locator('#writing-briefs').inputValue(), '');

  const independent = await pageFor(context); await independent.goto(legacy, {waitUntil: 'networkidle'}); await independent.locator('#writing-tab').click();
  equal('Another browser tab has no retained frame', (await history(independent)).state, null);
  equal('Another browser tab cannot download this tab’s frame', await independent.locator('#writing-download').isDisabled(), true);

  // Explicit source refresh revalidates only a still-accepted active selection.
  await importFrame(independent, frameA);
  const beforeRefreshHistory = await history(independent);
  const refreshed = independent.waitForResponse(response => new URL(response.url()).pathname === '/api/writing/resolve');
  await independent.locator('#source-refresh').click(); await refreshed; await waitValidated(independent, frameA);
  equal('Unchanged source refresh revalidates the exact accepted frame', await resolved(independent), frameA);
  equal('Unchanged source refresh leaves browser history unchanged', await history(independent), beforeRefreshHistory);
  const heldRefresh = await hold(independent, 'status');
  await independent.locator('#source-refresh').click(); await heldRefresh.started();
  const refreshReadCount = report.api.filter(item => item.path === '/api/writing/resolve').length;
  await independent.locator('#writing-tick').fill('-99');
  await heldRefresh.release();
  equal('Editing during source refresh prevents automatic revalidation', report.api.filter(item => item.path === '/api/writing/resolve').length, refreshReadCount);
  equal('Late source refresh retains the edited unapproved selection', [await independent.locator('#writing-tick').inputValue(), await independent.locator('#writing-download').isDisabled()], ['-99', true]);
  await importFrame(independent, frameA);
  const heldTabRefresh = await hold(independent, 'status');
  await independent.locator('#source-refresh').click(); await heldTabRefresh.started();
  const tabRefreshReadCount = report.api.filter(item => item.path === '/api/writing/resolve').length;
  await independent.locator('#browse-tab').click(); await heldTabRefresh.release();
  equal('Leaving Writing during refresh prevents automatic revalidation', report.api.filter(item => item.path === '/api/writing/resolve').length, tabRefreshReadCount);
  equal('Source refresh cannot switch views back to Writing', await independent.locator('#browse-tab').getAttribute('aria-pressed'), 'true');

  // An old navigation response cannot reclaim fields or history after a new edit.
  const heldResolve = await hold(page, 'writing/resolve'); await page.goBack(); await heldResolve.started();
  await page.locator('#writing-tick').fill('-9');
  const editedHistory = await history(page);
  await heldResolve.release();
  equal('Late Back response cannot restore prior inputs', await page.locator('#writing-tick').inputValue(), '-9');
  equal('Late Back response cannot restore download approval', await page.locator('#writing-download').isDisabled(), true);
  equal('Late Back response cannot replace the new draft history', await history(page), editedHistory);
  await page.reload({waitUntil: 'networkidle'});
  equal('An unvalidated history draft remains unselected after reload', [(await history(page)).state[key], await page.locator('#writing-download').isDisabled()], [null, true]);
  equal('Reload of a draft never resuscitates the old private work', await page.locator('#writing-work-id').inputValue(), '');

  await page.locator('#writing-tab').click(); await importFrame(page, frameC);
  equal('Imported multiline timeline survives the visible text input normalization', (await history(page)).state[key], frameC);
  await page.reload({waitUntil: 'networkidle'}); await waitValidated(page, frameC);
  equal('Reload preserves exact UTF-8 timeline and signed coordinate limits', await resolved(page), frameC);
  const downloading = page.waitForEvent('download'); await page.locator('#writing-download').click();
  const download = await downloading, filename = path.join(runDirectory, 'downloaded-frame.json'); await download.saveAs(filename);
  equal('Download preserves the restored exact six-field frame', JSON.parse(fs.readFileSync(filename, 'utf8')), frameC);
  await page.locator('#writing-timeline').fill('main'); await page.locator('#writing-resolve').click();
  const editedTimeline = {...frameC, at: {...frameC.at, timeline: 'main'}}; await waitValidated(page, editedTimeline);
  equal('Explicit timeline edits supersede exact imported text', (await history(page)).state[key], editedTimeline);

  // Malformed state is rejected before any field can inherit its values.
  const malformed = [{...frameB, extra: 'not a frame field'}, {...frameB, at: {...frameB.at, tick: 9007199254740992}}, {...frameB, work: {...frameB.work, title: 'private prose'}}, {...frameB, at: {...frameB.at, order: '-0'}}];
  for (let index = 0; index < malformed.length; index++) {
    const invalid = await pageFor(context); await invalid.goto(legacy, {waitUntil: 'networkidle'});
    await invalid.evaluate(({key, frame}) => window.history.replaceState({[key]: frame, unrelated: 17}, ''), {key, frame: malformed[index]});
    const readCount = report.api.filter(item => item.path === '/api/writing/resolve').length;
    await invalid.reload({waitUntil: 'networkidle'});
    check('Malformed history ' + index + ' reports a closed validation error', (await invalid.locator('#writing-status').textContent()).startsWith('Saved Writing frame is invalid:'));
    equal('Malformed history ' + index + ' never assigns its private selectors', [(await inputs(invalid)).fingerprint, (await inputs(invalid))['work-id'], (await inputs(invalid)).scene], ['', '', '']);
    equal('Malformed history ' + index + ' is not sent to native resolve', report.api.filter(item => item.path === '/api/writing/resolve').length, readCount);
    equal('Malformed history ' + index + ' cannot enable download', await invalid.locator('#writing-download').isDisabled(), true);
    await invalid.close();
  }

  equal('All browser reads preserve initial fixture bytes and nanosecond mtimes', snapshot(), expectedSource);
  report.sourcePhases.push({name: 'initial', preserved: true});
  await importFrame(page, frameB);
  const filenameWorld = path.join(world, 'story', ids.world + '.md'); fs.appendFileSync(filenameWorld, '\nExplicit test source change.\n');
  report.fixtureEdit = {path: path.relative(world, filenameWorld), reason: 'Make the exact retained source pin stale'};
  expectedSource = snapshot(); const newer = cli(['status']); check('Explicit fixture edit changes source fingerprint', newer.fingerprint !== frameB.fingerprint);
  const beforeStaleRefresh = await history(page);
  await page.locator('#source-refresh').click(); await page.waitForFunction(() => document.getElementById('writing-status').classList.contains('error'));
  equal('Changed source refresh retains the original frame pins and history', await history(page), beforeStaleRefresh);
  equal('Changed source refresh refuses the stale frame without repinning', [await page.locator('#writing-fingerprint').inputValue(), await page.locator('#writing-download').isDisabled()], [frameB.fingerprint, true]);
  const staleStatus = await hold(page, 'status'); await page.reload({waitUntil: 'domcontentloaded'}); await staleStatus.started();
  equal('Stale reload retains original pin before startup completes', await page.locator('#writing-fingerprint').inputValue(), frameB.fingerprint);
  await staleStatus.release(); await page.waitForFunction(() => document.getElementById('writing-status').classList.contains('error'));
  equal('Stale reload does not capture the newer source pin', await page.locator('#writing-fingerprint').inputValue(), frameB.fingerprint);
  equal('Stale reload preserves exact historical frame data', (await history(page)).state[key], frameB);
  equal('Stale reload refuses download approval', await page.locator('#writing-download').isDisabled(), true);
  check('Stale reload reports native unavailable result', (await page.locator('#writing-status').textContent()).includes('unavailable'));
  equal('Stale revalidation preserves source bytes and mtimes', snapshot(), expectedSource);
  report.sourcePhases.push({name: 'explicitly-changed', preserved: true});
  equal('All browser API traffic is read-only', report.api.every(item => item.method === 'GET'), true);
  equal('No browser script errors', report.pageErrors, []);
}

main().catch(error => { report.failure = error.stack || String(error); process.exitCode = 1; }).finally(async () => {
  if (browser) await browser.close();
  if (server) server.kill('SIGTERM');
  report.sourceAfter = fs.existsSync(world) ? snapshot() : [];
  report.sourcePreserved = JSON.stringify(report.sourceAfter) === JSON.stringify(expectedSource);
  report.binarySha256After = hash(binary);
  if (!report.sourcePreserved || report.binarySha256After !== report.binarySha256) { report.failure ||= 'Fixture source or binary changed during browser reads'; process.exitCode = 1; }
  report.passed = !report.failure;
  fs.writeFileSync(path.join(runDirectory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({runDirectory, passed: report.passed, checks: report.checks.length, sourcePreserved: report.sourcePreserved, pageErrors: report.pageErrors, httpFailures: report.httpFailures, binarySha256: report.binarySha256, failure: report.failure}, null, 2));
});
