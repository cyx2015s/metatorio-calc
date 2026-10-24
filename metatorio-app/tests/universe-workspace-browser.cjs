const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const assert=require('node:assert/strict');
const path=require('node:path');
const {spawn}=require('node:child_process');
const appRoot=path.resolve(__dirname,'..'),port=5214,url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(appRoot,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:appRoot,windowsHide:true,stdio:'ignore'});
 let browser;
 try {
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1600,height:1000}});page.setDefaultTimeout(15000);
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await seed(page,5);
  await page.addInitScript(()=>{
   const doc=window.__layoutDocument,p=doc.projects[0],planet=name=>({kind:'planet',name}),q=id=>({id,quality:'normal'}),flow={Item:q('iron-plate')};
   const local=(id,location)=>({id,enabled:true,mechanic:{type:'local',location,mechanic:structuredClone(p.factories[0].mechanics[0].mechanic)}});
   p.universes=[{id:7,name:'宇宙工厂',strict_source:true,strict_sink:false,mechanics:[local(101,planet('nauvis')),local(102,planet('fulgora')),{id:103,enabled:true,mechanic:{type:'transfer',location:planet('nauvis'),item:q('iron-plate')}}],targets:[{id:201,flow:{location:planet('nauvis'),flow:{Fluid:{name:'water',temperature:[15,100]}}},amount:1},{id:202,flow:{location:{kind:'space'},flow},amount:2}],target_expressions:[{id:301,constant:1,terms:[{id:401,flow:{location:planet('fulgora'),flow},coefficient:1},{id:402,flow:{location:{kind:'space'},flow},coefficient:2}]}]}];
   const u=p.universes[0],raw=window.__TAURI_INTERNALS__.invoke;let next=500;
   window.__universeWrites=[];
   window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
    if(command==='solve_universe'||command==='auto_plan_universe') {
     if(command==='auto_plan_universe'&&!u.mechanics.some(e=>e.id===104))u.mechanics.push(local(104,planet('nauvis')));
     await new Promise(r=>setTimeout(r,40));
     return {project:1,universe:7,status:{status:'solved',cost:12.345,mechanics:u.mechanics.map(e=>({mechanic:e.id,location:e.mechanic.location,amount:0.5,cost:4,rate:0.5})),flows:u.targets.map(t=>({flow:t.flow,amount:t.amount,rate:t.amount}))}};
    }
    if(command==='universe_preview')return {mechanics:u.mechanics.map(e=>({id:e.id,error:null,flows:e.mechanic.type==='transfer'?[{flow:{location:e.mechanic.location,flow},amount:-1,rate:-1},{flow:{location:{kind:'space'},flow},amount:1,rate:1}]:[{flow:{location:e.mechanic.location,flow},amount:e.id===102?-1:1,rate:1}]})),sources:[]};
    if(command==='dispatch'&&args.message.scope==='universe'){
     const message=JSON.parse(JSON.stringify(args.message)),a=message.action.action;window.__universeWrites.push(message);
     const find=id=>u.mechanics.find(e=>e.id===id);
     if(a['set-mechanic-enabled']){const v=a['set-mechanic-enabled'];find(v.mechanic).enabled=v.enabled;}
     if(a['set-mechanic-location']){const v=a['set-mechanic-location'];find(v.mechanic).mechanic.location=v.location;}
     if(a['clone-mechanic']){const e=structuredClone(find(a['clone-mechanic'].mechanic));e.id=next++;u.mechanics.push(e);}
     if(a['remove-mechanic'])u.mechanics=u.mechanics.filter(e=>e.id!==a['remove-mechanic'].mechanic);
     if(a.mechanic){const e=find(a.mechanic.mechanic),edit=a.mechanic.action.recipe;if(edit?.['set-recipe'])e.mechanic.mechanic.recipe=edit['set-recipe'].recipe;if(edit?.['set-machine'])e.mechanic.mechanic.machine=edit['set-machine'].machine;}
     if(a['set-transfer-item']){const v=a['set-transfer-item'];find(v.mechanic).mechanic.item=v.item;}
     if(a['set-target-flow']){const v=a['set-target-flow'];u.targets.find(t=>t.id===v.target).flow=v.flow;}
     if(a['set-target-amount']){const v=a['set-target-amount'];u.targets.find(t=>t.id===v.target).amount=v.amount;}
     if(a['reorder-target']){const v=a['reorder-target'],i=u.targets.findIndex(t=>t.id===v.target);u.targets.splice(v.position,0,...u.targets.splice(i,1));}
     if(a['set-target-expression-term-coefficient']){const v=a['set-target-expression-term-coefficient'];u.target_expressions.find(e=>e.id===v.expression).terms.find(t=>t.id===v.term).coefficient=v.coefficient;}
     if(a['set-target-expression-term-flow']){const v=a['set-target-expression-term-flow'];u.target_expressions.find(e=>e.id===v.expression).terms.find(t=>t.id===v.term).flow=v.flow;}
     if(a['reorder-target-expression-term']){const v=a['reorder-target-expression-term'],e=u.target_expressions.find(e=>e.id===v.expression),i=e.terms.findIndex(t=>t.id===v.term);e.terms.splice(v.position,0,...e.terms.splice(i,1));}
     if(a['add-target'])u.targets.push({id:next++,flow:a['add-target'].flow,amount:a['add-target'].amount});
     return {revision:1,changed:true,created:{},commands:[],history:{can_undo:true,can_redo:false}};
    }
    const value=await raw(command,args);
    if(command==='catalog_index') for(const name of ['nauvis','fulgora'])value.entries.push({kind:'planet',name,localized_name:name,categories:[],group:'environment',subgroup:'planet',order:name});
    return value;
   };
  });
  await page.goto(url);

  await page.getByRole('button',{name:'宇宙工厂',exact:true}).click();
  await page.locator('.mech-card').first().waitFor();
  assert.equal(await page.getByRole('tab',{name:/工厂/}).count(),1);
  assert.equal(await page.locator('.universe-page').count(),0);
  assert.equal(await page.locator('.mech-card').count(),3);
  await waitFor(async()=>(await page.locator('.cost-value').textContent())==='12.345');
  const card=page.locator('[data-mechanic-id="101"]');
  await card.getByRole('switch').click();
  await waitFor(async()=>page.evaluate(()=>window.__universeWrites.some(w=>w.action.action['set-mechanic-enabled'])));
  await card.getByLabel('机制地点').selectOption('fulgora');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].mechanics[0].mechanic.location.name==='fulgora'));
  await page.mouse.move(1200,80);await page.getByRole('tooltip').waitFor({state:'detached'});
  await card.getByLabel('克隆机制').click();
  await waitFor(async()=>(await page.locator('.mech-card').count())===4);
  const before=await page.evaluate(()=>JSON.stringify(window.__layoutDocument.projects[0].factories));
  await page.getByLabel('期望温度下限（℃）').fill('120');await page.getByLabel('期望温度下限（℃）').press('Tab');
  await waitFor(async()=>page.evaluate(()=>JSON.stringify(window.__layoutDocument.projects[0].universes[0].targets[0].flow.flow.Fluid.temperature)==='[100,120]'));
  await page.locator('.targets-panel .rows > .row-item').first().focus();await page.keyboard.press('Space');await page.keyboard.press('ArrowDown');await page.keyboard.press('Space');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].targets[0].id===202));
  await page.locator('.targets-panel .flow-ref').first().locator('.hover-icon').click();
  await page.getByRole('textbox',{name:'搜索原型'}).waitFor();await page.keyboard.press('Escape');
  await page.mouse.move(1200,80);await page.getByRole('tooltip').waitFor({state:'detached'});
  await page.getByRole('button',{name:/目标表达式/}).click();
  await page.getByLabel('表达式系数').first().fill('3');await page.getByLabel('表达式系数').first().press('Tab');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].target_expressions[0].terms[0].coefficient===3));
  await page.locator('.expr-term').first().focus();await page.keyboard.press('Space');await page.keyboard.press('ArrowDown');await page.keyboard.press('Space');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].target_expressions[0].terms[0].id===402));
  await page.getByRole('tab',{name:/项目/}).click();await page.getByLabel('时间刻度').waitFor();await page.getByRole('tab',{name:/工厂/}).click();
  await page.getByRole('button',{name:'自动规划',exact:true}).click();
  await waitFor(async()=>(await page.locator('.mech-card').count())===5);
  await waitFor(async()=>(await page.locator('.cost-value').textContent())==='12.345');
  if(process.env.UNIVERSE_LIST_SCREENSHOT)await page.screenshot({path:process.env.UNIVERSE_LIST_SCREENSHOT});
  await page.getByRole('button',{name:'网络',exact:true}).click();
  const canvas=page.getByRole('group',{name:'生产网络画布'});await canvas.waitFor();
  await waitFor(async()=>canvas.evaluate(el=>!!el._cyreg?.cy&&el._cyreg.cy.nodes().length>=5),600);
  const labels=await canvas.evaluate(el=>el._cyreg.cy.nodes().map(n=>n.data('label')));
  assert.ok(labels.some(s=>s.includes('fulgora')));assert.ok(labels.some(s=>s.includes('太空')));
  await canvas.evaluate(el=>el._cyreg.cy.nodes().filter(n=>n.data('kind')==='mechanic').first().emit('tap'));
  await page.locator('.edit-card [data-mechanic-editor]').waitFor();
  const editing=await page.locator('.edit-card [data-mechanic-editor]').getAttribute('data-mechanic-editor');
  await page.locator('.edit-card').getByRole('switch').click();
  await waitFor(async()=>(await page.locator('.edit-card [data-mechanic-editor]').getAttribute('data-mechanic-editor'))===editing);
  await page.locator('.edit-card').getByLabel('机制地点').selectOption('nauvis');
  await waitFor(async()=>(await page.locator('.edit-card [data-mechanic-editor]').getAttribute('data-mechanic-editor'))===editing);
  await waitFor(async()=>(await page.locator('.cost-value').textContent())==='12.345');
  if(process.env.UNIVERSE_NETWORK_SCREENSHOT)await page.screenshot({path:process.env.UNIVERSE_NETWORK_SCREENSHOT});
  assert.equal(await page.evaluate(()=>JSON.stringify(window.__layoutDocument.projects[0].factories)),before);
  await page.getByRole('button',{name:'65 机制 · 混合品质',exact:true}).click();await page.getByRole('button',{name:'列表',exact:true}).click();await page.locator('.mech-card').first().waitFor();
  assert.equal(await page.getByLabel('机制地点').count(),0);
  assert.deepEqual(errors,[]);
  console.log('Universe shared workspace passed: tabs, targets/temperature/selector, expressions, scoped edits/clone/location, solved panel, network editor and ordinary factory isolation.');
 } finally {await browser?.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1});
