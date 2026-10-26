const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const {seed,waitFor}=require('./mechanic-layout.cjs');
const assert=require('node:assert/strict'),path=require('node:path'),{spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),port=5222,url=`http://127.0.0.1:${port}`;
(async()=>{
 const server=spawn(process.execPath,[path.join(root,'node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,windowsHide:true,stdio:'ignore'});
 let browser;
 try{
  await waitFor(async()=>{try{return(await fetch(url)).ok}catch{return false}},600);
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
  const page=await browser.newPage({viewport:{width:1600,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await seed(page,2);
  await page.addInitScript(()=>{
   const p=window.__layoutDocument.projects[0],factory=p.factories[0],q=(id,quality='normal')=>({id,quality});
   p.settings.reactor_neighbours={r:5};
   factory.mechanics.splice(0,factory.mechanics.length,
    {id:1,enabled:true,mechanic:{type:'fusion-reactor',reactor:q('r','rare'),neighbours:null,fuel:null}},
    {id:2,enabled:true,mechanic:{type:'fusion-generator',generator:q('g','rare'),temperature:4000000}});
   const raw=window.__TAURI_INTERNALS__.invoke;window.__fusionEdits=[];
   window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
    if(command==='catalog_index'){
     const index=await raw(command,args);
     for(const [kind,name,localized_name] of [['fusion-reactor','r','测试聚变堆'],['fusion-generator','g','测试聚变机'],['item','cell','聚变燃料棒'],['fluid','plasma','等离子体']])index.entries.push({kind,name,localized_name,categories:[],fuel_categories:kind==='item'?['fusion']:[],fuel_value_j:kind==='item'?4e10:null,group:'test',subgroup:'test'});
     return index;
    }
    if(command==='prototype_detail'&&['r','g'].includes(args.name))return {kind:args.kind,name:args.name,localized_name:args.name==='r'?'测试聚变堆':'测试聚变机',categories:[],ingredients:[],products:[],module_slots:0,burner_fuel_categories:args.name==='r'?['fusion']:[],machine_energy_source:args.name==='r'?'electric':null,fluid_filter:args.name==='r'?'cold':'plasma',output_fluid_filter:args.name==='r'?'plasma':'hot',neighbour_bonus:args.name==='r'?1:null};
    if(command==='icon'){
     const c=document.createElement('canvas');c.width=c.height=32;const ctx=c.getContext('2d');ctx.fillStyle=args.name==='r'?'#b033aa':'#3456ab';ctx.fillRect(0,0,32,32);
     return Array.from(atob(c.toDataURL().split(',')[1]),c=>c.charCodeAt(0));
    }
    if(command==='mechanic_flow')return args.mechanic===1?[['Electricity',-1e7],[{Fluid:{name:'plasma',temperature:[6e6,6e6]}},4]]:[['Electricity',5e7],[{Fluid:{name:'plasma',temperature:[4e6,4e6]}},-0.5]];
    if(command==='dispatch'&&args.message.action?.action?.mechanic){
     const value=args.message.action.action.mechanic,entry=factory.mechanics.find(e=>e.id===value.mechanic),a=value.action[entry.mechanic.type];window.__fusionEdits.push(value);
     for(const [operation,field] of [['set-neighbours','neighbours'],['set-temperature','temperature'],['set-reactor','reactor'],['set-generator','generator'],['set-fuel','fuel']])if(a[operation])entry.mechanic[field]=a[operation][field];
     return {revision:window.__fusionEdits.length,changed:true,created:[],commands:[],history:{can_undo:true,can_redo:false}};
    }
    return raw(command,args);
   };
  });
  await page.goto(url);await page.locator('.mech-card').first().waitFor();
  assert.equal(await page.locator('.mech-card').count(),2);
  await page.getByRole('button',{name:/添加机制/}).click();
  await page.getByRole('button',{name:'聚变反应堆',exact:true}).waitFor();
  await page.getByRole('button',{name:'聚变发电机',exact:true}).waitFor();
  await page.locator('.menu-catcher').click({position:{x:1,y:1}});
  const reactor=page.locator('[data-mechanic-editor="1"]'),generator=page.locator('[data-mechanic-editor="2"]');
  await reactor.getByRole('button',{name:/相邻 5/}).click();
  const neighbours=page.getByLabel('反应堆邻居数');await neighbours.fill('4');await neighbours.press('Tab');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].factories[0].mechanics[0].mechanic.neighbours===4));
  await neighbours.fill('');await neighbours.press('Tab');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].factories[0].mechanics[0].mechanic.neighbours===null));
  await page.getByRole('button',{name:'燃料：自动',exact:true}).click();
  const fuelDialog=page.getByRole('dialog',{name:'选择燃料',exact:true});await fuelDialog.getByRole('button',{name:'聚变燃料棒',exact:true}).dblclick();
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].factories[0].mechanics[0].mechanic.fuel?.item.id==='cell'));
  await page.keyboard.press('Escape');
  await generator.getByRole('button',{name:'参数',exact:false}).click();
  await page.getByLabel('聚变发电机输入温度').fill('6000000');await page.getByLabel('聚变发电机输入温度').press('Tab');
  await waitFor(async()=>page.evaluate(()=>window.__layoutDocument.projects[0].factories[0].mechanics[1].mechanic.temperature===6000000));
  await page.keyboard.press('Escape');
  await page.getByRole('button',{name:'网络',exact:true}).click();
  const graph=page.getByRole('region',{name:'机制网络视图'});await waitFor(async()=>await graph.getAttribute('data-network-ready')==='true',600);
  assert.ok(await graph.locator('.network-canvas').evaluate(el=>el._cyreg.cy.nodes().filter(n=>n.data('kind')==='mechanic').every(n=>String(n.style('background-width')).includes('%'))));
  assert.deepEqual(errors,[]);console.log('Fusion browser passed: inherited neighbours, fuel category filtering, temperature edit and quality network icons.');
 }finally{await browser?.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1});
