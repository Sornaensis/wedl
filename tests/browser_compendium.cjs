'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const {spawn, spawnSync} = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {chromium} = require(process.env.WEDL_TEST_PLAYWRIGHT || 'playwright');
const fixture = require('./fixtures/compendium-disclosure.json');

const root = path.resolve(__dirname, '..');
const binary = path.resolve(process.env.WEDL_TEST_BINARY || path.join(root, 'target', 'debug',
  process.platform === 'win32' ? 'wedl-rs.exe' : 'wedl-rs'));
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const binaryBefore = hash(fs.readFileSync(binary));
const artifacts = path.resolve(process.env.WEDL_TEST_ARTIFACT_DIR || os.tmpdir());
fs.mkdirSync(artifacts, {recursive: true});
const run = fs.mkdtempSync(path.join(artifacts, 'wedl-browser-compendium-'));
const ada = 'char_00000000000000000000000001';
const privateDock = 'MATRIX_AUTHOR_DOCK_PROSE';
const address = /^\/compendium\/v1\/([0-9a-f]{1592})$/;
const mtime = new Date('2000-01-01T00:00:00Z');
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]));
  }
  return value;
}

function writeFixture(variant) {
  const folder = path.join(run, variant.name);
  const world = path.join(folder, 'world');
  const story = path.join(world, 'story');
  fs.mkdirSync(story, {recursive: true});
  const filename = id => id.replace(/[:/]/g, '-') + '.md';
  function write({$body = '', ...fields}) {
    const text = JSON.stringify(canonical({schema: 'wedl/v1', ...fields}))
      .replaceAll('"$tick"', fixture.tick)
      .replaceAll('$withheldEdge', 'matrix-private-edge-' + variant.name);
    const output = path.join(story, filename(fields.id));
    fs.writeFileSync(output, '---\n' + text + '\n---\n' + $body + '\n');
    fs.utimesSync(output, mtime, mtime);
  }
  fixture.records.forEach(write);
  const knowledgeIds = [];
  for (const [index, [title, target, order]] of fixture.knowledge.entries()) {
    const id = 'know_' + String(index + 1).padStart(26, '0');
    knowledgeIds.push(id);
    write({
      kind: 'knowledge', id, title: 'Private note',
      knower: ada, claim: {key: 'known-' + index, statement: 'I know ' + title + '.', truth: 'true', object: {entity: target}},
      transitions: [{id: 'observed', time: {timeline: 'main', tick: '$tick', order},
        state: 'accepted', acquisition: 'observed', confidence: 1}],
    });
  }
  const hidden = {
    map: 'MATRIX_PRIVATE_MAP_' + variant.name,
    alias: 'MATRIX_PRIVATE_ALIAS_' + variant.name,
    prose: 'MATRIX_PRIVATE_PROSE_' + variant.name,
  };
  write({
    kind: 'map', id: 'map:matrix-hidden', title: hidden.map, audience: [],
    crs: 'local-planar:hidden', axis_order: ['east', 'north'], unit: 'pace',
    bounds: {min: [0, 0], max: [variant.bound, variant.bound]},
  });
  write({
    kind: 'location', id: 'location:matrix-hidden', title: 'Dock', audience: [], aliases: [hidden.alias],
    spatial: {map_id: 'map:matrix-hidden', geometry: {kind: 'point', coordinates: variant.point}}, $body: hidden.prose,
  });
  const status = spawnSync(binary, ['--compact', '--repo', world, 'status'], {encoding: 'utf8', timeout: 10000, windowsHide: true});
  assert.equal(status.status, 0, 'fixture source is readable');
  const fingerprint = JSON.parse(status.stdout).fingerprint;
  const forbidden = [
    fixture.tick, privateDock, fingerprint, fingerprint.replace(/^sha256:/, ''), ...Object.values(hidden),
    'matrix-private-edge-' + variant.name, 'map:matrix-hidden', 'location:matrix-hidden',
    'story/location-matrix-hidden.md', 'story/map-matrix-hidden.md', ...variant.point.map(String),
    ...fixture.records.map(({id}) => id).concat(knowledgeIds).flatMap(id => [id, 'story/' + filename(id)]),
  ];
  const keyring = path.join(folder, 'links.json');
  fs.writeFileSync(keyring, JSON.stringify({schema: 'wedl-compendium-keyring/v1',
    installation: crypto.randomBytes(16).toString('hex'), keys: [crypto.randomBytes(32).toString('hex')]}), {mode: 0o600});
  return {world, story, keyring, forbidden, compass: path.join(story, filename('obj_00000000000000000000000001'))};
}

