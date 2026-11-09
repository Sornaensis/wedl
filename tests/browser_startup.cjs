'use strict';

// Run with Node and Playwright installed. Optional paths:
// WEDL_TEST_BINARY, WEDL_TEST_CHROMIUM, WEDL_TEST_PLAYWRIGHT,
// WEDL_TEST_ARTIFACT_DIR. No package or source files are changed by this test.
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
const runDirectory = fs.mkdtempSync(path.join(artifactDirectory, 'wedl-browser-startup-'));
const world = path.join(runDirectory, 'world');
const hash = filename => crypto.createHash('sha256').update(fs.readFileSync(filename)).digest('hex');
const report = {runDirectory, binary, binarySha256: hash(binary), checks: [], api: [], pageErrors: [], httpFailures: [], downloads: []};
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const equal = (name, actual, expected) => { assert.deepEqual(actual, expected, name); report.checks.push(name); };
let server, browser;

function cli(arguments_) {
  const result = spawnSync(binary, ['--compact', '--repo', world, ...arguments_], {encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, timeout: 20000});
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

function setup() {
  cli(['init', world, '--title', 'Startup selection regression']);
  const filename = path.join(world, 'story', 'world.md');
  const id = fs.readFileSync(filename, 'utf8').match(/^id: (world_[A-Z0-9]+)$/m)[1];
  fs.writeFileSync(filename, [
    '---', 'schema: wedl/v1', 'kind: world', 'id: ' + id,
    'title: Startup selection regression', 'capabilities: []', 'default_timeline: main',
    'timelines: [{id: main, label: Main}, {id: other, label: Other}]',
    'current_time: {timeline: other, tick: 9007199254740993, order: 7}',
    'state_keys: {}', 'relationship_metrics: {}', '---', '',
  ].join('\n'));
}

function snapshot() {
  const files = [];
  function walk(directory) {
    for (const entry of fs.readdirSync(directory, {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))) {
      const filename = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(filename);
      else if (entry.isFile()) {
        const stat = fs.statSync(filename, {bigint: true});
        files.push({path: path.relative(world, filename), bytes: stat.size.toString(), mtimeNs: stat.mtimeNs.toString(), sha256: hash(filename)});
      }
    }
  }
  walk(world);
  return files;
}

async function unusedPort() {
  const listener = net.createServer();
  await new Promise((resolve, reject) => { listener.once('error', reject); listener.listen(0, '127.0.0.1', resolve); });
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  return port;
}

async function waitFor(promise, label) {
  let timer;
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(label + ' timed out')), 15000); })]).finally(() => clearTimeout(timer));
}

async function pageFor(context) {
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('request', request => { const url = new URL(request.url()); if (url.pathname.startsWith('/api/')) report.api.push({method: request.method(), path: url.pathname}); });
  page.on('response', response => { const url = new URL(response.url()); if (url.pathname.startsWith('/api/') && !response.ok()) report.httpFailures.push({path: url.pathname, status: response.status()}); });
  return page;
}

async function holdStatus(page) {
  let start, release;
  const started = new Promise(resolve => { start = resolve; });
  const gate = new Promise(resolve => { release = resolve; });
  const handler = async route => {
    const response = await route.fetch(), body = await response.body();
    start(); await gate; await route.fulfill({response, body});
  };
  await page.route('**/api/status', handler);
  return {
    started: () => waitFor(started, 'Held initial status'),
    release: async () => {
      release();
      await page.waitForFunction(() => document.getElementById('snapshot').textContent.startsWith('Snapshot '));
      await page.waitForLoadState('networkidle');
      await page.unroute('**/api/status', handler);
    },
  };
}

const groups = [
  {prefix: 'story', tab: 'navigation', timeline: 'main'},
  {prefix: 'chronology', tab: 'chronology', operation: ['chronology-operation', 'from-story'], timeline: 'other'},
  {prefix: 'spatial-overlay', tab: 'spatial', timeline: 'main'},
  {prefix: 'spatial-history', tab: 'spatial', dated: 'spatial-history-dated', timeline: 'other'},
  {prefix: 'geometry', tab: 'spatial', dated: 'geometry-dated', timeline: 'main'},
  {prefix: 'lore', tab: 'lore', dated: 'lore-dated', timeline: 'main'},
  {prefix: 'narrative', tab: 'narrative', operation: ['narrative-operation', 'descriptions-list'], dated: 'narrative-dated', timeline: 'other'},
  {prefix: 'writing', tab: 'writing', timeline: 'main'},
  {prefix: 'current-time', tab: 'writing', timeline: 'main'},
  {prefix: 'scene-control', tab: 'writing', timeline: 'main'},
  {prefix: 'conversation-control', tab: 'writing', timeline: 'main'},
  {prefix: 'revision', tab: 'revisions', timeline: 'main'},
  {prefix: 'retcon', tab: 'retcons', timeline: 'main'},
  {prefix: 'consequence', tab: 'consequence', timeline: 'main'},
  {prefix: 'reader-preview', tab: 'manuscripts', timeline: 'main'},
];
const prefixes = groups.map(group => group.prefix);
const capture = (page, selected = prefixes) => page.evaluate(selected => Object.fromEntries(selected.map(prefix => [prefix, Object.fromEntries(['timeline', 'tick', 'order'].map(key => [key, document.getElementById(prefix + '-' + key).value]))])), selected);

