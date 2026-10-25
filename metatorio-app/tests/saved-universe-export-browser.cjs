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
 const server=spawn(process.execPath,[path.join(root,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,windowsHide:true,stdio:'ignore'});
 let browser;
 try {
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  for(const entry of fixture.universes.filter(e=>!process.env.METATORIO_FIXTURE_UNIVERSE||String(e.universe)===process.env.METATORIO_FIXTURE_UNIVERSE)) {
   const page=await browser.newPage({viewport:{width:1600,height:1000}});page.setDefaultTimeout(180000);
   const errors=[];page.on('pageerror',e=>errors.push(String(e)));
   await page.exposeFunction('__saveFixturePng',args=>{
    const bytes=Buffer.from(args.base64,'base64');assert.equal(bytes.subarray(1,4).toString(),'PNG');
    const dest=path.join(out,args.fileName);fs.writeFileSync(dest,bytes);
    return {path:dest,width:bytes.readUInt32BE(16),height:bytes.readUInt32BE(20)};
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
     if(command==='save_network_png') {
      args.fileName=`fixture-${entry.universe}-${window.__realFixtureExports.length}.png`;
      const base64=await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result.split(',')[1]);reader.onerror=reject;reader.readAsDataURL(new Blob([new Uint8Array(args.bytes)],{type:'image/png'}));});
      const saved=await window.__saveFixturePng({fileName:args.fileName,base64});window.__realFixtureExports.push(saved);return saved.path;
     }
     return raw(command,args);
    };
   },{document:fixture.document,entry});
   await page.goto(url);
   const name=fixture.document.projects.find(p=>p.id===entry.project).universes.find(u=>u.id===entry.universe).name;
   await page.getByRole('button',{name,exact:true}).click();
   await page.getByRole('button',{name:'网络',exact:true}).click();
   const graph=page.getByRole('region',{name:'机制网络视图',exact:true});
   await waitFor(async()=>await graph.getAttribute('data-network-ready')==='true',1800);
   for(const clustered of [false,true]) {
    if(clustered){await page.getByLabel('按地点聚类').check();await waitFor(async()=>await graph.locator('.network-canvas').evaluate(el=>el._cyreg?.cy.nodes('[kind="location"]').length>0),1800);}
    await waitFor(async()=>await graph.getAttribute('data-network-ready')==='true',1800);
    const bounds=await graph.locator('.network-canvas').evaluate(el=>{const cy=el._cyreg.cy,b=cy.elements().boundingBox();return {width:b.w,height:b.h,nodes:cy.nodes().length,edges:cy.edges().length};});
    const count=await page.evaluate(()=>window.__realFixtureExports.length);
    await graph.getByRole('button',{name:'导出 PNG',exact:true}).click();
    try {
      await waitFor(async()=>await graph.getByRole('button',{name:'导出 PNG',exact:true}).isEnabled(),1800);
    } catch(error) {
      console.log(await page.evaluate(()=>({saved:window.__realFixtureExports.length,status:document.querySelector('.export-status')?.textContent,alerts:[...document.querySelectorAll('.network-view [role=alert]')].map(e=>e.textContent),buttons:[...document.querySelectorAll('.network-toolbar button')].map(e=>e.textContent)})));
      throw error;
    }
    assert.equal(await graph.getByRole('alert').count(),0,await graph.innerText());
    const saved=await page.evaluate(()=>window.__realFixtureExports.at(-1));
    assert.equal(await page.evaluate(()=>window.__realFixtureExports.length),count+1);
    assert.ok(saved.width<=8192&&saved.height<=8192&&saved.width*saved.height<=16*1024*1024);
    console.log(JSON.stringify({universe:entry.universe,clustered,mechanics:entry.preview.mechanics.length,...bounds,pngWidth:saved.width,pngHeight:saved.height}));
   }
   assert.deepEqual(errors,[]);await page.close();
  }
 } finally {await browser?.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1});
