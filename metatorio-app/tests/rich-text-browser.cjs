// Real Svelte DOM with the existing mocked IPC fixture; no production data changes.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const { seed, waitFor } = require("./mechanic-layout.cjs");
const assert = require("node:assert/strict");
const path = require("node:path");
const { spawn } = require("node:child_process");
const appRoot=path.resolve(__dirname,"..");
const port=Number(process.env.UI_TEST_PORT||5198),url=process.env.UI_TEST_URL||`http://127.0.0.1:${port}`;

(async()=>{
 const server=process.env.UI_TEST_URL?null:spawn(process.execPath,[path.join(appRoot,"node_modules/vite/bin/vite.js"),"--host","127.0.0.1","--port",String(port),"--strictPort"],{cwd:appRoot,windowsHide:true,stdio:"ignore"});
 let browser;
 try {
  await waitFor(async()=>{try{return (await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1600,height:1000}});
  const errors=[];page.on('pageerror',error=>errors.push(String(error)));
  await seed(page,65);
  await page.addInitScript(()=>{
   const raw='[color=#FF4040]量[/color][color=#FFA020]子[/color][color=#FFE040]点[/color][color=#40E080]催[/color][color=#40C0FF]化[/color][color=#B060FF]剂[/color]';
   const names={'quantum-dot-catalyst':raw,reference:'[item=iron-plate,quality=rare][font=default-bold]催化剂[/font]',untrusted:'<img src=x onerror="window.__richInjected=true">[color=red]安全[/color]'};
   window.__layoutDocument.projects[0].factories[0].targets[0].flow={Item:{id:'quantum-dot-catalyst',quality:'normal'}};
   const invoke=window.__TAURI_INTERNALS__.invoke;
   window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
    const result=await invoke(command,args);
    if(command==='catalog_index')for(const [name,localized_name] of Object.entries(names))result.entries.push({...result.entries[0],kind:'item',name,localized_name});
    if(command==='prototype_detail'&&names[args.name])result.localized_name=names[args.name];
    return result;
   };
  });
  await page.goto(url);
  const target=page.locator('.row-item').filter({has:page.locator('.flow-name')}).first();
  await target.waitFor();
  assert.equal(await target.locator('.flow-name').textContent(),'量子点催化剂');
  assert.equal(await target.locator('.rich-text [style*="color"]').count(),6);
  const colors=await target.locator('.rich-text > span').evaluateAll(nodes=>nodes.map(n=>getComputedStyle(n).color));
  assert.equal(new Set(colors).size,6);
  await target.locator('.hover-icon').hover();
  const tooltip=page.getByRole('tooltip');await tooltip.waitFor();
  assert.equal(await tooltip.locator('.heading strong').textContent(),'量子点催化剂');
  assert.ok(!(await tooltip.textContent()).includes('[color='));
  if(process.env.RICH_TEXT_SCREENSHOT)await page.screenshot({path:process.env.RICH_TEXT_SCREENSHOT});
  await page.mouse.move(1200,80);await tooltip.waitFor({state:'detached'});
  await target.locator('.hover-icon').click();
  const search=page.getByRole('textbox',{name:'搜索原型'});
  await search.fill('量子点');
  const rows=page.locator('.search-list > button');
  await rows.first().waitFor();assert.equal(await rows.count(),1);
  assert.equal(await rows.first().getAttribute('aria-label'),'量子点催化剂');
  assert.ok(!(await rows.first().getAttribute('title')).includes('[color='));
  await search.fill('FF4040');assert.equal(await rows.count(),0);
  await search.fill('ＱＵＡＮＴＵＭ＿ＤＯＴ');assert.equal(await rows.count(),1);
  await search.fill('铁板');assert.ok(await rows.count()>0);
  await search.fill('reference');
  assert.equal(await rows.count(),1);
  assert.ok((await rows.first().textContent()).includes('铁板'));
  assert.equal(await rows.first().getByRole('img',{name:'铁板 (稀有)',exact:true}).getAttribute('title'),'铁板 (稀有)');
  assert.equal(await rows.first().locator('[style*="font-weight"]').textContent(),'催化剂');
  await search.fill('安全');assert.equal(await rows.count(),1);
  assert.ok((await rows.first().textContent()).includes('<img src=x'));
  assert.equal(await rows.first().locator('img[src="x"]').count(),0);
  assert.equal(await page.evaluate(()=>window.__richInjected),undefined);
  assert.deepEqual(errors,[]);
  console.log('Rich text DOM regression passed: colors, tooltip, CJK search, full-width ID, metadata exclusion, references, fallback, HTML inertness.');
 } finally {await browser?.close();server?.kill();}
})().catch(error=>{console.error(error);process.exitCode=1});
