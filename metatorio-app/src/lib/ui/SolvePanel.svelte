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
    if(located) return located.flows.filter(f=>Math.abs(f.amount ?? NaN)/Math.max(f.scale??1,1e-12)>1e-9).map(f=>({flow:f.flow.flow,location:f.flow.location,amount:f.amount ?? NaN}));
    return (status?.flows ?? []).filter(f=>Math.abs(f.amount ?? NaN)/Math.max(f.scale??1,1e-12)>1e-9).map(f=>({flow:f.flow,location:undefined as Location|undefined,amount:f.amount ?? NaN}));
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
          <strong class="mono amount" class:amount-pos={row.amount>0} class:amount-neg={row.amount<0}>{row.amount<0?'-':row.amount>0?'+':''}{networkRate(row.flow,row.amount,timeScale)}</strong>
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
  .panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }
  .solve-head, .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .solve-head {
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line);
  }
  .solve-head .title {
    margin-bottom: 0;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .section-head {
    margin-top: 4px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.04em;
  }
  .cost-block {
    display: grid;
    gap: 4px;
    margin: 14px 0 16px;
    padding: 12px 14px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius);
    box-shadow: var(--shadow-sm, var(--shadow));
  }
  .cost-value {
    font-size: 28px;
    font-weight: 750;
    line-height: 1.15;
    letter-spacing: -0.02em;
  }
  .cost-note, .empty-hint {
    font-size: 10.5px;
    color: var(--faint);
    line-height: 1.4;
  }
  .count {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 10px;
    color: var(--faint);
  }
  .mono {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .rows {
    display: grid;
    gap: 2px;
    margin-top: 8px;
  }
  .row-item {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 4px 0;
  }
  .result-row {
    padding: 4px 6px;
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .result-row:hover {
    background: var(--card-hover);
  }
  .amount {
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }
  .amount-pos { color: var(--ok); }
  .amount-neg { color: var(--text); }
  .result-row .icon-btn {
    width: 24px;
    height: 24px;
    color: var(--muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .result-row .icon-btn:hover {
    color: var(--text);
    background: var(--panel-elev);
    border-color: var(--line-strong);
  }
  .err-box {
    margin-top: 12px;
    padding: 10px 12px;
    color: var(--danger);
    background: var(--danger-dim);
    border: 1px solid var(--danger-line);
    border-left: 3px solid var(--danger);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  .err-box > strong {
    display: block;
    margin-bottom: 4px;
    font-size: 12px;
    font-weight: 700;
  }
  .err-box p {
    margin: 4px 0 6px;
    color: var(--text);
  }
  .err-box h4 {
    margin: 8px 0 4px;
    color: var(--muted);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .empty-hint {
    padding: 20px 0;
    text-align: center;
  }
</style>
