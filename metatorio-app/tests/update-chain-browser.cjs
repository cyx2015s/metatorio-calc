const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const assert=require('node:assert/strict'),path=require('node:path'),{spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),port=5223,url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(root,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,windowsHide:true,stdio:'ignore'});
 let browser;
 try {
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1600,height:1000}}),errors=[];
  page.on('pageerror',e=>errors.push(String(e)));page.setDefaultTimeout(10000);
  await seed(page,2);
  await page.addInitScript(()=>{
   const internals=window.__TAURI_INTERNALS__,raw=internals.invoke,transform=internals.transformCallback,callbacks=new Map(),events=new Map();
   internals.transformCallback=callback=>{const id=transform(callback);callbacks.set(id,callback);return id;};
   window.__updateEmit=(event,payload)=>callbacks.get(events.get(event))?.({payload});
   window.__updatePending=[];window.__updateDeferred=new Set();
   const p=window.__layoutDocument.projects[0];p.factories.push({...structuredClone(p.factories[0]),id:9,name:'second'});
   window.__layoutDocument.projects.push({...structuredClone(p),id:10,name:'other project',factories:[{...structuredClone(p.factories[0]),id:11}]});
   internals.invoke=async(command,args={})=>{
    if(command==='plugin:event|listen')events.set(args.event,args.handler);
    if(window.__updateDeferred.has(command)) {
     const value=command==='get_document'?structuredClone(window.__layoutDocument):command==='implicit_sources'&&args.factory===9?['Heat']:command==='accessibility'?[{Item:'iron-plate'}]:command==='milestones_ordered'?[{node:{Item:'iron-plate'},unlocked:true}]:await raw(command,args);
     return new Promise(resolve=>window.__updatePending.push({command,args,resolve:()=>resolve(value)}));
    }
    if(command==='productivity'&&args.project===10)return {recipes:[],mining:10,auto_mining:0,infinite_levels:[],ignore:false};
    return raw(command,args);
   };
  });
  await page.goto(url);await page.locator('[data-mechanic-id="1"]').waitFor();
  await waitFor(async()=>await page.evaluate(async()=>{const {runtime}=await import('/src/lib/runtime/store.svelte.ts');return runtime.ready&&runtime.solve!==null}));
  const run=fn=>page.evaluate(fn);
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');const factory=r.selectedFactory;await r.refresh();return factory===r.selectedFactory}),true,'unchanged snapshots preserve reactive object identity');

  // No-op selection must neither drop focus nor send a new solve request.
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');r.selectedMechanic=1;window.__callStart=window.__layoutCalls.length;await r.selectFactory(2);await r.selectProject(1);});
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.selectedMechanic}),1);
  assert.equal(await run(()=>window.__layoutCalls.slice(window.__callStart).filter(c=>c.command==='dispatch').length),0);

  // An old snapshot is deliberately held while a newer refresh is requested.
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateDeferred.add('get_document');window.__oldRefresh=r.refresh();});
  await waitFor(async()=>await run(()=>window.__updatePending.some(p=>p.command==='get_document')));
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__layoutDocument.projects[0].factories[0].mechanics[0].enabled=false;window.__updateDeferred.delete('get_document');window.__newRefresh=r.refresh();const pending=window.__updatePending.find(p=>p.command==='get_document');pending.resolve();});
  await run(async()=>{await window.__oldRefresh;await window.__newRefresh;});
  await waitFor(async()=>(await page.locator('[data-mechanic-id="1"] .enabled').getAttribute('aria-checked'))==='false');

  // Results belong to their payload factory, even when the user switched elsewhere.
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateEmit('solve-result',{project:1,factory:2,status:{solved:{cost:12,mechanics:[],flows:[]}}});});
  await waitFor(async()=>await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.solve?.status?.solved?.cost===12}));
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateEmit('solve-result',{project:1,factory:9,status:{solved:{cost:99,mechanics:[],flows:[]}}});});
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.refresh();});
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.solve.status.solved.cost}),12);
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.selectFactory(9);});
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.solve.status.solved.cost}),99);
  await run(()=>window.__updateEmit('solve-error',{project:1,factory:2,revision:0,code:'operation-failed',message:'late error',no_provider:[],no_consumer:[],flows:[]}));
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.solveError}),null);

  // A late implicit-source request for the previous factory cannot populate this one.
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateDeferred.add('implicit_sources');window.__implicitOld=r.refreshImplicitSources();});
  await waitFor(async()=>await run(()=>window.__updatePending.some(p=>p.command==='implicit_sources')));
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateDeferred.delete('implicit_sources');await r.selectFactory(2);window.__updatePending.find(p=>p.command==='implicit_sources').resolve();await window.__implicitOld;});
  assert.deepEqual(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.implicitSourcesCache}),[]);

  // A slow dispatch should not hide the already committed edit, and overlapping
  // dispatches must keep busy set until both have completed.
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateDeferred.add('dispatch');window.__editA=r.mechanicCommands().setMechanicEnabled(1,true);window.__editB=r.mechanicCommands().setMechanicEnabled(2,false);});
  await waitFor(async()=>await run(()=>window.__updatePending.filter(p=>p.command==='dispatch').length===2));
  await run(()=>window.__updateEmit('document-changed',2));
  await waitFor(async()=>(await page.locator('[data-mechanic-id="1"] .enabled').getAttribute('aria-checked'))==='true');
  await run(async()=>{window.__updatePending.filter(p=>p.command==='dispatch')[0].resolve();await window.__editA;});
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.busy}),true);
  await run(async()=>{window.__updateDeferred.delete('dispatch');window.__updatePending.filter(p=>p.command==='dispatch')[1].resolve();await window.__editB;});
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.busy}),false);
  await run(()=>window.__updateEmit('solve-result',{revision:1,project:1,factory:2,status:{solved:{cost:888,mechanics:[],flows:[]}}}));
  assert.notEqual(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.solve?.status?.solved?.cost}),888);
  // A newer edit in factory 2 must not invalidate a still-current result of 9.
  await run(()=>window.__updateEmit('solve-result',{revision:0,project:1,factory:9,status:{solved:{cost:77,mechanics:[],flows:[]}}}));
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.refresh();});
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.selectFactory(9);});
  assert.equal(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.solve.status.solved.cost}),77);
  // Project-derived responses must also stay bound to their original selection.
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');for(const command of ['productivity','milestones_ordered','accessibility'])window.__updateDeferred.add(command);r.accessibility=null;window.__oldDerived=Promise.all([r.refreshProductivity(),r.refreshOrderedMilestones(),r.ensureAccessibility()]);});
  await waitFor(async()=>await run(()=>['productivity','milestones_ordered','accessibility'].every(command=>window.__updatePending.some(p=>p.command===command))));
  await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');window.__updateDeferred.clear();await r.selectProject(10);await r.ensureAccessibility();for(const p of window.__updatePending.filter(p=>['productivity','milestones_ordered','accessibility'].includes(p.command)))p.resolve();await window.__oldDerived;});
  assert.deepEqual(await run(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return {mining:r.productivityInfo.mining,milestones:r.orderedMilestones,accessibility:r.accessibility}}),{mining:10,milestones:[],accessibility:[]});
  assert.deepEqual(errors,[]);
  console.log('Update chain passed: immediate edits, refresh ordering, scoped solve cache, stale replies, no-op selection and overlapping dispatches.');
 } finally {await browser?.close();server.kill();}
})().catch(error=>{console.error(error);process.exitCode=1;});
