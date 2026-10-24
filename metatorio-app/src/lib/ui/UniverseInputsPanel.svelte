<script lang="ts">
  import {dndzone} from 'svelte-dnd-action';
  import { runtime } from '$lib/runtime/store.svelte';
  import type { DualVar, LocatedDualVar, Location, UniverseAction, UniverseDocument } from '$lib/runtime/types';
  import { flowSelectionInitial } from '$lib/mechanics/flow-selection';
  import type { MechanicFlowSelection } from '$lib/mechanics/actions';
  import { locationKey } from '$lib/mechanics/located';
  import { isEnergyFlow, presentFlow } from '$lib/dual-var';
  import FlowRef from './FlowRef.svelte';
  import FlowRangeEditor from './FlowRangeEditor.svelte';
  import LocationIcon from './LocationIcon.svelte';
  import LocationPicker from './LocationPicker.svelte';
  let {project, universe, selectFlow}: {project:number;universe:UniverseDocument;selectFlow:(request:MechanicFlowSelection)=>void}=$props();
  let open=$state(true), sources=$state<LocatedDualVar[]>([]), loading=$state(false), error=$state('');
  let pending=$state<Record<string,boolean>>({});
  const key=(flow:LocatedDualVar)=>JSON.stringify([locationKey(flow.location),flow.flow]);
  const inputs=$derived(universe.external_inputs??[]);
  let dragInputs=$state<typeof universe.external_inputs>([]);
  $effect(()=>{dragInputs=inputs;});
  const hasFreeEnergy=$derived(["Electricity","Heat"].every(flow=>inputs.some(i=>key(i.flow)===key({location:runtime.universeAddLocation,flow:flow as DualVar})&&i.penalty===0)));
  const implicit=$derived(sources.filter(flow=>!inputs.some(input=>key(input.flow)===key(flow))));
  const factor=$derived(runtime.selectedProject?.settings.time_scale==='minutes'?60:runtime.selectedProject?.settings.time_scale==='hours'?3600:1);
  $effect(()=>{
    void universe; void runtime.selectedProject?.settings; void runtime.effectiveContextId;
    let alive=true;loading=true;error='';sources=[];
    runtime.loadUniversePreview(project,universe.id).then(preview=>{if(alive)sources=preview.sources;}).catch(e=>{if(alive)error=String(e);}).finally(()=>{if(alive)loading=false;});
    return ()=>{alive=false;};
  });
  function writer(){const p=project,u=universe.id;return (action:UniverseAction)=>runtime.dispatchUniverse(p,u,action);}
  const label=(flow:DualVar)=>presentFlow(flow,(k,n)=>runtime.localizedName(k,n)).description;
  const displayed=(flow:DualVar,penalty:number)=>isEnergyFlow(flow)?penalty*1e6:penalty/factor;
  const internal=(flow:DualVar,value:number)=>isEnergyFlow(flow)?value/1e6:value*factor;
  function setFlow(input:typeof inputs[number], flow:DualVar, location:Location=input.flow.location){return writer()({'set-external-input-flow':{input:input.id,flow:{location,flow}}}).catch(()=>{});}
  function choose(input?:typeof inputs[number]){
    const send=writer(),location=input?.flow.location??runtime.universeAddLocation;
    selectFlow({...flowSelectionInitial(input?.flow.flow),title:input?'更改外部输入流':'添加外部输入流',onSelectFlow:flow=>send(input?{'set-external-input-flow':{input:input.id,flow:{location,flow}}}:{'add-external-input':{flow:{location,flow},penalty:1}}).catch(()=>{})});
  }
  async function promote(flow:LocatedDualVar,penalty:number){
    const token=`${project}:${universe.id}:${key(flow)}`;
    if(pending[token]||inputs.some(i=>key(i.flow)===key(flow)))return;
    pending[token]=true;try{await writer()({'add-external-input':{flow,penalty}});}finally{delete pending[token];}
  }
  async function freeEnergy(){
    const send=writer(),location=runtime.universeAddLocation, snapshot=[...inputs];
    const token=`energy:${project}:${universe.id}`;if(pending[token])return;pending[token]=true;
    try{for(const flow of ['Electricity','Heat'] as DualVar[]){const old=snapshot.find(i=>key(i.flow)===key({location,flow}));await send(old?{'set-external-input-penalty':{input:old.id,penalty:0}}:{'add-external-input':{flow:{location,flow},penalty:0}});}}finally{delete pending[token];}
  }
