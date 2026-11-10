'use strict';

(() => {
  const byId = id => document.getElementById(id);
  const nav = byId('workflows');
  const panes = byId('panes');
  const labels = new Map([
    ['writing', 'Writing & Plan'], ['actions', 'Actions'], ['edits', 'Edits & Revisions'],
    ['manuscripts', 'Manuscripts'], ['retcons', 'Retcons'], ['publication', 'Publication'],
  ]);
  const screens = new Map();
  const extensions = new Map();
  const mounted = new Map();
  const allowedWrites = new Set(['/api/action-drafts/save', '/api/action-drafts/apply-outcome',
    '/api/edit-drafts/save', '/api/edit-drafts/apply', '/api/manuscripts/save', '/api/retcon-reviews/save']);
  const messages = {stale: 'The source changed. Reload the selection.', invalid: 'Choose a supported Workbench selection.',
    unsupported: 'This selection needs the native file workflow.', unavailable: 'This selection is unavailable.'};
  let source = null;
  let refreshed = false;
  let refreshEpoch = 0;
  let inspected = false;
  let submission = null;
  let publication = null;
  let frame = null;
  let plan = null;

  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined && text !== null) node.textContent = text;
    if (className) node.className = className;
    return node;
  }
  function button(label, action, className) {
    const node = element('button', label, className);
    node.type = 'button';
    node.addEventListener('click', action);
    return node;
  }
  function panel(title, ...children) {
    const node = element('section', null, 'panel');
    node.append(element('h3', title), ...children);
    return node;
  }
  function status(message = '') { byId('workbench-status').textContent = message; }
  function changed() {
    window.dispatchEvent(new Event('workbench-authority'));
    if (publication) renderReceipt(publication);
  }

  async function get(endpoint, fields = {}) {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(fields)) {
      if (value !== null && value !== undefined) query.set(key, value);
    }
    if (query.toString().length > 32768) throw new Error('This input is too large for the Workbench. Use the exact native file workflow.');
    const response = await fetch(`/api/workbench/${endpoint}?${query}`, {cache:'no-store', credentials:'omit'});
    const value = await response.json();
    if (!response.ok) throw new Error(messages[value.code] ?? messages.unavailable);
    return value;
  }
  async function refresh(announce = true) {
    const epoch = ++refreshEpoch;
    refreshed = false;
    inspected = false;
    if (submission) delete submission.semanticVerification;
    window.dispatchEvent(new Event('workbench-source'));
    changed();
    try {
      const captured = await get('source');
      if (epoch !== refreshEpoch) return false;
      source = captured;
      refreshed = true;
      byId('authority').textContent = source.writes === true ? 'Explicit live Author' : source.current ? 'Read-only Author worktree' : 'Read-only historical Author snapshot';
      byId('world').textContent = source.label;
      if (announce) status('Source refreshed. Review the exact saved candidate again before applying.');
      return true;
    } catch {
      if (epoch !== refreshEpoch) return false;
      source = null;
      byId('authority').textContent = 'Source unavailable';
      byId('world').textContent = '';
      if (announce) status('The source could not be refreshed.');
      return false;
    } finally {
      if (epoch === refreshEpoch) {
        changed();
        window.dispatchEvent(new Event('workbench-source'));
      }
    }
  }

  function canWrite() {
    return refreshed && source?.writes === true && submission?.state !== 'pending'
      && (submission?.state !== 'unconfirmed' || (refreshed && inspected && submission.manuallyInspected === true));
  }
  const fileText = async token => (await get('file', {token})).bytes;
  const nativeRequest = async (route, token) => (await get('native-request', {route, token})).bytes;
  function downloadBytes(bytes, name) {
    const url = URL.createObjectURL(new Blob([bytes], {type:'application/octet-stream'}));
    const link = element('a');
    link.href = url;
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  async function download(token, name) { downloadBytes(await fileText(token), name); }

  function note(state, message, values = {}) {
    submission = {...submission, state, message, ...values};
    changed();
    return submission;
  }
  async function write(route, raw, label, requestToken = null) {
    if (!allowedWrites.has(route) || !canWrite()) throw new Error('Publication is unavailable until the source and previous submission are checked.');
    if (typeof raw !== 'string' && !(raw instanceof Uint8Array)) throw new Error('Choose an exact native request file.');
    refreshed = false;
    inspected = false;
    submission = {route, raw, label, requestToken};
    note('pending', 'The exact operation was submitted. Waiting for acknowledgement.');
    let response;
    try {
      response = await fetch(route, {method:'POST', cache:'no-store', credentials:'omit', body:raw,
        headers:{'Content-Type':'application/json','X-WEDL-Write':'1','Accept':'application/vnd.wedl.workbench+json'}});
      const facts = await response.json();
      if (!response.ok && facts.outcome === 'refused') return note('refused', 'The operation was refused before native dispatch. Reload the saved candidate and run its checks again.');
      if (!response.ok || facts.outcome !== 'confirmed' || facts.confirmed !== true) return note('unconfirmed', 'The acknowledgement was unavailable. Refresh source and inspect the receipt before resubmitting.');
      // Keep the positive acknowledgement even if the subsequent read fails.
      submission.facts = facts;
    } catch {
      return note('unconfirmed', 'The acknowledgement was lost. Refresh source and inspect the receipt before resubmitting.');
    }
    const current = await refresh(false);
    return note(current ? 'confirmed' : 'confirmed-stale', current ? 'Publication confirmed and source refreshed.' : 'Publication confirmed. Source refresh failed.');
  }
  async function writePrepared(route, token, label) { return write(route, await nativeRequest(route, token), label, token); }

  async function inspect() {
    if (!submission || submission.state === 'pending') return;
    if (!refreshed && !await refresh(false)) {
      status('Refresh the source before inspecting this operation.');
      return;
    }
    const selected = submission;
    const epoch = refreshEpoch;
    try {
      const check = selected.requestToken && await get('inspect', {route:selected.route, token:selected.requestToken});
      if (submission !== selected || epoch !== refreshEpoch || !refreshed) return;
      inspected = true;
      const semanticVerification = check?.semanticVerification;
      if (check?.verified === true && selected.state === 'unconfirmed') {
        note('confirmed', 'The exact saved result was verified after refreshing source.', {facts:{sourceMatches:true}, semanticVerification});
        window.dispatchEvent(new CustomEvent('workbench-recovered-write', {detail:selected.route}));
      } else {
        selected.semanticVerification = semanticVerification;
        status(check?.verified === true ? 'The saved result matches the submitted request.' : 'The saved result could not be verified. Inspect the exact request and current native receipt before continuing.');
        changed();
      }
    } catch {
      if (submission === selected && epoch === refreshEpoch) {
        inspected = false;
        status('The saved result could not be verified. Inspect the exact request and current native receipt before continuing.');
        changed();
      }
    }
  }

  function renderReceipt(container) {
    const titles = {pending:'Pending', refused:'Refused', confirmed:'Confirmed · source refreshed',
      'confirmed-stale':'Confirmed · refresh failed', unconfirmed:'Unconfirmed'};
    if (!submission) {
      container.replaceChildren(panel('Last submitted operation', element('p', 'No operation has been submitted in this Workbench.', 'muted')));
      return;
    }
    const report = element('div', null, `panel receipt ${submission.state === 'refused' ? 'error' : submission.state === 'confirmed' ? '' : 'warning'}`);
    report.append(element('p', titles[submission.state], 'tag'), element('h4', submission.label), element('p', submission.message));
    for (const [key, label] of [['sourceMatches','Source match'],['evidenceVerified','Original evidence'],['canonicalNoOp','Canonical no-op'],['alreadyApplied','Previously applied'],['writesPerformed','Writes performed']]) {
      if (typeof submission.facts?.[key] === 'boolean') report.append(element('p', `${label} · ${submission.facts[key] ? 'Yes' : 'No'}`, 'muted'));
    }
    if (submission.semanticVerification?.length) {
      const checks = element('ul', null, 'checks');
      for (const check of submission.semanticVerification) {
        checks.append(element('li', `${check.policy} · ${check.label} · ${check.status}`));
      }
      report.append(element('h4', 'Current-source semantic verification'), checks);
    }
    const actions = element('div', null, 'row');
    const inspectButton = button('Inspect receipt', inspect, 'secondary');
    inspectButton.disabled = submission.state === 'pending';
    actions.append(button('Refresh source', () => refresh(), 'secondary'), inspectButton,
      button('Download exact request', () => downloadBytes(submission.raw, 'wedl-submitted-request.json'), 'secondary'));
    if (submission.state === 'unconfirmed' && refreshed && inspected) {
      actions.append(button('I inspected the native receipt', () => {
        note('unconfirmed', 'Result remains unconfirmed. A new explicit operation requires a fresh selection and review.', {manuallyInspected:true});
        status('Review the fresh saved candidate before submitting another operation.');
      }, 'secondary'));
    }
    container.replaceChildren(panel('Last submitted operation', report, actions));
  }

  function register(name, render) { screens.set(name, render); }
  function extend(name, render) { extensions.set(name, render); }
  function show(name, focus = true) {
    if (!labels.has(name)) return;
    if (!mounted.has(name)) {
      const pane = element('section');
      const heading = element('h2', labels.get(name));
      heading.tabIndex = -1;
      const layout = element('div', null, 'layout');
      const main = element('div', null, 'stack');
      const side = element('aside', null, 'stack');
      side.setAttribute('aria-label', 'Source and review');
      layout.append(main, side);
      pane.append(element('p', 'Private workflow', 'eyebrow'), heading, layout);
      panes.append(pane);
      mounted.set(name, pane);
      if (screens.has(name)) screens.get(name)(main, side);
      else main.append(panel('Unavailable', element('p', 'This workflow could not be opened.', 'muted')));
      extensions.get(name)?.(main, side);
    }
    for (const [key, pane] of mounted) pane.hidden = key !== name;
    for (const link of nav.querySelectorAll('button')) {
      if (link.dataset.pane === name) link.setAttribute('aria-current', 'page');
      else link.removeAttribute('aria-current');
    }
    if (focus) mounted.get(name).querySelector('h2').focus();
  }
  register('publication', main => { publication = main; renderReceipt(main); });

  window.WedlWorkbench = {register, extend, get, write, writePrepared, canWrite, download, fileText, nativeRequest,
    source:() => source, sourceReady:() => refreshed, refresh, status, element, button, panel, show, frame:() => frame,
    setFrame:value => {frame = value;window.dispatchEvent(new Event('workbench-frame'));},
    plan:() => plan, setPlan:value => {plan = value;window.dispatchEvent(new Event('workbench-plan'));}};
  window.addEventListener('DOMContentLoaded', async () => {
    for (const [key, label] of labels) {
      const link = button(label, () => show(key));
      link.dataset.pane = key;
      nav.append(link);
    }
    byId('refresh').addEventListener('click', () => refresh());
    byId('skip').addEventListener('click', event => {event.preventDefault(); byId('workspace').focus();});
    await refresh(false);
    show('writing', false);
  });
})();
