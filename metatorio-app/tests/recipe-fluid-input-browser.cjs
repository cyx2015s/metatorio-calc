const { withBrowser } = require('./browser-harness.cjs');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({ port: 5225 }, async (browser, url) => {
  const page=await browser.newPage({viewport:{width:1600,height:1000}}),errors=[];page.on('pageerror',e=>{errors.push(String(e));console.log(e.stack||String(e));});
  await seed(page,1);
  await page.addInitScript(()=>{
   const raw=window.__TAURI_INTERNALS__.invoke;window.__fluidRequired=true;
   window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
    const value=await raw(command,args);
    if(command==='catalog_index')for(const [name,localized_name,supports_fluid_input] of [['stone-furnace','石炉',false],['steel-furnace','钢炉',false],['assembling-machine-1','组装机 1',false],['blast-furnace','高炉',true]])if(!value.entries.some(entry=>entry.kind==='machine'&&entry.name===name))value.entries.push({kind:'machine',name,localized_name,supports_fluid_input,categories:['smelting'],group:'test',subgroup:'test'});
    if(command==='prototype_detail'&&args.kind==='recipe')return {...value,categories:['smelting'],ingredients:window.__fluidRequired?[{kind:'fluid',name:'water',amount:1}]:[]};
    return value;
   };
  });
  await page.goto(url);await page.locator('.machine-button').waitFor();
  await page.locator('.machine-button').click();
  const picker=page.getByRole('dialog',{name:'选择机器',exact:true});await picker.waitFor();
  assert.deepEqual(await picker.locator('.sg-items > button').evaluateAll(nodes=>nodes.map(node=>node.getAttribute('aria-label'))),['高炉']);
  await page.keyboard.press('Escape');
  await page.evaluate(async()=>{window.__fluidRequired=false;const {runtime}=await import('/src/lib/runtime/store.svelte.ts');runtime.clearCatalogCache();await runtime.loadCatalogIndex();});
  await page.locator('.machine-button').click();await picker.waitFor();
  assert.equal(await picker.locator('.sg-items > button').count(),4,'solid-only smelting still offers machines without fluid input');
  assert.deepEqual(errors,[]);
  console.log('Recipe fluid-input selector passed: incompatible machines hidden, solid-only recipe unaffected.');
}).catch(error=>{console.error(error);process.exitCode=1});