async function reveal(page, selector) {
  await page.locator(selector).evaluate(node => { for (let parent = node.parentElement; parent; parent = parent.parentElement) if (parent.tagName === 'DETAILS') parent.open = true; });
}

async function main() {
  setup();
  const status = cli(['status']);
  report.sourceBefore = snapshot();
  const origin = 'http://127.0.0.1:' + await unusedPort();
  server = spawn(binary, ['--repo', world, 'serve', '--host', '127.0.0.1', '--port', new URL(origin).port], {stdio: ['ignore', 'pipe', 'pipe']});
  let log = '';
  for (const stream of [server.stdout, server.stderr]) stream.on('data', chunk => { log += chunk; report.serverLog = log; });
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error('Server stopped: ' + log);
    try {
      const response = await fetch(origin + '/api/status');
      if (response.ok) { equal('Server uses the exact fixture source', (await response.json()).fingerprint, status.fingerprint); ready = true; break; }
    } catch (error) { if (error.code === 'ERR_ASSERTION') throw error; }
    await pause(100);
  }
  assert.ok(ready, 'Server did not become ready');
  browser = await chromium.launch({headless: true, ...(process.env.WEDL_TEST_CHROMIUM ? {executablePath: process.env.WEDL_TEST_CHROMIUM} : {}), args: process.platform === 'linux' ? ['--no-sandbox'] : []});
  const context = await browser.newContext({viewport: {width: 1280, height: 900}, acceptDownloads: true});

  const page = await pageFor(context), held = await holdStatus(page);
  await page.goto(origin, {waitUntil: 'domcontentloaded'}); await held.started();
  equal('Source refresh is disabled before initial status arrives', await page.locator('#source-refresh').isDisabled(), true);
  report.chosen = {};
  for (const group of groups) {
    await page.locator('#' + group.tab + '-tab').click();
    if (group.operation) await page.locator('#' + group.operation[0]).selectOption(group.operation[1]);
    if (group.dated) { await reveal(page, '#' + group.dated); await page.locator('#' + group.dated).check(); }
    const at = {timeline: group.timeline, tick: '-9007199254740993', order: '-7'};
    for (const [key, value] of Object.entries(at)) { await reveal(page, '#' + group.prefix + '-' + key); await page.locator('#' + group.prefix + '-' + key).fill(value); }
    report.chosen[group.prefix] = at;
  }
  await page.locator('#retcons-tab').click();
  const queueRevision = 'a'.repeat(40), retconText = 'Unsent retcon Plan 🌲';
  await page.locator('#retcon-queue-revision').fill(queueRevision);
  await page.locator('#retcon-plan').fill(retconText);
  await page.locator('#consequence-tab').click();
  const editor = 'Unsent exact Plan input 🌲';
  await page.locator('#grouped-consequence-plan').fill(editor);
  report.beforeStatus = await capture(page);
  report.queueRevisionBefore = await page.locator('#retcon-queue-revision').inputValue();
  await held.release();
  report.afterStatus = await capture(page);
  report.queueRevisionAfter = await page.locator('#retcon-queue-revision').inputValue();
  equal('Startup retains unsent consequence Plan text', await page.locator('#grouped-consequence-plan').inputValue(), editor);
  equal('Startup retains unsent retcon Plan text', await page.locator('#retcon-plan').inputValue(), retconText);
  report.mismatches = prefixes.filter(prefix => JSON.stringify(report.chosen[prefix]) !== JSON.stringify(report.afterStatus[prefix]));
  if (report.queueRevisionAfter !== queueRevision) report.mismatches.push('retcon-queue-revision');

  const untouched = await pageFor(context), untouchedHeld = await holdStatus(untouched);
  await untouched.goto(origin, {waitUntil: 'domcontentloaded'}); await untouchedHeld.started();
  report.untouchedBefore = await capture(untouched);
  await untouchedHeld.release(); report.untouchedAfter = await capture(untouched);
  const native = status.result.currentTime, expected = structuredClone(report.untouchedBefore);
  for (const prefix of ['chronology', 'spatial-overlay', 'spatial-history', 'geometry', 'lore', 'narrative']) expected[prefix].timeline = status.result.defaultTimeline;
  for (const prefix of ['spatial-overlay', 'spatial-history', 'geometry', 'lore']) { expected[prefix].tick = native.tick; expected[prefix].timeline = native.timeline; }
  for (const prefix of ['story', 'writing', 'current-time', 'scene-control', 'conversation-control', 'revision', 'retcon', 'consequence', 'reader-preview']) expected[prefix] = {...native};
  equal('Untouched controls receive their native startup defaults', report.untouchedAfter, expected);
  equal('Untouched queue source retains its working-file default', await untouched.locator('#retcon-queue-revision').inputValue(), '');

  // A successful request download uses the horizon without editing its fields.
  const used = await pageFor(context), usedHeld = await holdStatus(used);
  await used.goto(origin, {waitUntil: 'domcontentloaded'}); await usedHeld.started();
  await used.locator('#consequence-tab').click();
  const plan = cli(['authoring', 'scaffold', '--family', 'changeset', '--expected-fingerprint', status.fingerprint]).result.plan;
  await used.locator('#grouped-consequence-file').setInputFiles({name: 'plan.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(plan))});
  await used.waitForFunction(() => document.getElementById('grouped-consequence-status').textContent.startsWith('Explicit plan imported'));
  report.usedWithoutTimeEditBefore = (await capture(used, ['consequence'])).consequence;
  const downloading = used.waitForEvent('download'); await used.locator('#grouped-consequence-download').click();
  const download = await downloading, filename = path.join(runDirectory, 'used-consequence-request.json');
  await download.saveAs(filename); report.downloads.push(filename);
  equal('Downloaded request retains the exact used initial horizon', JSON.parse(fs.readFileSync(filename, 'utf8')).at, report.usedWithoutTimeEditBefore);
  await usedHeld.release();
  report.usedWithoutTimeEditAfter = (await capture(used, ['consequence'])).consequence;
  if (JSON.stringify(report.usedWithoutTimeEditAfter) !== JSON.stringify(report.usedWithoutTimeEditBefore)) report.mismatches.push('consequence-used-without-time-edit');
  equal('Downloaded Plan and its original pin remain in the editor', JSON.parse(await used.locator('#grouped-consequence-plan').inputValue()), plan);
  equal('Using one request does not suppress another untouched horizon', (await capture(used, ['revision'])).revision, native);

  // Editing one coordinate preserves the whole point, including its timeline.
  const partial = await pageFor(context), partialHeld = await holdStatus(partial);
  await partial.goto(origin, {waitUntil: 'domcontentloaded'}); await partialHeld.started();
  await partial.locator('#consequence-tab').click(); await partial.locator('#consequence-tick').fill('-9007199254740993');
  report.partialBefore = (await capture(partial, ['consequence'])).consequence;
  await partialHeld.release(); report.partialAfter = (await capture(partial, ['consequence'])).consequence;
  if (JSON.stringify(report.partialAfter) !== JSON.stringify(report.partialBefore)) report.mismatches.push('partially-edited-consequence-point');
  equal('No browser script errors', report.pageErrors, []);
  equal('All browser API traffic is read-only', report.api.every(request => request.method === 'GET'), true);
  equal('Every explicit startup selection survives delayed status', report.mismatches, []);
}

main().catch(error => { report.failure = error.stack || String(error); process.exitCode = 1; }).finally(async () => {
  if (browser) await browser.close();
  if (server) server.kill('SIGTERM');
  report.sourceAfter = fs.existsSync(world) ? snapshot() : [];
  report.sourcePreserved = JSON.stringify(report.sourceAfter) === JSON.stringify(report.sourceBefore);
  report.binarySha256After = hash(binary);
  if (!report.sourcePreserved || report.binarySha256After !== report.binarySha256) { report.failure ||= 'Fixture source or binary changed during browser reads'; process.exitCode = 1; }
  report.passed = !report.failure;
  fs.writeFileSync(path.join(runDirectory, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({runDirectory, passed: report.passed, checks: report.checks.length, mismatches: report.mismatches, sourcePreserved: report.sourcePreserved, pageErrors: report.pageErrors, httpFailures: report.httpFailures, binarySha256: report.binarySha256, failure: report.failure}, null, 2));
});
