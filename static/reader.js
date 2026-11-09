'use strict';
const $ = id => document.getElementById(id);
const PROTOCOL = 'wedl-reader-presentation/v1';
let epoch = 0, result = null, last = null;
function clear(message = '') {
  epoch++; result = null; last = null; $('reader-results').hidden = true; $('reader-items').replaceChildren();
  for (const id of ['previous','next','more']) $(id).disabled = true;
  $('reader-status').textContent = message; $('reader-status').classList.remove('error');
}
function input(operation) {
  const revision = $('revision').value, publicationId = $('publication').value, occurrence = Number($('occurrence').value), limit = Number($('page-size').value);
  if (!/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(revision) || !/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(publicationId) || !Number.isInteger(occurrence) || occurrence < 0 || occurrence > 1023 || !Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error('Enter an exact publication, edition and reading position.');
  const value = {protocol:PROTOCOL,publicationId,revision,occurrence,limit,cursor:null};
  if (operation === 'search') { const query = $('query').value; if (!query.trim() || [...query].length > 256 || new TextEncoder().encode(query).length > 1024) throw new Error('Enter 1–256 search characters.'); value.query = query; }
  return value;
}
function fail(message) { clear(message); $('reader-status').classList.add('error'); }
function render(value,operation,request) {
  if (value.protocol !== PROTOCOL || value.mode !== 'reader' || value.operation !== operation || value.revision !== request.revision || value.occurrence !== request.occurrence || !Array.isArray(value.items) || !Number.isInteger(value.count) || value.count < value.items.length || value.count > 1024 || (value.nextCursor !== null && (typeof value.nextCursor !== 'string' || !/^[0-9a-f]{72}$/.test(value.nextCursor)))) throw new Error('Reader presentation unavailable.');
  const nodes = [];
  for (const item of value.items) {
    if (!['record','section','text'].includes(item.kind) || typeof item.markdown !== 'string' || (item.title !== null && typeof item.title !== 'string')) throw new Error('Reader presentation unavailable.');
    const article = document.createElement('article'); article.className = 'item';
    const kind = document.createElement('p'); kind.className = 'kind'; kind.textContent = item.kind === 'text' ? 'Revealed text' : item.kind === 'section' ? 'Revealed section' : 'Revealed record'; article.append(kind);
    if (item.title !== null) { const title = document.createElement('h2'); title.textContent = item.title; article.append(title); }
    const prose = document.createElement('pre'); prose.className = 'prose'; prose.textContent = item.markdown; article.append(prose); nodes.push(article);
  }
  $('reader-items').replaceChildren(...nodes); $('reader-count').textContent = value.count + (operation === 'search' ? ' revealed matches' : ' revealed items') + ' · position ' + value.occurrence;
  for (const [button,key] of [['previous','previousOccurrence'],['next','nextOccurrence']]) { const ordinal = value.navigation?.[key]; if (ordinal !== null && (!Number.isInteger(ordinal) || ordinal < 0 || ordinal > 1023)) throw new Error('Reader presentation unavailable.'); $(button).disabled = ordinal === null; }
  $('more').disabled = value.nextCursor === null; $('reader-results').hidden = false; $('reader-status').textContent = value.items.length ? '' : 'No content is revealed for this selection.';
  result = value; last = {operation,request};
}
async function read(operation,continuation = null) {
  let request;
  try { request = input(operation); if (continuation !== null) request.cursor = continuation; } catch (error) { fail(error.message); return; }
  clear('Reading…'); const ticket = epoch;
  try {
    const response = await fetch('/api/reader-presentation/' + operation,{method:'POST',credentials:'omit',cache:'no-store',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)}), value = await response.json();
    if (ticket !== epoch) return;
    if (!response.ok || value.outcome !== 'available') { fail(value.protocol === PROTOCOL && typeof value.message === 'string' ? value.message : 'Reader presentation unavailable.'); return; }
    render(value,operation,request);
  } catch (error) { if (ticket === epoch) fail(error.message === 'Reader presentation unavailable.' ? error.message : 'Reader presentation unavailable.'); }
}
$('reader-form').addEventListener('submit',event=>{event.preventDefault();read('page');});
$('search-form').addEventListener('submit',event=>{event.preventDefault();read('search');});
for (const id of ['publication','revision','occurrence','page-size','query']) $(id).addEventListener('input',()=>clear('Selection changed. Read or search again.'));
for (const [button,key] of [['previous','previousOccurrence'],['next','nextOccurrence']]) $(button).addEventListener('click',()=>{if (!result || result.navigation[key] === null || !last) return; $('occurrence').value = String(result.navigation[key]); read(last.operation);});
$('more').addEventListener('click',()=>{if (result?.nextCursor && last) read(last.operation,result.nextCursor);});
const fragment = new URLSearchParams(location.hash.slice(1));
if (fragment.has('publication')) $('publication').value = fragment.get('publication');
if (fragment.has('revision')) $('revision').value = fragment.get('revision');
if (fragment.has('occurrence')) $('occurrence').value = fragment.get('occurrence');
