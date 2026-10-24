<script lang="ts">
  import { dndzone } from 'svelte-dnd-action';
  import { runtime } from '$lib/runtime/store.svelte';
  import type { FactoryAction, UniverseAction, FlowTarget, TargetExpression, DualVar, Location } from '$lib/runtime/types';
  import type { MechanicScope } from '$lib/runtime/mechanic-commands';
  import type { MechanicFlowSelection } from '$lib/mechanics/actions';
  import { parseCompactNumber } from "$lib/format";
  import { networkRate } from "$lib/mechanics/network";
  import { isEnergyFlow } from '$lib/dual-var';
  import FlowRef from './FlowRef.svelte';
  import FlowRangeEditor from './FlowRangeEditor.svelte';
  import LocationPicker from './LocationPicker.svelte';
  import { flowSelectionInitial } from '$lib/mechanics/flow-selection';
  let {scope, targets, expressions, selectFlow, onSuggest}: {scope: MechanicScope; targets: FlowTarget[]; expressions: TargetExpression[];
    selectFlow: (request: MechanicFlowSelection & {initialName?:string;initialQuality?:string}) => void;
    onSuggest?: (flow: DualVar,location?:Location) => void} = $props();
  let targetsOpen=$state(true), expressionsOpen=$state(false), rows=$state<FlowTarget[]>([]);
  $effect(() => { rows=targets; });
  let exprRows=$state<TargetExpression[]>([]);
  $effect(()=>{exprRows=expressions.map(e=>({...e,terms:[...e.terms]}));});
  const factor=$derived(runtime.selectedProject?.settings.time_scale === 'minutes' ? 60 : runtime.selectedProject?.settings.time_scale === 'hours' ? 3600 : 1);
  function writer() {
    const destination={...scope};
    return (normal: FactoryAction, universe: UniverseAction) => (destination.kind === 'universe'
      ? runtime.dispatchUniverse(destination.project,destination.factory,universe)
      : runtime.send({scope:'factory',action:{project:destination.project,factory:destination.factory,action:normal}})).catch(() => {});
  }
  function choose(title: string, current: DualVar | undefined, apply: (flow:DualVar) => void) {
    selectFlow({title,...flowSelectionInitial(current),onSelectFlow:apply});
  }
  function setTarget(target:FlowTarget, flow:DualVar, location=target.location) {
    return writer()({target:{'set-flow':{target:target.id,flow}}},{'set-target-flow':{target:target.id,flow:{location:location ?? runtime.universeAddLocation,flow}}});
  }
  function editTarget(target:FlowTarget) {
    const send=writer(), location=target.location ?? runtime.universeAddLocation;
    choose('更改目标流',target.flow,flow => send({target:{'set-flow':{target:target.id,flow}}},{'set-target-flow':{target:target.id,flow:{location,flow}}}));
  }
  function addTarget() {
    const send=writer(), location=runtime.universeAddLocation;
    choose('添加生产目标',undefined,flow => send({flow:{'add-to-target':{flow,amount:1}}},{'add-target':{flow:{location,flow},amount:1}}));
  }
  function editTerm(expression:number, term:TargetExpression['terms'][number], next?:DualVar, location=term.location) {
    const send=writer(), loc=location ?? runtime.universeAddLocation;
    const apply=(flow:DualVar) => send({'target-expression':{'set-term-flow':{expression,term:term.id,flow}}},{'set-target-expression-term-flow':{expression,term:term.id,flow:{location:loc,flow}}});
    if (next) apply(next); else choose('更改目标表达式的流',term.flow,apply);
  }
  function addTerm(expression:number) {
    const send=writer(), location=runtime.universeAddLocation;
    choose('添加目标表达式项',undefined,flow => send({'target-expression':{'add-term':{expression,term:{id:0,flow,coefficient:1}}}},{'add-target-expression-term':{expression,flow:{location,flow}}}));
  }
  async function reorder(event:CustomEvent<{items:FlowTarget[]}>) {
    rows=event.detail.items;
    const send=writer(),order=event.detail.items.map(t=>t.id);
    for (let position=order.length-1;position>=0;position--) await send({target:{reorder:{target:order[position],position}}},{'reorder-target':{target:order[position],position}});
  }
