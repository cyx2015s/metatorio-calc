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
   p.universes=[{id:7,name:'宇宙工厂',external_inputs:[],strict_source:true,strict_sink:false,mechanics:[local(101,planet('nauvis')),local(102,planet('fulgora')),{id:103,enabled:true,mechanic:{type:'rocket-export',location:planet('nauvis'),item:q('iron-plate')}}],targets:[{id:201,flow:{location:planet('nauvis'),flow:{Fluid:{name:'water',temperature:[15,100]}}},amount:1},{id:202,flow:{location:{kind:'space'},flow},amount:2}],target_expressions:[{id:301,constant:1,terms:[{id:401,flow:{location:planet('fulgora'),flow},coefficient:1},{id:402,flow:{location:{kind:'space'},flow},coefficient:2}]}]}];
   const u=p.universes[0],raw=window.__TAURI_INTERNALS__.invoke;let next=500;
   window.__universeWrites=[];window.__universeSolves=0;window.__locationIcons=[];window.__universePngs=[];
   window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
    if(command==='icon'){
     window.__locationIcons.push(args);
     const canvas=document.createElement('canvas');canvas.width=canvas.height=64;const ctx=canvas.getContext('2d');
     ctx.fillStyle=args.ty==='planet'?'#348ba8':args.name==='space-platform-starter-pack'?'#795f91':'#9b9b9b';
     ctx.fillRect(0,0,64,64);ctx.fillStyle='#fff';ctx.font='bold 14px sans-serif';ctx.fillText(args.name.slice(0,3),6,35);
     return Array.from(atob(canvas.toDataURL().split(',')[1]),c=>c.charCodeAt(0));
    }
    if(command==='save_network_png'){window.__universePngs.push(args);return `C:\\Exports\\${args.fileName}`;}
    if(command==='suggest')return [{kind:'recipe',name:'iron-plate',role:'producer'}];
    if(command==='solve_universe' ||command==='auto_plan_universe') {
     window.__universeSolves++;
     if(command==='auto_plan_universe'&&!u.mechanics.some(e=>e.id===104))u.mechanics.push(local(104,planet('nauvis')));
     await new Promise(r=>setTimeout(r,40));
     return {project:1,universe:7,status:{status:'solved',cost:12.345,mechanics:u.mechanics.map(e=>({mechanic:e.id,location:e.mechanic.location,amount:0.5,cost:4,rate:0.5})),flows:u.targets.map(t=>({flow:t.flow,amount:t.amount,rate:t.amount}))}};
    }
    if(command==='universe_preview')return {mechanics:u.mechanics.map(e=>({id:e.id,error:null,flows:e.mechanic.type==='rocket-export'?[{flow:{location:e.mechanic.location,flow},amount:-1,rate:-1},{flow:{location:{kind:'space'},flow},amount:1,rate:1}]:[{flow:{location:e.mechanic.location,flow},amount:e.id===102?-1:1,rate:1}]})),sources:[{location:planet('nauvis'),flow},{location:planet('fulgora'),flow},{location:{kind:'space'},flow}]};
    if(command==='dispatch'&&args.message.scope==='universe'){
     const message=JSON.parse(JSON.stringify(args.message)),a=message.action.action;window.__universeWrites.push(message);const created=[];
     for(const type of ['rocket-export','orbital-drop']) if(a[`add-${type}`]){const entry={id:next++,enabled:true,mechanic:{type,...a[`add-${type}`]}};created.push(entry.id);u.mechanics.push(entry);}
     if(a['add-local-mechanic']){const entry=local(next++,a['add-local-mechanic'].location);created.push(entry.id);u.mechanics.push(entry);}
     if(a['reorder-external-input']){const v=a['reorder-external-input'],i=u.external_inputs.findIndex(e=>e.id===v.input);u.external_inputs.splice(v.position,0,...u.external_inputs.splice(i,1));}
     if(a['add-external-input'])u.external_inputs.push({id:next++,...a['add-external-input']});
     if(a['set-external-input-penalty']){const v=a['set-external-input-penalty'];u.external_inputs.find(i=>i.id===v.input).penalty=v.penalty;}
     if(a['set-external-input-flow']){const v=a['set-external-input-flow'];u.external_inputs.find(i=>i.id===v.input).flow=v.flow;}
     if(a['remove-external-input'])u.external_inputs=u.external_inputs.filter(i=>i.id!==a['remove-external-input'].input);
     const find=id=>u.mechanics.find(e=>e.id===id);
     if(a['set-mechanic-enabled']){const v=a['set-mechanic-enabled'];find(v.mechanic).enabled=v.enabled;}
     if(a['set-mechanic-location']){const v=a['set-mechanic-location'];find(v.mechanic).mechanic.location=v.location;}
     if(a['clone-mechanic']){const e=structuredClone(find(a['clone-mechanic'].mechanic));e.id=next++;u.mechanics.push(e);}
     if(a['remove-mechanic'])u.mechanics=u.mechanics.filter(e=>e.id!==a['remove-mechanic'].mechanic);
     if(a.mechanic){const e=find(a.mechanic.mechanic),edit=a.mechanic.action.recipe;if(edit?.['set-recipe'])e.mechanic.mechanic.recipe=edit['set-recipe'].recipe;if(edit?.['set-machine'])e.mechanic.mechanic.machine=edit['set-machine'].machine;}
     if(a['set-transport-item']){const v=a['set-transport-item'];find(v.mechanic).mechanic.item=v.item;}
     if(a['set-target-flow']){const v=a['set-target-flow'];u.targets.find(t=>t.id===v.target).flow=v.flow;}
     if(a['set-target-amount']){const v=a['set-target-amount'];u.targets.find(t=>t.id===v.target).amount=v.amount;}
     if(a['reorder-target']){const v=a['reorder-target'],i=u.targets.findIndex(t=>t.id===v.target);u.targets.splice(v.position,0,...u.targets.splice(i,1));}
     if(a['set-target-expression-term-coefficient']){const v=a['set-target-expression-term-coefficient'];u.target_expressions.find(e=>e.id===v.expression).terms.find(t=>t.id===v.term).coefficient=v.coefficient;}
     if(a['set-target-expression-term-flow']){const v=a['set-target-expression-term-flow'];u.target_expressions.find(e=>e.id===v.expression).terms.find(t=>t.id===v.term).flow=v.flow;}
     if(a['reorder-target-expression-term']){const v=a['reorder-target-expression-term'],e=u.target_expressions.find(e=>e.id===v.expression),i=e.terms.findIndex(t=>t.id===v.term);e.terms.splice(v.position,0,...e.terms.splice(i,1));}
     if(a['add-target'])u.targets.push({id:next++,flow:a['add-target'].flow,amount:a['add-target'].amount});
     return {revision:1,changed:true,created:{mechanics:created,external_inputs:[]},universe_solve:Object.keys(a).some(k=>k.startsWith('reorder-'))?undefined:await window.__TAURI_INTERNALS__.invoke('solve_universe'),commands:[],history:{can_undo:true,can_redo:false}};
    }
    const value=await raw(command,args);
    if(command==='catalog_index') for(const name of ['nauvis','fulgora'])value.entries.push({kind:'planet',name,localized_name:name,categories:[],group:'environment',subgroup:'planet',order:name});
    return value;
   };
  });
  await page.goto(url);

  await page.getByRole('button',{name:'宇宙工厂',exact:true}).click();
  await page.locator('.mech-card').first().waitFor();
  assert.equal(await page.getByRole('button',{name:'求解',exact:true}).count(),0);
  assert.equal(await page.getByRole('tab',{name:/工厂/}).count(),1);
  assert.equal(await page.locator('.universe-page').count(),0);
  assert.equal(await page.locator('.mech-card').count(),3);
  await waitFor(async()=>(await page.locator('.cost-value').textContent())==='12.345');
  const inputsPanel=page.getByRole('region',{name:'外部输入',exact:true});
  await waitFor(async()=>(await inputsPanel.locator('.implicit').count())===3);
  assert.equal(await inputsPanel.locator('select').count(),0);
  await inputsPanel.locator('.implicit').first().getByRole('spinbutton').fill('2');
  await inputsPanel.locator('.implicit').first().getByRole('spinbutton').press('Tab');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].external_inputs.length===1));
  await waitFor(async()=>(await inputsPanel.locator('.implicit').count())===2);
  await inputsPanel.getByRole('button',{name:'外部输入地点',exact:true}).click();
  await page.getByRole('dialog',{name:'外部输入地点',exact:true}).getByRole('button',{name:'太空',exact:true}).click();
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].external_inputs[0].flow.location.kind==='space'));
  await inputsPanel.getByRole('button',{name:'移除外部输入',exact:true}).click();
  await waitFor(async()=>(await inputsPanel.locator('.implicit').count())===3);
  await inputsPanel.getByRole('button',{name:'+ 免费电力与热量',exact:true}).click();
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].external_inputs.length===2));
  assert.ok(await page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].external_inputs.every(i=>i.penalty===0&&i.flow.location.name==='nauvis')));
  assert.equal(await inputsPanel.getByRole('button',{name:'+ 免费电力与热量',exact:true}).isDisabled(),true);
  const inputRows=inputsPanel.locator('.input-row:not(.implicit)');await waitFor(async()=>(await inputRows.count())===2);await inputRows.first().focus();await page.keyboard.press('Space');await page.keyboard.press('ArrowDown');await page.keyboard.press('Space');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].external_inputs[0].flow.flow==='Heat'));
  const card=page.locator('[data-mechanic-id="101"]');
  const solvesBefore=await page.evaluate(()=>window.__universeSolves);
  await card.getByRole('switch').click();
  await waitFor(async()=>page.evaluate(()=>window.__universeWrites.some(w=>w.action.action['set-mechanic-enabled'])));
  await page.waitForTimeout(250);assert.equal(await page.evaluate(()=>window.__universeSolves),solvesBefore+1,'backend dispatch supplies result without timer-based duplicate solve');
  await card.getByRole('button',{name:'机制地点',exact:true}).click();await page.getByRole('dialog',{name:'机制地点',exact:true}).getByRole('button',{name:'fulgora',exact:true}).click();
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
  assert.ok(labels.every(s=>!s.includes('fulgora')));await waitFor(async()=>canvas.evaluate(el=>el._cyreg.cy.nodes().some(n=>String(n.style('background-width')).includes('40%'))));
  assert.ok(await page.evaluate(()=>window.__locationIcons.some(i=>i.ty==='item'&&i.name==='space-platform-starter-pack')));
  await canvas.evaluate(el=>el._cyreg.cy.nodes().filter(n=>n.data('kind')==='mechanic').first().emit('tap'));
  await page.locator('.edit-card [data-mechanic-editor]').waitFor();
  const editing=await page.locator('.edit-card [data-mechanic-editor]').getAttribute('data-mechanic-editor');
  await page.locator('.edit-card').getByRole('switch').click();
  await waitFor(async()=>(await page.locator('.edit-card [data-mechanic-editor]').getAttribute('data-mechanic-editor'))===editing);
  await page.locator('.edit-card').getByRole('button',{name:'机制地点',exact:true}).click();await page.getByRole('dialog',{name:'机制地点',exact:true}).getByRole('button',{name:'nauvis',exact:true}).click();
  await waitFor(async()=>(await page.locator('.edit-card [data-mechanic-editor]').getAttribute('data-mechanic-editor'))===editing);
  await waitFor(async()=>(await page.locator('.cost-value').textContent())==='12.345');
  if(process.env.UNIVERSE_NETWORK_SCREENSHOT)await page.screenshot({path:process.env.UNIVERSE_NETWORK_SCREENSHOT});
  async function exportUniversePng() {
    const before=await page.evaluate(()=>window.__universePngs.length);
    await page.getByRole('button',{name:'导出 PNG',exact:true}).click();
    await waitFor(async()=>await page.getByRole('button',{name:'导出 PNG',exact:true}).isEnabled());
    assert.equal(await page.locator('.network-view [role=alert]').count(),0,await page.locator('.network-view').innerText());
    await waitFor(async()=>await page.evaluate(n=>window.__universePngs.length>n,before));
    const bytes=Buffer.from(await page.evaluate(()=>window.__universePngs.at(-1).bytes.slice(0,24)));
    assert.equal(bytes.subarray(1,4).toString(),'PNG');
    return bytes;
  }
  await exportUniversePng();
  await page.getByLabel('按地点聚类').check();
  await waitFor(async()=>canvas.evaluate(el=>el._cyreg.cy.nodes('[kind="location"]').length===3));
  assert.ok(await canvas.evaluate(el=>el._cyreg.cy.nodes().filter(n=>n.data('kind')!=='location').every(n=>n.parent().length===1)));
  await exportUniversePng();
  const groupFont=await canvas.evaluate(el=>el._cyreg.cy.nodes('[kind="location"]').first().numericStyle('font-size'));
  await page.getByLabel('网络显示大小',{exact:true}).selectOption('2');
  await waitFor(async()=>await canvas.evaluate(el=>el._cyreg.cy.nodes('[kind="location"]').first().numericStyle('font-size'))===groupFont*2);
  // Export above the former arbitrary 8192-pixel side limit at original size.
  await canvas.evaluate(el=>el._cyreg.cy.nodes().filter(n=>!n.isParent()).last().position({x:9000,y:1000}));
  const originalBounds=await canvas.evaluate(el=>{const b=el._cyreg.cy.elements().boundingBox();return {w:Math.ceil(b.w),h:Math.ceil(b.h)};});
  const largePng = await exportUniversePng();
  assert.ok(originalBounds.w>8192);
  assert.equal(largePng.readUInt32BE(16),originalBounds.w);
  assert.equal(largePng.readUInt32BE(20),originalBounds.h);
  assert.doesNotMatch(await page.locator('.export-status').textContent(),/缩小/);
  // An actual renderer failure is reported and never silently resized.
  const saves=await page.evaluate(()=>window.__universePngs.length);
  await canvas.evaluate(el=>el._cyreg.cy.nodes().filter(n=>!n.isParent()).last().position({x:1000000,y:500000}));
  await page.getByRole('button',{name:'导出 PNG',exact:true}).click();
  await waitFor(async()=>await page.getByRole('button',{name:'导出 PNG',exact:true}).isEnabled());
  assert.match(await page.locator('.network-view [role=alert]').textContent(),/PNG 导出失败/);
  assert.equal(await page.evaluate(()=>window.__universePngs.length),saves);
  await page.getByRole('button',{name:'列表',exact:true}).click();
  assert.equal(await page.locator('.location-heading').count(),2);
  const originalOrder=await page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].mechanics.map(e=>e.id));
  await page.getByLabel('按地点聚类').uncheck();assert.equal(await page.locator('.location-heading').count(),0);
  assert.deepEqual(await page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].mechanics.map(e=>e.id)),originalOrder);
  for(const label of ['火箭发运','轨道投送']) {
   await page.getByRole('button',{name:`+ ${label}`,exact:true}).click();
   const dialog=page.getByRole('dialog',{name:`选择${label}物品`,exact:true});await dialog.waitFor();
   await dialog.getByLabel('搜索原型').fill('iron-plate');
   await dialog.getByRole('button',{name:'铁板',exact:true}).dblclick();
   await dialog.waitFor({state:'detached'});
  }
  assert.deepEqual(await page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].mechanics.slice(-2).map(e=>e.mechanic.type)),['rocket-export','orbital-drop']);
  assert.equal(await page.evaluate(()=>JSON.stringify(window.__layoutDocument.projects[0].factories)),before);
  await page.locator('.targets-panel .row-item').first().locator('.hover-icon').first().click({button:'right'});
  await page.getByRole('button',{name:'添加',exact:true}).click();
  await waitFor(async()=>page.evaluate(()=>window.__universeWrites.some(w=>w.action.action['add-local-mechanic'])));
  assert.equal(await page.evaluate(()=>window.__layoutDocument.projects[0].universes[0].mechanics.at(-1).mechanic.location.kind),'space');
  await page.getByRole('button',{name:'65 机制 · 混合品质',exact:true}).click();await page.getByRole('button',{name:'列表',exact:true}).click();await page.locator('.mech-card').first().waitFor();
  assert.equal(await page.getByLabel('机制地点').count(),0);
  assert.deepEqual(errors,[]);
  console.log('Universe shared workspace passed: tabs, targets/temperature/selector, expressions, scoped edits/clone/location, solved panel, network editor and ordinary factory isolation.');
 } finally {await browser?.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1});
