// Browser regression for the mechanism workspace. IPC is mocked: this tests UI
// layout/interactions, not Factorio data or the LP solver. See docs/mechanic-workspace.md.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const assert = require("node:assert/strict");
const path = require("node:path");
const { spawn } = require("node:child_process");
const appRoot = path.resolve(__dirname, "..");
const port = Number(process.env.UI_TEST_PORT || 5197);
const url = `http://127.0.0.1:${port}`;

async function seed(page, count) {
 await page.addInitScript(({count})=>{
  const q=(id,quality='normal')=>({id,quality});
  const names = {
    "iron-plate": "铁板",
    "iron-ore": "铁矿石",
    "steel-plate": "钢材",
    "assembling-machine-2": "组装机 2 型",
    "speed-module-3": "速度插件 3",
    "quality-module-3": "品质插件 3",
    beacon: "插件塔",
    water: "水",
    steam: "蒸汽",
    boiler: "锅炉",
    "nuclear-reactor": "核反应堆",
    "solar-panel": "太阳能板",
    accumulator: "蓄电池",
    coal: "煤炭",
    land: "土地",
    lava: "岩浆",
    normal: "普通",
    rare: "稀有",
    legendary: "传说",
  };
  const config={modules:[q('quality-module-3','rare'),q('quality-module-3','rare'),q('speed-module-3','legendary')],beacons:[{beacon:q('beacon','rare'),count:8,share:8,modules:[[q('speed-module-3','legendary'),16]]}]};
  const variants=[
   {type:'recipe',recipe:q('iron-plate'),machine:q('assembling-machine-2'),module_config:config},
   {type:'recipe',recipe:q('iron-plate','rare'),machine:q('assembling-machine-2'),module_config:config},
   {type:'mining',resource:'iron-ore',machine:q('assembling-machine-2'),module_config:config},
   {type:'boiler',boiler:q('boiler'),fluid:'water',temperature:15,output_temperature:165},
   {type:'reactor',reactor:q('nuclear-reactor'),neighbours:4},
   {type:'solar',solar_panel:q('solar-panel'),accumulator:q('accumulator')},
   {type:'generator',generator:q('boiler'),fluid:'steam',temperature:500},
   {type:'fluid-heat',fluid:'steam',temperature:500},
   {type:'virtual-science',item:q('iron-plate','rare')},
   {type:'spoil',item:q('iron-plate')},
   {type:'plant',seed:q('iron-plate')},
   {type:'item-fuel',item:q('coal')},
   {type:'fluid-fuel',fluid:'steam'},
   {type:'item-launch',item:q('iron-plate'),weight_mode:true},
   {type:'tile-extract',tile:'water',machine:q('assembling-machine-2')},
   {type:'tile-dispose',tile:'lava',item:q('iron-plate')}
  ];
  const mechanics=Array.from({length:count},(_,i)=>({id:i+1,enabled:true,mechanic:structuredClone(variants[i%variants.length])}));
  const project={id:1,name:'布局验证',context_id:'test',settings:{time_scale:'minutes',milestones:[],recipe_productivity:[],infinite_levels:[],ignore_productivity:false,mining_productivity:0,all_accessible:true,quality_limit:'legendary'},planning:{preserve_existing_mechanics:false,alternative_count:1,machine_preferences:[],enumerate_modules:[],enumerate_beacons:[]},factories:[{id:2,name:'65 机制 · 混合品质',settings:{planet:'nauvis',surface:null,major_quality:'rare',debug:false},targets:[{id:1,flow:{Item:q('iron-plate')},amount:1}],external_inputs:[],target_expressions:[],mechanics,strict_source:true,strict_sink:true}]};
  const doc={schema_version:1,projects:[project]};
  const ctx={id:'test',name:'vanilla-2.1',created_at:0,game_version:'2.1.22',mods:[],loaded:true,groups:[],icon_root:null,active:true};
  let callbacks=new Map(),callbackId=1; const calls=[]; window.__layoutCalls=calls; window.__layoutDocument=doc;
  const index={context_id:'test',qualities:['normal','rare','legendary'],names:{},entries:[]};
  for(const [name,localized_name] of Object.entries(names)) for(const kind of ['item','entity','recipe','machine','module','fluid','quality','beacon','mining-machine','generator','boiler','reactor','solar-panel','accumulator','tile','resource']) index.entries.push({kind,name,localized_name,group:'test',subgroup:'test',icon_type:kind==='fluid'?'fluid':'item',module_slots:3,categories:['crafting','basic-solid'],fuel_categories:[],fuel_value_j:null,technology_base_level:0,technology_max_level:null});
  for(const entry of index.entries) if(entry.kind==='tile') {entry.tile={fluid:entry.name==='water'?'water':null,destroys_dropped_items:entry.name==='lava'}; if(entry.name==='water') entry.localized_name='湖面';}
  const history={can_undo:false,can_redo:false};
  window.__TAURI_INTERNALS__={
   transformCallback(callback){const id=callbackId++;callbacks.set(id,callback);return id;},
   unregisterCallback(id){callbacks.delete(id);},
   async invoke(command,args={}) {
    calls.push({command,args});
    if(command==='plugin:event|listen') {
     if(args.event==='solve-result') setTimeout(()=>callbacks.get(args.handler)?.({payload:{project:1,factory:2,status:{solved:{cost:100,mechanics:mechanics.map((m,i)=>({mechanic:m.id,amount:(i+1)/3,cost:4,rate:1,scale:1,variant:0})),flows:[]}}}}),100);
     return callbackId++;
    }
    if(command==='plugin:event|unlisten') return;
    if(command==='plugin:app|version') return '1.5.33';
    if(command==='get_document') return structuredClone(doc);
    if(command==='history_state') return history;
    if(command==='list_contexts') return {active:'test',contexts:[ctx]};
    if(command==='catalog_index') return index;
    if(command==='icon') return null;
    if(command==='project_save_path') return null;
    if(command==='milestones_ordered' || command==='implicit_sources' || command==='accessibility') return [];
    if(command==='productivity') return {recipes:[],mining:0,auto_mining:0,infinite_levels:[],ignore:false};
    if(command==='prototype_detail') {
     const stat=(key,value,unit)=>({key,value,unit});
     const tile=args.kind==='tile'?{fluid:args.name==='water'?'water':null,destroys_dropped_items:args.name==='lava'}:undefined;
     const qualities=args.kind==='module' ? ['normal','rare','legendary'].map((quality,i)=>({quality,stats:[stat('effect-speed',0.5*(i+1),'percent'),stat('effect-consumption',0.7,'percent')]})):[];
     const product={kind:'item',name:'iron-plate',amount:2,base_rate:1,probability:0.5,productivity:1,amount_min:2,amount_max:6,quality_min:'rare',quality_max:'legendary',quality_change:1,temperature:null,min_temperature:null,max_temperature:null};
     return {kind:args.kind,name:args.name,localized_name:names[args.name]||args.name,module_slots:3,beacon_module_slots:2,uses_beacon_effects:true,machine_energy_source:'electric',boiler_mode:'heat-fluid-inside',boiler_burns_fluid:false,ingredients:[],results:args.kind==='recipe'?[product,{...product,productivity:2}]:[],categories:[],fuel_value_j:null,fuel_categories:[],rocket_launch_products:[],surface_conditions:[],calculation:{stats:args.kind==='recipe'?[stat('craft-time',2,'seconds')]:[],qualities,tile}};
    }
    if(command==='allowed_modules') return ['speed-module-3','quality-module-3'];
    if(command==='solar_balance') return doc.projects[0]?.factories[0]?.settings.surface ? {continuous_daylight:true,peak_power:180000,average_power:180000,surplus_per_cycle:0,cycle_seconds:0,recommended_accumulators:0,accumulator_capacity:0} : {continuous_daylight:false,peak_power:60000,average_power:42000,surplus_per_cycle:100,cycle_seconds:420,recommended_accumulators:0.84,accumulator_capacity:5000000};
    if(command==='mechanic_flow') {
     const output=(name,quality,n)=>[{Item:q(name,quality)},n];
     const base=[['Electricity',-24000],[{Pollution:{name:'pollution'}},0.12],output('iron-ore','normal',-2),output('iron-plate','normal',1),output('iron-plate','rare',0.1),output('iron-plate','legendary',0.01)];
     return args.mechanic===1 ? [...base,...Array.from({length:18},(_,i)=>output(`extra-${i}`,'normal',i+1))] : base;
    }
    if(command==='dispatch') {
     if(args.message.action?.['new-project']) {
      const id=Math.max(0,...doc.projects.map(p=>p.id))+10;
      const created=structuredClone(project);
      created.id=id; created.name=args.message.action['new-project'].name;
      created.factories=[{...structuredClone(project.factories[0]),id:id+1,mechanics:[],targets:[],external_inputs:[],target_expressions:[]}];
      doc.projects.push(created);
      return {revision:1,changed:true,created:{projects:[id],factories:[id+1]},commands:[],history};
     }
     const action=args.message.action?.action;
     if(action?.['mechanic-list']?.['set-enabled']) {const a=action['mechanic-list']['set-enabled']; mechanics.find(m=>m.id===a.mechanic).enabled=a.enabled;}
     if(action?.['mechanic-list']?.reorder) {const a=action['mechanic-list'].reorder; const i=mechanics.findIndex(m=>m.id===a.mechanic); mechanics.splice(a.position,0,...mechanics.splice(i,1));}
     if(action?.mechanic){ const entry=mechanics.find(m=>m.id===action.mechanic.mechanic); const a=action.mechanic.action[entry.mechanic.type]; if(a?.['set-temperature']) entry.mechanic.temperature=a['set-temperature'].temperature; }
     return {revision:1,changed:true,created:[],commands:[],history};
    }
    throw Error(`Unmocked command ${command}`);
   }
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
 },{count});

}
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function waitFor(check, attempts = 100) {
  for (let i = 0; i < attempts; i++) { if (await check()) return; await pause(50); }
  throw Error("Timed out waiting for UI state");
}
async function checkRows(page) {
  const geometry = await page.locator(".mech-card").evaluateAll((cards) => cards.map((card) => ({
    height: card.getBoundingClientRect().height,
    leftOverflow: card.querySelector(".identity").scrollWidth > card.querySelector(".identity").clientWidth,
  })));
  assert.ok(geometry.length > 0);
  assert.ok(geometry.every((card) => card.height === 72), JSON.stringify(geometry));
  assert.ok(geometry.every((card) => !card.leftOverflow), "mechanism configuration overflows its column");
  assert.equal(await page.evaluate(() => document.body.scrollWidth > innerWidth), false);
}
async function run() {
  const server = spawn(process.execPath, [path.join(appRoot, "node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: appRoot, windowsHide: true, stdio: "ignore",
  });
  let browser;
  try {
    await waitFor(async () => { try { return (await fetch(url)).ok; } catch { return false; } });
    browser = await chromium.launch({ headless: true, ...(process.env.BROWSER_EXECUTABLE ? { executablePath: process.env.BROWSER_EXECUTABLE } : {}) });
    const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.stack || error.message));
    await seed(page, 65);
    await page.goto(url);
    await page.waitForSelector('.mech-card[data-mechanic-id="1"]');
    await waitFor(async () => (await page.locator('.mech-card .amount').count()) === 65);
    await checkRows(page);
    // The same cached detail must display the quality of the hovered icon.
    await page.locator('[data-mechanic-id="1"] .config-preview .hover-icon').first().hover();
    await page.getByRole('tooltip').waitFor();
    assert.ok((await page.getByRole('tooltip').textContent()).includes('稀有基础值'));
    assert.ok((await page.getByRole('tooltip').textContent()).includes('+100%'));
    await page.getByRole('tooltip').hover();
    await pause(200);
    assert.equal(await page.getByRole('tooltip').count(),1,'tooltip can be entered for scrolling');
    await page.mouse.move(1200,80);
    await page.getByRole('tooltip').waitFor({state:'detached'});
    await page.locator('[data-mechanic-id="1"] .flow-strip .hover-icon').first().hover();
    await page.getByRole('tooltip').waitFor();
    assert.match(await page.getByRole('tooltip').textContent(),/实际消耗.*8 kW/);
    await page.mouse.move(1200,80);
    await page.getByRole('tooltip').waitFor({state:'detached'});
    // Repeated products retain their productivity split without raw probability metadata.
    await page.locator('[data-mechanic-id="1"] .primary .hover-icon').hover();
    await page.getByRole('tooltip').waitFor();
    assert.equal(await page.getByRole('tooltip').locator('.flow-row').count(),2);
    const productRows=page.getByRole('tooltip').locator('.flow-row');
    assert.equal(await productRows.nth(0).locator('.quantity strong').textContent(),'1 + 1');
    assert.equal(await productRows.nth(1).locator('.quantity strong').textContent(),'0 + 2');
    assert.doesNotMatch(await page.getByRole('tooltip').textContent(),/概率|原始|50%/);
    assert.match(await page.getByRole('tooltip').textContent(),/不受产能加成部分 \+ 可享受产能加成部分/);
    if(process.env.UI_TEST_SCREENSHOT) await page.screenshot({path:process.env.UI_TEST_SCREENSHOT});
    await page.setViewportSize({width:800,height:600});
    const bounds=await page.getByRole('tooltip').boundingBox();
    assert.ok(bounds.x>=0 && bounds.y>=0 && bounds.x+bounds.width<=800 && bounds.y+bounds.height<=600,'tooltip remains inside a resized viewport');
    await page.setViewportSize({width:1600,height:1000});
    await page.mouse.move(1200,80);
    await page.getByRole('tooltip').waitFor({state:'detached'});
    // Physical capabilities are enforced even when all prototypes are accessible.
    for(const [id,expected] of [[15,'湖面'],[16,'岩浆']]) {
      const row=page.locator(`[data-mechanic-row="${id}"]`);
      await row.scrollIntoViewIfNeeded();
      await row.locator(id===15?'.primary':'[aria-label="选择可销毁物品的地格"]').click();
      const selector=page.locator('.backdrop [aria-modal="true"]');
      await selector.waitFor();
      assert.equal(await selector.locator('.search-list > button').count(),1);
      assert.equal(await selector.locator('.search-list > button').getAttribute('aria-label'),expected);
      const calls=await page.evaluate(()=>window.__layoutCalls.filter(call=>call.command==='prototype_detail').length);
      if(id===15) {
        await selector.getByRole('textbox').fill('水');
        assert.equal(await selector.locator('.search-list > button').count(),1,'tile search includes its output fluid name');
      }
      await selector.getByRole('textbox').fill('土地');
      assert.equal(await selector.locator('.sg-items > button, .search-list > button').count(),0);
      assert.equal(await page.evaluate(()=>window.__layoutCalls.filter(call=>call.command==='prototype_detail').length),calls,'tile filtering makes no detail requests');
      await page.keyboard.press('Escape');
    }
    await page.locator('.center').evaluate(node=>node.scrollTop=0);
    assert.equal(await page.locator('.mechanic-row').count(), 65);
    const first = page.locator('[data-mechanic-id="1"]');
    assert.match(await first.locator('.config-preview').getAttribute('title'), /8 座塔（1 组配置）/);
    await first.locator('.enabled').click();
    await waitFor(async () => (await first.locator('.enabled').getAttribute('aria-checked')) === 'false');
    await first.locator('.enabled').click();
    await first.locator('.config-preview').click();
    const editor = page.getByRole('dialog', { name: '插件与插件塔', exact: true });
    await editor.waitFor();
    await editor.locator('[title="插件槽 1（点击更换）"]').click();
    await page.locator('.backdrop [aria-modal="true"]').waitFor();
    const prototypePicker=page.locator('.backdrop [aria-modal="true"]');
    await prototypePicker.locator('.q-chip[title="legendary"]').click();
    await prototypePicker.locator('.sg-items > button[aria-label="品质插件 3"]').hover();
    await page.getByRole('tooltip').waitFor();
    assert.match(await page.getByRole('tooltip').textContent(),/传说基础值/);
    assert.match(await page.getByRole('tooltip').textContent(),/\+150%/);
    assert.equal(await page.evaluate(()=>window.__layoutCalls.filter(call=>call.command==='prototype_detail' && call.args.kind==='module' && call.args.name==='quality-module-3').length),1,'quality changes reuse the same prototype detail');
    await page.keyboard.press('Escape');
    await page.locator('.backdrop [aria-modal="true"]').waitFor({state:'detached'});
    assert.equal(await editor.count(), 1, 'closing the prototype selector must retain the module editor');
    await page.keyboard.press('Escape');
    await editor.waitFor({state:'detached'});
    assert.equal(await first.locator('.config-preview').evaluate((button) => document.activeElement === button), true);
    await first.locator('.all-flows').click();
    await page.locator('.full-flow-list').waitFor();
    assert.equal(await page.locator('.full-flow-list .flow-chip').count(), 24, 'overflow flows remain accessible');
    assert.ok((await page.locator('.full-flow-list').textContent()).includes('铁矿石'));
    await page.keyboard.press('Escape');
    const boilerRow = page.locator('[data-mechanic-row="4"]');
    await boilerRow.scrollIntoViewIfNeeded();
    await boilerRow.locator('.settings-button').click();
    const boiler = page.getByRole('dialog', {name:'锅炉设置', exact:true});
    await boiler.getByLabel('温度', {exact:true}).fill('25');
    await boiler.getByRole('button', {name:'关闭', exact:true}).click();
    await waitFor(() => page.evaluate(() => window.__layoutDocument.projects[0].factories[0].mechanics.find(m=>m.id===4).mechanic.temperature===25));
    for (const width of [1920, 1024, 800]) {
      await page.setViewportSize({width, height:900});
      await checkRows(page);
    }
    await page.setViewportSize({width:1600,height:1000});
    await page.locator('.center').evaluate((node) => node.scrollTop = 0);
    // Merged groups, rather than raw mechanisms, are the actual draggable children.
    const group = page.locator('.mech-list > .mech-group').first();
    await group.focus();
    await page.keyboard.press('Space');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Space');
    await waitFor(() => page.evaluate(() => window.__layoutDocument.projects[0].factories[0].mechanics[0].id === 3));
    const ids = await page.evaluate(() => window.__layoutDocument.projects[0].factories[0].mechanics.map(m=>m.id));
    assert.equal(new Set(ids).size,65);
    assert.equal(ids.length,65);
    assert.deepEqual(errors, []);
    await page.close();

    const longPage = await browser.newPage({viewport:{width:1600,height:1000}});
    longPage.on('pageerror',error=>errors.push(error.stack || error.message));
    await seed(longPage,1000);
    await longPage.goto(url);
    await longPage.waitForSelector('.mech-card');
    await checkRows(longPage);
    assert.equal(await longPage.locator('.mechanic-row').count(),1000);
    const mounted = await longPage.locator('.mech-card').count();
    assert.ok(mounted < 35, `mounted ${mounted} cards for 1000 rows`);
    const requests = await longPage.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='mechanic_flow').length);
    assert.ok(requests < 35, `requested ${requests} mechanism expansions on the first screen`);
    await longPage.locator('[data-mechanic-id="1"] .config-preview').click();
    await longPage.getByRole('dialog',{name:'插件与插件塔',exact:true}).waitFor();
    await longPage.locator('.center').evaluate(node=>node.scrollTop=node.scrollHeight);
    await pause(150);
    assert.equal(await longPage.locator('[data-mechanic-id="1"]').count(),1,'an open editor pins its card while scrolling');
    await longPage.mouse.click(1000,75);
    await longPage.locator('.secondary-menu').waitFor({state:'detached'});
    await waitFor(async()=>await longPage.locator('[data-mechanic-id="1"]').count()===0);
    await longPage.locator('[data-mechanic-row="1000"]').scrollIntoViewIfNeeded();
    await longPage.waitForSelector('[data-mechanic-id="1000"]');
    await checkRows(longPage);
    assert.ok(await longPage.locator('.mech-card').count()<35);
    // Every heterogenous mechanism still exposes its appropriate parameters.
    for(const id of [3,4,5,6,7,8,9,10,11,12,13,14,15,16]) {
      await longPage.locator(`[data-mechanic-row="${id}"]`).scrollIntoViewIfNeeded();
      await longPage.waitForSelector(`[data-mechanic-id="${id}"]`);
    }
    await longPage.setViewportSize({width:800,height:900});
    await pause(200);
    assert.ok(await longPage.locator('.mech-card').count()<35, 'narrow workspaces retain virtual mounting');
    await longPage.setViewportSize({width:1600,height:1000});
    await pause(200);
    assert.ok(await longPage.locator('.mech-card').count()<35, 'resize restores the column scroll root');
    assert.deepEqual(errors,[]);
    // Switching the same factory into orbit updates solar details and removes storage controls.
    await longPage.locator('[data-mechanic-row="6"]').scrollIntoViewIfNeeded();
    await longPage.waitForSelector('[data-mechanic-id="6"]');
    assert.equal(await longPage.locator('[data-mechanic-id="6"] [aria-label="更换蓄电器"]').count(),1);
    await longPage.evaluate(async () => {
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      window.__layoutDocument.projects[0].factories[0].settings.surface='space-platform';
      await runtime.refresh();
    });
    await waitFor(async()=>await longPage.locator('[data-mechanic-id="6"] [aria-label="更换蓄电器"]').count()===0);
    await longPage.locator('[data-mechanic-id="6"] .settings-button').click();
    const solarDialog=longPage.getByRole('dialog',{name:'太阳能设置',exact:true});
    await solarDialog.waitFor();
    await waitFor(async()=>/持续发电.*W.*无需蓄电器/.test(await solarDialog.textContent()));
    assert.equal(await solarDialog.getByText('选择蓄电器',{exact:true}).count(),0);
    await longPage.keyboard.press('Escape');
    await solarDialog.waitFor({state:'detached'});
    await longPage.evaluate(async () => {
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      window.__layoutDocument.projects[0].factories[0].settings.surface=null;
      await runtime.refresh();
    });
    await waitFor(async()=>await longPage.locator('[data-mechanic-id="6"] [aria-label="更换蓄电器"]').count()===1);
    const selectionChecks = await longPage.evaluate(async () => {
      const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
      const doc = window.__layoutDocument;
      const original = structuredClone(doc.projects[0]);
      const other = structuredClone(original);
      other.id=10001; other.name='第二项目';
      other.factories=other.factories.map(f=>({...f,id:10002}));
      other.factories.push({...structuredClone(other.factories[0]),id:10003});
      doc.projects.push(other);
      const seen=[];
      const snapshot=()=>seen.push([runtime.selectedProjectId,runtime.selectedFactoryId]);
      await runtime.refresh(); snapshot(); // refresh preserves the current selection
      await runtime.selectProject(10001); snapshot(); // switching selects its first factory
      await runtime.selectFactory(10003);
      await runtime.selectProject(10001); snapshot(); // selecting the same tab keeps its factory
      await runtime.selectFactory(null); snapshot(); // null cannot leave a nonempty list unselected
      await runtime.selectProject(null); snapshot();
      await runtime.selectProject(10001);
      await runtime.selectFactory(10003);
      doc.projects[1].factories.pop();
      await runtime.refresh(); snapshot(); // external factory removal falls back
      doc.projects.pop();
      await runtime.refresh(); snapshot(); // external project removal falls back
      doc.projects[0].factories=[];
      await runtime.refresh(); snapshot(); // an empty project may have no factory
      doc.projects[0].factories=[structuredClone(original.factories[0])];
      await runtime.refresh(); snapshot(); // an externally added factory is selected
      doc.projects=[];
      await runtime.refresh(); snapshot(); // no projects means both ids are null
      doc.projects=[original];
      await runtime.refresh(); snapshot(); // reopen/import fills both ids
      await runtime.newProject('带默认工厂'); snapshot();
      return seen;
    });
    assert.deepEqual(selectionChecks, [[1,2],[10001,10002],[10001,10003],[10001,10002],[1,2],[10001,10002],[1,2],[1,null],[1,2],[null,null],[1,2],[11,12]]);
    assert.deepEqual(errors,[]);
    console.log(JSON.stringify({rows:1000,initialMounted:mounted,initialFlowRequests:requests,rowHeight:72,checks:'layout, nested selectors, focus, parameters, group reorder, virtual mounting, project/factory selection'},null,2));
  } finally {
    await browser?.close();
    server.kill();
  }
}
if (require.main === module) run().catch(error=>{console.error(error);process.exitCode=1});
module.exports = { seed, waitFor };
