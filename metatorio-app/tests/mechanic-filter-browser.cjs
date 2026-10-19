const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { seed, waitFor } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawn } = require('node:child_process');
const appRoot = path.resolve(__dirname, '..');
const port = Number(process.env.UI_TEST_PORT || 5201), url = `http://127.0.0.1:${port}`;
(async () => {
  const server = spawn(process.execPath, [path.join(appRoot,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'], {cwd:appRoot,windowsHide:true,stdio:'ignore'});
  let browser;
  try {
    await waitFor(async()=>{try{return (await fetch(url)).ok}catch{return false}},600);
    browser = await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
    const page = await browser.newPage({viewport:{width:1600,height:1100}}), errors=[];
    page.on('pageerror',error=>errors.push(String(error)));
    await seed(page,96);
    await page.addInitScript(()=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      let failed=false;
      window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
        const result=await invoke(command,args);
        if(command!=='mechanic_flow')return result;
        await new Promise(resolve=>setTimeout(resolve,12));
        if(args.mechanic===95&&!failed){failed=true;throw Error('retryable expansion error')}
        const item=(id,quality='normal')=>({Item:{id,quality}}), water=t=>({Fluid:{name:'water',temperature:[t,t]}});
        switch(args.mechanic){
          case 1:return [[item('iron-plate'),1],[water(15),-2],['Electricity',-24000]];
          case 2:return [[item('iron-plate','rare'),1],[water(15),-2],[item('steel-plate'),0]];
          case 3:return [[item('iron-plate','legendary'),-1],[item('steel-plate'),1]];
          case 4:return [[item('iron-plate','rare'),1],[{Fluid:{name:'steam',temperature:[165,165]}},1]];
          case 5:return [['Heat',100]];
          case 96:return [[item(window.__filterOrbit?'steel-plate':'iron-plate',window.__changed96?'rare':'legendary'),1],[water(100),-2]];
          default:return [[item('coal'),1],['Electricity',-10]];
        }
      };
    });
    await page.goto(url);
    await page.locator('.mech-card').first().waitFor();
    await waitFor(async()=>await page.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='mechanic_flow').length>0));
    assert.ok(await page.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='mechanic_flow').length)<96,'unused feature preserves virtual lazy expansion');
    await page.getByRole('button',{name:'筛选',exact:true}).click();
    const panel=page.getByRole('region',{name:'机制流筛选'});
    await panel.getByText('1 个机制读取失败，尚未判定').waitFor();
    await panel.getByRole('button',{name:'重试',exact:true}).click();
    await waitFor(async()=>!(await panel.textContent()).includes('正在读取')&&!(await panel.textContent()).includes('读取失败'));
    const calls=await page.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='mechanic_flow'));
    assert.equal(calls.length,97,'each expansion is cached, with one failed request retried');
    assert.equal(new Set(calls.map(c=>c.args.mechanic)).size,96,'offscreen mechanisms are indexed');
    const rows=page.locator('.mech-list [data-mechanic-row]');
    const rowIds=async()=>rows.evaluateAll(nodes=>nodes.map(n=>Number(n.dataset.mechanicRow)).sort((a,b)=>a-b));
    const expectIds=async(ids)=>{
      try { await waitFor(async()=>JSON.stringify(await rowIds())===JSON.stringify(ids)); }
      catch { throw Error(JSON.stringify({expected:ids,actual:await rowIds(),errors,selection:await page.evaluate(async()=>{
        const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
        return {factory:runtime.selectedFactoryId,factories:runtime.document.projects[0].factories.map(f=>({id:f.id,count:f.mechanics.length}))};
      })})); }
    };
    async function choose(side,name){
      await panel.getByRole('button',{name:`选择${side}对象`,exact:true}).click();
      await panel.getByRole('textbox',{name:'搜索筛选对象'}).fill(name);
      await panel.locator('.flow-option').getByText(name,{exact:true}).click();
    }
    await choose('产出','铁板');
    await expectIds([1,2,4,96]);
    assert.equal(await panel.getByLabel('产出品质').inputValue(),'*');
    assert.equal(await page.locator('.mech-group').count(),1,'only matching recipe variants are merged');
    assert.equal(await page.locator('.mech-group [data-mechanic-row]').count(),2);
    await panel.getByLabel('产出品质').selectOption('rare');
    await expectIds([2,4]);
    await choose('消耗','水');
    await expectIds([2]);
    await panel.getByLabel('产出品质').selectOption('*');
    await expectIds([1,2,96]);
    await panel.getByLabel('消耗温度').selectOption('[100,100]');
    await expectIds([96]);
    await panel.getByRole('button',{name:'清除消耗筛选',exact:true}).click();
    await expectIds([1,2,4,96]);
    await page.getByRole('button',{name:'更多机制操作',exact:true}).click();
    await page.getByRole('button',{name:'配方合并',exact:false}).click();
    await page.locator('.menu-catcher').click({position:{x:2,y:2}});
    assert.equal(await page.locator('.mech-group').count(),0);
    await expectIds([1,2,4,96]);
    await page.getByRole('button',{name:'筛选 · 已启用',exact:true}).click();
    assert.equal(await panel.getByRole('button',{name:'选择产出对象'}).count(),0);
    assert.ok((await panel.textContent()).includes('显示 4 / 96'));
    await page.getByRole('button',{name:'筛选 · 已启用',exact:true}).click();
    await panel.getByLabel('产出品质').selectOption('legendary');
    await expectIds([96]);
    await page.evaluate(async()=>{
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      window.__changed96=true;
      runtime.document.projects[0].factories[0].mechanics.find(e=>e.id===96).mechanic.item.quality='rare';
    });
    await expectIds([]);
    await panel.getByLabel('产出品质').selectOption('rare');
    await expectIds([2,4,96]);
    await page.evaluate(async()=>{
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      window.__filterOrbit=true;
      runtime.document.projects[0].factories[0].settings.surface='space-platform';
    });
    await expectIds([2,4]);
    await waitFor(async()=>!(await panel.textContent()).includes('正在读取'));
    assert.equal(await page.evaluate(()=>window.__layoutCalls.filter(c=>c.command==='dispatch').length),0,'filtering does not edit the factory');
    if(process.env.FILTER_SCREENSHOT)await page.screenshot({path:process.env.FILTER_SCREENSHOT});
    await panel.getByRole('button',{name:'清除筛选',exact:true}).click();
    await expectIds(Array.from({length:96},(_,i)=>i+1));
    assert.equal(await page.getByText('筛选中暂停手动拖拽排序，清除筛选后恢复。').count(),0);
    assert.deepEqual(await rows.evaluateAll(nodes=>nodes.map(n=>Number(n.dataset.mechanicRow))),Array.from({length:96},(_,i)=>i+1),'hidden entries retain original order');
    // Disabled mechanisms still match by their expansion, not solved quantity.
    await choose('产出','铁板');
    await page.evaluate(async()=>{
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      runtime.document.projects[0].factories[0].mechanics.find(e=>e.id===2).enabled=false;
    });
    await expectIds([1,2,4]);
    await page.setViewportSize({width:800,height:700});
    assert.ok(await panel.evaluate(node=>node.getBoundingClientRect().right<=window.innerWidth),'filter fields wrap within a narrow viewport');
    await page.evaluate(async()=>{
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      const factory=JSON.parse(JSON.stringify(runtime.document.projects[0].factories[0]));
      factory.id=99; factory.name='另一个工厂'; factory.mechanics=factory.mechanics.slice(0,2);
      // Switching to an uncached factory recomputes and refreshes the document;
      // keep the mock backend snapshot consistent with the new factory too.
      window.__layoutDocument.projects[0].factories.push(structuredClone(factory));
      runtime.document.projects[0].factories.push(factory);
      await runtime.selectFactory(99);
    });
    await expectIds([1,2]);
    assert.equal(await panel.getByRole('button',{name:'选择产出对象'}).textContent(),'不限 · 选择对象','new factory starts without inherited criteria');
    await page.evaluate(async()=>{
      const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
      await runtime.selectFactory(2);
    });
    await expectIds(Array.from({length:96},(_,i)=>i+1));
    assert.deepEqual(errors,[]);
    console.log('Mechanism filter DOM passed: lazy/deduplicated cache, retry, offscreen entries, quality/temperature wildcards, AND, grouping, collapsed summary, config/environment invalidation, disabled entries, narrow viewport, factory switch and no document writes.');
  } finally { await browser?.close(); server.kill(); }
})().catch(error=>{console.error(error);process.exitCode=1});
