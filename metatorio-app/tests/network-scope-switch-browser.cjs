const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const assert=require('node:assert/strict'),path=require('node:path'),{spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),port=5224,url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(root,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,windowsHide:true,stdio:'ignore'});
 let browser;
 try {
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1600,height:1000}}),errors=[];
  page.on('pageerror',error=>{errors.push(String(error));console.log('PAGE ERROR:',error.stack||String(error));});
  await seed(page,0);
  await page.addInitScript(()=>{
   const doc=window.__layoutDocument,raw=window.__TAURI_INTERNALS__.invoke;let next=100,revision=0;
   doc.projects[0].universes=[];
   doc.projects[0].settings.time_scale='seconds';
   window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
    if(command==='universe_preview')return {mechanics:[],sources:[]};
    if(command==='solve_universe')return {project:args.project,universe:args.universe,revision,status:{status:'solved',cost:0,mechanics:[],flows:[]},report:{}};
    if(command==='dispatch'&&args.message.action?.action){
     const {project:p,factory:f,action:a}=args.message.action,project=doc.projects.find(pj=>pj.id===p),factory=project?.factories.find(fa=>fa.id===f);
     const created={mechanics:[],universes:[],targets:[],external_inputs:[]};
     if(a['add-universe']) {const id=next++;project.universes.push({id,name:a['add-universe'].name,mechanics:[],targets:[],external_inputs:[],target_expressions:[],strict_source:true,strict_sink:false});created.universes.push(id);}
     else if(a['mechanic-list']?.add){const id=next++;factory.mechanics.push({id,enabled:true,mechanic:{type:a['mechanic-list'].add.kind,item:{id:'iron-plate',quality:'normal'}}});created.mechanics.push(id);}
     else if(a.flow?.['add-to-target']){const id=next++;factory.targets.push({id,...a.flow['add-to-target']});created.targets.push(id);}
     else if(a['external-input']?.add){const id=next++;factory.external_inputs.push({...a['external-input'].add.input,id});created.external_inputs.push(id);}
     else if(a.target?.['set-amount']){const v=a.target['set-amount'];factory.targets.find(t=>t.id===v.target).amount=v.amount;}
     else if(a['external-input']?.['set-penalty']){const v=a['external-input']['set-penalty'];factory.external_inputs.find(i=>i.id===v.input).penalty=v.penalty;}
     else return raw(command,args);
     return {revision:++revision,changed:true,created,commands:[],history:{can_undo:true,can_redo:false}};
    }
    return raw(command,args);
   };
  });
  await page.goto(url);
  await waitFor(async()=>page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.ready}));
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.newProject('new project');await r.addUniverse('new universe');});
  await page.getByRole('button',{name:'网络',exact:true}).click();
  await waitFor(async()=>await page.getByRole('region',{name:'机制网络视图'}).getAttribute('data-network-ready')==='true');
  await page.getByLabel('网络显示大小').selectOption('1.5');
  await page.getByLabel('能量与污染').check();
  await page.getByLabel('搜索网络节点').fill('universe marker');
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.selectFactory(r.selectedProject.factories[0].id);});
  await waitFor(async()=>await page.getByRole('region',{name:'机制网络视图'}).getAttribute('data-network-ready')==='true');
  assert.equal(await page.getByLabel('网络显示大小').inputValue(),'1');
  assert.equal(await page.getByLabel('搜索网络节点').inputValue(),'');
  assert.equal(await page.getByLabel('能量与污染').isChecked(),false);
  await page.getByLabel('网络显示大小').selectOption('0.75');
  await page.getByLabel('搜索网络节点').fill('ordinary marker');
  await page.getByRole('button',{name:/添加机制/}).click();
  await page.getByRole('button',{name:'物品燃料',exact:true}).click();
  await waitFor(async()=>page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.selectedFactory.mechanics.length===1}));
  const canvas=page.locator('.network-canvas');
  await waitFor(async()=>canvas.evaluate(el=>el._cyreg?.cy.nodes('[kind="mechanic"]').length===1)).catch(async error=>{
   console.log('STALE STATE',await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return {project:r.selectedProjectId,factory:r.selectedFactoryId,universe:r.selectedUniverseId,mechanics:r.selectedFactory.mechanics.length,search:document.querySelector('[aria-controls^="network-search"]')?.outerHTML,toolbar:document.querySelector('.center .toolbar')?.textContent,graph:document.querySelector('.network-status')?.textContent,nodes:document.querySelector('.network-canvas')._cyreg.cy.nodes().map(n=>({id:n.id(),kind:n.data('kind')})),calls:window.__layoutCalls.slice(-15)}}));throw error;
  });
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.addTargetFlow({Item:{id:'iron-plate',quality:'normal'}},2);});
  await waitFor(async()=>await page.locator('.targets-panel .row-item').count()===1);
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.addExternalInputFlow('Electricity',3);});
  await waitFor(async()=>await page.getByRole('region',{name:'外部输入',exact:true}).getByRole('spinbutton').count()===1);
  const targetAmount=page.locator('.targets-panel .row-item input').first();
  await targetAmount.fill('5');await targetAmount.press('Tab');
  const externalCost=page.getByRole('region',{name:'外部输入',exact:true}).getByRole('spinbutton');
  await externalCost.fill('7');await externalCost.press('Tab');
  await waitFor(async()=>page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');return r.selectedFactory.targets[0].amount===5&&r.selectedFactory.external_inputs[0].penalty===7e-6}));
  assert.equal(await page.locator('.targets-panel .row-item input').first().inputValue(),'5');
  assert.equal(await page.getByRole('region',{name:'外部输入',exact:true}).getByRole('spinbutton').inputValue(),'7');
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.selectUniverse(r.selectedProject.universes[0].id);});
  await waitFor(async()=>await page.getByRole('region',{name:'机制网络视图'}).getAttribute('data-network-ready')==='true');
  assert.equal(await page.getByLabel('网络显示大小').inputValue(),'1.5');
  assert.equal(await page.getByLabel('搜索网络节点').inputValue(),'universe marker');
  assert.equal(await page.getByLabel('能量与污染').isChecked(),true);
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.selectFactory(r.selectedProject.factories[0].id);});
  await waitFor(async()=>await page.getByRole('region',{name:'机制网络视图'}).getAttribute('data-network-ready')==='true');
  assert.equal(await page.getByLabel('网络显示大小').inputValue(),'0.75');
  assert.equal(await page.getByLabel('搜索网络节点').inputValue(),'ordinary marker');
  await page.evaluate(async()=>{const {runtime:r}=await import('/src/lib/runtime/store.svelte.ts');await r.addMechanic('item-fuel');});
  await waitFor(async()=>canvas.evaluate(el=>el._cyreg.cy.nodes('[kind="mechanic"]').length===2));
  assert.deepEqual(errors,[]);
  console.log('Network scope switch passed: immediate mechanism/target/external-input edits and independent view settings across repeated universe/factory switches.');
 }finally{await browser?.close();server.kill();}
})().catch(error=>{console.error(error);process.exitCode=1;});
