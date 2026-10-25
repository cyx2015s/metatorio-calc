const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const {spawn}=require('node:child_process');
const path=require('node:path');const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..'),port=5220,url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(root,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,windowsHide:true,stdio:'ignore'});
 let browser;
 try{
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1000,height:700},deviceScaleFactor:2});await seed(page,1);await page.goto(url);
  const result=await page.evaluate(async()=>{
   const cytoscape=(await import('/node_modules/.vite/deps/cytoscape.js')).default;
   const {createNetworkPngSnapshot}=await import('/src/lib/mechanics/network-render.ts');
   const {networkPngTiles}=await import('/src/lib/mechanics/network-export.ts');
   const host=document.createElement('div');Object.assign(host.style,{position:'fixed',left:'0',top:'0',width:'350px',height:'250px'});document.body.append(host);
   const icon=document.createElement('canvas');icon.width=icon.height=24;const ic=icon.getContext('2d');ic.fillStyle='#00e000';ic.fillRect(0,0,24,24);const green=icon.toDataURL();ic.fillStyle='#0000ff';ic.fillRect(0,0,24,24);const blue=icon.toDataURL();
   const cy=cytoscape({container:host,pixelRatio:1,layout:{name:'preset'},elements:[
    {data:{id:'parent'}},{data:{id:'a',parent:'parent',label:'节点 alpha\n稀有'},position:{x:-30,y:-10}},
    {data:{id:'b',parent:'parent',label:'节点 beta'},position:{x:160,y:110}},
    {data:{id:'c',label:'屏幕之外'},position:{x:500,y:280}},
    {data:{id:'edge1',source:'a',target:'b',label:'跨分区标签'}},{data:{id:'edge2',source:'b',target:'c',label:'第二条边'}}
   ],style:[{selector:'node',style:{width:42,height:36,label:'data(label)','background-color':'#d22222','font-size':16,'text-wrap':'wrap','text-valign':'bottom','text-margin-y':6}},
   {selector:':parent',style:{'background-opacity':0.1,'background-color':'#999999','border-width':2,padding:20}},
   {selector:'edge',style:{label:'data(label)','curve-style':'bezier','target-arrow-shape':'triangle','font-size':12,width:2}}]});
   cy.getElementById('a').style({'background-image':[green,blue],'background-fit':['contain','none'],'background-width':['auto','35%'],'background-height':['auto','35%'],'background-position-x':['50%','100%'],'background-position-y':['50%','100%']});
   cy.zoom(.5);cy.pan({x:100,y:200});
   await new Promise(resolve=>setTimeout(resolve,200));
   const before=JSON.stringify({zoom:cy.zoom(),pan:cy.pan(),positions:cy.nodes().map(n=>n.position())});
   const snapshot=await createNetworkPngSnapshot(cy,'#ffffff',()=>false);
   const full=await snapshot.render({x:0,y:0,width:snapshot.width,height:snapshot.height});
   const reference=document.createElement('canvas');reference.width=snapshot.width;reference.height=snapshot.height;reference.getContext('2d').drawImage(await createImageBitmap(full),0,0);
   const stitched=document.createElement('canvas');stitched.width=snapshot.width;stitched.height=snapshot.height;
   let count=0;for(const tile of networkPngTiles(snapshot.width,snapshot.height,83,61)){
    const blob=await snapshot.render(tile);stitched.getContext('2d').drawImage(await createImageBitmap(blob),tile.x,tile.y);count++;
   }
   const expected=reference.getContext('2d').getImageData(0,0,reference.width,reference.height).data;
   const actual=stitched.getContext('2d').getImageData(0,0,stitched.width,stitched.height).data;
   let differences=0,ink=0,bluePixels=0;for(let i=0;i<actual.length;i++){if(expected[i]!==actual[i])differences++;if(i%4===0){if(actual[i]<240)ink++;if(actual[i]<20&&actual[i+1]<20&&actual[i+2]>220)bluePixels++;}}
   const unchanged=before===JSON.stringify({zoom:cy.zoom(),pan:cy.pan(),positions:cy.nodes().map(n=>n.position())});
   snapshot.destroy();cy.destroy();host.remove();
   return {differences,count,width:reference.width,height:reference.height,ink,bluePixels,unchanged};
  });
  assert.equal(result.differences,0,JSON.stringify(result));assert.ok(result.ink>100&&result.bluePixels>10);assert.ok(result.count>10);assert.ok(result.unchanged);
  console.log('Tiled renderer passed:',JSON.stringify(result));
 }finally{await browser?.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1});