function snapshot(story) {
  return fs.readdirSync(story).sort().map(name => {
    const source = path.join(story, name);
    const stat = fs.statSync(source, {bigint: true});
    return [name, stat.size.toString(), stat.mtimeNs.toString(), hash(fs.readFileSync(source))];
  });
}

async function start(world, command, args = []) {
  const server = spawn(binary, ['--repo', world, command, '--port', '0', ...args],
    {stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true});
  let output = '', failure;
  server.on('error', error => { failure = error.code; });
  server.stderr.on('data', bytes => { output = (output + bytes.toString()).slice(-2048); });
  for (let attempt = 0; attempt < 150; attempt++) {
    if (failure || server.exitCode !== null) break;
    const origin = output.match(/Ready: (http:\/\/127\.0\.0\.1:\d+)/)?.[1];
    if (origin) return {server, origin};
    await pause(50);
  }
  await stop(server);
  throw new Error(command + ' listener did not become ready');
}

async function stop(server) {
  if (!server.pid || server.exitCode !== null || server.signalCode !== null) return;
  const exited = new Promise(resolve => server.once('exit', resolve));
  server.kill();
  const ended = await Promise.race([exited.then(() => true), pause(1500).then(() => false)]);
  assert.ok(ended, 'owned listener exited during teardown');
}

const main = page => page.locator('main');
const heading = (page, name) => main(page).getByRole('heading', {name, exact: true});
const card = (page, name) => main(page).locator('a.record').filter({has: page.getByText(name, {exact: true})});
const ready = page => page.locator('#view:not([aria-busy])').waitFor();

async function pick(page, field, name) {
  const select = page.locator('#' + field);
  const option = select.locator('option').filter({hasText: new RegExp('^' + name + '(?: ·|$)')});
  await option.waitFor({state: 'attached'});
  await select.selectOption({label: await option.textContent()});
}

async function character(page, moment) {
  await page.locator('#change').click();
  await pick(page, 'mode', 'Character');
  await pick(page, 'choose-timeline', 'Harbor chronology');
  await pick(page, 'choose-moment', moment);
  await pick(page, 'choose-viewpoint', 'Ada River');
  await page.locator('#apply').click();
  await page.locator('#context-label').getByText('Character · Ada River', {exact: true}).waitFor();
  await page.locator('#context-detail').getByText(moment, {exact: false}).waitFor();
  await ready(page);
}

async function hold(page, endpoint) {
  await page.evaluate(({endpoint, privateDock}) => {
    window.matrixHold = {endpoint, release: null, containedPrivate: false};
    window.matrixOriginalFetch ??= window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await window.matrixOriginalFetch(...args);
      const active = window.matrixHold;
      if (active.release || new URL(args[0], location.href).pathname !== active.endpoint) return response;
      const body = await response.text();
      active.containedPrivate = body.includes(privateDock);
      const saved = new Response(body, {status: response.status, headers: response.headers});
      return new Promise(resolve => { active.release = () => resolve(saved); });
    };
  }, {endpoint: '/api/compendium/' + endpoint, privateDock});
  return () => page.evaluate(() => window.matrixHold.release());
}

async function scan(page, forbidden) {
  const content = await page.evaluate(() => ({
    html: document.documentElement.outerHTML,
    state: history.state,
    address: location.pathname + location.search,
    controls: [...document.querySelectorAll('option,input,textarea')].map(control => control.value),
    links: [...document.querySelectorAll('a[href]')].map(link => link.getAttribute('href')),
  }));
  const serialized = JSON.stringify(content);
  const decoded = content.links.concat(content.address).map(link => {
    const text = decodeURIComponent(link);
    const encrypted = text.match(/^\/compendium\/v1\/([0-9a-f]{1592})$/)?.[1];
    return text + (encrypted ? Buffer.from(encrypted, 'hex').toString('utf8') : '');
  }).join('\n');
  for (const sentinel of forbidden) {
    assert.ok(!serialized.includes(sentinel) && !decoded.includes(sentinel), 'DOM or decoded URL contains a fixture token');
  }
}

function normalize(text) {
  return text.replace(/\/compendium\/v1\/[0-9a-f]{1592}/g, '/compendium/v1/<address>');
}

async function semantics(page) {
  const accessibility = normalize(await main(page).ariaSnapshot());
  const focusable = await page.evaluate(() => [...document.querySelectorAll('main a[href], main button, main input, main select')]
    .filter(element => element.getClientRects().length && !element.disabled)
    .map(element => [element.localName, element.getAttribute('aria-label') || element.textContent.trim(),
      element.hasAttribute('href')]));
  return {accessibility, focusable};
}

