<script lang="ts">
  import type { DualVar, RuntimeDiagnostic } from "$lib/runtime/types";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { presentFlow } from "$lib/dual-var";
  import { normalizeSearch } from "$lib/rich-text";
  import FlowRef from "./FlowRef.svelte";
  import LocationIcon from './LocationIcon.svelte';
  let { diagnostic, onFlowClick }: { diagnostic: RuntimeDiagnostic; onFlowClick?: (flow: DualVar,location?:import("$lib/runtime/types").Location) => void } = $props();
  let query = $state("");
  let sections = $derived([
    {label: "无供给", flows: [...diagnostic.no_provider.map(flow=>({flow,location:undefined as import("$lib/runtime/types").Location|undefined})),...(diagnostic.located_no_provider??[])]},
    {label: "无消耗", flows: [...diagnostic.no_consumer.map(flow=>({flow,location:undefined as import("$lib/runtime/types").Location|undefined})),...(diagnostic.located_no_consumer??[])]},
    {label: "关联对象", flows: diagnostic.flows.map(flow=>({flow,location:undefined as import("$lib/runtime/types").Location|undefined}))},
  ]);
  let total = $derived(sections.reduce((sum, section) => sum + section.flows.length, 0));
  let needle = $derived(normalizeSearch(query));
  function matches(row:{flow:DualVar;location?:import("$lib/runtime/types").Location}) {
    const flow=row.flow;
    return !needle || normalizeSearch(`${presentFlow(flow, (kind, name) => runtime.localizedName(kind, name)).description} ${JSON.stringify(flow)} ${JSON.stringify(row.location)}`).includes(needle);
  }
</script>

<section class="diagnostic" aria-label="诊断信息" data-diagnostic-code={diagnostic.code}>
  <div class="diagnostic-message">{diagnostic.message}</div>
  {#if total}
    <input aria-label="搜索诊断对象" placeholder={`搜索 ${total} 个关联对象的名称或 ID…`} bind:value={query} />
    {#each sections as section (section.label)}
      {#if section.flows.length}
        <div class="diagnostic-section">
          <strong>{section.label} <small>{section.flows.filter(matches).length}/{section.flows.length}</small></strong>
          <div class="diagnostic-flows">
            {#each section.flows.filter(matches) as flow, i (i)}
              <div class="located-row">{#if flow.location}<LocationIcon location={flow.location} size={22}/>{/if}<FlowRef flow={flow.flow} onClick={onFlowClick?()=>onFlowClick?.(flow.flow,flow.location):undefined} title={onFlowClick?"查看供给与消耗建议":undefined}/></div>
            {:else}<span class="muted">没有匹配的对象</span>{/each}
          </div>
        </div>
      {/if}
    {/each}
  {/if}
</section>

<style>
  .located-row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .located-row:hover {
    background: color-mix(in srgb, var(--danger-dim) 50%, var(--card-hover));
  }
  .diagnostic { min-width: 0; }
  .diagnostic-message { color: var(--text); font-size: 11.5px; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
  input {
    width: 100%;
    min-width: 0;
    min-height: 28px;
    margin-top: 8px;
    padding: 0 8px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      box-shadow var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  input::placeholder { color: var(--faint); }
  input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .diagnostic-section {
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--danger-line);
  }
  .diagnostic-section > strong {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
    color: var(--danger);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  small {
    color: var(--muted);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-weight: normal;
  }
  .diagnostic-flows { display: flex; flex-direction: column; gap: 3px; max-height: 280px; overflow: auto; }
</style>
