// Shared reference rendering and structured error consumption in the actual app.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { seed, waitFor } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawn } = require('node:child_process');
const appRoot=path.resolve(__dirname,'..'),port=Number(process.env.UI_TEST_PORT||5202),url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(appRoot,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:appRoot,windowsHide:true,stdio:'ignore'});
 let browser;
 try {
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1600,height:1000}}),errors=[];
  page.on('pageerror',error=>errors.push(String(error)));
  await seed(page,16);
  await page.addInitScript(()=>{
   const callbacks=new Map(),listeners=new Map(),rawCallback=window.__TAURI_INTERNALS__.transformCallback,invoke=window.__TAURI_INTERNALS__.invoke;
   window.__TAURI_INTERNALS__.transformCallback=callback=>{const id=rawCallback(callback);callbacks.set(id,callback);return id};
   window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
    if(command==='dispatch'&&window.__rejectDiagnostic)throw window.__rejectDiagnostic;
    const result=await invoke(command,args);
    if(command==='plugin:event|listen')listeners.set(args.event,callbacks.get(args.handler));
    if(command==='catalog_index')result.names['fuel-category/chemical']='化学燃料';
    return result;
   };
   window.__emitReferenceTest=(event,payload)=>listeners.get(event)?.({payload});
   window.__referenceDiagnostic={code:'auto-plan-not-solved',message:'自动规划缺少供给',no_provider:[
    {Item:{id:'iron-plate',quality:'rare'}},
    {Fluid:{name:'steam',temperature:[165,500]}},
    {Entity:{id:'assembling-machine-2',quality:'legendary'}},
    {VirtualScience:'iron-plate'},
    {ItemFuel:{category:['chemical'],has_burnt_result:false}},
    {ItemFuelSupply:{category:['chemical'],has_burnt_result:true}},
   ],no_consumer:['Electricity','Heat',{FluidHeat:{filter:'steam'}},{FluidFuel:{filter:''}}],flows:[
    'RocketSlotCapacity','RocketWeightCapacity',{Pollution:{name:'pollution'}},
    {Custom:{name:'<b>literal-custom</b>[color=red]'}},'Unknown',
   ]};
  });
  await page.goto(url);
  await page.locator('.mech-card').first().waitFor();
  const before=await page.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='prototype_detail').length);
  await page.evaluate(()=>window.__emitReferenceTest('solve-error',window.__referenceDiagnostic));
  const diagnostic=page.locator('.solve-panel [data-diagnostic-code="auto-plan-not-solved"]');
  await diagnostic.waitFor();
  assert.equal(await diagnostic.locator('.flow-ref').count(),15);
  assert.equal(await page.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='prototype_detail').length),before,'reference lists do not eagerly request prototype details');
  const search=diagnostic.getByRole('textbox',{name:'搜索诊断对象'});
  await search.fill('165');assert.equal(await diagnostic.locator('.flow-ref').count(),1);
  assert.equal(await diagnostic.locator('.flow-ref').getAttribute('data-flow-kind'),'Fluid');
  await search.fill('ItemFuelSupply');assert.equal(await diagnostic.locator('.flow-ref').count(),1);
  await search.fill('铁板');assert.equal(await diagnostic.locator('.flow-ref').count(),2,'item and virtual science remain distinct references');
  await search.fill('');
  const tooltip=page.getByRole('tooltip');
  async function hover(kind){await diagnostic.locator(`[data-flow-kind="${kind}"] .hover-icon`).hover();await tooltip.waitFor()}
  await hover('ItemFuelSupply');assert.ok((await tooltip.textContent()).includes('有燃尽产物'));assert.ok((await tooltip.textContent()).includes('化学燃料'));
  await hover('ItemFuel');assert.ok((await tooltip.textContent()).includes('不可接收燃尽产物'));
  await hover('Fluid');assert.ok((await tooltip.textContent()).includes('165～500℃'));
  const entity=diagnostic.locator('[data-flow-kind="Entity"]');
  assert.equal(await entity.locator('.quality-corner').count(),1);
  await entity.locator('.hover-icon').focus();await tooltip.waitFor();
  await waitFor(async()=>(await tooltip.textContent()).includes('传说'));
  await hover('VirtualScience');assert.ok((await tooltip.textContent()).includes('不带品质'));
  await hover('Unknown');assert.ok((await tooltip.textContent()).includes('未指定'));
  const custom=diagnostic.locator('[data-flow-kind="Custom"] .flow-name');
  assert.equal(await custom.textContent(),'<b>literal-custom</b>[color=red]');
  assert.equal(await custom.locator('b').count(),0);
  await page.mouse.move(1200,80);await entity.locator('.hover-icon').blur();
  await page.evaluate(()=>window.__emitReferenceTest('solve-result',{project:1,factory:2,status:{'not-solved':{
   description:'普通重算失败',no_provider:window.__referenceDiagnostic.no_provider.slice(0,2),no_consumer:[],
  }}}));
  assert.equal(await page.locator('.solve-panel [data-diagnostic-code="not-solved"] .flow-ref').count(),2);
  await page.evaluate(async()=>{
   const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
   window.__rejectDiagnostic={code:'invalid-flow',message:'该流不在当前上下文中',flows:[{Item:{id:'missing-item',quality:'rare'}}],no_provider:[],no_consumer:[]};
   try {await runtime.setTargetFlow(1,window.__rejectDiagnostic.flows[0])}catch{}
  });
  const operation=page.locator('.err-strip [data-diagnostic-code="invalid-flow"]');await operation.waitFor();
  assert.equal(await operation.locator('.flow-ref').count(),1);
  assert.ok(!(await operation.textContent()).includes('[object Object]'));
  await page.evaluate(()=>window.__emitReferenceTest('command-error',[{code:'invalid-flow',message:'命令返回对象',flows:[{Fluid:{name:'steam',temperature:[165,165]}}],no_provider:[],no_consumer:[]}]));
  assert.equal(await page.locator('.err-strip [data-flow-kind="Fluid"]').count(),1);
  // Clear test errors, then verify the very same reference component in filters.
  await page.evaluate(async()=>{
   const {runtime}=await import('/src/lib/runtime/store.svelte.ts');runtime.lastError=null;window.__rejectDiagnostic=null;
  });
  await page.getByRole('button',{name:'筛选',exact:true}).click();
  const panel=page.getByRole('region',{name:'机制流筛选'});
  await waitFor(async()=>!(await panel.textContent()).includes('正在读取'));
  await panel.getByRole('button',{name:'选择产出对象'}).click();
  const option=panel.getByRole('button',{name:'铁板',exact:true});await option.waitFor();
  assert.equal(await option.locator('.flow-ref[data-wildcard="true"]').count(),1);
  await option.click();
  const chosen=panel.getByRole('button',{name:'选择产出对象'});
  assert.equal(await chosen.locator('.wildcard').count(),1);
  await chosen.locator('.hover-icon').hover();await tooltip.waitFor();
  assert.ok((await tooltip.textContent()).includes('任意品质'));
  assert.ok((await tooltip.textContent()).includes('普通品质参考值'));
  await panel.getByLabel('产出品质').selectOption('rare');
  assert.equal(await chosen.locator('.wildcard').count(),0);
  assert.equal(await chosen.locator('.quality-corner').count(),1);
  await chosen.locator('.hover-icon').hover();await tooltip.waitFor();
  assert.ok((await tooltip.textContent()).includes('稀有'));
  assert.ok(!(await tooltip.textContent()).includes('任意品质'));
  if(process.env.FLOW_REF_SCREENSHOT)await page.screenshot({path:process.env.FLOW_REF_SCREENSHOT});
  assert.deepEqual(errors,[]);
  console.log('DualVar DOM passed: all variants, semantic badges, lazy details, hover/focus, searchable diagnostics, literal custom text, structured events/dispatch and filter wildcard/exact qualities.');
 } finally {await browser?.close();server.kill()}
})().catch(error=>{console.error(error);process.exitCode=1});