function characterTraffic(requests) {
  assert.ok(requests.length && requests.every(([method, route]) =>
    method === 'GET' && route.startsWith('/api/compendium/') && route !== '/api/compendium/map'));
}

async function traverse(context, source, origin) {
  const page = await context.newPage();
  const traffic = new Map([[page, []]]);
  const requests = traffic.get(page), pageErrors = [];
  context.on('request', request => {
    const url = new URL(request.url());
    if (!url.pathname.startsWith('/api/')) return;
    const owner = request.frame().page();
    if (!traffic.has(owner)) traffic.set(owner, []);
    traffic.get(owner).push([request.method(), url.pathname]);
  });
  page.on('pageerror', error => pageErrors.push(error.name));
  await page.goto(origin);
  await heading(page, 'Directory').waitFor();
  await ready(page);
  await page.getByRole('navigation', {name: 'Compendium'}).getByRole('link', {name: 'Map', exact: true}).click();
  await heading(page, 'Harbor grid').waitFor();
  await main(page).locator('svg').waitFor();
  const authorMap = normalize(await main(page).locator('svg').innerHTML());
  await scan(page, source.forbidden);
  await page.getByRole('navigation', {name: 'Compendium'}).getByRole('link', {name: 'Directory', exact: true}).click();
  await heading(page, 'Directory').waitFor();
  await ready(page);

  const releaseAuthor = await hold(page, 'record');
  await card(page, 'Dock').click();
  await page.waitForFunction(() => Boolean(window.matrixHold.release));
  assert.equal(await page.evaluate(() => window.matrixHold.containedPrivate), true);
  await page.evaluate(privateDock => {
    window.matrixDisclosed = false;
    new MutationObserver(records => {
      if (document.body.textContent.includes(privateDock) || records.some(record =>
        [...record.addedNodes].some(node => node.textContent.includes(privateDock)))) window.matrixDisclosed = true;
    }).observe(document.body, {subtree: true, childList: true, characterData: true});
  }, privateDock);
  const characterBoundary = requests.length;
  await character(page, 'First tide');
  await heading(page, 'Dock').waitFor();
  await page.locator('#related-records').getByText('Pier ledger', {exact: true}).waitFor();
  await scan(page, source.forbidden);
  const detailBefore = await semantics(page);
  await releaseAuthor();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.equal(await page.evaluate(() => window.matrixDisclosed), false);
  assert.deepEqual(await semantics(page), detailBefore);
  await scan(page, source.forbidden);

  await page.getByRole('link', {name: 'Browse all eligible records'}).click();
  await heading(page, 'Directory').waitFor();
  await ready(page);
  assert.equal(await card(page, 'Bronze token').count(), 0);
  const first = await semantics(page);
  const compassLink = await card(page, 'Loose compass').getAttribute('href');
  assert.match(compassLink, address);
  await card(page, 'Loose compass').focus();
  await page.keyboard.press('Enter');
  await heading(page, 'Loose compass').waitFor();
  assert.equal(await heading(page, 'Loose compass').evaluate(element => element === document.activeElement), true);
  await page.evaluate(() => history.back());
  await page.waitForFunction(() => document.activeElement.querySelector('strong')?.textContent === 'Loose compass');
  await page.evaluate(() => history.forward());
  await heading(page, 'Loose compass').waitFor();
  await page.evaluate(() => history.back());
  await ready(page);
  const opened = context.waitForEvent('page');
  await card(page, 'Loose compass').click({modifiers: [process.platform === 'darwin' ? 'Meta' : 'Control']});
  const clean = await opened;
  await heading(clean, 'Loose compass').waitFor();
  await scan(clean, source.forbidden);
  characterTraffic(traffic.get(clean) || []);
  await clean.close();
  await page.getByRole('navigation', {name: 'Compendium'}).getByRole('link', {name: 'Map', exact: true}).click();
  await heading(page, 'Locations and routes').waitFor();
  await page.locator('#locations').getByText('Dock', {exact: true}).waitFor();
  assert.equal(await main(page).locator('svg').count(), 0);
  assert.equal(await page.locator('#nav .private').count(), 0);
  const locations = await semantics(page);
  await scan(page, source.forbidden);
  await page.getByRole('navigation', {name: 'Compendium'}).getByRole('link', {name: 'Directory', exact: true}).click();
  await heading(page, 'Directory').waitFor();
  await ready(page);
  const firstCharacter = requests.slice(characterBoundary);
  assert.ok(firstCharacter.some(([, route]) => route === '/api/compendium/locations'));
  characterTraffic(firstCharacter);

  await page.locator('#change').click();
  await page.getByRole('button', {name: 'Return to Author discovery'}).click();
  await heading(page, 'Directory').waitFor();
  await ready(page);
  const secondBoundary = requests.length;
  await character(page, 'Second tide');
  await card(page, 'Bronze token').waitFor();
  const second = await semantics(page);
  const releaseFirst = await hold(page, 'page');
  await page.evaluate(() => history.go(-2));
  await page.waitForFunction(() => Boolean(window.matrixHold.release));
  assert.match(await page.locator('#context-detail').textContent(), /First tide/);
  await page.evaluate(() => history.go(2));
  await card(page, 'Bronze token').waitFor();
  await releaseFirst();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
  assert.match(await page.locator('#context-detail').textContent(), /Second tide/);
  assert.deepEqual(await semantics(page), second);
  await scan(page, source.forbidden);

  const releaseSearch = await hold(page, 'page');
  await page.locator('#directory-search').fill('Dock');
  await page.waitForFunction(() => Boolean(window.matrixHold.release));
  await page.locator('#directory-search').fill('Bronze');
  await card(page, 'Bronze token').waitFor();
  await releaseSearch();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
  assert.equal(await card(page, 'Dock').count(), 0);
  await scan(page, source.forbidden);

  const saved = fs.readFileSync(source.compass);
  const previous = fs.statSync(source.compass, {bigint: true});
  const renamed = Buffer.from(saved.toString().replace('Loose compass', 'Loose compaxs'));
  assert.equal(renamed.length, saved.length);
  assert.notDeepEqual(renamed, saved);
  const oldView = await page.evaluate(() => current.viewToken);
  try {
    fs.writeFileSync(source.compass, renamed);
    fs.utimesSync(source.compass, mtime, mtime);
    assert.equal(fs.statSync(source.compass, {bigint: true}).mtimeNs, previous.mtimeNs);
    const stale = await fetch(origin + '/api/compendium/page?view=' + oldView);
    assert.equal(stale.status, 409);
    assert.equal((await stale.json()).code, 'context_changed');
    await page.goto(origin + compassLink);
    await heading(page, 'Loose compaxs').waitFor();
    await scan(page, source.forbidden);
  } finally {
    fs.writeFileSync(source.compass, saved);
    fs.utimesSync(source.compass, mtime, mtime);
  }
  characterTraffic(requests.slice(secondBoundary));
  assert.deepEqual(pageErrors, []);
  await page.close();
  return {authorMap, first, second, locations, detail: detailBefore};
}

