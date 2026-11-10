'use strict';

const byId = id => document.getElementById(id);
const api = '/api/reader-presentation/';
const protocol = 'wedl-reader-browser/v1';
const selector = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const choice = value => value && selector(value.selector) && typeof value.label === 'string';
const optionalChoice = value => value === null || choice(value);
const labels = new Map([['record', 'Revealed record'], ['section', 'Revealed section'], ['text', 'Revealed text']]);
let epoch = 0;
let view = null;

function pending(message) {
  epoch++;
  view = null;
  for (const id of ['choices', 'search-form', 'reader-results', 'unavailable']) byId(id).hidden = true;
  for (const id of ['edition', 'position', 'reader-items']) byId(id).replaceChildren();
  for (const id of ['reader-edition', 'reader-position', 'reader-count']) byId(id).textContent = '';
  updateNavigation('previous', null, 'Previous position');
  updateNavigation('next', null, 'Next position');
  byId('more').hidden = true;
  byId('reader-status').textContent = message;
  return epoch;
}

function unavailable(focus) {
  pending('');
  byId('query').value = '';
  byId('unavailable').hidden = false;
  if (focus) byId('unavailable-heading').focus();
}

async function request(path, input) {
  const options = {credentials: 'omit', cache: 'no-store'};
  if (input !== undefined) {
    options.method = 'POST';
    options.headers = {'Content-Type': 'application/json'};
    options.body = JSON.stringify(input);
  }
  const response = await fetch(api + path, options);
  const value = await response.json();
  if (!response.ok || value.protocol !== protocol || value.outcome !== 'available') throw new Error();
  return value;
}

function readAddress() {
  if (!location.hash || location.hash === '#reading') return null;
  const match = /^#read\/([0-9a-f]{64})\/([0-9a-f]{64})$/.exec(location.hash);
  if (!match) {
    history.replaceState(null, '', location.pathname);
    throw new Error();
  }
  return {edition: match[1], position: match[2]};
}

function address(edition, position) {
  return `#read/${edition}/${position}`;
}

async function resolve(focus = false) {
  byId('query').value = '';
  const ticket = pending('Opening the published edition…');
  try {
    const route = readAddress();
    const catalogue = await request('choices');
    if (ticket !== epoch) return;
    if (!choice(catalogue.edition) || !choice(catalogue.entry) || !Array.isArray(catalogue.positions)
      || catalogue.positions.length > 1024 || !catalogue.positions.every(choice)
      || (route && route.edition !== catalogue.edition.selector)) throw new Error();
    const position = route?.position ?? catalogue.entry.selector;
    const value = await reading(catalogue, position, null, null);
    if (ticket !== epoch) return;
    render(catalogue, value, null, focus);
    if (!route) history.replaceState(null, '', location.pathname + address(catalogue.edition.selector, position));
  } catch {
    if (ticket === epoch) unavailable(focus);
  }
}

async function reading(catalogue, position, query, cursor) {
  const input = {edition: catalogue.edition.selector, position, cursor};
  if (query !== null) input.query = query;
  const value = await request(query === null ? 'read' : 'find', input);
  if (!choice(value.position) || value.position.selector !== position || !optionalChoice(value.previous)
    || !optionalChoice(value.next) || !Array.isArray(value.items) || !Number.isInteger(value.count)
    || value.count < value.items.length || value.count > 1024
    || (value.nextCursor !== null && (typeof value.nextCursor !== 'string' || !/^[0-9a-f]{72}$/.test(value.nextCursor)))
    || !value.items.every(item => item && labels.has(item.kind) && typeof item.markdown === 'string'
      && (item.title === null || typeof item.title === 'string'))) throw new Error();
  return value;
}

function updateNavigation(id, next, fallback) {
  const button = byId(id);
  button.disabled = next === null;
  button.textContent = next && next.label !== fallback ? `${fallback} · ${next.label}` : fallback;
}

function itemNode(item) {
  const article = document.createElement('article');
  article.className = 'item';
  const kind = document.createElement('p');
  kind.className = 'eyebrow';
  kind.textContent = labels.get(item.kind);
  article.append(kind);
  if (item.title !== null) {
    const title = document.createElement('h3');
    title.textContent = item.title;
    article.append(title);
  }
  const prose = document.createElement('p');
  prose.className = 'prose';
  prose.textContent = item.markdown;
  article.append(prose);
  return article;
}

function render(catalogue, value, query, focus) {
  const {edition, entry, positions} = catalogue;
  const options = new Map([entry, ...positions].map(option => [option.selector, option.label]));
  if (!options.has(value.position.selector)) options.set(value.position.selector, value.position.label);
  byId('edition').replaceChildren(new Option(edition.label, edition.selector));
  const position = byId('position');
  position.replaceChildren(...Array.from(options, ([selector, label]) => new Option(label, selector)));
  position.value = value.position.selector;
  byId('reader-edition').textContent = edition.label;
  byId('reader-position').textContent = options.get(value.position.selector);
  byId('reader-count').textContent = `${value.count} revealed ${query === null ? (value.count === 1 ? 'item' : 'items') : (value.count === 1 ? 'match' : 'matches')}`;
  if (value.items.length) {
    byId('reader-items').replaceChildren(...value.items.map(itemNode));
  } else {
    const empty = document.createElement('p');
    empty.className = 'empty';
    empty.textContent = query === null ? 'No content is revealed at this position.' : 'No revealed content matches this search.';
    byId('reader-items').replaceChildren(empty);
  }
  updateNavigation('previous', value.previous, 'Previous position');
  updateNavigation('next', value.next, 'Next position');
  byId('more').hidden = value.nextCursor === null;
  for (const id of ['choices', 'search-form', 'reader-results']) byId(id).hidden = false;
  byId('reader-status').textContent = '';
  view = {catalogue, value, query};
  if (focus) byId('reader-position').focus();
}

async function searchOrContinue(query, append) {
  if (!view) return;
  const {catalogue, value: old} = view;
  const cursor = append ? old.nextCursor : null;
  if (append && cursor === null) return;
  if (query !== null && (!query.trim() || [...query].length > 256 || new TextEncoder().encode(query).length > 1024)) {
    byId('reader-status').textContent = 'Enter up to 256 search characters.';
    return;
  }
  const ticket = pending(query === null ? 'Reading…' : 'Searching revealed content…');
  try {
    const value = await reading(catalogue, old.position.selector, query, cursor);
    if (ticket !== epoch) return;
    if (append) value.items = [...old.items, ...value.items];
    render(catalogue, value, query, true);
  } catch {
    if (ticket === epoch) unavailable(true);
  }
}

function open(position) {
  if (!view || !selector(position)) return;
  const next = address(view.catalogue.edition.selector, position);
  if (next !== location.hash) history.pushState(null, '', location.pathname + next);
  resolve(true);
}

byId('skip').addEventListener('click', event => {
  event.preventDefault();
  byId('reading').focus();
});
byId('reader-form').addEventListener('submit', event => {
  event.preventDefault();
  open(byId('position').value);
});
byId('search-form').addEventListener('submit', event => {
  event.preventDefault();
  searchOrContinue(byId('query').value, false);
});
for (const direction of ['previous', 'next']) {
  byId(direction).addEventListener('click', () => open(view?.value[direction]?.selector));
}
byId('more').addEventListener('click', () => searchOrContinue(view?.query ?? null, true));
byId('reopen').addEventListener('click', () => {
  history.replaceState(null, '', location.pathname);
  resolve(true);
});
window.addEventListener('hashchange', () => resolve(true));
resolve();