</script>
<section class="panel targets-panel">
  <button class="title panel-toggle" aria-expanded={targetsOpen} onclick={() => targetsOpen=!targetsOpen}> <span>{targetsOpen?'▾':'▸'}　目标</span><span class="count">{targets.length}</span></button>
  {#if targetsOpen}
    <div class="rows" use:dndzone={{items:rows,type:`targets-${scope.kind ?? 'factory'}-${scope.factory}`,flipDurationMs:120}} onconsider={e=>rows=e.detail.items} onfinalize={reorder}>
      {#each rows as target (target.id)}
        <div class="row-item one-line">
          {#if target.location}<LocationPicker value={target.location} label="目标地点" onChange={location=>setTarget(target,target.flow,location)} />{/if}
          <FlowRef flow={target.flow} title="左键更改流 · 右键查看建议" grow showQualifiers={false} showBadge={!(typeof target.flow==='object' && 'Fluid' in target.flow)} size={26} onClick={()=>editTarget(target)} onContextMenu={()=>onSuggest?.(target.flow,target.location)} />
          <FlowRangeEditor flow={target.flow} onChange={flow=>setTarget(target,flow)} />
          <span class="target-value">
          <input class="num" type="text" inputmode="decimal" aria-label="目标数量" value={isEnergyFlow(target.flow)?networkRate(target.flow,target.amount,"seconds"):target.amount*factor} onchange={e=>{
            const raw=e.currentTarget.value.replace(/\s|W$/g,""); const parsed=Number.isFinite(Number(raw))?Number(raw):parseCompactNumber(raw); const amount=parsed == null ? NaN : parsed/(isEnergyFlow(target.flow)?1:factor); if(Number.isFinite(amount)) writer()({target:{'set-amount':{target:target.id,amount}}},{'set-target-amount':{target:target.id,amount}});
          }} />
          <button class="icon-btn danger" title="移除目标" onclick={()=>writer()({target:{remove:{target:target.id}}},{'remove-target':{target:target.id}})}>×</button>
          </span>
        </div>
      {/each}
    </div>
    <button class="btn block" onclick={addTarget}>+ 添加目标</button>
  {/if}
</section>
<section class="panel expression-panel">
  <button class="title panel-toggle" aria-expanded={expressionsOpen} onclick={()=>expressionsOpen=!expressionsOpen}><span>{expressionsOpen?'▾':'▸'}　目标表达式</span><span class="count">{expressions.length}</span></button>
  {#if expressionsOpen}
    <div use:dndzone={{items:exprRows,type:`expressions-${scope.kind??'factory'}-${scope.factory}`,flipDurationMs:120}} onconsider={e=>exprRows=e.detail.items} onfinalize={async e=>{
      exprRows=e.detail.items;const send=writer(),order=exprRows.map(e=>e.id); for(let position=order.length-1;position>=0;position--)await send({'target-expression':{reorder:{expression:order[position],position}}},{'reorder-target-expression':{expression:order[position],position}});
    }}>
    {#each exprRows as expression (expression.id)}
      <div class="expr-card">
        <div class="row-item"><label>常数 <input class="num" type="number" aria-label="目标表达式常数" value={expression.constant} onchange={e=>{const constant=Number(e.currentTarget.value);if(Number.isFinite(constant))writer()({'target-expression':{'set-constant':{expression:expression.id,constant}}},{'set-target-expression-constant':{expression:expression.id,constant}})}} /></label>
          <button class="icon-btn" title="移除表达式" onclick={()=>writer()({'target-expression':{remove:{expression:expression.id}}},{'remove-target-expression':{expression:expression.id}})}>×</button></div>
        <div use:dndzone={{items:expression.terms,type:`expression-terms-${expression.id}`,flipDurationMs:120}} onconsider={e=>expression.terms=e.detail.items} onfinalize={async e=>{
          expression.terms=e.detail.items; const send=writer(),order=expression.terms.map(t=>t.id); for(let position=order.length-1;position>=0;position--)await send({'target-expression':{'reorder-term':{expression:expression.id,term:order[position],position}}},{'reorder-target-expression-term':{expression:expression.id,term:order[position],position}});
        }}>
        {#each expression.terms as term (term.id)}
          <div class="row-item expr-term">
            {#if term.location}<LocationPicker value={term.location} label="表达式项地点" onChange={location=>editTerm(expression.id,term,term.flow,location)} />{/if}
            <FlowRef flow={term.flow} grow size={20} onClick={()=>editTerm(expression.id,term)} />
            <FlowRangeEditor flow={term.flow} onChange={flow=>editTerm(expression.id,term,flow)} />
            <input class="num" type="number" aria-label="表达式系数" value={term.coefficient} onchange={e=>{const coefficient=Number(e.currentTarget.value);if(Number.isFinite(coefficient))writer()({'target-expression':{'set-term-coefficient':{expression:expression.id,term:term.id,coefficient}}},{'set-target-expression-term-coefficient':{expression:expression.id,term:term.id,coefficient}})}} />
            <button class="icon-btn" title="移除项" onclick={()=>writer()({'target-expression':{'remove-term':{expression:expression.id,term:term.id}}},{'remove-target-expression-term':{expression:expression.id,term:term.id}})}>×</button>
          </div>
        {/each}
        </div>
        <button class="btn block" onclick={()=>addTerm(expression.id)}>+ 添加项</button>
      </div>
    {/each}
    </div>
    {#if !exprRows.length}<div class="empty-hint">目标表达式 = 常数 + Σ(流 × 系数)</div>{/if}
    <button class="btn block" onclick={()=>writer()({'target-expression':{add:{expression:{id:0,constant:0,terms:[]}}}},'add-target-expression')}>+ 添加表达式</button>
  {/if}
</section>
<style>
  .panel {padding:14px 16px;border:0;border-bottom:1px solid var(--line);border-radius:0;box-shadow:none;background:transparent;flex:0 0 auto}
  .panel-toggle {display:flex;justify-content:space-between;width:100%;border:0;padding:0 0 8px;background:none;color:var(--text);font:inherit;font-weight:600;text-align:left;cursor:pointer}
  .count,.empty-hint {font-size:11px;color:var(--muted)}
  .rows {display:flex;flex-direction:column;gap:4px}.row-item {display:flex;align-items:center;gap:5px;flex-wrap:wrap;padding:4px 0;min-width:0}
  .target-value {display:inline-flex;align-items:center;gap:4px;margin-left:auto}
  .num {width:65px;min-width:0;padding:5px;border:1px solid var(--line);border-radius:var(--radius-sm);color:var(--text);background:var(--panel);font-family:var(--mono);text-align:right}
  .block {width:100%;margin-top:6px}.expr-card {padding:6px 0;border-bottom:1px solid var(--line)}
  .icon-btn {border:0;background:none;color:var(--muted);cursor:pointer;padding:3px}.danger:hover {color:var(--danger)}
</style>