async function noPolicyReader(context, world) {
  const {origin, server} = await start(world, 'serve-reader');
  try {
    const page = await context.newPage();
    const paths = [];
    page.on('request', request => {
      const url = new URL(request.url());
      if (url.pathname.startsWith('/api/')) paths.push(url.pathname);
    });
    await page.goto(origin);
    await page.getByRole('heading', {name: 'This selection is unavailable.'}).waitFor();
    assert.ok(paths.length && paths.every(route => route.startsWith('/api/reader-presentation/')));
    await page.close();
  } finally {
    await stop(server);
  }
}

async function mainRun() {
  const browser = await chromium.launch({headless: true,
    executablePath: process.env.WEDL_TEST_CHROMIUM || undefined,
    args: process.platform === 'linux' ? ['--no-sandbox'] : []});
  try {
    const pair = [];
    for (const variant of fixture.variants) {
      const source = writeFixture(variant);
      const before = snapshot(source.story);
      const context = await browser.newContext({viewport: {width: 1280, height: 900}});
      const {origin, server} = await start(source.world, 'serve', ['--compendium-keyring', source.keyring]);
      try {
        pair.push(await traverse(context, source, origin));
        if (pair.length === 1) await noPolicyReader(context, source.world);
        assert.deepEqual(snapshot(source.story), before);
      } finally {
        try {
          await context.close();
        } finally {
          await stop(server);
        }
      }
    }
    assert.deepEqual(pair[0], pair[1], 'withheld fixture differences changed browser observations');
    assert.equal(hash(fs.readFileSync(binary)), binaryBefore);
    console.log('PASS differential DOM, accessible names, focus, SVG, links, Author and Character response races, freshness, closed Reader; binary ' + binaryBefore);
  } finally {
    await browser.close();
  }
}

mainRun().catch(error => {
  console.error(error.stack);
  process.exitCode = 1;
}).finally(() => {
  if (path.dirname(run) === artifacts && path.basename(run).startsWith('wedl-browser-compendium-')) {
    fs.rmSync(run, {recursive: true, force: true});
  }
});
