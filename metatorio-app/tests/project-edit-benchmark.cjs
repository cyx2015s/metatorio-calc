// Manual mocked-IPC benchmark: compare selection costs in the same 600-mechanic document.
const { withBrowser, pause } = require('./browser-harness.cjs');
const { seed } = require('./mechanic-layout.cjs');
const fs = require('node:fs');
withBrowser({port:5223},async(browser,url)=>{
  const page=await browser.newPage({viewport:{width:1600,height:1000}});
  page.setDefaultTimeout(60000);
  await seed(page,600);
  await page.addInitScript(()=>{
    const doc=window.__layoutDocument,p=doc.projects[0];
    p.factories.push({...structuredClone(p.factories[0]),id:3,name:'empty',mechanics:[],targets:[]});
    const invoke=window.__TAURI_INTERNALS__.invoke;
    let revision=1;
    window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
      if(command==='dispatch'){
        const action=args.message.action.action;
        if(action['set-mining-productivity'])p.settings.mining_productivity=action['set-mining-productivity'].productivity;
        if(action['set-time-scale'])p.settings.time_scale=action['set-time-scale'].time_scale;
        if(action['set-name'])p.name=action['set-name'].name;
        return {revision:++revision,changed:true,created:{},commands:[],history:{can_undo:true,can_redo:false}};
      }
      return invoke(command,args);
    };
  });
  await page.goto(url);await page.locator('.mech-card').first().waitFor();await pause(600);
  const result=await page.evaluate(async()=>{
    const {runtime}=await import('/src/lib/runtime/store.svelte.ts');
    const paint=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    const rows=[];
    for(const factory of [3,2]){
      await runtime.selectFactory(factory);await paint();await new Promise(r=>setTimeout(r,200));
      for(const kind of ['name','time','numeric']){
        const samples=[];
        for(let i=0;i<6;i++){
          const old=runtime.selectedFactory.mechanics;
          const calls=window.__layoutCalls.length;
          const start=performance.now();
          if(kind==='name')await runtime.setProjectName(`name${factory}-${i}`);
          if(kind==='time')await runtime.setTimeScale(i%2?'hours':'seconds');
          if(kind==='numeric')await runtime.setMiningProductivity(i%2?0.5:0.25);
          await paint();
          samples.push({ms:performance.now()-start,retained:old===runtime.selectedFactory.mechanics,
            flows:window.__layoutCalls.slice(calls).filter(c=>c.command==='mechanic_flow').length});
        }
        rows.push({factory,kind,samples});
      }
    }
    return rows;
  });
  if (process.env.PERF_OUTPUT) fs.writeFileSync(process.env.PERF_OUTPUT,JSON.stringify(result,null,2));
  console.log(JSON.stringify(result.map(({factory, kind, samples}) => {
    const sorted = samples.map(sample => sample.ms).sort((a,b) => a-b);
    return {factory, kind, median: (sorted[2] + sorted[3]) / 2,
      flowCalls: samples.map(sample => sample.flows), retained: samples.every(sample => sample.retained)};
  }),null,2));
}).catch(error=>{console.error(error);process.exitCode=1;});
