'use strict';

(() => {
  const app = window.WedlWorkbench;
  const {element, button, panel} = app;
  const text = (value, style = 'muted') => element('p', value, style);
  const row = (...children) => { const item = element('div', null, 'row'); item.append(...children); return item; };
  const grid = (...children) => { const item = element('div', null, 'grid'); item.append(...children); return item; };
  const run = task => async () => {
    app.status('');
    try { await task(); }
    catch (error) { app.status(error instanceof Error && error.message ? error.message : 'This selection is unavailable.'); }
  };
  const action = (label, task, type = 'secondary') => button(label, run(task), type);
  const labelled = (label, input) => { const wrapper = element('label', label); wrapper.append(input); return wrapper; };
  const select = (label, values = [], blank = null) => {
    const input = element('select');
    const set = values => {
      input.replaceChildren();
      if (blank !== null) input.add(new Option(blank, ''));
      for (const item of values) input.add(new Option(item.label, item.token));
    };
    set(values);
    return {input, set, node:labelled(label,input), get value() {return input.value;}};
  };
  const fixed = (label, entries) => select(label, entries.map(([token,label])=>({label,token})));
  const entry = (label, multiline = false) => {
    const input = element(multiline ? 'textarea' : 'input');
    if (multiline) input.rows = 4;
    else input.type = 'text';
    input.autocomplete = 'off';
    return {input,node:labelled(label,input),get value(){return input.value;}};
  };
  const file = label => { const input=element('input');input.type='file';return {input,node:labelled(label,input)}; };
  function fileExchange(...children) {
    const node = element('details');
    node.append(element('summary','Native file exchange'), text('Use an exact native file for requests or operands this Workbench cannot express.'), ...children);
    return node;
  }
  function list(items, render) {
    const node = element('ul',null,'checks');
    for (const item of items) node.append(element('li',render(item)));
    return node;
  }
  function values(rows) {
    const node = element('dl');
    for (const [label,value] of rows) {
      if (value === null || value === undefined) continue;
      const item=element('div'); item.append(element('dt',label),element('dd',value));node.append(item);
    }
    return node;
  }
  function sourceCard() {
    const state = text('Source status pending.');
    const render = () => { const source = app.source(); state.textContent = source?.writes ? 'Explicit live Author is enabled.' : source ? 'Read-only Author source.' : 'Source unavailable.'; };
    render();
    window.addEventListener('workbench-source',render);
    return panel('Source and authority',state,action('Refresh source',()=>app.refresh()));
  }
  function valueNode(display) {
    if (display.kind === 'list') {
      const node=element('ol');
      for (const value of display.value) {const item=element('li');item.append(valueNode(value));node.append(item);}
      return node.childElementCount ? node : text('Empty list');
    }
    if (display.kind === 'fields') {
      const node=element('dl');
      for (const field of display.value) {const pair=element('div'), value=element('dd');value.append(valueNode(field.value));pair.append(element('dt',field.label),value);node.append(pair);}
      return node.childElementCount ? node : text('Empty field group');
    }
    const values = {missing:'Omitted',empty:'Explicitly empty',unavailable:'Open the exact native file to inspect this field.'};
    const number = display.kind === 'integer' ? 'Whole number · ' : display.kind === 'number' ? 'Decimal · ' : '';
    return element('span', values[display.kind] ?? (number + String(display.value === '' ? 'Empty text' : display.value)), 'value');
  }
  function changes(items) {
    const result=element('div',null,'stack');
    if (!items.length) result.append(text('No source changes to display.'));
    for (const item of items) {
      const group=panel(item.record,text(item.operation));
      for (const field of item.fields) {
        const before=element('div',null,'diff before'), after=element('div',null,'diff');
        before.append(element('h4','Before'),valueNode(field.before));after.append(element('h4','After'),valueNode(field.after));
        group.append(element('h4',`${field.operation} · ${field.label}`),grid(before,after));
      }
      if (!item.complete) group.append(text('The browser cannot display the complete exact change. Use native file exchange for review.','warning'));
      result.append(group);
    }
    return result;
  }
  function checks(items) {
    const output=element('ul',null,'checks');
    for (const item of items) {
      const check=element('li',`${item.policy} · ${item.label} · ${item.status}`);
      if (item.operands?.length) check.append(valueNode({kind:'fields',value:item.operands}));
      output.append(check);
    }
    return output;
  }
  const picked = input => {
    const selected=input.files?.[0];
    if (!selected) throw new Error('Choose an exact native file.');
    return selected;
  };
  const exactText = async input => new TextDecoder('utf-8',{fatal:true,ignoreBOM:true}).decode(await picked(input).arrayBuffer());

  app.register('writing',(main,side)=>{
    const work=select('Private work',[],'No private work selected');
    const moment=select('Named moment');
    const scene=select('Scene',[],'No scene selected');
    const viewpoint=select('Viewpoint',[],'No character selected');
    const brief=select('Required brief',[],'No required brief');
    const lens=fixed('Writing lens',[['author','Author'],['character','Character'],['dramatic-irony','Dramatic irony · separate views']]);
    const family=file('Optional exact family selection file');
    const conversation=select('Conversation',[],'Choose a conversation');
    const prose=entry('Spoken words',true);
    const planKind=fixed('Plan operation',[['clock','Set story clock to this moment'],['advance','Advance the selected scene'],['close','Close the selected scene'],['speech','Append a spoken beat']]);
    const operation=fixed('Native operation',[['plan','Consequence Plan'],['clock','Current time'],['scene','Scene control'],['conversation','Conversation control']]);
    const planFile=file('Exact Plan or control request');
    const frameFile=file('Exact captured frame');
    const frameResult=element('div',null,'stack');
    const planResult=element('div',null,'stack');
    const frameStatus=text('Capture source to load named choices.');
    let current=null;
    let retained=null;
    let epoch=0;
    const clearDerived=()=>{
      epoch++;
      current=null;
      app.setFrame(null);
      app.setPlan(null);
      frameResult.replaceChildren();
      planResult.replaceChildren();
      openPlan.disabled=true;
    };
    const invalidate=()=>{
      clearDerived();
      retained=null;
      if (location.hash.startsWith('#writing/')) history.replaceState(null,'',location.pathname);
      frameStatus.textContent='Selection changed. Read the writing frame again.';
    };
    const loadChoices=async()=>{
      const mine=++epoch;
      const value=await app.get('writing-choices');
      if (mine!==epoch) return false;
      work.set(value.works);moment.set(value.moments);scene.set(value.scenes);viewpoint.set(value.viewpoints);conversation.set(value.conversations);brief.set(value.briefs);
      read.disabled=!moment.value || !app.sourceReady();
      if (!retained) frameStatus.textContent='Named choices captured from the current Author source.';
      return true;
    };
    const capture=action('Capture source',async()=>{
      if (!await app.refresh(false)) throw new Error('The Author source could not be captured.');
      await loadChoices();
    });
    const showFrame=value=>{
      retained={restore:value.restore,file:value.file};
      current=value.frame;app.setFrame(value.frame);
      history.replaceState(null,'',location.pathname+`#writing/${value.restore}`);
      frameStatus.textContent='Writing frame open. Its horizon remains separate from Compendium.';
      frameResult.replaceChildren(panel('Frame summary',values([['Named moment',value.moment],['Scene',value.scene ?? 'No scene selected'],
        ['Viewpoint',value.viewpoint ?? 'No character selected'],['Private work',value.work ?? 'No private work selected']]),
        row(action('Download exact frame',()=>app.download(value.file,'wedl-writing-frame.json')))));
      for (const [key,familyKey,title] of [['characterPrompt','characterFamilies','Character prompt'],['authorMargin','authorFamilies','Private Author margin']]) {
        if (Array.isArray(value[key])) {
          const group=panel(title,value[key].length ? values(value[key].map(line=>[line.label,line.text])) : text('No additional evidence at this frame.'));
          for (const family of value[familyKey] ?? []) group.append(panel(family.label,family.entries.length ? list(family.entries,item=>item) : text('No further named evidence at this frame.')));
          frameResult.append(group);
        }
      }
      if (!value.hasContext) frameResult.append(panel('Writing context',text('Choose a scene and viewpoint to read their context.')));
      openPlan.disabled=false;
    };
    const restoreFrame=async token=>{
      const mine=++epoch;
      try {
        const value=await app.get('writing-frame',{frame:token});
        if (mine===epoch) showFrame(value);
      } catch {
        if (mine===epoch) frameStatus.textContent='The exact frame could not be revalidated. Capture source and choose a new frame.';
      }
    };
    const read=action('Read writing frame',async()=>{
      const mine=++epoch;
      const familyRaw=family.input.files?.[0] ? await exactText(family.input) : undefined;
      const value=await app.get('writing-frame',{work:work.value,moment:moment.value,scene:scene.value,viewpoint:viewpoint.value,lens:lens.value,brief:brief.value,family:familyRaw});
      if (mine===epoch) showFrame(value);
    },'primary');
    read.disabled=true;
    const showPlan=value=>planResult.replaceChildren(panel('Plan preview',value.clock ? text(`Story clock · ${value.clock}`) : text('Ordered changes from the native read-only Plan.'),
      changes(value.changes),checks(value.validation),row(action('Download exact Plan',()=>app.download(value.file,'wedl-plan.json')),
        action('Use this Plan in Actions',()=>{app.setPlan(value.file);app.show('actions');}))));
    const openPlan=action('Preview Plan',async()=>{
      const mine=++epoch;
      const value=await app.get('writing-plan',{frame:current,kind:planKind.value,conversation:conversation.value,prose:prose.value});
      if (mine===epoch) showPlan(value);
    });
    openPlan.disabled=true;
    for (const field of [work,moment,scene,viewpoint,lens,brief,family]) field.input.addEventListener('change',invalidate);
    for (const field of [planKind,conversation,prose]) field.input.addEventListener('input',()=>{epoch++;app.setPlan(null);planResult.replaceChildren();});
    window.addEventListener('workbench-source',()=>{
      clearDerived();
      read.disabled=!moment.value || !app.sourceReady();
      if (!retained) return;
      frameStatus.textContent='Source needs revalidation. Exact frame pins and Plan selections are retained.';
      const previous=retained;
      const revalidate=action('Revalidate exact frame',()=>restoreFrame(previous.restore));
      revalidate.disabled=!app.sourceReady();
      frameResult.append(panel('Stale writing frame',row(
        revalidate,
        ...(previous.file ? [action('Download exact frame',()=>app.download(previous.file,'wedl-writing-frame.json'))] : []))));
    });
    main.append(panel('Writing frame',grid(work.node,moment.node,scene.node,viewpoint.node,lens.node,brief.node),row(capture,read),frameStatus,
      fileExchange(frameFile.node,action('Open exact frame',async()=>{const mine=++epoch;const value=await app.get('writing-import',{raw:await exactText(frameFile.input)});if(mine===epoch)showFrame(value);}),family.node)),
      frameResult,panel('Plan controls',grid(planKind.node,conversation.node),prose.node,row(openPlan),fileExchange(operation.node,planFile.node,
      action('Preview exact request',async()=>{const mine=++epoch;const value=await app.get('writing-exchange',{kind:operation.value,raw:await exactText(planFile.input)});if(mine===epoch)showPlan(value);}))),planResult);
    side.append(sourceCard());
    run(async()=>{
      const saved=/^#writing\/([0-9a-f]{48})$/.exec(location.hash);
      if (location.hash && !saved && location.hash!=='#workspace') history.replaceState(null,'',location.pathname);
      if (saved) retained={restore:saved[1],file:null};
      if (await loadChoices() && saved) await restoreFrame(saved[1]);
    })();
  });

  function candidates(main, side, isAction) {
    const kind=isAction?'action':'edit';
    const saveRoute=isAction?'/api/action-drafts/save':'/api/edit-drafts/save';
    const drafts=select('Saved draft',[],'Choose a saved draft');
    const actor=select('Participant',[],'Choose a participant');
    const target=select('Record',[],'Choose a record');
    const title=entry('Draft title');
    const content=entry(isAction?'Draft intent':'Prose or field value',true);
    const editOperation=fixed('Field operation',[['omit','Leave unchanged'],['replace-prose','Replace prose'],['clear-prose','Clear prose'],['rename','Replace title'],['reorder-aliases','Set aliases in this order']]);
    const proposals=select('Saved proposal',[],'Choose a saved proposal');
    const expectation=select('Record field',[],'Choose a state field');
    const predicate=fixed('Operation',[['equals','Equals value'],['absent','Is absent']]);
    const policy=fixed('Check policy',[['required','Required'],['advisory','Advisory']]);
    const expected=entry('Expected value');
    const expectedBoolean=fixed('Expected value',[['true','True'],['false','False']]);
    const expectedHint=text('Choose a field to set its value type.');
    const expectationResult=element('div');
    const openResult=element('div',null,'stack');
    const reviewResult=element('div',null,'stack');
    const approval=element('input');approval.type='checkbox';approval.disabled=true;
    const approvalLabel=labelled('Approve this exact saved candidate',approval);approvalLabel.className='check';
    const summary=entry('New proposal summary');
    const supersedes=select('Supersede a saved proposal',[],'Keep prior proposals');
    const context=element('div',null,'stack');
    const sufficient=element('input');sufficient.type='checkbox';sufficient.disabled=true;
    const sufficiency=labelled('Confirm the exact context is sufficient for this private draft',sufficient);sufficiency.className='check';
    const imported=file('Exact private save request');
    let openDraft=null, reviewed=null, stagedContext=null, analysis=null, dirty=false, epoch=0;
    let draftListEpoch=0;
    const invalidate=()=>{
      epoch++;reviewed=null;approval.checked=false;approval.disabled=true;apply.disabled=true;
      stagedContext=null;sufficient.checked=false;sufficient.disabled=true;saveContext.disabled=true;
      context.replaceChildren();
      reviewResult.replaceChildren(text('Review the saved candidate to enable approval.'));
    };
    const controls=()=>{
      target.input.disabled=!!openDraft;
      actor.input.disabled=!!openDraft;
      title.input.readOnly=!!openDraft && !openDraft.editable;
      save.disabled=!app.canWrite() || (openDraft && !openDraft.editable);
      upload.disabled=!app.canWrite();
      apply.disabled=!app.canWrite() || !reviewed || !approval.checked;
      reviewButton.disabled=!openDraft || !openDraft.current || dirty || (isAction && !proposals.value);
      prepareReadiness.disabled=!openDraft?.canPrepareReadiness || !app.frame();
      prepareProposal.disabled=!openDraft?.canPrepareProposal || !app.frame() || !app.plan();
      saveContext.disabled=!app.canWrite() || !stagedContext || !sufficient.checked;
    };
    const readChoices=async()=>{
      const mine=++draftListEpoch;
      const value=await app.get(`${kind}-list`);
      return mine===draftListEpoch ? value : null;
    };
    const load=async()=>{
      const value=await readChoices();
      if (!value) return;
      content.input.value='';
      invalidate();openDraft=null;dirty=false;openResult.replaceChildren();content.input.readOnly=false;
      proposals.set([]);supersedes.set([]);expectation.set([]);expectationResult.replaceChildren();analysis=null;
      drafts.set(value.drafts);target.set(value.targets);actor.set(value.actors);controls();
    };
    const refreshSavedChoices=async()=>{
      const value=await readChoices();
      if (!value) return;
      const selected=drafts.input.selectedOptions[0];
      drafts.set(value.drafts);
      // Keep the editor bound to its original exact token until the user
      // deliberately picks and opens a newly listed draft.
      if (selected?.value) {
        if (!selected.disabled) selected.textContent=`Previous selection · ${selected.textContent}`;
        selected.disabled=true;
        drafts.input.add(selected,1);
        selected.selected=true;
      }
    };
    const open=action('Reload saved draft',async()=>{
      invalidate();const mine=epoch;
      const value=await app.get(`${kind}-open`,{draft:drafts.value});
      if (mine!==epoch) return;
      openDraft=value;
      dirty=false;
      content.input.value=isAction?value.intent:value.body;
      content.input.readOnly=!value.editable;
      title.input.value=value.title;
      openResult.replaceChildren(panel(isAction?'Draft intent':'Retained edit',values([['Title',value.title],['State',value.phase],
        ['Source',value.current?'Current':'Stale or historical'],['Record',value.record],['Participants',value.participants?.join(', ')],['Targets',value.targets?.join(', ')]]),
        row(action('Download exact saved file',()=>app.download(value.native,`wedl-${kind}-draft.json`)))));
      if (isAction) {
        proposals.set(value.proposals.filter(item=>item.active));
        supersedes.set(value.proposals.filter(item=>item.active));
        expectation.set(value.fields);
        analysis=null;expectationResult.replaceChildren();updateExpected();
        openResult.append(panel('Proposal history',value.proposals.length?list(value.proposals,item=>`${item.label} · ${item.active?'Active':'Superseded'}`):text('No saved proposals.')));
        openResult.append(panel('Readiness questions',value.questions.length?list(value.questions,item=>`${item.text} · ${item.required?'Required':'Optional'} · ${item.answered?'Answered':'Unanswered'}`):text('No saved readiness questions.')));
      }
      controls();
    });
    const save=action('Save private draft',async()=>{
      const draft=openDraft?drafts.value:null;
      invalidate();const mine=epoch;
      const params=isAction?{draft,title:title.value,intent:content.value,actor:actor.value,target:target.value,frame:app.frame()}
        :{draft,target:target.value,operation:editOperation.value,text:content.value};
      const request=await app.get(`${kind}-save`,params);
      const raw=await app.nativeRequest(saveRoute,request.request);
      if (mine!==epoch) return;
      const receipt=await app.write(saveRoute,raw,request.label,request.request);
      if (receipt.state==='confirmed') await load();
      app.show('publication');
    },'primary');
    const upload=action('Submit exact private save file',async()=>{
      invalidate();
      const exact=new Uint8Array(await picked(imported.input).arrayBuffer());
      await app.write(saveRoute,exact,isAction?'Save private action':'Save retained edit');
      app.show('publication');
    });
    const reviewButton=action('Review saved candidate',async()=>{
      invalidate();const mine=epoch;
      const result=await app.get(`${kind}-review`,{draft:drafts.value,proposal:isAction?proposals.value:null});
      if (mine!==epoch) return;
      reviewResult.replaceChildren(text(result.complete?'Complete exact candidate':'Incomplete presentation · use native file exchange',result.complete?'tag':'warning'),
        changes(result.changes),checks(result.validation),row(action('Download exact native review',()=>app.download(result.file,'wedl-outcome-review.json'))));
      if (result.evidence) reviewResult.append(panel('Original evidence',result.evidence.length?list(result.evidence,item=>item.label):text('No original evidence citations.')));
      reviewed=result.canApply?result.approval:null;
      approval.disabled=!reviewed || !app.canWrite();
      controls();
    });
    const apply=action('Apply approved change',async()=>{
      if (!approval.checked || !reviewed || !app.canWrite()) return;
      const ticket=reviewed;
      const request=await app.get('approve',{candidate:ticket,approved:'1'});
      const raw=await app.nativeRequest(request.route,request.request);
      if (reviewed!==ticket || !approval.checked) return;
      invalidate();
      await app.write(request.route,raw,isAction?'Apply approved action outcome':'Apply approved edit',request.request);
      app.show('publication');
    },'primary');
    const check=action('Check expectation',async()=>{
      invalidate(); const mine=epoch;
      const operand=expectedType()==='boolean'?expectedBoolean.value:expected.value;
      const result=await app.get('action-expectation',{draft:drafts.value,field:expectation.value,operation:predicate.value,value:operand,policy:policy.value});
      if (mine!==epoch) return;
      analysis=result.analysis;
      expectationResult.replaceChildren(checks(result.validation),text('This check does not change the saved proposal or grant approval.'),
        row(action('Download exact expectation check',()=>app.download(result.file,'wedl-expectation-check.json'))));
    });
    const expectedType=()=>openDraft?.fields?.find(item=>item.token===expectation.value)?.valueType ?? 'text';
    const updateExpected=()=>{
      const type=expectedType(), absent=predicate.value==='absent';
      expected.node.hidden=absent || type==='boolean';expectedBoolean.node.hidden=absent || type!=='boolean';
      expected.input.inputMode=['integer','number'].includes(type)?'decimal':'text';
      expectedHint.textContent=absent?'This expectation checks for an omitted field.':
        type==='integer'?'Enter the exact whole number.':type==='number'?'Enter the exact decimal number.':type==='boolean'?'Choose true or false.':'Enter the exact text value.';
    };
    const stageContext=stage=>async()=>{
      invalidate();const mine=epoch;
      const result=await app.get('action-prepare-context',{draft:drafts.value,frame:app.frame(),stage,
        plan:stage==='proposal'?app.plan():null,analysis:stage==='proposal'?analysis:null,summary:stage==='proposal'?summary.value:null,
        supersedes:stage==='proposal'?supersedes.value:null});
      if (mine!==epoch) return;
      stagedContext=result.candidate;
      sufficient.disabled=!stagedContext || !app.canWrite();
      context.replaceChildren(panel(result.label,text(result.requiredAnswered?'Required questions are answered.':'Required questions remain unanswered. This is not publication approval.','muted'),
        changes(result.changes),row(action('Download exact context review',()=>app.download(result.file,'wedl-action-context-review.json')))));
      for (const [key,label] of [['characterPrompt','Character prompt'],['authorMargin','Private Author margin']]) {
        if (Array.isArray(result[key])) context.append(panel(label,values(result[key].map(line=>[line.label,line.text]))));
      }
      controls();
    };
    const prepareReadiness=action('Review readiness',stageContext('readiness'));
    const prepareProposal=action('Review alternative from selected Plan',stageContext('proposal'));
    const saveContext=action('Save reviewed private draft',async()=>{
      if (!stagedContext || !sufficient.checked || !app.canWrite()) return;
      const selected=stagedContext;
      const result=await app.get('action-confirm-context',{candidate:selected,sufficient:'1'});
      const raw=await app.nativeRequest(saveRoute,result.request);
      if (selected!==stagedContext || !sufficient.checked) return;
      invalidate();
      const receipt=await app.write(saveRoute,raw,result.label,result.request);
      if (receipt.state==='confirmed') await load();
      app.show('publication');
    });
    const start=action('New draft',()=>{invalidate();openDraft=null;dirty=false;drafts.input.value='';content.input.value='';content.input.readOnly=false;title.input.value='';openResult.replaceChildren();controls();});
    for (const field of [drafts,actor,target,title,content,editOperation,proposals,expectation,predicate,expected,expectedBoolean,policy,imported,summary,supersedes]) field.input.addEventListener('input',()=>{invalidate();controls();});
    for (const field of [title,content,editOperation,expectation,predicate,expected,expectedBoolean,policy,imported,summary,supersedes]) field.input.addEventListener('input',()=>{
      if (openDraft) {dirty=true;reviewResult.replaceChildren(text('Save or reload the exact draft before reviewing it again.'));controls();}
    });
    for (const field of [expectation,predicate,expected,expectedBoolean,policy]) field.input.addEventListener('input',()=>{analysis=null;expectationResult.replaceChildren();updateExpected();});
    drafts.input.addEventListener('change',()=>{drafts.input.querySelector('option:disabled')?.remove();openDraft=null;dirty=false;analysis=null;proposals.set([]);supersedes.set([]);expectation.set([]);expectationResult.replaceChildren();openResult.replaceChildren();content.input.value='';content.input.readOnly=false;controls();});
    approval.addEventListener('change',controls);
    sufficient.addEventListener('change',controls);
    window.addEventListener('workbench-authority',controls);
    window.addEventListener('workbench-source',()=>{analysis=null;invalidate();controls();});
    window.addEventListener('workbench-recovered-write',event=>{if(event.detail===saveRoute) run(refreshSavedChoices)();});
    window.addEventListener('workbench-plan',()=>{invalidate();controls();});
    window.addEventListener('workbench-frame',()=>{invalidate();controls();});
    main.append(panel(isAction?'Draft intent':'Retained edit',grid(drafts.node,target.node,isAction?actor.node:editOperation.node,...(isAction?[title.node]:[])),content.node,
      row(start,open,save,action('Refresh saved draft choices',refreshSavedChoices)),fileExchange(imported.node,upload)),openResult);
    if (isAction) {
      main.append(panel('Expectation',grid(expectation.node,predicate.node,policy.node,expected.node,expectedBoolean.node),expectedHint,row(check),expectationResult),
        panel('Readiness and proposal',text('Read a named Writing frame first. A proposal uses the exact Plan selected in Writing & Plan.'),
          grid(summary.node,supersedes.node),row(prepareReadiness,prepareProposal),context,sufficiency,row(saveContext)));
      updateExpected();
    }
    side.append(panel('Saved candidate review',...(isAction?[proposals.node]:[]),row(reviewButton),reviewResult,approvalLabel,row(apply)),sourceCard());
    invalidate();controls();run(load)();
  }
  app.register('actions',(main,side)=>candidates(main,side,true));
  app.register('edits',(main,side)=>candidates(main,side,false));
})();
