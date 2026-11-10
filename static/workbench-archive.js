(() => {
  'use strict';

  const workbench = window.WedlWorkbench;
  const manuscriptSave = '/api/manuscripts/save';
  const reviewSave = '/api/retcon-reviews/save';
  const unavailable = 'This selection is unavailable. Reload its source and try again.';
  const normalized = text => text.replace(/\r\n?/g, '\n');

  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  }

  function panel(title, description) {
    const section = element('section', undefined, 'panel');
    section.append(element('h2', title));
    if (description) section.append(element('p', description, 'muted'));
    return section;
  }

  function status(text) {
    const node = element('p', text, 'muted');
    node.setAttribute('role', 'status');
    return node;
  }

  function button(text, listener) {
    const node = element('button', text);
    node.type = 'button';
    node.addEventListener('click', listener);
    return node;
  }

  function control(text, input) {
    const label = element('label');
    label.append(element('span', text), input);
    return label;
  }

  function fill(select, items, placeholder) {
    select.replaceChildren(new Option(placeholder, ''));
    for (const item of items) select.add(new Option(item.label, item.token));
    select.disabled = items.length === 0;
  }

  function details(pairs) {
    const list = element('dl');
    for (const [name, value] of pairs) list.append(element('dt', name), element('dd', value));
    return list;
  }

  function row(...children) {
    const node = element('div', undefined, 'row');
    node.append(...children);
    return node;
  }

  function choices() {
    return workbench.get('archive-sources');
  }

  function exactImport(route, title, limit, notice) {
    const input = element('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    let busy = false;
    const submit = button('Submit exact private save file', async () => {
      if (!workbench.canWrite()) {
        notice.textContent = 'Enable live Author writes and inspect any pending receipt.';
        return;
      }
      const file = input.files[0];
      if (!file || file.size > limit) {
        notice.textContent = 'Choose a supported native save file within the source size limit.';
        return;
      }
      busy = true;
      update();
      try {
        const receipt = await workbench.write(route, new Uint8Array(await file.arrayBuffer()), title);
        notice.textContent = receipt.message;
        input.value = '';
      } catch {
        notice.textContent = 'The write could not be confirmed. Inspect Publication before retrying.';
      } finally {
        busy = false;
        update();
      }
    });
    function update() {
      submit.disabled = busy || !workbench.canWrite();
    }
    update();
    window.addEventListener('workbench-authority', update);
    const section = element('details');
    section.append(element('summary', 'Exact file exchange'), element('p',
      'Submit a complete native save request. The native writer rechecks the source and private-file pins.', 'muted'),
      control('Native save file', input), submit);
    return section;
  }

  function withLineEndings(original, value, fallback = '\n') {
    if (value === normalized(original)) return original;
    const endings = new Set(original.match(/\r\n|\r|\n/g) || []);
    if (endings.size > 1) throw new Error('mixed-line-endings');
    const ending = endings.values().next().value || fallback;
    return value.replaceAll('\n', ending);
  }

  function manuscriptParts(request, bodyBytes) {
    if (typeof request.source !== 'string' || !Number.isSafeInteger(bodyBytes) || bodyBytes < 0) {
      throw new Error('invalid-native-source');
    }
    const bytes = new TextEncoder().encode(request.source);
    if (bodyBytes > bytes.length) throw new Error('invalid-native-source');
    const decoder = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true});
    const boundary = bytes.length - bodyBytes;
    return [decoder.decode(bytes.subarray(0, boundary)), decoder.decode(bytes.subarray(boundary))];
  }

  async function sha256(text) {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
    return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
  }

  function manuscripts(main, side) {
    const section = panel('Private manuscript', 'Read the selected private source and its authored order.');
    const source = element('select');
    const documentChoice = element('select');
    const position = element('select');
    const plan = element('select');
    const textPolicy = element('input');
    textPolicy.type = 'checkbox';
    const notice = status('Choose a manuscript source.');
    const result = element('div', undefined, 'stack');
    const editor = element('div', undefined, 'stack');
    const simulation = panel('Reader simulation',
      'A separately installed simulation policy admits this private preview at the snapshot’s current story position.');
    const preview = element('div', undefined, 'stack');
    const previewStatus = status('Choose a named position and reveal plan from the same original snapshot.');
    let epoch = 0;
    let selection = null;
    let activeDocument = '';
    let editorEpoch = 0;
    let simulationEpoch = 0;
    let editButton = null;

    function clear() {
      epoch += 1;
      editorEpoch += 1;
      simulationEpoch += 1;
      selection = null;
      result.replaceChildren();
      editor.replaceChildren();
      editButton = null;
      preview.replaceChildren();
      previewStatus.textContent = 'Choose a named position and reveal plan from the same original snapshot.';
      updateSimulation();
    }

    function updateSimulation() {
      simulate.disabled = !position.value || !plan.value;
    }

    async function loadDocuments() {
      clear();
      activeDocument = '';
      fill(documentChoice, [], 'Choose a document');
      fill(position, [], 'Choose a reading position');
      fill(plan, [], 'Choose a reveal plan');
      updateSimulation();
      const generation = epoch;
      if (!source.value) return;
      notice.textContent = 'Reading private document choices…';
      try {
        const value = await workbench.get('archive-documents', {source: source.value});
        if (generation !== epoch) return;
        fill(documentChoice, value.items.map(item => ({
          label: item.label + ' · ' + item.kind, token: item.token,
        })), 'Choose a document');
        fill(plan, value.editable ? [] : value.items.filter(item => item.plan), 'Choose a reveal plan');
        notice.textContent = value.items.length ? value.source + ' · choose a document' : 'No private documents are available.';
      } catch {
        if (generation === epoch) notice.textContent = unavailable;
      }
    }

    function renderDocument(value) {
      selection = value;
      const article = panel(value.label || 'Private document');
      article.append(details([['Document', value.kind], ['Source', value.source]]),
        element('p', value.prose || 'No prose in this document.', 'prose'));
      const actions = row(button('Download exact source', () => workbench.download(value.file, 'wedl-manuscript.md')));
      if (value.editable && activeDocument) {
        editButton = button('Capture source and edit prose', openEditor);
        editButton.disabled = !workbench.canWrite();
        actions.append(editButton);
      }
      article.append(actions);
      result.replaceChildren(article);
    }

    async function loadDocument() {
      clear();
      fill(position, [], 'Choose a reading position');
      activeDocument = documentChoice.value;
      updateSimulation();
      const generation = epoch;
      if (!activeDocument) return;
      notice.textContent = 'Reading private document…';
      try {
        const value = await workbench.get('archive-document', {document: activeDocument});
        if (generation !== epoch) return;
        renderDocument(value);
        notice.textContent = 'Private source loaded.';
        if (value.kind === 'Private work') {
          const positions = await workbench.get('archive-work-positions', {document: activeDocument});
          if (generation === epoch) fill(position, positions.items, 'Choose a reading position');
        }
      } catch {
        if (generation === epoch) notice.textContent = unavailable;
      }
    }

    async function loadPosition() {
      clear();
      activeDocument = '';
      const generation = epoch;
      if (!position.value) return;
      notice.textContent = 'Reading the selected manuscript occurrence…';
      try {
        const value = await workbench.get('archive-position', {position: position.value});
        if (generation !== epoch) return;
        renderDocument(value);
        notice.textContent = 'Authored position loaded.';
      } catch {
        if (generation === epoch) notice.textContent = unavailable;
      }
    }

    async function openEditor() {
      if (!workbench.canWrite() || !selection?.editable || !activeDocument) {
        notice.textContent = 'Enable live Author writes and inspect any pending receipt before editing.';
        return;
      }
      const currentEditor = ++editorEpoch;
      const document = activeDocument;
      editor.replaceChildren(status('Capturing current source and exact private-file pins…'));
      try {
        const capture = await workbench.get('archive-manuscript-edit', {document});
        const raw = await workbench.nativeRequest(manuscriptSave, capture.template);
        if (currentEditor !== editorEpoch || document !== activeDocument) return;
        const request = JSON.parse(raw);
        const [prefix, original] = manuscriptParts(request, capture.bodyBytes);
        const body = element('textarea');
        body.rows = 12;
        body.value = original;
        const state = status('Exact source captured for this edit.');
        const save = button('Save manuscript', async () => {
          if (!workbench.canWrite()) {
            state.textContent = 'Inspect Publication before retrying the write.';
            return;
          }
          let source;
          const edited = body.value;
          try {
            source = prefix + withLineEndings(original, edited, prefix.includes('\r\n') ? '\r\n' : '\n');
          } catch {
            state.textContent = 'This edit has mixed line endings. Use exact file exchange to preserve the source.';
            return;
          }
          save.disabled = true;
          let proof;
          try {
            proof = await workbench.get('archive-manuscript-proof', {edit: capture.edit, sha256: await sha256(source)});
          } catch {
            if (currentEditor === editorEpoch) {
              state.textContent = 'The exact edit could not be prepared. Recapture the source before trying again.';
              save.disabled = false;
            }
            return;
          }
          if (currentEditor !== editorEpoch || document !== activeDocument) return;
          if (body.value !== edited) {
            save.disabled = false;
            state.textContent = 'The prose changed during preparation. Check it and save again.';
            return;
          }
          try {
            const receipt = await workbench.write(manuscriptSave, JSON.stringify({...request, source}), 'Manuscript save', proof.token);
            if (currentEditor === editorEpoch) {
              state.textContent = receipt.message + ' Reload the document before preparing another save.';
            }
          } catch {
            if (currentEditor === editorEpoch) state.textContent = 'The write could not be confirmed. Inspect Publication before retrying.';
          }
        });
        const form = panel('Private prose');
        form.append(control('Manuscript body', body), row(save, button('Close editor', () => {
          editorEpoch += 1;
          editor.replaceChildren();
        })), state);
        editor.replaceChildren(form);
      } catch {
        if (currentEditor === editorEpoch) editor.replaceChildren(status(unavailable));
      }
    }

    const simulate = button('Open simulation preview', async () => {
      if (!position.value || !plan.value) return;
      const generation = ++simulationEpoch;
      preview.replaceChildren();
      previewStatus.textContent = 'Checking installed simulation permission…';
      try {
        const value = await workbench.get('archive-simulation', {
          position: position.value, plan: plan.value, text: textPolicy.checked ? 'admitted' : 'none',
        });
        if (generation !== simulationEpoch) return;
        previewStatus.textContent = 'Private simulation under the installed policy.';
        for (const item of value.items) {
          const passage = panel(item.label);
          passage.append(element('p', item.prose, 'prose'));
          preview.append(passage);
        }
        if (!value.items.length) preview.append(element('p', 'No passages are admitted at this position.'));
        if (value.moreAvailable) preview.append(element('p', 'More admitted passages are available through the native preview.'));
      } catch {
        if (generation === simulationEpoch) previewStatus.textContent = 'Private simulation unavailable for this selection.';
      }
    });
    function clearPreview() {
      simulationEpoch += 1;
      preview.replaceChildren();
      previewStatus.textContent = 'Open a new simulation for this selection.';
      updateSimulation();
    }
    source.addEventListener('change', loadDocuments);
    documentChoice.addEventListener('change', loadDocument);
    position.addEventListener('change', loadPosition);
    plan.addEventListener('change', clearPreview);
    textPolicy.addEventListener('change', clearPreview);
    window.addEventListener('workbench-authority', () => {
      if (editButton) editButton.disabled = !workbench.canWrite();
    });
    window.addEventListener('workbench-source', () => {
      if (editor.childElementCount) {
        editorEpoch += 1;
        editor.replaceChildren(status('Source refreshed. Capture it again before editing.'));
      }
    });
    section.append(control('Source', source), control('Document', documentChoice),
      control('Reading position', position), row(button('Reload private source', loadDocuments)),
      notice, result, editor, exactImport(manuscriptSave, 'Manuscript save', 8 * 1024 * 1024, notice));
    simulation.append(control('Reveal plan', plan), control('Include explicitly admitted plan text', textPolicy),
      simulate, previewStatus, preview);
    main.append(section, simulation);
    const sourceContext = panel('Source context', 'Manuscript files and Reader simulation have separate source pins and permissions.');
    const originalState = status('');
    sourceContext.append(originalState);
    side.append(sourceContext);
    fill(source, [], 'Choose a source');
    fill(documentChoice, [], 'Choose a document');
    fill(position, [], 'Choose a reading position');
    fill(plan, [], 'Choose a reveal plan');
    updateSimulation();
    choices().then(({items, originalsAvailable}) => {
      if (!originalsAvailable) originalState.textContent = 'Original snapshot choices are unavailable.';
      fill(source, items, 'Choose a source');
      if (items.length) {
        source.value = items[0].token;
        return loadDocuments();
      }
      notice.textContent = 'No source choices are available.';
    }).catch(() => { notice.textContent = unavailable; });
  }

  function retcons(main, side) {
    const section = panel('Retcon comparison', 'Keep the fixed original analysis separate from its currentness comparison.');
    const source = element('select');
    const review = element('select');
    const notice = status('Choose a saved review source.');
    const results = element('div', undefined, 'stack');
    const captureFile = element('input');
    captureFile.type = 'file';
    captureFile.accept = 'application/json,.json';
    let epoch = 0;
    let discardPrepared = () => {};

    async function loadQueue() {
      const generation = ++epoch;
      discardPrepared = () => {};
      results.replaceChildren();
      fill(review, [], 'Choose a saved review');
      if (!source.value) return;
      notice.textContent = 'Reading the private review queue…';
      try {
        const value = await workbench.get('archive-reviews', {source: source.value});
        if (generation !== epoch) return;
        fill(review, value.items.map(item => ({
          label: item.label + ' · ' + item.state, token: item.token,
        })), 'Choose a saved review');
        notice.textContent = value.items.length ? value.source + ' · choose a review' : 'No saved reviews are available.';
      } catch {
        if (generation === epoch) notice.textContent = unavailable;
      }
    }

    function showReview(value, generation) {
      const article = panel(value.label || 'Private review');
      article.append(details([['Original', value.original], ['Compared source', value.comparison]]));
      if (value.file) article.append(button('Download original review file', () =>
        workbench.download(value.file, 'wedl-retcon-review.json')));
      if (value.editable && !value.draft && review.value) {
        article.append(button('Capture source for decisions', () => openReview(true)));
      }
      const changes = [];
      let prepared = null;
      let prepareEpoch = 0;
      let title;
      let save;
      let download;
      const state = status(value.draft ? 'Change dispositions, then prepare the exact private save.' : 'Original impact order is preserved.');
      const clearPrepared = () => {
        prepared = null;
        prepareEpoch += 1;
        if (save) save.disabled = true;
        if (download) download.disabled = true;
      };
      discardPrepared = clearPrepared;
      if (value.draft) {
        title = element('input');
        title.value = value.label;
        title.maxLength = 1024;
        title.addEventListener('input', clearPrepared);
        article.append(control('Private review title', title));
      }
      value.impacts.forEach((impact, index) => {
        const entry = panel(impact.record + ' · ' + impact.relation);
        entry.append(element('p', impact.source, 'muted'));
        const disposition = element('select');
        for (const [key, label] of [['', 'Pending'], ['reviewed', 'Reviewed'],
          ['needs-revision', 'Needs revision'], ['intentionally-retained', 'Intentionally retained']]) {
          disposition.add(new Option(label, key));
        }
        disposition.value = impact.disposition || '';
        disposition.disabled = !value.draft;
        const note = element('textarea');
        note.rows = 3;
        note.value = impact.note;
        note.readOnly = !value.draft || !disposition.value;
        disposition.addEventListener('change', () => {
          clearPrepared();
          note.readOnly = !disposition.value;
          if (!disposition.value) note.value = '';
        });
        note.addEventListener('input', clearPrepared);
        entry.append(control('Disposition', disposition), control('Private note', note));
        changes.push({index, disposition, note, original: impact});
        article.append(entry);
      });
      if (!value.impacts.length) article.append(element('p', 'No impact items were recorded.'));
      if (value.draft) {
        const prepare = button('Prepare private review save', async () => {
          clearPrepared();
          const request = prepareEpoch;
          const changed = changes.filter(({disposition, note, original}) =>
            disposition.value !== (original.disposition || '') || note.value !== normalized(original.note))
            .map(({index, disposition, note, original}) => ({
              index, disposition: disposition.value || null,
              note: note.value !== normalized(original.note) ? note.value : null,
            }));
          const fields = {draft: value.draft, changes: JSON.stringify(changed)};
          if (title.value !== value.label) fields.title = title.value;
          if (new URLSearchParams(fields).toString().length > 30 * 1024) {
            state.textContent = 'The combined edits exceed the browser preparation bound. Use exact file exchange.';
            return;
          }
          state.textContent = 'Checking original analysis, ordered impact decisions, and private save pins…';
          try {
            const checked = await workbench.get('archive-review-prepare', fields);
            if (generation !== epoch || request !== prepareEpoch) return;
            prepared = checked;
            save.disabled = !workbench.canWrite();
            download.disabled = false;
            state.textContent = checked.label;
          } catch {
            if (generation === epoch && request === prepareEpoch) {
              state.textContent = unavailable + ' For an exact note with mixed line endings, use file exchange.';
            }
          }
        });
        save = button('Save retcon review', async () => {
          if (!prepared || !workbench.canWrite()) return;
          const token = prepared.request;
          clearPrepared();
          try {
            const receipt = await workbench.writePrepared(reviewSave, token, 'Retcon review save');
            if (generation === epoch) state.textContent = receipt.message + ' Reload the saved review to continue.';
          } catch {
            if (generation === epoch) state.textContent = 'The write could not be confirmed. Inspect Publication before retrying.';
          }
        });
        download = button('Download exact save request', () => {
          if (prepared) workbench.download(prepared.file, 'wedl-retcon-save.json');
        });
        save.disabled = true;
        download.disabled = true;
        article.append(row(prepare, save, download));
      }
      article.append(state);
      results.replaceChildren(article);
    }

    async function openReview(edit) {
      const generation = ++epoch;
      discardPrepared = () => {};
      const selected = review.value;
      results.replaceChildren();
      if (!selected) return;
      notice.textContent = edit ? 'Capturing current source for private decisions…' : 'Reading the immutable original analysis…';
      try {
        const value = await workbench.get(edit ? 'archive-review-edit' : 'archive-review', {review: selected});
        if (generation !== epoch) return;
        showReview(value, generation);
        notice.textContent = 'Private review loaded.';
      } catch {
        if (generation === epoch) notice.textContent = unavailable;
      }
    }

    const capture = button('Capture original analysis', async () => {
      const file = captureFile.files[0];
      const generation = ++epoch;
      discardPrepared = () => {};
      results.replaceChildren();
      if (!file || file.size > 20 * 1024) {
        notice.textContent = 'Choose a bounded native preview request. Larger requests remain available through the native workflow.';
        return;
      }
      notice.textContent = 'Checking the exact committed original and comparing its dependencies…';
      try {
        const request = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true}).decode(await file.arrayBuffer());
        const fields = {request};
        if (new URLSearchParams(fields).toString().length > 30 * 1024) throw new Error('browser request bound');
        if (generation !== epoch) return;
        const value = await workbench.get('archive-review-capture', fields);
        if (generation !== epoch) return;
        review.value = '';
        showReview(value, generation);
        notice.textContent = 'Original analysis captured. Review each impact before saving.';
      } catch {
        if (generation === epoch) notice.textContent = unavailable;
      } finally {
        if (generation === epoch) captureFile.value = '';
      }
    });

    source.addEventListener('change', loadQueue);
    review.addEventListener('change', () => openReview(false));
    captureFile.addEventListener('change', () => {
      epoch += 1;
      discardPrepared = () => {};
      results.replaceChildren();
    });
    window.addEventListener('workbench-source', () => discardPrepared());
    const start = element('details');
    start.append(element('summary', 'Start from an exact preview request'),
      control('Native retcon preview file', captureFile), capture);
    section.append(control('Saved review source', source), control('Saved review', review),
      row(button('Reload review queue', loadQueue)), notice, results, start,
      exactImport(reviewSave, 'Retcon review save', 4 * 1024 * 1024, notice));
    main.append(section);
    const sourceContext = panel('Source context', 'An unavailable comparison leaves the original analysis and its item order unchanged.');
    const originalState = status('');
    sourceContext.append(originalState);
    side.append(sourceContext);
    fill(source, [], 'Choose a source');
    fill(review, [], 'Choose a saved review');
    choices().then(({items, originalsAvailable}) => {
      if (!originalsAvailable) originalState.textContent = 'Original snapshot choices are unavailable.';
      fill(source, items, 'Choose a source');
      if (items.length) {
        source.value = items[0].token;
        return loadQueue();
      }
      notice.textContent = 'No source choices are available.';
    }).catch(() => { notice.textContent = unavailable; });
  }

  function typed(value) {
    if (value.kind === 'list' && value.items?.length) {
      const list = element('ol');
      for (const item of value.items) {
        const entry = element('li');
        entry.append(typed(item));
        list.append(entry);
      }
      return list;
    }
    return element('span', value.label);
  }

  function revisions(main) {
    const section = panel('Compare named snapshots', 'Compare three original snapshots at one exact story position.');
    const base = element('select');
    const left = element('select');
    const right = element('select');
    const moment = element('select');
    const notice = status('Choose a base, left, and right snapshot.');
    const results = element('div', undefined, 'stack');
    let baseEpoch = 0;
    let reviewEpoch = 0;

    function clear() {
      reviewEpoch += 1;
      results.replaceChildren();
      compare.disabled = !base.value || !left.value || !right.value || !moment.value;
    }

    async function loadMoments() {
      const generation = ++baseEpoch;
      fill(moment, [], 'Choose a named moment');
      clear();
      if (!base.value) return;
      notice.textContent = 'Reading the base snapshot’s named story positions…';
      try {
        const value = await workbench.get('archive-revision-moments', {source: base.value});
        if (generation !== baseEpoch) return;
        fill(moment, value.items, 'Choose a named moment');
        if (value.items.length) moment.value = value.items[0].token;
        clear();
        notice.textContent = 'Base positions loaded. Choose both comparison snapshots.';
      } catch {
        if (generation === baseEpoch) notice.textContent = unavailable;
      }
    }

    const compare = button('Compare snapshots', async () => {
      clear();
      const generation = reviewEpoch;
      notice.textContent = 'Reading all three authenticated originals at the chosen moment…';
      try {
        const value = await workbench.get('archive-revision-compare', {
          base: base.value, left: left.value, right: right.value, moment: moment.value,
        });
        if (generation !== reviewEpoch) return;
        notice.textContent = value.complete
          ? 'Native comparison complete. Download the exact comparison for full original detail.'
          : 'Comparison is incomplete.';
        if (!value.changes.length && !value.conflicts.length) {
          results.append(element('p', 'No admitted changes at this position.'));
        }
        for (const change of value.changes) {
          const article = panel(change.label);
          article.append(details([['Left', change.left], ['Right', change.right]]));
          if (change.fields.length) article.append(element('p', 'Fields: ' + change.fields.join(', ')));
          results.append(article);
        }
        for (const conflict of value.conflicts) {
          const article = panel(conflict.label + ' · ' + conflict.field);
          const comparison = element('dl');
          for (const [label, entry] of [['Base', conflict.base], ['Left', conflict.left], ['Right', conflict.right]]) {
            const value = element('dd');
            value.append(typed(entry));
            comparison.append(element('dt', label), value);
          }
          article.append(comparison);
          results.append(article);
        }
        results.append(button('Download exact comparison', () => workbench.download(value.file, 'wedl-revision-comparison.json')));
      } catch {
        if (generation === reviewEpoch) notice.textContent = unavailable;
      }
    });
    base.addEventListener('change', loadMoments);
    for (const selector of [left, right, moment]) selector.addEventListener('change', clear);
    section.append(control('Base snapshot', base), control('Left snapshot', left),
      control('Right snapshot', right), control('Named moment', moment), compare, notice, results);
    main.append(section);
    for (const selector of [base, left, right]) fill(selector, [], 'Choose an original snapshot');
    fill(moment, [], 'Choose a named moment');
    clear();
    choices().then(({items}) => {
      const originals = items.filter(item => item.label !== 'Working files');
      for (const selector of [base, left, right]) fill(selector, originals, 'Choose an original snapshot');
      if (!originals.length) notice.textContent = 'No original snapshots are available.';
    }).catch(() => { notice.textContent = unavailable; });
  }

  workbench.register('manuscripts', manuscripts);
  workbench.register('retcons', retcons);
  workbench.extend('edits', revisions);
})();
