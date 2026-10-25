const {createPngNativeBridge}=require('./png-native-bridge.cjs');
// Opt-in end-to-end PNG rendering of real saved-project expansion/solve fixtures.
// Prepare with external_universe_export.rs; no fixture contents are printed.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {spawn}=require('node:child_process');
const fixturePath=process.env.METATORIO_NETWORK_FIXTURE;
assert.ok(fixturePath,'METATORIO_NETWORK_FIXTURE is required');
const fixture=JSON.parse(fs.readFileSync(fixturePath,'utf8'));
const out=path.resolve(process.env.METATORIO_NETWORK_PNG_OUT||path.dirname(fixturePath));
const root=path.resolve(__dirname,'..'),port=5218,url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(root,'node_modules/vite/bin/vite.js'),...(process.env.NETWORK_PREVIEW?['preview']:[]),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,windowsHide:true,stdio:'ignore'});
 let browser;
 const bridge=createPngNativeBridge(process.env.METATORIO_PNG_BRIDGE||path.resolve(root,'../target/debug/examples/network-png-bridge.exe'));
 try {
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.METATORIO_DEBUG_PORT?{args:[`--remote-debugging-port=${process.env.METATORIO_DEBUG_PORT}`]}:{}),...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  for(const entry of fixture.universes.filter(e=>!process.env.METATORIO_FIXTURE_UNIVERSE||String(e.universe)===process.env.METATORIO_FIXTURE_UNIVERSE)) {
   const page=await browser.newPage({viewport:{width:1600,height:1000}});page.setDefaultTimeout(600000);
   const errors=[];page.on('pageerror',e=>{errors.push(String(e));console.error(String(e));});
   const progress=new Map();
   await page.exposeFunction('__nativePng',async(command,args)=>{
    if(command==='begin_network_png'){
     const info=await bridge.call({cmd:'begin',path:path.join(out,args.fileName),width:args.width,height:args.height});
     progress.set(info.id,{done:0,total:Math.ceil(info.width/info.tile_width)*Math.ceil(info.height/info.tile_height)});return info;
    }
    if(command==='write_network_png_tile'){
     await bridge.call({cmd:'tile',id:args.exportId,x:args.x,y:args.y,imageBase64:args.imageBase64});
     const p=progress.get(args.exportId);p.done++;if(p.done===1||p.done%10===0||p.done===p.total)console.log(JSON.stringify({universe:entry.universe,tiles:p.done,total:p.total}));return;
    }
    if(command==='abort_network_png')return bridge.call({cmd:'abort',id:args.exportId});
    const dest=await bridge.call({cmd:'finish',id:args.exportId});
    const header=Buffer.alloc(24),file=fs.openSync(dest,'r');try{fs.readSync(file,header,0,24,0)}finally{fs.closeSync(file)};
    assert.equal(header.subarray(1,4).toString(),'PNG');
    return {path:dest,width:header.readUInt32BE(16),height:header.readUInt32BE(20)};
   });
   await seed(page,1);
   await page.addInitScript(({document:doc,entry})=>{
    doc.projects=doc.projects.filter(p=>p.id===entry.project);
    for(const p of doc.projects){p.factories=[];p.universes=p.universes.filter(u=>u.id===entry.universe);}
    const raw=window.__TAURI_INTERNALS__.invoke;
    window.__realFixtureExports=[];
    window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
     if(command==='get_document')return structuredClone(doc);
     if(command==='list_contexts') {
      const original=await raw(command,args),id=doc.projects[0].context_id||'test';
      return {active:id,contexts:[{...original.contexts[0],id,name:id}]};
     }
     if(command==='universe_preview')return entry.preview;
     if(command==='solve_universe')return entry.solved;
     if(command==='icon') {
      const canvas=document.createElement('canvas');canvas.width=canvas.height=32;
      const ctx=canvas.getContext('2d');ctx.fillStyle=args.ty==='planet'?'#4b9cbd':'#8b9b65';ctx.fillRect(0,0,32,32);
      return Array.from(atob(canvas.toDataURL().split(',')[1]),c=>c.charCodeAt(0));
     }
     if(command==='begin_network_png') {
      return window.__nativePng(command,{...args,fileName:`fixture-${entry.universe}-${window.__realFixtureExports.length}.png`});
     }
     if(command==='write_network_png_tile'||command==='abort_network_png')return window.__nativePng(command,args);
     if(command==='finish_network_png') {
      const saved=await window.__nativePng(command,args);window.__realFixtureExports.push(saved);return saved.path;
     }
     return raw(command,args);
    };
   },{document:fixture.document,entry});
   await page.goto(url);
   const name=fixture.document.projects.find(p=>p.id===entry.project).universes.find(u=>u.id===entry.universe).name;
   await page.getByRole('button',{name,exact:true}).click();
   await page.getByRole('button',{name:'网络',exact:true}).click();
   console.log(JSON.stringify({universe:entry.universe,phase:'layout'}));
   const graph=page.getByRole('region',{name:'机制网络视图',exact:true});
   await waitFor(async()=>{const failure=await graph.getByRole('alert').allTextContents();if(failure.some(Boolean))throw Error(failure.join('\n'));return await graph.getAttribute('data-network-ready')==='true'},12000);
   for(const clustered of [false,true]) {
    if(clustered){await page.getByLabel('按地点聚类').check();await waitFor(async()=>await graph.locator('.network-canvas').evaluate(el=>el._cyreg?.cy.nodes('[kind="location"]').length>0),12000);}
    await waitFor(async()=>{const failure=await graph.getByRole('alert').allTextContents();if(failure.some(Boolean))throw Error(failure.join('\n'));return await graph.getAttribute('data-network-ready')==='true'},12000);
    const bounds=await graph.locator('.network-canvas').evaluate(el=>{const cy=el._cyreg.cy,b=cy.elements().boundingBox();return {width:b.w,height:b.h,nodes:cy.nodes().length,edges:cy.edges().length};});
    const count=await page.evaluate(()=>window.__realFixtureExports.length);
    console.log(JSON.stringify({universe:entry.universe,clustered,phase:'export',...bounds}));
    await graph.getByRole('button',{name:'导出 PNG',exact:true}).click();
    try {
      await waitFor(async()=>await graph.getByRole('button',{name:'导出 PNG',exact:true}).isEnabled(),12000);
    } catch(error) {
      console.log(await page.evaluate(()=>({saved:window.__realFixtureExports.length,status:document.querySelector('.export-status')?.textContent,alerts:[...document.querySelectorAll('.network-view [role=alert]')].map(e=>e.textContent),buttons:[...document.querySelectorAll('.network-toolbar button')].map(e=>e.textContent)})));
      throw error;
    }
    assert.equal(await graph.getByRole('alert').count(),0,await graph.innerText());
    const saved=await page.evaluate(()=>window.__realFixtureExports.at(-1));
    assert.equal(await page.evaluate(()=>window.__realFixtureExports.length),count+1);
    assert.equal(saved.width,Math.ceil(bounds.width));
    assert.equal(saved.height,Math.ceil(bounds.height));
    console.log(JSON.stringify({universe:entry.universe,clustered,mechanics:entry.preview.mechanics.length,...bounds,pngWidth:saved.width,pngHeight:saved.height}));
   }
   assert.deepEqual(errors,[]);await page.close();
  }
 } finally {await browser?.close();server.kill();bridge.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
