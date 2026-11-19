<script lang="ts">
  import { untrack } from 'svelte';
  import { reorderIds } from '$lib/reorder';
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
  const inputKeys=$derived(new Set(inputs.map(input=>key(input.flow))));
  const projectId=$derived(project);
  const universeId=$derived(universe.id);
  const environmentKey=$derived(runtime.mechanicEnvironmentKey({project:projectId,factory:universeId,kind:'universe'}));
  let dragInputs=$state<typeof universe.external_inputs>([]);
  $effect(()=>{dragInputs=inputs;});
  const hasFreeEnergy=$derived(["Electricity","Heat"].every(flow=>inputs.some(i=>key(i.flow)===key({location:runtime.universeAddLocation,flow:flow as DualVar})&&i.penalty===0)));
  const implicit=$derived(sources.filter(flow=>!inputKeys.has(key(flow))));
  const factor=$derived(runtime.selectedProject?.settings.time_scale==='minutes'?60:runtime.selectedProject?.settings.time_scale==='hours'?3600:1);
  $effect(()=>{
    const p=projectId,u=universeId;
    void environmentKey; void runtime.flowRevision;
    let alive=true;loading=true;error='';sources=[];
    untrack(()=>runtime.loadUniversePreview(p,u)).then(preview=>{if(alive)sources=preview.sources;}).catch(e=>{if(alive)error=String(e);}).finally(()=>{if(alive)loading=false;});
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
    if(pending[token]||inputKeys.has(key(flow)))return;
    pending[token]=true;try{await writer()({'add-external-input':{flow,penalty}});}finally{delete pending[token];}
  }
  async function freeEnergy(){
    const send=writer(),location=runtime.universeAddLocation, snapshot=[...inputs];
    const token=`energy:${project}:${universe.id}`;if(pending[token])return;pending[token]=true;
    try{for(const flow of ['Electricity','Heat'] as DualVar[]){const old=snapshot.find(i=>key(i.flow)===key({location,flow}));await send(old?{'set-external-input-penalty':{input:old.id,penalty:0}}:{'add-external-input':{flow:{location,flow},penalty:0}});}}finally{delete pending[token];}
  }
</script>
<section class="inputs-panel" aria-label="外部输入">
  <button class="title panel-toggle" aria-expanded={open} onclick={()=>open=!open}>
    <span class="toggle-label">
      <span class="chev">{open?'▾':'▸'}</span>
      外部输入 <span class="count">{inputs.length}</span>
    </span>
  </button>
  {#if open}
    <div class="shortcuts"><button class="btn" onclick={()=>freeEnergy().catch(()=>{})} disabled={hasFreeEnergy||!!pending[`energy:${project}:${universe.id}`]}>+ 免费电力与热量</button><LocationPicker value={runtime.universeAddLocation} label="外部输入默认地点" onChange={location=>runtime.universeAddLocation=location}/></div>
    <div class="rows" use:dndzone={{items:dragInputs,type:`universe-inputs-${project}-${universe.id}`,flipDurationMs:120}} onconsider={e=>dragInputs=e.detail.items} onfinalize={async e=>{
      dragInputs=e.detail.items;const send=writer(),p=project,u=universe.id,order=dragInputs.map(i=>i.id);
      await reorderIds(order, () => runtime.document?.projects.find(e=>e.id===p)?.universes?.find(e=>e.id===u)?.external_inputs ?? [],
        (input,position) => send({'reorder-external-input':{input,position}}));
    }}>
    {#each dragInputs as input (input.id)}
      <div class="input-row">
        <LocationPicker value={input.flow.location} label="外部输入地点" onChange={location=>setFlow(input,input.flow.flow,location)}/>
        <FlowRef flow={input.flow.flow} grow showQualifiers={false} size={22} showBadge={!(typeof input.flow.flow==='object'&&'Fluid' in input.flow.flow)} onClick={()=>choose(input)} />
        <FlowRangeEditor flow={input.flow.flow} onChange={flow=>setFlow(input,flow)} />
        {#if input.penalty===1e12}<span class="chip">已禁用</span>{/if}
        <input class="num" type="number" min="0" step="any" aria-label={`外部输入成本：${label(input.flow.flow)}`} title={isEnergyFlow(input.flow.flow)?'每 MW 成本':'当前时间单位下的外部输入成本'} value={displayed(input.flow.flow,input.penalty ?? NaN)} onchange={e=>{const value=Number(e.currentTarget.value);if(e.currentTarget.value.trim()&&Number.isFinite(value)&&value>=0)writer()({'set-external-input-penalty':{input:input.id,penalty:internal(input.flow.flow,value)}}).catch(()=>{});}} />
        <span class="row-actions">
          <button class="icon-btn danger" aria-label="移除外部输入" title="移除覆盖；若环境提供该资源，将恢复免费输入" onclick={()=>writer()({'remove-external-input':{input:input.id}}).catch(()=>{})}>×</button>
        </span>
      </div>
    {:else}<div class="empty-hint">还没有外部输入</div>{/each}
    </div>
    {#if loading}<div class="empty-hint">读取环境来源…</div>{/if}
    {#if error}<div class="error" role="status">{error}</div>{/if}
    {#if implicit.length}<div class="implicit-hint">环境自带（隐式免费，编辑成本可转为手动输入）</div>{/if}
    {#each implicit as flow (key(flow))}
      <div class="input-row implicit">
        <LocationIcon location={flow.location} size={20}/><FlowRef flow={flow.flow} grow showQualifiers={false} size={22}/><span class="chip implicit-chip">隐式</span>
        <input class="num" type="number" min="0" step="any" value="0" aria-label={`外部成本：${label(flow.flow)}`} disabled={pending[`${project}:${universe.id}:${key(flow)}`]} onchange={e=>{const value=Number(e.currentTarget.value);if(e.currentTarget.value.trim()&&Number.isFinite(value)&&value>=0)promote(flow,internal(flow.flow,value)).catch(()=>{});}}/>
        <span class="row-actions">
          <button class="icon-btn" aria-label="设为手动外部输入" disabled={pending[`${project}:${universe.id}:${key(flow)}`]} onclick={()=>promote(flow,0).catch(()=>{})}>+</button>
          <button class="icon-btn danger" aria-label={`禁用环境资源：${label(flow.flow)}`} title="将该地点的外部输入成本设为 10¹²" disabled={pending[`${project}:${universe.id}:${key(flow)}`]} onclick={()=>promote(flow,1e12).catch(()=>{})}>⊘</button>
        </span>
      </div>
    {/each}
    <button class="btn block" onclick={()=>choose()}>+ 添加外部输入</button>
  {/if}
</section>
<style>
  .inputs-panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-bottom: 0;
    border-radius: 0;
    box-shadow: none;
  }
  .panel-toggle {
    width: 100%;
    display: block;
    margin: 0 0 10px;
    padding: 0;
    color: var(--text);
    background: none;
    border: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .panel-toggle:hover .chev {
    color: var(--text);
  }
  .toggle-label {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
  }
  .chev {
    width: 9px;
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
    transition: color var(--transition-fast);
  }
  .panel-toggle .count {
    margin-left: auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0;
  }
  .shortcuts {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
  }
  .shortcuts .btn {
    flex: 1;
    font-size: 11px;
  }
  .rows {
    display: grid;
    gap: 1px;
    margin-bottom: 10px;
  }
  .input-row {
    min-width: 0;
    min-height: 32px;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    padding: 4px 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .input-row:hover,
  .input-row:focus-within {
    background: var(--card-hover);
    border-color: var(--line);
  }
  .input-row.implicit {
    border-style: dashed;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--panel) 92%, var(--accent) 4%);
    margin: 2px 0;
  }
  .implicit-chip {
    padding: 0 5px;
    min-height: 19px;
    font-size: 9.5px;
    flex: 0 0 auto;
  }
  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex: 0 0 auto;
  }
  .row-actions .icon-btn {
    width: 21px;
    height: 22px;
    font-size: 11px;
  }
  .num {
    width: 50px;
    min-height: 24px;
    padding: 0 5px;
    flex: 0 0 auto;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10.5px;
    text-align: right;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .num:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .implicit-hint {
    margin: 10px 0 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .empty-hint {
    padding: 12px 4px;
    color: var(--faint);
    font-size: 11px;
    line-height: 1.55;
  }
  .error {
    color: var(--danger);
    font-size: 11px;
    margin: 6px 0;
  }
  .inputs-panel > .btn.block {
    margin-top: 8px;
  }
  @media (max-width: 1360px) {
    .input-row {
      gap: 4px;
    }
    .num {
      width: 46px;
    }
  }
</style>
