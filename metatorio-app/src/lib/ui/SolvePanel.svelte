<script lang="ts">
  import { diagnosticFrom } from "$lib/diagnostic";
  import { runtime } from '$lib/runtime/store.svelte';
  import type { SolveResult, UniverseSolveResult, DualVar, Location } from '$lib/runtime/types';
  import { networkRate } from '$lib/mechanics/network';
  import LocationIcon from './LocationIcon.svelte';
  import DiagnosticView from './DiagnosticView.svelte';
  import OptimizationSummary from './OptimizationSummary.svelte';
  import FlowRef from './FlowRef.svelte';
  let {solve, universeSolve=null, error=null, diagnostic=null, busy=false, onSuggest}: {
    solve:SolveResult|null;universeSolve?:UniverseSolveResult|null;error?:string|null;diagnostic?:import("$lib/runtime/types").RuntimeDiagnostic|null;busy?:boolean;onSuggest?:(flow:DualVar,location?:Location)=>void;
  }=$props();
  const status=$derived(solve && 'solved' in solve.status ? solve.status.solved : null);
  const located=$derived(universeSolve?.status.status==='solved'?universeSolve.status:null);
  const failed=$derived(universeSolve?.status.status==='not-solved'?universeSolve.status:null);
  const rows=$derived.by(()=>{
    if(located) return located.flows.filter(f=>Math.abs(f.amount)/Math.max(f.scale??1,1e-12)>1e-9).map(f=>({flow:f.flow.flow,location:f.flow.location,amount:f.amount}));
    return (status?.flows ?? []).filter(f=>Math.abs(f.amount)/Math.max(f.scale??1,1e-12)>1e-9).map(f=>({flow:f.flow,location:undefined as Location|undefined,amount:f.amount}));
  });
  const ordinaryFailure=$derived(solve && 'not-solved' in solve.status ? solve.status['not-solved'] : null);
  const solved=$derived(!!status||!!located);
  const cost=$derived(located?.cost??status?.cost??0);
  const timeScale=$derived(runtime.selectedProject?.settings.time_scale??'seconds');
</script>
<section class="panel solve-panel">
  <div class="solve-head"><div class="title">求解结果</div><span class="chip" class:ok={solved} class:warn={!!error||!!failed||!!ordinaryFailure||busy}>{busy?'求解中':solved?'已求解':failed||ordinaryFailure?'未求解':'待求解'}</span></div>
  {#if error}<div class="err-box"><strong>求解失败</strong><DiagnosticView diagnostic={diagnostic??(runtime.solveDiagnostic?.message===error ? runtime.solveDiagnostic : diagnosticFrom(error))} /></div>{/if}
  {#if solved}
    <div class="cost-block"><span class="eyebrow">总成本</span><strong class="mono cost-value">{cost.toFixed(3)}</strong><span class="cost-note">机制与外部输入成本之和 · 结果随编辑自动刷新</span></div>
    {#if (universeSolve??solve)?.report}<OptimizationSummary report={(universeSolve??solve)!.report} />{/if}
    <div class="section-head"><span>总流平衡</span><span class="count">{rows.length} 条</span></div>
    <div class="rows compact">
      {#each rows as row (JSON.stringify([row.location,row.flow]))}
        <div class="row-item one-line result-row">
          {#if row.location}<LocationIcon location={row.location} size={22}/>{/if}
          <FlowRef flow={row.flow} grow showQualifiers={false} size={22} onClick={onSuggest?()=>onSuggest?.(row.flow,row.location):undefined} onContextMenu={onSuggest?()=>onSuggest?.(row.flow,row.location):undefined} />
          <strong class="mono amount" class:amount-pos={row.amount>0}>{row.amount<0?'-':row.amount>0?'+':''}{networkRate(row.flow,row.amount,timeScale)}</strong>
          {#if onSuggest}<button class="icon-btn" title="查看建议" onclick={()=>onSuggest?.(row.flow,row.location)}>✦</button>{/if}
        </div>
      {/each}
    </div>
  {:else if failed}
    <div class="err-box"><strong>未求解</strong><p>{failed.description}</p>
      {#each [{label:'无供给',flows:failed.no_provider},{label:'无消耗',flows:failed.no_consumer}] as group}
        {#if group.flows.length}<h4>{group.label}</h4>{#each group.flows as flow}<div class="row-item"><LocationIcon location={flow.location} size={22}/><FlowRef flow={flow.flow} grow onClick={onSuggest?()=>onSuggest?.(flow.flow,flow.location):undefined} /></div>{/each}{/if}
      {/each}
    </div>
  {:else if ordinaryFailure}<div class="err-box"><strong>未求解</strong><DiagnosticView diagnostic={{code:'not-solved',message:ordinaryFailure.description,no_provider:ordinaryFailure.no_provider,no_consumer:ordinaryFailure.no_consumer,flows:[]}} onFlowClick={onSuggest} /></div>
  {:else}<div class="empty-hint">改动目标或机制后会自动重新求解。</div>{/if}
</section>
<style>
  .panel{padding:14px 16px;background:transparent;border:0;border-radius:0;box-shadow:none}.solve-head,.section-head{display:flex;align-items:center;justify-content:space-between;gap:8px}.title{font-weight:600}.chip{padding:3px 8px;border:1px solid var(--line);border-radius:16px;font-size:10px}.ok,.amount-pos{color:var(--ok)}.warn,.err-box{color:var(--danger)}
  .cost-block{display:grid;gap:5px;margin:18px 0 22px;padding:16px;border:1px solid var(--line);border-radius:var(--radius)}.eyebrow,.cost-note,.count,.empty-hint{font-size:10px;color:var(--muted)}.cost-value{font-size:30px;font-weight:750}.mono{font-family:var(--mono);font-variant-numeric:tabular-nums}
  .rows{display:grid;gap:4px;margin-top:10px}.row-item{display:flex;align-items:center;gap:6px;padding:6px 0;min-width:0}.amount{font-size:11px;white-space:nowrap}.icon-btn{border:0;background:none;color:var(--muted);cursor:pointer}.err-box{font-size:12px;overflow-wrap:anywhere}.empty-hint{padding-top:18px}
</style>