</script>
<section class="inputs-panel" aria-label="外部输入">
  <button class="panel-toggle" aria-expanded={open} onclick={()=>open=!open}><span>{open?'▾':'▸'}　外部输入</span><span class="muted">{inputs.length}</span></button>
  {#if open}
    <div class="shortcuts"><button class="btn" onclick={()=>freeEnergy().catch(()=>{})} disabled={hasFreeEnergy||!!pending[`energy:${project}:${universe.id}`]}>+ 免费电力与热量</button><LocationPicker value={runtime.universeAddLocation} label="外部输入默认地点" onChange={location=>runtime.universeAddLocation=location}/></div>
    <div use:dndzone={{items:dragInputs,type:`universe-inputs-${project}-${universe.id}`,flipDurationMs:120}} onconsider={e=>dragInputs=e.detail.items} onfinalize={async e=>{
      dragInputs=e.detail.items;const send=writer(),order=dragInputs.map(i=>i.id);for(let position=order.length-1;position>=0;position--)await send({'reorder-external-input':{input:order[position],position}});
    }}>
    {#each dragInputs as input (input.id)}
      <div class="input-row">
        <LocationPicker value={input.flow.location} label="外部输入地点" onChange={location=>setFlow(input,input.flow.flow,location)}/>
        <FlowRef flow={input.flow.flow} grow size={24} showBadge={!(typeof input.flow.flow==='object'&&'Fluid' in input.flow.flow)} onClick={()=>choose(input)} />
        <FlowRangeEditor flow={input.flow.flow} onChange={flow=>setFlow(input,flow)} />
        {#if input.penalty===1e12}<span class="muted">已禁用</span>{/if}
        <input class="num" type="number" min="0" step="any" aria-label={`外部输入成本：${label(input.flow.flow)}`} title={isEnergyFlow(input.flow.flow)?'每 MW 成本':'当前时间单位下的外部输入成本'} value={displayed(input.flow.flow,input.penalty)} onchange={e=>{const value=Number(e.currentTarget.value);if(e.currentTarget.value.trim()&&Number.isFinite(value)&&value>=0)writer()({'set-external-input-penalty':{input:input.id,penalty:internal(input.flow.flow,value)}}).catch(()=>{});}} />
        <button class="icon-btn danger" aria-label="移除外部输入" title="移除覆盖；若环境提供该资源，将恢复免费输入" onclick={()=>writer()({'remove-external-input':{input:input.id}}).catch(()=>{})}>×</button>
      </div>
    {:else}<div class="muted hint">还没有外部输入</div>{/each}
    </div>
    {#if loading}<div class="muted hint">读取环境来源…</div>{/if}
    {#if error}<div class="error" role="status">{error}</div>{/if}
    {#if implicit.length}<div class="muted hint">环境自带（隐式免费，编辑成本可转为手动输入）</div>{/if}
    {#each implicit as flow (key(flow))}
      <div class="input-row implicit">
        <LocationIcon location={flow.location}/><FlowRef flow={flow.flow} grow size={24}/><span class="muted">隐式</span>
        <input class="num" type="number" min="0" step="any" value="0" aria-label={`外部成本：${label(flow.flow)}`} disabled={pending[`${project}:${universe.id}:${key(flow)}`]} onchange={e=>{const value=Number(e.currentTarget.value);if(e.currentTarget.value.trim()&&Number.isFinite(value)&&value>=0)promote(flow,internal(flow.flow,value)).catch(()=>{});}}/>
        <button class="icon-btn" aria-label="设为手动外部输入" disabled={pending[`${project}:${universe.id}:${key(flow)}`]} onclick={()=>promote(flow,0).catch(()=>{})}>+</button>
        <button class="icon-btn danger" aria-label={`禁用环境资源：${label(flow.flow)}`} title="将该地点的外部输入成本设为 10¹²" disabled={pending[`${project}:${universe.id}:${key(flow)}`]} onclick={()=>promote(flow,1e12).catch(()=>{})}>⊘</button>
      </div>
    {/each}
    <button class="btn block" onclick={()=>choose()}>+ 添加外部输入</button>
  {/if}
</section>
<style>
  .inputs-panel{padding:14px 16px;border-bottom:1px solid var(--line)}.panel-toggle{display:flex;justify-content:space-between;width:100%;border:0;background:none;color:var(--text);font:inherit;font-weight:600;padding:0 0 8px;cursor:pointer}
  .input-row{display:flex;align-items:center;gap:4px;flex-wrap:wrap;min-width:0;padding:5px 0}.implicit{border:1px dashed var(--line-strong);border-radius:var(--radius-sm);padding:4px;margin:3px 0}.shortcuts{display:flex;align-items:center;gap:5px;margin:4px 0 8px}
  .num{width:62px;padding:5px;border:1px solid var(--line);border-radius:var(--radius-sm);background:var(--panel);color:var(--text);font-family:var(--mono);text-align:right}.muted{font-size:11px;color:var(--muted)}.hint{padding:10px 0}.error{color:var(--danger);font-size:11px}.block{width:100%;margin-top:8px}.icon-btn{border:0;background:none;color:var(--muted);cursor:pointer;padding:3px}.danger{color:var(--danger)}
</style>
